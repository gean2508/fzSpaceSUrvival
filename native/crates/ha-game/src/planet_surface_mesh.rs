//! Displaced surface mesh builders — tangent patches + legacy UV sphere.

use bevy::prelude::*;

use crate::lunar_surface::build_displaced_sphere;
use crate::materials::{block_std_material, luna_regolith_std, ToonMaterial};
use crate::planet_world::PlanetDecor;
use crate::surface_patch::spawn_celestial_surface_patch;
use ha_shared::{body_def, body_walk_radius, CelestialBody};

#[derive(Component)]
pub struct PlanetSurfaceMesh {
    pub body: CelestialBody,
}

/// Legacy full-sphere spawn — prefer `spawn_celestial_surface_patch`.
#[allow(dead_code)]
pub fn spawn_planet_sphere(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    toon_mats: &mut Assets<ToonMaterial>,
    std_mats: &mut Assets<StandardMaterial>,
    body: CelestialBody,
    rgb: [f32; 3],
) {
    let _ = (toon_mats, rgb);
    let (cx, cz) = crate::surface_patch::default_spawn_tangent(body);
    spawn_celestial_surface_patch(
        commands,
        meshes,
        std_mats,
        body,
        cx,
        cz,
        PlanetDecor,
        &format!("PlanetSurface_{body:?}"),
        (
            PlanetSurfaceMesh { body },
            crate::planet_world::PlanetSurface,
        ),
    );
}

#[allow(dead_code)]
pub fn build_full_displaced_sphere(
    body: CelestialBody,
    lat_segments: u32,
    lon_segments: u32,
) -> Mesh {
    build_displaced_sphere(body, body_walk_radius(body), lat_segments, lon_segments)
}

#[allow(dead_code)]
pub fn sphere_material(
    body: CelestialBody,
    std_mats: &mut Assets<StandardMaterial>,
    toon_mats: &mut Assets<ToonMaterial>,
) -> Handle<StandardMaterial> {
    let _ = toon_mats;
    std_mats.add(block_std_material(body_def(body).color))
}
