//! Player-placed voxel blocks — Minecraft-style break/place wired into the
//! hold-interact pipeline. Sim rules live in `ha-shared/blocks.rs`; this plugin
//! owns targeting override, hold consumption, batched rendering and persistence.

use std::collections::HashMap;
use std::path::Path;

use bevy::prelude::*;
use ha_shared::{
    body_def, cell_of, is_block_item, palette_rgb, planet_gameplay_world_pos,
    planet_ground_y_at, raycast_terrain_place_cell, BlockCell, CelestialBody, InteractTargetKind,
    PaletteKey, PlacedBlockKind, PlacedBlockWorld, INTERACT_REACH,
};
use serde::{Deserialize, Serialize};

use crate::input::{CameraLookIntent, EquippedItem, HoldInteractState};
use crate::inventory_persist::{persist_dir, InventoryPersistState, PlayerInventory};
use crate::planet_world::ActivePlanet;
use crate::platform::is_app_paused;
use crate::rocket_flight::RocketFlight;
use crate::scene::{RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, SimState};
use crate::surface_patch::active_landable_body;
use crate::surface_placement::surface_normal_at_tangent;
use crate::targeting::LookTarget;

pub struct BlockWorldPlugin;

impl Plugin for BlockWorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlacedBlocks>()
            .add_systems(Startup, load_blocks_on_startup)
            .add_systems(
                Update,
                (
                    sync_block_target.after(crate::targeting::update_look_target),
                    apply_block_hold
                        .after(sync_block_target)
                        .before(crate::hud::update_interact_hud),
                    sync_block_visuals.after(apply_block_hold),
                    save_blocks_debounced,
                ),
            );
    }
}

/// All player-placed blocks, one voxel world per celestial body.
#[derive(Resource, Default)]
pub struct PlacedBlocks {
    per_body: HashMap<&'static str, PlacedBlockWorld>,
    visual_dirty: bool,
    save_dirty: bool,
    was_paused: bool,
    save_cooldown: f32,
}

impl PlacedBlocks {
    pub fn for_body(&self, body: CelestialBody) -> Option<&PlacedBlockWorld> {
        self.per_body.get(body_def(body).label)
    }

    pub fn world_mut(&mut self, body: CelestialBody) -> &mut PlacedBlockWorld {
        self.per_body.entry(body_def(body).label).or_default()
    }

    pub fn mark_changed(&mut self) {
        self.visual_dirty = true;
        self.save_dirty = true;
    }
}

#[derive(Component)]
struct BlockBatchVisual;

/// Marker so perf breakdown and biome visibility can identify block meshes.
#[derive(Component)]
pub struct PlacedBlockDecor;

// --- Targeting ---------------------------------------------------------------

/// Tangent-frame look ray — matches how the FP camera reads yaw/pitch locally.
fn tangent_look_ray(sim: &SimState, look: &CameraLookIntent) -> ([f32; 3], [f32; 3]) {
    let rot = Quat::from_rotation_y(look.yaw) * Quat::from_rotation_x(look.pitch);
    let dir = rot * Vec3::NEG_Z;
    (
        [sim.body.x, sim.body.y + 1.55, sim.body.z],
        [dir.x, dir.y, dir.z],
    )
}

pub(crate) fn sync_block_target(
    region: Res<RegionRuntime>,
    world_ctx: Res<WorldContextRuntime>,
    flight: Res<RocketFlight>,
    planet: Res<ActivePlanet>,
    blocks: Res<PlacedBlocks>,
    look: Res<CameraLookIntent>,
    equipped: Res<EquippedItem>,
    player: Query<&SimState, With<Player>>,
    mut target: ResMut<LookTarget>,
) {
    let Some(body) = active_landable_body(&region, &world_ctx, &flight, &planet) else {
        return;
    };
    let Ok(sim) = player.get_single() else {
        return;
    };
    let (origin, dir) = tangent_look_ray(sim, &look);

    // 1) Break target: closest placed block beats a farther world target.
    if let Some(bw) = blocks.for_body(body) {
        if let Some((cell, face_cell, t)) = bw.raycast(origin, dir, INTERACT_REACH) {
            let closer = target.kind.is_none() || t < target.distance;
            if closer {
                *target = LookTarget {
                    entity: None,
                    kind: Some(InteractTargetKind::Block),
                    distance: t,
                    hit_point: block_world_pos(body, cell),
                    block_cell: Some(cell),
                    place_cell: Some(face_cell),
                    ..default()
                };
                return;
            }
        }
    }

    // 2) Place slot: block item equipped, empty crosshair → terrain cell.
    if target.kind.is_none() && is_block_item(&equipped.item_id) {
        if let Some(cell) = raycast_terrain_place_cell(body, origin, dir, INTERACT_REACH) {
            if !cell_overlaps_player(cell, sim) {
                target.block_cell = None;
                target.place_cell = Some(cell);
            }
        }
    }
}

fn cell_overlaps_player(cell: BlockCell, sim: &SimState) -> bool {
    let feet = cell_of(sim.body.x, sim.body.y + 0.05, sim.body.z);
    let head = cell_of(sim.body.x, sim.body.y + 1.6, sim.body.z);
    cell == feet || cell == head
}

fn block_world_pos(body: CelestialBody, cell: BlockCell) -> Vec3 {
    let cx = cell.0 as f32 + 0.5;
    let cz = cell.2 as f32 + 0.5;
    let ground = planet_ground_y_at(body, cx, cz);
    let lift = cell.1 as f32 + 0.5 - ground;
    Vec3::from_array(planet_gameplay_world_pos(body, cx, cz, lift))
}

// --- Hold consumption ---------------------------------------------------------

pub(crate) fn apply_block_hold(
    region: Res<RegionRuntime>,
    world_ctx: Res<WorldContextRuntime>,
    flight: Res<RocketFlight>,
    planet: Res<ActivePlanet>,
    target: Res<LookTarget>,
    equipped: Res<EquippedItem>,
    mut hold: ResMut<HoldInteractState>,
    mut blocks: ResMut<PlacedBlocks>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
    player: Query<&SimState, With<Player>>,
) {
    if !hold.completed {
        return;
    }
    let Some(body) = active_landable_body(&region, &world_ctx, &flight, &planet) else {
        return;
    };

    // Break: hold locked on a placed block.
    if hold.locked_kind == Some(InteractTargetKind::Block) && !hold.placing_block {
        let Some(cell) = target.block_cell else {
            hold.completed = false;
            return;
        };
        if let Some(kind) = blocks.world_mut(body).remove(cell) {
            inv.add(kind.item_id(), 1);
            persist.mark_immediate();
            blocks.mark_changed();
            eprintln!("ha-block: broke {:?} at {:?} on {}", kind, cell, body_def(body).label);
        }
        hold.completed = false;
        return;
    }

    // Place: hold started with a block item over a valid slot.
    if hold.placing_block {
        hold.completed = false;
        hold.placing_block = false;
        let Some(cell) = target.place_cell else {
            return;
        };
        let Some(kind) = PlacedBlockKind::from_item(&equipped.item_id) else {
            return;
        };
        let Ok(sim) = player.get_single() else {
            return;
        };
        if cell_overlaps_player(cell, sim) {
            return;
        }
        if inv.qty(kind.item_id()) == 0 {
            return;
        }
        if blocks.world_mut(body).insert(cell, kind) {
            inv.remove(kind.item_id(), 1);
            persist.mark_immediate();
            blocks.mark_changed();
            eprintln!("ha-block: placed {:?} at {:?} on {}", kind, cell, body_def(body).label);
        }
    }
}

// --- Rendering -----------------------------------------------------------------

fn block_kind_palette(kind: PlacedBlockKind) -> PaletteKey {
    match kind {
        PlacedBlockKind::Dirt => PaletteKey::Earth,
        PlacedBlockKind::Wood => PaletteKey::Wood,
        PlacedBlockKind::Stone => PaletteKey::Stone,
    }
}

fn sync_block_visuals(
    region: Res<RegionRuntime>,
    world_ctx: Res<WorldContextRuntime>,
    flight: Res<RocketFlight>,
    planet: Res<ActivePlanet>,
    mut blocks: ResMut<PlacedBlocks>,
    mut last_body: Local<Option<CelestialBody>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
    existing: Query<Entity, With<BlockBatchVisual>>,
) {
    let body = active_landable_body(&region, &world_ctx, &flight, &planet);
    let body_changed = *last_body != body;
    if !blocks.visual_dirty && !body_changed {
        return;
    }
    *last_body = body;
    blocks.visual_dirty = false;

    for e in &existing {
        commands.entity(e).despawn_recursive();
    }
    let Some(body) = body else {
        return;
    };
    let Some(bw) = blocks.for_body(body) else {
        return;
    };
    if bw.is_empty() {
        return;
    }

    // One merged mesh per block kind → max 3 draw calls however big the build.
    let mut by_kind: HashMap<PlacedBlockKind, Vec<Transform>> = HashMap::new();
    for (&cell, &kind) in &bw.cells {
        let pos = block_world_pos(body, cell);
        let normal = surface_normal_at_tangent(body, cell.0 as f32 + 0.5, cell.2 as f32 + 0.5);
        let rot = Quat::from_rotation_arc(Vec3::Y, normal);
        by_kind
            .entry(kind)
            .or_default()
            .push(Transform::from_translation(pos).with_rotation(rot));
    }
    for (kind, transforms) in by_kind {
        let mesh = meshes.add(crate::render::mesh_batch::build_merged_cuboids(
            &transforms,
            1.0,
            1.0,
            1.0,
        ));
        let rgb = palette_rgb(block_kind_palette(kind));
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(std_mats.add(crate::materials::lit_surface_std(rgb))),
            Transform::IDENTITY,
            BlockBatchVisual,
            PlacedBlockDecor,
            Name::new(format!("PlacedBlocks_{kind:?}")),
        ));
    }
}

// --- Persistence -----------------------------------------------------------------

#[derive(Serialize, Deserialize, Default)]
struct BlocksSave {
    bodies: Vec<BodyBlocksSave>,
}

#[derive(Serialize, Deserialize)]
struct BodyBlocksSave {
    body: String,
    cells: Vec<(i32, i32, i32, PlacedBlockKind)>,
}

fn blocks_path() -> std::path::PathBuf {
    persist_dir().join("blocks.json")
}

fn load_blocks_on_startup(mut blocks: ResMut<PlacedBlocks>) {
    let Ok(text) = std::fs::read_to_string(blocks_path()) else {
        return;
    };
    let Ok(save) = serde_json::from_str::<BlocksSave>(&text) else {
        eprintln!("ha-block: blocks.json unreadable — starting empty");
        return;
    };
    let mut total = 0usize;
    for entry in save.bodies {
        let Some(&body) = CelestialBody::all()
            .iter()
            .find(|b| body_def(**b).label == entry.body)
        else {
            continue;
        };
        let world = blocks.world_mut(body);
        for (x, y, z, kind) in entry.cells {
            world.insert((x, y, z), kind);
            total += 1;
        }
    }
    blocks.visual_dirty = true;
    if total > 0 {
        eprintln!("ha-block: loaded {total} placed blocks");
    }
}

fn save_blocks_to_disk(blocks: &PlacedBlocks) {
    let save = BlocksSave {
        bodies: blocks
            .per_body
            .iter()
            .filter(|(_, w)| !w.is_empty())
            .map(|(label, w)| BodyBlocksSave {
                body: (*label).to_string(),
                cells: w
                    .cells
                    .iter()
                    .map(|(&(x, y, z), &k)| (x, y, z, k))
                    .collect(),
            })
            .collect(),
    };
    let path = blocks_path();
    if let Some(parent) = path.parent() {
        if parent != Path::new(".") {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    match serde_json::to_string(&save) {
        Ok(body) => {
            if let Err(err) = std::fs::write(&path, body) {
                eprintln!("ha-block: save failed ({}): {err}", path.display());
            }
        }
        Err(err) => eprintln!("ha-block: serialize failed: {err}"),
    }
}

fn save_blocks_debounced(time: Res<Time>, mut blocks: ResMut<PlacedBlocks>) {
    let paused = is_app_paused();
    if paused && !blocks.was_paused && blocks.save_dirty {
        save_blocks_to_disk(&blocks);
        blocks.save_dirty = false;
    }
    blocks.was_paused = paused;

    if !blocks.save_dirty {
        return;
    }
    blocks.save_cooldown += time.delta_secs();
    if blocks.save_cooldown >= 2.0 {
        blocks.save_cooldown = 0.0;
        save_blocks_to_disk(&blocks);
        blocks.save_dirty = false;
    }
}
