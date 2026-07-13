//! 2D targeting — pointer (touch/cursor) pick with proximity fallback.
//!
//! Fills the same `LookTarget` contract the HUD/interaction systems already
//! consume: entity targets (trees, rocks, ore, gems, machines, pad modules),
//! diggable tiles (`block_cell`) and placement slots (`place_cell`).

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use ha_shared::topdown::TILE_M;
use ha_shared::{interact_reach_for, InteractTargetKind};

use crate::camera::GameCamera;
use crate::input::{LookPointer, EquippedItem};
use crate::scene::{RegionId, RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, SimState};
use crate::world2d::{active_surface_body, render_xy, ActivePlanet, TileWorld};

/// Reach for digging/placing tiles (m).
const TILE_REACH: f32 = 5.6;
/// Pointer pick radius around sprites (m).
const POINT_PICK_RADIUS: f32 = 1.35;

#[derive(Component, Clone, Copy, Debug)]
pub struct InteractTarget {
    pub kind: InteractTargetKind,
    pub id: u32,
}

#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct LookTarget {
    pub entity: Option<Entity>,
    pub kind: Option<InteractTargetKind>,
    pub distance: f32,
    pub hit_point: Vec3,
    pub block_cell: Option<(i32, i32, i32)>,
    pub place_cell: Option<(i32, i32, i32)>,
    /// Tree bookkeeping (chunk cx, 0, wrapped col) — consumed by `FloraHarvest`.
    pub flora_cell: Option<(i32, i32, u32)>,
    /// Debug/autodrive: freeze the target (targeting skips its update).
    pub locked: bool,
}

pub struct TargetingPlugin;

impl Plugin for TargetingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LookTarget>()
            .add_systems(Update, update_look_target);
    }
}

fn target_allowed_in_region(kind: InteractTargetKind, region: RegionId) -> bool {
    match region {
        RegionId::Overworld { .. } => matches!(
            kind,
            InteractTargetKind::LaunchModule
                | InteractTargetKind::LaunchIgnition
                | InteractTargetKind::Tree
                | InteractTargetKind::Rock
                | InteractTargetKind::Container
                | InteractTargetKind::OreIron
                | InteractTargetKind::OreGold
                | InteractTargetKind::GemBlue
                | InteractTargetKind::GemRed
                | InteractTargetKind::Water
                | InteractTargetKind::Block
                | InteractTargetKind::Entity
        ),
        RegionId::Luna | RegionId::Planet(_) | RegionId::Cave(_) => matches!(
            kind,
            InteractTargetKind::LaunchModule
                | InteractTargetKind::LaunchIgnition
                | InteractTargetKind::Rock
                | InteractTargetKind::Container
                | InteractTargetKind::OreIron
                | InteractTargetKind::OreGold
                | InteractTargetKind::GemBlue
                | InteractTargetKind::GemRed
                | InteractTargetKind::Block
        ),
        RegionId::Space => matches!(
            kind,
            InteractTargetKind::SpaceReentry
                | InteractTargetKind::SpaceTransitLuna
                | InteractTargetKind::SpaceTransitBody
                | InteractTargetKind::SpaceStationModule
        ),
        _ => false,
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_look_target(
    pointer: Res<LookPointer>,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    equipped: Res<EquippedItem>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    player: Query<(&Transform, &SimState), With<Player>>,
    targets: Query<(Entity, &GlobalTransform, &InteractTarget)>,
    mut current: ResMut<LookTarget>,
) {
    if current.locked {
        return;
    }
    let Ok((player_tf, sim)) = player.get_single() else {
        *current = LookTarget::default();
        return;
    };
    let body = active_surface_body(&ctx, &planet);
    let origin = player_tf.translation.truncate();

    // Pointer world position: touch look-gesture first, desktop cursor second.
    let pointer_world: Option<Vec2> = {
        let screen = pointer.pos.or_else(|| {
            window.get_single().ok().and_then(|w| w.cursor_position())
        });
        screen.and_then(|p| {
            camera
                .get_single()
                .ok()
                .and_then(|(cam, tf)| cam.viewport_to_world_2d(tf, p).ok())
        })
    };

    let mut best: Option<(Entity, InteractTargetKind, f32, Vec2, u32)> = None;
    let mut consider = |entity: Entity, kind: InteractTargetKind, center: Vec2, id: u32, d: f32| {
        if !target_allowed_in_region(kind, region.active) {
            return;
        }
        let reach = interact_reach_for(kind).max(TILE_REACH);
        if origin.distance(center) > reach {
            return;
        }
        if best.map(|(_, _, bd, _, _)| d < bd).unwrap_or(true) {
            best = Some((entity, kind, d, center, id));
        }
    };

    if let Some(point) = pointer_world {
        // Pointer pick: closest sprite under the finger/cursor.
        for (entity, gtf, target) in &targets {
            let center = gtf.translation().truncate();
            let d = center.distance(point);
            if d <= POINT_PICK_RADIUS {
                consider(entity, target.kind, center, target.id, d);
            }
        }
    } else {
        // Proximity auto-target (top-down): nearest interactable within reach.
        for (entity, gtf, target) in &targets {
            let center = gtf.translation().truncate();
            consider(entity, target.kind, center, target.id, origin.distance(center));
        }
    }

    if let Some((entity, kind, distance, hit, id)) = best {
        // Trees/rocks use a flat per-body removed set at chunk key (0,0);
        // `id` = packed global cell.
        let flora_cell = (kind == InteractTargetKind::Tree).then_some((0, 0, id));
        *current = LookTarget {
            entity: Some(entity),
            kind: Some(kind),
            distance,
            hit_point: hit.extend(0.0),
            block_cell: None,
            place_cell: None,
            flora_cell,
            locked: false,
        };
        return;
    }

    // Tile targeting (dig / place) on surface regions only.
    if matches!(region.active, RegionId::Space | RegionId::Transit) {
        *current = LookTarget::default();
        return;
    }

    // Aim world point: pointer if present, else the cell the player faces.
    let aim = pointer_world.unwrap_or_else(|| {
        let fx = sim.body.rot.cos();
        let fz = sim.body.rot.sin();
        render_xy(sim.body.x + fx * 1.2, sim.body.z + fz * 1.2)
    });
    if origin.distance(aim) <= TILE_REACH {
        // Screen (x, -z) → grid (tx, tz).
        let tx = (aim.x / TILE_M).floor() as i64;
        let tz = (-aim.y / TILE_M).floor() as i64;
        if tiles.solid_at(body, tx, tz) {
            let t = tiles.tile_at(body, tx, tz);
            if t != ha_shared::topdown::Tile::Bedrock
                && target_allowed_in_region(InteractTargetKind::Block, region.active)
            {
                *current = LookTarget {
                    entity: None,
                    kind: Some(InteractTargetKind::Block),
                    distance: origin.distance(aim),
                    hit_point: aim.extend(0.0),
                    block_cell: Some((tx as i32, tz as i32, 0)),
                    place_cell: None,
                    flora_cell: None,
                    locked: false,
                };
                return;
            }
        } else if ha_shared::is_block_item(&equipped.item_id)
            || crate::world2d::tile_for_block_item(&equipped.item_id).is_some()
        {
            // Empty cell adjacent to something solid → placement slot.
            let adjacent_solid = tiles.solid_at(body, tx - 1, tz)
                || tiles.solid_at(body, tx + 1, tz)
                || tiles.solid_at(body, tx, tz - 1)
                || tiles.solid_at(body, tx, tz + 1);
            let px = (sim.body.x / TILE_M).floor() as i64;
            let pz = (sim.body.z / TILE_M).floor() as i64;
            let overlaps_player = tx == px && tz == pz;
            if adjacent_solid && !overlaps_player {
                *current = LookTarget {
                    entity: None,
                    kind: None,
                    distance: origin.distance(aim),
                    hit_point: aim.extend(0.0),
                    block_cell: None,
                    place_cell: Some((tx as i32, tz as i32, 0)),
                    flora_cell: None,
                    locked: false,
                };
                return;
            }
        }
    }

    *current = LookTarget::default();
}
