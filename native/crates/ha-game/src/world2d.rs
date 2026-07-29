//! Top-Down 3/4 tile world (LimeZu) — successor to the side-scroller strip.
//!
//! Ground plane = XZ. Grid cell `(tx, tz)` renders to screen `(tx*TILE,
//! -tz*TILE)` — same `(x, -z)` projection as `orbital2d.rs`. Depth is a
//! discrete Z-Level (`z`): 0 = surface, negative = mine floors (Fase 3).
//! Chunks are 2D grids of `Mesh2d` quads (vertex colors); decor/deposits are
//! y-sorted sprites. See `MIGRATION_PLAN.md`.
//!
//! Module keeps the old public names (`TileWorld`, `ActivePlanet`,
//! `active_surface_body`, …) so machines/launch_pad/targeting compile through
//! the migration; the side-scroller `flat2d`/strip logic is gone from here.

use std::collections::{HashMap, HashSet};

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use serde::{Deserialize, Serialize};

use ha_shared::topdown::{
    deposit_at, portal_here, topdown_tile, Deposit, Portal, Tile, MIN_Z_LEVEL, TILE_M,
};
use ha_shared::{CelestialBody, InteractTargetKind, WorldContext};

use crate::art2d::ArtLibrary;
use crate::harvest::{FloraHarvest, TreeInstance};
use crate::inventory_persist::persist_dir;
use crate::platform::is_app_paused;
use crate::scene::{RegionId, RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, SimState};
use crate::targeting::InteractTarget;

/// Tiles per chunk side (a chunk is CHUNK×CHUNK cells).
pub const CHUNK: i64 = 16;
/// Chunks kept alive around the player (Chebyshev radius).
const CHUNK_RANGE: i64 = 3;
/// Cuántos chunks se les puede construir mesh+decoración en UN frame. Sin este
/// tope, un salto grande de `range` (aterrizar, teletransportarse, entrar a
/// una ciudad densa) intenta poblar cientos de chunks de una vez y el frame se
/// congela (medido: 250 ms/4 fps). Con el tope se reparte en varios frames,
/// más cerca primero — imperceptible en vez de un tirón. Mismo patrón que
/// streaming de chunks en Minecraft/GTA.
const CHUNK_BUILD_BUDGET_PER_FRAME: usize = 6;

/// Screen render position of a world ground point `(x, z)` (metres).
/// North (−z) is up. Shared with the orbital view.
pub fn render_xy(x: f32, z: f32) -> Vec2 {
    Vec2::new(x, -z)
}

/// Y-sort depth for a sprite at world z (more south / larger z draws in front).
pub fn sort_z(world_z: f32) -> f32 {
    2.0 + world_z * 0.001
}

/// Hash estable de una celda → id u32 para el set de cosecha/flora. ANTES truncaba
/// a `i16` (±32 km), así que lejos del origen las celdas COLISIONABAN con el
/// removed-set y la decoración dejaba de aparecer ("se cargaban las bases"). Ahora
/// mezcla los i64 completos: colisiones ~1/4e9 por par → despreciables, sin repetir.
pub fn pack_cell(tx: i64, tz: i64) -> u32 {
    let mut h = (tx as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (tz as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 29;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 32;
    h as u32
}

// ---------------------------------------------------------------------------
// Active body/layer
// ---------------------------------------------------------------------------

#[derive(Resource, Default, Clone, Copy)]
pub struct ActivePlanet {
    pub body: Option<CelestialBody>,
}

pub fn active_surface_body(ctx: &WorldContextRuntime, planet: &ActivePlanet) -> CelestialBody {
    match ctx.active {
        WorldContext::Luna => CelestialBody::Moon,
        WorldContext::Planet => planet.body.unwrap_or(CelestialBody::Mars),
        _ => CelestialBody::Earth,
    }
}

/// Reverso de `CelestialBody as u8` (para overrides autorados / persistencia por
/// u8, ej. estructuras y `world_edits.json`).
pub fn body_from_u8(v: u8) -> Option<CelestialBody> {
    CelestialBody::all().iter().copied().find(|b| *b as u8 == v)
}

pub fn active_landable_body(
    region: &RegionRuntime,
    ctx: &WorldContextRuntime,
    flight: &crate::flight::Flight,
    planet: &ActivePlanet,
) -> Option<CelestialBody> {
    if flight.blocks_walking() {
        return None;
    }
    surface_region_active(region)?;
    Some(active_surface_body(ctx, planet))
}

// ---------------------------------------------------------------------------
// Tile storage (natural terrain + sparse edits per (body, z, tx, tz))
// ---------------------------------------------------------------------------

fn tile_name(t: Tile) -> &'static str {
    match t {
        Tile::Air => "air",
        Tile::Grass => "grass",
        Tile::Dirt => "dirt",
        Tile::Sand => "sand",
        Tile::Water => "water",
        Tile::Regolith => "regolith",
        Tile::Basalt => "basalt",
        Tile::Ice => "ice",
        Tile::RockRed => "rock_red",
        Tile::Stone => "stone",
        Tile::RockDark => "rock_dark",
        Tile::Bedrock => "bedrock",
        Tile::CaveFloor => "cave_floor",
        Tile::WoodFloor => "wood_floor",
        Tile::Wall => "wall",
        Tile::WoodWall => "wood_wall",
        Tile::StoneBrick => "stone_brick",
        Tile::Door => "door",
        Tile::Window => "window",
        Tile::StairsDown => "stairs_down",
        Tile::StairsUp => "stairs_up",
    }
}

fn tile_from_name(name: &str) -> Tile {
    match name {
        "grass" => Tile::Grass,
        "dirt" => Tile::Dirt,
        "sand" => Tile::Sand,
        "water" => Tile::Water,
        "regolith" => Tile::Regolith,
        "basalt" => Tile::Basalt,
        "ice" => Tile::Ice,
        "rock_red" => Tile::RockRed,
        "stone" => Tile::Stone,
        "rock_dark" => Tile::RockDark,
        "bedrock" => Tile::Bedrock,
        "cave_floor" => Tile::CaveFloor,
        "wood_floor" => Tile::WoodFloor,
        "wall" => Tile::Wall,
        "wood_wall" => Tile::WoodWall,
        "stone_brick" => Tile::StoneBrick,
        "door" => Tile::Door,
        "window" => Tile::Window,
        "stairs_down" => Tile::StairsDown,
        "stairs_up" => Tile::StairsUp,
        _ => Tile::Air,
    }
}

/// Tile a placeable block item becomes.
pub fn tile_for_block_item(item_id: &str) -> Option<Tile> {
    match item_id {
        "wood" => Some(Tile::WoodFloor),
        "stone" => Some(Tile::Wall),
        "dirt" => Some(Tile::Dirt),
        "sand" => Some(Tile::Sand),
        "regolith" => Some(Tile::Regolith),
        "basalt" => Some(Tile::Basalt),
        "ice" => Some(Tile::Ice),
        _ => None,
    }
}

#[derive(Resource, Default)]
pub struct TileWorld {
    /// (body u8, z, tx, tz) → override tile.
    edits: HashMap<(u8, i32, i64, i64), Tile>,
    /// Active depth layer (0 = surface).
    pub z: i32,
    pub version: u64,
    /// Celdas editadas desde el último refresh de arte (autotile por vecinos):
    /// `rebuild_edited_tile_art` la drena y re-spawnea el arte de los chunks
    /// tocados (la edición cambia la forma de los 8 vecinos, no sólo la celda).
    edit_log: Vec<(i64, i64)>,
    /// Regeneración de césped: celdas superficiales cuyo edit es `Dirt` sobre
    /// césped natural — tras `timer` segundos el edit se borra y el pasto
    /// vuelve (Minecraft-style). Ver `tick_grass_regrowth`.
    regrow: Vec<RegrowCell>,
    dirty: bool,
    was_paused: bool,
    debounce: f32,
}

/// Una celda esperando volver a césped.
struct RegrowCell {
    body: u8,
    tx: i64,
    tz: i64,
    timer: f32,
}

impl TileWorld {
    pub fn tile_at(&self, body: CelestialBody, tx: i64, tz: i64) -> Tile {
        if let Some(t) = self.edits.get(&(body as u8, self.z, tx, tz)) {
            return *t;
        }
        topdown_tile(body, self.z, tx, tz)
    }

    pub fn solid_at(&self, body: CelestialBody, tx: i64, tz: i64) -> bool {
        self.tile_at(body, tx, tz).solid()
    }

    pub fn break_tile(&mut self, body: CelestialBody, tx: i64, tz: i64) -> Option<Tile> {
        let t = self.tile_at(body, tx, tz);
        if t == Tile::Bedrock || t == Tile::Air || t == Tile::Water {
            return None;
        }
        // Digging a surface tile reveals dirt/cave floor beneath (walkable);
        // digging underground rock opens cave floor.
        let revealed = if self.z == 0 { Tile::Dirt } else { Tile::CaveFloor };
        self.edits
            .insert((body as u8, self.z, tx, tz), revealed);
        self.version = self.version.wrapping_add(1);
        self.dirty = true;
        self.edit_log.push((tx, tz));
        self.maybe_schedule_regrow(body, tx, tz);
        Some(t)
    }

    pub fn place_tile(&mut self, body: CelestialBody, tx: i64, tz: i64, kind: Tile) -> bool {
        if self.solid_at(body, tx, tz) {
            return false;
        }
        self.edits.insert((body as u8, self.z, tx, tz), kind);
        self.version = self.version.wrapping_add(1);
        self.dirty = true;
        self.edit_log.push((tx, tz));
        // Tierra colocada sobre césped natural también termina en pasto (el
        // césped "se come" la tierra con el tiempo, como en Minecraft).
        if kind == Tile::Dirt {
            self.maybe_schedule_regrow(body, tx, tz);
        }
        true
    }

    /// Si la celda quedó con edit `Dirt` en superficie y su tile NATURAL es
    /// césped (sin edificio generado encima), programa el rebrote con un delay
    /// pseudo-aleatorio por celda (45–120 s) para que no sea uniforme.
    fn maybe_schedule_regrow(&mut self, body: CelestialBody, tx: i64, tz: i64) {
        if self.z != 0 {
            return;
        }
        if self.edits.get(&(body as u8, 0, tx, tz)) != Some(&Tile::Dirt) {
            return;
        }
        if ha_shared::topdown::building_cell(body, tx, tz).is_some() {
            return;
        }
        // Natural = lo que habría sin ediciones. Sólo el césped rebrota.
        if topdown_tile(body, 0, tx, tz) != Tile::Grass {
            return;
        }
        if self
            .regrow
            .iter()
            .any(|r| r.body == body as u8 && (r.tx, r.tz) == (tx, tz))
        {
            return;
        }
        let delay = 45.0 + (hash_cell(0x9e9e_0001, tx, tz) % 7500) as f32 * 0.01;
        self.regrow.push(RegrowCell { body: body as u8, tx, tz, timer: delay });
    }

    /// Overwrite a cell at the active z UNCONDITIONALLY (even over solid rock).
    /// Used to carve the ship interior room; deliberately does NOT mark `dirty`
    /// (interior tiles are derived on boarding, never persisted — see the
    /// `SHIP_INTERIOR_Z` filter in `debounced_tiles_save`).
    pub fn force_tile(&mut self, body: CelestialBody, tx: i64, tz: i64, kind: Tile) {
        self.edits.insert((body as u8, self.z, tx, tz), kind);
        self.version = self.version.wrapping_add(1);
    }

    /// Escribe un tile **autorado en el editor de mundo** en su capa `z` (no en
    /// la activa) y sin condiciones. `place_tile` no sirve para esto: se niega
    /// sobre celdas sólidas (no podrías reemplazar roca por pasto), escribe
    /// siempre en la capa activa (el subsuelo terminaba pisando la superficie) y
    /// programa el rebrote de césped, que a los ~60 s deshacía tu diseño.
    pub fn author_tile(&mut self, body: CelestialBody, z: i32, tx: i64, tz: i64, kind: Tile) {
        self.edits.insert((body as u8, z, tx, tz), kind);
        self.version = self.version.wrapping_add(1);
    }
}

/// Celdas que el usuario AUTORÓ en el editor de mundo y en las que el juego no
/// debe sembrar decor procedural: donde pintaste terreno (lo tuyo reemplaza al
/// mundo) y donde borraste decor a mano. Sin editor de mundo está vacío y no
/// cuesta nada. Lo llena `world_edits::apply_world_edits`.
#[derive(Resource, Default)]
pub struct DecorFreeCells(std::collections::HashSet<(u8, i32, i64, i64)>);

impl DecorFreeCells {
    pub fn set(&mut self, cells: std::collections::HashSet<(u8, i32, i64, i64)>) {
        self.0 = cells;
    }

    /// ¿Esta celda quedó libre de decor procedural?
    pub fn contains(&self, body: CelestialBody, z: i32, tx: i64, tz: i64) -> bool {
        !self.0.is_empty() && self.0.contains(&(body as u8, z, tx, tz))
    }
}

// ---------------------------------------------------------------------------
// Decor (trees/rocks) + surface deposits — top-down, per grid cell
// ---------------------------------------------------------------------------

// Generación DETERMINISTA de decor extraída a `ha_shared::worldgen` (fuente ÚNICA para el
// juego y el editor de mundo). Re-exportada acá para no tocar los call sites internos.
pub use ha_shared::worldgen::{
    hash_cell, nature_decor_at, pick_tree_species_in, prop_decor_at, surface_decor_at, DecorKind,
};

/// ¿La decoración de esta celda bloquea el paso del jugador? Default por tipo
/// (árbol = sí, para no pisar el tronco; roca/flora = no, son props chicos),
/// pero el usuario puede FIJARLO por asset con la casilla "bloquea el paso" del
/// editor (`collides` en el manifest). Determinista → coincide con lo que
/// `spawn_chunk_decor` colocó. Primer paso hacia la colisión por-celda precisa
/// (relieves/cráteres) — ver `PLAN_COLISIONES_Y_CIUDADES.md`.
pub fn decor_blocks_walk(
    art: &crate::art2d::ArtLibrary,
    body: CelestialBody,
    tx: i64,
    tz: i64,
) -> bool {
    // (A1) Huella multi-celda: un decor anclado en `(cx,cz)` con huella marca
    // sólidas las celdas `(cx+dx, cz+dz)`. Sólo escaneamos la vecindad si ALGÚN
    // asset declara huella (cero costo hasta que el usuario pinte una en el
    // editor). `R` = radio máximo de huella soportado (assets hasta ~9 tiles).
    if art.has_footprints() {
        const R: i64 = 4;
        for cz in (tz - R)..=(tz + R) {
            for cx in (tx - R)..=(tx + R) {
                let fp = match surface_decor_at(body, cx, cz) {
                    Some(DecorKind::Tree) => art.collision_footprint(&[pick_tree_species_in(body, cx, cz).key]),
                    Some(DecorKind::Rock) => {
                        let rk = format!("decor/rock{}", body_art_suffix(body));
                        art.collision_footprint(&[&rk, "decor/rock"])
                    }
                    None => None,
                };
                if let Some(fp) = fp {
                    let off = ((tx - cx) as i32, (tz - cz) as i32);
                    if fp.solid_offsets().any(|o| o == off) {
                        return true;
                    }
                }
            }
        }
    }
    // Default 1 celda (comportamiento actual) para decor SIN huella en esta celda.
    match surface_decor_at(body, tx, tz) {
        Some(DecorKind::Tree) => {
            let key = pick_tree_species_in(body, tx, tz).key;
            if art.collision_footprint(&[key]).is_some() {
                return false; // su colisión la define la huella (ya evaluada arriba)
            }
            art.collides_override(&[key]).unwrap_or(true)
        }
        Some(DecorKind::Rock) => {
            let rock_key = format!("decor/rock{}", body_art_suffix(body));
            if art.collision_footprint(&[&rock_key, "decor/rock"]).is_some() {
                return false;
            }
            art.collides_override(&[&rock_key, "decor/rock"]).unwrap_or(false)
        }
        None => false,
    }
}

/// ¿Se puede construir sobre esta celda? NO si tiene decor COSECHABLE aún
/// presente (árbol / roca / depósito de mineral): hay que talar o picar primero
/// — no se edifica encima (ej.: un cuarto 5×4 no cabe sobre un árbol). Los props
/// pequeños (flora visual sin `InteractTarget`) no bloquean. Determinista →
/// coincide con lo que `spawn_chunk_decor` colocó, y respeta lo ya cosechado.
pub fn cell_build_blocked(
    flora: &FloraHarvest,
    body: CelestialBody,
    z: i32,
    tx: i64,
    tz: i64,
) -> bool {
    if flora.is_removed(body, 0, 0, pack_cell(tx, tz)) {
        return false;
    }
    (z == 0 && surface_decor_at(body, tx, tz).is_some()) || deposit_at(body, z, tx, tz).is_some()
}

fn deposit_target_kind(d: Deposit) -> InteractTargetKind {
    match d {
        Deposit::OreIron => InteractTargetKind::OreIron,
        Deposit::OreGold => InteractTargetKind::OreGold,
        Deposit::GemBlue => InteractTargetKind::GemBlue,
        Deposit::GemRed => InteractTargetKind::GemRed,
    }
}

// ---------------------------------------------------------------------------
// Chunk streaming + meshing (2D grid)
// ---------------------------------------------------------------------------

#[derive(Component)]
pub struct TileChunk {
    pub body: CelestialBody,
    pub z: i32,
    pub cx: i64,
    pub cz: i64,
    pub built_version: u64,
    /// Si este chunk tiene poblada su decoración fina (sprites de árboles,
    /// props y autotiles). Los chunks lejanos se crean SIN ella y la reciben al
    /// entrar en el anillo cercano — ver `stream_tile_chunks`.
    pub decorated: bool,
}

#[derive(Component)]
pub struct SurfaceDecor {
    pub cx: i64,
    pub cz: i64,
    pub z: i32,
}

#[derive(Resource, Default)]
pub struct ChunkMaterial(pub Handle<ColorMaterial>);

/// Fundido al cambiar de capa (subir/bajar un piso, entrar en una cueva).
///
/// El cambio de capa NUNCA mueve al jugador ni carga una escena: caminás sobre
/// la boca de la cueva o la grada y ya estás abajo, en las mismas coordenadas.
/// Lo único que hacía falta era que el mundo nuevo APAREZCA en vez de aparecer
/// de golpe — un cuarto de segundo de fundido y deja de leerse como un corte.
#[derive(Resource, Default)]
pub struct LayerFade(pub f32);

/// Duración del fundido de capa, en segundos.
const LAYER_FADE_SECS: f32 = 0.28;

/// Decor AUTORADO (árboles/props colocados a mano en el editor de mundo, `world_edits.json`).
/// Se pinta junto al decor procedural al streamear cada chunk. Lo llena `world_edits.rs`.
#[derive(Resource, Default)]
pub struct AuthoredDecor(pub Vec<ha_shared::world_edits::PlacedDecor>);

pub struct World2dPlugin;

impl Plugin for World2dPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TileWorld>()
            .init_resource::<ActivePlanet>()
            .init_resource::<ChunkMaterial>()
            .init_resource::<LayerFade>()
            .init_resource::<AuthoredDecor>()
            .init_resource::<DecorFreeCells>()
            .add_systems(
                Startup,
                (
                    load_tiles_on_startup,
                    init_chunk_material,
                    spawn_cursor_highlight,
                    spawn_pointer_reticle,
                ),
            )
            .add_systems(
                Update,
                (
                    apply_tile_hold.before(crate::hud::update_interact_hud),
                    try_place_tile_on_hold.before(crate::hud::update_interact_hud),
                    layer_transition,
                    tick_grass_regrowth,
                    stream_tile_chunks,
                    rebuild_dirty_chunks,
                    rebuild_edited_tile_art.after(rebuild_dirty_chunks),
                    // Colonos "tontos" reemplazados por NPCs con cerebro + memoria
                    // (`society.rs`, F6). `sync_colonists` queda inactivo.
                    sync_cursor_highlight,
                    sync_pointer_reticle,
                    canopy_translucency,
                    fade_surface_lod.after(canopy_translucency),
                    debounced_tiles_save,
                ),
            );
    }
}

/// Tree canopies fade to translucent when the player walks BEHIND them, so a
/// big crown never hides the character (gamer UX). A canopy occludes (draws over)
/// the player by Y-sort when the tree is more "south" (larger sort z); in that
/// case, if the player overlaps the crown box on screen, we lower the sprite's
/// alpha smoothly and restore it when they step clear. Only nearby trees are in
/// the world (chunk streaming), so this is cheap.
fn canopy_translucency(
    time: Res<Time>,
    player: Query<&Transform, With<crate::sim::Player>>,
    mut trees: Query<(&Transform, &mut Sprite, &InteractTarget), Without<crate::sim::Player>>,
) {
    let Ok(pt) = player.get_single() else { return };
    let pp = pt.translation;
    let rate = (time.delta_secs() * 9.0).min(1.0);
    for (tt, mut sprite, target) in &mut trees {
        if target.kind != InteractTargetKind::Tree {
            continue;
        }
        let size = sprite.custom_size.unwrap_or(Vec2::splat(1.0));
        // Crown box (render space): the sprite spans `size` centred on its y.
        let overlap = (pp.x - tt.translation.x).abs() < size.x * 0.5
            && (pp.y - tt.translation.y).abs() < size.y * 0.5;
        // Player is BEHIND when it sorts under the tree (tree draws over it).
        let behind = pp.z < tt.translation.z;
        let target_a = if overlap && behind { 0.42 } else { 1.0 };
        let a = sprite.color.alpha();
        sprite.color.set_alpha(a + (target_a - a) * rate);
    }
}

/// Yellow corner-bracket highlight on the grid cell under the aim cursor —
/// so you know where the reticle is detecting (base of the building system).
#[derive(Component)]
struct CursorHighlight;

fn spawn_cursor_highlight(mut commands: Commands) {
    let yellow = Color::srgb(1.0, 0.9, 0.15);
    // Four L-shaped corner brackets on a 1×1 tile (corners at ±0.5).
    commands
        .spawn((
            CursorHighlight,
            Transform::from_xyz(0.0, 0.0, 6.0),
            Visibility::Hidden,
            InheritedVisibility::default(),
            ViewVisibility::default(),
        ))
        .with_children(|p| {
            let arm = 0.28;
            let thick = 0.09;
            for (cx, cz) in [(-1.0, 1.0), (1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
                let corner = Vec2::new(cx * 0.5, cz * 0.5);
                // Horizontal arm (pointing inward).
                p.spawn((
                    Sprite::from_color(yellow, Vec2::new(arm, thick)),
                    Transform::from_xyz(corner.x - cx * arm * 0.5, corner.y, 0.0),
                ));
                // Vertical arm.
                p.spawn((
                    Sprite::from_color(yellow, Vec2::new(thick, arm)),
                    Transform::from_xyz(corner.x, corner.y - cz * arm * 0.5, 0.0),
                ));
            }
        });
}

fn sync_cursor_highlight(
    cursor: Res<crate::input::AimCursor>,
    region: Res<RegionRuntime>,
    mut hl: Query<(&mut Transform, &mut Visibility), With<CursorHighlight>>,
) {
    let Ok((mut tf, mut vis)) = hl.get_single_mut() else {
        return;
    };
    if surface_region_active(&region).is_none() || !cursor.initialized {
        *vis = Visibility::Hidden;
        return;
    }
    *vis = Visibility::Visible;
    // Snap to the cell centre under the cursor.
    let tx = cursor.world.x.floor();
    let tz = cursor.world.y.floor();
    let r = render_xy(tx + 0.5, tz + 0.5);
    tf.translation = Vec3::new(r.x, r.y, 6.0);
}

/// Free-moving pointer reticle: a small cross drawn at the EXACT aim-cursor
/// world position — never snapped to the grid. This is the "where is my
/// pointer" marker the right stick drives; the yellow brackets above merely
/// highlight whichever cell the pointer currently sits on (Terraria-style).
#[derive(Component)]
struct PointerReticle;

#[derive(Component)]
struct PointerReticleBar;

fn spawn_pointer_reticle(mut commands: Commands) {
    let color = crate::ui_theme::crosshair_color(false);
    let arm = 0.34;
    let thick = 0.07;
    commands
        .spawn((
            PointerReticle,
            Transform::from_xyz(0.0, 0.0, 6.5),
            Visibility::Hidden,
            InheritedVisibility::default(),
            ViewVisibility::default(),
        ))
        .with_children(|p| {
            // Horizontal + vertical bars form the cross.
            p.spawn((
                Sprite::from_color(color, Vec2::new(arm, thick)),
                Transform::default(),
                PointerReticleBar,
            ));
            p.spawn((
                Sprite::from_color(color, Vec2::new(thick, arm)),
                Transform::default(),
                PointerReticleBar,
            ));
        });
}

fn sync_pointer_reticle(
    cursor: Res<crate::input::AimCursor>,
    region: Res<RegionRuntime>,
    target: Res<crate::targeting::LookTarget>,
    mut reticle: Query<(&mut Transform, &mut Visibility), With<PointerReticle>>,
    mut bars: Query<&mut Sprite, With<PointerReticleBar>>,
) {
    let Ok((mut tf, mut vis)) = reticle.get_single_mut() else {
        return;
    };
    if surface_region_active(&region).is_none() || !cursor.initialized {
        *vis = Visibility::Hidden;
        return;
    }
    *vis = Visibility::Visible;
    // Exactly where the cursor is — free, no grid snap.
    let r = render_xy(cursor.world.x, cursor.world.y);
    tf.translation = Vec3::new(r.x, r.y, 6.5);

    // Brighten when the pointer is over an interactable, like the old reticle.
    let color = crate::ui_theme::crosshair_color(target.entity.is_some());
    for mut sprite in &mut bars {
        sprite.color = color;
    }
}

fn init_chunk_material(
    mut mat: ResMut<ChunkMaterial>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    // Blend explícito: `fade_surface_lod` baja el alpha de ESTE material para
    // fundir los tiles finos en el mapa satelital al alejarse (`universe.rs`).
    mat.0 = materials.add(ColorMaterial {
        alpha_mode: bevy::sprite::AlphaMode2d::Blend,
        ..default()
    });
}

/// Funde la capa de tiles finos (suelo + decoración) hacia el mapa satelital a
/// medida que la cámara se aleja. Es la mitad visible de la continuidad
/// superficie↔espacio: sin esto se vería el BORDE del anillo streameado, que es
/// exactamente lo que antes había que tapar con una pantalla opaca de nubes.
#[allow(clippy::type_complexity)]
fn fade_surface_lod(
    time: Res<Time>,
    zoom: Res<crate::camera::CameraZoom>,
    mat: Res<ChunkMaterial>,
    mut layer: ResMut<LayerFade>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut was_fading: Local<bool>,
    mut decor: Query<&mut Sprite, Or<(With<SurfaceDecor>, With<TileArt>)>>,
) {
    layer.0 = (layer.0 - time.delta_secs() / LAYER_FADE_SECS).max(0.0);
    let f = crate::universe::fine_tile_alpha(zoom.live_view_h) * (1.0 - layer.0);
    if let Some(m) = materials.get_mut(&mat.0) {
        m.color = Color::srgba(1.0, 1.0, 1.0, f);
    }
    let fading = f < 0.999;
    // A pie (f == 1) no tocamos los sprites: `canopy_translucency` es la dueña
    // del alpha de los árboles. Sólo se hace una pasada final al volver.
    if !fading && !*was_fading {
        return;
    }
    for mut sprite in &mut decor {
        sprite.color.set_alpha(f);
    }
    *was_fading = fading;
}

pub fn surface_region_active(region: &RegionRuntime) -> Option<()> {
    match region.active {
        RegionId::Overworld { .. }
        | RegionId::Luna
        | RegionId::Planet(_)
        | RegionId::Cave(_)
        | RegionId::ShipInterior => Some(()),
        _ => None,
    }
}

fn apply_tile_hold(
    mut hold: ResMut<crate::input::HoldInteractState>,
    target: Res<crate::targeting::LookTarget>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    mut tiles: ResMut<TileWorld>,
    mut inv: ResMut<crate::inventory_persist::PlayerInventory>,
    mut persist: ResMut<crate::inventory_persist::InventoryPersistState>,
    mut sfx: EventWriter<crate::audio::PlaySfx>,
) {
    if !hold.completed || target.kind != Some(InteractTargetKind::Block) {
        return;
    }
    let Some((tx, tz, _)) = target.block_cell else {
        return;
    };
    hold.completed = false;
    let body = active_surface_body(&ctx, &planet);
    if let Some(t) = tiles.break_tile(body, tx as i64, tz as i64) {
        sfx.send(crate::audio::PlaySfx(crate::audio::Sfx::Dig));
        if let Some(item) = t.dug_item() {
            inv.add(item, 1);
            persist.mark_immediate();
        }
    }
}

fn try_place_tile_on_hold(
    mut hold: ResMut<crate::input::HoldInteractState>,
    target: Res<crate::targeting::LookTarget>,
    equipped: Res<crate::input::EquippedItem>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    mut tiles: ResMut<TileWorld>,
    mut inv: ResMut<crate::inventory_persist::PlayerInventory>,
    mut persist: ResMut<crate::inventory_persist::InventoryPersistState>,
    mut sfx: EventWriter<crate::audio::PlaySfx>,
) {
    if !hold.completed || !hold.placing_block {
        return;
    }
    let Some((tx, tz, _)) = target.place_cell else {
        hold.completed = false;
        return;
    };
    hold.completed = false;
    let Some(kind) = tile_for_block_item(&equipped.item_id) else {
        return;
    };
    if inv.qty(&equipped.item_id) == 0 {
        return;
    }
    let body = active_surface_body(&ctx, &planet);
    if tiles.place_tile(body, tx as i64, tz as i64, kind) {
        sfx.send(crate::audio::PlaySfx(crate::audio::Sfx::Place));
        inv.remove(&equipped.item_id, 1);
        persist.mark_immediate();
    }
}

/// Z-Level transition: standing on a portal cell drops/climbs one floor
/// (a coherent vertical shaft, so `(x,z)` stays — only the layer changes).
/// Latched: re-arms only after the player steps off the portal (so you don't
/// fall through every floor at once). Discrete frame change, never a cutscene.
fn layer_transition(
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    mut tiles: ResMut<TileWorld>,
    mut armed: Local<bool>,
    mut started: Local<bool>,
    mut sfx: EventWriter<crate::audio::PlaySfx>,
    mut fade: ResMut<LayerFade>,
    player: Query<&SimState, With<Player>>,
) {
    if !*started {
        *armed = true;
        *started = true;
    }
    if surface_region_active(&region).is_none() {
        return;
    }
    let Ok(sim) = player.get_single() else {
        return;
    };
    let body = active_surface_body(&ctx, &planet);
    let tx = (sim.body.x / TILE_M).floor() as i64;
    let tz = (sim.body.z / TILE_M).floor() as i64;
    // Portales naturales (mina) O gradas colocadas por el jugador (F6): ambos
    // son el MISMO swap instantáneo de capa, nunca una cutscene.
    let portal = portal_here(body, tiles.z, tx, tz).or_else(|| {
        match tiles.tile_at(body, tx, tz) {
            Tile::StairsDown => Some(Portal::Descend),
            Tile::StairsUp => Some(Portal::Ascend),
            _ => None,
        }
    });
    match portal {
        Some(portal) if *armed => {
            // Verticalidad interior: hasta 3 pisos hacia arriba; hacia abajo,
            // el fondo de la mina.
            let new_z = match portal {
                Portal::Descend => (tiles.z - 1).max(MIN_Z_LEVEL),
                Portal::Ascend => (tiles.z + 1).min(3),
            };
            if new_z != tiles.z {
                let from_stairs = matches!(
                    tiles.tile_at(body, tx, tz),
                    Tile::StairsDown | Tile::StairsUp
                );
                tiles.z = new_z;
                tiles.version = tiles.version.wrapping_add(1);
                // Al llegar por gradas, tallar la sala destino (idempotente) y
                // dejar la grada de vuelta — sensación de subsuelo/piso sin
                // cuevas procedurales ni loading.
                if from_stairs {
                    carve_stair_room(&mut tiles, body, tx, tz, portal);
                }
                fade.0 = 1.0; // el piso nuevo entra fundido, no de golpe
                sfx.send(crate::audio::PlaySfx(crate::audio::Sfx::Stairs));
                *armed = false;
                eprintln!("ha-layer: → z={new_z} ({:?})", portal);
            }
        }
        Some(_) => {}
        None => *armed = true,
    }
}

/// Talla una sala cerrada 7×7 alrededor de la celda de llegada de una grada
/// (perímetro = muro, interior = piso, la celda de llegada = grada de vuelta).
/// Idempotente: sólo talla celdas aún naturales (no pisa lo que el jugador
/// construyó); las ediciones persisten en `tiles.json` como cualquier otra.
fn carve_stair_room(tiles: &mut TileWorld, body: CelestialBody, tx: i64, tz: i64, took: Portal) {
    let z = tiles.z;
    let back = match took {
        Portal::Descend => Tile::StairsUp,
        Portal::Ascend => Tile::StairsDown,
    };
    // Si la grada de vuelta ya está, la sala ya existe.
    if tiles.tile_at(body, tx, tz) == back {
        return;
    }
    const R: i64 = 3;
    for dz in -R..=R {
        for dx in -R..=R {
            let (cx, cz) = (tx + dx, tz + dz);
            let key = (body as u8, z, cx, cz);
            if tiles.edits.contains_key(&key) {
                continue; // respeta lo construido por el jugador
            }
            let perimeter = dx.abs() == R || dz.abs() == R;
            let t = if (cx, cz) == (tx, tz) {
                back
            } else if perimeter {
                Tile::WoodWall
            } else {
                Tile::WoodFloor
            };
            tiles.edits.insert(key, t);
            tiles.edit_log.push((cx, cz));
        }
    }
    tiles.version = tiles.version.wrapping_add(1);
    tiles.dirty = true;
}

#[allow(clippy::too_many_arguments)]
fn stream_tile_chunks(
    mut commands: Commands,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    art: Res<ArtLibrary>,
    mat: Res<ChunkMaterial>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut flora: ResMut<FloraHarvest>,
    authored: Res<AuthoredDecor>,
    decor_free: Res<DecorFreeCells>,
    zoom: Res<crate::camera::CameraZoom>,
    player: Query<&Transform, With<Player>>,
    mut chunks: Query<(Entity, &mut TileChunk)>,
    decor: Query<(Entity, &SurfaceDecor)>,
) {
    // Los tiles de 1 m sólo existen mientras se VEN. Pasado `FINE_OFF_VIEW_H` ya
    // los reemplazó el mapa satelital (`universe.rs`), así que streamear más
    // sería trabajo puro para nada — y era de donde salían los 19 k entities
    // volando con un anillo enorme que igual no llenaba la pantalla.
    if surface_region_active(&region).is_none()
        || zoom.live_view_h > crate::universe::FINE_OFF_VIEW_H
    {
        for (e, _) in &chunks {
            commands.entity(e).despawn_recursive();
        }
        for (e, _) in &decor {
            commands.entity(e).despawn_recursive();
        }
        return;
    }
    let Ok(player_tf) = player.get_single() else {
        return;
    };
    let body = active_surface_body(&ctx, &planet);
    let z = tiles.z;
    // Player render pos is (x, -z); recover grid chunk from world coords.
    let px = player_tf.translation.x;
    let pz = -player_tf.translation.y;
    let pcx = (px / (CHUNK as f32 * TILE_M)).floor() as i64;
    let pcz = (pz / (CHUNK as f32 * TILE_M)).floor() as i64;

    // El anillo sólo tiene que llenar la pantalla hasta `FINE_OFF_VIEW_H`; más
    // allá manda el satelital. Se ensancha con el zoom en vez de saltar a un
    // valor fijo por "estar volando".
    let need_m = zoom.live_view_h * 1.25;
    let range = ((need_m / (CHUNK as f32 * TILE_M)).ceil() as i64).clamp(CHUNK_RANGE, 9);

    // La DECORACIÓN (árboles, props, autotiles) sólo vive en el anillo cercano:
    // más allá son unos pocos píxeles y el mapa satelital ya pinta el mismo
    // terreno debajo. Antes se poblaban los ~440 chunks del anillo ancho de
    // vuelo, unas 19 000 entidades — el grueso del coste por frame volando, para
    // dibujar árboles del tamaño de una mota.
    let near = CHUNK_RANGE;
    let in_near = |cx: i64, cz: i64| (cx - pcx).abs() <= near && (cz - pcz).abs() <= near;

    let mut budget = CHUNK_BUILD_BUDGET_PER_FRAME;
    let mut live: HashSet<(i64, i64)> = HashSet::new();
    // Recorrer los chunks vivos por distancia al jugador primero: si el
    // presupuesto se agota, lo que queda sin decorar es lo más lejano (menos
    // visible), no una franja al azar según el orden de iteración del ECS.
    let mut pending_decor: Vec<(i64, i64, Entity)> = Vec::new();
    for (e, chunk) in &chunks {
        let stale = chunk.body != body
            || chunk.z != z
            || (chunk.cx - pcx).abs() > range + 1
            || (chunk.cz - pcz).abs() > range + 1;
        if stale {
            commands.entity(e).despawn_recursive();
            flora.clear_chunk(chunk.cx as i32, chunk.cz as i32);
            continue;
        }
        live.insert((chunk.cx, chunk.cz));
        // Un chunk que se creó lejos (sin decoración) y ahora quedó cerca —
        // bajaste volando — la puebla en este momento (con presupuesto: ver abajo).
        if in_near(chunk.cx, chunk.cz) && !chunk.decorated {
            pending_decor.push((chunk.cx, chunk.cz, e));
        }
    }
    pending_decor.sort_by_key(|&(cx, cz, _)| (cx - pcx).pow(2) + (cz - pcz).pow(2));
    for (cx, cz, e) in pending_decor {
        if budget == 0 {
            break;
        }
        budget -= 1;
        if let Ok((_, mut chunk)) = chunks.get_mut(e) {
            chunk.decorated = true;
        }
        spawn_chunk_decor(
            &mut commands, &mut flora, &art, &tiles, &authored.0, &decor_free, body, z, cx, cz,
        );
        spawn_chunk_tile_art(&mut commands, &art, &tiles, body, z, cx, cz);
    }
    for (e, d) in &decor {
        if d.z != z || !in_near(d.cx, d.cz) {
            commands.entity(e).despawn_recursive();
        }
    }

    // Chunks faltantes del anillo, más cercanos primero — el mismo motivo:
    // bajo presupuesto, lo que se demora un frame más es lo más lejano.
    let mut missing: Vec<(i64, i64)> = Vec::new();
    for cx in (pcx - range)..=(pcx + range) {
        for cz in (pcz - range)..=(pcz + range) {
            if !live.contains(&(cx, cz)) {
                missing.push((cx, cz));
            }
        }
    }
    missing.sort_by_key(|&(cx, cz)| (cx - pcx).pow(2) + (cz - pcz).pow(2));
    for (cx, cz) in missing {
        if budget == 0 {
            break;
        }
        budget -= 1;
        let decorated = in_near(cx, cz);
        let mesh = build_chunk_mesh(&tiles, body, cx, cz);
        commands.spawn((
            TileChunk {
                body,
                z,
                cx,
                cz,
                built_version: tiles.version,
                decorated,
            },
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(mat.0.clone()),
            Transform::from_xyz(0.0, 0.0, 0.0),
        ));
        if decorated {
            spawn_chunk_decor(
                &mut commands,
                &mut flora,
                &art,
                &tiles,
                &authored.0,
                &decor_free,
                body,
                z,
                cx,
                cz,
            );
            // Tiles con arte "vivo" (agua/autotile/piezas construidas) como
            // sprites — en TODAS las capas (subsuelos/pisos usan floor/wall/*).
            spawn_chunk_tile_art(&mut commands, &art, &tiles, body, z, cx, cz);
        }
    }
}

fn body_art_suffix(body: CelestialBody) -> &'static str {
    match body {
        CelestialBody::Earth => "",
        CelestialBody::Moon => "_moon",
        _ => "_mars",
    }
}

fn rebuild_dirty_chunks(
    tiles: Res<TileWorld>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut chunks: Query<(&mut TileChunk, &Mesh2d)>,
) {
    if !tiles.is_changed() {
        return;
    }
    for (mut chunk, mesh2d) in &mut chunks {
        if chunk.built_version == tiles.version || chunk.z != tiles.z {
            continue;
        }
        let mesh = build_chunk_mesh(&tiles, chunk.body, chunk.cx, chunk.cz);
        meshes.insert(mesh2d.0.id(), mesh);
        chunk.built_version = tiles.version;
    }
}

/// One `Mesh2d` covering a CHUNK×CHUNK grid of tiles, each a colored quad at
/// render pos `(tx*TILE, -tz*TILE)`.
fn build_chunk_mesh(tiles: &TileWorld, body: CelestialBody, cx: i64, cz: i64) -> Mesh {
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(1024);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(1024);
    let mut indices: Vec<u32> = Vec::with_capacity(1536);

    let mut quad = |x0: f32, y0: f32, size: f32, rgba: [f32; 4]| {
        let base = positions.len() as u32;
        positions.push([x0, y0, 0.0]);
        positions.push([x0 + size, y0, 0.0]);
        positions.push([x0 + size, y0 + size, 0.0]);
        positions.push([x0, y0 + size, 0.0]);
        let lin = Color::srgba(rgba[0], rgba[1], rgba[2], rgba[3]).to_linear();
        for _ in 0..4 {
            colors.push([lin.red, lin.green, lin.blue, lin.alpha]);
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    };

    for lx in 0..CHUNK {
        for lz in 0..CHUNK {
            let tx = cx * CHUNK + lx;
            let tz = cz * CHUNK + lz;
            let t = tiles.tile_at(body, tx, tz);
            let mut rgb = t.color();
            // Per-tile shade so big fills don't look flat.
            let n = (hash_cell(0x51ce, tx, tz) % 100) as f32 / 100.0;
            let shade = 0.93 + n * 0.09;
            rgb = [rgb[0] * shade, rgb[1] * shade, rgb[2] * shade];
            // Render pos: cell (tx,tz) spans [tx,tx+1]×[tz,tz+1] in world; screen
            // y = -z so the quad bottom-left is (tx*TILE, -(tz+1)*TILE).
            let x0 = tx as f32 * TILE_M;
            let y0 = -(tz as f32 + 1.0) * TILE_M;
            quad(x0, y0, TILE_M, [rgb[0], rgb[1], rgb[2], 1.0]);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

#[allow(clippy::too_many_arguments)]
fn spawn_chunk_decor(
    commands: &mut Commands,
    flora: &mut FloraHarvest,
    art: &ArtLibrary,
    tiles: &TileWorld,
    authored: &[ha_shared::world_edits::PlacedDecor],
    decor_free: &DecorFreeCells,
    body: CelestialBody,
    z: i32,
    cx: i64,
    cz: i64,
) {
    let suffix = body_art_suffix(body);
    let mut trees: Vec<TreeInstance> = Vec::new();

    for lx in 0..CHUNK {
        for lz in 0..CHUNK {
            let tx = cx * CHUNK + lx;
            let tz = cz * CHUNK + lz;
            let id = pack_cell(tx, tz);
            let wx = tx as f32 * TILE_M + 0.5;
            let wz = tz as f32 * TILE_M + 0.5;
            let r = render_xy(wx, wz);
            let sort = sort_z(wz);

            // Z-Level portals (mine entrance / ladders).
            if let Some(portal) = portal_here(body, z, tx, tz) {
                let (color, size) = match portal {
                    Portal::Descend => (Color::srgb(0.05, 0.04, 0.06), Vec2::new(1.6, 1.6)),
                    Portal::Ascend => (Color::srgb(0.55, 0.40, 0.20), Vec2::new(1.2, 1.6)),
                };
                let key = match portal {
                    Portal::Descend => "decor/mine_hole",
                    Portal::Ascend => "decor/ladder",
                };
                let mut e = commands.spawn((
                    SurfaceDecor { cx, cz, z },
                    art.sprite_or_color(key, color, size),
                    Transform::from_xyz(r.x, r.y, sort - 0.5),
                ));
                if let Some(a) = art.animation(key) {
                    e.insert(a);
                }
                crate::light2d::attach(&mut e, &art, &[key], size);
            }

            // Surface decor only on the surface layer, y NUNCA sobre un tile SÓLIDO —
            // natural (roca) o CONSTRUIDO por el jugador (muros): así los árboles/props ya
            // no tapan tu muro (bug de capas). `solid_at` incluye las ediciones.
            // Tampoco sobre celdas AUTORADAS en el editor de mundo: si pintaste
            // un claro de pasto donde había bosque, el bosque no vuelve a crecer.
            if z == 0 && !tiles.solid_at(body, tx, tz) && !decor_free.contains(body, z, tx, tz) {
                if let Some(decor) = surface_decor_at(body, tx, tz) {
                    match decor {
                        DecorKind::Tree => {
                            // Removed set is a flat per-body set at key (0,0);
                            // `id` = packed global cell (see `pack_cell`).
                            if !flora.is_removed(body, 0, 0, id) {
                                trees.push(TreeInstance {
                                    cx: 0,
                                    cz: 0,
                                    id,
                                    world: Vec3::new(r.x, r.y, 0.0),
                                });
                                let species = pick_tree_species_in(body, tx, tz);
                                let sprite = art
                                    .get_first(&[
                                        &format!("{}{suffix}", species.key),
                                        species.key,
                                        &format!("decor/tree{suffix}"),
                                        "decor/tree",
                                    ])
                                    .map(|img| {
                                        let mut s = Sprite::from_image(img);
                                        s.custom_size = Some(species.size.into());
                                        s
                                    })
                                    .unwrap_or_else(|| {
                                        let f = species.fallback;
                                        Sprite::from_color(
                                            Color::srgb(f[0], f[1], f[2]),
                                            species.size.into(),
                                        )
                                    });
                                // Base al suelo: el centro sube medio alto (menos el
                                // hundido que ya tenía el árbol de 3.2 → -0.6).
                                let y = r.y + species.size[1] * 0.5 - 0.6;
                                let mut ent = commands.spawn((
                                    SurfaceDecor { cx, cz, z },
                                    InteractTarget {
                                        kind: InteractTargetKind::Tree,
                                        id,
                                    },
                                    crate::render::particles::HarvestReactive,
                                    sprite,
                                    Transform::from_xyz(r.x, y, sort),
                                ));
                                // Árboles bioluminiscentes / con farol colgado: si el
                                // usuario le marcó luz a la especie, la lleva puesta.
                                crate::light2d::attach(
                                    &mut ent,
                                    &art,
                                    &[
                                        &format!("{}{suffix}", species.key),
                                        species.key,
                                        &format!("decor/tree{suffix}"),
                                        "decor/tree",
                                    ],
                                    species.size.into(),
                                );
                            }
                        }
                        DecorKind::Rock => {
                            // Mined rocks are removed just like chopped trees.
                            if !flora.is_removed(body, 0, 0, id) {
                                let sprite = art
                                    .get_first(&[&format!("decor/rock{suffix}"), "decor/rock"])
                                    .map(|img| {
                                        let mut s = Sprite::from_image(img);
                                        s.custom_size = Some(Vec2::new(1.3, 1.0));
                                        s
                                    })
                                    .unwrap_or_else(|| {
                                        Sprite::from_color(rock_color(body), Vec2::new(1.2, 0.9))
                                    });
                                let mut ent = commands.spawn((
                                    SurfaceDecor { cx, cz, z },
                                    InteractTarget {
                                        kind: InteractTargetKind::Rock,
                                        id,
                                    },
                                    sprite,
                                    Transform::from_xyz(r.x, r.y, sort),
                                ));
                                crate::light2d::attach(
                                    &mut ent,
                                    &art,
                                    &[&format!("decor/rock{suffix}"), "decor/rock"],
                                    Vec2::new(1.3, 1.0),
                                );
                            }
                        }
                    }
                }
            }

            // Flora/props extra (F3): sólo visual, sólo si el slot TIENE arte
            // (cero entidades sin textura asignada) y sin pisar decor cosechable.
            if z == 0
                && !tiles.solid_at(body, tx, tz)
                && surface_decor_at(body, tx, tz).is_none()
                && !decor_free.contains(body, z, tx, tz)
            {
                let extra = nature_decor_at(body, tx, tz)
                    .or_else(|| prop_decor_at(body, tx, tz));
                if let Some(e) = extra {
                    if let Some(img) = art.get(e.key) {
                        let mut s = Sprite::from_image(img);
                        s.custom_size = Some(e.size.into());
                        // Pivote al pie (LimeZu): el centro sube medio alto.
                        let y = r.y + e.size[1] * 0.5 - 0.5;
                        let mut ent = commands.spawn((
                            SurfaceDecor { cx, cz, z },
                            s,
                            Transform::from_xyz(r.x, y, sort),
                        ));
                        // Faroles, fogatas, hongos brillantes: la luz viene del
                        // slot, no de una lista hardcodeada acá.
                        crate::light2d::attach(&mut ent, &art, &[e.key], e.size.into());
                    }
                }
            }

            // Ore/gem deposits (surface sparse, underground rich). Mined ones are
            // removed like rocks/trees.
            if let Some(dep) = deposit_at(body, z, tx, tz).filter(|_| !flora.is_removed(body, 0, 0, id)) {
                let kind = deposit_target_kind(dep);
                let (color, size) = deposit_style(kind);
                let key = match kind {
                    InteractTargetKind::GemRed => "decor/gem_red",
                    InteractTargetKind::GemBlue => "decor/gem_blue",
                    InteractTargetKind::OreGold => "decor/ore_gold",
                    _ => "decor/ore_iron",
                };
                let mut e = commands.spawn((
                    SurfaceDecor { cx, cz, z },
                    InteractTarget { kind, id },
                    art.sprite_or_color(key, color, size),
                    Transform::from_xyz(r.x, r.y, sort),
                ));
                if let Some(a) = art.animation(key) {
                    e.insert(a);
                }
                // Vetas de gema/mineral: brillan en la cueva si el slot lo declara.
                crate::light2d::attach(&mut e, &art, &[key], size);
            }
        }
    }
    // Decor AUTORADO (editor de mundo): pinta los árboles/props colocados a mano cuya celda
    // cae en este chunk. Sólo visual (sin cosecha). Tamaño de la tabla `worldgen` si se
    // conoce; si no, 1.5×1.5.
    for d in authored {
        if d.body != body as u8
            || d.z != z
            || d.x.div_euclid(CHUNK) != cx
            || d.zc.div_euclid(CHUNK) != cz
        {
            continue;
        }
        let wx = d.x as f32 * TILE_M + 0.5;
        let wz = d.zc as f32 * TILE_M + 0.5;
        let r = render_xy(wx, wz);
        // Tamaño: el que fijó el editor (deducido del recorte del asset) manda;
        // si no, el de la tabla de especies; si no, un decor chico.
        let size: [f32; 2] = d
            .size
            .filter(|s| s[0] > 0.0 && s[1] > 0.0)
            .or_else(|| {
                ha_shared::worldgen::TREE_SPECIES
                    .iter()
                    .find(|t| t.key == d.key)
                    .map(|t| t.size)
            })
            .or_else(|| {
                ha_shared::worldgen::NATURE_SPECIES
                    .iter()
                    .chain(ha_shared::worldgen::PROP_SPECIES.iter())
                    .find(|e| e.key == d.key)
                    .map(|e| e.size)
            })
            .unwrap_or([1.5, 1.5]);
        // `world_sprite` respeta el recorte/pose del slot: sin esto, un asset
        // recortado de un atlas se plantaba con el atlas entero.
        let Some((s, anim)) = art.world_sprite(&d.key, size.into()) else { continue };
        let y = r.y + size[1] * 0.5 - 0.5;
        let mut ent = commands.spawn((
            SurfaceDecor { cx, cz, z },
            s,
            Transform::from_xyz(r.x, y, sort_z(wz)),
        ));
        if let Some(a) = anim {
            ent.insert(a);
        }
        crate::light2d::attach(&mut ent, &art, &[d.key.as_str()], size.into());
    }

    flora.set_chunk_trees(cx as i32, cz as i32, trees);
}

fn rock_color(body: CelestialBody) -> Color {
    match body {
        CelestialBody::Moon => Color::srgb(0.55, 0.55, 0.58),
        CelestialBody::Earth => Color::srgb(0.48, 0.48, 0.50),
        _ => Color::srgb(0.52, 0.38, 0.30),
    }
}

fn deposit_style(kind: InteractTargetKind) -> (Color, Vec2) {
    match kind {
        InteractTargetKind::GemRed => (Color::srgb(0.85, 0.20, 0.25), Vec2::new(0.9, 0.9)),
        InteractTargetKind::GemBlue => (Color::srgb(0.25, 0.45, 0.90), Vec2::new(0.9, 0.9)),
        InteractTargetKind::OreGold => (Color::srgb(0.85, 0.70, 0.25), Vec2::new(1.0, 0.9)),
        _ => (Color::srgb(0.62, 0.48, 0.40), Vec2::new(1.0, 0.9)),
    }
}

// ---------------------------------------------------------------------------
// Autotile de tiles con arte (agua, cascadas…) — sprites por tile SÓLO para
// materiales con arte "vivo" (poses o animación). El suelo base sin arte sigue
// siendo el quad vertex-color barato (cero overhead hasta que asignes arte).
// ---------------------------------------------------------------------------

/// Clave de arte del material de un tile, o `None` si ese tile no tiene slot.
fn tile_art_key(tile: Tile) -> Option<&'static str> {
    match tile {
        Tile::Water => Some("tiles/water"),
        Tile::Grass => Some("tiles/grass"),
        Tile::Sand => Some("tiles/sand"),
        Tile::Regolith => Some("tiles/regolith"),
        Tile::Basalt => Some("tiles/basalt"),
        Tile::Ice => Some("tiles/ice"),
        // Piezas de construcción (autotile de Room_Builder). El muro legacy
        // (`Wall`) comparte el arte de madera.
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

/// Elige la forma autotile de una celda según si sus 8 vecinos son del MISMO
/// material (`true` = mismo → sin borde de ese lado). El borde "mira" hacia los
/// lados DISTINTOS. Los nombres son los que escribe el editor
/// (`center`/`edge_*`/`corner_*`/`inner_*`).
#[allow(clippy::too_many_arguments)]
fn autotile_shape(n: bool, e: bool, s: bool, w: bool, ne: bool, nw: bool, se: bool, sw: bool) -> &'static str {
    match (!n, !e, !s, !w) {
        // Todos los lados iguales: puede ser esquina cóncava si una diagonal difiere.
        (false, false, false, false) => {
            if !ne { "inner_ne" } else if !nw { "inner_nw" } else if !se { "inner_se" } else if !sw { "inner_sw" } else { "center" }
        }
        // Un solo lado distinto → borde de ese lado.
        (true, false, false, false) => "edge_n",
        (false, true, false, false) => "edge_e",
        (false, false, true, false) => "edge_s",
        (false, false, false, true) => "edge_w",
        // Dos lados adyacentes distintos → esquina convexa.
        (true, true, false, false) => "corner_ne",
        (true, false, false, true) => "corner_nw",
        (false, true, true, false) => "corner_se",
        (false, false, true, true) => "corner_sw",
        // Lados opuestos / 3-4 lados distintos → center como fallback seguro.
        _ => "center",
    }
}

/// Spawnea sprites por-tile para los materiales con arte "vivo" del chunk (agua…),
/// eligiendo la variante autotile por vecinos. Cada sprite lleva `SurfaceDecor`
/// para despawnearse con el streaming de chunks. Sólo la superficie (z==0) por
/// ahora; los tiles sin arte no cuestan nada.
fn spawn_chunk_tile_art(
    commands: &mut Commands,
    art: &ArtLibrary,
    tiles: &TileWorld,
    body: CelestialBody,
    z: i32,
    cx: i64,
    cz: i64,
) {
    for lx in 0..CHUNK {
        for lz in 0..CHUNK {
            let tx = cx * CHUNK + lx;
            let tz = cz * CHUNK + lz;
            let tile = tiles.tile_at(body, tx, tz);
            let Some(key) = tile_art_key(tile) else { continue };
            let wx = tx as f32 * TILE_M + 0.5;
            let wz = tz as f32 * TILE_M + 0.5;
            let r = render_xy(wx, wz);
            // Puertas: pose open/closed animada por proximidad (building.rs las
            // conmuta). Con arte de poses, spawn especial; si no, sigue al autotile.
            if tile == Tile::Door && art.has_poses(key) {
                if let Some((_, _)) = art.pose(key, "closed").or_else(|| art.pose(key, "open")) {
                    let sprite = art.sprite_or_color(key, Color::srgb(0.30, 0.20, 0.10), Vec2::splat(TILE_M));
                    let mut ent = commands.spawn((
                        SurfaceDecor { cx, cz, z },
                        TileArt,
                        crate::building::DoorArt { tx, tz, open: false },
                        sprite,
                        Transform::from_xyz(r.x, r.y, sort_z(wz)),
                    ));
                    crate::light2d::attach(&mut ent, &art, &[key], Vec2::splat(TILE_M));
                    continue;
                }
            }
            if !art.tile_has_art(key) {
                continue;
            }
            let same = |dx: i64, dz: i64| tiles.tile_at(body, tx + dx, tz + dz) == tile;
            // n=norte(−z) e=este(+x) s=sur(+z) w=oeste(−x) + diagonales.
            let shape = autotile_shape(
                same(0, -1), same(1, 0), same(0, 1), same(-1, 0),
                same(1, -1), same(-1, -1), same(1, 1), same(-1, 1),
            );
            let Some((sprite, anim)) = art.tile_sprite(key, shape, Vec2::splat(TILE_M)) else { continue };
            // Suelo: capa baja (encima del mesh, debajo del decor). Muros y
            // piezas sólidas: Y-sort, el jugador camina "detrás" de ellas.
            let depth = if tile.solid() { sort_z(wz) } else { 1.0 };
            let mut ent = commands.spawn((
                SurfaceDecor { cx, cz, z },
                TileArt,
                sprite,
                Transform::from_xyz(r.x, r.y, depth),
            ));
            if let Some(a) = anim {
                ent.insert(a);
            }
            // Tiles que iluminan (lava, cristal, un piso de luz). Sólo pagan quad
            // los materiales que el usuario marcó como emisores.
            crate::light2d::attach(&mut ent, &art, &[key], Vec2::splat(TILE_M));
        }
    }
}

/// Regeneración de césped (pedido #2): las celdas de tierra expuesta (cavar,
/// demoler un piso, tierra colocada) sobre césped natural vuelven a pasto tras
/// 45–120 s. Al expirar se BORRA el edit → el terreno natural (Grass) reaparece
/// solo, con rebuild de mesh + autotile vía `edit_log`. Barato: corre cada 2 s
/// y sólo recorre la lista pendiente (vacía casi siempre).
fn tick_grass_regrowth(time: Res<Time>, mut acc: Local<f32>, mut tiles: ResMut<TileWorld>) {
    *acc += time.delta_secs();
    if *acc < 2.0 {
        return;
    }
    let step = *acc;
    *acc = 0.0;
    if tiles.regrow.is_empty() {
        return;
    }
    let mut changed = false;
    let mut i = 0;
    while i < tiles.regrow.len() {
        tiles.regrow[i].timer -= step;
        if tiles.regrow[i].timer > 0.0 {
            i += 1;
            continue;
        }
        let RegrowCell { body, tx, tz, .. } = tiles.regrow.swap_remove(i);
        let key = (body, 0, tx, tz);
        // Sólo si sigue siendo tierra expuesta (el jugador pudo construir ahí).
        if tiles.edits.get(&key) == Some(&Tile::Dirt) {
            tiles.edits.remove(&key);
            tiles.edit_log.push((tx, tz));
            changed = true;
        }
    }
    if changed {
        tiles.version = tiles.version.wrapping_add(1);
        tiles.dirty = true;
    }
}

/// Marca los sprites de arte de tile (autotile/puertas) para poder re-spawnearlos
/// cuando una edición cambia la forma de los vecinos — sin tocar el resto del
/// decor del chunk (árboles, portales, depósitos).
#[derive(Component)]
pub struct TileArt;

/// El pedido explícito del usuario: al colocar/cavar una pieza, los 8 vecinos
/// deben re-elegir su variante autotile (borde/esquina) — si no, quedan
/// desalineados. Drena `TileWorld::edit_log` y re-spawnea el arte de los chunks
/// tocados (una edición en el borde de un chunk afecta al de al lado: se expande
/// la celda ±1 antes de mapear a chunks). Costo cero sin ediciones.
fn rebuild_edited_tile_art(
    mut commands: Commands,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    mut tiles: ResMut<TileWorld>,
    art: Res<ArtLibrary>,
    tile_art: Query<(Entity, &SurfaceDecor), With<TileArt>>,
) {
    if tiles.edit_log.is_empty() {
        return;
    }
    let edits = std::mem::take(&mut tiles.edit_log);
    if surface_region_active(&region).is_none() {
        return;
    }
    let body = active_surface_body(&ctx, &planet);
    let z = tiles.z;
    // Chunks afectados = celdas editadas ±1 (los vecinos cruzan bordes de chunk).
    let mut dirty: HashSet<(i64, i64)> = HashSet::new();
    for (tx, tz) in edits {
        for dx in -1..=1i64 {
            for dz in -1..=1i64 {
                dirty.insert(((tx + dx).div_euclid(CHUNK), (tz + dz).div_euclid(CHUNK)));
            }
        }
    }
    for (e, d) in &tile_art {
        if d.z == z && dirty.contains(&(d.cx, d.cz)) {
            commands.entity(e).despawn_recursive();
        }
    }
    // Re-spawnea el arte de cada chunk tocado (incluye chunks que aún no tenían
    // arte: p.ej. la primera pieza colocada en un chunk sin agua).
    for (cx, cz) in dirty {
        spawn_chunk_tile_art(&mut commands, &art, &tiles, body, z, cx, cz);
    }
}

// ---------------------------------------------------------------------------
// NPCs / colonos — sprites idle en la base lunar, uno por colono del sim
// abstracto (`colony.state.colonists`). Usan el MISMO esquema de poses que el
// jugador (`npc/colonist` → "idle_down"); sin arte caen a una cápsula de color.
// ---------------------------------------------------------------------------

// Reemplazados por `society.rs` (F6): NPCs con cerebro + memoria. Se conservan por si
// hace falta el sprite idle simple; hoy no se schedulean.
#[allow(dead_code)]
#[derive(Component)]
pub struct Colonist;

/// Sprite idle de un colono (o cápsula si no hay arte).
#[allow(dead_code)]
fn colonist_sprite(art: &ArtLibrary) -> (Sprite, Option<crate::art2d::ArtAnimated>) {
    let size = Vec2::new(1.0, 1.6);
    let pose_name = ["idle_down", "idle"]
        .into_iter()
        .find(|n| art.has_pose("npc/colonist", n));
    if let Some((rect_px, anim)) = pose_name.and_then(|n| art.pose("npc/colonist", n)) {
        // La hoja puede ser la del slot o la propia de esa pose (colono armado
        // con un PNG por dirección).
        if let Some(image) = pose_name.and_then(|n| art.pose_image("npc/colonist", n)) {
            let mut s = Sprite::from_image(image);
            s.custom_size = Some(size);
            if anim.is_none() {
                let [x, y, w, h] = rect_px;
                s.rect = Some(Rect::from_corners(
                    Vec2::new(x as f32, y as f32),
                    Vec2::new((x + w) as f32, (y + h) as f32),
                ));
            }
            return (s, anim);
        }
    }
    if let Some(img) = art.get("npc/colonist") {
        let mut s = Sprite::from_image(img);
        s.custom_size = Some(size);
        return (s, art.animation("npc/colonist"));
    }
    (Sprite::from_color(Color::srgb(0.85, 0.82, 0.75), Vec2::new(0.7, 1.4)), None)
}

/// Mantiene un sprite por colono junto a los hábitats de la Luna, sólo cuando
/// estás en la superficie lunar. Respawnea cuando cambia la cuenta o el cuerpo.
#[allow(dead_code)]
fn sync_colonists(
    mut commands: Commands,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    colony: Res<crate::colony::ColonyRuntime>,
    machines: Res<crate::machines::MachineWorld>,
    art: Res<ArtLibrary>,
    existing: Query<Entity, With<Colonist>>,
) {
    let on_moon = surface_region_active(&region).is_some()
        && active_surface_body(&ctx, &planet) == CelestialBody::Moon
        && tiles.z == 0;
    let want = if on_moon { colony.state.colonists } else { 0 };
    let have = existing.iter().count() as u32;
    if want == have {
        return;
    }
    for e in &existing {
        commands.entity(e).despawn_recursive();
    }
    if want == 0 {
        return;
    }
    // Anclas = hábitats en la Luna (o el origen si aún no hay).
    let anchors: Vec<(f32, f32)> = machines
        .on_body(CelestialBody::Moon)
        .filter(|m| m.machine_id == ha_shared::HABITAT.id)
        .map(|m| (m.x, m.z))
        .collect();
    let n_anchor = anchors.len().max(1) as u32;
    for i in 0..want {
        let (bx, bz) = if anchors.is_empty() {
            (0.0, 0.0)
        } else {
            anchors[(i % n_anchor) as usize]
        };
        // Cluster en espiral (ángulo áureo) alrededor del hábitat.
        let ring = (i / n_anchor) as f32;
        let ang = i as f32 * 2.399_963_2;
        let rad = 1.5 + ring * 1.2;
        let wx = bx + ang.cos() * rad;
        let wz = bz + 1.0 + ang.sin() * rad;
        let r = render_xy(wx, wz);
        let (sprite, anim) = colonist_sprite(&art);
        let mut ent = commands.spawn((Colonist, sprite, Transform::from_xyz(r.x, r.y, sort_z(wz))));
        if let Some(a) = anim {
            ent.insert(a);
        }
    }
}

// ---------------------------------------------------------------------------
// Persistence (B10)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Default)]
struct TilesSave {
    /// (body, z, tx, tz, kind name)
    edits: Vec<(u8, i32, i64, i64, String)>,
}

fn tiles_path() -> std::path::PathBuf {
    persist_dir().join("tiles.json")
}

fn load_tiles_on_startup(mut tiles: ResMut<TileWorld>) {
    let Ok(raw) = std::fs::read_to_string(tiles_path()) else {
        return;
    };
    let Ok(save) = serde_json::from_str::<TilesSave>(&raw) else {
        return;
    };
    for (body, z, tx, tz, name) in save.edits {
        tiles.edits.insert((body, z, tx, tz), tile_from_name(&name));
    }
    // Re-programar el rebrote de césped para la tierra expuesta que quedó
    // guardada de sesiones anteriores (el timer no se persiste — se reinicia).
    let pending: Vec<(u8, i64, i64)> = tiles
        .edits
        .iter()
        .filter(|((_, z, _, _), t)| *z == 0 && **t == Tile::Dirt)
        .map(|((b, _, tx, tz), _)| (*b, *tx, *tz))
        .collect();
    for (b, tx, tz) in pending {
        if let Some(body) = CelestialBody::from_u8(b) {
            tiles.maybe_schedule_regrow(body, tx, tz);
        }
    }
    tiles.version = tiles.version.wrapping_add(1);
}

fn debounced_tiles_save(time: Res<Time>, mut tiles: ResMut<TileWorld>) {
    let paused = is_app_paused();
    let flush_now = paused && !tiles.was_paused && tiles.dirty;
    tiles.was_paused = paused;
    if !tiles.dirty {
        return;
    }
    tiles.debounce += time.delta_secs();
    if !flush_now && tiles.debounce < 20.0 {
        return;
    }
    tiles.debounce = 0.0;
    tiles.dirty = false;
    let save = TilesSave {
        edits: tiles
            .edits
            .iter()
            .map(|((b, z, tx, tz), t)| (*b, *z, *tx, *tz, tile_name(*t).to_string()))
            .collect(),
    };
    if let Ok(json) = serde_json::to_string(&save) {
        let _ = std::fs::write(tiles_path(), json);
    }
}
