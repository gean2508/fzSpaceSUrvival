//! Editor de Mundo — canvas WYSIWYG del mundo procedural + PINCELES + PALETA DE
//! ASSETS.
//!
//! Rasteriza el MISMO mundo determinista del juego (`topdown_tile`→`Tile::color`)
//! y encima deja **autorar overrides**: pintar terreno, colocar los assets que
//! creaste en el editor de assets, y borrar el decor que genera el mundo. Todo va
//! a `world_edits.json`, que el juego aplica sobre la generación en boot
//! (`ha-game::world_edits`).
//!
//! Las dos herramientas están **conectadas**: la paleta sale de `art/manifest.json`
//! (lo que escribe el editor de assets), con las miniaturas reales recortadas de
//! sus hojas. Si acabás de crear un asset, "↻ assets" lo trae sin reabrir nada.
//!
//! Regla de oro del terreno: **lo que pintás reemplaza al mundo**. Una celda con
//! tile autorado ya no genera árboles/rocas/flora procedurales — por eso podés
//! abrir un claro de pasto en medio del bosque y que quede así en el juego.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use eframe::egui;
use ha_assets::{ArtManifest, ArtValue};
use ha_shared::topdown::{large_asset, large_asset_count, topdown_tile, Tile};
use ha_shared::world_edits::{ClearedCell, PlacedDecor, PlacedStructure, TileEdit, WorldEdits};
use ha_shared::zonas::{ClaseZona, Zona, Zonas};
use ha_shared::CelestialBody;

/// Tiles que se pueden pintar (curado; nombre en ASCII para el juego, acá egui).
const PAINT_TILES: &[(Tile, &str)] = &[
    (Tile::Grass, "Pasto"),
    (Tile::Dirt, "Tierra"),
    (Tile::Sand, "Arena"),
    (Tile::Water, "Agua (bajío, se nada)"),
    (Tile::WaterDeep, "Agua honda (bloquea)"),
    (Tile::Regolith, "Regolito"),
    (Tile::Basalt, "Basalto"),
    (Tile::Ice, "Hielo"),
    (Tile::RockRed, "Roca roja"),
    (Tile::Stone, "Piedra"),
    // Relieve: pintar a mano un acantilado o una rampa es la forma de retocar el
    // terreno que generó el mundo — cerrar un paso o abrir uno donde te conviene.
    (Tile::Cliff, "Acantilado"),
    (Tile::Slope, "Rampa"),
    (Tile::Fence, "Cerca"),
    (Tile::Pavement, "Vereda"),
    (Tile::WoodFloor, "Piso"),
    (Tile::WoodWall, "Pared"),
    (Tile::StoneBrick, "Ladrillo"),
    (Tile::Door, "Puerta"),
];

/// Arte que el juego usa para cada tile (espejo de `ha-game::world2d::tile_art_key`).
/// Si el slot tiene PNG asignado, el editor lo dibuja igual que el juego.
fn tile_art_key(tile: Tile) -> Option<&'static str> {
    match tile {
        Tile::Water => Some("tiles/water"),
        Tile::WaterDeep => Some("tiles/water_deep"),
        Tile::Grass => Some("tiles/grass"),
        Tile::Sand => Some("tiles/sand"),
        Tile::Dirt => Some("tiles/dirt"),
        Tile::Stone => Some("tiles/stone"),
        Tile::Regolith => Some("tiles/regolith"),
        Tile::Basalt => Some("tiles/basalt"),
        Tile::Ice => Some("tiles/ice"),
        // Relieve. El editor dibuja la clave GENÉRICA; el juego además elige la
        // variante por bioma. Mostrar la genérica acá es correcto: es la que
        // aparece si esa variante no está curada.
        Tile::Cliff => Some("cliff/roca"),
        Tile::Slope => Some("slope/roca"),
        Tile::Fence => Some("fence/madera"),
        Tile::Pavement => Some("floor/vereda"),
        Tile::WoodFloor => Some("floor/wood"),
        Tile::WoodWall | Tile::Wall => Some("wall/wood"),
        Tile::StoneBrick => Some("wall/stone"),
        Tile::Door => Some("door/wood"),
        Tile::Window => Some("window/basic"),
        Tile::Step => Some("stairs/up"),
        Tile::Stairwell => Some("stairs/down"),
        _ => None,
    }
}

/// Categorías que NO tiene sentido plantar en el mapa (interfaz, cielo) y los
/// tiles, que tienen su propia paleta de pintura.
fn placeable_category(key: &str) -> bool {
    !(key.starts_with("ui/") || key.starts_with("backgrounds/") || key.starts_with("tiles/"))
}

/// Lado mínimo de una zona, en tiles. Por debajo de una manzana chica no cambia
/// nada y sólo ensucia la lista.
const MIN_ZONA: i64 = 16;

#[derive(PartialEq, Clone, Copy)]
enum Tool {
    Mover,
    /// **Seleccionar celdas** y después hacerles algo a todas juntas. Es la
    /// herramienta con la que se rediseña rápido: elegís la calle entera y la
    /// rellenás, la borrás o la copiás de una.
    Seleccionar,
    PintarTile,
    RellenarRect,
    Bote,
    Cuentagotas,
    ColocarEstructura,
    ColocarDecor,
    BorrarDecor,
    Borrar,
    /// **Pintar ZONAS**: repartís los barrios y el juego reparte lo que va dentro
    /// de cada uno (`ha_shared::zonas`).
    Zona,
}

/// Cómo elige celdas la herramienta de selección. Son los cuatro gestos que
/// cubren todo lo que se hace en un mapa: un bloque, una forma a mano, "lo que
/// está pegado a esto" y "todo lo que es igual a esto".
#[derive(PartialEq, Clone, Copy)]
enum SelMode {
    /// Arrastre de esquina a esquina.
    Rect,
    /// Contorno a mano alzada: al soltar se rellena TODO lo que quedó adentro.
    Libre,
    /// Varita: la mancha contigua del mismo terreno (un lago, una calle).
    Varita,
    /// Todo lo IGUAL que se ve en pantalla, esté pegado o no (todo el pasto).
    Similares,
}

impl SelMode {
    fn label(self) -> &'static str {
        match self {
            SelMode::Rect => "▭ rectángulo",
            SelMode::Libre => "✎ libre",
            SelMode::Varita => "✨ varita (contiguo)",
            SelMode::Similares => "≡ similares (a la vista)",
        }
    }

    fn ayuda(self) -> &'static str {
        match self {
            SelMode::Rect => "Arrastrá de una esquina a la otra.",
            SelMode::Libre => {
                "Arrastrá dibujando el contorno; al soltar se selecciona todo lo de adentro."
            }
            SelMode::Varita => "Clic derecho = agarra la mancha contigua del mismo terreno.",
            SelMode::Similares => {
                "Clic derecho = agarra todas las celdas del mismo terreno que se ven en pantalla."
            }
        }
    }
}

/// Qué hace un gesto de selección con lo que ya había elegido. Shift suma,
/// Ctrl resta, sin nada reemplaza — como en cualquier editor de imagen.
#[derive(PartialEq, Clone, Copy)]
enum SelOp {
    Reemplazar,
    Sumar,
    Restar,
}

/// Techo de la selección. Con más celdas que esto, cualquier operación masiva
/// (rellenar, plantar) tarda más de lo que tarda en arrepentirse el que la pidió.
const MAX_SEL: usize = 120_000;

/// Un pedazo de mundo copiado: los tiles tal como se VEN (aunque sean del mundo
/// procedural) y los assets colocados, en coordenadas relativas a la esquina de
/// la selección. Pegar reproduce la FORMA original, no su rectángulo: copiar una
/// curva de calle y pegarla más allá deja la curva, no un bloque.
#[derive(Clone, Default)]
struct Portapapeles {
    tiles: Vec<((i64, i64), Tile)>,
    decor: Vec<((i64, i64), PlacedDecor)>,
    ancho: i64,
    alto: i64,
}

/// Un asset colocable, tal como salió de `art/manifest.json`.
struct PaletteEntry {
    key: String,
    /// Nombre humano del objeto (`"Farol de calle"`), del catálogo de slots. Es
    /// lo que se lee en la paleta — la clave es para el código, no para vos.
    label: String,
    category: String,
}

/// Nombre humano de una clave de arte, buscado en el catálogo del juego. Para lo
/// que no esté catalogado (variantes, slots hechos a mano) cae a la hoja de la
/// clave con los guiones bajos abiertos: peor nombre, pero nunca vacío.
fn slot_label(key: &str) -> String {
    static NAMES: std::sync::OnceLock<std::collections::HashMap<&'static str, &'static str>> =
        std::sync::OnceLock::new();
    let names = NAMES.get_or_init(|| {
        ha_assets::game_slots().into_iter().map(|s| (s.key, s.label)).collect()
    });
    // La variante (`floor/piso#madera`) se lee por su objeto base.
    let (base, variant) = match key.split_once(ha_assets::VARIANT_SEP) {
        Some((b, v)) => (b, Some(v)),
        None => (key, None),
    };
    if let Some(label) = names.get(base) {
        return match variant {
            Some(v) => format!("{label} ({})", v.replace('_', " ")),
            None => (*label).to_string(),
        };
    }
    let leaf = base.split_once('/').map(|(_, n)| n).unwrap_or(base);
    leaf.replace('_', " ")
}

pub fn run_world_gui() -> ExitCode {
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 860.0]),
        ..Default::default()
    };
    let res = eframe::run_native(
        "Editor de Mundo (WYSIWYG)",
        native,
        Box::new(|_cc| Ok(Box::new(WorldApp::new()))),
    );
    if let Err(e) = res {
        eprintln!("editor de mundo: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

pub(crate) struct WorldApp {
    body: CelestialBody,
    z: i32,
    center: egui::Vec2,
    px_per_tile: f32,
    hover_cell: Option<(i64, i64)>,
    tool: Tool,
    paint_tile: Tile,
    brush: i64,                     // radio del pincel (1 = 1 celda, 3 = 5×5…)
    rect_start: Option<(i64, i64)>, // esquina del rectángulo en curso
    /// Carácter de la próxima zona que dibujes.
    zona_clase: ClaseZona,
    /// Su nombre. Sólo para vos: una lista de veinte rectángulos sin nombre es
    /// ilegible.
    zona_nombre: String,
    place_idx: usize,
    decor_key: String, // asset seleccionado para colocar a mano
    edits: WorldEdits,
    edits_path: PathBuf,
    status: String,

    // ── puente con el editor de assets ──────────────────────────────────────
    /// `art/manifest.json` del juego: lo escribe el editor de assets, lo lee esto.
    art: ArtManifest,
    art_path: PathBuf,
    /// Raíz de assets del juego (las rutas del manifest son relativas a ella).
    game_assets: PathBuf,
    palette: Vec<PaletteEntry>,
    palette_filter: String,
    /// Texturas por ruta relativa. `None` = intentada y fallida (no reintentar).
    tex: HashMap<String, Option<egui::TextureHandle>>,
    /// Dibujar los sprites reales en el canvas (WYSIWYG) o sólo colores.
    show_art: bool,
    /// Pintar en amarillo la COLISIÓN real de los assets colocados (la huella del
    /// editor de assets). Es lo que te dice con qué te vas a chocar en el juego.
    show_collision: bool,

    /// Historial para Ctrl+Z (snapshots completos; el archivo es chico).
    undo: Vec<WorldEdits>,
    /// Última celda donde se colocó decor arrastrando (evita apilar en la misma).
    last_decor_cell: Option<(i64, i64)>,
    /// Escala con la que se planta el asset (1 = su tamaño natural en tiles).
    decor_scale: f32,
    /// **El asset estampa su solar**: al colocarlo, reemplaza el terreno de todas
    /// las celdas de su huella. Es lo que hace que plantar una casa no sea
    /// "dibujar un sprite sobre el bosque" — y como la huella sale del recorte del
    /// slot, el solar se adapta SOLO al tamaño del asset.
    decor_stamps_ground: bool,
    /// Tile con el que se estampa ese solar.
    decor_ground_tile: Tile,
    /// Caché de las celdas que los assets colocados reemplazan con su solar
    /// (cuerpo/capa actual). Lo reconstruye [`WorldApp::rebuild_solar`].
    solar: HashMap<(i64, i64), Tile>,

    // ── selección ───────────────────────────────────────────────────────────
    /// Celdas elegidas (cuerpo/capa actual). Todo lo del menú flotante actúa
    /// sobre esto.
    sel: std::collections::HashSet<(i64, i64)>,
    /// Gesto con el que se eligen.
    sel_mode: SelMode,
    /// Esquina del arrastre de selección en curso.
    sel_drag: Option<(i64, i64)>,
    /// Contorno a mano alzada en curso (en tiles).
    sel_libre: Vec<(i64, i64)>,
    /// **La selección hace de máscara**: con celdas elegidas, el pincel, el bote
    /// y el rectángulo no pintan fuera. Es lo que deja repasar una calle sin
    /// desbordarse a la vereda.
    sel_limita: bool,
    /// Menú flotante abierto: dónde se dibuja y sobre qué celda se abrió.
    menu: Option<(egui::Pos2, (i64, i64))>,
    /// Lo último copiado (Ctrl+C).
    clip: Option<Portapapeles>,
    /// Rango de celdas visible del último frame — lo necesita "seleccionar
    /// similares", que trabaja sobre lo que se ve.
    vista: (i64, i64, i64, i64),
}

impl WorldApp {
    pub(crate) fn new() -> Self {
        // Rutas ancladas a la compilación (como el editor de assets): siempre correctas.
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let native_root = manifest_dir
            .parent()
            .and_then(std::path::Path::parent)
            .unwrap_or(manifest_dir);
        let edits_path = native_root.join("crates/ha-game/assets/world_edits.json");
        let edits = std::fs::read_to_string(&edits_path)
            .ok()
            .and_then(|s| serde_json::from_str::<WorldEdits>(&s).ok())
            .unwrap_or_default();
        let game_assets = native_root.join("crates/ha-game/assets");
        let art_path = game_assets.join("art/manifest.json");
        let mut app = Self {
            body: CelestialBody::Earth,
            z: 0,
            center: egui::vec2(0.0, 0.0),
            px_per_tile: 12.0,
            hover_cell: None,
            tool: Tool::Mover,
            paint_tile: Tile::Water,
            brush: 1,
            rect_start: None,
            zona_clase: ClaseZona::Residencial,
            zona_nombre: String::new(),
            place_idx: 0,
            decor_key: String::new(),
            edits,
            edits_path,
            status: String::new(),
            art: ArtManifest::default(),
            art_path,
            game_assets,
            palette: Vec::new(),
            palette_filter: String::new(),
            tex: HashMap::new(),
            show_art: true,
            show_collision: true,
            undo: Vec::new(),
            last_decor_cell: None,
            decor_scale: 1.0,
            decor_stamps_ground: false,
            decor_ground_tile: Tile::Stone,
            solar: HashMap::new(),
            sel: std::collections::HashSet::new(),
            sel_mode: SelMode::Rect,
            sel_drag: None,
            sel_libre: Vec::new(),
            sel_limita: true,
            menu: None,
            clip: None,
            vista: (0, 0, 0, 0),
        };
        app.load_art();
        app
    }

    // ── puente con el editor de assets ──────────────────────────────────────

    /// Relee el arte del juego (lo llama el Studio al entrar a este espacio: el
    /// objeto/variante que acabás de crear en Assets aparece en la paleta solo).
    pub(crate) fn reload_art(&mut self) {
        self.load_art();
    }

    /// (Re)lee `art/manifest.json` y arma la paleta con TODO lo que tenga arte.
    /// Es lo que hace que un asset recién creado aparezca acá para colocarlo.
    fn load_art(&mut self) {
        self.art = std::fs::read_to_string(&self.art_path)
            .ok()
            .and_then(|s| ArtManifest::from_json(&s).ok())
            .unwrap_or_default();
        self.tex.clear();
        self.palette = self
            .art
            .sprites
            .keys()
            .filter(|k| placeable_category(k))
            .map(|k| PaletteEntry {
                key: k.clone(),
                label: slot_label(k),
                category: k.split('/').next().unwrap_or("otros").to_string(),
            })
            .collect();
        if self.decor_key.is_empty() || !self.art.sprites.contains_key(&self.decor_key) {
            self.decor_key = self.palette.first().map(|p| p.key.clone()).unwrap_or_default();
        }
        self.status = format!(
            "{} assets con arte disponibles para colocar ({})",
            self.palette.len(),
            self.art_path.display()
        );
    }

    /// Qué hoja y qué recorte px representan a un slot: su `rect`, o su pose más
    /// representativa (`idle_down`/`center`/la primera), o la imagen entera. Es lo
    /// mismo que elige el juego, así la miniatura no miente.
    fn slot_view(&self, key: &str) -> Option<(String, Option<[u32; 4]>)> {
        let v = self.art.get(key)?;
        let ArtValue::Full(e) = v else {
            return Some((v.png().to_string(), None));
        };
        let grid = e.grid.unwrap_or((32, 32));
        if let Some(r) = e.rect {
            return Some((
                e.png.clone(),
                Some([r.col * grid.0, r.row * grid.1, r.w * grid.0, r.h * grid.1]),
            ));
        }
        if let Some(poses) = &e.poses {
            let pose = poses
                .get("idle_down")
                .or_else(|| poses.get("center"))
                .or_else(|| poses.values().next());
            if let Some(p) = pose {
                let (gw, gh) = p.grid_or(grid);
                return Some((
                    p.png_or(&e.png).to_string(),
                    Some([p.rect.col * gw, p.rect.row * gh, p.rect.w * gw, p.rect.h * gh]),
                ));
            }
        }
        // Tira animada: mostrar sólo el primer frame, no la hoja entera.
        if let Some(s) = e.strip {
            return Some((
                e.png.clone(),
                Some([s.col * grid.0, s.row * grid.1, s.w * grid.0, s.h * grid.1]),
            ));
        }
        Some((e.png.clone(), None))
    }

    /// Profundidad de dibujo del asset, **misma regla que el juego**
    /// (`ha-game::layers::depth`): la capa del slot elige la banda y el pie del
    /// sprite desempata dentro de la banda Y-sorted. Se usa sólo para ordenar el
    /// pintado del editor, así que lo que ves acá es lo que el juego dibuja.
    fn draw_depth(&self, key: &str, tz: i64) -> f32 {
        use ha_assets::ArtLayer;
        let foot = (tz + 1) as f32;
        match self.art.slot_layer(key).unwrap_or(ArtLayer::Object) {
            ArtLayer::Floor | ArtLayer::Light => 1.0 + foot * 1e-6,
            ArtLayer::Basement => 0.5 + foot * 1e-6,
            ArtLayer::Wall | ArtLayer::Object => 2.0 + foot * 0.001,
            ArtLayer::Roof => 6.0,
        }
    }

    /// Tamaño en METROS que le corresponde al asset: sus celdas de recorte
    /// (1 celda = 1 tile = 1 m). Una casa de 5×4 celdas mide 5×4 y se ve como una
    /// casa; un arbusto de 1×1 mide 1×1. Si el slot es una imagen suelta sin
    /// recorte, se deduce de sus píxeles y su grilla.
    fn slot_size_m(&mut self, ctx: &egui::Context, key: &str) -> Option<[f32; 2]> {
        // Resolver contra el manifest primero y soltar el préstamo: `sheet_px`
        // necesita `&mut self` para cachear la textura.
        let resolved: Result<(u32, u32), (String, (u32, u32))> = {
            let v = self.art.get(key)?;
            match v {
                ArtValue::Full(e) => {
                    let grid = e.grid.unwrap_or((32, 32));
                    let cells = e
                        .rect
                        .map(|r| (r.w, r.h))
                        .or_else(|| {
                            e.poses
                                .as_ref()
                                .and_then(|m| {
                                    m.get("idle_down")
                                        .or_else(|| m.get("center"))
                                        .or_else(|| m.values().next())
                                })
                                .map(|p| (p.rect.w, p.rect.h))
                        })
                        .or_else(|| e.strip.map(|s| (s.w, s.h)));
                    match cells {
                        Some(c) => Ok(c),
                        // Imagen suelta: sus píxeles divididos por la grilla.
                        None => Err((e.png.clone(), grid)),
                    }
                }
                ArtValue::Path(p) => Err((p.clone(), (32, 32))),
            }
        };
        let cells = match resolved {
            Ok(c) => c,
            Err((png, grid)) => {
                let (w, h) = self.sheet_px(ctx, &png)?;
                (w / grid.0.max(1), h / grid.1.max(1))
            }
        };
        Some([cells.0.max(1) as f32, cells.1.max(1) as f32])
    }

    fn sheet_px(&mut self, ctx: &egui::Context, rel: &str) -> Option<(u32, u32)> {
        let t = self.tex_for(ctx, rel)?;
        let s = t.size();
        Some((s[0] as u32, s[1] as u32))
    }

    fn tex_for(&mut self, ctx: &egui::Context, rel: &str) -> Option<egui::TextureHandle> {
        if let Some(t) = self.tex.get(rel) {
            return t.clone();
        }
        let path = self.game_assets.join(rel);
        let handle = load_texture(ctx, &path, rel);
        self.tex.insert(rel.to_string(), handle.clone());
        handle
    }

    /// Dibuja el sprite de un slot dentro de `rect` (respetando su aspecto y con
    /// el pie apoyado abajo, como en el juego). `false` = ese slot no tiene arte
    /// cargable y el que llama debe dibujar su fallback de color.
    fn draw_slot(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        key: &str,
        cell: egui::Rect,
        scale: f32,
    ) -> bool {
        let Some((uv, px, id)) = self.slot_uv(ctx, key) else { return false };
        // Alto proporcional al ancho de la celda, apoyado en el borde inferior
        // (pivote al pie: un árbol sobresale hacia arriba, como en el juego).
        let w = cell.width() * scale;
        let h = w * px.y / px.x;
        let draw = egui::Rect::from_min_max(
            egui::pos2(cell.center().x - w * 0.5, cell.max.y - h),
            egui::pos2(cell.center().x + w * 0.5, cell.max.y),
        );
        painter.image(id, draw, uv, egui::Color32::WHITE);
        true
    }

    /// Dibuja el sprite EXACTAMENTE dentro de `dest` (sin re-encuadrar): el rect
    /// ya viene de la regla de colocación, así que lo que se ve acá es pixel por
    /// pixel lo que el juego va a plantar.
    fn draw_slot_in(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        key: &str,
        dest: egui::Rect,
    ) -> bool {
        let Some((uv, _, id)) = self.slot_uv(ctx, key) else { return false };
        painter.image(id, dest, uv, egui::Color32::WHITE);
        true
    }

    /// UV + tamaño px del recorte del slot y la textura de su hoja.
    fn slot_uv(
        &mut self,
        ctx: &egui::Context,
        key: &str,
    ) -> Option<(egui::Rect, egui::Vec2, egui::TextureId)> {
        let (rel, crop) = self.slot_view(key)?;
        let tex = self.tex_for(ctx, &rel)?;
        let img = tex.size_vec2();
        let (uv, px) = match crop {
            Some([x, y, w, h]) => (
                egui::Rect::from_min_max(
                    egui::pos2(x as f32 / img.x, y as f32 / img.y),
                    egui::pos2((x + w) as f32 / img.x, (y + h) as f32 / img.y),
                ),
                egui::vec2(w as f32, h as f32),
            ),
            None => (
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                img,
            ),
        };
        if px.x <= 0.0 || px.y <= 0.0 {
            return None;
        }
        Some((uv, px, tex.id()))
    }

    /// Celdas (tiles) que ocupa el slot y su celda ANCLA — la que va a quedar
    /// justo donde apuntás con el mouse. Espejo exacto de lo que hace el juego
    /// (`ArtLibrary::slot_cells` / `anchor`).
    fn slot_cells_anchor(&mut self, ctx: &egui::Context, key: &str) -> ((u32, u32), (u32, u32)) {
        let s = self.slot_size_m(ctx, key).unwrap_or([1.0, 1.0]);
        let cells = (s[0].round().max(1.0) as u32, s[1].round().max(1.0) as u32);
        let anchor = self.art.anchor(key, cells);
        (cells, anchor)
    }

    /// Rect que el asset ocupará **en tiles del mundo** al colocarlo en la celda
    /// `(tx, tz)` con la escala actual. Es la MISMA cuenta que hace el juego
    /// (`ha_assets::placement_center_tiles`), así que el fantasma no miente: el
    /// asset cae donde está el mouse, no una celda más arriba.
    fn placed_tile_rect(
        &mut self,
        ctx: &egui::Context,
        key: &str,
        tx: i64,
        tz: i64,
        size: [f32; 2],
    ) -> egui::Rect {
        let (cells, anchor) = self.slot_cells_anchor(ctx, key);
        let c = ha_assets::placement_center_tiles(tx, tz, cells, anchor, size);
        egui::Rect::from_center_size(
            egui::pos2(c[0], c[1]),
            egui::vec2(size[0].max(0.01), size[1].max(0.01)),
        )
    }

    /// Cajas de colisión (en tiles del mundo) del asset colocado en `(tx, tz)`:
    /// la huella que pintaste en el editor de assets, escalada como el sprite.
    /// Vacío = ese asset no declara colisión (no frena a nadie).
    fn collision_tile_rects(
        &mut self,
        ctx: &egui::Context,
        key: &str,
        tx: i64,
        tz: i64,
        size: [f32; 2],
    ) -> Vec<egui::Rect> {
        let _ = ctx;
        let Some(fp) = self.art.footprint(key).cloned() else { return Vec::new() };
        let (sx, sz) = if fp.w > 0 && fp.h > 0 {
            (size[0] / fp.w as f32, size[1] / fp.h as f32)
        } else {
            (1.0, 1.0)
        };
        // Escala alrededor del CENTRO de la celda ancla — igual que el juego
        // (`world2d::footprint_hits`), para que caja y sprite crezcan juntos.
        let (ox, oz) = (tx as f32 + 0.5, tz as f32 + 0.5);
        fp.solid_rects()
            .map(|r| {
                egui::Rect::from_min_max(
                    egui::pos2(ox + (r[0] - 0.5) * sx, oz + (r[1] - 0.5) * sz),
                    egui::pos2(ox + (r[2] - 0.5) * sx, oz + (r[3] - 0.5) * sz),
                )
            })
            .collect()
    }

    // ── edición ─────────────────────────────────────────────────────────────

    fn push_undo(&mut self) {
        self.undo.push(self.edits.clone());
        if self.undo.len() > 64 {
            self.undo.remove(0);
        }
    }

    fn save(&mut self) {
        match serde_json::to_string_pretty(&self.edits) {
            Ok(json) => match std::fs::write(&self.edits_path, &json) {
                Ok(()) => {
                    // Verificar releyendo (como los otros editores).
                    let ok = std::fs::read_to_string(&self.edits_path)
                        .ok()
                        .and_then(|s| serde_json::from_str::<WorldEdits>(&s).ok())
                        .is_some();
                    self.status = if ok {
                        format!(
                            "Guardado ({} tiles, {} assets, {} estructuras, {} celdas limpiadas) — recompilá el juego para verlo",
                            self.edits.tiles.len(),
                            self.edits.decor.len(),
                            self.edits.structures.len(),
                            self.edits.cleared.len(),
                        )
                    } else {
                        "Guardado pero no se pudo releer (revisá el archivo)".into()
                    };
                }
                Err(e) => self.status = format!("Error al guardar: {e}"),
            },
            Err(e) => self.status = format!("Error al serializar: {e}"),
        }
    }

    /// Borra overrides de tile y estructuras que toquen la celda.
    fn erase(&mut self, x: i64, zc: i64) {
        let b = self.body as u8;
        self.edits
            .tiles
            .retain(|t| !(t.body == b && t.z == self.z && t.x == x && t.zc == zc));
        self.edits.structures.retain(|s| {
            !(s.body == b
                && s.z == self.z
                && x >= s.cx
                && x < s.cx + s.w
                && zc >= s.cz
                && zc < s.cz + s.h)
        });
    }

    /// Tile EFECTIVO: override si existe, si no el procedural. (Cuentagotas, bote.)
    fn effective_tile(&self, x: i64, zc: i64) -> Tile {
        let b = self.body as u8;
        self.edits
            .tiles
            .iter()
            .find(|t| t.body == b && t.z == self.z && t.x == x && t.zc == zc)
            .map(|t| t.tile)
            // Mismo orden que el juego: el solar de los assets va por debajo de lo
            // que pintaste a mano, y por encima del mundo procedural. Sin esto
            // estampabas solares a ciegas: el editor te seguía mostrando el bosque.
            .or_else(|| self.solar.get(&(x, zc)).copied())
            .unwrap_or_else(|| topdown_tile(self.body, self.z, x, zc))
    }

    /// Recalcula el mapa de SOLARES del cuerpo/capa actual: las celdas que los
    /// assets colocados reemplazan con su `ground`. Se cachea porque el rasterizado
    /// consulta `effective_tile` una vez por celda visible (decenas de miles) y
    /// recorrer la lista de assets en cada una no escala.
    ///
    /// Hay que llamarlo tras CUALQUIER cambio en `edits.decor` (colocar, borrar,
    /// deshacer, cargar).
    fn rebuild_solar(&mut self) {
        let (b, z) = (self.body as u8, self.z);
        self.solar = self
            .edits
            .decor
            .iter()
            .filter(|d| d.body == b && d.z == z)
            .filter_map(|d| d.ground.map(|t| (d, t)))
            .flat_map(|(d, tile)| d.covered_cells().map(move |c| (c, tile)))
            .collect();
    }

    /// Overrides del cuerpo/z actual como mapa (para operaciones masivas rápidas).
    fn override_map(&self) -> HashMap<(i64, i64), Tile> {
        let b = self.body as u8;
        self.edits
            .tiles
            .iter()
            .filter(|t| t.body == b && t.z == self.z)
            .map(|t| ((t.x, t.zc), t.tile))
            .collect()
    }

    /// Reescribe los overrides del cuerpo/z actual desde el mapa (conserva otros).
    fn commit_map(&mut self, map: HashMap<(i64, i64), Tile>) {
        let b = self.body as u8;
        self.edits.tiles.retain(|t| !(t.body == b && t.z == self.z));
        for ((x, zc), tile) in map {
            self.edits.tiles.push(TileEdit { body: b, z: self.z, x, zc, tile });
        }
    }

    /// Pinta/borra un bloque de pincel `(2r-1)²` centrado en la celda.
    fn brush_apply(&mut self, cx: i64, cz: i64, tile: Option<Tile>) {
        let r = self.brush - 1;
        let mut map = self.override_map();
        for dz in -r..=r {
            for dx in -r..=r {
                // La selección hace de máscara: el pincel no se desborda.
                if !self.en_seleccion(cx + dx, cz + dz) {
                    continue;
                }
                match tile {
                    Some(t) => {
                        map.insert((cx + dx, cz + dz), t);
                    }
                    None => {
                        map.remove(&(cx + dx, cz + dz));
                    }
                }
            }
        }
        self.commit_map(map);
    }

    /// Rellena un rectángulo (acotado a 40k celdas por seguridad).
    fn fill_rect(&mut self, a: (i64, i64), b: (i64, i64), tile: Tile) {
        let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
        let (z0, z1) = (a.1.min(b.1), a.1.max(b.1));
        if (x1 - x0 + 1) * (z1 - z0 + 1) > 40_000 {
            self.status = "Rectángulo demasiado grande (>40k celdas)".into();
            return;
        }
        let mut map = self.override_map();
        for zc in z0..=z1 {
            for x in x0..=x1 {
                if !self.en_seleccion(x, zc) {
                    continue;
                }
                map.insert((x, zc), tile);
            }
        }
        self.commit_map(map);
    }

    /// Bote: flood-fill del TERRENO contiguo del mismo tipo (BFS acotado a 20k).
    fn bucket_fill(&mut self, x: i64, zc: i64, tile: Tile) {
        let mut map = self.override_map();
        let eff = |m: &HashMap<(i64, i64), Tile>, x: i64, zc: i64| {
            m.get(&(x, zc)).copied().unwrap_or_else(|| topdown_tile(self.body, self.z, x, zc))
        };
        let target = eff(&map, x, zc);
        if target == tile {
            return;
        }
        let mut stack = vec![(x, zc)];
        let mut seen = std::collections::HashSet::new();
        let mut n = 0;
        while let Some((cx, cz)) = stack.pop() {
            if !seen.insert((cx, cz)) || eff(&map, cx, cz) != target {
                continue;
            }
            // Con selección viva, el bote se derrama sólo dentro de ella.
            if !self.en_seleccion(cx, cz) {
                continue;
            }
            map.insert((cx, cz), tile);
            n += 1;
            if n > 20_000 {
                self.status = "Área muy grande (bote cortado a 20k)".into();
                break;
            }
            stack.push((cx + 1, cz));
            stack.push((cx - 1, cz));
            stack.push((cx, cz + 1));
            stack.push((cx, cz - 1));
        }
        self.commit_map(map);
    }

    fn place_structure(&mut self, x: i64, zc: i64) {
        let Some(a) = large_asset(self.place_idx) else {
            return;
        };
        let (cx, cz) = (x - a.w / 2, zc - a.h / 2);
        // No apilar: si ya hay una estructura tocando la celda, no colocar.
        let b = self.body as u8;
        let clash = self.edits.structures.iter().any(|s| {
            s.body == b
                && s.z == self.z
                && cx < s.cx + s.w
                && cx + a.w > s.cx
                && cz < s.cz + s.h
                && cz + a.h > s.cz
        });
        if clash {
            return;
        }
        self.edits.structures.push(PlacedStructure {
            body: b,
            z: self.z,
            cx,
            cz,
            w: a.w,
            h: a.h,
            art: a.art.to_string(),
            solid: Vec::new(), // base completa; la huella fina se pinta en el editor de assets
            rot: 0,
        });
    }

    /// Planta el asset elegido en la celda. Una celda = un asset (volver a
    /// clickear lo reemplaza, no lo apila).
    fn place_decor(&mut self, ctx: &egui::Context, x: i64, zc: i64) {
        let key = self.decor_key.clone();
        let scale = self.decor_scale.max(0.1);
        let size = self.slot_size_m(ctx, &key).map(|[w, h]| [w * scale, h * scale]);
        // La huella se guarda CON la colocación: si mañana recortás el sprite de
        // otra manera, los solares ya autorados no se mueven de sitio.
        let (cells, anchor) = self.slot_cells_anchor(ctx, &key);
        self.place_decor_sized(x, zc, size, Some(cells), Some(anchor));
    }

    /// El alta en sí (sin UI): una celda = un asset. Volver a colocar reemplaza.
    fn place_decor_sized(
        &mut self,
        x: i64,
        zc: i64,
        size: Option<[f32; 2]>,
        cells: Option<(u32, u32)>,
        anchor: Option<(u32, u32)>,
    ) {
        if self.decor_key.is_empty() {
            self.status = "elegí un asset de la paleta (derecha)".into();
            return;
        }
        let b = self.body as u8;
        self.edits
            .decor
            .retain(|d| !(d.body == b && d.z == self.z && d.x == x && d.zc == zc));
        self.edits.decor.push(PlacedDecor {
            body: b,
            z: self.z,
            x,
            zc,
            key: self.decor_key.clone(),
            size,
            // SOLAR: si la casilla está tildada, el asset reemplaza el terreno de
            // toda su huella — el suelo se adapta al tamaño del asset, sea una
            // maceta o una universidad. Destildada, se comporta como siempre
            // (sprite encima, terreno intacto).
            ground: self.decor_stamps_ground.then_some(self.decor_ground_tile),
            cells: cells.map(|(w, h)| [w, h]),
            anchor: anchor.map(|(ax, az)| [ax, az]),
        });
    }

    /// Goma de DECOR: saca el asset que hayas puesto ahí y, si la celda generaba
    /// decor procedural (un árbol del bosque), la marca como limpia para que el
    /// juego tampoco lo genere. El terreno no se toca.
    fn erase_decor(&mut self, x: i64, zc: i64) {
        let b = self.body as u8;
        let had = self.edits.decor.len();
        self.edits
            .decor
            .retain(|d| !(d.body == b && d.z == self.z && d.x == x && d.zc == zc));
        let removed_authored = self.edits.decor.len() != had;
        // Sólo marcamos la celda si el mundo genera algo ahí (si no, sería basura
        // en el JSON). Si borramos algo autorado, con eso alcanza.
        if !removed_authored && self.procedural_decor_at(x, zc).is_some() && !self.is_cleared(x, zc) {
            self.edits.cleared.push(ClearedCell { body: b, z: self.z, x, zc });
        }
    }

    fn is_cleared(&self, x: i64, zc: i64) -> bool {
        let b = self.body as u8;
        self.edits
            .cleared
            .iter()
            .any(|c| c.body == b && c.z == self.z && c.x == x && c.zc == zc)
    }

    /// ¿Esta celda tiene tile autorado? Entonces el decor procedural NO se genera
    /// (misma regla que el juego: lo que pintás reemplaza al mundo).
    fn has_tile_override(&self, x: i64, zc: i64) -> bool {
        let b = self.body as u8;
        self.edits
            .tiles
            .iter()
            .any(|t| t.body == b && t.z == self.z && t.x == x && t.zc == zc)
    }

    /// Decor PROCEDURAL de la celda tal como lo generará el juego: `(clave, ¿es
    /// árbol/roca cosechable?)`. `None` = ahí no crece nada.
    fn procedural_decor_at(&self, tx: i64, tz: i64) -> Option<(String, bool)> {
        use ha_shared::worldgen::{
            nature_decor_at, pick_tree_species_in, prop_decor_at, surface_decor_at, DecorKind,
        };
        if self.z != 0 {
            return None;
        }
        if let Some(kind) = surface_decor_at(self.body, tx, tz) {
            return Some(match kind {
                DecorKind::Tree => (pick_tree_species_in(self.body, tx, tz).key.to_string(), true),
                DecorKind::Rock => ("decor/rock".to_string(), true),
            });
        }
        nature_decor_at(self.body, tx, tz)
            .or_else(|| prop_decor_at(self.body, tx, tz))
            .map(|e| (e.key.to_string(), false))
    }

    /// Lo que el juego dibujará en la celda: el decor autorado gana; si no, el
    /// procedural, salvo que la celda esté pintada o limpiada. Es EXACTAMENTE la
    /// regla de `world2d::spawn_chunk_decor`.
    fn effective_decor(&self, tx: i64, tz: i64) -> Option<String> {
        let b = self.body as u8;
        if let Some(d) = self
            .edits
            .decor
            .iter()
            .find(|d| d.body == b && d.z == self.z && d.x == tx && d.zc == tz)
        {
            return Some(d.key.clone());
        }
        if self.has_tile_override(tx, tz) || self.is_cleared(tx, tz) {
            return None;
        }
        // El decor procedural tampoco aparece sobre terreno sólido.
        if self.effective_tile(tx, tz).solid() {
            return None;
        }
        self.procedural_decor_at(tx, tz).map(|(k, _)| k)
    }

    // ── selección ───────────────────────────────────────────────────────────
    // Todo lo de acá es puro (no toca egui) a propósito: es la parte que se
    // testea. La UI sólo decide QUÉ gesto llama a cuál.

    /// ¿Esta celda se puede tocar? Con selección viva y la máscara puesta, el
    /// pincel no pinta fuera de lo elegido.
    fn en_seleccion(&self, x: i64, zc: i64) -> bool {
        !self.sel_limita || self.sel.is_empty() || self.sel.contains(&(x, zc))
    }

    /// Suma/resta/reemplaza celdas en la selección, respetando el techo.
    fn sel_aplicar(&mut self, celdas: impl IntoIterator<Item = (i64, i64)>, op: SelOp) {
        if op == SelOp::Reemplazar {
            self.sel.clear();
        }
        let mut lleno = false;
        for c in celdas {
            match op {
                SelOp::Restar => {
                    self.sel.remove(&c);
                }
                _ => {
                    if self.sel.len() >= MAX_SEL {
                        lleno = true;
                        break;
                    }
                    self.sel.insert(c);
                }
            }
        }
        if lleno {
            self.status = format!("selección cortada en {MAX_SEL} celdas");
        } else {
            self.status = format!("{} celdas seleccionadas", self.sel.len());
        }
    }

    /// Rectángulo de esquina a esquina, en cualquier sentido.
    fn sel_rect(&mut self, a: (i64, i64), b: (i64, i64), op: SelOp) {
        let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
        let (z0, z1) = (a.1.min(b.1), a.1.max(b.1));
        let celdas: Vec<(i64, i64)> =
            (z0..=z1).flat_map(|z| (x0..=x1).map(move |x| (x, z))).collect();
        self.sel_aplicar(celdas, op);
    }

    /// Cierra el contorno a mano alzada y se queda con TODO lo de adentro (más
    /// el propio trazo). Sin el relleno, "dibujar libremente" sólo daría una
    /// línea de un tile y no serviría para rediseñar una manzana.
    fn sel_libre_cerrar(&mut self, op: SelOp) {
        let trazo = std::mem::take(&mut self.sel_libre);
        if trazo.is_empty() {
            return;
        }
        let poly: Vec<(f32, f32)> =
            trazo.iter().map(|&(x, z)| (x as f32 + 0.5, z as f32 + 0.5)).collect();
        let (x0, x1) = (
            trazo.iter().map(|c| c.0).min().unwrap(),
            trazo.iter().map(|c| c.0).max().unwrap(),
        );
        let (z0, z1) = (
            trazo.iter().map(|c| c.1).min().unwrap(),
            trazo.iter().map(|c| c.1).max().unwrap(),
        );
        let mut celdas: Vec<(i64, i64)> = trazo.clone();
        if (x1 - x0 + 1) * (z1 - z0 + 1) <= MAX_SEL as i64 {
            for z in z0..=z1 {
                for x in x0..=x1 {
                    if punto_en_poly((x as f32 + 0.5, z as f32 + 0.5), &poly) {
                        celdas.push((x, z));
                    }
                }
            }
        }
        self.sel_aplicar(celdas, op);
    }

    /// Varita: la mancha CONTIGUA del mismo terreno efectivo (BFS acotado).
    fn sel_varita(&mut self, x: i64, zc: i64, op: SelOp) {
        let objetivo = self.effective_tile(x, zc);
        let mut vistas = std::collections::HashSet::new();
        let mut pila = vec![(x, zc)];
        let mut celdas = Vec::new();
        while let Some((cx, cz)) = pila.pop() {
            if !vistas.insert((cx, cz)) || self.effective_tile(cx, cz) != objetivo {
                continue;
            }
            celdas.push((cx, cz));
            if celdas.len() >= MAX_SEL {
                break;
            }
            pila.extend([(cx + 1, cz), (cx - 1, cz), (cx, cz + 1), (cx, cz - 1)]);
        }
        self.sel_aplicar(celdas, op);
    }

    /// Todas las celdas del mismo terreno **que se ven en pantalla**. Acotarlo a
    /// la vista es lo que lo hace usable: el mundo es infinito, "todo el pasto"
    /// no termina nunca.
    fn sel_similares(&mut self, x: i64, zc: i64, op: SelOp) {
        let objetivo = self.effective_tile(x, zc);
        let (x0, z0, x1, z1) = self.vista;
        let mut celdas = Vec::new();
        for z in z0..=z1 {
            for x in x0..=x1 {
                if self.effective_tile(x, z) == objetivo {
                    celdas.push((x, z));
                }
            }
        }
        self.sel_aplicar(celdas, op);
    }

    /// Deja sólo el BORDE de lo elegido: es como se dibuja el cordón de una
    /// vereda o la pared de una manzana sin marcar celda por celda.
    fn sel_contorno(&mut self) {
        let borde: std::collections::HashSet<(i64, i64)> = self
            .sel
            .iter()
            .filter(|&&(x, z)| {
                [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .any(|(dx, dz)| !self.sel.contains(&(x + dx, z + dz)))
            })
            .copied()
            .collect();
        self.sel = borde;
        self.status = format!("contorno: {} celdas", self.sel.len());
    }

    /// Engorda (`+1`) o adelgaza (`-1`) la selección un anillo.
    fn sel_dilatar(&mut self, d: i64) {
        if d > 0 {
            let mut nueva = self.sel.clone();
            for &(x, z) in &self.sel {
                for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    if nueva.len() < MAX_SEL {
                        nueva.insert((x + dx, z + dz));
                    }
                }
            }
            self.sel = nueva;
        } else {
            self.sel = self
                .sel
                .iter()
                .filter(|&&(x, z)| {
                    [(1, 0), (-1, 0), (0, 1), (0, -1)]
                        .iter()
                        .all(|(dx, dz)| self.sel.contains(&(x + dx, z + dz)))
                })
                .copied()
                .collect();
        }
        self.status = format!("{} celdas seleccionadas", self.sel.len());
    }

    /// Esquinas (mín, máx) de lo elegido.
    fn sel_bbox(&self) -> Option<((i64, i64), (i64, i64))> {
        let mut it = self.sel.iter();
        let &(mut x0, mut z0) = it.next()?;
        let (mut x1, mut z1) = (x0, z0);
        for &(x, z) in it {
            x0 = x0.min(x);
            z0 = z0.min(z);
            x1 = x1.max(x);
            z1 = z1.max(z);
        }
        Some(((x0, z0), (x1, z1)))
    }

    /// Pinta TODAS las celdas elegidas con ese tile.
    fn sel_rellenar(&mut self, tile: Tile) {
        let mut map = self.override_map();
        for &c in &self.sel {
            map.insert(c, tile);
        }
        let n = self.sel.len();
        self.commit_map(map);
        self.status = format!("{n} celdas pintadas");
    }

    /// Saca el override de terreno de lo elegido (vuelve a lo que genera el
    /// mundo) y se lleva las estructuras que toque.
    fn sel_quitar_pintado(&mut self) {
        let celdas: Vec<(i64, i64)> = self.sel.iter().copied().collect();
        let mut map = self.override_map();
        for c in &celdas {
            map.remove(c);
        }
        self.commit_map(map);
        for &(x, z) in &celdas {
            self.erase(x, z);
        }
        self.status = format!("{} celdas devueltas al mundo", celdas.len());
    }

    /// Planta el asset elegido en la selección, una cada `paso` celdas (paso 1 =
    /// todas; 3 = una hilera rala, para arbolado o farolas).
    fn sel_plantar(&mut self, ctx: &egui::Context, paso: i64) {
        let paso = paso.max(1);
        let celdas: Vec<(i64, i64)> = self
            .sel
            .iter()
            .copied()
            .filter(|&(x, z)| x.rem_euclid(paso) == 0 && z.rem_euclid(paso) == 0)
            .collect();
        for (x, z) in &celdas {
            self.place_decor(ctx, *x, *z);
        }
        self.status = format!("{} assets plantados", celdas.len());
    }

    /// Saca assets y decor procedural de todo lo elegido.
    fn sel_borrar_decor(&mut self) {
        let celdas: Vec<(i64, i64)> = self.sel.iter().copied().collect();
        for (x, z) in &celdas {
            self.erase_decor(*x, *z);
        }
        self.status = format!("{} celdas limpiadas de assets", celdas.len());
    }

    /// Copia lo elegido: el terreno tal como se VE (aunque sea procedural) y los
    /// assets anclados adentro.
    fn sel_copiar(&mut self) {
        let Some(((x0, z0), (x1, z1))) = self.sel_bbox() else {
            self.status = "no hay nada seleccionado para copiar".into();
            return;
        };
        let b = self.body as u8;
        let celdas: Vec<(i64, i64)> = self.sel.iter().copied().collect();
        let tiles = celdas
            .iter()
            .map(|&(x, z)| ((x - x0, z - z0), self.effective_tile(x, z)))
            .collect();
        let decor = self
            .edits
            .decor
            .iter()
            .filter(|d| d.body == b && d.z == self.z && self.sel.contains(&(d.x, d.zc)))
            .map(|d| ((d.x - x0, d.zc - z0), d.clone()))
            .collect();
        self.clip = Some(Portapapeles { tiles, decor, ancho: x1 - x0 + 1, alto: z1 - z0 + 1 });
        self.status = format!("copiadas {} celdas ({}×{})", celdas.len(), x1 - x0 + 1, z1 - z0 + 1);
    }

    /// Pega lo copiado con su esquina mínima en `dest`, y deja seleccionado lo
    /// pegado (así se puede seguir retocando sin volver a marcarlo).
    fn sel_pegar(&mut self, dest: (i64, i64)) {
        let Some(clip) = self.clip.clone() else {
            self.status = "no hay nada copiado (Ctrl+C sobre una selección)".into();
            return;
        };
        let b = self.body as u8;
        let mut map = self.override_map();
        let mut nuevas = std::collections::HashSet::new();
        for ((dx, dz), tile) in &clip.tiles {
            let c = (dest.0 + dx, dest.1 + dz);
            map.insert(c, *tile);
            nuevas.insert(c);
        }
        self.commit_map(map);
        for ((dx, dz), d) in &clip.decor {
            let (x, zc) = (dest.0 + dx, dest.1 + dz);
            self.edits.decor.retain(|o| !(o.body == b && o.z == self.z && o.x == x && o.zc == zc));
            let mut nuevo = d.clone();
            nuevo.body = b;
            nuevo.z = self.z;
            nuevo.x = x;
            nuevo.zc = zc;
            self.edits.decor.push(nuevo);
        }
        self.sel = nuevas;
        self.status = format!(
            "pegado en ({}, {}) — {} celdas, {} assets",
            dest.0,
            dest.1,
            clip.tiles.len(),
            clip.decor.len()
        );
    }

    /// Convierte la selección en una ZONA (su rectángulo envolvente): así se
    /// dibuja un barrio con la varita sobre el casco en vez de a ojo.
    fn sel_zona(&mut self) {
        let Some((min, max)) = self.sel_bbox() else { return };
        self.crear_zona(min, max);
    }
}

/// Test par-impar de punto en polígono (el mismo que usa el editor de assets
/// para la pluma de la huella).
fn punto_en_poly(p: (f32, f32), v: &[(f32, f32)]) -> bool {
    let mut dentro = false;
    let n = v.len();
    if n < 3 {
        return false;
    }
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

/// Tamaño en metros que el juego le da al decor PROCEDURAL de esa clave (mismas
/// tablas que `worldgen`), para dibujarlo del porte correcto en el editor.
fn procedural_size(key: &str) -> Option<[f32; 2]> {
    ha_shared::worldgen::TREE_SPECIES
        .iter()
        .find(|t| t.key == key)
        .map(|t| t.size)
        .or_else(|| {
            ha_shared::worldgen::NATURE_SPECIES
                .iter()
                .chain(ha_shared::worldgen::PROP_SPECIES.iter())
                .find(|e| e.key == key)
                .map(|e| e.size)
        })
        .or(if key.contains("rock") { Some([1.3, 1.0]) } else { None })
}

fn tile_color(t: Tile) -> egui::Color32 {
    let [r, g, b] = t.color();
    egui::Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

fn load_texture(ctx: &egui::Context, path: &Path, id: &str) -> Option<egui::TextureHandle> {
    let img = image::open(path).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    let color = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw());
    Some(ctx.load_texture(id, color, egui::TextureOptions::NEAREST))
}

impl eframe::App for WorldApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui(ctx);
    }
}

impl WorldApp {
    /// Dibuja este espacio de trabajo. Lo llama el shell del Studio
    /// (`studio.rs`) o `eframe` cuando el editor corre solo.
    pub(crate) fn ui(&mut self, ctx: &egui::Context) {
        // Una vez por frame, antes de dibujar nada: es más barato que rastrear los
        // ocho sitios que mutan `edits.decor` (colocar, borrar, deshacer, cargar,
        // cambiar de cuerpo o de capa) y olvidarse de uno.
        self.rebuild_solar();
        self.top_bar(ctx);
        self.tools_panel(ctx);
        self.palette_panel(ctx);
        // El menú va ANTES del lienzo a propósito: el clic derecho que lo abre
        // lo procesa el lienzo, así que el menú nace recién al frame siguiente y
        // el "clic afuera cierra" no lo mata en el mismo clic que lo creó.
        self.menu_flotante(ctx);
        self.canvas(ctx);
    }
}

impl WorldApp {
    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("world_top").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong("Editor de Mundo");
                ui.separator();
                ui.label("cuerpo:");
                for &b in CelestialBody::all() {
                    ui.selectable_value(&mut self.body, b, b.label());
                }
                ui.separator();
                // CAPA (z-level): subsuelo (<0) · superficie (0) · pisos (>0). Navegación
                // clara por capas para acomodar cada cosa en la que toca (E1).
                ui.label("capa:");
                if ui.small_button("▼").on_hover_text("bajar capa (subsuelo)").clicked() {
                    self.z = (self.z - 1).max(-8);
                }
                let cap = if self.z < 0 {
                    format!("subsuelo {}", self.z)
                } else if self.z == 0 {
                    "superficie".to_string()
                } else {
                    format!("piso {}", self.z)
                };
                ui.monospace(cap);
                if ui.small_button("▲").on_hover_text("subir capa (piso)").clicked() {
                    self.z = (self.z + 1).min(4);
                }
                if ui.small_button("⌂").on_hover_text("ir a superficie (0)").clicked() {
                    self.z = 0;
                }
                ui.add(egui::DragValue::new(&mut self.z).range(-8..=4));
                ui.separator();
                if ui.button("💾 Guardar").clicked() {
                    self.save();
                }
                if ui
                    .button("↻ assets")
                    .on_hover_text("relee art/manifest.json — trae lo que acabás de crear en el editor de assets")
                    .clicked()
                {
                    self.load_art();
                }
                if ui
                    .add_enabled(!self.undo.is_empty(), egui::Button::new("↶ deshacer"))
                    .on_hover_text("Ctrl+Z")
                    .clicked()
                {
                    if let Some(prev) = self.undo.pop() {
                        self.edits = prev;
                    }
                }
                ui.checkbox(&mut self.show_art, "ver sprites")
                    .on_hover_text("dibuja el arte real (WYSIWYG) en vez de puntos de color");
                ui.checkbox(&mut self.show_collision, "ver colisión")
                    .on_hover_text(
                        "amarillo = con qué te vas a chocar en el juego (la huella que \
                         pintaste en el editor de assets). Sin amarillo, el asset se pisa.",
                    );
                // Inspector: qué va a ver el juego exactamente en esa celda.
                if let Some((x, zc)) = self.hover_cell {
                    ui.separator();
                    let tile = self.effective_tile(x, zc);
                    let pintado = if self.has_tile_override(x, zc) { " ✎" } else { "" };
                    let decor = match self.effective_decor(x, zc) {
                        Some(k) => format!(" · {k}"),
                        None if self.is_cleared(x, zc) => " · (limpiada)".to_string(),
                        None => String::new(),
                    };
                    ui.monospace(format!("({x}, {zc}) {tile:?}{pintado}{decor}"));
                }
                ui.separator();
                ui.label(format!("{:.0}px", self.px_per_tile));
            });
            if !self.status.is_empty() {
                ui.label(egui::RichText::new(&self.status).color(egui::Color32::LIGHT_GREEN).small());
            }
        });
        // Ctrl+Z global.
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Z)) {
            if let Some(prev) = self.undo.pop() {
                self.edits = prev;
                self.status = "deshecho".into();
            }
        }
        // Atajos de la selección — los mismos que en cualquier editor, para no
        // tener que ir al menú por lo que se hace cien veces por sesión.
        let (ctrl, esc, copiar, pegar, borrar, rellenar) = ctx.input(|i| {
            let m = i.modifiers.ctrl || i.modifiers.command;
            (
                m,
                i.key_pressed(egui::Key::Escape),
                m && i.key_pressed(egui::Key::C),
                m && i.key_pressed(egui::Key::V),
                i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace),
                !m && i.key_pressed(egui::Key::Enter),
            )
        });
        let _ = ctrl;
        if esc && self.menu.is_none() && !self.sel.is_empty() {
            self.sel.clear();
            self.status = "selección limpiada".into();
        }
        if !self.sel.is_empty() {
            if copiar {
                self.sel_copiar();
            }
            if borrar {
                self.push_undo();
                self.sel_quitar_pintado();
            }
            if rellenar {
                self.push_undo();
                let t = self.paint_tile;
                self.sel_rellenar(t);
            }
        }
        if pegar {
            if let Some(cell) = self.hover_cell {
                self.push_undo();
                self.sel_pegar(cell);
            }
        }
    }

    /// **Crea una zona con el rectángulo que acabás de arrastrar.**
    ///
    /// Normaliza el arrastre (se puede dibujar de cualquier esquina a cualquier
    /// otra) y descarta los accidentes: un clic sin arrastre no es una zona, es un
    /// clic, y una zona de dos tiles no cubre ni media manzana — dejarla entrar
    /// llenaría la lista de rectángulos invisibles imposibles de volver a tocar.
    fn crear_zona(&mut self, a: (i64, i64), b: (i64, i64)) {
        let min = (a.0.min(b.0), a.1.min(b.1));
        let size = ((a.0 - b.0).abs() + 1, (a.1 - b.1).abs() + 1);
        if size.0 < MIN_ZONA || size.1 < MIN_ZONA {
            self.status = format!("zona demasiado chica (mínimo {MIN_ZONA}×{MIN_ZONA} tiles)");
            return;
        }
        let nombre = if self.zona_nombre.trim().is_empty() {
            String::new()
        } else {
            self.zona_nombre.trim().to_string()
        };
        self.edits.zonas.push(Zona {
            nombre,
            body: self.body as u8,
            min,
            size,
            clase: self.zona_clase,
        });
        self.sync_zonas();
        self.status = format!(
            "zona {} de {}×{} creada",
            self.zona_clase.label(),
            size.0,
            size.1
        );
    }

    /// **Deja el generador al día con lo que acabás de dibujar.**
    ///
    /// El lienzo rasteriza el MISMO mundo que el juego (`topdown_tile`), y el
    /// generador de ciudad consulta las zonas por dentro. Reinstalarlas después
    /// de cada cambio es lo que hace que pintar un barrio comercial reordene los
    /// edificios delante de tus ojos, en vez de "guardá, abrí el juego y fijate".
    fn sync_zonas(&self) {
        ha_shared::zonas::instalar(Zonas::new(self.edits.zonas.clone()));
    }

    /// Panel de la herramienta de zonas: qué clase estás por pintar, y la lista
    /// de las que ya hay.
    fn zonas_panel(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Clase de la próxima zona").strong());
        for c in ClaseZona::TODAS {
            let [r, g, b] = c.color();
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                ui.painter().rect_filled(
                    rect,
                    2.0,
                    egui::Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8),
                );
                ui.selectable_value(&mut self.zona_clase, c, c.label())
                    .on_hover_text(c.ayuda());
            });
        }
        ui.horizontal(|ui| {
            ui.label("nombre:");
            ui.text_edit_singleline(&mut self.zona_nombre);
        });
        ui.label(
            egui::RichText::new(
                "Arrastrá sobre el mapa para dibujarla. Una zona DENTRO de otra manda \
                 sobre ella: así se hacen las sub-zonas.",
            )
            .weak()
            .small(),
        );
        ui.separator();
        ui.label(egui::RichText::new(format!("Zonas ({})", self.edits.zonas.len())).strong());
        let mut borrar: Option<usize> = None;
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            for (i, z) in self.edits.zonas.iter().enumerate() {
                ui.horizontal(|ui| {
                    let [r, g, b] = z.clase.color();
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                    ui.painter().rect_filled(
                        rect,
                        2.0,
                        egui::Color32::from_rgb(
                            (r * 255.0) as u8,
                            (g * 255.0) as u8,
                            (b * 255.0) as u8,
                        ),
                    );
                    let nombre = if z.nombre.is_empty() { z.clase.label() } else { &z.nombre };
                    ui.label(format!("{nombre}  {}×{}", z.size.0, z.size.1))
                        .on_hover_text(format!(
                            "{} · esquina ({}, {})",
                            z.clase.label(),
                            z.min.0,
                            z.min.1
                        ));
                    if ui.small_button("✖").on_hover_text("borrar zona").clicked() {
                        borrar = Some(i);
                    }
                });
            }
        });
        if let Some(i) = borrar {
            self.push_undo();
            self.edits.zonas.remove(i);
            self.sync_zonas();
            self.status = "zona borrada".into();
        }
    }

    /// **El menú del clic derecho.** Todo lo que se le puede hacer a lo que
    /// acabás de tocar, ahí mismo, sin ir a buscar la herramienta al panel: es
    /// el atajo que convierte "pintar celda por celda" en "elegir y resolver".
    /// Actúa sobre la SELECCIÓN; si no hay, sobre la celda del clic (que el
    /// canvas ya dejó seleccionada).
    fn menu_flotante(&mut self, ctx: &egui::Context) {
        let Some((pos, cell)) = self.menu else { return };
        let mut cerrar = false;
        let tile_actual = self.paint_tile;
        let nombre_tile = PAINT_TILES
            .iter()
            .find(|(t, _)| *t == tile_actual)
            .map(|(_, n)| *n)
            .unwrap_or("tile");
        let asset = if self.decor_key.is_empty() {
            "(ninguno)".to_string()
        } else {
            slot_label(&self.decor_key)
        };
        let hay_clip = self.clip.is_some();
        let clip_tam = self
            .clip
            .as_ref()
            .map(|c| format!(" ({}×{})", c.ancho, c.alto))
            .unwrap_or_default();
        let area = egui::Area::new(egui::Id::new("menu_tile"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_max_width(300.0);
                    let tile = self.effective_tile(cell.0, cell.1);
                    ui.label(
                        egui::RichText::new(format!(
                            "({}, {}) · {tile:?} · ⬚ {} celdas",
                            cell.0,
                            cell.1,
                            self.sel.len()
                        ))
                        .small()
                        .weak()
                        .monospace(),
                    );
                    ui.separator();
                    if ui.button(format!("💧 Copiar este tile ({tile:?})")).clicked() {
                        self.paint_tile = tile;
                        self.status = format!("pincel = {tile:?}");
                        cerrar = true;
                    }
                    if ui
                        .button(format!("🎨 Rellenar con «{nombre_tile}»"))
                        .on_hover_text("pinta todas las celdas seleccionadas")
                        .clicked()
                    {
                        self.push_undo();
                        self.sel_rellenar(tile_actual);
                        cerrar = true;
                    }
                    if ui
                        .button("🧽 Quitar lo pintado")
                        .on_hover_text("devuelve esas celdas al mundo procedural")
                        .clicked()
                    {
                        self.push_undo();
                        self.sel_quitar_pintado();
                        cerrar = true;
                    }
                    ui.separator();
                    if ui.button(format!("🌳 Plantar «{asset}»")).clicked() {
                        self.push_undo();
                        self.sel_plantar(ctx, 1);
                        cerrar = true;
                    }
                    if ui
                        .button(format!("🌲 Plantar «{asset}» cada 3 celdas"))
                        .on_hover_text("arbolado / farolas: una hilera rala en vez de un muro")
                        .clicked()
                    {
                        self.push_undo();
                        self.sel_plantar(ctx, 3);
                        cerrar = true;
                    }
                    if ui.button("🍃 Borrar assets y árboles").clicked() {
                        self.push_undo();
                        self.sel_borrar_decor();
                        cerrar = true;
                    }
                    ui.separator();
                    ui.label(egui::RichText::new("SELECCIÓN").small().strong());
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .button("✨ contiguas")
                            .on_hover_text("la mancha pegada a esta celda con el mismo terreno")
                            .clicked()
                        {
                            self.sel_varita(cell.0, cell.1, SelOp::Reemplazar);
                        }
                        if ui
                            .button("≡ iguales a la vista")
                            .on_hover_text("todas las celdas del mismo terreno que se ven")
                            .clicked()
                        {
                            self.sel_similares(cell.0, cell.1, SelOp::Reemplazar);
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .button("⬒ sólo el contorno")
                            .on_hover_text("deja el borde: así se hace un cordón o una pared")
                            .clicked()
                        {
                            self.sel_contorno();
                        }
                        if ui.button("⊕ expandir").clicked() {
                            self.sel_dilatar(1);
                        }
                        if ui.button("⊖ contraer").clicked() {
                            self.sel_dilatar(-1);
                        }
                    });
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("⧉ Copiar").on_hover_text("Ctrl+C").clicked() {
                            self.sel_copiar();
                            cerrar = true;
                        }
                        if ui
                            .add_enabled(hay_clip, egui::Button::new(format!("📋 Pegar acá{clip_tam}")))
                            .on_hover_text("Ctrl+V — la esquina del bloque cae en esta celda")
                            .clicked()
                        {
                            self.push_undo();
                            self.sel_pegar(cell);
                            cerrar = true;
                        }
                    });
                    if ui
                        .button("🗺 Crear zona con esto")
                        .on_hover_text("usa el rectángulo que envuelve a la selección")
                        .clicked()
                    {
                        self.push_undo();
                        self.sel_zona();
                        cerrar = true;
                    }
                    if ui.button("✖ Limpiar selección").clicked() {
                        self.sel.clear();
                        cerrar = true;
                    }
                });
            });
        // Se cierra al elegir algo, con Esc, o al clickear afuera.
        let fuera = ctx.input(|i| i.pointer.any_click())
            && ctx
                .pointer_interact_pos()
                .is_some_and(|p| !area.response.rect.contains(p));
        if cerrar || fuera || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.menu = None;
        }
    }

    fn tools_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("world_tools").resizable(false).show(ctx, |ui| {
            ui.heading("Herramienta");
            ui.selectable_value(&mut self.tool, Tool::Mover, "✋ Mover/zoom");
            ui.selectable_value(&mut self.tool, Tool::Seleccionar, "⬚ Seleccionar")
                .on_hover_text(
                    "Elegí celdas y hacéles algo a todas juntas. Clic izquierdo copia el \
                     tile; clic derecho abre el menú.",
                );
            ui.selectable_value(&mut self.tool, Tool::PintarTile, "🖌 Pintar (pincel)");
            ui.selectable_value(&mut self.tool, Tool::RellenarRect, "▭ Rellenar rectángulo");
            ui.selectable_value(&mut self.tool, Tool::Bote, "🪣 Bote (relleno por terreno)");
            ui.selectable_value(&mut self.tool, Tool::Cuentagotas, "💧 Cuentagotas");
            ui.selectable_value(&mut self.tool, Tool::ColocarDecor, "🌳 Colocar asset");
            ui.selectable_value(&mut self.tool, Tool::BorrarDecor, "🍃 Borrar asset/árbol");
            ui.selectable_value(&mut self.tool, Tool::ColocarEstructura, "🏛 Estructura grande");
            ui.selectable_value(&mut self.tool, Tool::Borrar, "🧽 Borrar terreno");
            ui.selectable_value(&mut self.tool, Tool::Zona, "🗺 Zonas (barrios)");
            ui.separator();
            if self.tool == Tool::Zona {
                self.zonas_panel(ui);
            }
            if self.tool == Tool::Seleccionar {
                ui.label(egui::RichText::new("Cómo elegís").strong());
                for m in [SelMode::Rect, SelMode::Libre, SelMode::Varita, SelMode::Similares] {
                    ui.selectable_value(&mut self.sel_mode, m, m.label()).on_hover_text(m.ayuda());
                }
                ui.label(
                    egui::RichText::new(
                        "Shift suma · Ctrl resta · Esc limpia\n\
                         Clic IZQUIERDO = copiar el tile de esa celda\n\
                         Clic DERECHO = menú con todo lo que se le puede hacer",
                    )
                    .weak()
                    .small(),
                );
            }
            // La máscara vale para TODAS las herramientas de pintura, no sólo
            // mientras estás seleccionando: elegís una vez y después pintás
            // tranquilo sabiendo que no te vas a desbordar.
            if !self.sel.is_empty() {
                ui.separator();
                ui.label(
                    egui::RichText::new(format!("⬚ {} celdas seleccionadas", self.sel.len()))
                        .color(egui::Color32::from_rgb(120, 210, 255))
                        .strong(),
                );
                ui.checkbox(&mut self.sel_limita, "🔒 pintar sólo adentro").on_hover_text(
                    "Como en cualquier editor de imagen: con la selección puesta, el pincel, \
                     el bote y el rectángulo no salen de ella.",
                );
                if ui.button("✖ limpiar selección (Esc)").clicked() {
                    self.sel.clear();
                }
            }
            // Tamaño de pincel (pintar/borrar) — diseñá rápido, no cuadrito por cuadrito.
            if matches!(self.tool, Tool::PintarTile | Tool::Borrar) {
                ui.horizontal(|ui| {
                    ui.label("pincel:");
                    ui.add(egui::DragValue::new(&mut self.brush).range(1..=6));
                    ui.label(format!("{0}×{0}", self.brush * 2 - 1));
                });
            }
            // Paleta de tiles (para las herramientas que pintan tile).
            if matches!(self.tool, Tool::PintarTile | Tool::RellenarRect | Tool::Bote) {
                ui.label("Tile:");
                for &(t, name) in PAINT_TILES {
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 2.0, tile_color(t));
                        ui.selectable_value(&mut self.paint_tile, t, name);
                    });
                }
                ui.label(
                    egui::RichText::new(
                        "Lo que pintás REEMPLAZA al mundo: en esas celdas ya no crecen \
                         árboles ni rocas procedurales.",
                    )
                    .weak()
                    .small(),
                );
            }
            #[allow(clippy::single_match)]
            match self.tool {
                // Zonas y selección ya tienen su propio panel arriba.
                Tool::Zona | Tool::Seleccionar => {}
                Tool::ColocarEstructura => {
                    ui.label("Asset grande:");
                    for i in 0..large_asset_count() {
                        if let Some(a) = large_asset(i) {
                            ui.selectable_value(&mut self.place_idx, i, a.label);
                        }
                    }
                    ui.label(egui::RichText::new("(gris hasta asignarle textura y huella en el editor de assets)").weak().small());
                }
                Tool::ColocarDecor => {
                    ui.label(
                        egui::RichText::new(
                            "Elegí un asset en la paleta (derecha) y clickeá/arrastrá en el mapa.",
                        )
                        .weak()
                        .small(),
                    );
                    ui.horizontal(|ui| {
                        ui.label("escala:");
                        ui.add(
                            egui::DragValue::new(&mut self.decor_scale)
                                .range(0.2..=8.0)
                                .speed(0.05),
                        )
                        .on_hover_text(
                            "1 = el tamaño que da su recorte (una casa de 5×4 celdas mide 5×4 tiles)",
                        );
                    });
                    ui.separator();
                    ui.checkbox(&mut self.decor_stamps_ground, "🧱 reemplaza el suelo")
                        .on_hover_text(
                            "El asset estampa su SOLAR: borra el terreno de todas las celdas \
                             que ocupa y lo reemplaza por el tile elegido. El solar se adapta \
                             solo al tamaño del asset — una universidad tapa lo suyo, una \
                             maceta no toca nada. Sin tildar, el asset se dibuja encima y el \
                             terreno queda como estaba.",
                        );
                    if self.decor_stamps_ground {
                        ui.label("suelo del solar:");
                        for &(t, name) in PAINT_TILES {
                            ui.horizontal(|ui| {
                                let (rect, _) = ui
                                    .allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                                ui.painter().rect_filled(rect, 2.0, tile_color(t));
                                ui.selectable_value(&mut self.decor_ground_tile, t, name);
                            });
                        }
                    }
                }
                Tool::BorrarDecor => {
                    ui.label(
                        egui::RichText::new(
                            "Clic = saca el asset de esa celda. Si ahí crecía un árbol del \
                             mundo, la celda queda marcada y el juego tampoco lo generará.",
                        )
                        .weak()
                        .small(),
                    );
                }
                Tool::Bote => {
                    ui.label(egui::RichText::new("Clic = rellena el TERRENO contiguo del mismo tipo (lagos, campos, playas).").weak().small());
                }
                Tool::RellenarRect => {
                    ui.label(egui::RichText::new("Arrastrá para marcar el rectángulo; soltá para rellenar.").weak().small());
                }
                Tool::Cuentagotas => {
                    ui.label(egui::RichText::new("Clic = copia el tile de esa celda al pincel (y pasa a pintar).").weak().small());
                }
                Tool::Mover => {
                    ui.label(egui::RichText::new("Arrastrá para desplazar, rueda para zoom.").weak().small());
                }
                Tool::Borrar => {
                    ui.label(egui::RichText::new("Clic/arrastre borra tu override de terreno (vuelve a lo procedural).").weak().small());
                }
                Tool::PintarTile => {}
            }
            ui.separator();
            ui.label(egui::RichText::new(format!(
                "overrides: {} tiles · {} assets · {} estructuras · {} celdas limpias",
                self.edits.tiles.len(),
                self.edits.decor.len(),
                self.edits.structures.len(),
                self.edits.cleared.len(),
            )).small());
            ui.label(egui::RichText::new("Rueda del medio = mover en cualquier herramienta.").weak().small());
        });
    }

    /// Paleta de assets: TODO lo que tenga arte en `art/manifest.json`, con la
    /// miniatura real. Es el puente con el editor de assets — lo que creás allá
    /// se coloca acá.
    fn palette_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("world_palette")
            .resizable(true)
            .default_width(240.0)
            .width_range(160.0..=420.0)
            .show(ctx, |ui| {
                ui.heading("Assets");
                if self.palette.is_empty() {
                    ui.label(
                        egui::RichText::new(
                            "Todavía no hay assets con arte. Abrí el editor de assets \
                             (`ha-asset-tools gui`), asignales PNG, y tocá '↻ assets'.",
                        )
                        .weak(),
                    );
                    return;
                }
                ui.horizontal(|ui| {
                    ui.label("🔎");
                    ui.text_edit_singleline(&mut self.palette_filter);
                });
                if !self.decor_key.is_empty() {
                    ui.label(
                        egui::RichText::new(format!("elegido: {}", slot_label(&self.decor_key)))
                            .small(),
                    );
                }
                ui.separator();
                let f = self.palette_filter.to_lowercase();
                // Agrupar por categoría respetando el filtro. El filtro mira el
                // NOMBRE primero (se busca "farol", no "prop/farol").
                let mut groups: Vec<(String, Vec<(String, String)>)> = Vec::new();
                for p in &self.palette {
                    if !f.is_empty()
                        && !p.label.to_lowercase().contains(&f)
                        && !p.key.to_lowercase().contains(&f)
                    {
                        continue;
                    }
                    let row = (p.key.clone(), p.label.clone());
                    match groups.last_mut() {
                        Some((c, v)) if *c == p.category => v.push(row),
                        _ => groups.push((p.category.clone(), vec![row])),
                    }
                }
                let mut pick: Option<String> = None;
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (cat, keys) in &groups {
                        egui::CollapsingHeader::new(
                            egui::RichText::new(format!(
                                "{} ({})",
                                ha_assets::category_label(cat),
                                keys.len()
                            ))
                            .strong()
                            .color(egui::Color32::from_rgb(120, 210, 255)),
                        )
                        .default_open(f.is_empty() && groups.len() <= 3 || !f.is_empty())
                        .id_salt(cat)
                        .show(ui, |ui| {
                            for (key, label) in keys {
                                let selected = self.decor_key == *key;
                                let resp = ui.horizontal(|ui| {
                                    // Miniatura real recortada de su hoja.
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(28.0, 28.0),
                                        egui::Sense::hover(),
                                    );
                                    let painter = ui.painter().clone();
                                    if !self.draw_slot(ctx, &painter, key, rect, 1.0) {
                                        painter.rect_filled(rect, 2.0, egui::Color32::from_gray(60));
                                    }
                                    ui.selectable_label(selected, label)
                                        .on_hover_text(key.as_str())
                                        .clicked()
                                });
                                if resp.inner {
                                    pick = Some(key.clone());
                                }
                            }
                        });
                    }
                });
                if let Some(k) = pick {
                    self.decor_key = k;
                    // Elegir un asset implica querer plantarlo.
                    self.tool = Tool::ColocarDecor;
                }
            });
    }

    fn canvas(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let (resp, painter) =
                ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
            let rect = resp.rect;

            // Pan: herramienta Mover (arrastre) o botón del medio (siempre).
            let mid_down = ui.input(|i| i.pointer.middle_down());
            if (self.tool == Tool::Mover && resp.dragged()) || mid_down {
                let d = if mid_down { ui.input(|i| i.pointer.delta()) } else { resp.drag_delta() };
                self.center -= d / self.px_per_tile;
            }
            // Zoom por rueda.
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll.abs() > 0.0 {
                self.px_per_tile = (self.px_per_tile * (scroll * 0.0015).exp()).clamp(2.0, 48.0);
            }

            // Rango visible.
            let cols = (rect.width() / self.px_per_tile).ceil() as i64 + 2;
            let rows = (rect.height() / self.px_per_tile).ceil() as i64 + 2;
            let x0 = self.center.x.round() as i64 - cols / 2;
            let z0 = self.center.y.round() as i64 - rows / 2;
            // Lo que se ve ahora: "seleccionar todas las iguales" trabaja sobre
            // esto (el mundo es infinito, "todo el pasto" no termina nunca).
            self.vista = (x0, z0, x0 + cols - 1, z0 + rows - 1);
            let center = rect.center();
            let ppt = self.px_per_tile;
            let cx0 = self.center.x;
            let cz0 = self.center.y;
            let screen_of = move |tx: f32, tz: f32| -> egui::Pos2 {
                egui::pos2(center.x + (tx - cx0) * ppt, center.y + (tz - cz0) * ppt)
            };

            // Overrides de tile del cuerpo/z actual → lookup rápido.
            let b = self.body as u8;
            let overrides: HashMap<(i64, i64), Tile> = self
                .edits
                .tiles
                .iter()
                .filter(|t| t.body == b && t.z == self.z)
                .map(|t| ((t.x, t.zc), t.tile))
                .collect();
            let cleared: std::collections::HashSet<(i64, i64)> = self
                .edits
                .cleared
                .iter()
                .filter(|c| c.body == b && c.z == self.z)
                .map(|c| (c.x, c.zc))
                .collect();
            let authored: HashMap<(i64, i64), (String, Option<[f32; 2]>)> = self
                .edits
                .decor
                .iter()
                .filter(|d| d.body == b && d.z == self.z)
                .map(|d| ((d.x, d.zc), (d.key.clone(), d.size)))
                .collect();

            painter.rect_filled(rect, 0.0, egui::Color32::from_gray(12));
            let sz = egui::vec2(ppt + 1.0, ppt + 1.0);
            // Sprites a dibujar en un 2º pase (necesitan `&mut self` para cachear
            // texturas, y acá seguimos leyendo los mapas de arriba).
            // `(clave, celda, ancho en TILES, es_tile)`. El ancho sale del tamaño
            // real del asset: una casa ocupa sus 5 tiles, no uno.
            let mut sprites: Vec<(String, egui::Rect, Option<f32>, bool)> = Vec::new();
            // Assets AUTORADOS: van por la regla de colocación (celda + tamaño),
            // no por el encuadre de la celda — se resuelven en el 2º pase.
            let mut placed: Vec<(String, (i64, i64), Option<[f32; 2]>)> = Vec::new();
            for dz in 0..rows {
                for dx in 0..cols {
                    let (tx, tz) = (x0 + dx, z0 + dz);
                    let painted = overrides.get(&(tx, tz)).copied();
                    let t = painted.unwrap_or_else(|| topdown_tile(self.body, self.z, tx, tz));
                    let cell = egui::Rect::from_min_size(screen_of(tx as f32, tz as f32), sz);
                    painter.rect_filled(cell, 0.0, tile_color(t));
                    if self.show_art && ppt >= 8.0 {
                        if let Some(k) = tile_art_key(t).filter(|k| self.art.get(k).is_some()) {
                            sprites.push((k.to_string(), cell, Some(1.0), true));
                        }
                    }
                    // Marca sutil los overrides (borde) para distinguir lo autorado.
                    if painted.is_some() {
                        painter.rect_stroke(cell, 0.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(255, 230, 120)));
                    }
                    // SELECCIÓN: relleno tenue y borde SÓLO en los lados que dan
                    // afuera. Así se ve el contorno de la forma elegida y no una
                    // reja de cuadraditos que tapa el mapa que estás editando.
                    if !self.sel.is_empty() && self.sel.contains(&(tx, tz)) {
                        painter.rect_filled(
                            cell,
                            0.0,
                            egui::Color32::from_rgba_unmultiplied(90, 200, 255, 46),
                        );
                        let st = egui::Stroke::new(1.5, egui::Color32::from_rgb(120, 220, 255));
                        let (a, b) = (cell.min, egui::pos2(cell.min.x + ppt, cell.min.y + ppt));
                        if !self.sel.contains(&(tx, tz - 1)) {
                            painter.line_segment([a, egui::pos2(b.x, a.y)], st);
                        }
                        if !self.sel.contains(&(tx, tz + 1)) {
                            painter.line_segment([egui::pos2(a.x, b.y), b], st);
                        }
                        if !self.sel.contains(&(tx - 1, tz)) {
                            painter.line_segment([a, egui::pos2(a.x, b.y)], st);
                        }
                        if !self.sel.contains(&(tx + 1, tz)) {
                            painter.line_segment([egui::pos2(b.x, a.y), b], st);
                        }
                    }
                    if ppt < 4.0 {
                        continue;
                    }
                    // DECOR: exactamente lo que verá el juego. El autorado gana; el
                    // procedural NO aparece donde pintaste ni donde borraste (misma
                    // regla que `world2d::spawn_chunk_decor`), así que si tapás el
                    // bosque con pasto lo ves desaparecer acá mismo.
                    let entry = match authored.get(&(tx, tz)) {
                        Some((k, s)) => Some((k.clone(), s.map(|s| s[0]))),
                        None if painted.is_some() || cleared.contains(&(tx, tz)) || t.solid() => None,
                        None => self
                            .procedural_decor_at(tx, tz)
                            .map(|(k, _)| (k.clone(), procedural_size(&k).map(|s| s[0]))),
                    };
                    let Some((key, width_tiles)) = entry else { continue };
                    let is_authored = authored.contains_key(&(tx, tz));
                    if is_authored {
                        // Lo colocado a mano se dibuja por la regla de colocación
                        // (2º pase): mismo rect que la colisión y que el juego.
                        if self.show_art && ppt >= 6.0 {
                            let size = authored.get(&(tx, tz)).and_then(|(_, s)| *s);
                            placed.push((key, (tx, tz), size));
                            continue;
                        }
                    }
                    if self.show_art && ppt >= 6.0 {
                        sprites.push((key, cell, width_tiles, false));
                    } else {
                        let ctr = cell.center();
                        let rr = (ppt * 0.32).clamp(2.0, 8.0);
                        let col = if is_authored {
                            egui::Color32::from_rgb(255, 210, 90)
                        } else if key.contains("tree") {
                            egui::Color32::from_rgb(60, 170, 80)
                        } else if key.contains("rock") {
                            egui::Color32::from_rgb(150, 150, 150)
                        } else {
                            egui::Color32::from_rgb(120, 200, 120)
                        };
                        painter.circle_filled(ctr, rr, col);
                    }
                    if is_authored && ppt >= 6.0 {
                        painter.rect_stroke(
                            cell,
                            0.0,
                            egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(255, 210, 90, 120)),
                        );
                    }
                }
            }
            // 2º pase: los sprites reales. Los tiles van primero (ya vienen en ese
            // orden) y el decor encima.
            for (key, cell, width_tiles, is_tile) in sprites {
                // Sin tamaño conocido, el asset se deduce de su recorte.
                let scale = match width_tiles {
                    Some(w) => w,
                    None => self.slot_size_m(ctx, &key).map(|s| s[0]).unwrap_or(1.5),
                };
                if !self.draw_slot(ctx, &painter, &key, cell, scale) && !is_tile {
                    painter.circle_filled(
                        cell.center(),
                        (ppt * 0.3).clamp(2.0, 8.0),
                        egui::Color32::from_rgb(120, 200, 120),
                    );
                }
            }
            // Assets AUTORADOS: sprite en su rect real + (opcional) su COLISIÓN.
            // Se dibujan en el MISMO orden que el juego (ver `ha-game::layers`):
            // primero los de capa Suelo, después los Y-sorted por su fila del pie,
            // y los Techo al final. Sin esto el editor mentía — mostraba un charco
            // encima de la casa que en el juego iba debajo.
            placed.sort_by(|a, b| {
                self.draw_depth(&a.0, a.1 .1)
                    .partial_cmp(&self.draw_depth(&b.0, b.1 .1))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            for (key, (tx, tz), size) in placed {
                let size = size.filter(|s| s[0] > 0.0 && s[1] > 0.0).unwrap_or_else(|| {
                    self.slot_size_m(ctx, &key).unwrap_or([1.5, 1.5])
                });
                let r = self.placed_tile_rect(ctx, &key, tx, tz, size);
                let dest = egui::Rect::from_min_max(
                    screen_of(r.min.x, r.min.y),
                    screen_of(r.max.x, r.max.y),
                );
                if !self.draw_slot_in(ctx, &painter, &key, dest) {
                    painter.circle_filled(
                        dest.center(),
                        (ppt * 0.3).clamp(2.0, 8.0),
                        egui::Color32::from_rgb(255, 210, 90),
                    );
                }
                if self.show_collision {
                    for cr in self.collision_tile_rects(ctx, &key, tx, tz, size) {
                        let sr = egui::Rect::from_min_max(
                            screen_of(cr.min.x, cr.min.y),
                            screen_of(cr.max.x, cr.max.y),
                        );
                        painter.rect_filled(
                            sr,
                            0.0,
                            egui::Color32::from_rgba_unmultiplied(255, 210, 90, 60),
                        );
                    }
                }
            }
            // Estructuras del cuerpo/z: su sprite si lo tiene, si no el rectángulo.
            let structures: Vec<(String, egui::Rect)> = self
                .edits
                .structures
                .iter()
                .filter(|s| s.body == b && s.z == self.z)
                .map(|s| {
                    (
                        s.art.clone(),
                        egui::Rect::from_min_max(
                            screen_of(s.cx as f32, s.cz as f32),
                            screen_of((s.cx + s.w) as f32, (s.cz + s.h) as f32),
                        ),
                    )
                })
                .collect();
            for (art, r) in structures {
                if !(self.show_art && self.draw_slot(ctx, &painter, &art, r, 1.0)) {
                    painter.rect_filled(r, 0.0, egui::Color32::from_rgba_unmultiplied(120, 120, 130, 180));
                    if ppt >= 8.0 {
                        let name = art.rsplit('/').next().unwrap_or(&art).to_string();
                        painter.text(r.center(), egui::Align2::CENTER_CENTER, name, egui::FontId::proportional(11.0), egui::Color32::WHITE);
                    }
                }
                painter.rect_stroke(r, 0.0, egui::Stroke::new(1.5, egui::Color32::from_rgb(60, 200, 220)));
            }

            // Celda bajo el cursor.
            let hover = resp.hover_pos().map(|pos| {
                let tx = ((pos.x - center.x) / ppt + cx0).floor() as i64;
                let tz = ((pos.y - center.y) / ppt + cz0).floor() as i64;
                (tx, tz)
            });
            self.hover_cell = hover;

            // Fantasma del asset bajo el cursor: el sprite EN SU SITIO (la misma
            // cuenta que hace el juego al plantarlo) y en amarillo su COLISIÓN
            // real, no un cuadrito en el tile. Lo que ves acá es lo que va a
            // quedar y con lo que te vas a chocar.
            if let (Tool::ColocarDecor, Some((tx, tz))) = (self.tool, hover) {
                let key = self.decor_key.clone();
                if !key.is_empty() {
                    let scale = self.decor_scale.max(0.1);
                    let nat = self.slot_size_m(ctx, &key).unwrap_or([1.5, 1.5]);
                    let size = [nat[0] * scale, nat[1] * scale];
                    let r = self.placed_tile_rect(ctx, &key, tx, tz, size);
                    let dest = egui::Rect::from_min_max(
                        screen_of(r.min.x, r.min.y),
                        screen_of(r.max.x, r.max.y),
                    );
                    self.draw_slot_in(ctx, &painter, &key, dest);
                    // Contorno tenue = el sitio que ocupa el sprite.
                    painter.rect_stroke(
                        dest,
                        0.0,
                        egui::Stroke::new(
                            1.0,
                            egui::Color32::from_rgba_unmultiplied(255, 230, 140, 90),
                        ),
                    );
                    // Amarillo lleno = colisión (huella del editor de assets).
                    let boxes = self.collision_tile_rects(ctx, &key, tx, tz, size);
                    for cr in &boxes {
                        let sr = egui::Rect::from_min_max(
                            screen_of(cr.min.x, cr.min.y),
                            screen_of(cr.max.x, cr.max.y),
                        );
                        painter.rect_filled(
                            sr,
                            0.0,
                            egui::Color32::from_rgba_unmultiplied(255, 210, 90, 90),
                        );
                        painter.rect_stroke(
                            sr,
                            0.0,
                            egui::Stroke::new(1.0, egui::Color32::from_rgb(255, 210, 90)),
                        );
                    }
                    if boxes.is_empty() {
                        // Sin huella pintada: el asset no frena a nadie. Se avisa
                        // con una cruz en la celda ancla en vez de mentir con un
                        // cuadro sólido.
                        let cell = egui::Rect::from_min_size(screen_of(tx as f32, tz as f32), sz);
                        let st = egui::Stroke::new(
                            1.0,
                            egui::Color32::from_rgba_unmultiplied(255, 210, 90, 110),
                        );
                        painter.rect_stroke(cell, 0.0, st);
                        painter.line_segment([cell.min, cell.max], st);
                        painter.line_segment(
                            [egui::pos2(cell.min.x, cell.max.y), egui::pos2(cell.max.x, cell.min.y)],
                            st,
                        );
                    }
                }
            }

            // **Las zonas se ven siempre**, no sólo con su herramienta activa: son
            // el plano del barrio, y colocar una casa sin ver en qué zona cae es
            // exactamente el error que esto viene a evitar. Translúcidas y con
            // borde, para que no tapen el mundo que estás editando.
            for z in &self.edits.zonas {
                if z.body != self.body as u8 {
                    continue;
                }
                let pr = egui::Rect::from_min_max(
                    screen_of(z.min.0 as f32, z.min.1 as f32),
                    screen_of((z.min.0 + z.size.0) as f32, (z.min.1 + z.size.1) as f32),
                );
                let [r, g, b] = z.clase.color();
                let (r, g, b) = ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8);
                painter.rect_filled(pr, 0.0, egui::Color32::from_rgba_unmultiplied(r, g, b, 38));
                painter.rect_stroke(pr, 0.0, egui::Stroke::new(1.5, egui::Color32::from_rgb(r, g, b)));
                let nombre = if z.nombre.is_empty() { z.clase.label() } else { &z.nombre };
                if pr.width() > 40.0 {
                    painter.text(
                        pr.min + egui::vec2(4.0, 3.0),
                        egui::Align2::LEFT_TOP,
                        nombre,
                        egui::FontId::proportional(11.0),
                        egui::Color32::from_rgb(r, g, b),
                    );
                }
            }

            // Preview del rectángulo en curso (Rellenar y Zonas).
            if self.tool == Tool::Zona {
                if let (Some(a), Some(h)) = (self.rect_start, hover) {
                    let pr = egui::Rect::from_min_max(
                        screen_of(a.0.min(h.0) as f32, a.1.min(h.1) as f32),
                        screen_of((a.0.max(h.0) + 1) as f32, (a.1.max(h.1) + 1) as f32),
                    );
                    let [r, g, b] = self.zona_clase.color();
                    let (r, g, b) = ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8);
                    painter.rect_filled(pr, 0.0, egui::Color32::from_rgba_unmultiplied(r, g, b, 70));
                    painter.rect_stroke(pr, 0.0, egui::Stroke::new(2.0, egui::Color32::from_rgb(r, g, b)));
                }
            }
            // Gesto de selección en curso: el rectángulo o el trazo a mano.
            if self.tool == Tool::Seleccionar {
                let azul = egui::Color32::from_rgb(120, 220, 255);
                if let (Some(a), Some(h)) = (self.sel_drag, hover) {
                    if self.sel_mode != SelMode::Libre {
                        let pr = egui::Rect::from_min_max(
                            screen_of(a.0.min(h.0) as f32, a.1.min(h.1) as f32),
                            screen_of((a.0.max(h.0) + 1) as f32, (a.1.max(h.1) + 1) as f32),
                        );
                        painter.rect_filled(
                            pr,
                            0.0,
                            egui::Color32::from_rgba_unmultiplied(90, 200, 255, 40),
                        );
                        painter.rect_stroke(pr, 0.0, egui::Stroke::new(1.5, azul));
                    }
                }
                // Fantasma de lo copiado: dónde va a caer el bloque si pegás
                // acá. Sin esto, Ctrl+V es una apuesta.
                if let (Some(clip), Some((tx, tz))) = (self.clip.as_ref(), hover) {
                    let pr = egui::Rect::from_min_max(
                        screen_of(tx as f32, tz as f32),
                        screen_of((tx + clip.ancho) as f32, (tz + clip.alto) as f32),
                    );
                    painter.rect_stroke(
                        pr,
                        0.0,
                        egui::Stroke::new(
                            1.0,
                            egui::Color32::from_rgba_unmultiplied(120, 220, 255, 110),
                        ),
                    );
                    if ppt >= 8.0 {
                        painter.text(
                            pr.min + egui::vec2(3.0, 2.0),
                            egui::Align2::LEFT_TOP,
                            format!("Ctrl+V · {}×{}", clip.ancho, clip.alto),
                            egui::FontId::proportional(10.0),
                            egui::Color32::from_rgba_unmultiplied(120, 220, 255, 160),
                        );
                    }
                }
                for w in self.sel_libre.windows(2) {
                    painter.line_segment(
                        [
                            screen_of(w[0].0 as f32 + 0.5, w[0].1 as f32 + 0.5),
                            screen_of(w[1].0 as f32 + 0.5, w[1].1 as f32 + 0.5),
                        ],
                        egui::Stroke::new(2.0, azul),
                    );
                }
            }
            if self.tool == Tool::RellenarRect {
                if let (Some(a), Some(h)) = (self.rect_start, hover) {
                    let pr = egui::Rect::from_min_max(
                        screen_of(a.0.min(h.0) as f32, a.1.min(h.1) as f32),
                        screen_of((a.0.max(h.0) + 1) as f32, (a.1.max(h.1) + 1) as f32),
                    );
                    painter.rect_filled(pr, 0.0, egui::Color32::from_rgba_unmultiplied(80, 180, 255, 60));
                    painter.rect_stroke(pr, 0.0, egui::Stroke::new(1.5, egui::Color32::from_rgb(80, 180, 255)));
                }
            }

            // ── clic DERECHO: seleccionar y abrir el menú, con cualquier
            // herramienta. No interrumpe lo que estabas haciendo (el botón
            // izquierdo sigue siendo el de siempre).
            let mods = ui.input(|i| i.modifiers);
            let op = if mods.shift {
                SelOp::Sumar
            } else if mods.ctrl || mods.command {
                SelOp::Restar
            } else {
                SelOp::Reemplazar
            };
            if resp.secondary_clicked() {
                if let (Some(cell), Some(pos)) = (hover, resp.interact_pointer_pos()) {
                    match self.sel_mode {
                        SelMode::Varita if self.tool == Tool::Seleccionar => {
                            self.sel_varita(cell.0, cell.1, op)
                        }
                        SelMode::Similares if self.tool == Tool::Seleccionar => {
                            self.sel_similares(cell.0, cell.1, op)
                        }
                        // Fuera de la selección: agarra esa celda. Adentro: no
                        // la pisa (venías de elegir un montón, el menú es para
                        // esa selección).
                        _ if !self.sel.contains(&cell) => self.sel_aplicar([cell], op),
                        _ => {}
                    }
                    self.menu = Some((pos, cell));
                }
            }
            // ── botón izquierdo de la herramienta de selección: arrastre elige,
            // clic suelto copia el tile (el cuentagotas de siempre, sin cambiar
            // de herramienta).
            if self.tool == Tool::Seleccionar {
                if resp.drag_started_by(egui::PointerButton::Primary) {
                    self.sel_drag = hover;
                    self.sel_libre.clear();
                }
                if resp.dragged_by(egui::PointerButton::Primary) {
                    if let Some(c) = hover {
                        if self.sel_mode == SelMode::Libre && self.sel_libre.last() != Some(&c) {
                            self.sel_libre.push(c);
                        }
                    }
                }
                if resp.drag_stopped_by(egui::PointerButton::Primary) {
                    if self.sel_mode == SelMode::Libre {
                        self.sel_libre_cerrar(op);
                    } else if let (Some(a), Some(b)) = (self.sel_drag, hover) {
                        self.sel_rect(a, b, op);
                    }
                    self.sel_drag = None;
                    self.sel_libre.clear();
                }
                if resp.clicked() {
                    if let Some((x, zc)) = hover {
                        self.paint_tile = self.effective_tile(x, zc);
                        self.status = format!(
                            "tile copiado al pincel: {:?} — clic derecho para el menú",
                            self.paint_tile
                        );
                    }
                }
            }

            // Aplicar la herramienta. Un trazo = UN paso de deshacer: el snapshot
            // se toma al empezar (clic o inicio de arrastre), no en cada celda.
            let stroke_start = resp.clicked() || resp.drag_started();
            if let Some((x, zc)) = hover {
                let t = self.paint_tile;
                let acting = matches!(
                    self.tool,
                    Tool::PintarTile
                        | Tool::Borrar
                        | Tool::Bote
                        | Tool::ColocarEstructura
                        | Tool::ColocarDecor
                        | Tool::BorrarDecor
                ) && (resp.clicked() || resp.dragged());
                if stroke_start && (acting || matches!(self.tool, Tool::RellenarRect | Tool::Zona)) {
                    self.push_undo();
                }
                if resp.drag_started() {
                    self.last_decor_cell = None;
                }
                match self.tool {
                    Tool::PintarTile if acting => self.brush_apply(x, zc, Some(t)),
                    Tool::Borrar if acting => {
                        self.brush_apply(x, zc, None);
                        if resp.clicked() {
                            self.erase(x, zc); // además quita estructuras de esa celda
                        }
                    }
                    Tool::Bote if resp.clicked() => self.bucket_fill(x, zc, t),
                    Tool::Cuentagotas if resp.clicked() => {
                        self.paint_tile = self.effective_tile(x, zc);
                        self.tool = Tool::PintarTile;
                    }
                    Tool::ColocarEstructura if resp.clicked() => self.place_structure(x, zc),
                    // Arrastrar planta una hilera (una por celda, sin repetir).
                    Tool::ColocarDecor if acting => {
                        if self.last_decor_cell != Some((x, zc)) {
                            self.last_decor_cell = Some((x, zc));
                            self.place_decor(ctx, x, zc);
                        }
                    }
                    Tool::BorrarDecor if acting => {
                        if self.last_decor_cell != Some((x, zc)) {
                            self.last_decor_cell = Some((x, zc));
                            self.erase_decor(x, zc);
                        }
                    }
                    Tool::RellenarRect => {
                        if resp.drag_started() {
                            self.rect_start = Some((x, zc));
                        }
                        if resp.drag_stopped() {
                            if let Some(a) = self.rect_start.take() {
                                self.fill_rect(a, (x, zc), t);
                            }
                        }
                    }
                    Tool::Zona => {
                        if resp.drag_started() {
                            self.rect_start = Some((x, zc));
                        }
                        if resp.drag_stopped() {
                            if let Some(a) = self.rect_start.take() {
                                self.crear_zona(a, (x, zc));
                            }
                        }
                    }
                    _ => {}
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn app() -> WorldApp {
        WorldApp {
            body: CelestialBody::Earth,
            z: 0,
            center: egui::vec2(0.0, 0.0),
            px_per_tile: 12.0,
            hover_cell: None,
            tool: Tool::ColocarDecor,
            paint_tile: Tile::Grass,
            brush: 1,
            rect_start: None,
            zona_clase: ClaseZona::Residencial,
            zona_nombre: String::new(),
            place_idx: 0,
            decor_key: "decor/tree_roble".into(),
            edits: WorldEdits::default(),
            edits_path: PathBuf::from("/dev/null"),
            status: String::new(),
            art: ArtManifest::default(),
            art_path: PathBuf::new(),
            game_assets: PathBuf::new(),
            palette: Vec::new(),
            palette_filter: String::new(),
            tex: HashMap::new(),
            show_art: true,
            show_collision: true,
            undo: Vec::new(),
            last_decor_cell: None,
            decor_scale: 1.0,
            decor_stamps_ground: false,
            decor_ground_tile: Tile::Stone,
            solar: HashMap::new(),
            sel: std::collections::HashSet::new(),
            sel_mode: SelMode::Rect,
            sel_drag: None,
            sel_libre: Vec::new(),
            sel_limita: true,
            menu: None,
            clip: None,
            vista: (0, 0, 0, 0),
        }
    }

    /// Un asset de `cells` tiles ya recortado y elegido en la paleta.
    fn app_with(cells: (u32, u32)) -> (WorldApp, egui::Context) {
        let mut a = super::tests::app();
        a.decor_key = "prop/farol".into();
        a.art.set_rect(
            "prop/farol",
            "art/prop/farol.png".into(),
            (32, 32),
            ha_assets::CellRect { col: 0, row: 0, w: cells.0, h: cells.1 },
        );
        (a, egui::Context::default())
    }

    #[test]
    fn the_asset_lands_on_the_cell_you_point_at() {
        // Farol de 1×2: su PIE (la celda ancla) va justo en la celda del mouse y
        // el resto sobresale hacia arriba. Antes el sprite se apoyaba "a ojo" y
        // con anchos pares quedaba medio tile corrido del cuadro amarillo.
        let (mut a, ctx) = app_with((1, 2));
        let r = a.placed_tile_rect(&ctx, "prop/farol", 4, 7, [1.0, 2.0]);
        assert_eq!((r.min.x, r.max.x), (4.0, 5.0));
        assert_eq!((r.min.y, r.max.y), (6.0, 8.0)); // pie en la fila 7 = la del mouse
        // Ancho PAR: sigue clavado al grid (nada de medios tiles).
        let (mut a2, ctx2) = app_with((2, 2));
        let r2 = a2.placed_tile_rect(&ctx2, "prop/farol", 10, 5, [2.0, 2.0]);
        assert_eq!((r2.min.x, r2.max.x), (10.0, 12.0));
        assert_eq!((r2.min.y, r2.max.y), (4.0, 6.0));
    }

    #[test]
    fn the_yellow_boxes_are_the_real_collision_of_the_sprite() {
        // Huella fina (¼ de tile) del poste del farol, pintada en el editor de
        // assets: el editor de mundo debe mostrar ESAS cajas, no el tile entero.
        let (mut a, ctx) = app_with((1, 2));
        a.art.set_footprint(
            "prop/farol",
            Some(ha_assets::CollisionFootprint {
                w: 1,
                h: 2,
                ax: 0,
                az: 1,
                solid: vec![(1, 6), (2, 6), (1, 7), (2, 7)],
                step: Vec::new(), res: 4,
            }),
        );
        let boxes = a.collision_tile_rects(&ctx, "prop/farol", 4, 7, [1.0, 2.0]);
        assert_eq!(boxes.len(), 4);
        // Todas dentro de la celda apuntada y sólo en su mitad central.
        for b in &boxes {
            assert!(b.min.x >= 4.25 && b.max.x <= 4.75, "{b:?}");
            assert!(b.min.y >= 7.0 && b.max.y <= 8.0, "{b:?}");
        }
        // Sin huella pintada no hay colisión que mostrar (el asset se pisa).
        a.art.set_footprint("prop/farol", None);
        assert!(a.collision_tile_rects(&ctx, "prop/farol", 4, 7, [1.0, 2.0]).is_empty());
    }

    #[test]
    fn placing_an_asset_records_it_once_per_cell() {
        let mut a = super::tests::app();
        a.place_decor_sized(4, 7, Some([5.0, 4.0]), None, None);
        a.place_decor_sized(4, 7, Some([5.0, 4.0]), None, None); // volver a clickear reemplaza, no apila
        assert_eq!(a.edits.decor.len(), 1);
        assert_eq!(a.edits.decor[0].key, "decor/tree_roble");
        assert_eq!((a.edits.decor[0].x, a.edits.decor[0].zc), (4, 7));
        // Otro asset en la misma celda: sigue siendo uno solo, el nuevo.
        a.decor_key = "prop/farol".into();
        a.place_decor_sized(4, 7, Some([1.0, 2.0]), None, None);
        assert_eq!(a.edits.decor.len(), 1);
        assert_eq!(a.edits.decor[0].key, "prop/farol");
        // La goma lo saca y NO deja basura de celda limpiada (no había procedural).
        a.erase_decor(4, 7);
        assert!(a.edits.decor.is_empty());
        assert!(a.edits.cleared.is_empty());
    }

    /// El solar se adapta al TAMAÑO DEL ASSET, y el editor lo muestra. Es el fix
    /// de "el asset que coloco no borra el tile de abajo, se pone encima".
    #[test]
    fn stamping_an_asset_replaces_the_ground_under_its_footprint() {
        let mut a = super::tests::app();
        a.decor_stamps_ground = true;
        a.decor_ground_tile = Tile::Stone;
        // Un asset de 5×4 anclado en (40, 40): ancla por defecto = (2, 3).
        a.place_decor_sized(40, 40, Some([5.0, 4.0]), Some((5, 4)), None);
        a.rebuild_solar();
        assert_eq!(a.solar.len(), 20, "el solar mide lo que mide el asset");
        assert_eq!(a.effective_tile(38, 37), Tile::Stone, "esquina min del solar");
        assert_eq!(a.effective_tile(42, 40), Tile::Stone, "esquina max del solar");
        // Y ni un tile más allá de la huella.
        assert_ne!(a.solar.get(&(43, 40)), Some(&Tile::Stone));
        // Lo que pintás a mano GANA sobre el solar (lo dibujaste después).
        a.commit_map([((40, 40), Tile::Water)].into_iter().collect());
        assert_eq!(a.effective_tile(40, 40), Tile::Water);
        // Sacar el asset se lleva su solar: no queda una losa fantasma.
        a.erase_decor(40, 40);
        a.rebuild_solar();
        assert!(a.solar.is_empty());
    }

    /// Sin la casilla tildada, colocar un asset NO toca el terreno — es el
    /// comportamiento histórico y lo usan todos los props sueltos.
    #[test]
    fn placing_without_stamping_leaves_the_terrain_alone() {
        let mut a = super::tests::app();
        a.decor_stamps_ground = false;
        a.place_decor_sized(40, 40, Some([5.0, 4.0]), Some((5, 4)), None);
        a.rebuild_solar();
        assert!(a.solar.is_empty());
        assert!(a.edits.decor[0].ground.is_none());
    }

    #[test]
    fn painted_terrain_hides_procedural_decor_in_the_editor_too() {
        // El editor debe mostrar lo MISMO que el juego: buscamos una celda donde
        // el mundo genere algo y comprobamos que pintarla la vacía.
        let mut a = super::tests::app();
        let found = (0..4000i64)
            .flat_map(|i| (0..40i64).map(move |j| (i, j)))
            .find(|&(x, z)| a.procedural_decor_at(x, z).is_some() && !a.effective_tile(x, z).solid());
        let (x, z) = found.expect("el mundo genera decor cerca del origen");
        assert!(a.effective_decor(x, z).is_some(), "el mundo genera algo ahí");
        a.commit_map([((x, z), Tile::Grass)].into_iter().collect());
        assert!(
            a.effective_decor(x, z).is_none(),
            "lo que pintás reemplaza al mundo: el árbol no debe seguir ahí"
        );
        // Y el asset autorado gana incluso sobre el terreno pintado.
        a.place_decor_sized(x, z, None, None, None);
        assert_eq!(a.effective_decor(x, z).as_deref(), Some("decor/tree_roble"));
    }

    #[test]
    fn erasing_procedural_decor_marks_the_cell_for_the_game() {
        let mut a = super::tests::app();
        let found = (0..4000i64)
            .flat_map(|i| (0..40i64).map(move |j| (i, j)))
            .find(|&(x, z)| a.procedural_decor_at(x, z).is_some() && !a.effective_tile(x, z).solid());
        let (x, z) = found.expect("el mundo genera decor cerca del origen");
        a.erase_decor(x, z);
        assert!(a.is_cleared(x, z), "queda registrado para que el juego no lo genere");
        assert!(a.effective_decor(x, z).is_none());
        // Borrar dos veces no duplica la marca.
        a.erase_decor(x, z);
        assert_eq!(a.edits.cleared.len(), 1);
        // Y viaja en el JSON que lee el juego.
        let json = serde_json::to_string(&a.edits).unwrap();
        let back: WorldEdits = serde_json::from_str(&json).unwrap();
        assert_eq!(back.cleared.len(), 1);
    }
}

/// La herramienta de selección: elegir un montón de celdas y resolverlas de una.
/// Todo lo de acá es lo que reemplaza a "pintar cuadrito por cuadrito".
#[cfg(test)]
mod tests_seleccion {
    use super::*;

    fn editor() -> WorldApp {
        let mut a = super::tests::app();
        a.tool = Tool::Seleccionar;
        a
    }

    /// Un punto del mundo donde ni la celda ni sus vecinas (5×5) son piedra:
    /// hace falta para probar la varita sin que el terreno procedural se cuele
    /// en la mancha y el test dependa del ruido.
    fn hueco_sin_piedra(a: &WorldApp) -> (i64, i64) {
        (0..4000i64)
            .flat_map(|i| (0..60i64).map(move |j| (i * 7, j * 7)))
            .find(|&(x, z)| {
                (-2..=2).all(|dz| (-2..=2).all(|dx| a.effective_tile(x + dx, z + dz) != Tile::Stone))
            })
            .expect("algún rincón del mundo no es piedra")
    }

    #[test]
    fn el_rectangulo_agarra_todo_lo_de_adentro() {
        let mut e = editor();
        // En cualquier sentido: arrastrar de abajo-derecha a arriba-izquierda es
        // la mitad de los arrastres reales.
        for (a, b) in [((2, 2), (4, 5)), ((4, 5), (2, 2))] {
            e.sel.clear();
            e.sel_rect(a, b, SelOp::Reemplazar);
            assert_eq!(e.sel.len(), 12, "arrastre {a:?}→{b:?}");
            assert!(e.sel.contains(&(3, 4)) && e.sel.contains(&(2, 2)) && e.sel.contains(&(4, 5)));
            assert!(!e.sel.contains(&(5, 5)));
        }
    }

    #[test]
    fn shift_suma_y_ctrl_resta() {
        let mut e = editor();
        e.sel_rect((0, 0), (3, 3), SelOp::Reemplazar);
        e.sel_rect((10, 10), (11, 11), SelOp::Sumar);
        assert_eq!(e.sel.len(), 20, "sumar no pisa lo anterior");
        e.sel_rect((0, 0), (1, 1), SelOp::Restar);
        assert_eq!(e.sel.len(), 16);
        assert!(!e.sel.contains(&(0, 0)) && e.sel.contains(&(2, 2)));
        // Y un gesto sin modificadores empieza de cero.
        e.sel_rect((50, 50), (50, 50), SelOp::Reemplazar);
        assert_eq!(e.sel.len(), 1);
    }

    /// "Dibujar libremente y que se seleccione todo lo de adentro": sin el
    /// relleno, el lazo daría una línea de un tile y no serviría para nada.
    #[test]
    fn el_lazo_libre_rellena_lo_que_encierra() {
        let mut e = editor();
        // Contorno de un cuadrado de 6×6 (esquina en 10,10).
        let mut trazo = Vec::new();
        for i in 0..6 {
            trazo.push((10 + i, 10));
        }
        for i in 1..6 {
            trazo.push((15, 10 + i));
        }
        for i in (10..15).rev() {
            trazo.push((i, 15));
        }
        for i in (11..15).rev() {
            trazo.push((10, i));
        }
        e.sel_libre = trazo;
        e.sel_libre_cerrar(SelOp::Reemplazar);
        assert!(e.sel.contains(&(12, 12)), "el interior entra");
        assert!(e.sel.contains(&(10, 10)), "el propio trazo también");
        assert!(!e.sel.contains(&(16, 16)), "y nada de afuera");
        assert!(e.sel.len() >= 30, "se llenó el cuadrado: {}", e.sel.len());
    }

    #[test]
    fn la_varita_agarra_la_mancha_contigua_del_mismo_terreno() {
        let mut e = editor();
        let (x, z) = hueco_sin_piedra(&e);
        // Una crucecita de piedra: 5 celdas pegadas.
        let cruz = [(x, z), (x + 1, z), (x - 1, z), (x, z + 1), (x, z - 1)];
        e.commit_map(cruz.iter().map(|&c| (c, Tile::Stone)).collect());
        e.sel_varita(x, z, SelOp::Reemplazar);
        assert_eq!(e.sel.len(), 5, "ni una celda de más: {:?}", e.sel);
        for c in cruz {
            assert!(e.sel.contains(&c));
        }
    }

    #[test]
    fn el_contorno_deja_solo_el_borde() {
        let mut e = editor();
        e.sel_rect((0, 0), (4, 4), SelOp::Reemplazar);
        e.sel_contorno();
        assert_eq!(e.sel.len(), 16, "5×5 menos el 3×3 de adentro");
        assert!(e.sel.contains(&(0, 0)) && e.sel.contains(&(2, 0)));
        assert!(!e.sel.contains(&(2, 2)));
        // Y engordar/adelgazar es simétrico.
        e.sel_rect((0, 0), (4, 4), SelOp::Reemplazar);
        e.sel_dilatar(-1);
        assert_eq!(e.sel.len(), 9, "contraer deja el 3×3 interior");
        e.sel_dilatar(1);
        assert_eq!(e.sel.len(), 21, "expandir suma la cruz de vecinos");
    }

    /// La selección es una MÁSCARA: con ella puesta, el pincel no se desborda.
    /// Es lo que deja repasar una calle sin comerse la vereda.
    #[test]
    fn la_seleccion_limita_al_pincel() {
        let mut e = editor();
        e.brush = 3; // 5×5
        e.sel_rect((0, 0), (1, 1), SelOp::Reemplazar);
        e.brush_apply(0, 0, Some(Tile::Sand));
        let pintadas: Vec<_> = e.edits.tiles.iter().map(|t| (t.x, t.zc)).collect();
        assert_eq!(pintadas.len(), 4, "sólo las 4 celdas elegidas: {pintadas:?}");
        // Sin selección, el mismo pincel pinta sus 25.
        e.sel.clear();
        e.edits.tiles.clear();
        e.brush_apply(0, 0, Some(Tile::Sand));
        assert_eq!(e.edits.tiles.len(), 25);
    }

    #[test]
    fn rellenar_y_quitar_actuan_sobre_toda_la_seleccion() {
        let mut e = editor();
        e.sel_rect((3, 3), (6, 6), SelOp::Reemplazar);
        e.sel_rellenar(Tile::Pavement);
        assert_eq!(e.edits.tiles.len(), 16);
        assert_eq!(e.effective_tile(4, 4), Tile::Pavement);
        e.sel_quitar_pintado();
        assert!(e.edits.tiles.is_empty(), "vuelve a lo que genera el mundo");
    }

    /// Copiar y pegar es lo que convierte "rediseñar una calle" en un gesto:
    /// se lleva el terreno tal como se VE (aunque sea procedural) y los assets.
    #[test]
    fn copiar_y_pegar_mueve_el_terreno_y_los_assets() {
        let mut e = editor();
        e.sel_rect((0, 0), (2, 1), SelOp::Reemplazar);
        e.sel_rellenar(Tile::Pavement);
        e.decor_key = "prop/farol".into();
        e.place_decor_sized(1, 1, None, None, None);
        e.sel_copiar();
        let clip = e.clip.as_ref().expect("copió");
        assert_eq!((clip.ancho, clip.alto), (3, 2));
        assert_eq!(clip.tiles.len(), 6);
        assert_eq!(clip.decor.len(), 1);

        e.sel_pegar((100, 200));
        assert_eq!(e.effective_tile(100, 200), Tile::Pavement);
        assert_eq!(e.effective_tile(102, 201), Tile::Pavement);
        assert!(
            e.edits.decor.iter().any(|d| (d.x, d.zc) == (101, 201) && d.key == "prop/farol"),
            "el asset viaja con su sitio relativo"
        );
        // Y lo pegado queda seleccionado, para poder seguir retocándolo.
        assert!(e.sel.contains(&(100, 200)) && e.sel.len() == 6);
    }

    /// El origen sigue en su sitio: pegar copia, no mueve.
    #[test]
    fn pegar_no_se_lleva_el_original() {
        let mut e = editor();
        e.sel_rect((0, 0), (1, 1), SelOp::Reemplazar);
        e.sel_rellenar(Tile::Ice);
        e.sel_copiar();
        e.sel_pegar((40, 40));
        assert_eq!(e.effective_tile(0, 0), Tile::Ice);
        assert_eq!(e.effective_tile(40, 40), Tile::Ice);
    }

    #[test]
    fn la_seleccion_se_convierte_en_zona() {
        let mut e = editor();
        e.zona_clase = ClaseZona::Comercial;
        e.sel_rect((0, 0), (31, 23), SelOp::Reemplazar);
        e.sel_zona();
        assert_eq!(e.edits.zonas.len(), 1);
        assert_eq!(e.edits.zonas[0].size, (32, 24), "el rectángulo que la envuelve");
        ha_shared::zonas::instalar(Zonas::default());
    }
}

#[cfg(test)]
mod tests_zonas {
    use super::*;

    fn editor() -> WorldApp {
        let mut a = super::tests::app();
        a.tool = Tool::Zona;
        a
    }

    /// Arrastrar de una esquina a otra crea la zona, **en cualquier sentido**. Si
    /// sólo funcionara de arriba-izquierda a abajo-derecha, la mitad de los
    /// arrastres se perderían sin decir por qué.
    #[test]
    fn el_arrastre_crea_la_zona_en_cualquier_sentido() {
        for (a, b) in [((0, 0), (39, 29)), ((39, 29), (0, 0)), ((39, 0), (0, 29))] {
            let mut e = editor();
            e.crear_zona(a, b);
            assert_eq!(e.edits.zonas.len(), 1, "arrastre {a:?}→{b:?}");
            let z = &e.edits.zonas[0];
            assert_eq!(z.min, (0, 0));
            assert_eq!(z.size, (40, 30));
        }
    }

    /// Un clic suelto no es una zona. Sin esto la lista se llena de rectángulos
    /// de un tile, invisibles en el mapa e imposibles de volver a agarrar.
    #[test]
    fn un_clic_no_crea_una_zona() {
        let mut e = editor();
        e.crear_zona((10, 10), (10, 10));
        assert!(e.edits.zonas.is_empty());
        assert!(e.status.contains("chica"), "y avisa por qué: {}", e.status);
    }

    /// La zona nace con la clase elegida y sale al generador enseguida: eso es lo
    /// que hace que el lienzo muestre el barrio nuevo sin guardar ni reabrir.
    #[test]
    fn la_zona_llega_al_generador_al_dibujarla() {
        let mut e = editor();
        e.zona_clase = ClaseZona::Industrial;
        e.zona_nombre = "Puerto".into();
        e.crear_zona((200, 200), (299, 299));
        let z = &e.edits.zonas[0];
        assert_eq!(z.clase, ClaseZona::Industrial);
        assert_eq!(z.nombre, "Puerto");
        assert_eq!(
            ha_shared::zonas::clase_en(e.body as u8, 250, 250),
            Some(ClaseZona::Industrial),
            "dibujar una zona no la instaló en el generador"
        );
        // Y se limpia para no contaminar al resto de la suite (registro global).
        ha_shared::zonas::instalar(Zonas::default());
    }

    /// Las zonas viajan en `world_edits.json` con todo lo demás: un solo archivo,
    /// un solo guardado.
    #[test]
    fn las_zonas_se_guardan_con_el_resto_del_mundo() {
        let mut e = editor();
        e.crear_zona((0, 0), (63, 63));
        let json = serde_json::to_string(&e.edits).unwrap();
        let back: WorldEdits = serde_json::from_str(&json).unwrap();
        assert_eq!(back.zonas.len(), 1);
        assert_eq!(back.zonas[0].size, (64, 64));
        ha_shared::zonas::instalar(Zonas::default());
    }
}
