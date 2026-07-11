//! Player modular construction — snap grid, ghost preview, destroy built pieces only.

use bevy::prelude::*;
use ha_shared::{
    preview_placement, BuildPieceKind, BUILD_PIECE_HP, BUILD_WALL_HEIGHT,
};
use crate::assets::AssetFactory;
use crate::camera::look_forward_flat;
use crate::earth_world::EarthDecor;
use crate::input::{CameraLookIntent, EquippedItem, HoldInteractState};
use crate::inventory_persist::{InventoryPersistState, PlayerInventory};
use crate::materials::ToonMaterial;
use crate::sim::{Player, SimState};
use crate::surface_placement::sphere_surface_transform;
use crate::targeting::LookTarget;
use ha_shared::CelestialBody;

pub struct BuildingPlugin;

impl Plugin for BuildingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuildWorld>()
            .init_resource::<PendingBuildPiece>()
            .init_resource::<BuildModeActive>()
            .add_systems(Startup, spawn_build_ghost)
            .add_systems(Update, (update_build_ghost, try_place_build, try_destroy_build))
            .add_systems(FixedUpdate, try_place_build_on_hold);
    }
}

fn spawn_build_ghost(
    mut commands: Commands,
    mut factory: ResMut<AssetFactory>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut toon_mats: ResMut<Assets<ToonMaterial>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
) {
    let asset = factory.require(
        BuildPieceKind::FloorWood.asset_kind(),
        &mut meshes,
        &mut toon_mats,
        &mut std_mats,
    );
    let mut entity = commands.spawn((
        Mesh3d(asset.mesh.clone()),
        Transform::default(),
        Visibility::Hidden,
        BuildGhost,
        Name::new("BuildGhost"),
    ));
    asset.material.insert_on(&mut entity);
}

/// When true, hammer hold places structures instead of machines.
#[derive(Resource, Default)]
pub struct BuildModeActive {
    pub active: bool,
}

#[derive(Resource)]
pub struct PendingBuildPiece {
    pub piece: BuildPieceKind,
}

impl Default for PendingBuildPiece {
    fn default() -> Self {
        Self {
            piece: BuildPieceKind::FloorWood,
        }
    }
}

#[derive(Resource, Default)]
pub struct BuildWorld {
    pub next_uid: u32,
    pub pieces: Vec<ha_shared::PlacedBuildPiece>,
}

#[derive(Component)]
pub struct BuildPieceEntity {
    pub uid: u32,
}

#[derive(Component)]
pub struct BuildGhost;

fn piece_lift(piece: BuildPieceKind, level: i32) -> f32 {
    let base = match piece {
        BuildPieceKind::FloorWood => 0.08,
        BuildPieceKind::WallWood | BuildPieceKind::WallStone => 0.05,
        BuildPieceKind::RoofWood => 0.1,
        BuildPieceKind::StairsWood => 0.05,
    };
    base + level as f32 * BUILD_WALL_HEIGHT
}

fn piece_yaw(level_yaw_q: u8) -> f32 {
    level_yaw_q as f32 * std::f32::consts::FRAC_PI_2
}

fn update_build_ghost(
    build_mode: Res<BuildModeActive>,
    equipped: Res<EquippedItem>,
    look: Res<CameraLookIntent>,
    pending: Res<PendingBuildPiece>,
    build_world: Res<BuildWorld>,
    player: Query<&SimState, With<Player>>,
    mut ghosts: Query<&mut Transform, With<BuildGhost>>,
    mut visibility: Query<&mut Visibility, With<BuildGhost>>,
) {
    let show = build_mode.active && equipped.item_id == "hammer";
    for mut vis in &mut visibility {
        *vis = if show {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    if !show {
        return;
    }
    let Ok(sim) = player.get_single() else {
        return;
    };
    let fwd = look_forward_flat(&look);
    let hit_tx = sim.body.x + fwd.x * 3.5;
    let hit_tz = sim.body.z + fwd.y * 3.5;
    let preview = preview_placement(hit_tx, hit_tz, look.yaw, pending.piece, &build_world.pieces);
    let lift = piece_lift(pending.piece, preview.level);
    let tf = sphere_surface_transform(
        CelestialBody::Earth,
        preview.grid_x,
        preview.grid_z,
        lift,
        piece_yaw(preview.yaw_quarter),
    );
    for mut ghost_tf in &mut ghosts {
        *ghost_tf = tf;
    }
}

fn try_place_build_on_hold(
    mut hold: ResMut<HoldInteractState>,
    build_mode: Res<BuildModeActive>,
    equipped: Res<EquippedItem>,
    look: Res<CameraLookIntent>,
    pending: Res<PendingBuildPiece>,
    mut build_world: ResMut<BuildWorld>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
    mut commands: Commands,
    mut factory: ResMut<AssetFactory>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut toon_mats: ResMut<Assets<ToonMaterial>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
    player: Query<&SimState, With<Player>>,
    ghosts: Query<Entity, With<BuildGhost>>,
) {
    if !hold.completed || !hold.placing_build {
        return;
    }
    hold.completed = false;
    hold.placing_build = false;
    if !build_mode.active || equipped.item_id != "hammer" {
        return;
    }
    let Ok(sim) = player.get_single() else {
        return;
    };
    let fwd = look_forward_flat(&look);
    let hit_tx = sim.body.x + fwd.x * 3.5;
    let hit_tz = sim.body.z + fwd.y * 3.5;
    let preview = preview_placement(hit_tx, hit_tz, look.yaw, pending.piece, &build_world.pieces);
    if !preview.valid {
        return;
    }
    if !inv.can_afford_build(pending.piece) {
        return;
    }
    inv.consume_build_cost(pending.piece);
    persist.mark_immediate();

    let uid = build_world.next_uid;
    build_world.next_uid += 1;
    build_world.pieces.push(ha_shared::PlacedBuildPiece {
        uid,
        piece: pending.piece,
        grid_x: preview.grid_x,
        grid_z: preview.grid_z,
        level: preview.level,
        yaw_quarter: preview.yaw_quarter,
        hp: BUILD_PIECE_HP,
    });

    spawn_build_entity(
        &mut commands,
        &mut factory,
        &mut meshes,
        &mut toon_mats,
        &mut std_mats,
        uid,
        pending.piece,
        preview.grid_x,
        preview.grid_z,
        preview.level,
        preview.yaw_quarter,
    );
    let _ = ghosts;
}

fn spawn_build_entity(
    commands: &mut Commands,
    factory: &mut AssetFactory,
    meshes: &mut Assets<Mesh>,
    toon_mats: &mut Assets<ToonMaterial>,
    std_mats: &mut Assets<StandardMaterial>,
    uid: u32,
    piece: BuildPieceKind,
    gx: f32,
    gz: f32,
    level: i32,
    yaw_q: u8,
) {
    let asset = factory.require(piece.asset_kind(), meshes, toon_mats, std_mats);
    let lift = piece_lift(piece, level);
    let tf = sphere_surface_transform(CelestialBody::Earth, gx, gz, lift, piece_yaw(yaw_q));
    let mut entity = commands.spawn((
        Mesh3d(asset.mesh.clone()),
        tf,
        EarthDecor,
        BuildPieceEntity { uid },
        Name::new(format!("Build_{uid}_{}", piece.asset_kind())),
    ));
    asset.material.insert_on(&mut entity);
}

/// Tap ability while hammer equipped cycles build piece / toggles build mode.
fn try_place_build(
    mut build_mode: ResMut<BuildModeActive>,
    mut pending: ResMut<PendingBuildPiece>,
    equipped: Res<EquippedItem>,
    tap: Res<crate::input::AbilityTapPulse>,
) {
    if equipped.item_id != "hammer" {
        return;
    }
    if tap.slot != Some(1) {
        return;
    }
    if !build_mode.active {
        build_mode.active = true;
        return;
    }
    let kinds = BuildPieceKind::all();
    let idx = kinds
        .iter()
        .position(|k| *k == pending.piece)
        .unwrap_or(0);
    pending.piece = kinds[(idx + 1) % kinds.len()];
}

/// Destroy looked-at player build piece — never world flora/terrain.
fn try_destroy_build(
    mut commands: Commands,
    mut build_world: ResMut<BuildWorld>,
    equipped: Res<EquippedItem>,
    target: Res<LookTarget>,
    actions: Res<crate::input::TouchActions>,
    build_entities: Query<(Entity, &BuildPieceEntity)>,
) {
    if equipped.item_id != "hammer" || !actions.use_edge {
        return;
    }
    let Some(target_entity) = target.entity else {
        return;
    };
    let Ok((entity, piece)) = build_entities.get(target_entity) else {
        return;
    };
    build_world.pieces.retain(|p| p.uid != piece.uid);
    commands.entity(entity).despawn_recursive();
}
