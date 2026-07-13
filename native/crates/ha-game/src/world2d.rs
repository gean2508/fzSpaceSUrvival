//! 2D tile world — Terraria-style diggable surface strip per celestial body.
//!
//! Render frame on a surface: x = arc metres `s` (unwrapped near the player),
//! y = height above the walk shell. Tile content wraps the circumference so
//! walking keeps going around the planet. Chunks are Mesh2d quads with vertex
//! colors; edits (dig/place) are sparse overrides persisted to tiles.json.

use std::collections::{HashMap, HashSet};

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use serde::{Deserialize, Serialize};

use ha_shared::flat2d::{
    body_circumference_tiles, ground_row, natural_tile, wrap_tile, TileKind, DIG_DEPTH_TILES,
    TILE_M,
};
use ha_shared::{CelestialBody, InteractTargetKind, WorldContext};

use crate::art2d::ArtLibrary;
use crate::harvest::{FloraHarvest, TreeInstance};
use crate::inventory_persist::persist_dir;
use crate::platform::is_app_paused;
use crate::scene::{RegionId, RegionRuntime, WorldContextRuntime};
use crate::sim::Player;
use crate::targeting::InteractTarget;

pub const CHUNK_TILES: i64 = 32;
/// Tile rows meshed per chunk (vertical band around the terrain line).
pub const ROW_MIN: i32 = -(DIG_DEPTH_TILES + 34);
pub const ROW_MAX: i32 = 44;
/// Chunks kept alive around the player (each 32 m wide).
const CHUNK_RANGE: i64 = 5;

/// Body whose surface the player is currently on (compat shape with the old
/// `planet_world::ActivePlanet`).
#[derive(Resource, Default, Clone, Copy)]
pub struct ActivePlanet {
    pub body: Option<CelestialBody>,
}

/// The body the player currently walks on, resolved from context.
pub fn active_surface_body(ctx: &WorldContextRuntime, planet: &ActivePlanet) -> CelestialBody {
    match ctx.active {
        WorldContext::Luna => CelestialBody::Moon,
        WorldContext::Planet => planet.body.unwrap_or(CelestialBody::Mars),
        _ => CelestialBody::Earth,
    }
}

/// Body the player can act on right now (None mid-flight / in space).
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
// Tile storage (natural terrain + sparse edits + embedded deposits)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Default)]
struct TilesSave {
    /// body u8 → list of (wrapped col, row, kind name or "" = dug out)
    edits: HashMap<u8, Vec<(u32, i32, String)>>,
}

fn tile_kind_name(kind: TileKind) -> &'static str {
    match kind {
        TileKind::Air => "",
        TileKind::Grass => "grass",
        TileKind::Dirt => "dirt",
        TileKind::Stone => "stone",
        TileKind::Sand => "sand",
        TileKind::Regolith => "regolith",
        TileKind::RockDark => "rock_dark",
        TileKind::RockRed => "rock_red",
        TileKind::Water => "water",
        TileKind::Bedrock => "bedrock",
        TileKind::Wood => "wood",
    }
}

fn tile_kind_from_name(name: &str) -> TileKind {
    match name {
        "grass" => TileKind::Grass,
        "dirt" => TileKind::Dirt,
        "stone" => TileKind::Stone,
        "sand" => TileKind::Sand,
        "regolith" => TileKind::Regolith,
        "rock_dark" => TileKind::RockDark,
        "rock_red" => TileKind::RockRed,
        "water" => TileKind::Water,
        "bedrock" => TileKind::Bedrock,
        "wood" => TileKind::Wood,
        _ => TileKind::Air,
    }
}

/// TileKind for a placeable block item, if the item is a block.
pub fn tile_for_block_item(item_id: &str) -> Option<TileKind> {
    match item_id {
        "wood" => Some(TileKind::Wood),
        "stone" => Some(TileKind::Stone),
        "dirt" => Some(TileKind::Dirt),
        "sand" => Some(TileKind::Sand),
        "regolith" => Some(TileKind::Regolith),
        _ => None,
    }
}

#[derive(Resource, Default)]
pub struct TileWorld {
    /// (body u8, wrapped col, row) → override (Air = dug out).
    edits: HashMap<(u8, u32, i32), TileKind>,
    pub version: u64,
    dirty: bool,
    was_paused: bool,
    debounce: f32,
}

impl TileWorld {
    pub fn tile_at(&self, body: CelestialBody, i: i64, j: i32) -> TileKind {
        let c = body_circumference_tiles(body);
        let col = wrap_tile(i, c);
        if let Some(kind) = self.edits.get(&(body as u8, col, j)) {
            return *kind;
        }
        natural_tile(body, col as i64, j)
    }

    pub fn solid_at(&self, body: CelestialBody, i: i64, j: i32) -> bool {
        self.tile_at(body, i, j).solid()
    }

    /// Topmost solid row at column `i` considering edits (for placement).
    pub fn ground_row_at(&self, body: CelestialBody, i: i64) -> i32 {
        let natural = ground_row(body, i);
        let mut j = (natural + 40).min(ROW_MAX);
        while j > natural - DIG_DEPTH_TILES {
            if self.solid_at(body, i, j) {
                return j;
            }
            j -= 1;
        }
        natural
    }

    /// Ground height in metres at arc position `s` (top face of ground tile).
    pub fn ground_height_at(&self, body: CelestialBody, s: f32) -> f32 {
        (self.ground_row_at(body, (s / TILE_M).floor() as i64) + 1) as f32 * TILE_M
    }

    /// Dig out a tile. Returns the removed kind if it was diggable.
    pub fn break_tile(&mut self, body: CelestialBody, i: i64, j: i32) -> Option<TileKind> {
        let kind = self.tile_at(body, i, j);
        if !kind.solid() || kind == TileKind::Bedrock {
            return None;
        }
        let c = body_circumference_tiles(body);
        self.edits
            .insert((body as u8, wrap_tile(i, c), j), TileKind::Air);
        self.version = self.version.wrapping_add(1);
        self.dirty = true;
        Some(kind)
    }

    /// Place a tile into an empty cell.
    pub fn place_tile(&mut self, body: CelestialBody, i: i64, j: i32, kind: TileKind) -> bool {
        if self.tile_at(body, i, j).solid() {
            return false;
        }
        let c = body_circumference_tiles(body);
        self.edits.insert((body as u8, wrap_tile(i, c), j), kind);
        self.version = self.version.wrapping_add(1);
        self.dirty = true;
        true
    }
}

// ---------------------------------------------------------------------------
// Embedded deposits (ore/gem nodes revealed by digging) + surface decor
// ---------------------------------------------------------------------------

pub struct EmbeddedDeposit {
    pub col: u32,
    pub row: i32,
    pub kind: InteractTargetKind,
}

/// Deterministic ore/gem node for a column, if any (revealed by digging).
pub fn embedded_deposit_at(body: CelestialBody, col: u32) -> Option<EmbeddedDeposit> {
    let seed = ha_shared::body_terrain_seed(body) ^ 0x6f5e_a213;
    let h = hash2(seed, col, 7);
    let g = ground_row(body, col as i64);
    let depth_span = DIG_DEPTH_TILES - 12;
    let row = g - 8 - (h >> 8) as i32 % depth_span.max(1);
    let roll = h % 1000;
    let kind = match body {
        CelestialBody::Moon => match roll {
            0..=24 => Some(InteractTargetKind::GemRed),
            25..=79 => Some(InteractTargetKind::OreIron),
            _ => None,
        },
        CelestialBody::Earth => match roll {
            0..=39 => Some(InteractTargetKind::OreIron),
            40..=59 => Some(InteractTargetKind::OreGold),
            60..=74 => Some(InteractTargetKind::GemBlue),
            _ => None,
        },
        _ => match roll {
            0..=49 => Some(InteractTargetKind::OreIron),
            50..=79 => Some(InteractTargetKind::GemBlue),
            _ => None,
        },
    };
    kind.map(|kind| EmbeddedDeposit { col, row, kind })
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DecorKind {
    Tree,
    Rock,
}

/// Deterministic surface decor for a column, if any.
pub fn surface_decor_at(body: CelestialBody, col: u32) -> Option<DecorKind> {
    let seed = ha_shared::body_terrain_seed(body) ^ 0x1234_abcd;
    let h = hash2(seed, col, 3);
    match body {
        CelestialBody::Earth => {
            // No trees under water.
            if ground_row(body, col as i64) <= -3 {
                return match h % 100 {
                    0..=3 => Some(DecorKind::Rock),
                    _ => None,
                };
            }
            match h % 100 {
                0..=10 => Some(DecorKind::Tree),
                11..=17 => Some(DecorKind::Rock),
                _ => None,
            }
        }
        _ => match h % 100 {
            0..=8 => Some(DecorKind::Rock),
            _ => None,
        },
    }
}

fn hash2(seed: u32, a: u32, b: u32) -> u32 {
    let mut x = seed ^ a.wrapping_mul(0x9e37_79b9) ^ b.wrapping_mul(0x85eb_ca6b);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

// ---------------------------------------------------------------------------
// Chunk streaming + meshing
// ---------------------------------------------------------------------------

#[derive(Component)]
pub struct TileChunk {
    pub body: CelestialBody,
    /// Unwrapped chunk index (render window near the player).
    pub k: i64,
    pub built_version: u64,
}

/// Marker for decor/deposit sprites belonging to a chunk window.
#[derive(Component)]
pub struct SurfaceDecor {
    pub k: i64,
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
                    apply_tile_hold,
                    try_place_tile_on_hold,
                    stream_tile_chunks,
                    rebuild_dirty_chunks,
                    debounced_tiles_save,
                ),
            );
    }
}

/// Dig the targeted tile when the hold completes (consumes `hold.completed`
/// BEFORE the HUD's generic grant path — same pattern the old block world
/// used).
fn apply_tile_hold(
    mut hold: ResMut<crate::input::HoldInteractState>,
    target: Res<crate::targeting::LookTarget>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    mut tiles: ResMut<TileWorld>,
    mut inv: ResMut<crate::inventory_persist::PlayerInventory>,
    mut inv_ui: ResMut<crate::hud::inventory_ui::InventoryUiState>,
    mut persist: ResMut<crate::inventory_persist::InventoryPersistState>,
) {
    if !hold.completed || target.kind != Some(InteractTargetKind::Block) {
        return;
    }
    let Some((col, j, _)) = target.block_cell else {
        return;
    };
    hold.completed = false;
    let body = active_surface_body(&ctx, &planet);
    if let Some(kind) = tiles.break_tile(body, col as i64, j) {
        if let Some(item) = kind.dug_item() {
            crate::hud::inventory_ui::add_item_to_slots(&mut inv_ui, item, 1);
            crate::hud::inventory_ui::flush_slots_to_inventory(&inv_ui, &mut inv);
            persist.mark_immediate();
        }
    }
}

/// Place the equipped block item into the targeted empty cell.
fn try_place_tile_on_hold(
    mut hold: ResMut<crate::input::HoldInteractState>,
    target: Res<crate::targeting::LookTarget>,
    equipped: Res<crate::input::EquippedItem>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    mut tiles: ResMut<TileWorld>,
    mut inv: ResMut<crate::inventory_persist::PlayerInventory>,
    mut inv_ui: ResMut<crate::hud::inventory_ui::InventoryUiState>,
    mut persist: ResMut<crate::inventory_persist::InventoryPersistState>,
) {
    if !hold.completed || !hold.placing_block {
        return;
    }
    let Some((col, j, _)) = target.place_cell else {
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
    if tiles.place_tile(body, col as i64, j, kind) {
        crate::hud::inventory_ui::consume_from_ui_slots(&mut inv_ui, &[(&equipped.item_id, 1)]);
        crate::hud::inventory_ui::flush_slots_to_inventory(&inv_ui, &mut inv);
        persist.mark_immediate();
    }
}

fn init_chunk_material(
    mut mat: ResMut<ChunkMaterial>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    mat.0 = materials.add(ColorMaterial::default());
}

/// Is the player currently in a surface region (tiles visible)?
pub fn surface_region_active(region: &RegionRuntime) -> Option<()> {
    match region.active {
        RegionId::Overworld { .. } | RegionId::Luna | RegionId::Planet(_) | RegionId::Cave(_) => {
            Some(())
        }
        _ => None,
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
        // B6: leaving the surface drops every chunk + decor entity.
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
    let player_k = (player_tf.translation.x / (CHUNK_TILES as f32 * TILE_M)).floor() as i64;

    let mut live: HashSet<i64> = HashSet::new();
    for (e, chunk) in &chunks {
        if chunk.body != body || (chunk.k - player_k).abs() > CHUNK_RANGE + 1 {
            commands.entity(e).despawn_recursive();
            let c = body_circumference_tiles(chunk.body);
            let wrapped_k = wrap_tile(chunk.k * CHUNK_TILES, c) as i32 / CHUNK_TILES as i32;
            flora.clear_chunk(wrapped_k, 0);
        } else {
            live.insert(chunk.k);
        }
    }
    for (e, d) in &decor {
        if (d.k - player_k).abs() > CHUNK_RANGE + 1 {
            commands.entity(e).despawn_recursive();
        }
    }

    for k in (player_k - CHUNK_RANGE)..=(player_k + CHUNK_RANGE) {
        if live.contains(&k) {
            continue;
        }
        let mesh = build_chunk_mesh(&tiles, body, k);
        commands.spawn((
            TileChunk {
                body,
                k,
                built_version: tiles.version,
            },
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(mat.0.clone()),
            Transform::from_xyz(0.0, 0.0, 0.0),
        ));
        spawn_chunk_decor(&mut commands, &tiles, &mut flora, &art, body, k);
    }
}

/// Body-specific art key suffix (falls back to the generic key).
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
        if chunk.built_version == tiles.version {
            continue;
        }
        let mesh = build_chunk_mesh(&tiles, chunk.body, chunk.k);
        meshes.insert(mesh2d.0.id(), mesh);
        chunk.built_version = tiles.version;
    }
}

fn build_chunk_mesh(tiles: &TileWorld, body: CelestialBody, k: i64) -> Mesh {
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(4096);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(4096);
    let mut indices: Vec<u32> = Vec::with_capacity(6144);
    let c = body_circumference_tiles(body);

    let mut quad = |x0: f32, y0: f32, size: f32, z: f32, rgba: [f32; 4]| {
        let base = positions.len() as u32;
        positions.push([x0, y0, z]);
        positions.push([x0 + size, y0, z]);
        positions.push([x0 + size, y0 + size, z]);
        positions.push([x0, y0 + size, z]);
        for _ in 0..4 {
            colors.push(rgba);
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    };

    for local in 0..CHUNK_TILES {
        let i = k * CHUNK_TILES + local;
        let col = wrap_tile(i, c);
        let natural_ground = ground_row(body, col as i64);
        for j in ROW_MIN..=ROW_MAX {
            let kind = tiles.tile_at(body, i, j);
            let x = i as f32 * TILE_M;
            let y = j as f32 * TILE_M;
            match kind {
                TileKind::Air => {
                    // Dug-out pocket below the natural ground: dark backwall.
                    if j <= natural_ground {
                        quad(x, y, TILE_M, -0.5, [0.10, 0.09, 0.12, 1.0]);
                    }
                }
                TileKind::Water => {
                    quad(x, y, TILE_M, -0.3, [0.20, 0.42, 0.66, 1.0]);
                }
                _ => {
                    let mut rgb = kind.color();
                    // Per-tile shade variation so big fills don't look flat.
                    let n = (hash2(0x51ce, col, j as u32) % 100) as f32 / 100.0;
                    let shade = 0.92 + n * 0.10;
                    rgb = [rgb[0] * shade, rgb[1] * shade, rgb[2] * shade];
                    quad(x, y, TILE_M, 0.0, [rgb[0], rgb[1], rgb[2], 1.0]);
                }
            }
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
    tiles: &TileWorld,
    flora: &mut FloraHarvest,
    art: &ArtLibrary,
    body: CelestialBody,
    k: i64,
) {
    let suffix = body_art_suffix(body);
    let c = body_circumference_tiles(body);
    let wrapped_k = wrap_tile(k * CHUNK_TILES, c) as i32 / CHUNK_TILES as i32;
    let mut trees: Vec<TreeInstance> = Vec::new();

    for local in 0..CHUNK_TILES {
        let i = k * CHUNK_TILES + local;
        let col = wrap_tile(i, c);
        let ground_y = (ground_row(body, col as i64) + 1) as f32 * TILE_M;
        let x = i as f32 * TILE_M + 0.5;

        if let Some(decor) = surface_decor_at(body, col) {
            match decor {
                DecorKind::Tree => {
                    if !flora.is_removed(body, wrapped_k, 0, col) {
                        trees.push(TreeInstance {
                            cx: wrapped_k,
                            cz: 0,
                            id: col,
                            world: Vec3::new(x, ground_y + 1.4, 0.0),
                        });
                        // Drop-in pack art: one tree sprite. Fallback: trunk + canopy quads.
                        if let Some(image) =
                            art.get_first(&[&format!("decor/tree{suffix}"), "decor/tree"])
                        {
                            let mut sprite = Sprite::from_image(image);
                            sprite.custom_size = Some(Vec2::new(3.6, 4.6));
                            commands.spawn((
                                SurfaceDecor { k },
                                InteractTarget {
                                    kind: InteractTargetKind::Tree,
                                    id: col,
                                },
                                crate::render::particles::HarvestReactive,
                                sprite,
                                Transform::from_xyz(x, ground_y + 2.3, 0.4),
                            ));
                        } else {
                            let trunk_top = ground_y + 2.6;
                            commands
                                .spawn((
                                    SurfaceDecor { k },
                                    InteractTarget {
                                        kind: InteractTargetKind::Tree,
                                        id: col,
                                    },
                                    crate::render::particles::HarvestReactive,
                                    Sprite::from_color(
                                        Color::srgb(0.42, 0.30, 0.18),
                                        Vec2::new(0.6, 2.6),
                                    ),
                                    Transform::from_xyz(x, ground_y + 1.3, 0.4),
                                ))
                                .with_children(|p| {
                                    p.spawn((
                                        Sprite::from_color(
                                            Color::srgb(0.30, 0.55, 0.25),
                                            Vec2::new(3.2, 2.8),
                                        ),
                                        Transform::from_xyz(
                                            0.0,
                                            trunk_top - ground_y - 1.3 + 0.9,
                                            0.01,
                                        ),
                                    ));
                                });
                        }
                    }
                }
                DecorKind::Rock => {
                    let sprite = match art
                        .get_first(&[&format!("decor/rock{suffix}"), "decor/rock"])
                    {
                        Some(image) => {
                            let mut s = Sprite::from_image(image);
                            s.custom_size = Some(Vec2::new(1.5, 1.0));
                            s
                        }
                        None => Sprite::from_color(rock_color(body), Vec2::new(1.5, 1.0)),
                    };
                    commands.spawn((
                        SurfaceDecor { k },
                        InteractTarget {
                            kind: InteractTargetKind::Rock,
                            id: col,
                        },
                        sprite,
                        Transform::from_xyz(x, ground_y + 0.5, 0.4),
                    ));
                }
            }
        }

        if let Some(dep) = embedded_deposit_at(body, col) {
            let (color, size) = deposit_style(dep.kind);
            let key = match dep.kind {
                InteractTargetKind::GemRed => "decor/gem_red",
                InteractTargetKind::GemBlue => "decor/gem_blue",
                InteractTargetKind::OreGold => "decor/ore_gold",
                _ => "decor/ore_iron",
            };
            commands.spawn((
                SurfaceDecor { k },
                InteractTarget {
                    kind: dep.kind,
                    id: col,
                },
                art.sprite_or_color(key, color, size),
                Transform::from_xyz(
                    i as f32 * TILE_M + 0.5,
                    dep.row as f32 * TILE_M + 0.5,
                    -0.1,
                ),
            ));
        }
        let _ = tiles;
    }
    flora.set_chunk_trees(wrapped_k, 0, trees);
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
        InteractTargetKind::GemRed => (Color::srgb(0.85, 0.20, 0.25), Vec2::new(1.1, 1.1)),
        InteractTargetKind::GemBlue => (Color::srgb(0.25, 0.45, 0.90), Vec2::new(1.1, 1.1)),
        InteractTargetKind::OreGold => (Color::srgb(0.85, 0.70, 0.25), Vec2::new(1.3, 1.0)),
        _ => (Color::srgb(0.62, 0.48, 0.40), Vec2::new(1.3, 1.0)),
    }
}

// ---------------------------------------------------------------------------
// Persistence (B10: debounce + flush on pause)
// ---------------------------------------------------------------------------

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
    for (body, list) in save.edits {
        for (col, row, name) in list {
            tiles
                .edits
                .insert((body, col, row), tile_kind_from_name(&name));
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
    let mut save = TilesSave::default();
    for ((body, col, row), kind) in &tiles.edits {
        save.edits
            .entry(*body)
            .or_default()
            .push((*col, *row, tile_kind_name(*kind).to_string()));
    }
    if let Ok(json) = serde_json::to_string(&save) {
        let _ = std::fs::write(tiles_path(), json);
    }
}
