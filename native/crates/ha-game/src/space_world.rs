//! Orbital void — starfield + Earth/Moon system; tiny dock, not a turquoise hub square.

use bevy::prelude::*;

use crate::assets::WorldMaterial;
use crate::celestial_sphere::{
    space_starfield_radius, spawn_space_earth_sphere, spawn_space_moon_sphere, StarfieldSphere,
};
use crate::flight_beacon::spawn_interact_beacon;
use crate::space_travel::space_dock_origin;
use crate::materials::{starfield_material, starfield_void_std, PlanetSurfaceMaterial, ToonMaterial};
use crate::targeting::InteractTarget;
use ha_shared::{InteractTargetKind, SpaceStationState, SPACE_STATION_MODULE_COUNT};

#[derive(Resource, Default, Deref, DerefMut)]
pub struct SpaceStationRes(pub SpaceStationState);

pub struct SpaceWorldPlugin;

impl Plugin for SpaceWorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpaceWorldState>()
            .init_resource::<SpaceStationRes>()
            .add_systems(Startup, preassemble_dev_station)
            .add_systems(Update, sync_space_world);
    }
}

#[derive(Resource, Default)]
pub struct SpaceWorldState {
    pub spawned: bool,
    pub needs_spawn: bool,
}

fn preassemble_dev_station(mut station: ResMut<SpaceStationRes>) {
    station.assembled_modules = SPACE_STATION_MODULE_COUNT;
    station.ready = true;
}

pub fn despawn_space_entities(
    commands: &mut Commands,
    space: &mut SpaceWorldState,
    existing: &Query<Entity, With<SpaceDecor>>,
    slot_entities: &Query<Entity, With<SpaceStationModuleSlot>>,
    visual_entities: &Query<Entity, With<SpaceStationModuleVisual>>,
) {
    for e in existing
        .iter()
        .chain(slot_entities.iter())
        .chain(visual_entities.iter())
    {
        commands.entity(e).despawn_recursive();
    }
    space.spawned = false;
    space.needs_spawn = false;
}

#[derive(Component)]
pub struct SpaceDecor;

#[derive(Component)]
pub struct SpaceStationModuleSlot {
    pub module_index: u8,
}

#[derive(Component)]
pub struct SpaceStationModuleVisual {
    pub module_index: u8,
}

pub fn sync_space_world(
    mut commands: Commands,
    mut space: ResMut<SpaceWorldState>,
    station: Res<SpaceStationRes>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut toon_mats: ResMut<Assets<ToonMaterial>>,
    mut planet_mats: ResMut<Assets<PlanetSurfaceMaterial>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
    mut star_mats: ResMut<Assets<crate::materials::StarfieldMaterial>>,
    existing: Query<Entity, With<SpaceDecor>>,
    slot_entities: Query<Entity, With<SpaceStationModuleSlot>>,
    visual_entities: Query<Entity, With<SpaceStationModuleVisual>>,
) {
    if !space.needs_spawn || space.spawned {
        return;
    }
    space.needs_spawn = false;
    space.spawned = true;

    for e in existing
        .iter()
        .chain(slot_entities.iter())
        .chain(visual_entities.iter())
    {
        commands.entity(e).despawn_recursive();
    }

    // Void — procedural stars surround the Earth–Moon system (unlit Standard on all targets).
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(space_starfield_radius()))),
        MeshMaterial3d(std_mats.add(starfield_void_std())),
        Transform::from_scale(Vec3::new(-1.0, 1.0, -1.0)),
        SpaceDecor,
        StarfieldSphere,
        Name::new("StarfieldSphere"),
    ));

    let _ = (&mut toon_mats, &mut star_mats, &mut planet_mats);

    let dock_origin = space_dock_origin();

    spawn_space_earth_sphere(&mut commands, &mut meshes, &mut planet_mats, &mut std_mats);
    spawn_space_moon_sphere(&mut commands, &mut meshes, &mut planet_mats, &mut std_mats);
    crate::solar_system::spawn_solar_system_bodies(
        &mut commands,
        &mut meshes,
        &mut std_mats,
        dock_origin,
    );

    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(2.6, 0.14))),
        MeshMaterial3d(std_mats.add(crate::materials::unlit_srgb([0.22, 0.24, 0.28]))),
        Transform::from_translation(dock_origin),
        SpaceDecor,
        Name::new("SpaceDockRing"),
    ));

    let slot_layout = [
        (Vec3::new(-2.2, 0.12, 1.4), 0u8),
        (Vec3::new(2.2, 0.12, 1.4), 1u8),
        (Vec3::new(0.0, 0.12, -2.0), 2u8),
    ];
    for (offset, i) in slot_layout {
        let pos = dock_origin + offset;
        commands.spawn((
            Transform::from_translation(pos),
            InteractTarget {
                kind: InteractTargetKind::SpaceStationModule,
                id: i as u32,
            },
            SpaceStationModuleSlot { module_index: i },
            SpaceDecor,
            Name::new(format!("SpaceStationSlot_{i}")),
        ));
        spawn_interact_beacon(
            &mut commands,
            &mut meshes,
            &mut *std_mats,
            pos + Vec3::Y * 0.08,
            LinearRgba::new(0.2, 0.85, 1.0, 1.0),
            1.4,
            SpaceDecor,
        );
        if station.assembled_modules > i {
            spawn_station_module_visual(&mut commands, &mut meshes, &mut *std_mats, i, pos);
        }
    }

    let reentry_pos = dock_origin + Vec3::new(0.0, 0.12, 2.8);
    commands.spawn((
        Transform::from_translation(reentry_pos),
        InteractTarget {
            kind: InteractTargetKind::SpaceReentry,
            id: 0,
        },
        SpaceDecor,
        Name::new("SpaceReentry"),
    ));
    spawn_interact_beacon(
        &mut commands,
        &mut meshes,
        &mut *std_mats,
        reentry_pos + Vec3::Y * 0.08,
        LinearRgba::new(0.35, 0.65, 1.0, 1.0),
        2.0,
        SpaceDecor,
    );

    let luna_pos = dock_origin + Vec3::new(0.0, 0.12, -2.8);
    commands.spawn((
        Transform::from_translation(luna_pos),
        InteractTarget {
            kind: InteractTargetKind::SpaceTransitLuna,
            id: 0,
        },
        SpaceDecor,
        Name::new("SpaceTransitLuna"),
    ));
    spawn_interact_beacon(
        &mut commands,
        &mut meshes,
        &mut *std_mats,
        luna_pos + Vec3::Y * 0.08,
        LinearRgba::new(0.95, 0.35, 0.85, 1.0),
        2.0,
        SpaceDecor,
    );

    spawn_body_transit_beacons(&mut commands, &mut meshes, &mut std_mats, dock_origin);
}

fn spawn_body_transit_beacons(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    std_mats: &mut Assets<StandardMaterial>,
    dock: Vec3,
) {
    use ha_shared::{body_def, body_landable, CelestialBody};
    let ring_r = 4.2_f32;
    let bodies: Vec<CelestialBody> = CelestialBody::visitable_surfaces()
        .iter()
        .copied()
        .filter(|b| *b != CelestialBody::Earth)
        .collect();
    let n = bodies.len().max(1) as f32;
    for (i, body) in bodies.iter().enumerate() {
        let angle = (i as f32 / n) * std::f32::consts::TAU + 0.4;
        let offset = Vec3::new(angle.cos() * ring_r, 0.12, angle.sin() * ring_r);
        let pos = dock + offset;
        let def = body_def(*body);
        let rgb = def.color;
        commands.spawn((
            Transform::from_translation(pos),
            InteractTarget {
                kind: InteractTargetKind::SpaceTransitBody,
                id: *body as u8 as u32,
            },
            SpaceDecor,
            Name::new(format!("Transit_{}", def.label)),
        ));
        spawn_interact_beacon(
            commands,
            meshes,
            std_mats,
            pos + Vec3::Y * 0.08,
            LinearRgba::new(rgb[0], rgb[1], rgb[2], 1.0),
            1.6,
            SpaceDecor,
        );
    }
}

pub fn spawn_station_module_visual(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    std_mats: &mut Assets<StandardMaterial>,
    module_index: u8,
    pos: Vec3,
) {
    let (mesh, rgb, y) = match module_index {
        0 => (Mesh::from(Cuboid::new(1.8, 0.15, 1.2)), [0.15, 0.2, 0.55], 0.5),
        1 => (Mesh::from(Cuboid::new(1.6, 1.8, 1.6)), [0.72, 0.74, 0.78], 1.0),
        _ => (Mesh::from(Cylinder::new(0.35, 2.2)), [0.2, 0.85, 0.95], 1.2),
    };
    let mat = std_mats.add(crate::materials::unlit_srgb(rgb));
    commands.spawn((
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(mat),
        Transform::from_xyz(pos.x, pos.y + y, pos.z),
        SpaceStationModuleVisual { module_index },
        SpaceDecor,
        Name::new(format!("SpaceModuleVisual_{module_index}")),
    ));
}
