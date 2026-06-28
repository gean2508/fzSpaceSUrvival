//! Earth home surface — streaming tangent patch (walkable sphere collision).

use bevy::prelude::*;

use crate::assets::AssetFactory;
use crate::deposit_spawn::spawn_sphere_deposits;
use crate::materials::ToonMaterial;
use crate::sim::WorldLayout;
use crate::surface_patch::{
    default_spawn_tangent, spawn_celestial_surface_patch,
};
use ha_shared::{CelestialBody, EARTH_GAMEPLAY_RADIUS_M};

pub struct EarthWorldPlugin;

impl Plugin for EarthWorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_earth_world);
    }
}

#[derive(Component, Clone, Copy)]
pub struct EarthDecor;

#[derive(Component)]
pub struct EarthSurface;

fn spawn_earth_world(
    mut commands: Commands,
    mut factory: ResMut<AssetFactory>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut toon_mats: ResMut<Assets<ToonMaterial>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
    world: Res<WorldLayout>,
) {
    let (cx, cz) = default_spawn_tangent(CelestialBody::Earth);
    spawn_celestial_surface_patch(
        &mut commands,
        &mut meshes,
        &mut std_mats,
        CelestialBody::Earth,
        cx,
        cz,
        EarthDecor,
        "EarthSurface",
        (EarthSurface,),
    );

    spawn_sphere_deposits(
        &mut commands,
        &mut factory,
        &mut meshes,
        &mut toon_mats,
        &mut std_mats,
        CelestialBody::Earth,
        &world.0.deposits,
        EarthDecor,
    );

    eprintln!(
        "ha-earth: SFS patch half={} R={} deposits={}",
        crate::surface_patch::SURFACE_PATCH_HALF,
        EARTH_GAMEPLAY_RADIUS_M,
        world.0.deposits.len()
    );
}
