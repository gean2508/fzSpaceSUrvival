//! Walkable planet shells — Mars, Titan, etc. (Moon stays in `moon_world`).

use bevy::prelude::*;

use crate::assets::AssetFactory;
use crate::deposit_spawn::spawn_sphere_deposits;
use crate::flight_beacon::spawn_interact_beacon;
use crate::materials::ToonMaterial;
use crate::surface_patch::{default_spawn_tangent, spawn_celestial_surface_patch};
use crate::targeting::InteractTarget;
use ha_shared::{
    body_def, body_terrain_seed, body_walk_radius, generate_planet_deposits,
    planet_dir_from_tangent, surface_relief_at, CelestialBody, InteractTargetKind,
};

pub struct PlanetWorldPlugin;

impl Plugin for PlanetWorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlanetWorldState>()
            .init_resource::<ActivePlanet>()
            .add_systems(Update, sync_planet_world);
    }
}

#[derive(Resource, Default)]
pub struct ActivePlanet {
    pub body: Option<CelestialBody>,
}

#[derive(Resource, Default)]
pub struct PlanetWorldState {
    pub spawned: bool,
    pub needs_spawn: bool,
    pub body: Option<CelestialBody>,
}

#[derive(Component, Clone, Copy)]
pub struct PlanetDecor;

#[derive(Component)]
pub struct PlanetSurface;

pub fn despawn_planet_entities(
    commands: &mut Commands,
    planet: &mut PlanetWorldState,
    existing: &Query<Entity, With<PlanetDecor>>,
) {
    for e in existing.iter() {
        commands.entity(e).despawn_recursive();
    }
    planet.spawned = false;
    planet.needs_spawn = false;
    planet.body = None;
}

pub fn request_planet_spawn(state: &mut PlanetWorldState, body: CelestialBody) {
    if body == CelestialBody::Earth || body == CelestialBody::Moon || body == CelestialBody::Sun {
        return;
    }
    state.body = Some(body);
    state.needs_spawn = true;
    state.spawned = false;
}

fn sync_planet_world(
    mut commands: Commands,
    mut planet: ResMut<PlanetWorldState>,
    mut active: ResMut<ActivePlanet>,
    mut factory: ResMut<AssetFactory>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut toon_mats: ResMut<Assets<ToonMaterial>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
    existing: Query<Entity, With<PlanetDecor>>,
) {
    if !planet.needs_spawn || planet.spawned {
        return;
    }
    let Some(body) = planet.body else {
        return;
    };
    planet.needs_spawn = false;
    planet.spawned = true;
    active.body = Some(body);

    for e in &existing {
        commands.entity(e).despawn_recursive();
    }

    let def = body_def(body);
    let (spawn_tx, spawn_tz) = default_spawn_tangent(body);
    spawn_celestial_surface_patch(
        &mut commands,
        &mut meshes,
        &mut std_mats,
        body,
        spawn_tx,
        spawn_tz,
        PlanetDecor,
        &format!("PlanetSurface_{}", def.label),
        PlanetSurface,
    );

    let deposits = generate_planet_deposits(body, body_terrain_seed(body));
    spawn_sphere_deposits(
        &mut commands,
        &mut factory,
        &mut meshes,
        &mut toon_mats,
        &mut std_mats,
        body,
        &deposits,
        PlanetDecor,
        None,
    );

    let return_tx = 0.0_f32;
    let return_tz = -body_walk_radius(body) * 0.04;
    let return_dir = planet_dir_from_tangent(body, return_tx, return_tz);
    let return_disp = surface_relief_at(body, return_tx, return_tz);
    let r = body_walk_radius(body);
    let center = Vec3::from_array(ha_shared::planet_gameplay_center(body));
    let normal = Vec3::from_array(return_dir);
    let return_pos = center + normal * (r + return_disp) + normal * 0.15;

    commands.spawn((
        Transform::from_translation(return_pos),
        InteractTarget {
            kind: InteractTargetKind::PlanetReturn,
            id: body as u8 as u32,
        },
        PlanetDecor,
        Name::new(format!("PlanetReturn_{}", def.label)),
    ));
    spawn_interact_beacon(
        &mut commands,
        &mut meshes,
        &mut std_mats,
        return_pos + Vec3::Y * 0.1,
        LinearRgba::new(0.35, 0.95, 0.55, 1.0),
        2.0,
        PlanetDecor,
    );

    eprintln!(
        "ha-planet: {} patch spawned (R={}) deposits={}",
        def.label,
        r,
        deposits.len()
    );
}
