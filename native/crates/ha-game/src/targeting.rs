//! 2D targeting — **qué hay debajo del dedo**.
//!
//! Fills the same `LookTarget` contract the HUD/interaction systems already
//! consume: entity targets (trees, rocks, ore, gems, machines, pad modules),
//! diggable tiles (`block_cell`) and placement slots (`place_cell`).
//!
//! Sin dedo apoyado NO hay objetivo (ver `AimCursor::targeting`): antes había una
//! mira que siempre estaba en algún lado, así que siempre había algo "apuntado" y
//! el HUD siempre tenía algo que decir. Ahora la pantalla se queda callada hasta
//! que tocás.

use bevy::prelude::*;
use ha_shared::topdown::TILE_M;
use ha_shared::{interact_reach_for, InteractTargetKind};

use crate::input::{AimCursor, EquippedItem, HoldInteractState, AIM_MAX_REACH};
use crate::scene::{RegionId, RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, SimState};
use crate::world2d::{active_surface_body, render_xy, ActivePlanet, TileWorld};

/// Reach for digging/placing tiles (m) — el mismo brazo que valida el toque.
const TILE_REACH: f32 = AIM_MAX_REACH;
/// Radio de picado alrededor del centro del sprite (m), para lo que no declara
/// tamaño. Los que sí lo declaran se pican por su RECTÁNGULO: tocar la copa de un
/// árbol tiene que agarrar el árbol, y la copa está dos metros por encima de su
/// centro.
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
        app.init_resource::<LookTarget>().add_systems(
            Update,
            update_look_target
                .after(crate::input::InputSet::Pointer)
                .before(crate::input::InputSet::Gesture),
        );
    }
}

/// Decor that a harvest physically removes from the world (and persists as
/// removed): trees, rocks and surface ore/gem deposits. Machines, containers,
/// launch modules etc. have their own lifecycle and are NOT removed this way.
pub fn is_removable_decor(kind: InteractTargetKind) -> bool {
    use InteractTargetKind::*;
    matches!(kind, Tree | Rock | OreIron | OreGold | GemBlue | GemRed)
}

pub(crate) fn target_allowed_in_region(kind: InteractTargetKind, region: RegionId) -> bool {
    match region {
        RegionId::Overworld { .. } => matches!(
            kind,
            InteractTargetKind::LaunchModule
                | InteractTargetKind::LaunchIgnition
                | InteractTargetKind::Tree
                | InteractTargetKind::Rock
                | InteractTargetKind::Container
                | InteractTargetKind::Chest
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
                | InteractTargetKind::Chest
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
    cursor: Res<AimCursor>,
    hold: Res<HoldInteractState>,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    equipped: Res<EquippedItem>,
    build_mode: Res<crate::input::BuildModeActive>,
    player: Query<(&Transform, &SimState), With<Player>>,
    targets: Query<(Entity, &GlobalTransform, &InteractTarget, Option<&Sprite>)>,
    mut current: ResMut<LookTarget>,
) {
    if current.locked {
        return;
    }
    // **Una acción en curso CONGELA su objetivo.** El dedo tiembla, y el árbol
    // que empezaste a talar tiene que seguir siendo ése hasta que termines o
    // sueltes (soltar lo cancela desde el gesto). Sin esto, un temblor de dos
    // píxeles a mitad del hachazo lo transfería al arbusto de al lado.
    if hold.active || hold.completed {
        return;
    }
    if !cursor.targeting() {
        *current = LookTarget::default();
        return;
    }
    let Ok((player_tf, sim)) = player.get_single() else {
        *current = LookTarget::default();
        return;
    };
    let body = active_surface_body(&ctx, &planet);
    let origin = player_tf.translation.truncate();

    // Free aim cursor: world ground `(x, z)`. `pick` is its screen position.
    let cursor_world = cursor.world;
    let pick = render_xy(cursor_world.x, cursor_world.y);

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

    // Lo tocado: se pica por el RECTÁNGULO del sprite cuando declara tamaño (así
    // tocar la copa agarra el árbol y no el pasto de atrás), y por un radio
    // alrededor del centro cuando no. Entre varios candidatos gana el que tiene
    // el centro más cerca del dedo.
    for (entity, gtf, target, sprite) in &targets {
        let center = gtf.translation().truncate();
        let size = sprite.and_then(|s| s.custom_size);
        let hit = match size {
            Some(s) => {
                let half = s.abs() * 0.5;
                (pick.x - center.x).abs() <= half.x && (pick.y - center.y).abs() <= half.y
            }
            None => center.distance(pick) <= POINT_PICK_RADIUS,
        };
        if !hit {
            continue;
        }
        // El alcance se mide al PIE del sprite, no a su centro: un árbol alto
        // tiene el centro un par de metros "arriba" en pantalla, y medir hasta
        // ahí lo dejaba fuera del brazo estando parado al lado del tronco.
        let foot = match size {
            Some(s) => Vec2::new(center.x, center.y - s.y.abs() * 0.5 + 0.3),
            None => center,
        };
        consider(entity, target.kind, foot, target.id, center.distance(pick));
    }

    if let Some((entity, kind, distance, hit, id)) = best {
        // Harvestable decor (trees, rocks, ore, gems) use a flat per-body removed
        // set at chunk key (0,0); `id` = packed global cell. Setting `flora_cell`
        // is what tells the HUD to REMOVE the prop from the world on harvest.
        let flora_cell = is_removable_decor(kind).then_some((0, 0, id));
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

    // Tile under the aim cursor (world ground coords → grid cell).
    let aim = pick;
    let _ = sim;
    if origin.distance(aim) <= TILE_REACH {
        let tx = (cursor_world.x / TILE_M).floor() as i64;
        let tz = (cursor_world.y / TILE_M).floor() as i64;
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
        } else if build_mode.active
            || ha_shared::is_block_item(&equipped.item_id)
            || crate::world2d::tile_for_block_item(&equipped.item_id).is_some()
        {
            // Empty cell → placement slot. Block items need a solid neighbour;
            // BUILD mode places anywhere (floors on open ground), any tool.
            let adjacent_solid = tiles.solid_at(body, tx - 1, tz)
                || tiles.solid_at(body, tx + 1, tz)
                || tiles.solid_at(body, tx, tz - 1)
                || tiles.solid_at(body, tx, tz + 1);
            let px = (sim.body.x / TILE_M).floor() as i64;
            let pz = (sim.body.z / TILE_M).floor() as i64;
            let overlaps_player = tx == px && tz == pz;
            if (build_mode.active || adjacent_solid) && !overlaps_player {
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
