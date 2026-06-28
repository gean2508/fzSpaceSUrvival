//! Launch pad placement, module assembly, starship prefab visuals, and ignition targets.

use bevy::prelude::*;
use ha_shared::{
    assemble_module, create_launch_pad_state, ground_height_at, InteractActionKind,
    InteractTargetKind, LaunchPadState, LAUNCH_PAD, LAUNCH_PAD_MODULE_COUNT, LAUNCH_PAD_ZONE_RADIUS,
    LAUNCH_PAD_ZONE_X, LAUNCH_PAD_ZONE_Z, CelestialBody, WorldContext,
};

use crate::assets::{build_demo_ship_mesh, AssetFactory};
use crate::earth_world::EarthDecor;
use crate::flight_beacon::spawn_interact_beacon;
use crate::inventory_persist::{InventoryPersistState, PlayerInventory};
use crate::input::{EquippedItem, HoldInteractState};
use crate::world_travel::FlightTransition;
use crate::rocket_flight::RocketFlight;
use crate::machines::{LocalMachine, MachineEntity, MachineWorld};
use crate::materials::ToonMaterial;
use crate::scene::{RegionId, RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, SimState, WorldCollision, WorldLayout};
use crate::surface_placement::{
    sphere_surface_transform, sphere_world_pos, surface_normal_at_tangent,
    world_pos_from_tangent, PAD_SURFACE_LIFT,
};
use crate::targeting::{InteractTarget, LookTarget};

pub struct LaunchPadPlugin;

impl Plugin for LaunchPadPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LaunchPadWorld>()
            .add_systems(PostStartup, spawn_demo_launch_pad)
            .add_systems(Update, (try_assemble_on_hold, try_ignite_on_hold));
    }
}

#[derive(Resource, Default)]
pub struct LaunchPadWorld {
    pub pads: Vec<LaunchPadState>,
}

#[derive(Component)]
pub struct LaunchPadRoot {
    pub uid: u32,
}

#[derive(Component)]
pub struct LaunchModuleMarker {
    pub pad_uid: u32,
    pub module_index: u8,
}

#[derive(Component)]
pub struct LaunchIgnitionMarker {
    pub pad_uid: u32,
}

#[derive(Component)]
struct LaunchModuleVisual {
    pad_uid: u32,
    module_index: u8,
}

pub fn in_launch_pad_zone(x: f32, z: f32) -> bool {
    let dx = x - LAUNCH_PAD_ZONE_X;
    let dz = z - LAUNCH_PAD_ZONE_Z;
    (dx * dx + dz * dz).sqrt() <= LAUNCH_PAD_ZONE_RADIUS
}

pub fn zone_has_launch_pad(pads: &LaunchPadWorld) -> bool {
    pads.pads
        .iter()
        .any(|p| in_launch_pad_zone(p.x, p.z))
}

/// Pre-placed spaceport pad so the slice is visible without hammer placement grind.
fn spawn_demo_launch_pad(
    mut commands: Commands,
    mut world: ResMut<MachineWorld>,
    mut pads: ResMut<LaunchPadWorld>,
    mut factory: ResMut<AssetFactory>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut toon_mats: ResMut<Assets<ToonMaterial>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
    collision: Res<WorldCollision>,
    _layout: Res<WorldLayout>,
) {
    if zone_has_launch_pad(&pads) {
        return;
    }
    let x = LAUNCH_PAD_ZONE_X;
    let z = LAUNCH_PAD_ZONE_Z;
    let y = ground_height_at(x, z);
    let uid = world.next_uid;
    world.next_uid += 1;
    world.placed.push(LocalMachine {
        uid,
        machine_id: LAUNCH_PAD.id,
        x,
        y,
        z,
        output_accumulator: 0.0,
        stored_output: 0,
        producing_flash: 0.0,
    });
    spawn_launch_pad(
        &mut commands,
        &mut factory,
        &mut meshes,
        &mut toon_mats,
        &mut std_mats,
        uid,
        x,
        y,
        z,
        &mut pads,
    );
    preassemble_demo_ship(
        &mut commands,
        &mut meshes,
        &mut std_mats,
        uid,
        x,
        y,
        z,
        &mut pads,
    );
    eprintln!("ha-launch: demo pad at ({x}, {z}) — ship ready for launch");
}

fn module_slot_tangent(x: f32, z: f32) -> [((f32, f32, f32), u8); 3] {
    let lift = PAD_SURFACE_LIFT + 0.55;
    [
        ((x - 8.0, z + 8.0, lift), 0u8),
        ((x + 8.0, z + 8.0, lift), 1u8),
        ((x, z - 8.0, lift), 2u8),
    ]
}

/// Demo pad ships pre-assembled so the Tierra → orbit loop is always testable.
fn preassemble_demo_ship(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    std_mats: &mut Assets<StandardMaterial>,
    uid: u32,
    x: f32,
    _y: f32,
    z: f32,
    pads: &mut LaunchPadWorld,
) {
    if let Some(pad) = pads.pads.iter_mut().find(|p| p.uid == uid) {
        pad.assembled_modules = LAUNCH_PAD_MODULE_COUNT;
        pad.ready = true;
    }
    let ship_mat = std_mats.add(crate::materials::unlit_srgb([0.72, 0.74, 0.78]));
    let ship_tf = sphere_surface_transform(CelestialBody::Earth, x, z, PAD_SURFACE_LIFT, 0.0);
    commands.spawn((
        Mesh3d(meshes.add(build_demo_ship_mesh())),
        MeshMaterial3d(ship_mat),
        ship_tf,
        EarthDecor,
        LaunchModuleVisual {
            pad_uid: uid,
            module_index: 0,
        },
        Name::new(format!("ShipPrefab_{uid}")),
    ));
}

/// One merged mesh via AssetFactory + invisible ECS markers for hold-interact (zero extra draws).
pub fn spawn_launch_pad(
    commands: &mut Commands,
    factory: &mut AssetFactory,
    meshes: &mut Assets<Mesh>,
    toon_mats: &mut Assets<ToonMaterial>,
    std_mats: &mut Assets<StandardMaterial>,
    uid: u32,
    x: f32,
    y: f32,
    z: f32,
    pads: &mut LaunchPadWorld,
) {
    pads.pads.push(create_launch_pad_state(uid, x, y, z));

    let asset = factory.require(LAUNCH_PAD.asset_kind, meshes, toon_mats, std_mats);
    let pad_mat = std_mats.add(crate::materials::unlit_srgb([0.58, 0.61, 0.66]));
    let pad_tf = sphere_surface_transform(CelestialBody::Earth, x, z, PAD_SURFACE_LIFT, 0.0);

    commands.spawn((
        Mesh3d(asset.mesh.clone()),
        MeshMaterial3d(pad_mat),
        pad_tf,
        EarthDecor,
        MachineEntity { uid },
        LaunchPadRoot { uid },
        Name::new(format!("LaunchPad_{uid}")),
    ));

    let module_world = module_slot_tangent(x, z);
    for ((tx, tz, lift), i) in module_world {
        let pos = sphere_world_pos(CelestialBody::Earth, tx, tz, lift);
        commands.spawn((
            Transform::from_translation(pos),
            InteractTarget {
                kind: InteractTargetKind::LaunchModule,
                id: i as u32,
            },
            LaunchModuleMarker {
                pad_uid: uid,
                module_index: i,
            },
            Visibility::Hidden,
            Name::new(format!("LaunchModule_{uid}_{i}")),
        ));
    }

    let ignition_pos = sphere_world_pos(CelestialBody::Earth, x, z, PAD_SURFACE_LIFT + 2.8);
    commands.spawn((
        Transform::from_translation(ignition_pos),
        InteractTarget {
            kind: InteractTargetKind::LaunchIgnition,
            id: uid,
        },
        LaunchIgnitionMarker { pad_uid: uid },
        Name::new(format!("LaunchIgnition_{uid}")),
    ));
    spawn_interact_beacon(
        commands,
        meshes,
        std_mats,
        ignition_pos + Vec3::Y * 0.2,
        LinearRgba::new(1.0, 0.55, 0.12, 1.0),
        2.6,
        (),
    );
}

/// Starship prefab module mesh — hull / fuel tank / engine (visible after assembly).
pub fn spawn_starship_module_visual(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    std_mats: &mut Assets<StandardMaterial>,
    pad_uid: u32,
    module_index: u8,
    pos: Vec3,
) {
    let (mesh, rgb, y_off) = match module_index {
        0 => (
            Mesh::from(Cuboid::new(1.4, 2.8, 1.4)),
            [0.55, 0.58, 0.62],
            1.4,
        ),
        1 => (
            Mesh::from(Capsule3d::new(0.65, 1.6)),
            [0.92, 0.48, 0.12],
            1.1,
        ),
        _ => (
            Mesh::from(Cylinder::new(0.55, 1.5)),
            [0.88, 0.22, 0.14],
            0.9,
        ),
    };
    let mat = std_mats.add(crate::materials::unlit_srgb(rgb));
    let center = Vec3::from_array(ha_shared::planet_gameplay_center(CelestialBody::Earth));
    let up = (pos - center).normalize_or_zero();
    commands.spawn((
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(mat),
        Transform::from_translation(pos + up * y_off)
            .with_rotation(Quat::from_rotation_arc(Vec3::Y, up)),
        EarthDecor,
        LaunchModuleVisual {
            pad_uid,
            module_index,
        },
        Name::new(format!("ShipModule_{pad_uid}_{module_index}")),
    ));
}

pub(crate) fn try_assemble_on_hold(
    mut commands: Commands,
    mut hold: ResMut<HoldInteractState>,
    target: Res<LookTarget>,
    equipped: Res<EquippedItem>,
    region: Res<RegionRuntime>,
    transition: Res<FlightTransition>,
    mut pads: ResMut<LaunchPadWorld>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
    modules: Query<(&LaunchModuleMarker, &GlobalTransform)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
) {
    if !hold.completed || hold.placing_machine || transition.blocks_travel() {
        return;
    }
    if !matches!(region.active, RegionId::Overworld { .. }) {
        return;
    }
    let Some(kind) = target.kind else {
        return;
    };
    if kind != InteractTargetKind::LaunchModule {
        return;
    }
    let Some(entity) = target.entity else {
        return;
    };
    let Ok((marker, gtf)) = modules.get(entity) else {
        return;
    };
    let Some(rule) = ha_shared::resolve_interaction(Some(equipped.item_id.as_str()), kind) else {
        return;
    };
    if rule.kind != InteractActionKind::Assemble {
        return;
    }
    hold.completed = false;

    let pad_uid = marker.pad_uid;
    let module_index = marker.module_index;
    let pos = gtf.translation();

    let Some(pad) = pads.pads.iter_mut().find(|p| p.uid == pad_uid) else {
        return;
    };
    if assemble_module(pad, module_index, &mut inv) {
        persist.mark_immediate();
        spawn_starship_module_visual(
            &mut commands,
            &mut meshes,
            &mut std_mats,
            pad_uid,
            module_index,
            pos,
        );
        eprintln!(
            "ha-launch: starship module {} on pad {} ({}/3)",
            module_index, pad_uid, pad.assembled_modules
        );
    }
}

pub(crate) fn try_ignite_on_hold(
    mut hold: ResMut<HoldInteractState>,
    target: Res<LookTarget>,
    equipped: Res<EquippedItem>,
    region: Res<RegionRuntime>,
    transition: Res<crate::world_travel::FlightTransition>,
    pads: Res<LaunchPadWorld>,
    mut flight: ResMut<RocketFlight>,
    intent: Res<crate::input::MovementIntent>,
    mut moon: ResMut<crate::moon_world::MoonWorldState>,
    mut world_ctx: ResMut<WorldContextRuntime>,
    clock: Res<crate::solar_system_clock::SolarSystemClock>,
    ignition: Query<&LaunchIgnitionMarker>,
    player: Query<&SimState, With<Player>>,
) {
    if !hold.completed
        || hold.placing_machine
        || flight.active
        || transition.blocks_travel()
    {
        if hold.completed {
            hold.completed = false;
        }
        return;
    }
    if !matches!(region.active, RegionId::Overworld { .. }) {
        return;
    }
    let Some(kind) = target.kind else {
        return;
    };
    if kind != InteractTargetKind::LaunchIgnition {
        return;
    }
    let Some(entity) = target.entity else {
        return;
    };
    let Ok(marker) = ignition.get(entity) else {
        return;
    };
    let Some(rule) = ha_shared::resolve_interaction(Some(equipped.item_id.as_str()), kind) else {
        return;
    };
    if rule.kind != InteractActionKind::Launch {
        return;
    }
    let Some(pad) = pads.pads.iter().find(|p| p.uid == marker.pad_uid) else {
        return;
    };
    if !pad.ready {
        hold.completed = false;
        return;
    }
    hold.completed = false;
    let Ok(sim) = player.get_single() else {
        return;
    };
    flight.surface_flight = None;
    let tx = sim.body.x;
    let tz = sim.body.z;
    let pos = world_pos_from_tangent(CelestialBody::Earth, tx, tz, 1.8);
    let up = surface_normal_at_tangent(CelestialBody::Earth, tx, tz);

    // Dev: sprint + ignite = direct transit burn toward Moon (no station grind).
    if intent.sprint {
        let moon_c = ha_shared::body_center_at(CelestialBody::Moon, clock.t);
        let vel = (Vec3::from_array(moon_c) - pos).normalize_or_zero()
            * ha_shared::body_transit_burn(CelestialBody::Moon);
        flight.begin_coast_toward(pos, vel, CelestialBody::Moon);
        world_ctx.active = WorldContext::Space;
        moon.needs_spawn = true;
        moon.spawn_after_frames = 0;
        eprintln!("ha-launch: dev sprint ignition → Moon transit");
        return;
    }

    flight.begin_at(pos);
    flight.vel = up * 18.0;
    flight.pitch = 0.22;
    eprintln!("ha-launch: rocket ignition from pad");
}
