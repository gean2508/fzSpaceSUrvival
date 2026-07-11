//! Earth home surface — streaming tangent patch (walkable sphere collision).

use bevy::prelude::*;

use crate::materials::ToonMaterial;
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

#[derive(Component, Clone, Copy)]
pub struct StreamedChunkMark {
    pub cx: i32,
    pub cz: i32,
}

#[derive(Component)]
pub struct EarthSurface;

fn spawn_earth_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
    mut toon_mats: ResMut<Assets<ToonMaterial>>,
) {
    let _ = &mut toon_mats;
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

    eprintln!(
        "ha-earth: SFS patch half={} R={} — flora/deposits stream per chunk",
        crate::surface_patch::SURFACE_PATCH_HALF,
        EARTH_GAMEPLAY_RADIUS_M,
    );
}
