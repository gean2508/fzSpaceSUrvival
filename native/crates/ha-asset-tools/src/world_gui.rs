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
use ha_shared::CelestialBody;

/// Tiles que se pueden pintar (curado; nombre en ASCII para el juego, acá egui).
const PAINT_TILES: &[(Tile, &str)] = &[
    (Tile::Grass, "Pasto"),
    (Tile::Dirt, "Tierra"),
    (Tile::Sand, "Arena"),
    (Tile::Water, "Agua"),
    (Tile::Regolith, "Regolito"),
    (Tile::Basalt, "Basalto"),
    (Tile::Ice, "Hielo"),
    (Tile::RockRed, "Roca roja"),
    (Tile::Stone, "Piedra"),
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
        Tile::Grass => Some("tiles/grass"),
        Tile::Sand => Some("tiles/sand"),
        Tile::Regolith => Some("tiles/regolith"),
        Tile::Basalt => Some("tiles/basalt"),
        Tile::Ice => Some("tiles/ice"),
        Tile::WoodFloor => Some("floor/wood"),
        Tile::WoodWall | Tile::Wall => Some("wall/wood"),
        Tile::StoneBrick => Some("wall/stone"),
        Tile::Door => Some("door/wood"),
        Tile::Window => Some("window/basic"),
        Tile::StairsDown => Some("stairs/down"),
        Tile::StairsUp => Some("stairs/up"),
        _ => None,
    }
}

/// Categorías que NO tiene sentido plantar en el mapa (interfaz, cielo) y los
/// tiles, que tienen su propia paleta de pintura.
fn placeable_category(key: &str) -> bool {
    !(key.starts_with("ui/") || key.starts_with("backgrounds/") || key.starts_with("tiles/"))
}

#[derive(PartialEq, Clone, Copy)]
enum Tool {
    Mover,
    PintarTile,
    RellenarRect,
    Bote,
    Cuentagotas,
    ColocarEstructura,
    ColocarDecor,
    BorrarDecor,
    Borrar,
}

/// Un asset colocable, tal como salió de `art/manifest.json`.
struct PaletteEntry {
    key: String,
    category: String,
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

struct WorldApp {
    body: CelestialBody,
    z: i32,
    center: egui::Vec2,
    px_per_tile: f32,
    hover_cell: Option<(i64, i64)>,
    tool: Tool,
    paint_tile: Tile,
    brush: i64,                     // radio del pincel (1 = 1 celda, 3 = 5×5…)
    rect_start: Option<(i64, i64)>, // esquina del rectángulo en curso
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

    /// Historial para Ctrl+Z (snapshots completos; el archivo es chico).
    undo: Vec<WorldEdits>,
    /// Última celda donde se colocó decor arrastrando (evita apilar en la misma).
    last_decor_cell: Option<(i64, i64)>,
    /// Escala con la que se planta el asset (1 = su tamaño natural en tiles).
    decor_scale: f32,
}

impl WorldApp {
    fn new() -> Self {
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
            undo: Vec::new(),
            last_decor_cell: None,
            decor_scale: 1.0,
        };
        app.load_art();
        app
    }

    // ── puente con el editor de assets ──────────────────────────────────────

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
        let Some((rel, crop)) = self.slot_view(key) else { return false };
        let Some(tex) = self.tex_for(ctx, &rel) else { return false };
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
            return false;
        }
        // Alto proporcional al ancho de la celda, apoyado en el borde inferior
        // (pivote al pie: un árbol sobresale hacia arriba, como en el juego).
        let w = cell.width() * scale;
        let h = w * px.y / px.x;
        let draw = egui::Rect::from_min_max(
            egui::pos2(cell.center().x - w * 0.5, cell.max.y - h),
            egui::pos2(cell.center().x + w * 0.5, cell.max.y),
        );
        painter.image(tex.id(), draw, uv, egui::Color32::WHITE);
        true
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
            .unwrap_or_else(|| topdown_tile(self.body, self.z, x, zc))
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
        });
    }

    /// Planta el asset elegido en la celda. Una celda = un asset (volver a
    /// clickear lo reemplaza, no lo apila).
    fn place_decor(&mut self, ctx: &egui::Context, x: i64, zc: i64) {
        let key = self.decor_key.clone();
        let scale = self.decor_scale.max(0.1);
        let size = self.slot_size_m(ctx, &key).map(|[w, h]| [w * scale, h * scale]);
        self.place_decor_sized(x, zc, size);
    }

    /// El alta en sí (sin UI): una celda = un asset. Volver a colocar reemplaza.
    fn place_decor_sized(&mut self, x: i64, zc: i64, size: Option<[f32; 2]>) {
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
                DecorKind::Tree => (pick_tree_species_in(body, tx, tz).key.to_string(), true),
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
        self.top_bar(ctx);
        self.tools_panel(ctx);
        self.palette_panel(ctx);
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
    }

    fn tools_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("world_tools").resizable(false).show(ctx, |ui| {
            ui.heading("Herramienta");
            ui.selectable_value(&mut self.tool, Tool::Mover, "✋ Mover/zoom");
            ui.selectable_value(&mut self.tool, Tool::PintarTile, "🖌 Pintar (pincel)");
            ui.selectable_value(&mut self.tool, Tool::RellenarRect, "▭ Rellenar rectángulo");
            ui.selectable_value(&mut self.tool, Tool::Bote, "🪣 Bote (relleno por terreno)");
            ui.selectable_value(&mut self.tool, Tool::Cuentagotas, "💧 Cuentagotas");
            ui.selectable_value(&mut self.tool, Tool::ColocarDecor, "🌳 Colocar asset");
            ui.selectable_value(&mut self.tool, Tool::BorrarDecor, "🍃 Borrar asset/árbol");
            ui.selectable_value(&mut self.tool, Tool::ColocarEstructura, "🏛 Estructura grande");
            ui.selectable_value(&mut self.tool, Tool::Borrar, "🧽 Borrar terreno");
            ui.separator();
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
            match self.tool {
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
                    ui.label(egui::RichText::new(format!("elegido: {}", self.decor_key)).small());
                }
                ui.separator();
                let f = self.palette_filter.to_lowercase();
                // Agrupar por categoría respetando el filtro.
                let mut groups: Vec<(String, Vec<String>)> = Vec::new();
                for p in &self.palette {
                    if !f.is_empty() && !p.key.to_lowercase().contains(&f) {
                        continue;
                    }
                    match groups.last_mut() {
                        Some((c, v)) if *c == p.category => v.push(p.key.clone()),
                        _ => groups.push((p.category.clone(), vec![p.key.clone()])),
                    }
                }
                let mut pick: Option<String> = None;
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (cat, keys) in &groups {
                        egui::CollapsingHeader::new(
                            egui::RichText::new(format!("{} ({})", cat.to_uppercase(), keys.len()))
                                .strong()
                                .color(egui::Color32::from_rgb(120, 210, 255)),
                        )
                        .default_open(f.is_empty() && groups.len() <= 3 || !f.is_empty())
                        .id_salt(cat)
                        .show(ui, |ui| {
                            for key in keys {
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
                                    let leaf = key.split_once('/').map(|(_, n)| n).unwrap_or(key);
                                    ui.selectable_label(selected, leaf).clicked()
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

            // Fantasma del asset bajo el cursor (drag & drop legible).
            if let (Tool::ColocarDecor, Some((tx, tz))) = (self.tool, hover) {
                let cell = egui::Rect::from_min_size(screen_of(tx as f32, tz as f32), sz);
                let key = self.decor_key.clone();
                if !key.is_empty() {
                    let w = self.slot_size_m(ctx, &key).map(|s| s[0]).unwrap_or(1.5)
                        * self.decor_scale.max(0.1);
                    self.draw_slot(ctx, &painter, &key, cell, w);
                }
                painter.rect_stroke(cell, 0.0, egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 210, 90)));
            }

            // Preview del rectángulo en curso (herramienta Rellenar).
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
                if stroke_start && (acting || self.tool == Tool::RellenarRect) {
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
                    _ => {}
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> WorldApp {
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
            undo: Vec::new(),
            last_decor_cell: None,
            decor_scale: 1.0,
        }
    }

    #[test]
    fn placing_an_asset_records_it_once_per_cell() {
        let mut a = app();
        a.place_decor_sized(4, 7, Some([5.0, 4.0]));
        a.place_decor_sized(4, 7, Some([5.0, 4.0])); // volver a clickear reemplaza, no apila
        assert_eq!(a.edits.decor.len(), 1);
        assert_eq!(a.edits.decor[0].key, "decor/tree_roble");
        assert_eq!((a.edits.decor[0].x, a.edits.decor[0].zc), (4, 7));
        // Otro asset en la misma celda: sigue siendo uno solo, el nuevo.
        a.decor_key = "prop/farol".into();
        a.place_decor_sized(4, 7, Some([1.0, 2.0]));
        assert_eq!(a.edits.decor.len(), 1);
        assert_eq!(a.edits.decor[0].key, "prop/farol");
        // La goma lo saca y NO deja basura de celda limpiada (no había procedural).
        a.erase_decor(4, 7);
        assert!(a.edits.decor.is_empty());
        assert!(a.edits.cleared.is_empty());
    }

    #[test]
    fn painted_terrain_hides_procedural_decor_in_the_editor_too() {
        // El editor debe mostrar lo MISMO que el juego: buscamos una celda donde
        // el mundo genere algo y comprobamos que pintarla la vacía.
        let mut a = app();
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
        a.place_decor_sized(x, z, None);
        assert_eq!(a.effective_decor(x, z).as_deref(), Some("decor/tree_roble"));
    }

    #[test]
    fn erasing_procedural_decor_marks_the_cell_for_the_game() {
        let mut a = app();
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
