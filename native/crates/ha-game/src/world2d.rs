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

/// Screen render position of a world ground point `(x, z)` (metres).
/// North (−z) is up. Shared with the orbital view.
pub fn render_xy(x: f32, z: f32) -> Vec2 {
    Vec2::new(x, -z)
}

/// Y-sort depth for a sprite at world z (more south / larger z draws in front).
pub fn sort_z(world_z: f32) -> f32 {
    2.0 + world_z * 0.001
}

/// Pack a grid cell into a stable u32 id for flora/harvest bookkeeping
/// (±32 km range). Trees/rocks use a per-body flat set at chunk key (0,0).
pub fn pack_cell(tx: i64, tz: i64) -> u32 {
    ((tx as i16 as u16 as u32) << 16) | (tz as i16 as u16 as u32)
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

pub fn active_landable_body(
    region: &RegionRuntime,
    ctx: &WorldContextRuntime,
    flight: &crate::rocket_flight::RocketFlight,
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
        Tile::RockRed => "rock_red",
        Tile::Stone => "stone",
        Tile::RockDark => "rock_dark",
        Tile::Bedrock => "bedrock",
        Tile::CaveFloor => "cave_floor",
        Tile::WoodFloor => "wood_floor",
        Tile::Wall => "wall",
    }
}

fn tile_from_name(name: &str) -> Tile {
    match name {
        "grass" => Tile::Grass,
        "dirt" => Tile::Dirt,
        "sand" => Tile::Sand,
        "water" => Tile::Water,
        "regolith" => Tile::Regolith,
        "rock_red" => Tile::RockRed,
        "stone" => Tile::Stone,
        "rock_dark" => Tile::RockDark,
        "bedrock" => Tile::Bedrock,
        "cave_floor" => Tile::CaveFloor,
        "wood_floor" => Tile::WoodFloor,
        "wall" => Tile::Wall,
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
    dirty: bool,
    was_paused: bool,
    debounce: f32,
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
        Some(t)
    }

    pub fn place_tile(&mut self, body: CelestialBody, tx: i64, tz: i64, kind: Tile) -> bool {
        if self.solid_at(body, tx, tz) {
            return false;
        }
        self.edits.insert((body as u8, self.z, tx, tz), kind);
        self.version = self.version.wrapping_add(1);
        self.dirty = true;
        true
    }
}

// ---------------------------------------------------------------------------
// Decor (trees/rocks) + surface deposits — top-down, per grid cell
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DecorKind {
    Tree,
    Rock,
}

fn hash_cell(seed: u32, tx: i64, tz: i64) -> u32 {
    let a = (tx as i32 as u32).wrapping_mul(0x9e37_79b9);
    let b = (tz as i32 as u32).wrapping_mul(0x85eb_ca6b);
    let mut x = seed ^ a ^ b;
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

/// Deterministic surface decor at a grid cell (only on walkable surface tiles).
pub fn surface_decor_at(body: CelestialBody, tx: i64, tz: i64) -> Option<DecorKind> {
    let seed = ha_shared::body_terrain_seed(body) ^ 0x1234_abcd;
    let tile = topdown_tile(body, 0, tx, tz);
    if tile.solid() || tile == Tile::Water {
        return None;
    }
    let h = hash_cell(seed, tx, tz);
    match body {
        CelestialBody::Earth => {
            if tile == Tile::Grass {
                match h % 100 {
                    0..=7 => Some(DecorKind::Tree),
                    8..=11 => Some(DecorKind::Rock),
                    _ => None,
                }
            } else {
                match h % 100 {
                    0..=3 => Some(DecorKind::Rock),
                    _ => None,
                }
            }
        }
        _ => match h % 100 {
            0..=5 => Some(DecorKind::Rock),
            _ => None,
        },
    }
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
}

#[derive(Component)]
pub struct SurfaceDecor {
    pub cx: i64,
    pub cz: i64,
    pub z: i32,
}

#[derive(Resource, Default)]
pub struct ChunkMaterial(pub Handle<ColorMaterial>);

pub struct World2dPlugin;

impl Plugin for World2dPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TileWorld>()
            .init_resource::<ActivePlanet>()
            .init_resource::<ChunkMaterial>()
            .add_systems(Startup, (load_tiles_on_startup, init_chunk_material))
            .add_systems(
                Update,
                (
                    apply_tile_hold.before(crate::hud::update_interact_hud),
                    try_place_tile_on_hold.before(crate::hud::update_interact_hud),
                    layer_transition,
                    stream_tile_chunks,
                    rebuild_dirty_chunks,
                    debounced_tiles_save,
                ),
            );
    }
}

fn init_chunk_material(
    mut mat: ResMut<ChunkMaterial>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    mat.0 = materials.add(ColorMaterial::default());
}

pub fn surface_region_active(region: &RegionRuntime) -> Option<()> {
    match region.active {
        RegionId::Overworld { .. } | RegionId::Luna | RegionId::Planet(_) | RegionId::Cave(_) => {
            Some(())
        }
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
    match portal_here(body, tiles.z, tx, tz) {
        Some(portal) if *armed => {
            let new_z = match portal {
                Portal::Descend => (tiles.z - 1).max(MIN_Z_LEVEL),
                Portal::Ascend => tiles.z + 1,
            };
            if new_z != tiles.z {
                tiles.z = new_z;
                tiles.version = tiles.version.wrapping_add(1);
                *armed = false;
                eprintln!("ha-layer: → z={new_z} ({:?})", portal);
            }
        }
        Some(_) => {}
        None => *armed = true,
    }
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
    player: Query<&Transform, With<Player>>,
    chunks: Query<(Entity, &TileChunk)>,
    decor: Query<(Entity, &SurfaceDecor)>,
) {
    if surface_region_active(&region).is_none() {
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

    let mut live: HashSet<(i64, i64)> = HashSet::new();
    for (e, chunk) in &chunks {
        let stale = chunk.body != body
            || chunk.z != z
            || (chunk.cx - pcx).abs() > CHUNK_RANGE + 1
            || (chunk.cz - pcz).abs() > CHUNK_RANGE + 1;
        if stale {
            commands.entity(e).despawn_recursive();
            flora.clear_chunk(chunk.cx as i32, chunk.cz as i32);
        } else {
            live.insert((chunk.cx, chunk.cz));
        }
    }
    for (e, d) in &decor {
        if d.z != z
            || (d.cx - pcx).abs() > CHUNK_RANGE + 1
            || (d.cz - pcz).abs() > CHUNK_RANGE + 1
        {
            commands.entity(e).despawn_recursive();
        }
    }

    for cx in (pcx - CHUNK_RANGE)..=(pcx + CHUNK_RANGE) {
        for cz in (pcz - CHUNK_RANGE)..=(pcz + CHUNK_RANGE) {
            if live.contains(&(cx, cz)) {
                continue;
            }
            let mesh = build_chunk_mesh(&tiles, body, cx, cz);
            commands.spawn((
                TileChunk {
                    body,
                    z,
                    cx,
                    cz,
                    built_version: tiles.version,
                },
                Mesh2d(meshes.add(mesh)),
                MeshMaterial2d(mat.0.clone()),
                Transform::from_xyz(0.0, 0.0, 0.0),
            ));
            spawn_chunk_decor(&mut commands, &mut flora, &art, body, z, cx, cz);
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

fn spawn_chunk_decor(
    commands: &mut Commands,
    flora: &mut FloraHarvest,
    art: &ArtLibrary,
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
                commands.spawn((
                    SurfaceDecor { cx, cz, z },
                    art.sprite_or_color(key, color, size),
                    Transform::from_xyz(r.x, r.y, sort - 0.5),
                ));
            }

            // Surface decor only on the surface layer.
            if z == 0 {
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
                                let sprite = art
                                    .get_first(&[&format!("decor/tree{suffix}"), "decor/tree"])
                                    .map(|img| {
                                        let mut s = Sprite::from_image(img);
                                        s.custom_size = Some(Vec2::new(2.4, 3.2));
                                        s
                                    })
                                    .unwrap_or_else(|| {
                                        Sprite::from_color(
                                            Color::srgb(0.30, 0.55, 0.25),
                                            Vec2::new(2.2, 2.6),
                                        )
                                    });
                                commands.spawn((
                                    SurfaceDecor { cx, cz, z },
                                    InteractTarget {
                                        kind: InteractTargetKind::Tree,
                                        id,
                                    },
                                    crate::render::particles::HarvestReactive,
                                    sprite,
                                    Transform::from_xyz(r.x, r.y + 1.0, sort),
                                ));
                            }
                        }
                        DecorKind::Rock => {
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
                            commands.spawn((
                                SurfaceDecor { cx, cz, z },
                                InteractTarget {
                                    kind: InteractTargetKind::Rock,
                                    id,
                                },
                                sprite,
                                Transform::from_xyz(r.x, r.y, sort),
                            ));
                        }
                    }
                }
            }

            // Ore/gem deposits (surface sparse, underground rich).
            if let Some(dep) = deposit_at(body, z, tx, tz) {
                let kind = deposit_target_kind(dep);
                let (color, size) = deposit_style(kind);
                let key = match kind {
                    InteractTargetKind::GemRed => "decor/gem_red",
                    InteractTargetKind::GemBlue => "decor/gem_blue",
                    InteractTargetKind::OreGold => "decor/ore_gold",
                    _ => "decor/ore_iron",
                };
                commands.spawn((
                    SurfaceDecor { cx, cz, z },
                    InteractTarget { kind, id },
                    art.sprite_or_color(key, color, size),
                    Transform::from_xyz(r.x, r.y, sort),
                ));
            }
        }
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
