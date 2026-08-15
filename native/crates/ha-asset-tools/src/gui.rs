//! F3 — editor de assets standalone (egui). Programa de DESARROLLO: corre solo,
//! NUNCA junto al juego.
//!
//! Modelo (lo que pediste): la lista de la izquierda son los **slots que el
//! juego usa** (`game_slots()`): árbol, roca, agua, ítems, máquinas, jugador…
//! Elegís un slot → elegís un PNG del arte fuente (LimeZu singles) → se **copia**
//! al árbol de assets del juego y se registra en `manifest.json`. Marcás si es
//! animado (agua/personajes: tira de N frames a F fps). Guardás → el juego se
//! reskinea en el próximo build. "📄 Catálogo" vuelca la lista de slots con arte
//! a `native/assets/CATALOGO.md` (artefacto regenerable, para dárselo a la IA).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ha_assets::{
    game_slots, list_pngs, ArtAnim, ArtLayer, ArtManifest, CellRect, Emission, Pose, SlotDef,
    StripDir,
};

use crate::imglab::{ImageLab, LabAction};

/// Modo activo del editor: el flujo de slots de siempre, o el Image Lab (compositor
/// multi-PNG + editor de píxeles).
#[derive(Clone, Copy, PartialEq, Eq)]
enum EditorMode {
    Slots,
    Lab,
}

/// Direcciones que ofrece la matriz de animaciones. Un atlas LimeZu típico trae 4
/// (cardinales); los packs completos traen 8 (con diagonales); y hay acciones que
/// no miran a ningún lado (dormir, sentarse) → `Ninguna`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DirMode {
    None,
    Four,
    Eight,
}

impl DirMode {
    /// `(sufijo, flecha)` de cada dirección. El sufijo vacío = pose sin dirección
    /// (el nombre queda sólo con la acción: `sleep`, `sit`).
    fn dirs(self) -> &'static [(&'static str, &'static str)] {
        const NONE: &[(&str, &str)] = &[("", "●")];
        const FOUR: &[(&str, &str)] =
            &[("down", "↓"), ("up", "↑"), ("left", "←"), ("right", "→")];
        const EIGHT: &[(&str, &str)] = &[
            ("down", "↓"),
            ("down_left", "↙"),
            ("left", "←"),
            ("up_left", "↖"),
            ("up", "↑"),
            ("up_right", "↗"),
            ("right", "→"),
            ("down_right", "↘"),
        ];
        match self {
            DirMode::None => NONE,
            DirMode::Four => FOUR,
            DirMode::Eight => EIGHT,
        }
    }
}

/// Qué pinta el lienzo de la huella de colisión.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FpBrush {
    /// Frena el paso.
    Solid,
    /// Desnivel que se sube y se baja a pie (no frena a quien camina).
    Step,
    /// **Desde donde se puede interactuar.** No frena: es el area donde el juego
    /// muestra el boton. Se pinta aparte de la colision porque casi nunca
    /// coinciden — al muelle se le pinta la PUNTA para pescar, y a la casa el
    /// umbral de la puerta, no la pared entera.
    Zona,
    /// Deja la celda libre.
    Erase,
}

impl FpBrush {
    /// Color con el que se pinta y se muestra en la leyenda. Uno solo por
    /// material: el lienzo y los botones no pueden decir cosas distintas.
    fn color(self) -> egui::Color32 {
        match self {
            FpBrush::Solid => egui::Color32::from_rgb(210, 50, 40),
            FpBrush::Step => egui::Color32::from_rgb(235, 190, 40),
            FpBrush::Zona => egui::Color32::from_rgb(60, 150, 245),
            FpBrush::Erase => egui::Color32::from_gray(160),
        }
    }
}

/// **Cómo** se pinta. El material dice QUÉ queda en la celda; la herramienta,
/// cómo la elegís. Separarlos es lo que hace que la pluma sirva igual para la
/// colisión y para la zona de interacción, en vez de tener un solo modo
/// "celda por celda" que obliga a pintar una casa de 8×6 a razón de 192 clics.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FpTool {
    /// A mano alzada: arrastrás y pinta, con grosor.
    Libre,
    /// Rectángulo: arrastrás de esquina a esquina.
    Rect,
    /// Pluma tipo Photoshop: un clic por vértice, se cierra y se rellena.
    Pluma,
    /// Balde: rellena de una toda la región contigua que está igual.
    Balde,
}

/// Lo que se está editando en el lienzo de la huella, junto. Existe como struct
/// (y no como seis variables sueltas dentro del panel) porque el mismo lienzo se
/// dibuja en dos sitios — en línea y en la ventana grande.
struct FpEdit {
    /// Tamaño en TILES.
    w: u32,
    h: u32,
    /// Sub-celdas por tile (1 · 2 · 4).
    res: u32,
    /// Ancla: el tile que queda bajo el cursor al colocar el objeto.
    ax: u32,
    az: u32,
    /// Frena el paso.
    solid: FpSet,
    /// Se sube y se baja a pie.
    step: FpSet,
    /// Desde dónde se puede interactuar (eje aparte: una celda puede ser sólida
    /// Y zona a la vez — el frente de una heladera frena y se abre desde ahí).
    zona: FpSet,
}

type FpSet = std::collections::HashSet<(u32, u32)>;

/// Píxeles por tile: el mínimo deja ver una casa entera, el máximo deja pintar
/// sub-celda por sub-celda sin apuntar con lupa.
const FP_ZOOM_MIN: f32 = 8.0;
const FP_ZOOM_MAX: f32 = 400.0;

/// Una línea del plan de importación por lote: un PNG suelto que va a entrar como
/// UNA animación del slot. Todo es editable antes de escribir.
#[derive(Clone)]
struct ImportItem {
    path: PathBuf,
    /// Nombre de la animación deducido del archivo (`cuerpo_idle_left` → `idle_left`).
    name: String,
    /// Frames detectados por las proporciones (tira de 128×32 = 4 frames).
    frames: u32,
    /// Celda px = tamaño de UN frame.
    grid: (u32, u32),
    /// Hacia dónde avanzan los frames de la tira.
    dir: StripDir,
    take: bool,
}

/// Deduce de las dimensiones si el PNG es una **tira de frames** y de qué tamaño
/// es cada uno: 128×32 → 4 frames de 32×32 (→); 32×128 → 4 frames hacia abajo
/// (↓); 32×48 → una sola imagen. Es una heurística: la tabla del importador te
/// deja corregirla antes de escribir.
fn detect_strip(w: u32, h: u32) -> ((u32, u32), u32, StripDir) {
    if w > h && h > 0 && w % h == 0 {
        return ((h, h), w / h, StripDir::Right);
    }
    if h > w && w > 0 && h % w == 0 {
        return ((w, w), h / w, StripDir::Down);
    }
    ((w.max(1), h.max(1)), 1, StripDir::Right)
}

/// Nombre de animación a partir del nombre de archivo. Busca una acción conocida
/// y una dirección conocida en el nombre (`Body_Idle_Left_01` → `idle_left`); si
/// no reconoce nada, deja el nombre limpio tal cual para que vos lo corrijas.
fn infer_pose_name(stem: &str) -> String {
    let s = stem.to_lowercase();
    // Direcciones: primero las compuestas, si no `down_left` se leería `down`.
    let dir = DirMode::Eight
        .dirs()
        .iter()
        .map(|(d, _)| *d)
        .filter(|d| !d.is_empty())
        .filter(|d| s.contains(d))
        .max_by_key(|d| d.len());
    let action = ACTION_CATALOG
        .iter()
        .map(|(a, _, _)| *a)
        .filter(|a| s.contains(a))
        .max_by_key(|a| a.len());
    match (action, dir) {
        (Some(a), Some(d)) => format!("{a}_{d}"),
        (Some(a), None) => a.to_string(),
        (None, Some(d)) => format!("idle_{d}"),
        (None, None) => sanitize_pose_name(&s),
    }
}

/// Limpia un nombre de pose para el manifest (minúsculas, `_`).
fn sanitize_pose_name(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

/// Nombre en castellano de una dirección, para la cabecera de la matriz. El
/// sufijo del manifest sigue siendo el inglés (`down`, `up_left`) — lo que
/// cambia es lo que LEÉS.
fn dir_label(d: &str) -> &'static str {
    match d {
        "down" => "abajo",
        "up" => "arriba",
        "left" => "izq.",
        "right" => "der.",
        "down_left" => "abajo-izq.",
        "down_right" => "abajo-der.",
        "up_left" => "arriba-izq.",
        "up_right" => "arriba-der.",
        _ => "sin dir.",
    }
}

/// Nombre canónico de una pose: `accion_direccion`, o sólo `accion` si no tiene
/// dirección. Es EXACTAMENTE lo que busca el juego (`sim::face_player_sprite`,
/// `fauna::apply_pose`).
fn pose_name_of(action: &str, dir: &str) -> String {
    if dir.is_empty() {
        action.to_string()
    } else {
        format!("{action}_{dir}")
    }
}

/// Acción pedida en la barra de variantes. Se aplica DESPUÉS de dibujar la barra
/// (dentro del closure de egui no se puede tocar `self.manifest` mutable).
enum VariantOp {
    Activate(Option<String>),
    Add { id: String, label: String },
    Move(String, i32),
    Remove(String),
}

/// Orden en que un atlas dispone sus filas — lo que decide qué nombre le toca a
/// cada fila en el auto-slice.
#[derive(Clone, Copy, PartialEq, Eq)]
enum BulkOrder {
    /// idle_down, idle_up, idle_left, idle_right, walk_down… (lo más común).
    ByAction,
    /// idle_down, walk_down, run_down, …, idle_up, walk_up…
    ByDirection,
}

/// Catálogo de acciones: `(nombre canónico, etiqueta, familia)`. El nombre es el
/// que va al manifest y el que lee el juego; la etiqueta es sólo para vos. No es
/// una lista cerrada — abajo de la matriz podés agregar la que quieras.
const ACTION_CATALOG: &[(&str, &str, &str)] = &[
    // Movimiento
    ("idle", "quieto", "movimiento"),
    ("walk", "caminar", "movimiento"),
    ("run", "correr", "movimiento"),
    ("sprint", "esprintar", "movimiento"),
    ("crouch", "agacharse", "movimiento"),
    ("jump", "saltar", "movimiento"),
    ("fall", "caer", "movimiento"),
    ("climb", "trepar", "movimiento"),
    ("swim", "nadar", "movimiento"),
    ("dash", "esquivar", "movimiento"),
    // Combate
    ("attack", "atacar", "combate"),
    ("shoot", "disparar", "combate"),
    ("aim", "apuntar", "combate"),
    ("reload", "recargar", "combate"),
    ("block", "bloquear", "combate"),
    ("hurt", "recibir golpe", "combate"),
    ("die", "morir", "combate"),
    // Trabajo
    ("dig", "cavar", "trabajo"),
    ("chop", "talar", "trabajo"),
    ("mine", "picar", "trabajo"),
    ("build", "construir", "trabajo"),
    ("water", "regar", "trabajo"),
    ("harvest", "cosechar", "trabajo"),
    ("fish", "pescar", "trabajo"),
    ("carry", "cargar objeto", "trabajo"),
    ("push", "empujar", "trabajo"),
    // Vida / social
    ("sit", "sentarse", "vida"),
    ("sleep", "dormir", "vida"),
    ("phone", "usar celular", "vida"),
    ("read", "leer", "vida"),
    ("eat", "comer", "vida"),
    ("drink", "beber", "vida"),
    ("wave", "saludar", "vida"),
    ("talk", "hablar", "vida"),
    ("dance", "bailar", "vida"),
    // Fauna / vehículos
    ("bark", "ladrar", "fauna"),
    ("graze", "pastar", "fauna"),
    ("fly", "volar", "fauna"),
    ("drive", "conducir", "vehiculo"),
    ("ride", "montar", "vehiculo"),
];

pub fn run_gui() -> ExitCode {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };
    let res = eframe::run_native(
        "ha-asset-tools — editor de assets del juego (dev)",
        options,
        Box::new(|_cc| Ok(Box::new(EditorApp::new()))),
    );
    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("no pude abrir la ventana (¿hay display?): {e}");
            ExitCode::FAILURE
        }
    }
}

pub(crate) struct EditorApp {
    slots: Vec<SlotDef>,
    manifest: ArtManifest,
    /// Carpeta con los PNG fuente (LimeZu singles ya purgados).
    art_root: String,
    /// `manifest.json` del juego (destino).
    game_manifest: String,
    /// Raíz de assets del juego; los PNG elegidos se copian bajo `art/<key>.png`.
    game_assets: String,
    /// Ruta del `CATALOGO.md`.
    catalog_path: String,

    slot_filter: String,
    only_unbound: bool,
    png_filter: String,
    selected: Option<usize>,
    pngs: Vec<PathBuf>,

    /// Claves de los slots nativos del juego (no borrables desde el CRUD).
    core_keys: std::collections::BTreeSet<String>,
    /// Orden de categorías tal como las declara el catálogo (`slots.rs`):
    /// construcción primero, interfaz al final. Manda en el árbol de la izquierda.
    category_rank: Vec<&'static str>,
    /// Buffer del CRUD "añadir slot".
    new_slot_key: String,

    /// **Variantes** — variante activa por objeto (`"floor/piso"` → `"madera"`).
    /// Mientras hay una activa, TODO lo que editás abajo (PNG, recorte, poses,
    /// colisión, luz…) se guarda en la entrada de esa variante, no en la base.
    /// Pegajosa por objeto: volvés al slot y seguís donde estabas.
    active_variant: std::collections::HashMap<String, String>,
    /// Buffer "＋ nueva variante" (nombre lindo; el id sale de sanitizarlo).
    new_variant: String,
    /// Se acaba de cambiar de variante en la barra → hay que re-sincronizar el
    /// preview y los buffers de animación con la entrada de la variante nueva.
    variant_switched: bool,
    /// Cache de claves de variante filtradas a `&'static str` (una por variante
    /// distinta, no una por frame) — así `SlotDef` sigue siendo `Copy` y todo el
    /// editor de abajo funciona sin tocar una línea.
    variant_keys: std::collections::HashMap<String, &'static str>,

    // buffers de animación del slot seleccionado
    anim_on: bool,
    anim_frames: u32,
    anim_fps: f32,
    /// Dirección en que se disponen los frames contiguos de la tira (→ / ↓).
    strip_dir: StripDir,

    // caches de textura
    tex: Option<(String, egui::TextureHandle)>,       // binding actual (como se ve en el juego)
    selected_png: Option<PathBuf>,                    // PNG fuente elegido (candidato)
    src_tex: Option<(String, egui::TextureHandle)>,   // preview del candidato

    // slicer (canvas de hoja)
    sheet_grid: u32,                  // celda px de la hoja candidata (default 32)
    sheet_zoom: f32,
    preview_px: f32,                  // tamaño (lado mayor) del preview de la tira animada
    sheet_cols: u32,                  // columnas de la hoja candidata (calc. en sheet_canvas)
    sheet_rows: u32,                  // filas de la hoja candidata
    png_scroll_to: Option<usize>,     // fila a forzar visible tras navegar la lista con ↑/↓
    confirm_full_png: bool,           // 2do-click de confirmación para "Asignar PNG entero" si hay slicing
    sel_rect: Option<CellRect>,       // selección actual en celdas
    drag_start_cell: Option<(u32, u32)>,
    pose_name: String,                // nombre para "guardar como pose"
    pose_layer: ArtLayer,             // capa de la pose (suelo/pared/objeto/techo)
    dir_mode: DirMode,                // direcciones que ofrece la matriz de animaciones
    action_filter: String,            // filtro del catálogo de acciones
    new_action: String,               // buffer "＋ acción nueva"
    custom_actions: Vec<String>,      // acciones que el usuario agregó a mano
    bulk_order: BulkOrder,            // orden de las filas del atlas para el auto-slice
    bulk_rows: u32,                   // cuántas filas consecutivas cubre el auto-slice
    /// Texturas por RUTA relativa (`art/...png`): las poses pueden venir cada una
    /// de su propia hoja, así que el thumbnail no puede salir de una sola textura.
    tex_cache: std::collections::HashMap<String, egui::TextureHandle>,
    /// Plan del importador por lote (varios PNG → varias animaciones de un slot).
    import_plan: Vec<ImportItem>,
    /// Pincel activo del lienzo de la huella. Existe como BOTONES a la vista
    /// porque "Ctrl+arrastre para borrar" no lo descubre nadie.
    fp_brush: FpBrush,
    /// Herramienta activa del lienzo (libre · rectángulo · pluma · balde).
    fp_tool: FpTool,
    /// Píxeles por TILE del lienzo de la huella. Ya no está atado al ancho del
    /// panel: el lienzo es un visor con zoom y desplazamiento propios, así que
    /// esto llega hasta [`FP_ZOOM_MAX`] y el sprite se ve como en un editor.
    fp_zoom: f32,
    /// Desplazamiento del lienzo dentro del visor (arrastre con el botón del
    /// medio o con Shift).
    fp_pan: egui::Vec2,
    /// Pedido de reencuadre: el asset entero, centrado. Se activa al abrir otro
    /// slot y con el botón «ajustar».
    fp_fit: bool,
    /// Slot que está mostrando el lienzo, para reencuadrar al cambiar.
    fp_key: String,
    /// Grosor del pincel libre, en sub-celdas.
    fp_size: i32,
    /// Vértices de la pluma en coordenadas de sub-celda (sobreviven al zoom).
    fp_pen: Vec<(f32, f32)>,
    /// Esquina inicial del rectángulo en curso.
    fp_rect0: Option<(u32, u32)>,
    /// Última sub-celda pintada del trazo libre: se interpola hasta la actual
    /// para que arrastrar rápido no deje agujeros.
    fp_last: Option<(i32, i32)>,
    /// Hay cambios de huella sin escribir al disco. El guardado espera a que
    /// sueltes el botón: antes escribía el manifest ENTERO en cada frame del
    /// arrastre, y de ahí venía buena parte de lo pegajoso que se sentía.
    fp_dirty: bool,
    /// Alto del visor en línea (el de la ventana grande manda el suyo).
    fp_view_h: f32,
    /// El lienzo abierto en una ventana grande y redimensionable.
    fp_big: bool,
    /// Tamaño en TILES de los PNG sueltos (ruta → celdas), para no leer la
    /// cabecera del archivo en cada frame.
    png_cells: std::collections::HashMap<String, (u32, u32)>,
    /// Frames que quedan de "traé la huella a la vista" (gancho de QA `HA_SLOT`).
    fp_scroll_qa: u32,
    tile_inner_corner: bool,          // esquina cóncava (interior) vs convexa
    anim_preview_frame: usize,        // frame en curso del preview del CANDIDATO
    anim_preview_t: f32,              // acumulador de tiempo del preview del candidato
    bound_preview_frame: usize,       // frame en curso del preview del binding ACTUAL
    bound_preview_t: f32,             // acumulador de tiempo del preview actual

    // ── ficha del asset (lo que se ve al clickearlo en la lista) ────────────
    /// Dibujar la huella/zona sobre el sprite de la ficha. Encendido: la ficha
    /// tiene que contestar "¿con qué me choco?" sin abrir nada.
    ficha_colision: bool,
    /// Dibujar la grilla de tiles sobre el sprite de la ficha (1 tile = 1 m).
    ficha_grilla: bool,

    status: String,

    /// Modo activo (Slots / Image Lab) y estado del Lab.
    mode: EditorMode,
    lab: ImageLab,
}

impl EditorApp {
    pub(crate) fn new() -> Self {
        let slots = game_slots();
        let core_keys = slots.iter().map(|s| s.key.to_string()).collect();
        let mut category_rank: Vec<&'static str> = Vec::new();
        for s in &slots {
            if !category_rank.contains(&s.category) {
                category_rank.push(s.category);
            }
        }
        // Rutas ancladas a `CARGO_MANIFEST_DIR` (constante de COMPILACIÓN) en vez
        // de relativas al cwd: si el binario se lanza desde un directorio distinto
        // al esperado (doble-click, otra terminal, cargo run desde la raíz del
        // repo…), las relativas escribían "con éxito" en un lugar que el juego
        // nunca lee — guardabas y no pasaba nada, ni en el editor ni en el juego.
        // Estas son absolutas y correctas siempre.
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")); // .../ha-asset-tools
        let native_root = manifest_dir.parent().and_then(Path::parent).unwrap_or(manifest_dir); // .../native
        let game_manifest = native_root.join("crates/ha-game/assets/art/manifest.json");
        let game_assets = native_root.join("crates/ha-game/assets");
        let catalog_path = native_root.join("assets/CATALOGO.md");
        let repo_root = native_root.parent().unwrap_or(native_root);
        let art_root = repo_root.join("PRUEBAS-CODIGOS");
        let mut app = EditorApp {
            slots,
            manifest: ArtManifest::default(),
            art_root: art_root.display().to_string(),
            game_manifest: game_manifest.display().to_string(),
            game_assets: game_assets.display().to_string(),
            catalog_path: catalog_path.display().to_string(),
            slot_filter: String::new(),
            only_unbound: false,
            png_filter: String::new(),
            selected: None,
            pngs: Vec::new(),
            core_keys,
            category_rank,
            new_slot_key: String::new(),
            active_variant: std::collections::HashMap::new(),
            new_variant: String::new(),
            variant_switched: false,
            variant_keys: std::collections::HashMap::new(),
            anim_on: false,
            anim_frames: 4,
            anim_fps: 6.0,
            strip_dir: StripDir::Right,
            tex: None,
            selected_png: None,
            src_tex: None,
            sheet_grid: 32,
            sheet_zoom: 3.0,
            preview_px: 160.0,
            sheet_cols: 0,
            sheet_rows: 0,
            png_scroll_to: None,
            confirm_full_png: false,
            sel_rect: None,
            drag_start_cell: None,
            pose_name: String::new(),
            pose_layer: ArtLayer::Object,
            dir_mode: DirMode::Four,
            action_filter: String::new(),
            new_action: String::new(),
            custom_actions: Vec::new(),
            bulk_order: BulkOrder::ByAction,
            bulk_rows: 8,
            tex_cache: std::collections::HashMap::new(),
            import_plan: Vec::new(),
            fp_brush: FpBrush::Solid,
            fp_tool: FpTool::Libre,
            fp_zoom: 72.0,
            fp_pan: egui::Vec2::ZERO,
            fp_fit: true,
            fp_key: String::new(),
            fp_size: 1,
            fp_pen: Vec::new(),
            fp_rect0: None,
            fp_last: None,
            fp_dirty: false,
            fp_view_h: 420.0,
            fp_big: false,
            png_cells: std::collections::HashMap::new(),
            fp_scroll_qa: 0,
            tile_inner_corner: false,
            anim_preview_frame: 0,
            anim_preview_t: 0.0,
            bound_preview_frame: 0,
            bound_preview_t: 0.0,
            ficha_colision: true,
            ficha_grilla: true,
            status: String::new(),
            mode: EditorMode::Slots,
            lab: ImageLab::new(),
        };
        app.load_manifest(None);
        app
    }

    /// Asegura que exista un slot con esa clave (para claves del manifest que no
    /// son nativas, y para el CRUD "añadir slot"). Categoría = prefijo de la clave.
    fn ensure_slot(&mut self, key: &str) {
        // Las claves de VARIANTE (`objeto#variante`) no son slots de la lista: viven
        // dentro de su objeto, en la barra de variantes.
        if key.is_empty()
            || key.contains(ha_assets::VARIANT_SEP)
            || self.slots.iter().any(|s| s.key == key)
        {
            return;
        }
        let category: &'static str = leak(key.split('/').next().unwrap_or("custom").to_string());
        // Un slot añadido a mano no tiene nombre curado: usamos la hoja de la clave
        // como nombre provisorio (mejor eso que vacío) — al agregarlo al catálogo
        // de `slots.rs` recibe el nombre humano definitivo.
        let leaf = key.split_once('/').map(|(_, n)| n).unwrap_or(key);
        let label: &'static str = leak(leaf.replace('_', " "));
        self.slots.push(SlotDef { key: leak(key.to_string()), label, category, animated: false });
        // Ordena por el ORDEN DE CATEGORÍA del catálogo (construcción primero…), no
        // alfabético: si no, un slot añadido a mano reordenaba toda la lista y la
        // construcción se iba al fondo.
        let rank = self.category_rank.clone();
        let pos = move |c: &'static str| rank.iter().position(|r| *r == c).unwrap_or(usize::MAX);
        self.slots.sort_by(|a, b| {
            pos(a.category).cmp(&pos(b.category)).then(a.category.cmp(b.category)).then(a.key.cmp(b.key))
        });
    }

    /// ¿Este objeto ya tiene arte? Cuenta el arte genérico **o** el de cualquiera
    /// de sus variantes: un "piso" que sólo tiene madera y piedra está resuelto,
    /// aunque no tenga PNG genérico.
    fn slot_has_art(&self, key: &str) -> bool {
        self.manifest.get(key).is_some() || !self.manifest.variants_with_art(key).is_empty()
    }

    /// El Studio manda al editor a su modo Image Lab (pestaña "Imagen") y de
    /// vuelta a la lista de objetos, y necesita saber en cuál está para pintar la
    /// pestaña activa correcta.
    pub(crate) fn focus_lab(&mut self) {
        self.mode = EditorMode::Lab;
    }

    pub(crate) fn focus_slots(&mut self) {
        self.mode = EditorMode::Slots;
    }

    pub(crate) fn in_lab(&self) -> bool {
        self.mode == EditorMode::Lab
    }

    /// Cantidad de slots sin PNG asignado (arte procedural en el juego).
    fn unbound_count(&self) -> usize {
        self.slots.iter().filter(|s| !self.slot_has_art(s.key)).count()
    }

    pub(crate) fn ui(&mut self, ctx: &egui::Context) {
        self.top_bar(ctx);
        match self.mode {
            EditorMode::Slots => {
                self.left_slots(ctx);
                self.right_pngs(ctx);
                self.central_detail(ctx);
            }
            EditorMode::Lab => self.lab_panel(ctx),
        }
    }

    /// Panel del Image Lab (compositor multi-PNG + editor de píxeles). Al pedir
    /// "usar en slot", el PNG guardado queda como candidato del slot seleccionado
    /// y volvemos al modo Slots para recortarlo/animarlo con el slicer de siempre.
    fn lab_panel(&mut self, ctx: &egui::Context) {
        let game_assets = self.game_assets.clone();
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(action) = self.lab.ui(ctx, ui, &game_assets) {
                match action {
                    LabAction::UseInSlot(path) => {
                        self.src_tex = load_texture(ctx, &path, "src_preview");
                        self.selected_png = Some(path);
                        self.mode = EditorMode::Slots;
                        self.status = if self.selected.is_some() {
                            "imagen del Lab lista como candidato — recortala/animala y Guardá".into()
                        } else {
                            "imagen del Lab lista — elegí un slot a la izquierda para asignarla".into()
                        };
                    }
                    // 💡 La luz que pintaste en el Lab va al slot seleccionado.
                    // Necesita que el slot ya tenga PNG (la emisión vive en su
                    // entrada del manifest).
                    LabAction::SetEmission(em) => {
                        self.status = match self.current_key() {
                            Some(key) if self.manifest.get(key).is_some() => {
                                self.manifest.set_emission(key, Some(em));
                                let saved = self.persist_manifest();
                                self.mode = EditorMode::Slots;
                                format!(
                                    "luz aplicada a {} (radio {:.1} m) {}",
                                    key,
                                    em.radius,
                                    if saved { "· guardado" } else { "· ⚠ no guardó" }
                                )
                            }
                            Some(key) => format!(
                                "{key} todavía no tiene PNG — asignáselo primero y volvé a aplicar la luz"
                            ),
                            None => "elegí un slot a la izquierda antes de aplicarle la luz".into(),
                        };
                    }
                }
            }
        });
    }

    // ── paneles ───────────────────────────────────────────────────────────

    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("bar").show(ctx, |ui| {
            // Toggle de modo: Slots (flujo de siempre) ↔ Image Lab (compositor + editor).
            ui.horizontal(|ui| {
                if ui.selectable_label(self.mode == EditorMode::Slots, "🧩 Slots").clicked() {
                    self.mode = EditorMode::Slots;
                }
                if ui.selectable_label(self.mode == EditorMode::Lab, "🖌 Image Lab").clicked() {
                    self.mode = EditorMode::Lab;
                }
                ui.separator();
                // Atajo: editar el candidato actual en el Lab (mezclar con otros PNG,
                // retocar píxeles) y volver.
                if let Some(png) = self.selected_png.clone() {
                    if ui.button("✎ Editar candidato en Lab").clicked() {
                        self.lab.open_file_as_canvas(&png);
                        self.mode = EditorMode::Lab;
                    }
                }
            });
            ui.separator();
            if self.mode == EditorMode::Lab {
                ui.label(
                    egui::RichText::new(
                        "Image Lab: importá 1 o varios PNG (se acomodan en capas), editá, y \
                         'Guardar y usar en slot' → volvés a Slots con la hoja combinada como candidato.",
                    )
                    .weak()
                    .small(),
                );
                return;
            }
            ui.horizontal_wrapped(|ui| {
                ui.label("Arte fuente:");
                ui.add(egui::TextEdit::singleline(&mut self.art_root).desired_width(240.0));
                if ui.button("📁 Elegir carpeta…").clicked() {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        self.art_root = dir.display().to_string();
                        self.load_pngs();
                    }
                }
                if ui.button("📂 Cargar PNGs").clicked() {
                    self.load_pngs();
                }
                ui.separator();
                if ui.button("💾 Guardar (actualiza el juego)").clicked() {
                    self.save_manifest(ctx);
                }
                if ui.button("📄 Catálogo").clicked() {
                    self.write_catalog();
                }
                if ui.button("↻ Recargar manifest").clicked() {
                    self.load_manifest(Some(ctx));
                }
            });
            ui.label(egui::RichText::new(format!("manifest del juego: {}", self.game_manifest)).weak().small());
            // CRUD: añadir un slot nuevo (clave lógica, ej. ui/mi_boton).
            ui.horizontal(|ui| {
                ui.label("＋ nuevo slot:");
                ui.add(egui::TextEdit::singleline(&mut self.new_slot_key).desired_width(220.0)
                    .hint_text("categoria/nombre (ej. ui/boton_craft)"));
                if ui.button("Añadir").clicked() {
                    let key = self.new_slot_key.trim().to_string();
                    if key.contains('/') {
                        self.ensure_slot(&key);
                        self.status = format!("slot añadido: {key}");
                        self.new_slot_key.clear();
                    } else {
                        self.status = "la clave debe ser categoria/nombre".into();
                    }
                }
            });
            // Los assets de variante no son slots: se cuentan aparte para que el
            // total de "slots con PNG" siga significando lo mismo que siempre.
            let variant_assets = self
                .manifest
                .sprites
                .keys()
                .filter(|k| k.contains(ha_assets::VARIANT_SEP))
                .count();
            let bound = self.manifest.sprites.len() - variant_assets;
            ui.label(format!(
                "{} objetos · {} con PNG · {} SIN PNG (procedural) · {} assets de variante · {} PNGs fuente",
                self.slots.len(),
                bound,
                self.unbound_count(),
                variant_assets,
                self.pngs.len()
            ));
            if !self.status.is_empty() {
                ui.label(egui::RichText::new(&self.status).weak());
            }
        });
    }

    fn left_slots(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("slots")
            .resizable(true)
            .default_width(320.0)
            .width_range(180.0..=620.0)
            .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("🔎 buscar");
                ui.text_edit_singleline(&mut self.slot_filter);
            });
            ui.label(
                egui::RichText::new("buscá por nombre («puerta», «pared») o por clave")
                    .weak()
                    .small(),
            );
            ui.checkbox(&mut self.only_unbound, "solo sin PNG (a asignar)");
            ui.separator();
            let filter = self.slot_filter.to_lowercase();
            let only_unbound = self.only_unbound;
            // Agrupar índices por categoría (respetando filtros) → árbol.
            let mut groups: Vec<(&'static str, Vec<usize>)> = Vec::new();
            for (i, slot) in self.slots.iter().enumerate() {
                // El filtro mira el NOMBRE primero: quien busca escribe "puerta",
                // no "door/wood". La clave sigue matcheando para quien la sabe.
                if !filter.is_empty()
                    && !slot.label.to_lowercase().contains(&filter)
                    && !slot.key.to_lowercase().contains(&filter)
                {
                    continue;
                }
                if only_unbound && self.slot_has_art(slot.key) {
                    continue;
                }
                match groups.last_mut() {
                    Some((cat, v)) if *cat == slot.category => v.push(i),
                    _ => groups.push((slot.category, vec![i])),
                }
            }
            let mut clicked: Option<usize> = None;
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (cat, idxs) in &groups {
                    let bound = idxs.iter().filter(|&&i| self.slot_has_art(self.slots[i].key)).count();
                    let header = egui::RichText::new(format!(
                        "▸ {}  ({}/{})",
                        ha_assets::category_label(cat),
                        bound,
                        idxs.len()
                    ))
                    .strong()
                    .color(egui::Color32::from_rgb(120, 210, 255));
                    egui::CollapsingHeader::new(header)
                        .default_open(true)
                        .show(ui, |ui| {
                            let n = idxs.len();
                            for (row, &i) in idxs.iter().enumerate() {
                                let slot = self.slots[i];
                                let branch = if row + 1 == n { "└─" } else { "├─" };
                                let has = self.slot_has_art(slot.key);
                                let dot = if has { "●" } else { "○" };
                                // Sufijo de variantes: `⟨2/3⟩` = 3 declaradas, 2 con
                                // asset. Así ves de un vistazo qué objetos son
                                // multi-material y cuánto les falta.
                                let nvar = self.manifest.variants(slot.key).len();
                                let tail = if nvar == 0 {
                                    String::new()
                                } else {
                                    format!(
                                        " ⟨{}/{nvar}⟩",
                                        self.manifest.variants_with_art(slot.key).len()
                                    )
                                };
                                // Se lee el NOMBRE ("Puerta"), no la jerga
                                // ("door/wood"). La clave va abajo, chiquita, para
                                // quien la necesite al leer el código.
                                let mut txt = egui::RichText::new(format!(
                                    "{branch} {dot} {}{tail}",
                                    slot.label
                                ));
                                if has {
                                    txt = txt.color(egui::Color32::from_rgb(120, 230, 140));
                                }
                                let sel = self.selected == Some(i);
                                let resp = ui.selectable_label(sel, txt);
                                ui.horizontal(|ui| {
                                    ui.add_space(18.0);
                                    ui.label(
                                        egui::RichText::new(slot.key).weak().small().monospace(),
                                    );
                                });
                                if resp.clicked() {
                                    clicked = Some(i);
                                }
                            }
                        });
                }
            });
            if let Some(i) = clicked {
                self.select_slot(ctx, i);
            }
        });
    }

    fn central_detail(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(i) = self.selected else {
                ui.centered_and_justified(|ui| {
                    ui.label("Elegí un slot del juego a la izquierda para reskinearlo.");
                });
                return;
            };
            // TODO el contenido va en un scroll: si la ventana es chica o hay
            // mucho contenido (poses, canvas), nada queda inalcanzable/desbordado.
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            self.central_detail_content(ctx, ui, i);
            });
        });
    }

    /// Clave que el usuario está editando AHORA: el objeto seleccionado, o su
    /// variante activa. Todo lo que actúa "sobre lo seleccionado" (preview, buffers
    /// de animación, la luz que llega del Image Lab) tiene que pasar por acá, si no
    /// escribiría en el objeto genérico mientras vos ves una variante.
    fn current_key(&mut self) -> Option<&'static str> {
        let i = self.selected?;
        let base = self.slots[i].key;
        match self.active_variant.get(base).cloned() {
            Some(v) => Some(self.variant_slot_key(base, &v)),
            None => Some(base),
        }
    }

    /// Clave `&'static str` de una variante (`"floor/piso#madera"`), cacheada.
    fn variant_slot_key(&mut self, base: &str, variant: &str) -> &'static str {
        let key = ha_assets::variant_key(base, variant);
        if let Some(k) = self.variant_keys.get(&key) {
            return k;
        }
        let leaked = leak(key.clone());
        self.variant_keys.insert(key, leaked);
        leaked
    }

    /// **Barra de variantes** del objeto: `[genérico] [Madera ●] [Piedra ●] [Metal ○]`
    /// + crear/renombrar/reordenar/borrar. Un objeto (piso, pared, techo…) puede
    /// tener N materiales y cada uno su asset, sin multiplicar objetos en la lista.
    /// El objeto que no necesite variantes simplemente no tiene ninguna y la barra
    /// se queda en una sola línea ("＋ variante") — no molesta.
    ///
    /// Devuelve la variante activa (`None` = estás editando el arte genérico, el
    /// que se usa como fallback cuando una variante no tiene PNG propio).
    fn variant_bar(&mut self, ui: &mut egui::Ui, base: &str) -> Option<String> {
        let declared: Vec<(String, String, bool)> = self
            .manifest
            .variants(base)
            .iter()
            .map(|v| {
                let has_art = self.manifest.get(&ha_assets::variant_key(base, &v.id)).is_some();
                (v.id.clone(), v.display().to_string(), has_art)
            })
            .collect();
        let mut active = self.active_variant.get(base).cloned();
        // Una variante que ya no existe (la borraste) no puede quedar activa.
        if active.as_ref().is_some_and(|a| !declared.iter().any(|(id, _, _)| id == a)) {
            self.active_variant.remove(base);
            active = None;
        }
        let mut pending: Option<VariantOp> = None;

        egui::Frame::group(ui.style())
            .fill(ui.visuals().faint_bg_color)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("VARIANTES").small().strong())
                        .on_hover_text(
                            "Un mismo objeto con varios materiales/skins: piso de madera, de \
                             piedra… Cada variante tiene SU asset; el juego pide \
                             objeto+variante y cae al genérico si esa variante no tiene arte.",
                        );
                    if !declared.is_empty()
                        && ui
                            .selectable_label(active.is_none(), "◻ genérico")
                            .on_hover_text("el arte base: fallback de las variantes sin PNG propio")
                            .clicked()
                    {
                        pending = Some(VariantOp::Activate(None));
                    }
                    for (id, label, has_art) in &declared {
                        let dot = if *has_art { "●" } else { "○" };
                        let mut txt = egui::RichText::new(format!("{dot} {label}"));
                        if *has_art {
                            txt = txt.color(egui::Color32::from_rgb(120, 230, 140));
                        }
                        let sel = active.as_deref() == Some(id.as_str());
                        if ui
                            .selectable_label(sel, txt)
                            .on_hover_text(format!(
                                "{}\n{}",
                                ha_assets::variant_key(base, id),
                                if *has_art { "tiene asset" } else { "sin asset todavía" }
                            ))
                            .clicked()
                        {
                            pending = Some(VariantOp::Activate(Some(id.clone())));
                        }
                    }
                });
                // Crear una variante nueva. El id sale de sanitizar el nombre
                // (Madera Vieja → madera_vieja) — vos escribís en humano.
                ui.horizontal(|ui| {
                    ui.label("＋ variante:");
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.new_variant)
                            .desired_width(160.0)
                            .hint_text("madera, piedra, metal…"),
                    );
                    let go = ui.button("Añadir").clicked()
                        || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                    if go {
                        let label = self.new_variant.trim().to_string();
                        let id = sanitize_pose_name(&label);
                        if !id.is_empty() {
                            pending = Some(VariantOp::Add { id, label });
                        }
                    }
                    // Acciones sobre la variante activa.
                    if let Some(a) = active.clone() {
                        ui.separator();
                        if ui.small_button("◀").on_hover_text("mover antes").clicked() {
                            pending = Some(VariantOp::Move(a.clone(), -1));
                        }
                        if ui.small_button("▶").on_hover_text("mover después").clicked() {
                            pending = Some(VariantOp::Move(a.clone(), 1));
                        }
                        if ui
                            .small_button("🗑 borrar variante")
                            .on_hover_text("borra la variante y su asset (el objeto queda)")
                            .clicked()
                        {
                            pending = Some(VariantOp::Remove(a));
                        }
                    }
                });
            });

        match pending {
            Some(VariantOp::Activate(None)) => {
                self.active_variant.remove(base);
                active = None;
                self.variant_switched = true;
            }
            Some(VariantOp::Activate(Some(id))) => {
                self.active_variant.insert(base.to_string(), id.clone());
                active = Some(id);
                self.variant_switched = true;
            }
            Some(VariantOp::Add { id, label }) => {
                self.manifest.declare_variant(base, &id, &label);
                self.new_variant.clear();
                self.active_variant.insert(base.to_string(), id.clone());
                active = Some(id.clone());
                self.variant_switched = true;
                let saved = self.persist_manifest();
                self.status = format!(
                    "variante «{label}» de {base} creada{} — asignale un PNG abajo",
                    if saved { " · guardado" } else { " · ⚠ no guardó" }
                );
            }
            Some(VariantOp::Move(id, dir)) => {
                self.manifest.move_variant(base, &id, dir);
                self.persist_manifest();
            }
            Some(VariantOp::Remove(id)) => {
                self.manifest.remove_variant(base, &id);
                self.active_variant.remove(base);
                active = None;
                self.variant_switched = true;
                self.tex = None;
                let saved = self.persist_manifest();
                self.status =
                    format!("variante «{id}» de {base} borrada{}", if saved { " · guardado" } else { "" });
            }
            None => {}
        }
        active
    }

    fn central_detail_content(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, i: usize) {
            // El objeto elegido en la lista y, si tiene variantes, la variante
            // activa. `slot` se REDIRIGE a la clave de la variante: de acá para
            // abajo el editor entero (PNG, recorte, tira, poses, autotile,
            // colisión, huella, luz) escribe en la variante — un solo objeto en la
            // UI, un asset por variante en el manifest.
            let mut slot = self.slots[i];
            let base_key = slot.key;
            let variant = self.variant_bar(ui, base_key);
            if let Some(v) = &variant {
                slot.key = self.variant_slot_key(base_key, v);
            }
            // Cambiar de variante = cambiar de entrada: el preview y los buffers de
            // animación se releen de la variante nueva (si no, verías el arte de la
            // anterior y "Aplicar anim" escribiría los frames de otra).
            if std::mem::take(&mut self.variant_switched) {
                self.select_slot(ctx, i);
            }
            match &variant {
                Some(v) => {
                    let label = self
                        .manifest
                        .variants(base_key)
                        .iter()
                        .find(|d| &d.id == v)
                        .map(|d| d.display().to_string())
                        .unwrap_or_else(|| v.clone());
                    ui.heading(format!("{}  ▸  {label}", self.slots[i].label));
                    ui.label(
                        egui::RichText::new(format!("editando la variante «{label}» → {}", slot.key))
                            .weak()
                            .small(),
                    );
                }
                None => {
                    ui.heading(self.slots[i].label);
                    ui.label(
                        egui::RichText::new(format!("clave del juego: {}", slot.key))
                            .weak()
                            .small()
                            .monospace(),
                    );
                }
            }

            // FICHA del asset: lo primero que ves al clickearlo en la lista —
            // qué sprite usa, de qué hoja lo recorta, y con qué colisión va a
            // llegar al juego. Ver §8 de la biblia.
            self.ficha_panel(ctx, ui, slot);
            // 🎚 CAPA: en qué banda de profundidad se dibuja el asset en el mundo.
            // Es el eje HERMANO de la colisión de abajo y hay que leerlos juntos:
            // la capa dice QUÉ TE TAPA, la huella dice QUÉ TE FRENA. Un piso no
            // te tapa aunque te frene (el agua), y un techo te tapa sin frenarte.
            if self.manifest.get(slot.key).is_some() {
                self.layer_panel(ui, slot.key);
            }
            // Colisión de caminado del asset (relieve básico): el juego bloquea el
            // paso del jugador en la celda de este decor. Default por tipo
            // (árboles bloquean, resto no); acá lo FIJÁS por asset. La colisión
            // por-celda precisa (cráteres) es un plan aparte
            // (biblia.md §8).
            if self.manifest.get(slot.key).is_some() {
                egui::CollapsingHeader::new("🚧 colisión (bloquea el paso)")
                    .default_open(false)
                    .show(ui, |ui| {
                        let cur = self.manifest.collides(slot.key);
                        let mut mode = match cur {
                            None => 0u8,
                            Some(true) => 1,
                            Some(false) => 2,
                        };
                        ui.label(egui::RichText::new(
                            "Default = comportamiento del juego (arboles bloquean, rocas/flora no).",
                        ).weak().small());
                        let before = mode;
                        ui.radio_value(&mut mode, 0u8, "usar default del juego");
                        ui.radio_value(&mut mode, 1, "bloquea SIEMPRE (solido)");
                        ui.radio_value(&mut mode, 2, "NUNCA bloquea (se pisa)");
                        if mode != before {
                            let v = match mode {
                                1 => Some(true),
                                2 => Some(false),
                                _ => None,
                            };
                            self.manifest.set_collides(slot.key, v);
                            self.status = format!(
                                "colision de {}: {} — Guardá para aplicar al juego",
                                slot.key,
                                match v { Some(true) => "solido", Some(false) => "se pisa", None => "default" }
                            );
                        }
                    });
            }
            // 💡 LUZ: este asset ilumina su entorno con un halo suave (aura que se
            // difumina, no volumétrica). El juego la lee del manifest y le cuelga
            // el glow al sprite (`ha-game::light2d`). Por defecto la luz es
            // NOCTURNA: de día no se dibuja y no cuesta nada.
            if self.manifest.get(slot.key).is_some() {
                self.light_panel(ui, slot.key);
            }
            // (A1) Huella de colisión multi-celda: se PINTA sobre el propio
            // sprite. Habilita montañas sólidas, cráteres caminables (anillo
            // sólido + centro libre), assets grandes que bloquean sólo su base y
            // escalones que se trepan. Vacío = colisión de 1 celda (arriba).
            if self.manifest.get(slot.key).is_some() {
                self.footprint_panel(ui, slot);
            }
            // Poses guardadas del slot — SIEMPRE visibles (no dependen de tener un
            // candidato elegido), con thumbnail recortado del sheet ya asignado y
            // botón para volver a cargarla en el canvas y ajustarla.
            let saved_poses: Vec<(String, Pose)> = self
                .manifest
                .poses(slot.key)
                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default();
            if !saved_poses.is_empty() {
                let slot_grid = self.manifest.get(slot.key).map(|v| v.grid()).unwrap_or((32, 32));
                let slot_png =
                    self.manifest.get(slot.key).map(|v| v.png().to_string()).unwrap_or_default();
                let mut remove: Option<String> = None;
                let mut edit: Option<(String, Pose)> = None;
                let sheets = self.manifest.sheets(slot.key).len();
                let header = if sheets > 1 {
                    format!("🎭 poses / animaciones guardadas ({}) · {sheets} hojas", saved_poses.len())
                } else {
                    format!("🎭 poses / animaciones guardadas ({})", saved_poses.len())
                };
                egui::CollapsingHeader::new(header)
                    .default_open(true)
                    .show(ui, |ui| {
                egui::Grid::new("poses_grid").striped(true).show(ui, |ui| {
                    for (pn, pose) in &saved_poses {
                        // El thumbnail sale de la hoja DE ESA POSE (puede no ser la
                        // del slot: assets armados con un PNG por dirección).
                        let grid = pose.grid_or(slot_grid);
                        let sheet = pose.png_or(&slot_png).to_string();
                        match self.tex_for(ctx, &sheet) {
                            Some(tex) => {
                                let img = tex.size_vec2();
                                let uv = egui::Rect::from_min_max(
                                    egui::pos2(
                                        (pose.rect.col * grid.0) as f32 / img.x,
                                        (pose.rect.row * grid.1) as f32 / img.y,
                                    ),
                                    egui::pos2(
                                        ((pose.rect.col + pose.rect.w) * grid.0) as f32 / img.x,
                                        ((pose.rect.row + pose.rect.h) * grid.1) as f32 / img.y,
                                    ),
                                );
                                ui.add(
                                    egui::Image::new(egui::load::SizedTexture::new(tex.id(), egui::vec2(48.0, 48.0)))
                                        .uv(uv),
                                );
                            }
                            None => {
                                ui.label("—");
                            }
                        }
                        let anim_tag = if pose.anim.is_some() { " 🎞" } else { "" };
                        let layer_tag = match pose.layer {
                            Some(ArtLayer::Floor) => " · suelo",
                            Some(ArtLayer::Basement) => " · subsuelo",
                            Some(ArtLayer::Wall) => " · pared",
                            Some(ArtLayer::Roof) => " · techo",
                            Some(ArtLayer::Light) => " · luces",
                            _ => "",
                        };
                        // Mostrar de qué HOJA sale cada pose sólo cuando el asset
                        // usa varias (si no, sería ruido en todas las filas).
                        let sheet_tag = match (&pose.png, sheets > 1) {
                            (Some(p), true) => {
                                format!(" · {}", p.rsplit('/').next().unwrap_or(p))
                            }
                            (None, true) => " · hoja principal".to_string(),
                            _ => String::new(),
                        };
                        ui.label(format!("{pn}{anim_tag}{layer_tag}{sheet_tag}"));
                        if ui.small_button("✎ editar").clicked() {
                            edit = Some((pn.clone(), pose.clone()));
                        }
                        if ui.small_button("✖").clicked() {
                            remove = Some(pn.clone());
                        }
                        ui.end_row();
                    }
                });
                }); // cierra el CollapsingHeader "poses guardadas"
                if let Some(pn) = remove {
                    self.manifest.remove_pose(slot.key, &pn);
                    self.persist_manifest();
                    self.status = format!("{}: pose '{pn}' borrada · guardado", slot.key);
                }
                if let Some((pn, pose)) = edit {
                    self.pose_name = pn;
                    self.pose_layer = pose.layer.unwrap_or_default();
                    self.sel_rect = Some(pose.rect);
                    match pose.anim {
                        Some(a) => {
                            self.anim_on = true;
                            self.anim_frames = a.frames;
                            self.anim_fps = a.fps;
                            self.strip_dir = pose.dir.unwrap_or_default();
                        }
                        None => self.anim_on = false,
                    }
                    // Traer AL LIENZO la hoja de esa pose (la suya o la del slot) y
                    // su grilla: si no, ajustarías el recorte sobre otra imagen.
                    let sheet = pose.png_or(&slot_png).to_string();
                    self.load_candidate_rel(ctx, &sheet);
                    self.sheet_grid = pose.grid_or(slot_grid).0.max(1);
                    self.status = "pose cargada en el canvas — ajustá el recorte y volvé a Guardar".into();
                }
            }

            // Un asset repartido en MUCHOS PNG (uno por dirección/estado, que es
            // como vienen la mayoría de los packs): se importan todos de una y
            // cada archivo queda como una animación del MISMO slot.
            self.import_folder_panel(ctx, ui, slot);

            if let Some(png) = self.selected_png.clone() {
                let name = png.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("candidato: {name}"));
                    // "Asignar PNG entero" usa TODA la imagen como sprite y por eso
                    // BORRA las poses/recortes del slot. Si el slot ya está sliceado,
                    // pedí confirmación (2 clicks) para no perder tu trabajo por error.
                    let sliced = self.manifest.poses(slot.key).map(|m| !m.is_empty()).unwrap_or(false);
                    let label = if sliced && self.confirm_full_png {
                        "⚠ Confirmar: borrar poses y asignar entero"
                    } else {
                        "✔ Asignar PNG entero"
                    };
                    if ui
                        .button(label)
                        .on_hover_text("usa TODA la imagen como un solo sprite (sin recortes ni poses)")
                        .clicked()
                    {
                        if sliced && !self.confirm_full_png {
                            self.confirm_full_png = true;
                            self.status = "⚠ Este slot tiene poses/animaciones. Volvé a tocar para BORRARLAS y asignar la imagen entera, o 'cancelar'.".into();
                        } else {
                            self.bind_png(ctx, slot, &png);
                            self.confirm_full_png = false;
                            self.selected_png = None;
                            self.src_tex = None;
                        }
                    }
                    if sliced && self.confirm_full_png && ui.button("cancelar").clicked() {
                        self.confirm_full_png = false;
                        self.status = "cancelado — poses intactas".into();
                    }
                });

                // ── Slicer: recortar la hoja en regiones / poses / animación ──
                egui::CollapsingHeader::new("✂ Recortar hoja (grilla + arrastre)")
                    .default_open(true)
                    .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("celda px");
                        ui.add(egui::DragValue::new(&mut self.sheet_grid).range(4..=256));
                        ui.label("zoom")
                            .on_hover_text("agranda la grilla para ver mejor dónde recortás");
                        ui.add(egui::DragValue::new(&mut self.sheet_zoom).range(0.5..=12.0).speed(0.1));
                        if self.sheet_cols > 0 {
                            ui.label(egui::RichText::new(format!("hoja: {}×{} celdas", self.sheet_cols, self.sheet_rows)).weak());
                        }
                        if let Some(sr) = self.sel_rect {
                            ui.label(format!("sel: col{} row{} · {}×{}", sr.col, sr.row, sr.w, sr.h));
                        }
                    });
                    // ── Lienzo + preview, LADO A LADO ────────────────────────
                    //
                    // El preview estaba debajo del lienzo y de los controles de
                    // frames, o sea fuera de pantalla: para verlo había que hacer
                    // scroll, y entonces ya no veías el sprite que estabas
                    // recortando. Puestos uno al lado del otro, ves a la vez de
                    // dónde sale cada frame y cómo queda la animación.
                    //
                    // Y el alto del lienzo lo pide EL SPRITE: una hoja de 4 filas
                    // ya no abre una ventana de 520 px con 400 de vacío al que
                    // había que hacerle scroll. `Resize` sigue estando por si lo
                    // querés más grande, pero arranca en la medida justa.
                    let sheet_h = (self.sheet_rows.max(1) as f32
                        * self.sheet_grid.max(1) as f32
                        * self.sheet_zoom.max(0.25))
                        + 24.0;
                    let canvas_h = sheet_h.clamp(180.0, 560.0);
                    let has_preview = self.sel_rect.is_some();
                    let canvas_w = if has_preview {
                        (ui.available_width() - self.preview_px.max(48.0) - 40.0).max(260.0)
                    } else {
                        ui.available_width()
                    };
                    ui.label(
                        egui::RichText::new(
                            "↕ el lienzo se ajusta al sprite — arrastrá la esquina ⤡ para agrandarlo",
                        )
                        .weak()
                        .small(),
                    );
                    ui.horizontal_top(|ui| {
                        egui::Resize::default()
                            .id_salt("canvas_resize")
                            .default_size([canvas_w.min(900.0), canvas_h])
                            .min_size([220.0, 160.0])
                            .show(ui, |ui| {
                                let h = ui.available_height().max(150.0);
                                self.sheet_canvas(ui, h);
                            });
                        if let Some(sr) = self.sel_rect {
                            ui.vertical(|ui| {
                                let (frames, dir) = (self.anim_frames.max(1), self.strip_dir);
                                self.anim_strip_preview(ui, sr, frames, dir);
                            });
                        }
                    });

                    // La SELECCIÓN de arriba es UN frame (su tamaño w×h en celdas).
                    // Acá elegís CUÁNTOS frames tiene la animación y en qué dirección
                    // están pegados los bloques (→ a la derecha, ↓ hacia abajo). Cada
                    // frame es un bloque contiguo del MISMO tamaño — NO una celda por
                    // frame. (Este era el bug: 9 celdas de 1 frame se tomaban como 9
                    // frames; ahora 9 celdas = 1 frame, y ponés la cantidad acá.)
                    if let Some(sr) = self.sel_rect {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(egui::RichText::new(format!("frame: {}×{} celdas", sr.w, sr.h)).strong());
                            ui.separator();
                            ui.label("frames (cantidad)")
                                .on_hover_text("cuántos bloques como este forman la animación");
                            ui.add(egui::DragValue::new(&mut self.anim_frames).range(1..=256));
                            ui.label("fps");
                            ui.add(egui::DragValue::new(&mut self.anim_fps).range(0.1..=60.0));
                            ui.separator();
                            ui.label("dirección:");
                            if ui
                                .selectable_label(self.strip_dir == StripDir::Right, "→")
                                .on_hover_text("los frames siguen a la derecha")
                                .clicked()
                            {
                                self.strip_dir = StripDir::Right;
                            }
                            if ui
                                .selectable_label(self.strip_dir == StripDir::Down, "↓")
                                .on_hover_text("los frames siguen hacia abajo")
                                .clicked()
                            {
                                self.strip_dir = StripDir::Down;
                            }
                            ui.separator();
                            // Atajo para atlas de personaje: una FILA entera = una
                            // animación. Autocompleta la cantidad = cuántos frames del
                            // tamaño actual entran en lo que queda de fila (→)/columna (↓).
                            let (label, avail, extent) = match self.strip_dir {
                                StripDir::Right => ("toda la fila →", self.sheet_cols.saturating_sub(sr.col), sr.w.max(1)),
                                StripDir::Down => ("toda la columna ↓", self.sheet_rows.saturating_sub(sr.row), sr.h.max(1)),
                            };
                            if ui
                                .button(label)
                                .on_hover_text("frames = cuántos entran en lo que queda en esa dirección")
                                .clicked()
                            {
                                self.anim_frames = (avail / extent).max(1);
                            }
                        });
                        // ── RANGO de frames: "del 1 al 6 abre, del 7 al 12 cierra" ──
                        //
                        // Una tira suele traer VARIAS animaciones seguidas en la
                        // misma fila. Se podía hacer moviendo la selección a mano
                        // hasta el frame 7 y contando seis; ahora se escribe el
                        // rango y la selección salta sola. Es el caso de la puerta:
                        // un solo PNG con la apertura y el cierre pegados.
                        {
                            let step = match self.strip_dir {
                                StripDir::Right => sr.w.max(1),
                                StripDir::Down => sr.h.max(1),
                            };
                            let base = match self.strip_dir {
                                StripDir::Right => sr.col,
                                StripDir::Down => sr.row,
                            };
                            let mut first = base / step + 1; // 1-based, como se cuenta a ojo
                            let mut last = first + self.anim_frames.max(1) - 1;
                            ui.horizontal_wrapped(|ui| {
                                ui.label(egui::RichText::new("rango de frames:").strong())
                                    .on_hover_text(
                                        "Contando bloques del tamaño de la selección desde el \
                                         principio de la fila/columna. Poné 1→6 y guardá como \
                                         'abriéndose'; después 7→12 y guardá como 'cerrándose'.",
                                    );
                                ui.label("del");
                                let a = ui.add(egui::DragValue::new(&mut first).range(1..=256));
                                ui.label("al");
                                let b = ui.add(egui::DragValue::new(&mut last).range(1..=256));
                                if a.changed() || b.changed() {
                                    let last = last.max(first);
                                    self.anim_frames = last - first + 1;
                                    let off = (first - 1) * step;
                                    self.sel_rect = Some(match self.strip_dir {
                                        StripDir::Right => CellRect { col: off, ..sr },
                                        StripDir::Down => CellRect { row: off, ..sr },
                                    });
                                }
                                ui.label(
                                    egui::RichText::new(format!(
                                        "= {} frames desde la celda {}",
                                        self.anim_frames.max(1),
                                        match self.strip_dir {
                                            StripDir::Right => sr.col,
                                            StripDir::Down => sr.row,
                                        }
                                    ))
                                    .weak()
                                    .small(),
                                );
                            });
                        }
                        // (El preview animado va ARRIBA, al lado del lienzo.)
                    }

                    let grid = (self.sheet_grid.max(1), self.sheet_grid.max(1));
                    if let Some(sr) = self.sel_rect {
                        ui.horizontal(|ui| {
                            if ui.button("Guardar recorte estático").clicked() {
                                if let Some(rel) = self.ensure_sheet_copied(slot, &png) {
                                    self.manifest.set_rect(slot.key, rel, grid, sr);
                                    self.tex = self.preview_bound(ctx, slot.key);
                                    let saved = self.persist_manifest();
                                    self.status = format!(
                                        "{}: recorte {}×{} {}",
                                        slot.key, sr.w, sr.h,
                                        if saved { "· guardado en el juego" } else { "· ⚠ no guardó" }
                                    );
                                }
                            }
                            if ui.button("Guardar como animación ÚNICA (strip)")
                                .on_hover_text(
                                    "La animación BASE del objeto: hay UNA sola por asset, así que \
                                     volver a guardar acá REEMPLAZA la anterior.\n\n\
                                     Si tu objeto tiene varios estados (puerta cerrada / \
                                     abriéndose / abierta), NO uses este botón: usá el panel de \
                                     estados o la matriz de animaciones de abajo, donde cada \
                                     estado se guarda con su nombre y conviven todos.",
                                )
                                .clicked()
                            {
                                if let Some(rel) = self.ensure_sheet_copied(slot, &png) {
                                    let frames = self.anim_frames.max(1);
                                    self.manifest.set_strip(
                                        slot.key,
                                        rel,
                                        grid,
                                        sr,
                                        frames,
                                        self.anim_fps.max(0.1),
                                        self.strip_dir,
                                    );
                                    self.tex = self.preview_bound(ctx, slot.key);
                                    let saved = self.persist_manifest();
                                    self.status = format!(
                                        "{}: animación {frames} frames de {}×{} celdas {}",
                                        slot.key, sr.w, sr.h,
                                        if saved { "· guardado en el juego" } else { "· ⚠ no guardó" }
                                    );
                                }
                            }
                        });
                        // ── ESTADOS / PARTES del objeto ──────────────────
                        // Para `door/*`, `wall/*`, `window/*`: el juego pide una
                        // pose distinta según el estado (puerta cerrada,
                        // abriéndose, abierta…) o según de qué lado del cuarto
                        // está la pared. Cada una es una animación APARTE y
                        // conviven todas: guardar una nunca pisa a las otras.
                        //
                        // Va ANTES de la matriz de personaje a propósito: para
                        // una puerta, esto es el trabajo, y estaba enterrado al
                        // fondo del panel.
                        self.states_panel(ui, slot);
                        // ── Animaciones nombradas (personaje/NPC/estados) ──
                        // Para un atlas con FILAS = animaciones (idle, walk × 4 dir,
                        // acciones). Matriz guiada abajo; cada celda es una animación
                        // aparte (una pose) — no se pisan entre sí.
                        let is_character = matches!(slot.category, "player" | "npc")
                            || self.manifest.poses(slot.key).map(|m| !m.is_empty()).unwrap_or(false);
                        egui::CollapsingHeader::new("🎬 animaciones nombradas (personaje / NPC / estados)")
                            .default_open(is_character)
                            .show(ui, |ui| {
                                self.anim_matrix(ui, slot);
                            });
                        // Variantes de tile/autotile: el slot YA es el material
                        // (tiles/water…). Cada casilla es la POSICIÓN de la pieza y su
                        // nombre es el que el juego busca para el autotile por vecinos
                        // (center / edge_* / corner_* / inner_*). El ORDEN importa. Cada
                        // variante es otra pose del mismo slot y puede ser animada.
                        // ── Partes de construcción (sistema de bordes, LDoE) ──
                        // Para `wall/*`, `door/*`, `window/*`: el juego pide una
                        // pose distinta según de qué lado del cuarto está la
                        // pared. Acá elegís CUÁL estás recortando del atlas.
                        // El piso (`floor/*`) usa el MISMO autotile por vecinos que
                        // el agua: centro, bordes y esquinas.
                        let is_tile = slot.category == "tiles" || slot.key.starts_with("floor/");
                        egui::CollapsingHeader::new("🧩 autotile del tile (centro, bordes, esquinas — agua/cascada/piso)")
                            .default_open(is_tile && self.manifest.poses(slot.key).is_none())
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new(
                                    "El juego elige la pieza según los vecinos. Marcá en el lienzo la \
                                     celda de esta pieza, elegí acá su POSICIÓN, y guardá con 💾. Si es \
                                     animada (agua/cascada), poné sus frames arriba. Mínimo útil: 'centro'. \
                                     Repetí para cada borde y esquina."
                                ).weak());
                                ui.checkbox(&mut self.tile_inner_corner, "esquina cóncava (interior) en vez de convexa");
                                // La grilla se lee como el tile en el mundo: la casilla ↖ es
                                // la esquina superior-izquierda del charco de agua, ● es el
                                // relleno. Cada casilla dice en castellano QUÉ pieza es y
                                // marca ● si ya la guardaste, así se ve de un vistazo qué
                                // falta y el nombre que el juego va a buscar.
                                let defined: std::collections::BTreeSet<String> = self
                                    .manifest
                                    .poses(slot.key)
                                    .map(|m| m.keys().cloned().collect())
                                    .unwrap_or_default();
                                egui::Grid::new("tile_shapes").spacing([4.0, 4.0]).show(ui, |ui| {
                                    for row in TILE_DIRS.chunks(3) {
                                        for &(id, arrow) in row {
                                            // Nombre = el que lee el juego (world2d::autotile_shape).
                                            let shape = tile_shape_name(id, self.tile_inner_corner);
                                            let done = defined.contains(&shape);
                                            let label = format!(
                                                "{arrow} {}{}",
                                                tile_shape_label(id, self.tile_inner_corner),
                                                if done { " ●" } else { "" }
                                            );
                                            let mut txt = egui::RichText::new(label).small();
                                            if done {
                                                txt = txt.color(egui::Color32::from_rgb(120, 230, 140));
                                            }
                                            let active = self.pose_name == shape;
                                            let hint = format!(
                                                "el juego la busca como '{shape}'\n{}",
                                                if done { "ya guardada — guardar la reemplaza" } else { "sin definir" }
                                            );
                                            if ui.selectable_label(active, txt).on_hover_text(hint).clicked() {
                                                self.pose_name = shape;
                                            }
                                        }
                                        ui.end_row();
                                    }
                                });
                                self.pose_save_row(ui, slot);
                            });

                        // ── Capa de la pose (edificios/casas): el juego instancia cada
                        // capa a su altura — Suelo bajo el jugador, Pared/Objeto
                        // Y-sorted, Techo encima y TRANSLÚCIDO cuando el jugador entra.
                        // Para un edificio: guardá una pose `base` (capa Pared) y una
                        // pose `roof` (capa Techo). Sin capa = Objeto (como siempre).
                        ui.horizontal_wrapped(|ui| {
                            ui.label("capa:").on_hover_text(
                                "qué es este recorte dentro de una estructura — el techo se \
                                 vuelve translúcido cuando el jugador está dentro",
                            );
                            for (layer, label) in [
                                (ArtLayer::Floor, "Suelo"),
                                (ArtLayer::Basement, "Subsuelo"),
                                (ArtLayer::Wall, "Pared"),
                                (ArtLayer::Object, "Objeto"),
                                (ArtLayer::Roof, "Techo"),
                                (ArtLayer::Light, "Luces"),
                            ] {
                                if ui.selectable_label(self.pose_layer == layer, label).clicked() {
                                    self.pose_layer = layer;
                                }
                            }
                        });
                        self.pose_save_row(ui, slot);
                    }
                });
            }

            ui.separator();
            // Animación.
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.anim_on, "animado (tira de frames)");
                if self.anim_on {
                    ui.label("frames");
                    ui.add(egui::DragValue::new(&mut self.anim_frames).range(1..=64));
                    ui.label("fps").on_hover_text("cuadros por segundo — velocidad de la animación");
                    ui.add(egui::DragValue::new(&mut self.anim_fps).range(0.1..=60.0))
                        .on_hover_text("cuadros por segundo — más alto = más rápido");
                }
                if self.manifest.get(slot.key).is_some() && ui.button("Aplicar anim").clicked() {
                    self.apply_anim(slot.key);
                }
                if self.manifest.get(slot.key).is_some() && ui.button("🗑 Quitar PNG").clicked() {
                    self.manifest.clear(slot.key);
                    self.tex = None;
                    self.persist_manifest();
                    self.status = format!("{} → procedural · guardado", slot.key);
                }
                // Sólo el objeto se elimina acá; una variante se borra con su
                // botón en la barra de variantes (evita borrar el objeto entero
                // creyendo que borrás el material).
                if variant.is_none()
                    && !self.core_keys.contains(slot.key)
                    && ui.button("❌ Eliminar slot").clicked()
                {
                    self.manifest.clear(slot.key);
                    self.slots.retain(|x| x.key != slot.key);
                    self.selected = None;
                    self.tex = None;
                    self.persist_manifest();
                    self.status = format!("slot {} eliminado · guardado", slot.key);
                }
            });
    }

    /// 💡 Panel de LUZ del slot: acá decidís si el asset ilumina y cómo se ve su
    /// aura. Escribe `emission` en el manifest; el juego (`light2d`) le cuelga un
    /// halo difuminado al sprite. Es lo mismo que calcula el pincel de LUZ del
    /// Image Lab — este panel te deja retocarlo a mano después.
    /// **Capa de dibujo del slot**: en qué banda de profundidad se planta el
    /// asset en el mundo. Es lo que decide si el asset te TAPA o no, y es
    /// independiente de la colisión (que decide si te FRENA).
    ///
    /// Sin esto, todo lo que se colocaba desde el editor de mundo se ordenaba por
    /// su fila del pie y terminaba tapando al jugador: un charco pintado ocluía
    /// igual que un roble. Ver `ha-game::layers` para la escalera completa.
    fn layer_panel(&mut self, ui: &mut egui::Ui, key: &str) {
        let current = self.manifest.slot_layer(key);
        let header = match current {
            Some(ArtLayer::Floor) => "🎚 capa — SUELO (bajo el jugador)",
            Some(ArtLayer::Basement) => "🎚 capa — SUBSUELO",
            Some(ArtLayer::Wall) => "🎚 capa — PARED",
            Some(ArtLayer::Roof) => "🎚 capa — TECHO (se transparenta)",
            Some(ArtLayer::Light) => "🎚 capa — LUZ",
            Some(ArtLayer::Object) => "🎚 capa — OBJETO",
            None => "🎚 capa (qué tapa a qué)",
        };
        egui::CollapsingHeader::new(header).default_open(current.is_some()).show(ui, |ui| {
            ui.label(
                egui::RichText::new(
                    "La CAPA decide qué te TAPA; la huella de abajo decide qué te FRENA. \
                     Son ejes distintos a propósito: el agua te frena y no te tapa; un techo \
                     te tapa y no te frena. Si algo que dibujás en el editor de mundo se \
                     dibuja ENCIMA del jugador y no debería, marcalo Suelo.",
                )
                .weak()
                .small(),
            );
            // 0 = sin etiquetar (compat: se comporta como Objeto).
            let mut sel = match current {
                None => 0u8,
                Some(ArtLayer::Floor) => 1,
                Some(ArtLayer::Basement) => 2,
                Some(ArtLayer::Object) => 3,
                Some(ArtLayer::Wall) => 4,
                Some(ArtLayer::Roof) => 5,
                Some(ArtLayer::Light) => 6,
            };
            let before = sel;
            ui.radio_value(&mut sel, 0u8, "sin etiquetar (= Objeto)")
                .on_hover_text("Compatibilidad: Y-sort de siempre.");
            ui.radio_value(&mut sel, 1, "Suelo — SIEMPRE bajo el jugador")
                .on_hover_text("Agua, pisos, caminos, alfombras, plataformas, decals.");
            ui.radio_value(&mut sel, 2, "Subsuelo — bajo el piso actual")
                .on_hover_text("Sótanos asomando por un hueco.");
            ui.radio_value(&mut sel, 3, "Objeto — Y-sort (te tapa desde el sur)")
                .on_hover_text("Muebles, props, árboles, rocas.");
            ui.radio_value(&mut sel, 4, "Pared — Y-sort por su fila del pie")
                .on_hover_text(
                    "Pintá sólida SÓLO la fila de la base en la huella: caminás por detrás \
                     de la parte alta (que te tapa) sin atravesar la base.",
                );
            ui.radio_value(&mut sel, 5, "Techo — encima de todo, se transparenta")
                .on_hover_text(
                    "Se desvanece cuando el jugador camina bajo su huella. Un edificio de \
                     varios pisos = varios assets: los de abajo Pared/Objeto, los de arriba Techo.",
                );
            ui.radio_value(&mut sel, 6, "Luz — glow, no tapa nada")
                .on_hover_text("Ventanas encendidas, halos.");
            if sel != before {
                let v = match sel {
                    1 => Some(ArtLayer::Floor),
                    2 => Some(ArtLayer::Basement),
                    3 => Some(ArtLayer::Object),
                    4 => Some(ArtLayer::Wall),
                    5 => Some(ArtLayer::Roof),
                    6 => Some(ArtLayer::Light),
                    _ => None,
                };
                self.manifest.set_slot_layer(key, v);
                self.status =
                    format!("capa de {key} actualizada — Guardá para aplicar al juego");
            }
        });
    }

    /// 🎬 **Los ESTADOS del objeto**: puerta cerrada / abriéndose / abierta /
    /// cerrándose / bloqueada, o las caras de una pared.
    ///
    /// Cada estado es una animación aparte con su nombre, y **conviven todas**:
    /// guardar "abriéndose" no toca a "cerrada". Ése era el bug —guardar la
    /// segunda animación de un objeto pisaba la primera— y tenía dos causas: el
    /// botón "Guardar como animación (strip)" escribe la ÚNICA animación base
    /// del asset (por diseño, hay una sola), y además cambiar de hoja reseteaba
    /// la entrada entera (arreglado en `ArtManifest::full_mut`).
    ///
    /// La tabla muestra qué estados ya guardaste, con miniatura y cantidad de
    /// frames, para que se vea de un vistazo qué falta.
    fn states_panel(&mut self, ui: &mut egui::Ui, slot: SlotDef) {
        let states = build_part_poses(slot.key);
        if states.is_empty() {
            return;
        }
        let is_door = slot.key.starts_with("door/");
        let defined: std::collections::BTreeMap<String, Pose> = self
            .manifest
            .poses(slot.key)
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();
        let done = states.iter().filter(|(p, _)| defined.contains_key(*p)).count();
        let header = if is_door {
            format!("🚪 estados de la puerta ({done}/{})", states.len())
        } else {
            format!("🧱 partes de la construcción ({done}/{})", states.len())
        };
        egui::CollapsingHeader::new(header)
            .default_open(true)
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(if is_door {
                        "Marcá en el lienzo los frames de un estado (usá «rango de frames» si \
                         están todos en el mismo PNG), tocá acá el estado, y 💾. Cada estado se \
                         guarda por separado — guardar uno NO borra los otros.\n\
                         El juego encadena: cerrada → abriéndose → abierta → cerrándose → \
                         cerrada. Las transiciones son opcionales; sin ellas la puerta salta \
                         directo de cerrada a abierta."
                    } else {
                        "Marcá en el lienzo de dónde a dónde está esta parte, elegí acá cuál es, \
                         y guardá con 💾. El juego elige sola la pose según dónde esté el piso: \
                         la pared del fondo se ve de frente, la de adelante baja, y las de los \
                         costados de perfil. Con sólo «frente» ya funciona todo."
                    })
                    .weak()
                    .small(),
                );
                if is_door {
                    ui.label(
                        egui::RichText::new(
                            "🚧 Colisión: la puerta NUNCA frena — ni cerrada. Es a propósito: en \
                             el sistema de bordes la celda de una puerta es transitable siempre, \
                             así que la animación es sólo cómo se ve, y abrirla no puede dejarte \
                             encerrado ni atravesar una pared. Si querés que una puerta bloquee \
                             de verdad, marcala «bloquea SIEMPRE» arriba y usá el estado \
                             🔒 bloqueada.",
                        )
                        .color(egui::Color32::from_rgb(255, 200, 100))
                        .small(),
                    );
                }
                let mut pick: Option<(String, Option<Pose>)> = None;
                let mut wipe: Option<String> = None;
                egui::Grid::new("states_grid").striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
                    for (pose_name, label) in states {
                        let existing = defined.get(*pose_name).cloned();
                        // Miniatura de lo guardado — el "ya está" que se ve, no
                        // un puntito verde que hay que interpretar.
                        match &existing {
                            Some(p) => {
                                let thumb = self.pose_thumb(ui.ctx(), slot.key, p);
                                match thumb {
                                    Some((id, uv)) => {
                                        ui.add(
                                            egui::Image::new(egui::load::SizedTexture::new(
                                                id,
                                                egui::vec2(40.0, 40.0),
                                            ))
                                            .uv(uv),
                                        );
                                    }
                                    None => {
                                        ui.label("●");
                                    }
                                }
                            }
                            None => {
                                ui.label(egui::RichText::new("—").weak());
                            }
                        }
                        let mut txt = egui::RichText::new(*label);
                        if existing.is_some() {
                            txt = txt.color(egui::Color32::from_rgb(120, 230, 140));
                        }
                        let active = self.pose_name == *pose_name;
                        let hint = match &existing {
                            Some(p) => format!(
                                "el juego la busca como '{pose_name}'\n{} frames · guardar la reemplaza",
                                p.anim.map(|a| a.frames).unwrap_or(1)
                            ),
                            None => format!("el juego la busca como '{pose_name}' — sin definir"),
                        };
                        if ui.selectable_label(active, txt).on_hover_text(hint).clicked() {
                            pick = Some(((*pose_name).to_string(), existing.clone()));
                        }
                        match &existing {
                            Some(p) => {
                                ui.label(
                                    egui::RichText::new(match p.anim {
                                        Some(a) => format!("{} frames @ {:.0} fps", a.frames, a.fps),
                                        None => "estática".to_string(),
                                    })
                                    .weak()
                                    .small(),
                                );
                                if ui.small_button("✖").on_hover_text("borrar este estado").clicked() {
                                    wipe = Some((*pose_name).to_string());
                                }
                            }
                            None => {
                                ui.label(egui::RichText::new("sin definir").weak().small());
                                ui.label("");
                            }
                        }
                        ui.end_row();
                    }
                });
                if let Some(name) = wipe {
                    self.manifest.remove_pose(slot.key, &name);
                    self.persist_manifest();
                    self.status = format!("{}: estado '{name}' borrado · guardado", slot.key);
                }
                // Elegir un estado ya guardado lo trae AL LIENZO para reajustarlo.
                if let Some((name, pose)) = pick {
                    self.pose_name = name;
                    if let Some(p) = pose {
                        self.sel_rect = Some(p.rect);
                        match p.anim {
                            Some(a) => {
                                self.anim_frames = a.frames;
                                self.anim_fps = a.fps;
                                self.strip_dir = p.dir.unwrap_or_default();
                            }
                            None => self.anim_frames = 1,
                        }
                        if let Some(rel) = &p.png {
                            self.load_candidate_rel(ui.ctx(), &rel.clone());
                        }
                        if let Some(g) = p.grid {
                            self.sheet_grid = g.0.max(1);
                        }
                        self.status = "estado cargado en el lienzo — ajustalo y volvé a 💾".into();
                    }
                }
                self.pose_save_row(ui, slot);
            });
    }

    /// Miniatura (textura + UV del primer frame) de una pose guardada.
    fn pose_thumb(
        &mut self,
        ctx: &egui::Context,
        key: &str,
        pose: &Pose,
    ) -> Option<(egui::TextureId, egui::Rect)> {
        let slot_grid = self.manifest.get(key).map(|v| v.grid()).unwrap_or((32, 32));
        let slot_png = self.manifest.get(key)?.png().to_string();
        let grid = pose.grid_or(slot_grid);
        let tex = self.tex_for(ctx, pose.png_or(&slot_png))?;
        let img = tex.size_vec2();
        let r = pose.rect;
        Some((
            tex.id(),
            egui::Rect::from_min_max(
                egui::pos2((r.col * grid.0) as f32 / img.x, (r.row * grid.1) as f32 / img.y),
                egui::pos2(
                    ((r.col + r.w) * grid.0) as f32 / img.x,
                    ((r.row + r.h) * grid.1) as f32 / img.y,
                ),
            ),
        ))
    }

    /// 🗺 **La huella de colisión y la zona de interacción, pintadas sobre el
    /// sprite.**
    ///
    /// Es un editor de dibujo chico, no un formulario: visor con zoom y
    /// desplazamiento propios (el sprite se ve GRANDE), cuatro herramientas
    /// —libre, rectángulo, pluma y balde— y tres materiales —sólido, escalón y
    /// zona—. Lo que había antes fallaba en cuatro cosas concretas, y las cuatro
    /// están arregladas acá:
    ///
    /// 1. **El lienzo no podía crecer.** El zoom estaba topado por el ancho del
    ///    panel (`fp_zoom.min(ancho/w)`), así que arrastrar la barra a la derecha
    ///    no hacía nada. Ahora el lienzo es un visor: el zoom llega a
    ///    [`FP_ZOOM_MAX`] px por tile, se desplaza con el botón del medio o con
    ///    Shift, hay Ctrl+rueda anclado al cursor y una ventana grande aparte.
    /// 2. **La zona de interacción no se veía.** Se pintaba en el conjunto pero
    ///    NUNCA se dibujaba, y encima se descartaba al guardar si el objeto no
    ///    tenía verbo: pintabas 🟦 y no pasaba absolutamente nada. Ahora se
    ///    dibuja en azul y pintarla marca el objeto como interactuable sola.
    /// 3. **«Proponer del sprite» leía el PNG del árbol equivocado**
    ///    (`art_root`, que es la carpeta de PNGs sueltos del usuario, cuando las
    ///    rutas del manifest cuelgan de `game_assets`): fallaba siempre salvo
    ///    casualidad.
    /// 4. **Guardaba el manifest entero en cada frame del arrastre.** Ahora se
    ///    escribe al soltar el botón (`fp_dirty`).
    ///
    /// El fondo sigue siendo EL RECORTE que va a ver el juego
    /// (`footprint_backdrop`): recorte estático, primer frame de la tira o la
    /// pose representativa — nunca la hoja entera.
    fn footprint_panel(&mut self, ui: &mut egui::Ui, slot: SlotDef) {
        let cur = self.manifest.footprint(slot.key).cloned().unwrap_or_default();
        let inter_actual = self.manifest.interaccion(slot.key).cloned();
        let n_zona = inter_actual.as_ref().map(|i| i.zona.len()).unwrap_or(0);
        let title = if cur.is_empty() && cur.step.is_empty() && n_zona == 0 {
            "huella y zona — pintalas sobre el sprite".to_string()
        } else {
            format!(
                "huella y zona — {} sólidas · {} escalón · {} zona",
                cur.solid.len(),
                cur.step.len(),
                n_zona
            )
        };
        let cabecera = egui::CollapsingHeader::new(title).default_open(true).show(ui, |ui| {
            // Dimensiones. El default sale del tamaño REAL del asset en celdas
            // (recorte, tira o pose), no de 1×1: una casa de 5×4 abre su grilla
            // de 5×4 sin que tengas que adivinarla.
            let def = self.slot_cells_for_footprint(slot.key);
            // **El detalle arranca en ¼, no en «tile».** A un sub-celda por tile
            // la huella NO PUEDE seguir la silueta: la unidad de pintura es el
            // cuadrado entero donde se coloca el objeto, así que "perfilar al
            // sprite" devolvía el rectángulo del sprite y pintar a mano tampoco
            // daba otra cosa. Sólo se hereda el detalle viejo si ya hay huella
            // pintada (no se toca lo que el usuario guardó).
            let sin_pintar = cur.w == 0 && inter_actual.as_ref().is_none_or(|i| i.zona.is_empty());
            let old_res = if sin_pintar { 4 } else { cur.res() };
            let mut ed = FpEdit {
                w: if cur.w == 0 { def.0 } else { cur.w.clamp(1, 12) },
                h: if cur.h == 0 { def.1 } else { cur.h.clamp(1, 12) },
                res: old_res,
                ax: cur.ax,
                az: cur.az,
                solid: cur.solid.iter().copied().collect(),
                step: cur.step.iter().copied().collect(),
                zona: inter_actual
                    .as_ref()
                    .map(|i| i.zona.iter().copied().collect())
                    .unwrap_or_default(),
            };
            let mut verbo = inter_actual.as_ref().map(|i| i.verbo.clone()).unwrap_or_default();
            let mut requiere: Vec<ha_assets::Requisito> =
                inter_actual.as_ref().map(|i| i.requiere.clone()).unwrap_or_default();
            let mut changed = false;

            // ── Interactuable ────────────────────────────────────────────────
            // Es lo que decide si al acercarte aparece el boton. Antes esto no
            // existia: lo interactuable era un enum fijo de siete cosas en el
            // codigo, asi que un banco o un surtidor nunca podian serlo.
            ui.horizontal_wrapped(|ui| {
                let mut es = !verbo.is_empty();
                if ui
                    .checkbox(&mut es, "se puede interactuar")
                    .on_hover_text(
                        "Una piedra se pica, un banco se sienta, un poste no hace nada.\n                         Sin tildar, el objeto no muestra boton al acercarse.",
                    )
                    .changed()
                {
                    verbo = if es { ha_assets::VERBOS[0].to_string() } else { String::new() };
                    changed = true;
                }
                if !verbo.is_empty() {
                    egui::ComboBox::from_id_salt("verbo_interaccion")
                        .selected_text(verbo.clone())
                        .show_ui(ui, |ui| {
                            for v in ha_assets::VERBOS {
                                if ui.selectable_label(verbo == *v, *v).clicked() {
                                    verbo = v.to_string();
                                    changed = true;
                                }
                            }
                        });
                    ui.label(
                        egui::RichText::new(if ed.zona.is_empty() {
                            "zona: todo el asset (pintala para acotarla)".to_string()
                        } else {
                            format!("zona: {} sub-celdas pintadas", ed.zona.len())
                        })
                        .weak()
                        .small(),
                    );
                }
            });

            // ── Requisitos ───────────────────────────────────────────────────
            // Sin requisitos el boton sale siempre. Con ellos, sale solo cuando
            // se cumplen TODOS: es lo que separa "hay una cocina" de "puedo
            // cocinar".
            if !verbo.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("hace falta:").strong());
                    let mut borrar = None;
                    for (i, r) in requiere.iter().enumerate() {
                        let txt = match r {
                            ha_assets::Requisito::EnMano(k) => format!("en mano: {k}"),
                            ha_assets::Requisito::EnMochila(k) => format!("en mochila: {k}"),
                            ha_assets::Requisito::Receta(k) => format!("receta: {k}"),
                            ha_assets::Requisito::DeNoche(true) => "de noche".into(),
                            ha_assets::Requisito::DeNoche(false) => "de dia".into(),
                            ha_assets::Requisito::Estado(e) => format!("estado: {e}"),
                        };
                        if ui.button(format!("{txt}  ✖")).on_hover_text("quitar").clicked() {
                            borrar = Some(i);
                        }
                    }
                    if let Some(i) = borrar {
                        requiere.remove(i);
                        changed = true;
                    }
                    egui::ComboBox::from_id_salt("add_requisito")
                        .selected_text("+ agregar")
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(false, "en mano...").clicked() {
                                requiere.push(ha_assets::Requisito::EnMano("hacha".into()));
                                changed = true;
                            }
                            if ui.selectable_label(false, "en mochila...").clicked() {
                                requiere.push(ha_assets::Requisito::EnMochila("hacha".into()));
                                changed = true;
                            }
                            if ui.selectable_label(false, "receta...").clicked() {
                                requiere.push(ha_assets::Requisito::Receta("pan".into()));
                                changed = true;
                            }
                            if ui.selectable_label(false, "de noche").clicked() {
                                requiere.push(ha_assets::Requisito::DeNoche(true));
                                changed = true;
                            }
                            for e in ha_assets::ESTADOS {
                                if ui.selectable_label(false, format!("estado: {e}")).clicked() {
                                    requiere.push(ha_assets::Requisito::Estado((*e).into()));
                                    changed = true;
                                }
                            }
                        });
                });
            }

            // ── Herramienta + material ────────────────────────────────────────
            // Dos ejes separados: CÓMO se elige la celda y QUÉ queda en ella.
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("herramienta:").strong());
                for (t, label, hint) in [
                    (
                        FpTool::Libre,
                        "libre",
                        "A mano alzada: arrastrás y pinta. El grosor es de al lado.",
                    ),
                    (
                        FpTool::Rect,
                        "rectángulo",
                        "Arrastrás de esquina a esquina. Lo más rápido para una pared o una base.",
                    ),
                    (
                        FpTool::Pluma,
                        "pluma",
                        "Como la pluma de Photoshop: un clic por vértice y la figura se cierra \
                         sola y se rellena. Enter cierra, Esc cancela, clic derecho quita el \
                         último vértice. Ideal para perfilar un techo torcido o una zona en L.",
                    ),
                    (
                        FpTool::Balde,
                        "balde",
                        "Rellena de una toda la región contigua que está igual: clic dentro de \
                         un hueco y queda todo el hueco.",
                    ),
                ] {
                    if ui.selectable_label(self.fp_tool == t, label).on_hover_text(hint).clicked() {
                        self.fp_tool = t;
                        self.fp_pen.clear();
                    }
                }
                if self.fp_tool == FpTool::Libre {
                    ui.separator();
                    ui.label("grosor").on_hover_text("en sub-celdas");
                    ui.add(egui::DragValue::new(&mut self.fp_size).range(1..=8));
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("pinta:").strong());
                for (b, label, hint) in [
                    (
                        FpBrush::Solid,
                        "sólido",
                        "Frena el paso. El tronco de un árbol, el muro de una casa, la roca.",
                    ),
                    (
                        FpBrush::Step,
                        "escalón",
                        "Desnivel que se SUBE y se BAJA a pie: no frena a quien camina, pero \
                         queda marcado como borde para lo que no puede treparlo (vehículos). \
                         Se sube y se baja por el mismo sitio a propósito — un escalón de una \
                         sola dirección te dejaría encerrado abajo.",
                    ),
                    (
                        FpBrush::Zona,
                        "zona",
                        "DESDE DÓNDE se interactúa. No frena. La punta del muelle para pescar, \
                         el umbral de la puerta. Pintarla marca el objeto como interactuable \
                         sola: no hace falta tildar nada antes.",
                    ),
                    (
                        FpBrush::Erase,
                        "borrar",
                        "Deja la celda limpia de todo (sólido, escalón y zona). También borra \
                         con Ctrl+arrastre desde cualquier material.",
                    ),
                ] {
                    // El nombre va DEL COLOR con el que pinta, y al lado una
                    // muestra del color: los cuadraditos de emoji (🟥🟨🟦) salían
                    // caja en la fuente de egui, así que el botón decía "□ zona"
                    // y no se entendía nada.
                    let (rect, resp) =
                        ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect.shrink(1.0), 2.0, b.color());
                    let _ = resp;
                    if ui
                        .selectable_label(
                            self.fp_brush == b,
                            egui::RichText::new(label).color(b.color()),
                        )
                        .on_hover_text(hint)
                        .clicked()
                    {
                        self.fp_brush = b;
                        if b == FpBrush::Zona && verbo.is_empty() {
                            verbo = ha_assets::VERBOS[0].to_string();
                            changed = true;
                        }
                    }
                }
            });

            // ── Visor: zoom, encuadre, ventana grande ─────────────────────────
            ui.horizontal_wrapped(|ui| {
                ui.label("zoom");
                ui.add(
                    egui::Slider::new(&mut self.fp_zoom, FP_ZOOM_MIN..=FP_ZOOM_MAX)
                        .show_value(false),
                )
                .on_hover_text("px por tile · también Ctrl+rueda sobre el lienzo");
                if ui.button("  -  ").on_hover_text("alejar").clicked() {
                    self.fp_zoom = (self.fp_zoom / 1.25).max(FP_ZOOM_MIN);
                }
                if ui.button("  +  ").on_hover_text("acercar").clicked() {
                    self.fp_zoom = (self.fp_zoom * 1.25).min(FP_ZOOM_MAX);
                }
                if ui.button("ajustar").on_hover_text("el asset entero, centrado").clicked() {
                    self.fp_fit = true;
                }
                ui.separator();
                ui.label("alto");
                ui.add(egui::Slider::new(&mut self.fp_view_h, 200.0..=900.0).show_value(false))
                    .on_hover_text("alto del visor");
                ui.separator();
                let txt = if self.fp_big { "cerrar la ventana grande" } else { "ventana grande" };
                if ui
                    .button(txt)
                    .on_hover_text("abre el lienzo en una ventana aparte, redimensionable")
                    .clicked()
                {
                    self.fp_big = !self.fp_big;
                    self.fp_fit = true;
                }
            });

            // ── Tamaño y detalle ──────────────────────────────────────────────
            ui.horizontal_wrapped(|ui| {
                ui.label("ancho");
                let a = ui.add(egui::DragValue::new(&mut ed.w).range(1..=12)).changed();
                ui.label("alto");
                let b = ui.add(egui::DragValue::new(&mut ed.h).range(1..=12)).changed();
                if a || b {
                    changed = true;
                    self.fp_fit = true;
                }
                ui.separator();
                ui.label("detalle").on_hover_text(
                    "Sub-celdas por tile. ¼ perfila la silueta: un poste delgado deja de \
                     bloquear el metro entero.",
                );
                for (v, name) in [(1u32, "tile"), (2, "½"), (4, "¼")] {
                    ui.selectable_value(&mut ed.res, v, name);
                }
                if ed.res == 1 {
                    ui.label(
                        egui::RichText::new("OJO — a «tile» sólo podés pintar el cuadrado entero")
                            .color(egui::Color32::from_rgb(235, 190, 40))
                            .small(),
                    )
                    .on_hover_text(
                        "Con un sub-celda por tile la unidad de pintura ES la casilla donde se \
                         coloca el objeto: no hay forma de seguir la silueta del dibujo. Pasá \
                         a ¼ para perfilarlo.",
                    );
                }
            });
            // La grilla tiene que ser la del sprite: si no, el dibujo se aplasta
            // dentro del lienzo y lo que pintás no cae donde creés.
            if (ed.w, ed.h) != def {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "OJO — la grilla es {}×{} y el sprite mide {}×{} tiles",
                            ed.w, ed.h, def.0, def.1
                        ))
                        .color(egui::Color32::from_rgb(235, 190, 40))
                        .small(),
                    );
                    if ui.button(format!("usar {}×{}", def.0, def.1)).clicked() {
                        ed.solid = rescale_grid(&ed.solid, (ed.w, ed.h), def, ed.res);
                        ed.step = rescale_grid(&ed.step, (ed.w, ed.h), def, ed.res);
                        ed.zona = rescale_grid(&ed.zona, (ed.w, ed.h), def, ed.res);
                        ed.w = def.0;
                        ed.h = def.1;
                        self.fp_fit = true;
                        changed = true;
                    }
                });
            }
            // Cambiar el detalle conserva lo pintado (se re-escala) — la zona
            // también: vive en la misma grilla.
            if ed.res != old_res {
                ed.solid = rescale_solid(&ed.solid, old_res, ed.res);
                ed.step = rescale_solid(&ed.step, old_res, ed.res);
                ed.zona = rescale_solid(&ed.zona, old_res, ed.res);
                changed = true;
            }

            // ── Atajos: sacar la forma del propio dibujo ──────────────────────
            // Nadie pinta 400 assets a mano. Esto lee el ALFA del recorte que ve
            // el juego y propone; después lo corregís con la pluma.
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("del sprite:").strong());
                let silueta = ui
                    .button("sólido = silueta")
                    .on_hover_text("marca sólido donde el dibujo no es transparente")
                    .clicked();
                let base = ui
                    .button("sólido = sólo la base")
                    .on_hover_text(
                        "sólo la fila de tiles del pie: el tronco frena, la copa se atraviesa",
                    )
                    .clicked();
                let borde = ui
                    .button("zona = borde de lo sólido")
                    .on_hover_text(
                        "pinta 🟦 el anillo de celdas libres pegadas a lo sólido: es \
                         literalmente «desde dónde lo alcanzo». Necesita algo sólido pintado.",
                    )
                    .clicked();
                if silueta || base {
                    // Perfilar al sprite exige que la grilla SEA la del sprite y
                    // que el detalle sea fino. Si no, lo que sale es justo lo que
                    // se veía mal: el cuadrado donde se coloca el objeto, no su
                    // silueta — a un sub-celda por tile no hay silueta posible, y
                    // con una grilla de 1×1 sobre un sprite de 5×4 el dibujo
                    // entero se aplasta en un tile.
                    let mut aviso = Vec::new();
                    if (ed.w, ed.h) != def {
                        ed.solid = rescale_grid(&ed.solid, (ed.w, ed.h), def, ed.res);
                        ed.step = rescale_grid(&ed.step, (ed.w, ed.h), def, ed.res);
                        ed.zona = rescale_grid(&ed.zona, (ed.w, ed.h), def, ed.res);
                        aviso.push(format!("grilla ajustada a {}×{} (la del sprite)", def.0, def.1));
                        ed.w = def.0;
                        ed.h = def.1;
                        self.fp_fit = true;
                    }
                    if ed.res < 4 {
                        ed.solid = rescale_solid(&ed.solid, ed.res, 4);
                        ed.step = rescale_solid(&ed.step, ed.res, 4);
                        ed.zona = rescale_solid(&ed.zona, ed.res, 4);
                        aviso.push("detalle ajustado a ¼".to_string());
                        ed.res = 4;
                    }
                    match self.huella_desde_alfa(slot.key, ed.w, ed.h, ed.res, 0.2) {
                        Some(cov) => {
                            let base_from = (ed.h * ed.res).saturating_sub(ed.res);
                            ed.solid =
                                cov.into_iter().filter(|&(_, r)| !(base && r < base_from)).collect();
                            ed.step.retain(|c| !ed.solid.contains(c));
                            self.status = format!(
                                "huella de {} perfilada al sprite: {} sub-celdas{}{}",
                                slot.key,
                                ed.solid.len(),
                                if aviso.is_empty() { "" } else { " · " },
                                aviso.join(" · ")
                            );
                            changed = true;
                        }
                        None => {
                            self.status =
                                "no pude leer los píxeles del sprite (¿PNG asignado?)".into()
                        }
                    }
                }
                if borde {
                    let (cols, rows) = (ed.w * ed.res, ed.h * ed.res);
                    ed.zona = borde_de(&ed.solid, cols, rows);
                    if ed.zona.is_empty() {
                        self.status = "pintá algo sólido primero: la zona sale de su borde".into();
                    } else {
                        if verbo.is_empty() {
                            verbo = ha_assets::VERBOS[0].to_string();
                        }
                        changed = true;
                    }
                }
                ui.menu_button("limpiar...", |ui| {
                    if ui.button("todo").clicked() {
                        ed.solid.clear();
                        ed.step.clear();
                        ed.zona.clear();
                        changed = true;
                        ui.close_menu();
                    }
                    if ui.button("sólo lo sólido").clicked() {
                        ed.solid.clear();
                        changed = true;
                        ui.close_menu();
                    }
                    if ui.button("sólo los escalones").clicked() {
                        ed.step.clear();
                        changed = true;
                        ui.close_menu();
                    }
                    if ui.button("sólo la zona").clicked() {
                        ed.zona.clear();
                        changed = true;
                        ui.close_menu();
                    }
                });
            });

            // Reajustar a las dimensiones nuevas.
            let (cols, rows) = (ed.w * ed.res, ed.h * ed.res);
            ed.solid.retain(|&(c, r)| c < cols && r < rows);
            ed.step.retain(|&(c, r)| c < cols && r < rows);
            ed.zona.retain(|&(c, r)| c < cols && r < rows);
            ed.ax = ed.ax.min(ed.w.saturating_sub(1));
            ed.az = ed.az.min(ed.h.saturating_sub(1));

            // ── Lienzo ────────────────────────────────────────────────────────
            // Reencuadre al cambiar de slot: cada asset abre entero y centrado,
            // no con el zoom que dejaste en el anterior.
            if self.fp_key != slot.key {
                self.fp_key = slot.key.to_string();
                self.fp_pen.clear();
                self.fp_rect0 = None;
                self.fp_fit = true;
            }
            let backdrop = self.footprint_backdrop(ui.ctx(), slot.key);
            if self.fp_big {
                let ctx = ui.ctx().clone();
                let mut abierta = true;
                egui::Window::new(format!("huella y zona — {}", slot.key))
                    .default_size([900.0, 700.0])
                    .min_size([320.0, 240.0])
                    .resizable(true)
                    .open(&mut abierta)
                    .show(&ctx, |ui| {
                        let view = ui.available_size() - egui::vec2(0.0, 28.0);
                        changed |= self.fp_canvas(ui, &mut ed, backdrop, view.max(egui::vec2(320.0, 200.0)));
                        ui.label(egui::RichText::new(fp_hint()).weak().small());
                    });
                if !abierta {
                    self.fp_big = false;
                }
                ui.label(
                    egui::RichText::new("(el lienzo está abierto en la ventana grande)")
                        .weak()
                        .small(),
                );
            } else {
                let view = egui::vec2(ui.available_width().max(240.0), self.fp_view_h);
                changed |= self.fp_canvas(ui, &mut ed, backdrop, view);
            }
            let tiles = ed
                .solid
                .iter()
                .chain(ed.step.iter())
                .map(|&(c, r)| (c / ed.res, r / ed.res))
                .collect::<std::collections::HashSet<_>>()
                .len();
            ui.label(
                egui::RichText::new(format!(
                    "{} sólidas · {} escalón · {} zona · {tiles} tiles frenan",
                    ed.solid.len(),
                    ed.step.len(),
                    ed.zona.len()
                ))
                .weak()
                .small(),
            );
            ui.label(egui::RichText::new(fp_hint()).weak().small());

            if changed {
                // Pintar la ZONA ES declarar el objeto interactuable: si no, la zona se
                // tiraba a la basura al guardar y pintarla no hacía nada.
                if !ed.zona.is_empty() && verbo.is_empty() {
                    verbo = ha_assets::VERBOS[0].to_string();
                }
                let fp = if ed.solid.is_empty() && ed.step.is_empty() {
                    None
                } else {
                    let mut sv: Vec<(u32, u32)> = ed.solid.iter().copied().collect();
                    let mut tv: Vec<(u32, u32)> = ed.step.iter().copied().collect();
                    sv.sort_unstable();
                    tv.sort_unstable();
                    Some(ha_assets::CollisionFootprint {
                        w: ed.w,
                        h: ed.h,
                        ax: ed.ax,
                        az: ed.az,
                        solid: sv,
                        step: tv,
                        res: ed.res,
                    })
                };
                self.manifest.set_footprint(slot.key, fp);
                // La interaccion va por su cuenta: sin verbo no es interactuable
                // y se borra; con verbo se guarda con su zona (vacia = todo el
                // asset).
                let inter = if verbo.is_empty() {
                    None
                } else {
                    let mut zv: Vec<(u32, u32)> = ed.zona.iter().copied().collect();
                    zv.sort_unstable();
                    Some(ha_assets::Interaccion {
                        verbo: verbo.clone(),
                        requiere: requiere.clone(),
                        zona: zv,
                    })
                };
                self.manifest.set_interaccion(slot.key, inter);
                self.fp_dirty = true;
            }
            // El disco espera a que sueltes: escribir el manifest entero en cada
            // frame del arrastre es lo que hacía que pintar se sintiera pegajoso.
            if self.fp_dirty && !ui.input(|i| i.pointer.any_down()) {
                let saved = self.persist_manifest();
                self.fp_dirty = false;
                self.status = format!(
                    "huella de {} actualizada {}",
                    slot.key,
                    if saved { "· guardado" } else { "· ⚠ no guardó" }
                );
            }
        });
        // Gancho de QA (`HA_SLOT`): traer el panel a la vista unos frames.
        if self.fp_scroll_qa > 0 {
            self.fp_scroll_qa -= 1;
            cabecera.header_response.scroll_to_me(Some(egui::Align::TOP));
        }
    }

    /// El lienzo en sí. Vive aparte del panel porque se dibuja en dos sitios —en
    /// línea y en la ventana grande— con el mismo estado. Devuelve `true` si el
    /// usuario cambió algo.
    fn fp_canvas(
        &mut self,
        ui: &mut egui::Ui,
        ed: &mut FpEdit,
        backdrop: Option<(egui::TextureId, egui::Rect)>,
        view: egui::Vec2,
    ) -> bool {
        let mut changed = false;
        let (resp, painter) = ui.allocate_painter(view, egui::Sense::click_and_drag());
        let rect = resp.rect;
        let painter = painter.with_clip_rect(rect);
        painter.rect_filled(rect, 0.0, egui::Color32::from_gray(24));
        painter.rect_stroke(rect, 0.0, egui::Stroke::new(1.0, egui::Color32::from_gray(70)));
        let (cols, rows) = (ed.w * ed.res, ed.h * ed.res);

        // ── encuadre ──────────────────────────────────────────────────────────
        if self.fp_fit {
            self.fp_zoom = ((rect.width() - 24.0) / ed.w as f32)
                .min((rect.height() - 24.0) / ed.h as f32)
                .clamp(FP_ZOOM_MIN, FP_ZOOM_MAX);
            self.fp_pan = egui::Vec2::ZERO;
            self.fp_fit = false;
        }
        // Ctrl+rueda: zoom ANCLADO AL CURSOR — el punto que estás mirando no se
        // mueve, que es lo que hace que un zoom se sienta bien.
        if resp.hovered() {
            let (scroll, ctrl) =
                ui.input(|i| (i.raw_scroll_delta.y, i.modifiers.command || i.modifiers.ctrl));
            if ctrl && scroll.abs() > 0.5 {
                let z = self.fp_zoom;
                let nz = (z * if scroll > 0.0 { 1.1 } else { 1.0 / 1.1 })
                    .clamp(FP_ZOOM_MIN, FP_ZOOM_MAX);
                if let Some(p) = resp.hover_pos() {
                    let min = rect.center() + self.fp_pan
                        - egui::vec2(ed.w as f32 * z, ed.h as f32 * z) / 2.0;
                    let u = (p - min) / z; // punto bajo el cursor, en tiles
                    let nmin = p - u * nz;
                    self.fp_pan = (nmin + egui::vec2(ed.w as f32 * nz, ed.h as f32 * nz) / 2.0)
                        - rect.center();
                }
                self.fp_zoom = nz;
            }
        }
        // Desplazar: botón del medio, o Shift+arrastre (portátiles sin rueda).
        let shift = ui.input(|i| i.modifiers.shift);
        let panning = resp.dragged_by(egui::PointerButton::Middle)
            || (shift && resp.dragged_by(egui::PointerButton::Primary));
        if panning {
            self.fp_pan += resp.drag_delta();
        }
        let tile = self.fp_zoom;
        let sub = tile / ed.res as f32;
        let img = egui::Rect::from_center_size(
            rect.center() + self.fp_pan,
            egui::vec2(ed.w as f32 * tile, ed.h as f32 * tile),
        );

        // ── fondo ─────────────────────────────────────────────────────────────
        // Damero: sin él, una zona transparente del sprite y una zona sin pintar
        // se ven exactamente igual.
        for r in 0..ed.h {
            for c in 0..ed.w {
                if (c + r) % 2 == 0 {
                    continue;
                }
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        img.min + egui::vec2(c as f32 * tile, r as f32 * tile),
                        egui::vec2(tile, tile),
                    ),
                    0.0,
                    egui::Color32::from_gray(48),
                );
            }
        }
        if let Some((id, uv)) = backdrop {
            painter.image(id, img, uv, egui::Color32::WHITE);
        }
        let cell_rect = |c: u32, r: u32| {
            egui::Rect::from_min_size(
                img.min + egui::vec2(c as f32 * sub, r as f32 * sub),
                egui::vec2(sub, sub),
            )
        };
        let fill = |set: &FpSet, color: egui::Color32| {
            for &(c, r) in set {
                painter.rect_filled(cell_rect(c, r), 0.0, color);
            }
        };
        // La zona va DEBAJO: es contexto, no le tapa la cara a la colisión.
        let a = |c: egui::Color32, alpha: u8| {
            egui::Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha)
        };
        fill(&ed.zona, a(FpBrush::Zona.color(), 90));
        fill(&ed.solid, a(FpBrush::Solid.color(), 120));
        fill(&ed.step, a(FpBrush::Step.color(), 120));

        // ── grilla ────────────────────────────────────────────────────────────
        // La fina (sub-celdas) tenue, la del TILE marcada: un tile es un metro
        // del mundo. Por debajo de 4 px por sub-celda la fina es ruido.
        if ed.res > 1 && sub >= 4.0 {
            let fino = egui::Stroke::new(0.5, egui::Color32::from_gray(80));
            for i in 1..cols {
                let x = img.min.x + i as f32 * sub;
                painter.line_segment([egui::pos2(x, img.min.y), egui::pos2(x, img.max.y)], fino);
            }
            for i in 1..rows {
                let y = img.min.y + i as f32 * sub;
                painter.line_segment([egui::pos2(img.min.x, y), egui::pos2(img.max.x, y)], fino);
            }
        }
        for r in 0..ed.h {
            for c in 0..ed.w {
                let cr = egui::Rect::from_min_size(
                    img.min + egui::vec2(c as f32 * tile, r as f32 * tile),
                    egui::vec2(tile, tile),
                );
                painter.rect_stroke(cr, 0.0, egui::Stroke::new(1.0, egui::Color32::from_gray(120)));
                if c == ed.ax && r == ed.az {
                    painter.rect_stroke(
                        cr,
                        0.0,
                        egui::Stroke::new(2.5, egui::Color32::from_rgb(60, 200, 220)),
                    );
                }
            }
        }

        // ── interacción ───────────────────────────────────────────────────────
        let to_cell = |p: egui::Pos2| -> Option<(u32, u32)> {
            let (c, r) = ((p.x - img.min.x) / sub, (p.y - img.min.y) / sub);
            (c >= 0.0 && r >= 0.0 && (c as u32) < cols && (r as u32) < rows)
                .then(|| (c as u32, r as u32))
        };
        let to_sub = |p: egui::Pos2| ((p.x - img.min.x) / sub, (p.y - img.min.y) / sub);
        let brush = self.fp_brush;
        let erase = brush == FpBrush::Erase || ui.input(|i| i.modifiers.command && !shift);
        let color = if erase { FpBrush::Erase.color() } else { brush.color() };

        // Marca la celda bajo el cursor: sin esto no sabés dónde vas a pintar
        // cuando el sprite es oscuro.
        if let Some(p) = resp.hover_pos() {
            if let Some((c, r)) = to_cell(p) {
                painter.rect_stroke(cell_rect(c, r), 0.0, egui::Stroke::new(1.5, color));
            }
        }

        if !panning {
            match self.fp_tool {
                FpTool::Libre => {
                    // Se pinta desde el frame en que APRETÁS, no desde el
                    // primero en que egui decide que es un arrastre: si no, la
                    // celda donde hiciste clic se queda sin pintar y el trazo
                    // empieza dos celdas más allá.
                    let apretado = resp.is_pointer_button_down_on()
                        && ui.input(|i| i.pointer.primary_down())
                        && !shift; // Shift es desplazar la vista, no pintar
                    if !apretado && !resp.clicked() {
                        self.fp_last = None;
                    }
                    if let Some(p) = resp.interact_pointer_pos() {
                        if (apretado || resp.clicked()) && !resp.secondary_clicked() {
                            let (fc, fr) = to_sub(p);
                            let now = (fc.floor() as i32, fr.floor() as i32);
                            let from = self.fp_last.unwrap_or(now);
                            let grosor = self.fp_size.max(1);
                            for (c, r) in line_cells(from, now) {
                                for dy in 0..grosor {
                                    for dx in 0..grosor {
                                        let (px, py) =
                                            (c + dx - grosor / 2, r + dy - grosor / 2);
                                        if px < 0 || py < 0 {
                                            continue;
                                        }
                                        let cell = (px as u32, py as u32);
                                        if cell.0 < cols && cell.1 < rows {
                                            changed |= fp_apply(ed, cell, brush, erase);
                                        }
                                    }
                                }
                            }
                            self.fp_last = Some(now);
                        }
                    }
                }
                FpTool::Rect => {
                    // La esquina se fija al APRETAR (igual que arriba), así un
                    // clic suelto es un rectángulo de una celda y no un gesto
                    // perdido.
                    if self.fp_rect0.is_none()
                        && resp.is_pointer_button_down_on()
                        && ui.input(|i| i.pointer.primary_down())
                        && !shift
                    {
                        self.fp_rect0 = resp.interact_pointer_pos().and_then(to_cell);
                    }
                    if let (Some(a0), Some(p)) = (self.fp_rect0, resp.interact_pointer_pos()) {
                        if let Some(b0) = to_cell(p) {
                            let (c0, c1) = (a0.0.min(b0.0), a0.0.max(b0.0));
                            let (r0, r1) = (a0.1.min(b0.1), a0.1.max(b0.1));
                            let vista = egui::Rect::from_min_max(
                                cell_rect(c0, r0).min,
                                cell_rect(c1, r1).max,
                            );
                            painter.rect_stroke(vista, 0.0, egui::Stroke::new(2.0, color));
                            if resp.drag_stopped() || resp.clicked() {
                                for r in r0..=r1 {
                                    for c in c0..=c1 {
                                        changed |= fp_apply(ed, (c, r), brush, erase);
                                    }
                                }
                                self.fp_rect0 = None;
                            }
                        }
                    }
                    if resp.drag_stopped() {
                        self.fp_rect0 = None;
                    }
                }
                FpTool::Pluma => {
                    // Vértices en coordenadas de sub-celda: sobreviven al zoom y
                    // al desplazamiento mientras dibujás.
                    let mut cerrar = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        self.fp_pen.clear();
                    }
                    if resp.secondary_clicked() {
                        self.fp_pen.pop();
                    } else if resp.double_clicked() {
                        cerrar = true;
                    } else if resp.clicked() {
                        if let Some(p) = resp.interact_pointer_pos() {
                            let v = to_sub(p);
                            // Clic sobre el primer vértice = cerrar, como en
                            // cualquier editor vectorial.
                            let sobre_inicio = self.fp_pen.first().is_some_and(|f| {
                                (f.0 - v.0).abs() < 0.75 && (f.1 - v.1).abs() < 0.75
                            });
                            if sobre_inicio && self.fp_pen.len() >= 3 {
                                cerrar = true;
                            } else {
                                self.fp_pen.push(v);
                            }
                        }
                    }
                    if cerrar && self.fp_pen.len() >= 3 {
                        for cell in poly_cells(&self.fp_pen, cols, rows) {
                            changed |= fp_apply(ed, cell, brush, erase);
                        }
                        self.fp_pen.clear();
                    }
                    // Trazo en curso: vértices, lados y la línea de goma hasta el
                    // cursor.
                    let to_screen =
                        |v: (f32, f32)| img.min + egui::vec2(v.0 * sub, v.1 * sub);
                    let trazo = egui::Stroke::new(2.0, color);
                    for par in self.fp_pen.windows(2) {
                        painter.line_segment([to_screen(par[0]), to_screen(par[1])], trazo);
                    }
                    if let (Some(&ultimo), Some(p)) = (self.fp_pen.last(), resp.hover_pos()) {
                        painter.line_segment(
                            [to_screen(ultimo), p],
                            egui::Stroke::new(1.0, a(color, 140)),
                        );
                        if self.fp_pen.len() >= 2 {
                            painter.line_segment(
                                [p, to_screen(self.fp_pen[0])],
                                egui::Stroke::new(1.0, a(color, 70)),
                            );
                        }
                    }
                    for (i, &v) in self.fp_pen.iter().enumerate() {
                        painter.circle_filled(
                            to_screen(v),
                            if i == 0 { 5.0 } else { 3.5 },
                            if i == 0 { egui::Color32::WHITE } else { color },
                        );
                    }
                }
                FpTool::Balde => {
                    if resp.clicked() && !resp.secondary_clicked() {
                        if let Some(cell) = resp.interact_pointer_pos().and_then(to_cell) {
                            // "Igual" se mide en el eje que estás pintando: con
                            // 🟦 rellenás la región sin zona, con 🟥 la región
                            // que no frena.
                            let semilla = fp_state(ed, cell, brush);
                            let region = flood_cells(cell, cols, rows, |c| {
                                fp_state(ed, c, brush) == semilla
                            });
                            for c in region {
                                changed |= fp_apply(ed, c, brush, erase);
                            }
                        }
                    }
                }
            }
            // El ancla es un TILE (la celda que apuntás al colocar el objeto). La
            // pluma se queda el clic derecho para quitar vértices.
            if resp.secondary_clicked() && self.fp_tool != FpTool::Pluma {
                if let Some(cell) = resp.interact_pointer_pos().and_then(to_cell) {
                    ed.ax = cell.0 / ed.res;
                    ed.az = cell.1 / ed.res;
                    changed = true;
                }
            }
        }
        changed
    }

    /// Tamaño en celdas con el que abrir la huella de un slot: su recorte, su
    /// tira, su pose representativa — o, si el arte es un PNG suelto, el tamaño
    /// de la imagen medido en tiles.
    ///
    /// Los dos casos que faltaban dolían igual: los assets con POSES no tienen
    /// `rect`, y un PNG suelto no tiene ninguno de los tres. Ambos abrían en 1×1,
    /// y una grilla de 1×1 aplasta un dibujo de 5×4 dentro de una sola casilla:
    /// lo único que podías pintar era el cuadrado donde se coloca el objeto, que
    /// es justo lo que se veía mal. El tamaño del PNG se mide leyendo la cabecera
    /// (no la imagen entera) y se cachea: esto se llama en cada frame.
    fn slot_cells_for_footprint(&mut self, key: &str) -> (u32, u32) {
        let Some(v) = self.manifest.get(key) else { return (1, 1) };
        let cells = match v {
            ha_assets::ArtValue::Full(e) => e
                .rect
                .map(|r| (r.w, r.h))
                .or_else(|| e.strip.map(|s| (s.w, s.h)))
                .or_else(|| {
                    let m = e.poses.as_ref()?;
                    let p = m
                        .get("idle_down")
                        .or_else(|| m.get("center"))
                        .or_else(|| m.get("closed"))
                        .or_else(|| m.values().next())?;
                    Some((p.rect.w, p.rect.h))
                }),
            _ => None,
        };
        let cells = match cells {
            Some(c) => c,
            // PNG suelto: el sprite ES la imagen, así que sus tiles salen de los
            // píxeles.
            None => {
                let (png, grid) = (v.png().to_string(), v.grid());
                match self.png_cells.get(&png) {
                    Some(&c) => c,
                    None => {
                        let c = image::image_dimensions(
                            PathBuf::from(&self.game_assets).join(&png),
                        )
                        .map(|(pw, ph)| {
                            (
                                pw.div_ceil(grid.0.max(1)).max(1),
                                ph.div_ceil(grid.1.max(1)).max(1),
                            )
                        })
                        .unwrap_or((1, 1));
                        self.png_cells.insert(png, c);
                        c
                    }
                }
            }
        };
        (cells.0.clamp(1, 12), cells.1.clamp(1, 12))
    }

    /// Textura + UV del recorte que el JUEGO va a dibujar para este slot, para
    /// usarlo de fondo del lienzo de la huella. Cubre los tres casos: recorte
    /// estático, primer frame de una tira, y pose representativa (que puede
    /// vivir en otra hoja).
    /// **Propone la huella leyendo el ALFA del sprite.**
    ///
    /// Pintar 400 assets a mano no lo hace nadie, y una huella "de un tile" para
    /// todo es lo que produce las colisiones que se sienten mal: una gasolinera
    /// bloqueando el pasadizo que su propio dibujo muestra abierto. Esto mira los
    /// pixeles: una sub-celda se propone SOLIDA si al menos `umbral` de sus
    /// pixeles son opacos.
    ///
    /// No inventa nada — sale del grafico, que es la unica fuente honesta — y es
    /// una PROPUESTA: se pinta sobre el lienzo y la corregis a mano (borrar el
    /// techo, abrir un paso). Por eso no toca el manifest hasta que aceptes.
    ///
    /// **Ojo con la raíz.** Las rutas del manifest cuelgan de `game_assets`
    /// (`.../ha-game/assets`), no de `art_root` (la carpeta de PNGs sueltos que
    /// mirás en el navegador). Esto leía de `art_root` y por eso el botón fallaba
    /// con un "no pude leer el PNG" en casi todos los slots.
    fn huella_desde_alfa(
        &mut self,
        key: &str,
        w: u32,
        h: u32,
        res: u32,
        umbral: f32,
    ) -> Option<std::collections::HashSet<(u32, u32)>> {
        let v = self.manifest.get(key)?.clone();
        // El mismo recorte que ve el juego: estatico, tira, pose representativa
        // — o el PNG entero si el arte es un archivo suelto (ese caso devolvía
        // None, y por eso "perfilar al sprite" no hacía nada en esos slots).
        let recorte = match &v {
            ha_assets::ArtValue::Full(e) => {
                let grid = e.grid.unwrap_or((32, 32));
                if let Some(r) = e.rect {
                    Some((r, e.png.clone(), grid))
                } else if let Some(s) = e.strip {
                    Some((s, e.png.clone(), grid))
                } else if let Some(m) = &e.poses {
                    let p = m
                        .get("idle_down")
                        .or_else(|| m.get("center"))
                        .or_else(|| m.get("closed"))
                        .or_else(|| m.values().next())?;
                    Some((
                        p.rect,
                        p.png.clone().unwrap_or_else(|| e.png.clone()),
                        p.grid.unwrap_or(grid),
                    ))
                } else {
                    None
                }
            }
            _ => None,
        };
        let png = recorte.as_ref().map(|(_, p, _)| p.clone()).unwrap_or_else(|| v.png().to_string());
        let ruta = std::path::Path::new(&self.game_assets).join(&png);
        let img = image::open(&ruta).ok()?.to_rgba8();
        let (x0, y0, pw, ph) = match recorte {
            Some((rect, _, grid)) => {
                (rect.col * grid.0, rect.row * grid.1, rect.w * grid.0, rect.h * grid.1)
            }
            None => (0, 0, img.width(), img.height()),
        };
        if pw == 0 || ph == 0 {
            return None;
        }
        let (sub_w, sub_h) = (w * res, h * res);
        let mut out = std::collections::HashSet::new();
        for sy in 0..sub_h {
            for sx in 0..sub_w {
                // Rectangulo de pixeles que le toca a esta sub-celda.
                let px0 = x0 + pw * sx / sub_w;
                let px1 = x0 + pw * (sx + 1) / sub_w;
                let py0 = y0 + ph * sy / sub_h;
                let py1 = y0 + ph * (sy + 1) / sub_h;
                let (mut opacos, mut total) = (0u32, 0u32);
                for py in py0..py1.max(py0 + 1) {
                    for px in px0..px1.max(px0 + 1) {
                        if px >= img.width() || py >= img.height() {
                            continue;
                        }
                        total += 1;
                        if img.get_pixel(px, py).0[3] > 24 {
                            opacos += 1;
                        }
                    }
                }
                if total > 0 && (opacos as f32 / total as f32) >= umbral {
                    out.insert((sx, sy));
                }
            }
        }
        Some(out)
    }

    fn footprint_backdrop(
        &mut self,
        ctx: &egui::Context,
        key: &str,
    ) -> Option<(egui::TextureId, egui::Rect)> {
        let v = self.manifest.get(key)?.clone();
        let ha_assets::ArtValue::Full(e) = &v else {
            let (_, tex) = self.tex.clone()?;
            return Some((tex.id(), egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))));
        };
        let slot_grid = e.grid.unwrap_or((32, 32));
        // Rect en celdas + la hoja de la que sale.
        let (rect, sheet, grid) = if let Some(r) = e.rect {
            (r, e.png.clone(), slot_grid)
        } else if let Some(s) = e.strip {
            (s, e.png.clone(), slot_grid)
        } else if let Some(m) = &e.poses {
            let p = m
                .get("idle_down")
                .or_else(|| m.get("center"))
                .or_else(|| m.get("closed"))
                .or_else(|| m.values().next())?;
            (
                p.rect,
                p.png.clone().unwrap_or_else(|| e.png.clone()),
                p.grid.unwrap_or(slot_grid),
            )
        } else {
            // PNG suelto: la imagen entera es el sprite.
            let (_, tex) = self.tex.clone()?;
            return Some((tex.id(), egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))));
        };
        let tex = self.tex_for(ctx, &sheet)?;
        let img = tex.size_vec2();
        let uv = egui::Rect::from_min_max(
            egui::pos2((rect.col * grid.0) as f32 / img.x, (rect.row * grid.1) as f32 / img.y),
            egui::pos2(
                ((rect.col + rect.w) * grid.0) as f32 / img.x,
                ((rect.row + rect.h) * grid.1) as f32 / img.y,
            ),
        );
        Some((tex.id(), uv))
    }

    fn light_panel(&mut self, ui: &mut egui::Ui, key: &str) {
        let current = self.manifest.emission(key).copied();
        let mut on = current.is_some();
        let mut em = current.unwrap_or_default();
        let header = if on { "💡 luz — ENCENDIDA" } else { "💡 luz (emite / no emite)" };
        egui::CollapsingHeader::new(header).default_open(on).show(ui, |ui| {
            ui.label(
                egui::RichText::new(
                    "El asset ilumina su entorno con un aura suave que se difumina. \
                     Por defecto solo se ve de NOCHE (y bajo tierra / en cuerpos sin \
                     atmosfera): de dia no se dibuja y no cuesta FPS.",
                )
                .weak()
                .small(),
            );
            let before = (on, em);
            ui.checkbox(&mut on, "este asset EMITE luz");
            if on {
                ui.horizontal(|ui| {
                    ui.label("color");
                    ui.color_edit_button_rgb(&mut em.color);
                    ui.checkbox(&mut em.day, "tambien de dia")
                        .on_hover_text("cristal magico, pantalla encendida — cuesta un poco mas");
                });
                ui.add(
                    egui::Slider::new(&mut em.radius, 0.5..=24.0)
                        .text("radio del aura (m)")
                        .clamping(egui::SliderClamping::Always),
                )
                .on_hover_text("1 tile = 1 m. Vela ~2, antorcha ~4, farola ~8, hoguera ~10");
                ui.add(egui::Slider::new(&mut em.intensity, 0.0..=3.0).text("intensidad"));
                ui.add(egui::Slider::new(&mut em.flicker, 0.0..=1.0).text("parpadeo (llama)"))
                    .on_hover_text("0 = lampara fija · 1 = fuego vivo");
                ui.horizontal(|ui| {
                    ui.label("foco x/y (m)").on_hover_text(
                        "corre el centro de la luz: la ventana de una casa no esta en su centro",
                    );
                    ui.add(egui::DragValue::new(&mut em.offset[0]).speed(0.05).range(-8.0..=8.0));
                    ui.add(egui::DragValue::new(&mut em.offset[1]).speed(0.05).range(-8.0..=8.0));
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("presets:").weak());
                    for (label, e) in LIGHT_PRESETS {
                        if ui.small_button(*label).clicked() {
                            em = *e;
                        }
                    }
                });
                light_preview(ui, &em);
            }
            if (on, em) != before {
                self.manifest.set_emission(key, on.then_some(em));
                let saved = self.persist_manifest();
                self.status = format!(
                    "luz de {key}: {} {}",
                    if on { "encendida" } else { "apagada" },
                    if saved { "· guardado" } else { "· ⚠ no guardó" }
                );
            }
        });
    }

    /// Panel derecho dedicado a elegir el PNG fuente. Lista **virtualizada**
    /// (`show_rows`) → rinde miles de PNGs sin trabarse y siempre visible.
    fn right_pngs(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("pngs")
            .resizable(true)
            .default_width(300.0)
            .width_range(180.0..=620.0)
            .show(ctx, |ui| {
            ui.heading("PNGs fuente");
            ui.horizontal(|ui| {
                ui.label("🔎");
                ui.text_edit_singleline(&mut self.png_filter);
            });
            ui.label(egui::RichText::new(format!("{} PNGs · click o ↑/↓ = preview", self.pngs.len())).weak());
            ui.separator();
            if self.pngs.is_empty() {
                ui.label(egui::RichText::new("Elegí carpeta / Cargar PNGs arriba").weak());
                return;
            }
            let pf = self.png_filter.to_lowercase();
            let filtered: Vec<&PathBuf> = self
                .pngs
                .iter()
                .filter(|p| {
                    pf.is_empty()
                        || p.file_name()
                            .map(|n| n.to_string_lossy().to_lowercase().contains(&pf))
                            .unwrap_or(false)
                })
                .collect();
            let row_h = ui.text_style_height(&egui::TextStyle::Body) + 4.0;
            let mut pick: Option<PathBuf> = None;

            // Navegación con flechas ↑/↓: sólo cuando NINGÚN campo de texto/número
            // tiene el foco (así no pisa la edición de un DragValue). Mueve la
            // selección por la lista y actualiza el preview en vivo.
            let n = filtered.len();
            if n > 0 && ctx.memory(|m| m.focused().is_none()) {
                let down = ctx.input(|i| i.key_pressed(egui::Key::ArrowDown));
                let up = ctx.input(|i| i.key_pressed(egui::Key::ArrowUp));
                if down || up {
                    let cur = filtered
                        .iter()
                        .position(|p| self.selected_png.as_deref() == Some(p.as_path()));
                    let next = match cur {
                        Some(i) if down => (i + 1).min(n - 1),
                        Some(i) if up => i.saturating_sub(1),
                        _ => 0,
                    };
                    pick = Some((*filtered[next]).clone());
                    self.png_scroll_to = Some(next);
                }
            }

            let mut scroll = egui::ScrollArea::vertical().auto_shrink([false, false]);
            if let Some(row) = self.png_scroll_to.take() {
                // Mantener la fila navegada visible (unas filas desde arriba).
                scroll = scroll.vertical_scroll_offset(row.saturating_sub(4) as f32 * row_h);
            }
            scroll.show_rows(ui, row_h, filtered.len(), |ui, range| {
                for i in range {
                    let p = filtered[i];
                    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    let sel = self.selected_png.as_deref() == Some(p.as_path());
                    if ui.selectable_label(sel, name).clicked() {
                        pick = Some((*p).clone());
                    }
                }
            });
            if let Some(p) = pick {
                self.src_tex = load_texture(ctx, &p, "src_preview");
                self.selected_png = Some(p);
            }
        });
    }

    // ── acciones ───────────────────────────────────────────────────────────

    /// `ctx` presente (fuera del constructor) → también refresca la textura del
    /// slot seleccionado, para que "Recargar" se vea reflejado sin reabrir nada.
    fn load_manifest(&mut self, ctx: Option<&egui::Context>) {
        match std::fs::read_to_string(&self.game_manifest) {
            Ok(s) => match ArtManifest::from_json(&s) {
                Ok(m) => {
                    self.manifest = m;
                    self.status = format!("manifest cargado ({}): {} bindings", self.game_manifest, self.manifest.sprites.len());
                }
                Err(e) => self.status = format!("manifest ilegible: {e}"),
            },
            Err(_) => {
                self.manifest = ArtManifest::default();
                self.status = format!("manifest nuevo (no existía en {}) — se creará al guardar", self.game_manifest);
            }
        }
        // Claves del manifest que no son slots nativos → agregarlas (custom persist).
        // Una clave de variante (`objeto#variante`) siembra su OBJETO en la lista y
        // se declara como variante si venía sin declarar (manifest escrito a mano o
        // por un script): así nada del archivo queda invisible en el editor.
        let keys: Vec<String> = self.manifest.sprites.keys().cloned().collect();
        for k in keys {
            match ha_assets::split_variant(&k) {
                (base, Some(v)) => {
                    let base = base.to_string();
                    self.ensure_slot(&base);
                    if !self.manifest.variants(&base).iter().any(|d| d.id == v) {
                        self.manifest.declare_variant(&base, v, "");
                    }
                }
                (base, None) => self.ensure_slot(base),
            }
        }
        // Un objeto declarado con variantes también entra en la lista aunque no
        // tenga ni un PNG todavía.
        let bases: Vec<String> = self.manifest.objects.keys().cloned().collect();
        for b in bases {
            self.ensure_slot(&b);
        }
        if let Some(ctx) = ctx {
            if let Some(key) = self.current_key() {
                self.tex = self.preview_bound(ctx, key);
            }
        }
    }

    fn load_pngs(&mut self) {
        let root = self.abs_art_root();
        if !root.is_dir() {
            self.status = format!("Arte fuente no es carpeta: {}", root.display());
            return;
        }
        self.pngs = list_pngs(&root);
        self.status = format!("{} PNGs fuente cargados de {}", self.pngs.len(), root.display());
    }

    fn select_slot(&mut self, ctx: &egui::Context, i: usize) {
        self.selected = Some(i);
        self.confirm_full_png = false; // limpiar confirmación pendiente al cambiar de slot
        // Con una variante activa, lo que se sincroniza es SU entrada (cada
        // variante puede tener su propia animación).
        let mut slot = self.slots[i];
        if let Some(key) = self.current_key() {
            slot.key = key;
        }
        // Sincronizar buffers de animación desde el binding (o el hint del slot).
        match self.manifest.get(slot.key).and_then(|v| v.anim()) {
            Some(a) => {
                self.anim_on = true;
                self.anim_frames = a.frames;
                self.anim_fps = a.fps;
                self.strip_dir = self
                    .manifest
                    .get(slot.key)
                    .and_then(|v| v.strip_dir())
                    .unwrap_or_default();
            }
            None => {
                self.anim_on = slot.animated;
                self.anim_frames = 4;
                self.anim_fps = 6.0;
            }
        }
        self.tex = self.preview_bound(ctx, slot.key);
    }

    fn bind_png(&mut self, ctx: &egui::Context, slot: SlotDef, src: &Path) {
        // Copiar el PNG fuente al árbol del juego como art/<key>.png. En una
        // variante la clave trae `#` (`floor/piso#madera`): en el NOMBRE de archivo
        // va como `__` (hay shells/herramientas que tratan `#` como comentario), la
        // clave del manifest sí lo conserva.
        let rel = format!("art/{}.png", slot.key.replace(ha_assets::VARIANT_SEP, "__"));
        let dest = PathBuf::from(&self.game_assets).join(&rel);
        if let Some(parent) = dest.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                self.status = format!("no pude crear {}: {e}", parent.display());
                return;
            }
        }
        if let Err(e) = std::fs::copy(src, &dest) {
            self.status = format!("no pude copiar el PNG: {e}");
            return;
        }
        let anim = self.anim_on.then_some(ArtAnim { frames: self.anim_frames.max(1), fps: self.anim_fps.max(0.1) });
        self.manifest.set(slot.key, rel.clone(), anim);
        self.tex = load_texture(ctx, &dest, slot.key);
        let saved = self.persist_manifest();
        self.status = format!("{} ← {rel} {}", slot.key, if saved { "· guardado en el juego" } else { "· ⚠ no se pudo guardar" });
    }

    fn apply_anim(&mut self, key: &str) {
        if let Some(v) = self.manifest.get(key) {
            let png = v.png().to_string();
            let anim = self.anim_on.then_some(ArtAnim { frames: self.anim_frames.max(1), fps: self.anim_fps.max(0.1) });
            self.manifest.set(key, png, anim);
            let saved = self.persist_manifest();
            self.status = format!("anim de {key} aplicada {}", if saved { "· guardado" } else { "· ⚠ no guardó" });
        }
    }

    /// Copia la hoja fuente al árbol del juego bajo `art/atlas/<nombre>_<hash>.png`
    /// — **content-addressed**: si otro slot ya recortó de este MISMO atlas
    /// (mismos bytes), reusa el archivo en vez de duplicarlo. Esto es lo que
    /// hace viable tener un atlas de UI enorme (corazones, barras, botones,
    /// portraits…) y recortar decenas de piezas de él sin inflar el APK con
    /// una copia completa por cada pieza. Devuelve la ruta relativa a
    /// registrar en el manifest (compartida entre todos los slots que usen
    /// este mismo atlas).
    fn ensure_sheet_copied(&mut self, _slot: SlotDef, src: &Path) -> Option<String> {
        let bytes = match std::fs::read(src) {
            Ok(b) => b,
            Err(e) => {
                self.status = format!("no pude leer la hoja fuente: {e}");
                return None;
            }
        };
        let hash = fnv1a32(&bytes);
        let stem = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "atlas".to_string());
        let rel = format!("art/atlas/{}_{hash:08x}.png", sanitize_filename(&stem));
        let dest = PathBuf::from(&self.game_assets).join(&rel);
        if dest.exists() {
            self.status = format!("atlas ya copiado, reusado: {rel}");
            return Some(rel);
        }
        if let Some(parent) = dest.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                self.status = format!("no pude crear {}: {e}", parent.display());
                return None;
            }
        }
        match std::fs::write(&dest, &bytes) {
            Ok(()) => Some(rel),
            Err(e) => {
                self.status = format!("no pude copiar la hoja: {e}");
                None
            }
        }
    }

    /// Fila de guardado de la pose apuntada por `pose_name`. Se dibuja PEGADA a
    /// cada selector de nombre (matriz de animaciones, autotile, campo custom):
    /// antes había un solo botón al fondo del panel y desde la matriz —que tiene
    /// su propio scroll— quedaba fuera de vista, así que parecía que no se podía
    /// guardar y no se podía pasar a la dirección siguiente.
    ///
    /// Guardar NO pisa las otras poses: cada nombre (`idle_down`, `idle_up`…) es
    /// una entrada aparte del slot. Lo único que se reemplaza es el nombre exacto
    /// que estés apuntando, y eso se avisa antes de tocar el botón.
    fn pose_save_row(&mut self, ui: &mut egui::Ui, slot: SlotDef) {
        let ctx = ui.ctx().clone();
        let name = self.pose_name.trim().to_string();
        let Some(sr) = self.sel_rect else {
            ui.label(
                egui::RichText::new("💾 para guardar: marcá antes el recorte en el lienzo").weak(),
            );
            return;
        };
        let Some(png) = self.selected_png.clone() else {
            ui.label(egui::RichText::new("💾 para guardar: elegí un PNG a la derecha").weak());
            return;
        };
        // Aviso ANTES de guardar: si el nombre ya existe se reemplaza esa pose, y
        // si otra pose usa exactamente este mismo recorte lo más probable es que
        // te hayas olvidado de mover la selección al frame de la dirección nueva.
        let existing = self.manifest.poses(slot.key);
        let overwrites = existing.map(|m| m.contains_key(&name)).unwrap_or(false);
        let same_rect: Option<String> = existing.and_then(|m| {
            m.iter()
                .find(|(n, p)| **n != name && p.rect == sr && p.png.is_none())
                .map(|(n, _)| n.clone())
        });
        ui.horizontal_wrapped(|ui| {
            let btn = egui::Button::new(egui::RichText::new("💾 Guardar animación / pose").strong());
            if ui.add_enabled(!name.is_empty(), btn).clicked() {
                if let Some(rel) = self.ensure_sheet_copied(slot, &png) {
                    let grid = (self.sheet_grid.max(1), self.sheet_grid.max(1));
                    let frames = self.anim_frames.max(1);
                    let animated = frames > 1;
                    let anim = animated.then_some(ArtAnim { frames, fps: self.anim_fps.max(0.1) });
                    let dir = animated.then_some(self.strip_dir);
                    let layer = (self.pose_layer != ArtLayer::Object).then_some(self.pose_layer);
                    self.manifest.set_pose(
                        slot.key,
                        rel,
                        grid,
                        &name,
                        Pose { rect: sr, anim, dir, layer, png: None, grid: None },
                    );
                    self.tex = self.preview_bound(&ctx, slot.key);
                    let saved = self.persist_manifest();
                    let hojas = self.manifest.sheets(slot.key).len();
                    self.status = format!(
                        "{}: '{name}' ({}{} frames){} {}",
                        slot.key,
                        if animated { "" } else { "estática, " },
                        frames,
                        if hojas > 1 { format!(" · {hojas} hojas en este asset") } else { String::new() },
                        if saved { "· guardado en el juego" } else { "· ⚠ no guardó" }
                    );
                }
            }
            if name.is_empty() {
                ui.label(egui::RichText::new("← elegí una casilla o escribí el nombre").weak());
            } else {
                let verb = if overwrites { "reemplaza" } else { "nueva" };
                ui.label(
                    egui::RichText::new(format!(
                        "{name} ({verb}) · sel col{} row{} · {}×{} · {} frames",
                        sr.col, sr.row, sr.w, sr.h, self.anim_frames.max(1)
                    ))
                    .weak(),
                );
            }
        });
        if let Some(other) = same_rect {
            ui.label(
                egui::RichText::new(format!(
                    "⚠ ese mismo recorte ya está guardado como '{other}' — movés la selección al \
                     frame de esta dirección antes de guardar"
                ))
                .color(egui::Color32::from_rgb(255, 200, 100))
                .small(),
            );
        }
    }

    /// Matriz guiada de animaciones nombradas para atlas de personaje/NPC (idle/
    /// walk × 4 direcciones) + acciones custom. Cada casilla es una animación
    /// (una pose) aparte. Click = la elige como destino (y si ya existe, la carga
    /// al lienzo para ajustarla); después guardás con 💾. El juego lee estos
    /// nombres para el jugador (`idle_down`, `walk_left`…); NPCs reusan el mismo
    /// esquema cuando se cableen.
    fn anim_matrix(&mut self, ui: &mut egui::Ui, slot: SlotDef) {
        ui.label(
            egui::RichText::new(
                "Un atlas suele tener una FILA por animación y las columnas son sus frames. \
                 Marcá en el lienzo el PRIMER frame de una fila, usá 'toda la fila →' para los \
                 frames, tocá acá la casilla acción×dirección, y 💾. Cada casilla es una \
                 animación aparte: podés tener disparar, dormir, celular, correr… en las 8 \
                 direcciones, y cada una puede salir de OTRO PNG.",
            )
            .weak()
            .small(),
        );

        // Direcciones + filtro del catálogo.
        ui.horizontal_wrapped(|ui| {
            ui.label("direcciones:");
            for (m, label, hint) in [
                (DirMode::Four, "4 ↑↓←→", "cardinales (atlas LimeZu clásico)"),
                (DirMode::Eight, "8 ↖↗↘↙", "con diagonales (el juego las usa si están)"),
                (DirMode::None, "sin dirección", "dormir, sentarse, morir…"),
            ] {
                if ui.selectable_label(self.dir_mode == m, label).on_hover_text(hint).clicked() {
                    self.dir_mode = m;
                }
            }
            ui.separator();
            ui.label("🔎 acción");
            ui.add(
                egui::TextEdit::singleline(&mut self.action_filter)
                    .desired_width(120.0)
                    .hint_text("disparar, dormir…"),
            );
        });

        let dirs = self.dir_mode.dirs();
        let actions = self.visible_actions(slot);

        // Snapshot de las poses ya guardadas (para el estado ● y para cargarlas).
        let existing: std::collections::BTreeMap<String, Pose> = self
            .manifest
            .poses(slot.key)
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();

        let mut pick: Option<(String, Option<Pose>)> = None;
        let mut wipe: Option<String> = None;
        egui::ScrollArea::vertical()
            .id_salt("anim_matrix_scroll")
            .max_height(320.0)
            .show(ui, |ui| {
                egui::Grid::new("anim_matrix").spacing([6.0, 4.0]).striped(true).show(ui, |ui| {
                    // Cabecera con FLECHA Y NOMBRE. Sólo con la flecha, las
                    // columnas se leían como cuadraditos vacíos y no había forma
                    // de saber cuál era cuál sin pasar el mouse por encima.
                    ui.label(egui::RichText::new("acción").strong());
                    for (d, arrow) in dirs {
                        let name = dir_label(d);
                        ui.label(egui::RichText::new(format!("{arrow} {name}")).strong())
                            .on_hover_text(format!("la animación se guarda como accion_{d}"));
                    }
                    ui.label(egui::RichText::new("").weak());
                    ui.end_row();
                    let mut family = "";
                    for (action, label, fam) in &actions {
                        // Separador de familia (movimiento / combate / trabajo / vida).
                        if *fam != family {
                            family = fam;
                            ui.label(
                                egui::RichText::new(fam.to_uppercase())
                                    .small()
                                    .color(egui::Color32::from_rgb(120, 210, 255)),
                            );
                            for _ in dirs {
                                ui.label("");
                            }
                            ui.label("");
                            ui.end_row();
                        }
                        ui.label(egui::RichText::new(label.as_str()).strong())
                            .on_hover_text(action.as_str());
                        let mut defined = 0;
                        for (d, _arrow) in dirs {
                            let name = pose_name_of(action, d);
                            let has = existing.get(&name).cloned();
                            let cell = match &has {
                                Some(p) => {
                                    defined += 1;
                                    format!("●{}f", p.anim.map(|a| a.frames).unwrap_or(1))
                                }
                                None => "＋".to_string(),
                            };
                            let mut txt = egui::RichText::new(cell).monospace();
                            if has.is_some() {
                                txt = txt.color(egui::Color32::from_rgb(120, 230, 140));
                            }
                            let active = self.pose_name == name;
                            let hint = match &has {
                                Some(p) => format!(
                                    "{name}\nhoja: {}",
                                    p.png.as_deref().unwrap_or("(la del slot)")
                                ),
                                None => format!("{name} — sin definir"),
                            };
                            if ui.selectable_label(active, txt).on_hover_text(hint).clicked() {
                                pick = Some((name, has));
                            }
                        }
                        // Borrar toda la fila (las direcciones de esa acción).
                        if defined > 0 {
                            if ui
                                .small_button("✖")
                                .on_hover_text(format!("borrar las {defined} poses de '{action}'"))
                                .clicked()
                            {
                                wipe = Some((*action).to_string());
                            }
                        } else {
                            ui.label("");
                        }
                        ui.end_row();
                    }
                });
            });

        if let Some(action) = wipe {
            let names: Vec<String> =
                dirs.iter().map(|(d, _)| pose_name_of(&action, d)).collect();
            let mut n = 0;
            for name in names {
                if self.manifest.poses(slot.key).map(|m| m.contains_key(&name)).unwrap_or(false) {
                    self.manifest.remove_pose(slot.key, &name);
                    n += 1;
                }
            }
            self.persist_manifest();
            self.status = format!("{}: {n} poses de '{action}' borradas · guardado", slot.key);
        }

        // Aplicar la elección: setea el nombre destino y, si ya existía, carga su
        // recorte/animación al lienzo para verla y reajustarla.
        if let Some((name, pose)) = pick {
            self.pose_name = name;
            if let Some(p) = pose {
                self.sel_rect = Some(p.rect);
                match p.anim {
                    Some(a) => {
                        self.anim_frames = a.frames;
                        self.anim_fps = a.fps;
                        self.strip_dir = p.dir.unwrap_or_default();
                    }
                    None => self.anim_frames = 1,
                }
                // Si la pose vive en OTRA hoja, traé esa hoja al lienzo — si no,
                // estarías recortando la imagen equivocada.
                if let Some(rel) = &p.png {
                    self.load_candidate_rel(ui.ctx(), &rel.clone());
                }
                if let Some(g) = p.grid {
                    self.sheet_grid = g.0.max(1);
                }
                self.status = "animación cargada al lienzo — ajustá y volvé a 💾".into();
            }
        }

        // Acciones propias: lo que el catálogo no cubre (tu juego, tus verbos).
        ui.horizontal_wrapped(|ui| {
            ui.label("＋ acción propia:");
            ui.add(
                egui::TextEdit::singleline(&mut self.new_action)
                    .desired_width(150.0)
                    .hint_text("regatear, soldar…"),
            );
            if ui.button("añadir a la matriz").clicked() {
                let a = sanitize_action(&self.new_action);
                if a.is_empty() {
                    self.status = "poné un nombre de acción (sin espacios)".into();
                } else if !self.custom_actions.contains(&a) {
                    self.status = format!("acción '{a}' añadida a la matriz");
                    self.custom_actions.push(a);
                    self.new_action.clear();
                }
            }
            ui.separator();
            ui.label("nombre destino:");
            ui.add(
                egui::TextEdit::singleline(&mut self.pose_name)
                    .desired_width(160.0)
                    .hint_text("attack_down, sit…"),
            );
        });
        // Guardar acá mismo: elegís la casilla y el botón está al lado, no al
        // fondo del panel. Sin esto no se podía cerrar una dirección y pasar a la
        // siguiente.
        self.pose_save_row(ui, slot);

        self.bulk_slice_panel(ui, slot, dirs, &actions);
    }

    /// Acciones que muestra la matriz: el catálogo (filtrado) + las tuyas + las
    /// que YA existen en el manifest de este slot aunque no estén en el catálogo
    /// (así nada de lo que guardaste se vuelve invisible).
    fn visible_actions(&self, slot: SlotDef) -> Vec<(String, String, String)> {
        let f = self.action_filter.trim().to_lowercase();
        let known: std::collections::BTreeSet<&str> =
            ACTION_CATALOG.iter().map(|(a, _, _)| *a).collect();
        let dirs_all: Vec<&str> = DirMode::Eight.dirs().iter().map(|(d, _)| *d).collect();
        // Acciones deducidas de las poses guardadas (quitándoles el sufijo de dirección).
        let mut extra: Vec<String> = Vec::new();
        for name in self.manifest.poses(slot.key).map(|m| m.keys()).into_iter().flatten() {
            let base = dirs_all
                .iter()
                .find_map(|d| name.strip_suffix(&format!("_{d}")))
                .unwrap_or(name.as_str());
            if !known.contains(base) && !extra.iter().any(|e| e == base) {
                extra.push(base.to_string());
            }
        }
        let mut out: Vec<(String, String, String)> = Vec::new();
        for (a, label, fam) in ACTION_CATALOG {
            if f.is_empty() || a.contains(&f) || label.contains(&f) {
                out.push((a.to_string(), format!("{label}  ({a})"), fam.to_string()));
            }
        }
        for a in self.custom_actions.iter().chain(extra.iter()) {
            if out.iter().any(|(x, _, _)| x == a) {
                continue;
            }
            if f.is_empty() || a.contains(&f) {
                out.push((a.clone(), a.clone(), "propias".to_string()));
            }
        }
        out
    }

    /// 📥 **Importar varios PNG a UN slot**: el caso que no se podía hacer. Un
    /// cuerpo cuyo `idle_left` es un PNG y su `idle_down` es otro no es un atlas
    /// — son N archivos que pertenecen al MISMO asset. Acá elegís la carpeta, el
    /// editor deduce el nombre de cada animación del nombre del archivo y cuántos
    /// frames tiene (una tira horizontal de 128×32 son 4 frames de 32×32), te deja
    /// corregir todo en la tabla, y recién ahí escribe.
    fn import_folder_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, slot: SlotDef) {
        egui::CollapsingHeader::new("📥 importar VARIOS PNG a este asset (uno por dirección/estado)")
            .default_open(!self.import_plan.is_empty())
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(
                        "Ejemplo: cuerpo_idle_left.png, cuerpo_idle_down.png, cuerpo_walk_up.png… \
                         Cada archivo entra como una animación del slot, con SU hoja y SU grilla — \
                         ninguna pisa a las otras.",
                    )
                    .weak()
                    .small(),
                );
                // LOS PNG YA ESTÁN CARGADOS: no hay que volver a importarlos por
                // un diálogo. Los de la lista de la derecha (con el filtro que
                // tengas puesto) entran directo. Escribís "puerta" en el buscador
                // de la derecha y tomás las tres animaciones de una.
                let picked = self.filtered_pngs();
                ui.horizontal_wrapped(|ui| {
                    let take_list = format!("⬅ Tomar los {} PNG de la lista", picked.len());
                    if ui
                        .add_enabled(!picked.is_empty(), egui::Button::new(take_list))
                        .on_hover_text(
                            "Los PNG del panel derecho tal como los estás filtrando ahora. \
                             Filtrá por el nombre de tu objeto y entran todos sus estados juntos.",
                        )
                        .clicked()
                    {
                        self.plan_import(&picked);
                    }
                    if let Some(p) = self.selected_png.clone() {
                        if ui
                            .button("＋ añadir el candidato")
                            .on_hover_text("el PNG que tenés seleccionado a la derecha")
                            .clicked()
                        {
                            self.add_to_import_plan(&[p]);
                        }
                    }
                    ui.separator();
                    if ui.button("📁 carpeta…").clicked() {
                        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                            self.plan_import(&list_pngs(&dir));
                        }
                    }
                    if ui.button("🖼 archivos…").clicked() {
                        if let Some(files) =
                            rfd::FileDialog::new().add_filter("PNG", &["png"]).pick_files()
                        {
                            self.plan_import(&files);
                        }
                    }
                    if !self.import_plan.is_empty() && ui.button("✖ descartar").clicked() {
                        self.import_plan.clear();
                    }
                });
                if self.import_plan.is_empty() {
                    return;
                }
                let take = self.import_plan.iter().filter(|i| i.take).count();
                egui::ScrollArea::vertical().id_salt("import_plan").max_height(260.0).show(ui, |ui| {
                    egui::Grid::new("import_grid").striped(true).show(ui, |ui| {
                        ui.label(egui::RichText::new("✔").strong());
                        ui.label(egui::RichText::new("archivo").strong());
                        ui.label(egui::RichText::new("animación").strong());
                        ui.label(egui::RichText::new("frames").strong());
                        ui.label(egui::RichText::new("celda px").strong());
                        ui.end_row();
                        for item in &mut self.import_plan {
                            ui.checkbox(&mut item.take, "");
                            let file = item
                                .path
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default();
                            ui.label(egui::RichText::new(file).small());
                            ui.add(egui::TextEdit::singleline(&mut item.name).desired_width(150.0));
                            ui.add(egui::DragValue::new(&mut item.frames).range(1..=256));
                            ui.label(
                                egui::RichText::new(format!("{}×{}", item.grid.0, item.grid.1)).small(),
                            );
                            ui.end_row();
                        }
                    });
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("fps");
                    ui.add(egui::DragValue::new(&mut self.anim_fps).range(0.1..=60.0));
                    if ui
                        .button(egui::RichText::new(format!("✔ Importar {take} animaciones")).strong())
                        .clicked()
                    {
                        self.apply_import(ctx, slot);
                    }
                });
            });
    }

    /// Los PNG de la lista de la derecha con el filtro que hay puesto ahora. Es
    /// la MISMA lista que ves, así que "tomar los de la lista" hace exactamente
    /// lo que promete.
    fn filtered_pngs(&self) -> Vec<PathBuf> {
        let pf = self.png_filter.to_lowercase();
        self.pngs
            .iter()
            .filter(|p| {
                pf.is_empty()
                    || p.file_name()
                        .map(|n| n.to_string_lossy().to_lowercase().contains(&pf))
                        .unwrap_or(false)
            })
            .cloned()
            .collect()
    }

    /// Arma el plan de importación: nombre deducido + frames/grilla detectados de
    /// las dimensiones de cada PNG. No escribe nada todavía.
    fn plan_import(&mut self, paths: &[PathBuf]) {
        self.import_plan.clear();
        self.add_to_import_plan(paths);
    }

    /// Suma PNG al plan que ya haya (sin duplicar), para ir armándolo de a poco.
    fn add_to_import_plan(&mut self, paths: &[PathBuf]) {
        for p in paths {
            if self.import_plan.iter().any(|i| &i.path == p) {
                continue;
            }
            let Ok(img) = image::open(p) else { continue };
            let (grid, frames, dir) = detect_strip(img.width(), img.height());
            let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            self.import_plan.push(ImportItem {
                path: p.clone(),
                name: infer_pose_name(&stem),
                frames,
                grid,
                dir,
                take: true,
            });
        }
        self.status = if self.import_plan.is_empty() {
            "no encontré PNGs legibles ahí".into()
        } else {
            format!(
                "{} PNGs listos — revisá los nombres y tocá Importar",
                self.import_plan.len()
            )
        };
    }

    /// Escribe el plan: copia cada PNG al árbol del juego y registra su pose.
    fn apply_import(&mut self, ctx: &egui::Context, slot: SlotDef) {
        let plan: Vec<ImportItem> = self.import_plan.iter().filter(|i| i.take).cloned().collect();
        let fps = self.anim_fps.max(0.1);
        let (mut ok, mut fail) = (0usize, 0usize);
        for item in &plan {
            let Some(rel) = self.ensure_sheet_copied(slot, &item.path) else {
                fail += 1;
                continue;
            };
            let name = sanitize_pose_name(&item.name);
            if name.is_empty() {
                fail += 1;
                continue;
            }
            let frames = item.frames.max(1);
            // Cada archivo entra entero: el frame es UNA celda de SU grilla (que
            // por eso se detectó como el tamaño del frame), y si es una tira, los
            // `frames` siguientes son bloques contiguos en la dirección detectada.
            let rect = CellRect { col: 0, row: 0, w: 1, h: 1 };
            let anim = (frames > 1).then_some(ArtAnim { frames, fps });
            self.manifest.set_pose(
                slot.key,
                rel,
                item.grid,
                &name,
                Pose {
                    rect,
                    anim,
                    dir: anim.map(|_| item.dir),
                    layer: None,
                    png: None,
                    grid: None,
                },
            );
            ok += 1;
        }
        self.tex_cache.clear();
        self.tex = self.preview_bound(ctx, slot.key);
        let saved = self.persist_manifest();
        self.import_plan.clear();
        self.status = format!(
            "{}: {ok} animaciones importadas{} · {} hojas en el asset {}",
            slot.key,
            if fail > 0 { format!(" ({fail} fallaron)") } else { String::new() },
            self.manifest.sheets(slot.key).len(),
            if saved { "· guardado en el juego" } else { "· ⚠ no guardó" }
        );
    }

    /// ⚡ **Auto-slice de filas**: el 90 % del trabajo de un atlas de personaje en
    /// un click. Marcás el PRIMER frame de la PRIMERA fila y el editor reparte las
    /// filas siguientes entre las animaciones en el orden que tenga tu atlas —
    /// en vez de recortar 32 veces a mano.
    fn bulk_slice_panel(
        &mut self,
        ui: &mut egui::Ui,
        slot: SlotDef,
        dirs: &'static [(&'static str, &'static str)],
        actions: &[(String, String, String)],
    ) {
        egui::CollapsingHeader::new("⚡ auto-slice: repartir filas del atlas entre las animaciones")
            .default_open(false)
            .show(ui, |ui| {
                let Some(sr) = self.sel_rect else {
                    ui.label(
                        egui::RichText::new("Marcá primero en el lienzo el PRIMER frame de la PRIMERA fila.")
                            .weak(),
                    );
                    return;
                };
                ui.label(
                    egui::RichText::new(
                        "Cada animación ocupa una FILA (o columna, si la tira va ↓). Se escriben \
                         todas con el mismo tamaño de frame y la misma cantidad que pusiste arriba.",
                    )
                    .weak()
                    .small(),
                );
                ui.horizontal_wrapped(|ui| {
                    ui.label("orden del atlas:");
                    if ui
                        .selectable_label(self.bulk_order == BulkOrder::ByAction, "acción→direcciones")
                        .on_hover_text("idle_down, idle_up, idle_left, idle_right, walk_down…")
                        .clicked()
                    {
                        self.bulk_order = BulkOrder::ByAction;
                    }
                    if ui
                        .selectable_label(self.bulk_order == BulkOrder::ByDirection, "dirección→acciones")
                        .on_hover_text("idle_down, walk_down, run_down…, idle_up…")
                        .clicked()
                    {
                        self.bulk_order = BulkOrder::ByDirection;
                    }
                    ui.separator();
                    ui.label("filas");
                    ui.add(egui::DragValue::new(&mut self.bulk_rows).range(1..=64));
                });

                let names = bulk_names(actions, dirs, self.bulk_order, self.bulk_rows as usize);
                // TABLA, no una lista de nombres pegados con comas: acá se ve
                // fila por fila del atlas qué animación le va a tocar y de qué
                // celda va a salir. Es lo que hay que poder comprobar ANTES de
                // escribir 32 poses de un botonazo.
                let defined: std::collections::BTreeSet<String> = self
                    .manifest
                    .poses(slot.key)
                    .map(|m| m.keys().cloned().collect())
                    .unwrap_or_default();
                egui::ScrollArea::vertical().id_salt("bulk_preview").max_height(200.0).show(ui, |ui| {
                    egui::Grid::new("bulk_grid").striped(true).spacing([10.0, 2.0]).show(ui, |ui| {
                        ui.label(egui::RichText::new("fila").strong().small());
                        ui.label(egui::RichText::new("animación").strong().small());
                        ui.label(egui::RichText::new("sale de").strong().small());
                        ui.label(egui::RichText::new("").strong().small());
                        ui.end_row();
                        for (i, name) in names.iter().enumerate() {
                            let i = i as u32;
                            let rect = match self.strip_dir {
                                StripDir::Right => CellRect { row: sr.row + i * sr.h.max(1), ..sr },
                                StripDir::Down => CellRect { col: sr.col + i * sr.w.max(1), ..sr },
                            };
                            ui.label(egui::RichText::new(format!("{}", i + 1)).weak().small());
                            ui.label(egui::RichText::new(name).small());
                            ui.label(
                                egui::RichText::new(format!("col{} row{}", rect.col, rect.row))
                                    .weak()
                                    .small()
                                    .monospace(),
                            );
                            ui.label(
                                egui::RichText::new(if defined.contains(name) {
                                    "⚠ ya existe — se reemplaza"
                                } else {
                                    "nueva"
                                })
                                .small()
                                .color(if defined.contains(name) {
                                    egui::Color32::from_rgb(255, 200, 100)
                                } else {
                                    egui::Color32::from_gray(140)
                                }),
                            );
                            ui.end_row();
                        }
                    });
                });
                let Some(png) = self.selected_png.clone() else {
                    ui.label(egui::RichText::new("(elegí un PNG candidato)").weak());
                    return;
                };
                if ui
                    .button(egui::RichText::new(format!("⚡ Aplicar a {} animaciones", names.len())).strong())
                    .clicked()
                {
                    let Some(rel) = self.ensure_sheet_copied(slot, &png) else { return };
                    let grid = (self.sheet_grid.max(1), self.sheet_grid.max(1));
                    let frames = self.anim_frames.max(1);
                    let anim = (frames > 1).then_some(ArtAnim { frames, fps: self.anim_fps.max(0.1) });
                    let dir = (frames > 1).then_some(self.strip_dir);
                    for (i, name) in names.iter().enumerate() {
                        let i = i as u32;
                        // La fila siguiente avanza PERPENDICULAR a la tira: si los
                        // frames van a la derecha, la animación siguiente está una
                        // fila más abajo (y al revés).
                        let rect = match self.strip_dir {
                            StripDir::Right => CellRect { row: sr.row + i * sr.h.max(1), ..sr },
                            StripDir::Down => CellRect { col: sr.col + i * sr.w.max(1), ..sr },
                        };
                        self.manifest.set_pose(
                            slot.key,
                            rel.clone(),
                            grid,
                            name,
                            Pose { rect, anim, dir, layer: None, png: None, grid: None },
                        );
                    }
                    let saved = self.persist_manifest();
                    self.status = format!(
                        "{}: {} animaciones sliceadas de {} {}",
                        slot.key,
                        names.len(),
                        rel,
                        if saved { "· guardado en el juego" } else { "· ⚠ no guardó" }
                    );
                }
            });
    }

    /// Preview que REPRODUCE la animación 1:1 con el juego: cicla `frames` bloques
    /// del tamaño de `sr` (w×h celdas), avanzando a la derecha o hacia abajo según
    /// `dir`, a `anim_fps`.
    fn anim_strip_preview(&mut self, ui: &mut egui::Ui, sr: CellRect, frames: u32, dir: StripDir) {
        let Some((_, tex)) = self.src_tex.clone() else {
            return;
        };
        let img = tex.size_vec2();
        let grid = self.sheet_grid.max(1) as f32;
        let frames = frames.max(1);
        let dt = ui.input(|i| i.stable_dt).min(0.1);
        self.anim_preview_t += dt * self.anim_fps.max(0.1);
        if self.anim_preview_t >= 1.0 {
            let steps = self.anim_preview_t.floor() as usize;
            self.anim_preview_frame = (self.anim_preview_frame + steps) % frames as usize;
            self.anim_preview_t = self.anim_preview_t.fract();
        }
        let f = (self.anim_preview_frame as u32) % frames;
        let (fw, fh) = (sr.w.max(1), sr.h.max(1));
        // Origen del frame f: bloques contiguos del mismo tamaño en la dirección.
        let (fc, fr) = match dir {
            StripDir::Right => (sr.col + f * fw, sr.row),
            StripDir::Down => (sr.col, sr.row + f * fh),
        };
        let uv = egui::Rect::from_min_max(
            egui::pos2(fc as f32 * grid / img.x, fr as f32 * grid / img.y),
            egui::pos2((fc + fw) as f32 * grid / img.x, (fr + fh) as f32 * grid / img.y),
        );
        // Display `preview_px` de lado mayor, preservando el aspecto del frame.
        let base = self.preview_px.max(48.0);
        let aspect = fw as f32 / fh as f32;
        let (dw, dh) = if aspect >= 1.0 { (base, base / aspect) } else { (base * aspect, base) };
        // En COLUMNA: vive a la derecha del lienzo, así que crece hacia abajo y no
        // se come el ancho del sprite que estás recortando.
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("▶ preview animado").strong());
            ui.add(egui::Image::new(egui::load::SizedTexture::new(tex.id(), egui::vec2(dw, dh))).uv(uv));
            ui.label(
                egui::RichText::new(format!("{frames} frames de {fw}×{fh} @ {:.0} fps", self.anim_fps))
                    .weak()
                    .small(),
            );
            ui.horizontal(|ui| {
                ui.label("tamaño").on_hover_text("agrandá el preview para ver mejor la animación");
                ui.add(egui::DragValue::new(&mut self.preview_px).range(48.0..=640.0).speed(4.0));
            });
        });
        ui.ctx().request_repaint(); // sigue animando
    }

    /// Preview del binding ACTUAL (lo que verá el juego). Si el slot tiene poses,
    /// muestra una representativa (idle_down o la 1ª) ANIMADA y recortada del atlas
    /// — así confirmás al toque que tus poses quedaron guardadas, en vez de la hoja
    /// cruda. Sin poses, la imagen tal cual (recorte estático / tira / plano).
    // ── ficha del asset ─────────────────────────────────────────────────────
    // Lo primero que aparece al clickear el objeto en la lista de la izquierda:
    // el sprite tal como lo dibuja el juego, la hoja de la que sale (con el
    // recorte marcado) y los datos que el juego va a leer. La promesa es que lo
    // que se ve acá es lo que pasa allá — sobre todo la COLISIÓN, que es lo que
    // se viene a dibujar.

    /// **Qué va a hacer el juego con este asset**: texto humano + si frena o no.
    /// Vive aparte de la UI porque es la promesa que la ficha le hace al usuario
    /// y eso se testea. Espeja `ha-game::world2d::decor_blocks`: primero la
    /// huella pintada, después el override de la casilla, y al final el default
    /// por capa (`Pared` frena, el resto no) o por tipo (árbol frena, roca no).
    fn resumen_colision(&self, key: &str) -> (String, bool) {
        if let Some(fp) = self.manifest.footprint(key).filter(|f| !f.is_empty()) {
            let tiles = fp
                .solid
                .iter()
                .map(|&(c, r)| (c / fp.res(), r / fp.res()))
                .collect::<std::collections::HashSet<_>>()
                .len();
            return (
                format!(
                    "huella pintada — {} sub-celdas sólidas ({tiles} tiles frenan) · {} de escalón",
                    fp.solid.len(),
                    fp.step.len()
                ),
                !fp.solid.is_empty(),
            );
        }
        match self.manifest.collides(key) {
            Some(true) => ("sólido: frena en toda su celda (casilla del editor)".into(), true),
            Some(false) => ("se pisa: no frena a nadie (casilla del editor)".into(), false),
            None => {
                // Default del juego. El de la capa es el que aplica a lo que se
                // planta desde el editor de mundo; el de tipo, a lo procedural.
                let (base, _) = ha_assets::split_variant(key);
                if self.manifest.slot_layer(key) == Some(ArtLayer::Wall) {
                    ("default de capa Pared: frena en su celda".into(), true)
                } else if base.contains("tree") {
                    ("default de árbol: el tronco frena en su celda".into(), true)
                } else {
                    ("default: no frena (pintale la huella si tiene que frenar)".into(), false)
                }
            }
        }
    }

    /// Hoja de la que sale el sprite y su recorte EN PÍXELES — lo mismo que
    /// elige el juego (`rect` → pose representativa → tira → imagen entera).
    /// `None` en el recorte = la hoja entera ES el sprite.
    fn atlas_vista(&mut self, ctx: &egui::Context, key: &str) -> Option<(String, egui::TextureHandle, Option<[u32; 4]>, (u32, u32))> {
        let v = self.manifest.get(key)?.clone();
        let ha_assets::ArtValue::Full(e) = &v else {
            let rel = v.png().to_string();
            let tex = self.tex_for(ctx, &rel)?;
            return Some((rel, tex, None, (32, 32)));
        };
        let slot_grid = e.grid.unwrap_or((32, 32));
        let (rect, sheet, grid) = if let Some(r) = e.rect {
            (Some(r), e.png.clone(), slot_grid)
        } else if let Some(m) = &e.poses {
            match m
                .get("idle_down")
                .or_else(|| m.get("center"))
                .or_else(|| m.get("closed"))
                .or_else(|| m.values().next())
            {
                Some(p) => (
                    Some(p.rect),
                    p.png.clone().unwrap_or_else(|| e.png.clone()),
                    p.grid.unwrap_or(slot_grid),
                ),
                None => (None, e.png.clone(), slot_grid),
            }
        } else if let Some(s) = e.strip {
            (Some(s), e.png.clone(), slot_grid)
        } else {
            (None, e.png.clone(), slot_grid)
        };
        let tex = self.tex_for(ctx, &sheet)?;
        let px = rect.map(|r| [r.col * grid.0, r.row * grid.1, r.w * grid.0, r.h * grid.1]);
        Some((sheet, tex, px, grid))
    }

    /// El recorte del slot **en el frame que toca ahora**: si el arte se dibujó
    /// animado, la ficha lo muestra animado (regla del proyecto: nunca congelado).
    /// Sin animación cae al recorte fijo de `footprint_backdrop`.
    fn ficha_backdrop(
        &mut self,
        ui: &egui::Ui,
        key: &str,
    ) -> Option<(egui::TextureId, egui::Rect)> {
        let v = self.manifest.get(key)?.clone();
        let ha_assets::ArtValue::Full(e) = &v else { return self.footprint_backdrop(ui.ctx(), key) };
        let slot_grid = e.grid.unwrap_or((32, 32));
        // Misma elección que el juego: pose representativa, si no la tira.
        let (rect, sheet, grid, anim, dir) = if let Some(m) = &e.poses {
            let p = m
                .get("idle_down")
                .or_else(|| m.get("center"))
                .or_else(|| m.get("closed"))
                .or_else(|| m.values().next())?;
            (
                p.rect,
                p.png.clone().unwrap_or_else(|| e.png.clone()),
                p.grid.unwrap_or(slot_grid),
                p.anim,
                p.dir.unwrap_or_default(),
            )
        } else if let Some(s) = e.strip {
            (s, e.png.clone(), slot_grid, e.anim, e.strip_dir.unwrap_or_default())
        } else {
            return self.footprint_backdrop(ui.ctx(), key);
        };
        let Some(a) = anim.filter(|a| a.frames > 1) else {
            return self.footprint_backdrop(ui.ctx(), key);
        };
        // Reloj del preview (compartido con el resto de las vistas animadas).
        let dt = ui.input(|i| i.stable_dt).min(0.1);
        self.bound_preview_t += dt * a.fps.max(0.1);
        if self.bound_preview_t >= 1.0 {
            let pasos = self.bound_preview_t.floor() as usize;
            self.bound_preview_frame = (self.bound_preview_frame + pasos) % a.frames as usize;
            self.bound_preview_t = self.bound_preview_t.fract();
        }
        ui.ctx().request_repaint();
        let f = (self.bound_preview_frame as u32) % a.frames;
        let (fw, fh) = (rect.w.max(1), rect.h.max(1));
        let (fc, fr) = match dir {
            StripDir::Right => (rect.col + f * fw, rect.row),
            StripDir::Down => (rect.col, rect.row + f * fh),
        };
        let tex = self.tex_for(ui.ctx(), &sheet)?;
        let img = tex.size_vec2();
        Some((
            tex.id(),
            egui::Rect::from_min_max(
                egui::pos2((fc * grid.0) as f32 / img.x, (fr * grid.1) as f32 / img.y),
                egui::pos2(
                    ((fc + fw) * grid.0) as f32 / img.x,
                    ((fr + fh) * grid.1) as f32 / img.y,
                ),
            ),
        ))
    }

    /// El sprite del slot con su huella encima, del porte que le corresponde en
    /// tiles. Es el mismo dibujo que el lienzo de la huella, pero de sólo lectura
    /// y sin desplegar nada: la respuesta a "¿qué va a haber en el juego?".
    fn ficha_sprite(&mut self, ui: &mut egui::Ui, key: &str, max: f32) {
        let backdrop = self.ficha_backdrop(ui, key);
        let fp = self.manifest.footprint(key).cloned();
        let zona = self
            .manifest
            .interaccion(key)
            .map(|i| i.zona.clone())
            .unwrap_or_default();
        let (cw, ch) = match &fp {
            Some(f) if f.w > 0 && f.h > 0 => (f.w, f.h),
            _ => self.slot_cells_for_footprint(key),
        };
        let (cw, ch) = (cw.max(1), ch.max(1));
        let aspect = cw as f32 / ch as f32;
        let size = if aspect >= 1.0 {
            egui::vec2(max, max / aspect)
        } else {
            egui::vec2(max * aspect, max)
        };
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let p = ui.painter_at(rect);
        let tile = egui::vec2(rect.width() / cw as f32, rect.height() / ch as f32);
        // Damero: una zona transparente del sprite y una zona vacía no se ven
        // igual (es la misma razón que en el lienzo de la huella).
        p.rect_filled(rect, 0.0, egui::Color32::from_gray(28));
        for r in 0..ch {
            for c in 0..cw {
                if (c + r) % 2 == 0 {
                    continue;
                }
                p.rect_filled(
                    egui::Rect::from_min_size(
                        rect.min + egui::vec2(c as f32 * tile.x, r as f32 * tile.y),
                        tile,
                    ),
                    0.0,
                    egui::Color32::from_gray(44),
                );
            }
        }
        match backdrop {
            Some((id, uv)) => {
                p.image(id, rect, uv, egui::Color32::WHITE);
            }
            None => {
                p.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "— procedural —",
                    egui::FontId::proportional(13.0),
                    egui::Color32::from_gray(150),
                );
            }
        }
        if self.ficha_colision {
            let res = fp.as_ref().map(|f| f.res()).unwrap_or(1).max(1);
            let sub = egui::vec2(tile.x / res as f32, tile.y / res as f32);
            let pinta = |set: &[(u32, u32)], color: egui::Color32, alpha: u8| {
                for &(c, r) in set {
                    p.rect_filled(
                        egui::Rect::from_min_size(
                            rect.min + egui::vec2(c as f32 * sub.x, r as f32 * sub.y),
                            sub,
                        ),
                        0.0,
                        egui::Color32::from_rgba_unmultiplied(
                            color.r(),
                            color.g(),
                            color.b(),
                            alpha,
                        ),
                    );
                }
            };
            pinta(&zona, FpBrush::Zona.color(), 80);
            if let Some(f) = &fp {
                pinta(&f.solid, FpBrush::Solid.color(), 120);
                pinta(&f.step, FpBrush::Step.color(), 120);
            }
        }
        if self.ficha_grilla {
            let st = egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(140, 140, 140, 110));
            for c in 1..cw {
                let x = rect.min.x + c as f32 * tile.x;
                p.line_segment([egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)], st);
            }
            for r in 1..ch {
                let y = rect.min.y + r as f32 * tile.y;
                p.line_segment([egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)], st);
            }
            // Celda ANCLA: la que queda bajo el cursor al plantarlo en el mundo.
            let (ax, az) = match &fp {
                Some(f) => (f.ax, f.az),
                None => ha_assets::CollisionFootprint::default_anchor(cw, ch),
            };
            p.rect_stroke(
                egui::Rect::from_min_size(
                    rect.min + egui::vec2(ax as f32 * tile.x, az as f32 * tile.y),
                    tile,
                ),
                0.0,
                egui::Stroke::new(2.0, egui::Color32::from_rgb(60, 200, 220)),
            );
        }
        p.rect_stroke(rect, 0.0, egui::Stroke::new(1.0, egui::Color32::from_gray(90)));
    }

    /// La HOJA de la que sale el sprite, entera, con el recorte marcado y el
    /// resto apagado. Es lo que faltaba para poder dibujar la colisión mirando
    /// el mismo pixel que ve el juego. Un clic la deja de candidata para
    /// recortarla de nuevo abajo.
    fn ficha_atlas(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, key: &str, max: f32) {
        let Some((rel, tex, crop, grid)) = self.atlas_vista(ctx, key) else {
            ui.label(egui::RichText::new("sin hoja asignada").weak());
            return;
        };
        let img = tex.size_vec2();
        let escala = (max / img.x.max(1.0)).min(max / img.y.max(1.0)).min(4.0);
        let size = img * escala;
        let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 0.0, egui::Color32::from_gray(20));
        p.image(
            tex.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        match crop {
            Some([x, y, w, h]) => {
                let sel = egui::Rect::from_min_size(
                    rect.min + egui::vec2(x as f32 * escala, y as f32 * escala),
                    egui::vec2(w as f32 * escala, h as f32 * escala),
                );
                // Apagar TODO lo que no se usa: en un atlas de 40 tiles, saber
                // cuál es el tuyo es la mitad del trabajo.
                let velo = egui::Color32::from_rgba_unmultiplied(10, 10, 14, 170);
                for r in [
                    egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, sel.min.y)),
                    egui::Rect::from_min_max(egui::pos2(rect.min.x, sel.max.y), rect.max),
                    egui::Rect::from_min_max(
                        egui::pos2(rect.min.x, sel.min.y),
                        egui::pos2(sel.min.x, sel.max.y),
                    ),
                    egui::Rect::from_min_max(
                        egui::pos2(sel.max.x, sel.min.y),
                        egui::pos2(rect.max.x, sel.max.y),
                    ),
                ] {
                    if r.width() > 0.0 && r.height() > 0.0 {
                        p.rect_filled(r, 0.0, velo);
                    }
                }
                p.rect_stroke(sel, 0.0, egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 220, 60)));
            }
            None => {
                p.rect_stroke(
                    rect,
                    0.0,
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 220, 60)),
                );
            }
        }
        p.rect_stroke(rect, 0.0, egui::Stroke::new(1.0, egui::Color32::from_gray(90)));
        let hoja = rel.rsplit('/').next().unwrap_or(&rel).to_string();
        ui.label(
            egui::RichText::new(format!(
                "{hoja} · {}×{} px · grilla {}×{}",
                img.x as u32, img.y as u32, grid.0, grid.1
            ))
            .weak()
            .small()
            .monospace(),
        );
        ui.label(
            egui::RichText::new(match crop {
                Some([x, y, w, h]) => format!("recorte: {w}×{h} px en ({x}, {y})"),
                None => "recorte: la hoja entera".to_string(),
            })
            .weak()
            .small()
            .monospace(),
        );
        if resp.clicked() {
            self.load_candidate_rel(ctx, &rel);
            self.status = format!("{hoja} cargada como candidato — recortala abajo");
        }
        resp.on_hover_text("clic = cargar esta hoja abajo para recortarla de nuevo");
    }

    /// Los datos que el juego va a leer de este slot, en una tabla. Nada de
    /// abrir cinco desplegables para saber si algo frena o si emite luz.
    fn ficha_datos(&mut self, ui: &mut egui::Ui, key: &str) {
        let (colision, frena) = self.resumen_colision(key);
        let capa = match self.manifest.slot_layer(key) {
            Some(ArtLayer::Floor) => "Suelo (nunca te tapa)",
            Some(ArtLayer::Basement) => "Subsuelo",
            Some(ArtLayer::Wall) => "Pared (Y-sort por su base)",
            Some(ArtLayer::Roof) => "Techo (se transparenta)",
            Some(ArtLayer::Light) => "Luz (glow)",
            Some(ArtLayer::Object) => "Objeto (Y-sort)",
            None => "sin etiquetar (= Objeto)",
        };
        let (cw, ch) = self.slot_cells_for_footprint(key);
        let anim = self
            .manifest
            .get(key)
            .and_then(|v| v.anim())
            .map(|a| format!("{} frames @ {:.1} fps", a.frames, a.fps));
        let poses = self.manifest.poses(key).map(|m| m.len()).unwrap_or(0);
        let hojas = self.manifest.sheets(key).len();
        let luz = self
            .manifest
            .emission(key)
            .filter(|e| !e.is_off())
            .map(|e| format!("radio {:.1} m", e.radius));
        let inter = self.manifest.interaccion(key).map(|i| {
            let zona = if i.zona.is_empty() {
                "todo el asset".to_string()
            } else {
                format!("{} sub-celdas", i.zona.len())
            };
            if i.requiere.is_empty() {
                format!("{} (desde {zona})", i.verbo)
            } else {
                format!("{} (desde {zona}, {} requisito/s)", i.verbo, i.requiere.len())
            }
        });
        egui::Grid::new("ficha_datos").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
            let fila = |ui: &mut egui::Ui, k: &str, v: String, color: Option<egui::Color32>| {
                ui.label(egui::RichText::new(k).weak().small());
                let mut t = egui::RichText::new(v).small();
                if let Some(c) = color {
                    t = t.color(c);
                }
                ui.label(t);
                ui.end_row();
            };
            fila(ui, "clave", key.to_string(), None);
            fila(ui, "tamaño", format!("{cw}×{ch} tiles ({cw} m × {ch} m)"), None);
            fila(ui, "capa", capa.to_string(), None);
            fila(
                ui,
                "colisión",
                colision,
                Some(if frena {
                    egui::Color32::from_rgb(240, 120, 100)
                } else {
                    egui::Color32::from_rgb(140, 200, 140)
                }),
            );
            fila(ui, "animación", anim.unwrap_or_else(|| "quieto".into()), None);
            if poses > 0 {
                fila(ui, "poses", format!("{poses} ({hojas} hoja/s)"), None);
            }
            if let Some(l) = luz {
                fila(ui, "luz", l, Some(egui::Color32::from_rgb(250, 220, 130)));
            }
            if let Some(i) = inter {
                fila(ui, "interacción", i, Some(egui::Color32::from_rgb(120, 180, 245)));
            }
        });
    }

    /// La ficha entera: sprite del juego · hoja fuente · datos. Es lo que
    /// aparece al clickear el objeto en la lista, sin desplegar nada.
    fn ficha_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, slot: SlotDef) {
        let tiene_arte = self.manifest.get(slot.key).is_some();
        egui::Frame::group(ui.style())
            .fill(ui.visuals().faint_bg_color)
            .show(ui, |ui| {
                if !tiene_arte {
                    ui.label(
                        egui::RichText::new(
                            "Sin PNG: en el juego se dibuja con el fallback procedural. Elegí \
                             una hoja a la derecha y recortala abajo para asignarle arte.",
                        )
                        .weak(),
                    );
                }
                ui.horizontal_top(|ui| {
                    // 1 — lo que verá el juego.
                    ui.allocate_ui_with_layout(
                        egui::vec2(250.0, 0.0),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            ui.label(egui::RichText::new("EN EL JUEGO").small().strong());
                            self.ficha_sprite(ui, slot.key, 210.0);
                            ui.horizontal_wrapped(|ui| {
                                ui.checkbox(&mut self.ficha_colision, "colisión")
                                    .on_hover_text(
                                        "rojo = frena · amarillo = escalón · azul = desde dónde \
                                         se interactúa. Es la huella que el juego va a usar.",
                                    );
                                ui.checkbox(&mut self.ficha_grilla, "grilla")
                                    .on_hover_text(
                                        "1 cuadro = 1 tile = 1 metro. El borde celeste es la \
                                         celda ancla (donde cae el cursor al plantarlo).",
                                    );
                            });
                        },
                    );
                    ui.separator();
                    // 2 — de dónde sale ese sprite.
                    ui.allocate_ui_with_layout(
                        egui::vec2(280.0, 0.0),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            ui.label(egui::RichText::new("HOJA FUENTE (ATLAS)").small().strong());
                            if tiene_arte {
                                self.ficha_atlas(ctx, ui, slot.key, 240.0);
                            } else {
                                ui.label(egui::RichText::new("—").weak());
                            }
                        },
                    );
                    ui.separator();
                    // 3 — los datos que lee el juego + el candidato.
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("DATOS DEL JUEGO").small().strong());
                        if tiene_arte {
                            self.ficha_datos(ui, slot.key);
                        }
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            if tiene_arte
                                && ui
                                    .button("✏ Dibujar colisión")
                                    .on_hover_text(
                                        "abre el lienzo grande de la huella sobre este mismo sprite",
                                    )
                                    .clicked()
                            {
                                self.fp_big = true;
                                self.fp_fit = true;
                                self.fp_brush = FpBrush::Solid;
                            }
                            if ui
                                .button("🖌 Image Lab")
                                .on_hover_text("retocar/componer el PNG y volver")
                                .clicked()
                            {
                                if let Some(v) = self.manifest.get(slot.key) {
                                    let png = PathBuf::from(&self.game_assets).join(v.png());
                                    self.lab.open_file_as_canvas(&png);
                                }
                                self.mode = EditorMode::Lab;
                            }
                        });
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("CANDIDATO (lo que estás por asignar)").small().strong());
                        match self.src_tex.as_ref() {
                            Some((_, tex)) => {
                                let sz = preview_size(tex, 120.0);
                                ui.add(egui::Image::new(egui::load::SizedTexture::new(
                                    tex.id(),
                                    sz,
                                )));
                            }
                            None => {
                                ui.label(egui::RichText::new("(elegí un PNG a la derecha)").weak().small());
                            }
                        }
                    });
                });
            });
    }

    /// Canvas de la hoja candidata: dibuja el PNG con grilla y deja seleccionar
    /// un rango de celdas arrastrando (snap a la grilla) → `self.sel_rect`.
    fn sheet_canvas(&mut self, ui: &mut egui::Ui, max_h: f32) {
        let Some((_, tex)) = self.src_tex.clone() else {
            ui.label(egui::RichText::new("elegí un PNG abajo para recortarlo").weak());
            return;
        };
        let img_px = tex.size_vec2();
        let grid = self.sheet_grid.max(1) as f32;
        let cols = (img_px.x / grid).ceil().max(1.0) as u32;
        let rows = (img_px.y / grid).ceil().max(1.0) as u32;
        self.sheet_cols = cols;
        self.sheet_rows = rows;
        let zoom = self.sheet_zoom.max(0.25);
        let cell_px = grid * zoom;
        let disp = egui::vec2(cols as f32 * cell_px, rows as f32 * cell_px);

        egui::ScrollArea::both().id_salt("canvas").max_height(max_h.max(160.0)).show(ui, |ui| {
            let (rect, resp) = ui.allocate_exact_size(disp, egui::Sense::click_and_drag());
            let painter = ui.painter_at(rect);
            painter.image(
                tex.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
            let gs = egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(120, 210, 255, 70));
            for c in 0..=cols {
                let x = rect.min.x + c as f32 * cell_px;
                painter.line_segment([egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)], gs);
            }
            for r in 0..=rows {
                let y = rect.min.y + r as f32 * cell_px;
                painter.line_segment([egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)], gs);
            }
            let cell_of = |p: egui::Pos2| -> (u32, u32) {
                let cx = ((p.x - rect.min.x) / cell_px).floor().clamp(0.0, (cols - 1) as f32) as u32;
                let cy = ((p.y - rect.min.y) / cell_px).floor().clamp(0.0, (rows - 1) as f32) as u32;
                (cx, cy)
            };
            if resp.drag_started() {
                if let Some(p) = resp.interact_pointer_pos() {
                    self.drag_start_cell = Some(cell_of(p));
                }
            }
            if resp.dragged() || resp.drag_stopped() {
                if let (Some((sx, sy)), Some(p)) = (self.drag_start_cell, resp.interact_pointer_pos()) {
                    let (ex, ey) = cell_of(p);
                    let col = sx.min(ex);
                    let row = sy.min(ey);
                    self.sel_rect = Some(CellRect { col, row, w: sx.max(ex) - col + 1, h: sy.max(ey) - row + 1 });
                }
            }
            if resp.clicked() {
                if let Some(p) = resp.interact_pointer_pos() {
                    let (c, r) = cell_of(p);
                    self.sel_rect = Some(CellRect { col: c, row: r, w: 1, h: 1 });
                }
            }
            if let Some(sr) = self.sel_rect {
                let sel = egui::Rect::from_min_size(
                    egui::pos2(rect.min.x + sr.col as f32 * cell_px, rect.min.y + sr.row as f32 * cell_px),
                    egui::vec2(sr.w as f32 * cell_px, sr.h as f32 * cell_px),
                );
                painter.rect_filled(sel, 0.0, egui::Color32::from_rgba_unmultiplied(255, 220, 60, 40));
                painter.rect_stroke(sel, 0.0, egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 220, 60)));
            }
        });
    }

    /// Persiste el manifest al disco YA (sin round-trip ni cambiar el status) —
    /// para que CADA guardado del slicer (recorte / animación / pose / autotile)
    /// quede en el archivo del juego al instante, no sólo en memoria. Así corrés
    /// `./build-y-publicar.sh` y se ve. Devuelve `true` si escribió.
    fn persist_manifest(&self) -> bool {
        let Ok(json) = self.manifest.to_json() else { return false };
        if let Some(parent) = PathBuf::from(&self.game_manifest).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&self.game_manifest, json).is_ok()
    }

    fn save_manifest(&mut self, ctx: &egui::Context) {
        match self.manifest.to_json() {
            Ok(json) => {
                if let Some(parent) = PathBuf::from(&self.game_manifest).parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match std::fs::write(&self.game_manifest, &json) {
                    Ok(()) => {
                        // Round-trip: releo del disco lo que acabo de escribir, en
                        // vez de confiar en el estado en memoria — así "guardado"
                        // significa "esto es literalmente lo que hay en el archivo",
                        // y refresco la textura del slot actual para verlo YA en
                        // el editor (no hace falta reabrir).
                        match std::fs::read_to_string(&self.game_manifest).ok().map(|s| ArtManifest::from_json(&s)) {
                            Some(Ok(m)) => {
                                self.manifest = m;
                                if let Some(key) = self.current_key() {
                                    self.tex = self.preview_bound(ctx, key);
                                }
                                self.status = format!(
                                    "✔ guardado y verificado en {} — rebuildeá el juego (./editor_assets play) para verlo ahí",
                                    self.game_manifest
                                );
                            }
                            _ => {
                                self.status = format!(
                                    "⚠ escribí {} pero no pude releerlo para verificar",
                                    self.game_manifest
                                );
                            }
                        }
                    }
                    Err(e) => self.status = format!("✘ no pude escribir {}: {e}", self.game_manifest),
                }
            }
            Err(e) => self.status = format!("serialize: {e}"),
        }
    }

    fn write_catalog(&mut self) {
        // Catálogo simple de los bindings actuales (para contexto de Claude).
        let mut md = String::from(
            "# CATÁLOGO de arte del juego (autogenerado)\n\n\
             Los slots con `#` son **variantes** de un objeto: el juego las pide como\n\
             `(objeto, variante)` y cae al objeto genérico si esa variante no tiene arte.\n\n\
             | slot | PNG | animado |\n|------|-----|---------|\n",
        );
        for (key, v) in &self.manifest.sprites {
            let a = v.anim().map(|a| format!("{}f@{}fps", a.frames, a.fps)).unwrap_or_else(|| "no".into());
            md.push_str(&format!("| `{}` | {} | {} |\n", key, v.png(), a));
        }
        // Tabla de objetos con variantes: es la que le dice al juego/IA qué
        // materiales existen para cada objeto (piso, pared…) y cómo se llaman.
        if !self.manifest.objects.is_empty() {
            md.push_str("\n## Objetos con variantes\n\n| objeto | variante | nombre | asset |\n|--------|----------|--------|-------|\n");
            for (base, obj) in &self.manifest.objects {
                for v in &obj.variants {
                    let vk = ha_assets::variant_key(base, &v.id);
                    let png = self
                        .manifest
                        .get(&vk)
                        .map(|e| e.png().to_string())
                        .unwrap_or_else(|| "— (cae al genérico)".into());
                    md.push_str(&format!("| `{base}` | `{}` | {} | {png} |\n", v.id, v.display()));
                }
            }
        }
        if let Some(parent) = PathBuf::from(&self.catalog_path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::write(&self.catalog_path, md) {
            Ok(()) => self.status = format!("catálogo → {}", self.catalog_path),
            Err(e) => self.status = format!("write catálogo: {e}"),
        }
    }

    // ── helpers ─────────────────────────────────────────────────────────────

    fn abs_art_root(&self) -> PathBuf {
        PathBuf::from(&self.art_root)
    }

    fn preview_bound(&self, ctx: &egui::Context, key: &str) -> Option<(String, egui::TextureHandle)> {
        let v = self.manifest.get(key)?;
        let path = PathBuf::from(&self.game_assets).join(v.png());
        load_texture(ctx, &path, key)
    }

    /// Textura de una hoja YA copiada al juego, por su ruta relativa (`art/…png`),
    /// cacheada. La necesitan las poses multi-hoja: cada una puede venir de un PNG
    /// distinto, así que no alcanza con la textura del slot.
    fn tex_for(&mut self, ctx: &egui::Context, rel: &str) -> Option<egui::TextureHandle> {
        if let Some(t) = self.tex_cache.get(rel) {
            return Some(t.clone());
        }
        let path = PathBuf::from(&self.game_assets).join(rel);
        let (_, tex) = load_texture(ctx, &path, rel)?;
        self.tex_cache.insert(rel.to_string(), tex.clone());
        Some(tex)
    }

    // (`sprite_alpha_cells` se borró: hacía lo mismo que `huella_desde_alfa`
    // pero mirando sólo `rect`, así que en los slots con POSES perfilaba la hoja
    // entera en vez del recorte. Una sola lectura del alfa, la que entiende
    // recorte, tira y pose.)

    /// Deja como candidato del lienzo una hoja que YA está en el árbol del juego
    /// (ruta relativa) — para editar una pose que vino de otro PNG.
    fn load_candidate_rel(&mut self, ctx: &egui::Context, rel: &str) {
        let path = PathBuf::from(&self.game_assets).join(rel);
        if let Some(t) = load_texture(ctx, &path, "src_preview") {
            self.src_tex = Some(t);
            self.selected_png = Some(path);
        }
    }
}

impl eframe::App for EditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Gancho de QA: `HA_SLOT=decor/tree` abre el editor con ese objeto ya
        // elegido, para poder mirar (y capturar) una pantalla concreta sin
        // navegar la lista a mano. Mismo espíritu que `HA_FLYTEST` en el juego.
        if self.selected.is_none() {
            if let Ok(pedido) = std::env::var("HA_SLOT") {
                if let Some(i) = self.slots.iter().position(|s| s.key.contains(pedido.trim())) {
                    self.select_slot(ctx, i);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
                    // Unos frames trayendo la huella a la vista: el panel vive
                    // abajo del todo y una captura no puede scrollear sola.
                    self.fp_scroll_qa = 45;
                }
            }
        }
        EditorApp::ui(self, ctx);
    }
}

/// Grilla 3×3 del picker de tiles/autotile — el layout visual replica la
/// posición de la tierra/vecino respecto al agua (centro = agua sola).
const TILE_DIRS: [(&str, &str); 9] = [
    ("nw", "↖"), ("n", "↑"), ("ne", "↗"),
    ("w", "←"), ("c", "●"), ("e", "→"),
    ("sw", "↙"), ("s", "↓"), ("se", "↘"),
];

/// Nombre de forma del picker de tiles = el que lee `world2d::autotile_shape`:
/// centro / borde (`edge_*`) / esquina convexa (`corner_*`) o cóncava (`inner_*`).
/// Recetas listas de luz — el 90% de los casos, sin tocar sliders.
const LIGHT_PRESETS: &[(&str, Emission)] = &[
    ("🕯 vela", Emission { color: [1.0, 0.78, 0.42], radius: 2.2, intensity: 0.8, offset: [0.0, 0.0], flicker: 0.8, day: false }),
    ("🔥 antorcha", Emission { color: [1.0, 0.66, 0.25], radius: 4.5, intensity: 1.1, offset: [0.0, 0.0], flicker: 0.9, day: false }),
    ("🏮 farola", Emission { color: [1.0, 0.87, 0.6], radius: 8.0, intensity: 1.0, offset: [0.0, 0.0], flicker: 0.05, day: false }),
    ("💎 cristal", Emission { color: [0.5, 0.85, 1.0], radius: 3.5, intensity: 0.9, offset: [0.0, 0.0], flicker: 0.15, day: false }),
    ("🖥 pantalla", Emission { color: [0.55, 0.95, 0.85], radius: 2.6, intensity: 0.7, offset: [0.0, 0.0], flicker: 0.1, day: true }),
    ("🌋 lava", Emission { color: [1.0, 0.42, 0.12], radius: 6.0, intensity: 1.3, offset: [0.0, 0.0], flicker: 0.35, day: true }),
];

/// Dibuja el aura tal cual la verá el juego: anillos concéntricos con la misma
/// caída *smootherstep* que la textura procedural de `light2d`. No es una foto
/// del render, pero el tamaño relativo y la suavidad son los mismos.
fn light_preview(ui: &mut egui::Ui, em: &Emission) {
    let box_h = 120.0;
    let (resp, painter) = ui.allocate_painter(egui::vec2(ui.available_width().min(320.0), box_h), egui::Sense::hover());
    let rect = resp.rect;
    painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(10, 12, 20)); // "de noche"
    let c = rect.center();
    // 24 m de radio (el máximo del slider) ocupan el alto de la caja.
    let px_per_m = box_h / 24.0;
    let r_px = (em.radius * px_per_m).max(2.0);
    let [cr, cg, cb] = em.color;
    const RINGS: i32 = 22;
    for i in (0..RINGS).rev() {
        let t = 1.0 - i as f32 / RINGS as f32; // 0 en el borde, 1 en el centro
        let s = t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
        let a = (s * (0.4 + 0.6 * t) * em.intensity * 0.5).clamp(0.0, 1.0);
        painter.circle_filled(
            c,
            r_px * (1.0 - t),
            egui::Color32::from_rgba_unmultiplied(
                (cr * 255.0) as u8,
                (cg * 255.0) as u8,
                (cb * 255.0) as u8,
                (a * 255.0) as u8,
            ),
        );
    }
    painter.text(
        rect.left_bottom() + egui::vec2(6.0, -14.0),
        egui::Align2::LEFT_CENTER,
        format!("{:.1} m{}", em.radius, if em.day { " · dia y noche" } else { " · nocturna" }),
        egui::FontId::proportional(11.0),
        egui::Color32::from_gray(150),
    );
}

/// Nombres que le tocan a las `rows` primeras filas del atlas, en el orden que el
/// atlas las dispone. Es la única regla del auto-slice — el resto es aritmética
/// de rectángulos.
fn bulk_names(
    actions: &[(String, String, String)],
    dirs: &'static [(&'static str, &'static str)],
    order: BulkOrder,
    rows: usize,
) -> Vec<String> {
    let mut out = Vec::new();
    match order {
        BulkOrder::ByAction => {
            for (action, _, _) in actions {
                for (d, _) in dirs {
                    out.push(pose_name_of(action, d));
                }
            }
        }
        BulkOrder::ByDirection => {
            for (d, _) in dirs {
                for (action, _, _) in actions {
                    out.push(pose_name_of(action, d));
                }
            }
        }
    }
    out.truncate(rows);
    out
}

/// Un nombre de acción válido para el manifest: minúsculas y `_` (el juego arma
/// las poses como `accion_direccion`, así que un espacio o un guión rompería el
/// parseo del sufijo).
fn sanitize_action(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

/// Cómo se llama esa pieza en castellano — lo que ves en la casilla del picker.
/// El nombre técnico que guarda (`edge_n`, `corner_ne`…) va en el tooltip.
fn tile_shape_label(id: &str, inner: bool) -> &'static str {
    match (id, inner) {
        ("c", _) => "relleno",
        ("n", _) => "borde arriba",
        ("s", _) => "borde abajo",
        ("e", _) => "borde derecha",
        ("w", _) => "borde izq.",
        ("nw", false) => "esq. sup-izq",
        ("ne", false) => "esq. sup-der",
        ("sw", false) => "esq. inf-izq",
        ("se", false) => "esq. inf-der",
        ("nw", true) => "cóncava sup-izq",
        ("ne", true) => "cóncava sup-der",
        ("sw", true) => "cóncava inf-izq",
        _ => "cóncava inf-der",
    }
}

fn tile_shape_name(id: &str, inner: bool) -> String {
    match id {
        "c" => "center".to_string(),
        "n" | "s" | "e" | "w" => format!("edge_{id}"),
        _ if inner => format!("inner_{id}"),
        _ => format!("corner_{id}"),
    }
}

/// Poses que el juego busca para una pieza de CONSTRUCCIÓN por bordes
/// (`ha_shared::build_grid`): qué cara de la pared es cada recorte del atlas.
/// Vacío = el slot no es una pieza de construcción (no se muestra la sección).
///
/// - `front`: la pared del FONDO del cuarto (la que ves de frente, alta).
/// - `back`:  la de ADELANTE — se dibuja baja para ver el interior.
/// - `left` / `right`: las laterales (paredes verticales, de perfil).
fn build_part_poses(key: &str) -> &'static [(&'static str, &'static str)] {
    const WALL: &[(&str, &str)] = &[
        ("front", "🧱 pared de FRENTE (fondo del cuarto)"),
        ("back", "🧱 pared de ADELANTE (baja)"),
        ("left", "◧ pared IZQUIERDA (lateral)"),
        ("right", "◨ pared DERECHA (lateral)"),
    ];
    // ESTADOS de la puerta, en el orden en que ocurren. El juego los encadena:
    // cerrada → (abriéndose) → abierta → (cerrándose) → cerrada. Las dos
    // transiciones son OPCIONALES: si no las guardás, la puerta salta de un
    // estado al otro como siempre. `bloqueada` es un estado aparte, con su
    // propia animación en bucle (la cerradura que no cede).
    const DOOR: &[(&str, &str)] = &[
        ("closed", "🚪 CERRADA (reposo)"),
        ("opening", "▶ ABRIÉNDOSE (transición)"),
        ("open", "🚪 ABIERTA (reposo)"),
        ("closing", "◀ CERRÁNDOSE (transición)"),
        ("locked", "🔒 BLOQUEADA (no se abre)"),
        ("left", "◧ lateral izquierda"),
        ("right", "◨ lateral derecha"),
    ];
    if key.starts_with("door/") {
        DOOR
    } else if key.starts_with("wall/") || key.starts_with("window/") {
        WALL
    } else {
        &[]
    }
}

/// La línea de ayuda del lienzo. Es la misma en línea y en la ventana grande, y
/// dice los cuatro gestos que no se adivinan.
fn fp_hint() -> &'static str {
    "Ctrl+rueda = zoom · botón del medio o Shift+arrastre = mover · Ctrl+arrastre = borrar · \
     clic derecho = fijar el ancla (con la pluma, quitar vértice)"
}

/// Deja `cell` con el material pedido. Devuelve si algo cambió de verdad.
///
/// Sólido y escalón se excluyen (una celda frena o se trepa, no las dos); la
/// zona es un eje APARTE, así que puede convivir con cualquiera de los dos — el
/// frente de una heladera frena y se abre desde ahí.
fn fp_apply(ed: &mut FpEdit, cell: (u32, u32), brush: FpBrush, erase: bool) -> bool {
    if erase || brush == FpBrush::Erase {
        let a = ed.solid.remove(&cell);
        let b = ed.step.remove(&cell);
        let c = ed.zona.remove(&cell);
        return a || b || c;
    }
    match brush {
        FpBrush::Zona => ed.zona.insert(cell),
        FpBrush::Step => {
            ed.solid.remove(&cell);
            ed.step.insert(cell)
        }
        FpBrush::Solid | FpBrush::Erase => {
            ed.step.remove(&cell);
            ed.solid.insert(cell)
        }
    }
}

/// Qué hay en la celda MIRADO DESDE el material que estás pintando: es lo que el
/// balde considera "igual". Con 🟦 sólo importa si hay zona; con 🟥/🟨, si frena.
fn fp_state(ed: &FpEdit, cell: (u32, u32), brush: FpBrush) -> u8 {
    match brush {
        FpBrush::Zona => u8::from(ed.zona.contains(&cell)),
        _ => {
            if ed.solid.contains(&cell) {
                1
            } else if ed.step.contains(&cell) {
                2
            } else {
                0
            }
        }
    }
}

/// Celdas que toca un trazo recto entre dos sub-celdas. Sin esto, arrastrar
/// rápido deja el trazo lleno de agujeros: el ratón salta ocho celdas por frame.
fn line_cells(a: (i32, i32), b: (i32, i32)) -> Vec<(i32, i32)> {
    let pasos = (a.0 - b.0).abs().max((a.1 - b.1).abs()).max(1);
    (0..=pasos)
        .map(|i| {
            let t = i as f32 / pasos as f32;
            (
                (a.0 as f32 + (b.0 - a.0) as f32 * t).round() as i32,
                (a.1 as f32 + (b.1 - a.1) as f32 * t).round() as i32,
            )
        })
        .collect()
}

/// ¿El punto cae dentro del polígono? Regla par-impar (la de siempre): se cuenta
/// cuántos lados cruza un rayo horizontal hacia la derecha.
fn point_in_poly(p: (f32, f32), v: &[(f32, f32)]) -> bool {
    let mut dentro = false;
    let n = v.len();
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = v[i];
        let (xj, yj) = v[j];
        if (yi > p.1) != (yj > p.1) && p.0 < (xj - xi) * (p.1 - yi) / (yj - yi) + xi {
            dentro = !dentro;
        }
        j = i;
    }
    dentro
}

/// Las celdas que encierra la pluma: las de centro adentro MÁS las que pisa el
/// contorno. Sin el contorno, una figura fina (el borde de un muro de una sola
/// sub-celda) no encerraría ningún centro y no marcaría nada.
fn poly_cells(v: &[(f32, f32)], cols: u32, rows: u32) -> FpSet {
    let mut out = FpSet::default();
    if v.len() < 3 || cols == 0 || rows == 0 {
        return out;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for &(x, y) in v {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    let c0 = x0.floor().max(0.0) as u32;
    let r0 = y0.floor().max(0.0) as u32;
    let c1 = (x1.ceil().max(0.0) as u32).min(cols);
    let r1 = (y1.ceil().max(0.0) as u32).min(rows);
    for r in r0..r1 {
        for c in c0..c1 {
            if point_in_poly((c as f32 + 0.5, r as f32 + 0.5), v) {
                out.insert((c, r));
            }
        }
    }
    for i in 0..v.len() {
        let a = v[i];
        let b = v[(i + 1) % v.len()];
        for (c, r) in line_cells(
            (a.0.floor() as i32, a.1.floor() as i32),
            (b.0.floor() as i32, b.1.floor() as i32),
        ) {
            if c >= 0 && r >= 0 && (c as u32) < cols && (r as u32) < rows {
                out.insert((c as u32, r as u32));
            }
        }
    }
    out
}

/// Región contigua (4 vecinos) que comparte estado con la semilla — el balde.
fn flood_cells(seed: (u32, u32), cols: u32, rows: u32, igual: impl Fn((u32, u32)) -> bool) -> FpSet {
    let mut out = FpSet::default();
    if seed.0 >= cols || seed.1 >= rows || !igual(seed) {
        return out;
    }
    let mut pila = vec![seed];
    out.insert(seed);
    while let Some((c, r)) = pila.pop() {
        for (dc, dr) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
            let (nc, nr) = (c as i32 + dc, r as i32 + dr);
            if nc < 0 || nr < 0 || nc as u32 >= cols || nr as u32 >= rows {
                continue;
            }
            let n = (nc as u32, nr as u32);
            if igual(n) && out.insert(n) {
                pila.push(n);
            }
        }
    }
    out
}

/// El anillo de celdas LIBRES pegadas a algo sólido (8 vecinos). Es la zona de
/// interacción por defecto: "desde dónde alcanzo este objeto".
fn borde_de(solid: &FpSet, cols: u32, rows: u32) -> FpSet {
    let mut out = FpSet::default();
    for r in 0..rows {
        for c in 0..cols {
            if solid.contains(&(c, r)) {
                continue;
            }
            let pegado = (-1i32..=1).any(|dr| {
                (-1i32..=1).any(|dc| {
                    let (nc, nr) = (c as i32 + dc, r as i32 + dr);
                    nc >= 0 && nr >= 0 && solid.contains(&(nc as u32, nr as u32))
                })
            });
            if pegado {
                out.insert((c, r));
            }
        }
    }
    out
}

/// Reescala las sub-celdas sólidas de una huella al cambiar el DETALLE: subir de
/// resolución subdivide cada celda (no perdés lo pintado), bajar conserva el tile
/// si alguna de sus sub-celdas era sólida.
fn rescale_solid(
    solid: &std::collections::HashSet<(u32, u32)>,
    from: u32,
    to: u32,
) -> std::collections::HashSet<(u32, u32)> {
    let (from, to) = (from.max(1), to.max(1));
    if from == to {
        return solid.clone();
    }
    let mut out = std::collections::HashSet::new();
    for &(c, r) in solid {
        // Rango de sub-celdas destino que cubre esta celda origen.
        let (c0, c1) = (c * to / from, ((c + 1) * to).div_ceil(from));
        let (r0, r1) = (r * to / from, ((r + 1) * to).div_ceil(from));
        for rr in r0..r1.max(r0 + 1) {
            for cc in c0..c1.max(c0 + 1) {
                out.insert((cc, rr));
            }
        }
    }
    out
}

/// Reescala lo pintado cuando cambia el TAMAÑO de la grilla (no el detalle):
/// cada celda se estira a la posición proporcional que le toca en la grilla
/// nueva. Es lo que hace que corregir «esto era 1×1 y el sprite mide 5×4» no te
/// borre lo que ya habías pintado.
fn rescale_grid(set: &FpSet, from: (u32, u32), to: (u32, u32), res: u32) -> FpSet {
    let res = res.max(1);
    let (fc, fr) = (from.0.max(1) * res, from.1.max(1) * res);
    let (tc, tr) = (to.0.max(1) * res, to.1.max(1) * res);
    if (fc, fr) == (tc, tr) {
        return set.clone();
    }
    let mut out = FpSet::default();
    for &(c, r) in set {
        let (c0, c1) = (c * tc / fc, ((c + 1) * tc).div_ceil(fc));
        let (r0, r1) = (r * tr / fr, ((r + 1) * tr).div_ceil(fr));
        for rr in r0..r1.max(r0 + 1).min(tr) {
            for cc in c0..c1.max(c0 + 1).min(tc) {
                out.insert((cc, rr));
            }
        }
    }
    out
}

/// Escala la textura para que su lado mayor sea `max_px` (preserva aspecto), así
/// un preview nunca se come toda el área y la lista de abajo mantiene scroll.
fn preview_size(tex: &egui::TextureHandle, max_px: f32) -> egui::Vec2 {
    let s = tex.size_vec2();
    let m = s.x.max(s.y).max(1.0);
    s * (max_px / m)
}

/// Los slots derivados en runtime necesitan `&'static str` y se crean pocas
/// veces (abrir manifest / añadir slot), así que filtrar la cadena es aceptable.
/// FNV-1a de 32 bits sobre los bytes del PNG fuente — hash de contenido para
/// el dedupe de atlas (mismo archivo → mismo destino, sin dependencias extra).
fn fnv1a32(bytes: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for &b in bytes {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

fn sanitize_filename(s: &str) -> String {
    let out: String = s
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    if out.is_empty() {
        "atlas".to_string()
    } else {
        out
    }
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn load_texture(ctx: &egui::Context, path: &Path, id: &str) -> Option<(String, egui::TextureHandle)> {
    let img = image::open(path).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    let color = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw());
    let tex = ctx.load_texture(id, color, egui::TextureOptions::NEAREST);
    Some((id.to_string(), tex))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La ficha del asset promete "lo que ves acá es lo que vas a tener en el
    /// juego". Esta es esa promesa, escrita: el mismo orden de decisión que
    /// `ha-game::world2d::decor_blocks` (huella → casilla → default).
    #[test]
    fn la_ficha_dice_la_colision_que_va_a_haber_en_el_juego() {
        let mut app = EditorApp::new();
        app.manifest = ArtManifest::default();

        // Nada declarado: no frena. Un prop suelto no levanta un muro.
        let (txt, frena) = app.resumen_colision("prop/farol");
        assert!(!frena, "{txt}");

        // La casilla del editor manda sobre el default.
        app.manifest.set("prop/farol", "art/prop/farol.png".into(), None);
        app.manifest.set_collides("prop/farol", Some(true));
        let (txt, frena) = app.resumen_colision("prop/farol");
        assert!(frena, "{txt}");

        // Y la HUELLA pintada manda sobre la casilla: es más precisa.
        app.manifest.set_collides("prop/farol", Some(false));
        app.manifest.set_footprint(
            "prop/farol",
            Some(ha_assets::CollisionFootprint {
                w: 1,
                h: 2,
                ax: 0,
                az: 1,
                solid: vec![(1, 6), (2, 6)],
                step: Vec::new(),
                res: 4,
            }),
        );
        let (txt, frena) = app.resumen_colision("prop/farol");
        assert!(frena, "{txt}");
        assert!(txt.contains("huella"), "y dice de dónde sale: {txt}");

        // Default por CAPA: una pared frena aunque no le pintes nada.
        app.manifest.set("wall/prueba", "art/wall/prueba.png".into(), None);
        app.manifest.set_slot_layer("wall/prueba", Some(ArtLayer::Wall));
        let (txt, frena) = app.resumen_colision("wall/prueba");
        assert!(frena, "{txt}");

        // Default por TIPO: el tronco del árbol ocupa su celda.
        app.manifest.set("decor/tree_roble", "art/decor/tree.png".into(), None);
        let (txt, frena) = app.resumen_colision("decor/tree_roble");
        assert!(frena, "{txt}");
    }

    #[test]
    fn pose_names_are_deduced_from_file_names() {
        // El caso del pack: un PNG por dirección.
        assert_eq!(infer_pose_name("Body_Idle_Left"), "idle_left");
        assert_eq!(infer_pose_name("cuerpo_idle_down"), "idle_down");
        // La diagonal gana sobre la cardinal que la contiene.
        assert_eq!(infer_pose_name("hero_walk_down_left_01"), "walk_down_left");
        // Acción sin dirección (dormir, sentarse).
        assert_eq!(infer_pose_name("npc_sleep"), "sleep");
        // Sólo dirección → se asume quieto.
        assert_eq!(infer_pose_name("front_down"), "idle_down");
        // Nada reconocible → nombre limpio, para corregirlo en la tabla.
        assert_eq!(infer_pose_name("Mi Sprite Raro!!"), "mi_sprite_raro");
    }

    #[test]
    fn strips_are_detected_from_dimensions() {
        // Tira horizontal: 4 frames de 32×32.
        assert_eq!(detect_strip(128, 32), ((32, 32), 4, StripDir::Right));
        // Tira vertical: 3 frames de 16×16.
        assert_eq!(detect_strip(16, 48), ((16, 16), 3, StripDir::Down));
        // Imagen suelta (no divide) → un frame del tamaño entero.
        assert_eq!(detect_strip(32, 48), ((32, 48), 1, StripDir::Right));
        assert_eq!(detect_strip(40, 40), ((40, 40), 1, StripDir::Right));
    }

    #[test]
    fn bulk_slice_names_follow_the_atlas_order() {
        let actions: Vec<(String, String, String)> = ["idle", "walk"]
            .iter()
            .map(|a| (a.to_string(), a.to_string(), "movimiento".to_string()))
            .collect();
        let dirs = DirMode::Four.dirs();
        // Orden clásico: una acción, sus 4 direcciones, la siguiente acción.
        let by_action = bulk_names(&actions, dirs, BulkOrder::ByAction, 6);
        assert_eq!(by_action[0], "idle_down");
        assert_eq!(by_action[3], "idle_right");
        assert_eq!(by_action[4], "walk_down");
        assert_eq!(by_action.len(), 6, "se corta en las filas pedidas");
        // Orden por dirección: todas las acciones de una dirección primero.
        let by_dir = bulk_names(&actions, dirs, BulkOrder::ByDirection, 4);
        assert_eq!(by_dir, ["idle_down", "walk_down", "idle_up", "walk_up"]);
    }

    /// La pluma tiene que RELLENAR, no dejar sólo el contorno: es lo que separa
    /// "dibujé la silueta de la casa" de "tengo que pintar 40 celdas a mano".
    #[test]
    fn la_pluma_rellena_lo_que_encierra() {
        // Un cuadrado de 4×4 sub-celdas dentro de una grilla de 8×8.
        let v = [(1.0, 1.0), (5.0, 1.0), (5.0, 5.0), (1.0, 5.0)];
        let dentro = poly_cells(&v, 8, 8);
        assert!(dentro.contains(&(2, 2)), "el interior se rellena");
        assert!(dentro.contains(&(1, 1)), "el contorno también");
        assert!(!dentro.contains(&(6, 6)), "y nada de afuera");
        assert!(!dentro.contains(&(0, 0)));
        // Una L: el hueco de la escuadra NO se rellena.
        let l = [(0.0, 0.0), (4.0, 0.0), (4.0, 1.0), (1.0, 1.0), (1.0, 4.0), (0.0, 4.0)];
        let ele = poly_cells(&l, 8, 8);
        assert!(ele.contains(&(0, 3)) && ele.contains(&(3, 0)), "los brazos sí");
        assert!(!ele.contains(&(3, 3)), "el hueco de la escuadra no");
        // Menos de tres vértices no es una figura.
        assert!(poly_cells(&[(0.0, 0.0), (2.0, 2.0)], 8, 8).is_empty());
    }

    /// Arrastrar rápido salta celdas: el trazo se interpola o queda punteado.
    #[test]
    fn el_trazo_libre_no_deja_agujeros() {
        let t = line_cells((0, 0), (4, 2));
        assert_eq!(t.first(), Some(&(0, 0)));
        assert_eq!(t.last(), Some(&(4, 2)));
        for par in t.windows(2) {
            let d = (par[1].0 - par[0].0).abs().max((par[1].1 - par[0].1).abs());
            assert!(d <= 1, "dos celdas seguidas del trazo tienen que tocarse");
        }
    }

    #[test]
    fn el_balde_llena_la_region_contigua_y_para_en_el_borde() {
        // Pared vertical en la columna 2: el balde a la izquierda no la cruza.
        let pared: FpSet = (0..4).map(|r| (2u32, r)).collect();
        let izq = flood_cells((0, 0), 4, 4, |c| !pared.contains(&c));
        assert!(izq.contains(&(1, 3)));
        assert!(!izq.contains(&(3, 0)), "la pared corta la región");
        assert_eq!(izq.len(), 8);
    }

    /// La zona de interacción por defecto es el anillo de alrededor: "desde
    /// dónde alcanzo esto".
    #[test]
    fn el_borde_rodea_lo_solido_sin_pisarlo() {
        let solid: FpSet = [(1u32, 1u32)].into_iter().collect();
        let borde = borde_de(&solid, 3, 3);
        assert!(!borde.contains(&(1, 1)), "lo sólido no es zona");
        assert_eq!(borde.len(), 8, "las ocho de alrededor");
        assert!(borde_de(&FpSet::default(), 3, 3).is_empty(), "sin sólido no hay borde");
    }

    /// Sólido y escalón se excluyen; la zona convive con los dos y la goma
    /// limpia todo — incluida la zona, que antes se quedaba pegada.
    #[test]
    fn los_materiales_no_se_pisan_y_la_goma_limpia_todo() {
        let mut ed = FpEdit {
            w: 2,
            h: 2,
            res: 1,
            ax: 0,
            az: 0,
            solid: FpSet::default(),
            step: FpSet::default(),
            zona: FpSet::default(),
        };
        assert!(fp_apply(&mut ed, (0, 0), FpBrush::Solid, false));
        assert!(fp_apply(&mut ed, (0, 0), FpBrush::Step, false));
        assert!(ed.step.contains(&(0, 0)) && !ed.solid.contains(&(0, 0)));
        // La zona es un eje aparte: puede estar con el escalón a la vez.
        fp_apply(&mut ed, (0, 0), FpBrush::Zona, false);
        assert!(ed.zona.contains(&(0, 0)) && ed.step.contains(&(0, 0)));
        assert!(fp_apply(&mut ed, (0, 0), FpBrush::Erase, false));
        assert!(ed.zona.is_empty() && ed.step.is_empty() && ed.solid.is_empty());
        assert!(!fp_apply(&mut ed, (0, 0), FpBrush::Erase, false), "borrar el vacío no cambia nada");
    }

    /// Un lienzo donde arrastrar no pinta es lo que hace que el editor se sienta
    /// roto, así que el gesto se prueba de verdad: eventos de puntero contra
    /// `fp_canvas`, no sólo las funciones puras de abajo.
    #[test]
    fn arrastrar_por_el_lienzo_pinta_las_celdas_del_trazo() {
        let mut app = EditorApp::new();
        app.fp_fit = false;
        app.fp_zoom = 40.0;
        app.fp_pan = egui::Vec2::ZERO;
        app.fp_tool = FpTool::Libre;
        app.fp_brush = FpBrush::Solid;
        let mut ed = FpEdit {
            w: 4,
            h: 4,
            res: 1,
            ax: 0,
            az: 0,
            solid: FpSet::default(),
            step: FpSet::default(),
            zona: FpSet::default(),
        };
        // Lienzo de 400×400 pegado al (0,0) de la pantalla: la grilla de 4×4 a
        // 40 px queda centrada, o sea de (120,120) a (280,280).
        let vista = egui::vec2(400.0, 400.0);
        let centro = |c: f32, r: f32| egui::pos2(120.0 + c * 40.0 + 20.0, 120.0 + r * 40.0 + 20.0);
        let ctx = egui::Context::default();
        let pantalla = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 400.0));
        let frame = |eventos: Vec<egui::Event>, app: &mut EditorApp, ed: &mut FpEdit| {
            let entrada = egui::RawInput {
                screen_rect: Some(pantalla),
                events: eventos,
                ..Default::default()
            };
            let _ = ctx.run(entrada, |ctx| {
                egui::Area::new(egui::Id::new("lienzo_test")).fixed_pos(egui::pos2(0.0, 0.0)).show(
                    ctx,
                    |ui| {
                        app.fp_canvas(ui, ed, None, vista);
                    },
                );
            });
        };
        // egui hace el hit-test con los rects del frame ANTERIOR, así que cada
        // gesto necesita un par de frames en blanco para asentarse (en la app
        // real eso pasa solo, a 60 por segundo).
        let paso = |eventos: Vec<egui::Event>, app: &mut EditorApp, ed: &mut FpEdit| {
            frame(eventos, app, ed);
            frame(Vec::new(), app, ed);
            frame(Vec::new(), app, ed);
        };
        let boton = |pos: egui::Pos2, pressed: bool| {
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::default(),
            }]
        };
        let p0 = centro(1.0, 1.0);
        let p1 = centro(3.0, 1.0);
        paso(vec![egui::Event::PointerMoved(p0)], &mut app, &mut ed);
        paso(boton(p0, true), &mut app, &mut ed);
        paso(vec![egui::Event::PointerMoved(p1)], &mut app, &mut ed);
        paso(boton(p1, false), &mut app, &mut ed);
        assert!(ed.solid.contains(&(1, 1)), "pinta donde apretaste");
        assert!(ed.solid.contains(&(3, 1)), "y donde soltaste");
        assert!(ed.solid.contains(&(2, 1)), "y la celda del medio: el trazo se interpola");
        assert!(!ed.solid.contains(&(1, 2)), "sin manchar la fila de al lado");

        // Y la ZONA de interacción se pinta igual que lo demás: era el pincel
        // que no dibujaba nada porque ni se guardaba ni se veía.
        app.fp_brush = FpBrush::Zona;
        let pz = centro(0.0, 3.0);
        paso(vec![egui::Event::PointerMoved(pz)], &mut app, &mut ed);
        paso(boton(pz, true), &mut app, &mut ed);
        paso(boton(pz, false), &mut app, &mut ed);
        assert!(ed.zona.contains(&(0, 3)), "la zona se pinta");
        assert!(!ed.solid.contains(&(0, 3)), "y no frena a nadie");

        // La goma limpia las dos capas en la misma pasada.
        app.fp_brush = FpBrush::Erase;
        paso(boton(pz, true), &mut app, &mut ed);
        paso(boton(pz, false), &mut app, &mut ed);
        assert!(!ed.zona.contains(&(0, 3)), "la goma también borra la zona");
    }

    /// Perfilar al sprite a un sub-celda por tile es imposible por definición:
    /// la unidad de pintura es el cuadrado entero. Por eso una huella nueva abre
    /// en ¼ y estirar la grilla conserva lo pintado.
    #[test]
    fn el_detalle_fino_es_lo_que_permite_seguir_la_silueta() {
        // Grilla de 1×1 con la celda pintada → grilla de 2×2: se estira, no se
        // pierde ni se queda en la esquina.
        let una: FpSet = [(0u32, 0u32)].into_iter().collect();
        let estirada = rescale_grid(&una, (1, 1), (2, 2), 1);
        assert_eq!(estirada.len(), 4, "la celda cubría todo el sprite y lo sigue cubriendo");
        // Con detalle ¼ la misma celda son 16 sub-celdas: recién ahí hay silueta.
        let fina = rescale_solid(&una, 1, 4);
        assert_eq!(fina.len(), 16);
        // Y media grilla no invade la otra mitad.
        let izq: FpSet = [(0u32, 0u32)].into_iter().collect();
        let en_dos = rescale_grid(&izq, (2, 1), (4, 1), 1);
        assert!(en_dos.contains(&(0, 0)) && en_dos.contains(&(1, 0)));
        assert!(!en_dos.contains(&(2, 0)), "lo de la izquierda se queda a la izquierda");
    }

    /// El lienzo se dibuja de verdad (en línea y en la ventana grande) sin
    /// romperse, y dibujarlo sin tocar nada NO ensucia el manifest: el guardado
    /// tiene que salir de un gesto del usuario, no de que la pantalla exista.
    #[test]
    fn el_lienzo_de_la_huella_se_dibuja_sin_tocar_el_disco() {
        let mut app = EditorApp::new();
        // Por las dudas: si algo marcara cambios, que escriba en /tmp y no en el
        // manifest del juego.
        app.game_manifest =
            std::env::temp_dir().join("ha_huella_smoke.json").display().to_string();
        let slot = app.slots[0];
        for grande in [false, true] {
            app.fp_big = grande;
            egui::__run_test_ctx(|ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    app.footprint_panel(ui, slot);
                });
            });
        }
        assert!(!app.fp_dirty, "dibujar el panel no cambia nada por sí solo");
        assert!(app.fp_zoom >= FP_ZOOM_MIN && app.fp_zoom <= FP_ZOOM_MAX, "el zoom queda en rango");
        assert!(!std::path::Path::new(&app.game_manifest).exists(), "no escribió nada");
    }

    #[test]
    fn poses_without_direction_keep_the_bare_action_name() {
        assert_eq!(pose_name_of("sleep", ""), "sleep");
        assert_eq!(pose_name_of("walk", "up_left"), "walk_up_left");
        assert_eq!(sanitize_action("  Regatear!  "), "regatear");
    }
}
