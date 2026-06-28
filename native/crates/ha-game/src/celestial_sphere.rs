//! Low-poly planet spheres — real astronomical radii (metres); hub uses visual compression.

use bevy::prelude::*;

use ha_shared::{
    EARTH_CENTER_Y, EARTH_FLIGHT_RADIUS_M, EARTH_RADIUS_M, HUB_EARTH_VISUAL_DISTANCE_M,
    HUB_EARTH_VISUAL_RADIUS_M, HUB_MOON_VISUAL_RADIUS_M, LUNA_SKY_EARTH_DISTANCE_M,
    LUNA_SKY_EARTH_RADIUS_M, MOON_GAMEPLAY_RADIUS_M, MOON_ORBIT_X_M, MOON_RADIUS_M,
    MOON_TO_EARTH_RADIUS_RATIO, MOON_FLIGHT_RADIUS_M, moon_center, SPACE_STARFIELD_RADIUS_M,
};
use crate::materials::{
    earth_globe_material, earth_planet_material, moon_globe_std, moon_planet_material,
    PlanetSurfaceMaterial,
};
use crate::moon_world::MoonDecor;
use crate::space_world::SpaceDecor;

/// Walkable moon shell — arcade gameplay radius.
pub const MOON_SPHERE_RADIUS: f32 = MOON_GAMEPLAY_RADIUS_M;

/// Earth globe in LEO void (compressed visual metres).
pub const EARTH_SPHERE_RADIUS: f32 = HUB_EARTH_VISUAL_RADIUS_M;

#[derive(Component)]
pub struct PlanetSphere;

/// SFS Earth globe in orbital void (B2 exception — unlit procedural).
#[derive(Component)]
pub struct EarthSphere;

/// SFS Moon — ray-sphere target in Space (targeting.rs).
#[derive(Component)]
pub struct MoonSphere;

/// Inverted starfield shell — Space + Luna.
#[derive(Component)]
pub struct StarfieldSphere;

#[derive(Component)]
pub struct TransitPlanet {
    pub body: TransitBody,
}

#[derive(Clone, Copy)]
pub enum TransitBody {
    Moon,
    Earth,
}

pub fn spawn_moon_sphere(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    planet_mats: &mut Assets<PlanetSurfaceMaterial>,
    std_mats: &mut Assets<StandardMaterial>,
) -> Entity {
    let center_y = -MOON_SPHERE_RADIUS;
    #[cfg(target_os = "android")]
    {
        commands
            .spawn((
                Mesh3d(meshes.add(Sphere::new(MOON_SPHERE_RADIUS))),
                MeshMaterial3d(std_mats.add(moon_globe_std())),
                Transform::from_xyz(0.0, center_y, 0.0),
                MoonDecor,
                PlanetSphere,
                Name::new("MoonSphere"),
            ))
            .id()
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = planet_mats;
        commands
            .spawn((
                Mesh3d(meshes.add(Sphere::new(MOON_SPHERE_RADIUS))),
                MeshMaterial3d(std_mats.add(moon_globe_std())),
                Transform::from_xyz(0.0, center_y, 0.0),
                MoonDecor,
                PlanetSphere,
                Name::new("MoonSphere"),
            ))
            .id()
    }
}

pub fn spawn_transit_planets(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    planet_mats: &mut Assets<PlanetSurfaceMaterial>,
    std_mats: &mut Assets<StandardMaterial>,
) {
    let moon_r = HUB_MOON_VISUAL_RADIUS_M * 0.65;
    let earth_r = HUB_EARTH_VISUAL_RADIUS_M * 0.55;
    let _ = planet_mats;
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(moon_r))),
        MeshMaterial3d(std_mats.add(moon_globe_std())),
        Transform::from_xyz(-18.0, 4.0, -40.0),
        TransitPlanet {
            body: TransitBody::Moon,
        },
        Visibility::Hidden,
        Name::new("TransitMoon"),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(earth_r))),
        MeshMaterial3d(std_mats.add(earth_globe_material())),
        Transform::from_xyz(6.0, -2.0, -80.0),
        TransitPlanet {
            body: TransitBody::Earth,
        },
        Visibility::Hidden,
        Name::new("TransitEarth"),
    ));
}

/// Earth below the orbital void — SFS procedural shell (B2 exception documented).
pub fn spawn_space_earth_sphere(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    planet_mats: &mut Assets<PlanetSurfaceMaterial>,
    std_mats: &mut Assets<StandardMaterial>,
) -> Entity {
    #[cfg(target_os = "android")]
    {
        commands
            .spawn((
                Mesh3d(meshes.add(Sphere::new(SFS_EARTH_RADIUS))),
                MeshMaterial3d(std_mats.add(earth_globe_material())),
                Transform::from_translation(SFS_EARTH_CENTER),
                SpaceDecor,
                PlanetSphere,
                EarthSphere,
                Name::new("EarthSphere"),
            ))
            .id()
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = planet_mats;
        commands
            .spawn((
                Mesh3d(meshes.add(Sphere::new(SFS_EARTH_RADIUS))),
                MeshMaterial3d(std_mats.add(earth_globe_material())),
                Transform::from_translation(SFS_EARTH_CENTER),
                SpaceDecor,
                PlanetSphere,
                EarthSphere,
                Name::new("EarthSphere"),
            ))
            .id()
    }
}

/// Moon in flight space — SFS hold-to-transit target.
pub fn spawn_space_moon_sphere(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    planet_mats: &mut Assets<PlanetSurfaceMaterial>,
    std_mats: &mut Assets<StandardMaterial>,
) -> Entity {
    let c = moon_sphere_center();
    #[cfg(target_os = "android")]
    {
        commands
            .spawn((
                Mesh3d(meshes.add(Sphere::new(moon_sphere_radius()))),
                MeshMaterial3d(std_mats.add(moon_globe_std())),
                Transform::from_translation(c),
                SpaceDecor,
                PlanetSphere,
                MoonSphere,
                Name::new("MoonSphere"),
            ))
            .id()
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = planet_mats;
        commands
            .spawn((
                Mesh3d(meshes.add(Sphere::new(moon_sphere_radius()))),
                MeshMaterial3d(std_mats.add(moon_globe_std())),
                Transform::from_translation(c),
                SpaceDecor,
                PlanetSphere,
                MoonSphere,
                Name::new("MoonSphere"),
            ))
            .id()
    }
}

/// SFS visual Earth in Space — arcade scale (metres).
pub const SFS_EARTH_RADIUS: f32 = 95.0;
pub const SFS_EARTH_CENTER: Vec3 = Vec3::new(0.0, -120.0, 0.0);

/// SFS Moon in Space — hold-to-transit target (arcade distance, visible from dock).
pub fn moon_sphere_center() -> Vec3 {
    Vec3::new(220.0, 55.0, -140.0)
}

pub fn moon_sphere_radius() -> f32 {
    28.0
}

/// Ray-sphere intersection distance along ray (None if miss).
pub fn ray_sphere_t(origin: Vec3, dir: Vec3, center: Vec3, radius: f32) -> Option<f32> {
    let oc = origin - center;
    let b = oc.dot(dir);
    let c = oc.dot(oc) - radius * radius;
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    let t = -b - disc.sqrt();
    if t > 0.0 {
        Some(t)
    } else {
        let t2 = -b + disc.sqrt();
        if t2 > 0.0 { Some(t2) } else { None }
    }
}

pub fn space_starfield_radius() -> f32 {
    900.0
}

/// Earth as seen from Luna — correct angular size at `LUNA_SKY_EARTH_DISTANCE_M`.
pub fn luna_sky_earth_radius() -> f32 {
    LUNA_SKY_EARTH_RADIUS_M
}

pub fn luna_sky_earth_position() -> Vec3 {
    Vec3::new(-48.0, LUNA_SKY_EARTH_DISTANCE_M * 0.55, -LUNA_SKY_EARTH_DISTANCE_M)
}

/// Real body radii for sim / docs (metres).
pub const EARTH_BODY_RADIUS_M: f32 = EARTH_RADIUS_M;
pub const MOON_BODY_RADIUS_M: f32 = MOON_RADIUS_M;
pub const MOON_EARTH_RADIUS_RATIO: f32 = MOON_TO_EARTH_RADIUS_RATIO;
