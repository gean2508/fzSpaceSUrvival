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
    /// Deja la celda libre.
    Erase,
}

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
    /// Píxeles por tile del lienzo de la huella. Es la barra de zoom que arregla
    /// el cuadradito de 40 px en el que era imposible dibujar.
    fp_zoom: f32,
    tile_inner_corner: bool,          // esquina cóncava (interior) vs convexa
    anim_preview_frame: usize,        // frame en curso del preview del CANDIDATO
    anim_preview_t: f32,              // acumulador de tiempo del preview del candidato
    bound_preview_frame: usize,       // frame en curso del preview del binding ACTUAL
    bound_preview_t: f32,             // acumulador de tiempo del preview actual

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
            fp_zoom: 72.0,
            tile_inner_corner: false,
            anim_preview_frame: 0,
            anim_preview_t: 0.0,
            bound_preview_frame: 0,
            bound_preview_t: 0.0,
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

            // Binding actual + preview.
            match self.manifest.get(slot.key) {
                Some(v) => {
                    ui.label(format!("PNG actual: {}", v.png()));
                    if let Some(a) = v.anim() {
                        ui.label(format!("animado: {} frames @ {} fps", a.frames, a.fps));
                    }
                }
                None => {
                    ui.label(egui::RichText::new("sin PNG (usa arte procedural en el juego)").weak());
                }
            }
            // Previews lado a lado (colapsable). "actual" muestra lo que verá el
            // juego: con poses, una representativa ANIMADA (idle_down o la 1ª) —
            // así confirmás al instante que tus poses quedaron, no la hoja cruda.
            egui::CollapsingHeader::new("👁 vista previa (actual / candidato)")
                .default_open(true)
                .show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("actual (juego)").weak());
                            self.bound_preview(ui, slot, 220.0);
                        });
                        ui.separator();
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("candidato").weak());
                            match self.src_tex.as_ref() {
                                Some((_, tex)) => {
                                    let sz = preview_size(tex, 220.0);
                                    ui.add(egui::Image::new(egui::load::SizedTexture::new(tex.id(), sz)));
                                }
                                None => { ui.label(egui::RichText::new("(elegí un PNG abajo)").weak()); }
                            }
                        });
                    });
                });
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

    /// 🗺 **La huella de colisión, pintada sobre el sprite.**
    ///
    /// Reescrito de arriba abajo. Lo de antes era la peor pantalla del programa
    /// por tres motivos concretos, y los tres están arreglados acá:
    ///
    /// 1. **El lienzo salía de 40 px por tile.** Un objeto de 1×1 se pintaba en
    ///    un cuadradito de 40×40: imposible saber dónde estabas dibujando. Ahora
    ///    el lienzo se dimensiona SOLO para ocupar un tamaño cómodo (`fp_zoom`
    ///    arranca calculado y hay una barra para agrandarlo).
    /// 2. **El fondo era la hoja entera.** Si el asset se recortó de un atlas y
    ///    su arte vive en poses, se veía el atlas completo detrás de la grilla,
    ///    no el objeto. Ahora el fondo es EL RECORTE que va a ver el juego
    ///    (`footprint_backdrop`): recorte estático, primer frame de la tira o la
    ///    pose representativa.
    /// 3. **Pintar era un secreto.** "Ctrl+arrastrá para borrar" no lo descubre
    ///    nadie. Ahora hay tres botones de PINCEL a la vista.
    fn footprint_panel(&mut self, ui: &mut egui::Ui, slot: SlotDef) {
        let cur = self.manifest.footprint(slot.key).cloned().unwrap_or_default();
        let title = if cur.is_empty() && cur.step.is_empty() {
            "🗺 huella de colisión — pintala sobre el sprite".to_string()
        } else {
            format!(
                "🗺 huella de colisión — {} sólidas · {} escalón",
                cur.solid.len(),
                cur.step.len()
            )
        };
        egui::CollapsingHeader::new(title).default_open(true).show(ui, |ui| {
            // Dimensiones. El default sale del tamaño REAL del asset en celdas
            // (recorte, tira o pose), no de 1×1: una casa de 5×4 abre su grilla
            // de 5×4 sin que tengas que adivinarla.
            let def = self.slot_cells_for_footprint(slot.key);
            let mut w = if cur.w == 0 { def.0 } else { cur.w.clamp(1, 12) };
            let mut h = if cur.h == 0 { def.1 } else { cur.h.clamp(1, 12) };
            let old_res = cur.res();
            let mut res = old_res;
            let mut solid: std::collections::HashSet<(u32, u32)> = cur.solid.iter().copied().collect();
            let mut step: std::collections::HashSet<(u32, u32)> = cur.step.iter().copied().collect();
            let mut ax = cur.ax;
            let mut az = cur.az;
            let mut changed = false;

            // ── Pincel ────────────────────────────────────────────────────────
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("pincel:").strong());
                for (b, label, hint) in [
                    (
                        FpBrush::Solid,
                        "🟥 sólido",
                        "Frena el paso. El tronco de un árbol, el muro de una casa, la roca.",
                    ),
                    (
                        FpBrush::Step,
                        "🟨 escalón",
                        "Desnivel que se SUBE y se BAJA a pie: no frena a quien camina, pero \
                         queda marcado como borde para lo que no puede treparlo (vehículos). \
                         Se sube y se baja por el mismo sitio a propósito — un escalón de una \
                         sola dirección te dejaría encerrado abajo.",
                    ),
                    (
                        FpBrush::Erase,
                        "🧽 borrar",
                        "Deja la celda libre. (También podés borrar con Ctrl+arrastre \
                         estando en cualquier pincel.)",
                    ),
                ] {
                    if ui
                        .selectable_label(self.fp_brush == b, label)
                        .on_hover_text(hint)
                        .clicked()
                    {
                        self.fp_brush = b;
                    }
                }
                ui.separator();
                ui.label("ancla").on_hover_text(
                    "La celda que queda bajo el cursor al colocar el objeto (borde cyan). \
                     Se fija con CLIC DERECHO sobre el lienzo.",
                );
            });
            ui.label(
                egui::RichText::new(
                    "Arrastrá sobre el sprite para pintar. Clic derecho = fijar el ancla. \
                     Vacío = colisión de una sola celda (la casilla de arriba).",
                )
                .weak()
                .small(),
            );

            // ── Tamaño y detalle ──────────────────────────────────────────────
            ui.horizontal_wrapped(|ui| {
                ui.label("ancho");
                changed |= ui.add(egui::DragValue::new(&mut w).range(1..=12)).changed();
                ui.label("alto");
                changed |= ui.add(egui::DragValue::new(&mut h).range(1..=12)).changed();
                ui.separator();
                ui.label("detalle").on_hover_text(
                    "Sub-celdas por tile. ¼ perfila la silueta: un poste delgado deja de \
                     bloquear el metro entero.",
                );
                for (v, name) in [(1u32, "tile"), (2, "½"), (4, "¼")] {
                    ui.selectable_value(&mut res, v, name);
                }
                ui.separator();
                ui.label("zoom");
                ui.add(egui::Slider::new(&mut self.fp_zoom, 16.0..=160.0).show_value(false))
                    .on_hover_text("agranda el lienzo — el problema del cuadradito de 40 px");
            });
            // Cambiar el detalle conserva lo pintado (se re-escala).
            if res != old_res {
                solid = rescale_solid(&solid, old_res, res);
                step = rescale_solid(&step, old_res, res);
                changed = true;
            }

            // ── Perfilado automático desde los píxeles ────────────────────────
            let mut profile: Option<bool> = None;
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button("⟟ perfilar al sprite")
                    .on_hover_text("marca sólido todo lo que no es transparente")
                    .clicked()
                {
                    profile = Some(false);
                }
                if ui
                    .button("⟟ perfilar la base")
                    .on_hover_text("sólo la fila de tiles del pie: el tronco frena, la copa se atraviesa")
                    .clicked()
                {
                    profile = Some(true);
                }
                if ui.button("🧹 limpiar todo").clicked() {
                    solid.clear();
                    step.clear();
                    changed = true;
                }
            });
            if let Some(only_base) = profile {
                match self.sprite_alpha_cells(slot.key, w, h, res) {
                    Some(cov) => {
                        let base_from = (h * res).saturating_sub(res);
                        solid = cov
                            .into_iter()
                            .filter(|&((_, r), f)| f >= 0.2 && !(only_base && r < base_from))
                            .map(|(cell, _)| cell)
                            .collect();
                        step.retain(|c| !solid.contains(c));
                        self.status = format!(
                            "huella de {} perfilada al sprite: {} sub-celdas",
                            slot.key,
                            solid.len()
                        );
                        changed = true;
                    }
                    None => self.status = "no pude leer los píxeles del sprite (¿PNG asignado?)".into(),
                }
            }

            // Reajustar a las dimensiones nuevas.
            solid.retain(|&(c, r)| c < w * res && r < h * res);
            step.retain(|&(c, r)| c < w * res && r < h * res);
            ax = ax.min(w.saturating_sub(1));
            az = az.min(h.saturating_sub(1));

            // ── Lienzo ────────────────────────────────────────────────────────
            let backdrop = self.footprint_backdrop(ui.ctx(), slot.key);
            // El tile se dibuja a `fp_zoom` px, con un techo para no desbordar el
            // panel: el lienzo crece hasta donde entra y ahí se queda.
            let max_w = ui.available_width().max(240.0) - 8.0;
            let tile_px = self.fp_zoom.min(max_w / w as f32).max(12.0);
            let sub_px = tile_px / res as f32;
            let (canvas, painter) = ui.allocate_painter(
                egui::vec2(w as f32 * tile_px, h as f32 * tile_px),
                egui::Sense::click_and_drag(),
            );
            let origin = canvas.rect.min;
            // Damero de fondo: sin él, una zona transparente del sprite y una
            // zona sin pintar se ven exactamente igual.
            for r in 0..h {
                for c in 0..w {
                    if (c + r) % 2 == 0 {
                        continue;
                    }
                    painter.rect_filled(
                        egui::Rect::from_min_size(
                            origin + egui::vec2(c as f32 * tile_px, r as f32 * tile_px),
                            egui::vec2(tile_px, tile_px),
                        ),
                        0.0,
                        egui::Color32::from_gray(48),
                    );
                }
            }
            if let Some((id, uv)) = backdrop {
                painter.image(id, canvas.rect, uv, egui::Color32::WHITE);
            }
            let paint_cells = |set: &std::collections::HashSet<(u32, u32)>, color: egui::Color32| {
                for &(c, r) in set {
                    painter.rect_filled(
                        egui::Rect::from_min_size(
                            origin + egui::vec2(c as f32 * sub_px, r as f32 * sub_px),
                            egui::vec2(sub_px, sub_px),
                        ),
                        0.0,
                        color,
                    );
                }
            };
            paint_cells(&solid, egui::Color32::from_rgba_unmultiplied(210, 50, 40, 120));
            paint_cells(&step, egui::Color32::from_rgba_unmultiplied(235, 190, 40, 120));
            // Grilla: la fina tenue, la del TILE marcada (es un metro del mundo).
            if res > 1 {
                let fine = egui::Stroke::new(0.5, egui::Color32::from_gray(80));
                for i in 1..w * res {
                    let x = origin.x + i as f32 * sub_px;
                    painter.line_segment([egui::pos2(x, origin.y), egui::pos2(x, canvas.rect.max.y)], fine);
                }
                for i in 1..h * res {
                    let y = origin.y + i as f32 * sub_px;
                    painter.line_segment([egui::pos2(origin.x, y), egui::pos2(canvas.rect.max.x, y)], fine);
                }
            }
            for r in 0..h {
                for c in 0..w {
                    let cr = egui::Rect::from_min_size(
                        origin + egui::vec2(c as f32 * tile_px, r as f32 * tile_px),
                        egui::vec2(tile_px, tile_px),
                    );
                    painter.rect_stroke(cr, 0.0, egui::Stroke::new(1.0, egui::Color32::from_gray(120)));
                    if c == ax && r == az {
                        painter.rect_stroke(
                            cr,
                            0.0,
                            egui::Stroke::new(2.5, egui::Color32::from_rgb(60, 200, 220)),
                        );
                    }
                }
            }
            // Pintar arrastrando. El pincel manda; Ctrl siempre borra.
            if let Some(pos) = canvas.interact_pointer_pos() {
                let rel = pos - origin;
                let c = (rel.x / sub_px).floor() as i64;
                let r = (rel.y / sub_px).floor() as i64;
                if c >= 0 && r >= 0 && (c as u32) < w * res && (r as u32) < h * res {
                    let cell = (c as u32, r as u32);
                    if canvas.secondary_clicked() {
                        // El ancla es un TILE (la celda que apuntás en el mundo).
                        ax = cell.0 / res;
                        az = cell.1 / res;
                        changed = true;
                    } else if canvas.clicked() || canvas.dragged() {
                        let erase =
                            self.fp_brush == FpBrush::Erase || ui.input(|i| i.modifiers.command);
                        if erase {
                            changed |= solid.remove(&cell) | step.remove(&cell);
                        } else {
                            // Una celda es sólida O escalón, nunca las dos.
                            let (into, out) = match self.fp_brush {
                                FpBrush::Step => (&mut step, &mut solid),
                                _ => (&mut solid, &mut step),
                            };
                            out.remove(&cell);
                            // Clic suelto sobre una ya pintada = alternar;
                            // arrastrando siempre pinta (si no, sería insufrible).
                            changed |= if canvas.clicked() && into.contains(&cell) {
                                into.remove(&cell)
                            } else {
                                into.insert(cell)
                            };
                        }
                    }
                }
            }
            let tiles = solid
                .iter()
                .chain(step.iter())
                .map(|&(c, r)| (c / res, r / res))
                .collect::<std::collections::HashSet<_>>()
                .len();
            ui.label(
                egui::RichText::new(format!(
                    "🟥 {} sólidas · 🟨 {} escalón · {tiles} tiles tocados",
                    solid.len(),
                    step.len()
                ))
                .weak()
                .small(),
            );
            if changed {
                let fp = if solid.is_empty() && step.is_empty() {
                    None
                } else {
                    let mut sv: Vec<(u32, u32)> = solid.into_iter().collect();
                    let mut tv: Vec<(u32, u32)> = step.into_iter().collect();
                    sv.sort_unstable();
                    tv.sort_unstable();
                    Some(ha_assets::CollisionFootprint { w, h, ax, az, solid: sv, step: tv, res })
                };
                self.manifest.set_footprint(slot.key, fp);
                let saved = self.persist_manifest();
                self.status = format!(
                    "huella de {} actualizada {}",
                    slot.key,
                    if saved { "· guardado" } else { "· ⚠ no guardó" }
                );
            }
        });
    }

    /// Tamaño en celdas con el que abrir la huella de un slot: su recorte, su
    /// tira, o su pose representativa. Antes sólo miraba `rect`, y como los
    /// assets con poses no lo tienen, TODOS abrían en 1×1 — de ahí el
    /// "cuadradito enano".
    fn slot_cells_for_footprint(&self, key: &str) -> (u32, u32) {
        let Some(v) = self.manifest.get(key) else { return (1, 1) };
        let ha_assets::ArtValue::Full(e) = v else { return (1, 1) };
        let cells = e
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
            })
            .unwrap_or((1, 1));
        (cells.0.clamp(1, 12), cells.1.clamp(1, 12))
    }

    /// Textura + UV del recorte que el JUEGO va a dibujar para este slot, para
    /// usarlo de fondo del lienzo de la huella. Cubre los tres casos: recorte
    /// estático, primer frame de una tira, y pose representativa (que puede
    /// vivir en otra hoja).
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
    fn bound_preview(&mut self, ui: &mut egui::Ui, slot: SlotDef, max_px: f32) {
        let Some((_, tex)) = self.tex.clone() else {
            ui.label(egui::RichText::new("— procedural —").weak());
            return;
        };
        // Pose representativa (idle_down si existe, si no la primera).
        let rep: Option<(String, Pose)> = self.manifest.poses(slot.key).and_then(|m| {
            m.get("idle_down")
                .map(|p| ("idle_down".to_string(), p.clone()))
                .or_else(|| m.iter().next().map(|(k, v)| (k.clone(), v.clone())))
        });
        let Some((name, pose)) = rep else {
            // Sin poses: la imagen entera (recorte estático / tira / plano).
            let sz = preview_size(&tex, max_px);
            ui.add(egui::Image::new(egui::load::SizedTexture::new(tex.id(), sz)));
            return;
        };
        let slot_grid = self.manifest.get(slot.key).map(|v| v.grid()).unwrap_or((32, 32));
        let slot_png = self.manifest.get(slot.key).map(|v| v.png().to_string()).unwrap_or_default();
        // La pose representativa puede vivir en su propia hoja.
        let grid = pose.grid_or(slot_grid);
        let tex = match self.tex_for(ui.ctx(), pose.png_or(&slot_png)) {
            Some(t) => t,
            None => tex,
        };
        let img = tex.size_vec2();
        let (frames, dir, fps) = match pose.anim {
            Some(a) => (a.frames.max(1), pose.dir.unwrap_or_default(), a.fps),
            None => (1, StripDir::Right, 1.0),
        };
        if frames > 1 {
            let dt = ui.input(|i| i.stable_dt).min(0.1);
            self.bound_preview_t += dt * fps.max(0.1);
            if self.bound_preview_t >= 1.0 {
                let steps = self.bound_preview_t.floor() as usize;
                self.bound_preview_frame = (self.bound_preview_frame + steps) % frames as usize;
                self.bound_preview_t = self.bound_preview_t.fract();
            }
            ui.ctx().request_repaint();
        }
        let f = if frames > 1 { (self.bound_preview_frame as u32) % frames } else { 0 };
        let (fw, fh) = (pose.rect.w.max(1), pose.rect.h.max(1));
        let (fc, fr) = match dir {
            StripDir::Right => (pose.rect.col + f * fw, pose.rect.row),
            StripDir::Down => (pose.rect.col, pose.rect.row + f * fh),
        };
        let uv = egui::Rect::from_min_max(
            egui::pos2((fc * grid.0) as f32 / img.x, (fr * grid.1) as f32 / img.y),
            egui::pos2(((fc + fw) * grid.0) as f32 / img.x, ((fr + fh) * grid.1) as f32 / img.y),
        );
        let aspect = (fw * grid.0) as f32 / (fh * grid.1).max(1) as f32;
        let (dw, dh) = if aspect >= 1.0 { (max_px, max_px / aspect) } else { (max_px * aspect, max_px) };
        ui.add(egui::Image::new(egui::load::SizedTexture::new(tex.id(), egui::vec2(dw, dh))).uv(uv));
        let tag = if frames > 1 { format!("▶ {name} ({frames}f)") } else { format!("{name}") };
        ui.label(egui::RichText::new(tag).weak().small());
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

    /// Cobertura de píxeles OPACOS por sub-celda del recorte del slot: la base del
    /// "perfilar al sprite". `(col, fila) → fracción 0..1` sobre la grilla
    /// `(w*res) × (h*res)` estirada al recorte. Lee el PNG del disco (la textura de
    /// egui no se puede muestrear de vuelta). `None` = el slot no tiene PNG legible.
    fn sprite_alpha_cells(
        &self,
        key: &str,
        w: u32,
        h: u32,
        res: u32,
    ) -> Option<Vec<((u32, u32), f32)>> {
        let v = self.manifest.get(key)?;
        let img = image::open(PathBuf::from(&self.game_assets).join(v.png())).ok()?.to_rgba8();
        let (iw, ih) = img.dimensions();
        // Región del sprite dentro de la hoja: su recorte, o el PNG entero.
        let (x0, y0, cw, ch) = match v.rect() {
            Some(r) => {
                let g = v.grid();
                (r.col * g.0, r.row * g.1, r.w * g.0, r.h * g.1)
            }
            None => (0, 0, iw, ih),
        };
        if cw == 0 || ch == 0 || x0 + cw > iw || y0 + ch > ih {
            return None;
        }
        let (cols, rows) = (w * res, h * res);
        let mut out = Vec::with_capacity((cols * rows) as usize);
        for r in 0..rows {
            for c in 0..cols {
                let px0 = x0 + cw * c / cols;
                let px1 = (x0 + cw * (c + 1) / cols).max(px0 + 1).min(x0 + cw);
                let py0 = y0 + ch * r / rows;
                let py1 = (y0 + ch * (r + 1) / rows).max(py0 + 1).min(y0 + ch);
                let mut opaque = 0u32;
                let mut total = 0u32;
                for y in py0..py1 {
                    for x in px0..px1 {
                        total += 1;
                        if img.get_pixel(x, y).0[3] > 32 {
                            opaque += 1;
                        }
                    }
                }
                out.push(((c, r), if total == 0 { 0.0 } else { opaque as f32 / total as f32 }));
            }
        }
        Some(out)
    }

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

    #[test]
    fn poses_without_direction_keep_the_bare_action_name() {
        assert_eq!(pose_name_of("sleep", ""), "sleep");
        assert_eq!(pose_name_of("walk", "up_left"), "walk_up_left");
        assert_eq!(sanitize_action("  Regatear!  "), "regatear");
    }
}
