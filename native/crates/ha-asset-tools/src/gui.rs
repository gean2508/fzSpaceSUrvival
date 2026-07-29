//! F3 — editor de assets standalone (egui). Programa de DESARROLLO: corre solo,
//! NUNCA junto al juego.
//!
//! Modelo (lo que pediste): la lista de la izquierda son los **slots que el
//! juego usa** (`game_slots()`): árbol, roca, agua, ítems, máquinas, jugador…
//! Elegís un slot → elegís un PNG del arte fuente (LimeZu singles) → se **copia**
//! al árbol de assets del juego y se registra en `manifest.json`. Marcás si es
//! animado (agua/personajes: tira de N frames a F fps). Guardás → el juego se
//! reskinea en el próximo build. "Catálogo" regenera `CATALOGO.md` para Claude.

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

struct EditorApp {
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
    /// Buffer del CRUD "añadir slot".
    new_slot_key: String,

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
    fn new() -> Self {
        let slots = game_slots();
        let core_keys = slots.iter().map(|s| s.key.to_string()).collect();
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
            new_slot_key: String::new(),
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
        if key.is_empty() || self.slots.iter().any(|s| s.key == key) {
            return;
        }
        let category: &'static str = leak(key.split('/').next().unwrap_or("custom").to_string());
        self.slots.push(SlotDef { key: leak(key.to_string()), category, animated: false });
        self.slots.sort_by(|a, b| a.category.cmp(b.category).then(a.key.cmp(b.key)));
    }

    /// Cantidad de slots sin PNG asignado (arte procedural en el juego).
    fn unbound_count(&self) -> usize {
        self.slots.iter().filter(|s| self.manifest.get(s.key).is_none()).count()
    }

    fn ui(&mut self, ctx: &egui::Context) {
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
                        self.status = match self.selected.map(|i| self.slots[i]) {
                            Some(slot) if self.manifest.get(slot.key).is_some() => {
                                self.manifest.set_emission(slot.key, Some(em));
                                let saved = self.persist_manifest();
                                self.mode = EditorMode::Slots;
                                format!(
                                    "luz aplicada a {} (radio {:.1} m) {}",
                                    slot.key,
                                    em.radius,
                                    if saved { "· guardado" } else { "· ⚠ no guardó" }
                                )
                            }
                            Some(slot) => format!(
                                "{} todavía no tiene PNG — asignáselo primero y volvé a aplicar la luz",
                                slot.key
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
            let bound = self.manifest.sprites.len();
            ui.label(format!(
                "{} slots · {} con PNG · {} SIN PNG (procedural) · {} PNGs fuente",
                self.slots.len(),
                bound,
                self.unbound_count(),
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
                ui.label("🔎 slot");
                ui.text_edit_singleline(&mut self.slot_filter);
            });
            ui.checkbox(&mut self.only_unbound, "solo sin PNG (a asignar)");
            ui.separator();
            let filter = self.slot_filter.to_lowercase();
            let only_unbound = self.only_unbound;
            // Agrupar índices por categoría (respetando filtros) → árbol.
            let mut groups: Vec<(&'static str, Vec<usize>)> = Vec::new();
            for (i, slot) in self.slots.iter().enumerate() {
                if !filter.is_empty() && !slot.key.to_lowercase().contains(&filter) {
                    continue;
                }
                if only_unbound && self.manifest.get(slot.key).is_some() {
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
                    let bound = idxs
                        .iter()
                        .filter(|&&i| self.manifest.get(self.slots[i].key).is_some())
                        .count();
                    let header = egui::RichText::new(format!(
                        "▸ {}  ({}/{})",
                        cat.to_uppercase(),
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
                                let leaf = slot.key.split_once('/').map(|(_, n)| n).unwrap_or(slot.key);
                                let branch = if row + 1 == n { "└─" } else { "├─" };
                                let has = self.manifest.get(slot.key).is_some();
                                let dot = if has { "●" } else { "○" };
                                let mut txt = egui::RichText::new(format!("{branch} {dot} {leaf}")).monospace();
                                if has {
                                    txt = txt.color(egui::Color32::from_rgb(120, 230, 140));
                                }
                                let sel = self.selected == Some(i);
                                if ui.selectable_label(sel, txt).clicked() {
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

    fn central_detail_content(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, i: usize) {
            let slot = self.slots[i];
            ui.heading(slot.key);

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
            // Colisión de caminado del asset (relieve básico): el juego bloquea el
            // paso del jugador en la celda de este decor. Default por tipo
            // (árboles bloquean, resto no); acá lo FIJÁS por asset. La colisión
            // por-celda precisa (cráteres) es un plan aparte
            // (PLAN_COLISIONES_Y_CIUDADES.md).
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
            // (A1) Huella de colisión multi-celda: grilla cenital del SUELO donde
            // se pinta qué celdas son sólidas y cuál es el ancla. Habilita montañas
            // sólidas, cráteres caminables (anillo sólido + centro libre) y assets
            // grandes que bloquean su base. Vacío = colisión de 1 celda (arriba).
            if self.manifest.get(slot.key).is_some() {
                egui::CollapsingHeader::new("🗺 huella (colisión — DIBUJALA sobre el sprite)")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(
                            "Pintás la colisión JUSTO sobre el sprite: clic = alterna sólido \
                             (rojo); clic-derecho = fija el ancla (borde cyan). Ajustá ancho/alto \
                             a las celdas que ocupa el asset. Vacío = colisión de 1 celda (arriba).",
                        ).weak().small());
                        let cur = self.manifest.footprint(slot.key).cloned().unwrap_or_default();
                        // Default de dimensiones: las celdas que ocupa el recorte del asset.
                        let slot_grid = self.manifest.get(slot.key).map(|v| v.grid()).unwrap_or((32, 32));
                        let slot_rect = self.manifest.get(slot.key).and_then(|v| v.rect());
                        let def = slot_rect
                            .map(|r| (r.w.clamp(1, 9), r.h.clamp(1, 9)))
                            .unwrap_or((1, 1));
                        let mut w = if cur.w == 0 { def.0 } else { cur.w.clamp(1, 9) };
                        let mut h = if cur.h == 0 { def.1 } else { cur.h.clamp(1, 9) };
                        let mut solid: std::collections::HashSet<(u32, u32)> =
                            cur.solid.iter().copied().collect();
                        let mut ax = cur.ax;
                        let mut az = cur.az;
                        let mut changed = false;
                        ui.horizontal(|ui| {
                            ui.label("ancho");
                            if ui.add(egui::DragValue::new(&mut w).range(1..=9)).changed() {
                                changed = true;
                            }
                            ui.label("alto");
                            if ui.add(egui::DragValue::new(&mut h).range(1..=9)).changed() {
                                changed = true;
                            }
                        });
                        // Reajustar a las dimensiones nuevas.
                        solid.retain(|&(c, r)| c < w && r < h);
                        ax = ax.min(w.saturating_sub(1));
                        az = az.min(h.saturating_sub(1));

                        // Fondo = imagen del asset (su recorte si es sheet, o el PNG entero).
                        let img_uv: Option<(egui::TextureId, egui::Rect)> =
                            self.tex.as_ref().map(|(_, tex)| {
                                let img = tex.size_vec2();
                                let uv = match slot_rect {
                                    Some(r) => egui::Rect::from_min_max(
                                        egui::pos2(
                                            (r.col * slot_grid.0) as f32 / img.x,
                                            (r.row * slot_grid.1) as f32 / img.y,
                                        ),
                                        egui::pos2(
                                            ((r.col + r.w) * slot_grid.0) as f32 / img.x,
                                            ((r.row + r.h) * slot_grid.1) as f32 / img.y,
                                        ),
                                    ),
                                    None => egui::Rect::from_min_max(
                                        egui::pos2(0.0, 0.0),
                                        egui::pos2(1.0, 1.0),
                                    ),
                                };
                                (tex.id(), uv)
                            });

                        // Canvas: celdas grandes para pintar cómodo, con la imagen debajo.
                        let cell_px = 40.0_f32;
                        let (canvas, painter) = ui.allocate_painter(
                            egui::vec2(w as f32 * cell_px, h as f32 * cell_px),
                            egui::Sense::click(),
                        );
                        let origin = canvas.rect.min;
                        if let Some((id, uv)) = img_uv {
                            painter.image(id, canvas.rect, uv, egui::Color32::WHITE);
                        }
                        for r in 0..h {
                            for c in 0..w {
                                let cr = egui::Rect::from_min_size(
                                    origin + egui::vec2(c as f32 * cell_px, r as f32 * cell_px),
                                    egui::vec2(cell_px, cell_px),
                                );
                                if solid.contains(&(c, r)) {
                                    painter.rect_filled(
                                        cr,
                                        0.0,
                                        egui::Color32::from_rgba_unmultiplied(210, 50, 40, 110),
                                    );
                                }
                                painter.rect_stroke(
                                    cr,
                                    0.0,
                                    egui::Stroke::new(1.0, egui::Color32::from_gray(90)),
                                );
                                if c == ax && r == az {
                                    painter.rect_stroke(
                                        cr,
                                        0.0,
                                        egui::Stroke::new(2.5, egui::Color32::from_rgb(60, 200, 220)),
                                    );
                                }
                            }
                        }
                        if let Some(pos) = canvas.interact_pointer_pos() {
                            let rel = pos - origin;
                            let c = (rel.x / cell_px).floor() as i64;
                            let r = (rel.y / cell_px).floor() as i64;
                            if c >= 0 && r >= 0 && (c as u32) < w && (r as u32) < h {
                                let key = (c as u32, r as u32);
                                if canvas.clicked() {
                                    if solid.contains(&key) {
                                        solid.remove(&key);
                                    } else {
                                        solid.insert(key);
                                    }
                                    changed = true;
                                }
                                if canvas.secondary_clicked() {
                                    ax = key.0;
                                    az = key.1;
                                    changed = true;
                                }
                            }
                        }
                        if ui.button("limpiar huella").clicked() {
                            solid.clear();
                            changed = true;
                        }
                        if changed {
                            let fp = if solid.is_empty() {
                                None
                            } else {
                                let mut v: Vec<(u32, u32)> = solid.into_iter().collect();
                                v.sort_unstable();
                                Some(ha_assets::CollisionFootprint { w, h, ax, az, solid: v })
                            };
                            self.manifest.set_footprint(slot.key, fp);
                            self.status =
                                format!("huella de {} actualizada — Guardá para aplicar al juego", slot.key);
                        }
                    });
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
                    // Lienzo REDIMENSIONABLE: arrastrá el borde inferior-derecho (⤡)
                    // para agrandarlo y ver mejor la grilla / el recorte.
                    ui.label(egui::RichText::new("↕ arrastrá la esquina ⤡ de abajo-derecha para redimensionar el lienzo").weak().small());
                    egui::Resize::default()
                        .id_salt("canvas_resize")
                        .default_size([ui.available_width().min(900.0), 520.0])
                        .min_size([220.0, 200.0])
                        .show(ui, |ui| {
                            let h = ui.available_height().max(160.0);
                            self.sheet_canvas(ui, h);
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
                        // Preview reproducido con la MISMA semántica que el juego —
                        // COLAPSABLE: minimizalo si no lo querés ver ni ocupar espacio.
                        egui::CollapsingHeader::new("▶ preview animado")
                            .default_open(true)
                            .show(ui, |ui| {
                                let (frames, dir) = (self.anim_frames.max(1), self.strip_dir);
                                self.anim_strip_preview(ui, sr, frames, dir);
                            });
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
                            if ui.button("Guardar como animación (strip)")
                                .on_hover_text("cada frame = la selección; 'frames' = cuántos bloques contiguos hay")
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
                        let build_parts = build_part_poses(slot.key);
                        if !build_parts.is_empty() {
                            egui::CollapsingHeader::new(
                                "🧱 partes de construcción (pared de frente / lateral / puerta)",
                            )
                            .default_open(self.manifest.poses(slot.key).is_none())
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new(
                                    "Marcá en el lienzo DESDE DÓNDE HASTA DÓNDE está esta parte en \
                                     el atlas, elegí acá cuál es, y guardá con 💾. El juego elige \
                                     sola la pose según dónde esté el piso: la pared del fondo se \
                                     ve de frente, la de adelante baja, y las de los costados de \
                                     perfil. Con sólo 'frente' ya funciona todo."
                                ).weak());
                                let defined: std::collections::BTreeSet<String> = self
                                    .manifest
                                    .poses(slot.key)
                                    .map(|m| m.keys().cloned().collect())
                                    .unwrap_or_default();
                                egui::Grid::new("build_parts").spacing([4.0, 4.0]).show(ui, |ui| {
                                    for (i, (pose, label)) in build_parts.iter().enumerate() {
                                        let done = defined.contains(*pose);
                                        let mut txt = egui::RichText::new(format!(
                                            "{label}{}",
                                            if done { " ●" } else { "" }
                                        ))
                                        .small();
                                        if done {
                                            txt = txt.color(egui::Color32::from_rgb(120, 230, 140));
                                        }
                                        let active = self.pose_name == *pose;
                                        let hint = format!(
                                            "el juego la busca como '{pose}'\n{}",
                                            if done { "ya guardada — guardar la reemplaza" } else { "sin definir" }
                                        );
                                        if ui.selectable_label(active, txt).on_hover_text(hint).clicked() {
                                            self.pose_name = (*pose).to_string();
                                        }
                                        if i % 2 == 1 {
                                            ui.end_row();
                                        }
                                    }
                                });
                                self.pose_save_row(ui, slot);
                            });
                        }
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
                if !self.core_keys.contains(slot.key) && ui.button("❌ Eliminar slot").clicked() {
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
        let keys: Vec<String> = self.manifest.sprites.keys().cloned().collect();
        for k in keys {
            self.ensure_slot(&k);
        }
        if let Some(ctx) = ctx {
            if let Some(i) = self.selected {
                let key = self.slots[i].key;
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
        let slot = self.slots[i];
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
        // Copiar el PNG fuente al árbol del juego como art/<key>.png.
        let rel = format!("art/{}.png", slot.key);
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
                    ui.label("");
                    for (_d, arrow) in dirs {
                        ui.label(egui::RichText::new(*arrow).strong());
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
                ui.horizontal_wrapped(|ui| {
                    if ui.button("📁 Elegir carpeta…").clicked() {
                        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                            self.plan_import(&list_pngs(&dir));
                        }
                    }
                    if ui.button("🖼 Elegir archivos…").clicked() {
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

    /// Arma el plan de importación: nombre deducido + frames/grilla detectados de
    /// las dimensiones de cada PNG. No escribe nada todavía.
    fn plan_import(&mut self, paths: &[PathBuf]) {
        self.import_plan.clear();
        for p in paths {
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
                ui.label(
                    egui::RichText::new(format!(
                        "se escribirán: {}",
                        names.join(", ")
                    ))
                    .weak()
                    .small(),
                );
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
        ui.horizontal(|ui| {
            ui.label("▶ preview animado:");
            ui.add(egui::Image::new(egui::load::SizedTexture::new(tex.id(), egui::vec2(dw, dh))).uv(uv));
            ui.label("tamaño").on_hover_text("agrandá el preview para ver mejor la animación");
            ui.add(egui::DragValue::new(&mut self.preview_px).range(48.0..=640.0).speed(4.0));
            ui.label(format!("· {frames} frames de {fw}×{fh} @ {:.0} fps", self.anim_fps));
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
                                if let Some(i) = self.selected {
                                    let key = self.slots[i].key;
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
        let mut md = String::from("# CATÁLOGO de arte del juego (autogenerado)\n\n| slot | PNG | animado |\n|------|-----|---------|\n");
        for (key, v) in &self.manifest.sprites {
            let a = v.anim().map(|a| format!("{}f@{}fps", a.frames, a.fps)).unwrap_or_else(|| "no".into());
            md.push_str(&format!("| `{}` | {} | {} |\n", key, v.png(), a));
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
    const DOOR: &[(&str, &str)] = &[
        ("closed", "🚪 puerta CERRADA"),
        ("open", "🚪 puerta ABIERTA"),
        ("left", "◧ puerta lateral izquierda"),
        ("right", "◨ puerta lateral derecha"),
    ];
    if key.starts_with("door/") {
        DOOR
    } else if key.starts_with("wall/") || key.starts_with("window/") {
        WALL
    } else {
        &[]
    }
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
