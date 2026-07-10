//! Luna biome — displaced moon sphere, star shell, distant Earth in the sky.

use bevy::prelude::*;

use crate::deposit_spawn::spawn_sphere_deposits;
use crate::flight_beacon::spawn_interact_beacon;
use crate::materials::{
    earth_globe_material, luna_regolith_std, luna_star_point_std, luna_starfield_shell_std,
    starfield_material, ToonMaterial,
};
use crate::render::mesh_batch::build_merged_cuboids;
use crate::scene::{RegionId, RegionRuntime};
use crate::sim::Player;
use crate::surface_patch::{default_spawn_tangent, spawn_celestial_surface_patch};
use crate::targeting::InteractTarget;
use ha_shared::{
    body_terrain_seed, generate_luna_deposits, luna_crater_rim_markers, luna_dir_from_tangent,
    luna_displacement_at_dir, luna_gameplay_center, luna_gameplay_world_pos, surface_relief_at,
    CelestialBody,
    InteractTargetKind, LUNA_TANGENT_HALF, LUNA_SKY_EARTH_DISTANCE_M, MOON_GAMEPLAY_RADIUS_M,
};

/// Inverted star shell — must enclose the walkable shell (R=420) from the inside.
const LUNA_SKY_SHELL_RADIUS: f32 = MOON_GAMEPLAY_RADIUS_M + 460.0;

#[cfg(target_os = "android")]
const LUNA_STAR_POINT_COUNT: usize = 120;
#[cfg(not(target_os = "android"))]
const LUNA_STAR_POINT_COUNT: usize = 200;

const LUNA_ROCK_COUNT: usize = 40;
const LUNA_CRATER_RIM_COUNT: usize = 15;
const LUNA_SPAWN_CLEAR_RADIUS: f32 = 5.0;

pub struct MoonWorldPlugin;

impl Plugin for MoonWorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MoonWorldState>()
            .add_systems(
                Update,
                (sync_moon_world, billboard_luna_earth, follow_luna_sky_shell).chain(),
            );
    }
}

#[derive(Resource, Default)]
pub struct MoonWorldState {
    pub spawned: bool,
    pub needs_spawn: bool,
    pub spawn_after_frames: u8,
}

pub fn despawn_moon_entities(
    commands: &mut Commands,
    moon: &mut MoonWorldState,
    existing: &Query<Entity, With<MoonDecor>>,
) {
    for e in existing.iter() {
        commands.entity(e).despawn_recursive();
    }
    moon.spawned = false;
    moon.needs_spawn = false;
    moon.spawn_after_frames = 0;
}

#[derive(Component, Clone, Copy)]
pub struct MoonDecor;

/// Walkable displaced regolith mesh — stays visible on Luna (not hidden by chunk manager).
#[derive(Component)]
pub struct LunarSurface;

/// Distant Earth — sky decoration only (billboard toward player).
#[derive(Component)]
pub struct EarthDecor;

#[derive(Component)]
struct LunaSkyShell;

fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

fn luna_surface_transform(tx: f32, tz: f32, lift: f32, yaw: f32, scale: Vec3) -> Transform {
    let pos = Vec3::from_array(luna_gameplay_world_pos(tx, tz, lift));
    let dir = luna_dir_from_tangent(tx, tz);
    let normal = Vec3::new(dir[0], dir[1], dir[2]).normalize_or_zero();
    let rot = Quat::from_rotation_arc(Vec3::Y, normal) * Quat::from_rotation_y(yaw);
    Transform::from_translation(pos)
        .with_rotation(rot)
        .with_scale(scale)
}

fn spawn_luna_star_points(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    std_mats: &mut Assets<StandardMaterial>,
    moon_center: Vec3,
) {
    let r = LUNA_SKY_SHELL_RADIUS - 1.5;
    let mut transforms = Vec::with_capacity(LUNA_STAR_POINT_COUNT);
    for i in 0..LUNA_STAR_POINT_COUNT {
        let h = hash_u32(7 ^ i as u32);
        let h2 = hash_u32(h);
        let h3 = hash_u32(h2);
        let theta = (h as f32 / u32::MAX as f32) * std::f32::consts::TAU;
        let phi = (h2 as f32 / u32::MAX as f32) * std::f32::consts::PI;
        let dir = Vec3::new(
            phi.sin() * theta.cos(),
            phi.cos(),
            phi.sin() * theta.sin(),
        )
        .normalize_or_zero();
        let brightness = 0.5 + (h3 & 0xff) as f32 / 255.0 * 0.5;
        let s = 0.05 + brightness * 0.07;
        // Baked RELATIVE to the entity origin — `follow_luna_sky_shell`
        // recenters the whole shell on the player each frame so the starfield
        // is a skybox you can never cross (§1: airless bodies have no finite
        // crossable sky geometry), not a fixed sphere anchored at moon_center.
        transforms.push(Transform::from_translation(dir * r).with_scale(Vec3::splat(s)));
    }
    let mesh = meshes.add(build_merged_cuboids(&transforms, 1.0, 1.0, 1.0));
    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(std_mats.add(luna_star_point_std())),
        Transform::from_translation(moon_center),
        MoonDecor,
        LunaSkyShell,
        Name::new("LunaStarPoints"),
    ));
}

fn spawn_luna_props(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    std_mats: &mut Assets<StandardMaterial>,
) {
    let mut rock_transforms = Vec::with_capacity(LUNA_ROCK_COUNT);
    for i in 0..LUNA_ROCK_COUNT as u32 {
        let h = hash_u32(42 ^ i.wrapping_mul(0x9e37_79b9));
        let h2 = hash_u32(h);
        let tx = ((h as f32 / u32::MAX as f32) * 2.0 - 1.0) * LUNA_TANGENT_HALF * 0.82;
        let tz = ((h2 as f32 / u32::MAX as f32) * 2.0 - 1.0) * LUNA_TANGENT_HALF * 0.82;
        if tx * tx + tz * tz < LUNA_SPAWN_CLEAR_RADIUS * LUNA_SPAWN_CLEAR_RADIUS {
            continue;
        }
        let scale = 0.3 + (h & 0xff) as f32 / 255.0 * 2.2;
        let yaw = (h2 as f32 / u32::MAX as f32) * std::f32::consts::TAU;
        rock_transforms.push(luna_surface_transform(
            tx,
            tz,
            scale * 0.35,
            yaw,
            Vec3::new(scale, scale * 0.85, scale * 0.9),
        ));
    }

    let mut rim_transforms = Vec::with_capacity(LUNA_CRATER_RIM_COUNT);
    for (dir, rim_r) in luna_crater_rim_markers(LUNA_CRATER_RIM_COUNT) {
        let rim_scale = rim_r.clamp(3.0, 8.0);
        let (tx, tz) = ha_shared::luna_tangent_from_dir(dir);
        rim_transforms.push(luna_surface_transform(
            tx,
            tz,
            0.12,
            0.0,
            Vec3::new(rim_scale, 0.18, rim_scale),
        ));
    }

    let regolith = std_mats.add(luna_regolith_std());
    if !rock_transforms.is_empty() {
        let mesh = meshes.add(build_merged_cuboids(&rock_transforms, 1.0, 1.0, 1.0));
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(regolith.clone()),
            MoonDecor,
            Name::new("LunaRocksBatch"),
        ));
    }
    if !rim_transforms.is_empty() {
        let mesh = meshes.add(build_merged_cuboids(&rim_transforms, 1.0, 1.0, 1.0));
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(regolith),
            MoonDecor,
            Name::new("LunaCraterRimsBatch"),
        ));
    }
}

pub fn sync_moon_world(
    mut commands: Commands,
    mut moon: ResMut<MoonWorldState>,
    mut factory: ResMut<crate::assets::AssetFactory>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut toon_mats: ResMut<Assets<ToonMaterial>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
    mut star_mats: ResMut<Assets<crate::materials::StarfieldMaterial>>,
    existing: Query<Entity, With<MoonDecor>>,
) {
    if moon.spawn_after_frames > 0 {
        moon.spawn_after_frames -= 1;
        if moon.spawn_after_frames == 0 {
            moon.needs_spawn = true;
        }
        return;
    }
    if !moon.needs_spawn || moon.spawned {
        return;
    }
    moon.needs_spawn = false;
    moon.spawned = true;

    for e in &existing {
        commands.entity(e).despawn_recursive();
    }

    let (luna_tx, luna_tz) = default_spawn_tangent(CelestialBody::Moon);
    spawn_celestial_surface_patch(
        &mut commands,
        &mut meshes,
        &mut std_mats,
        CelestialBody::Moon,
        luna_tx,
        luna_tz,
        MoonDecor,
        "LunarSurface",
        LunarSurface,
    );
    spawn_luna_props(&mut commands, &mut meshes, &mut std_mats);

    let luna_deposits = generate_luna_deposits(body_terrain_seed(CelestialBody::Moon));
    spawn_sphere_deposits(
        &mut commands,
        &mut factory,
        &mut meshes,
        &mut toon_mats,
        &mut std_mats,
        CelestialBody::Moon,
        &luna_deposits,
        MoonDecor,
        None,
    );

    let moon_center = Vec3::from_array(luna_gameplay_center());
    let invert = Vec3::new(-1.0, 1.0, -1.0);

    #[cfg(target_os = "android")]
    {
        commands.spawn((
            Mesh3d(meshes.add(Sphere::new(LUNA_SKY_SHELL_RADIUS))),
            MeshMaterial3d(std_mats.add(luna_starfield_shell_std())),
            Transform::from_translation(moon_center).with_scale(invert),
            MoonDecor,
            LunaSkyShell,
            Name::new("LunaSkyShell"),
        ));
    }
    #[cfg(not(target_os = "android"))]
    {
        let mut star = starfield_material();
        star.settings.density = 1.05;
        star.settings.night = 1.0;
        commands.spawn((
            Mesh3d(meshes.add(Sphere::new(LUNA_SKY_SHELL_RADIUS))),
            MeshMaterial3d(star_mats.add(star)),
            Transform::from_translation(moon_center).with_scale(invert),
            MoonDecor,
            LunaSkyShell,
            Name::new("LunaSkyShell"),
        ));
    }

    spawn_luna_star_points(&mut commands, &mut meshes, &mut std_mats, moon_center);

    // Earth in the sky — fixed direction from moon centre, far past the regolith shell.
    let earth_dir = Vec3::new(-0.32, 0.54, -0.78).normalize_or_zero();
    let earth_dist = MOON_GAMEPLAY_RADIUS_M + LUNA_SKY_EARTH_DISTANCE_M;
    let earth_pos = moon_center + earth_dir * earth_dist;
    let earth_r = (earth_dist * 0.038).clamp(18.0, 32.0);
    let mut earth_mat = earth_globe_material();
    earth_mat.emissive = LinearRgba::new(0.45, 0.62, 0.95, 1.0);
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(earth_r))),
        MeshMaterial3d(std_mats.add(earth_mat)),
        Transform::from_translation(earth_pos),
        EarthDecor,
        MoonDecor,
        Name::new("EarthSkyGlobe"),
    ));

    let return_tangent_x = 0.0_f32;
    let return_tangent_z = -18.0_f32;
    let return_dir = ha_shared::luna_dir_from_tangent(return_tangent_x, return_tangent_z);
    let return_disp = surface_relief_at(CelestialBody::Moon, return_tangent_x, return_tangent_z);
    let r = MOON_GAMEPLAY_RADIUS_M;
    let normal = Vec3::new(return_dir[0], return_dir[1], return_dir[2]);
    let return_pos = moon_center + normal * (r + return_disp) + normal * 0.15;
    commands.spawn((
        Transform::from_translation(return_pos),
        InteractTarget {
            kind: InteractTargetKind::LunarReturn,
            id: 0,
        },
        MoonDecor,
        Name::new("LunarReturn"),
    ));
    spawn_interact_beacon(
        &mut commands,
        &mut meshes,
        &mut std_mats,
        return_pos + normal * 0.15,
        LinearRgba::new(0.35, 0.95, 0.55, 1.0),
        2.0,
        MoonDecor,
    );

    eprintln!(
        "ha-luna: patch R={} sky_shell R={} props={} rims={} deposits={}",
        MOON_GAMEPLAY_RADIUS_M,
        LUNA_SKY_SHELL_RADIUS,
        LUNA_ROCK_COUNT,
        LUNA_CRATER_RIM_COUNT,
        luna_deposits.len()
    );
}

/// Keep Earth sky globe facing the player (sky disc, not a world marble).
fn billboard_luna_earth(
    region: Res<RegionRuntime>,
    player: Query<&Transform, (With<Player>, Without<EarthDecor>)>,
    mut earth: Query<&mut Transform, (With<EarthDecor>, Without<Player>)>,
) {
    if !matches!(region.active, RegionId::Luna) {
        return;
    }
    let Ok(player_tf) = player.get_single() else {
        return;
    };
    let eye = player_tf.translation + Vec3::Y * 1.6;
    for mut tf in &mut earth {
        tf.look_at(eye, Vec3::Y);
    }
}

/// Recenter the star shell + star points on the player every frame so the Luna
/// "sky" is an uncrossable skybox, not a finite sphere anchored at moon_center.
/// Before this, climbing >460 m let you exit the shell and see it from outside
/// as a black marble with the clear color behind — the §1 "techo de cielo" bug.
/// Only translation is touched (the shell keeps its inverted scale).
fn follow_luna_sky_shell(
    region: Res<RegionRuntime>,
    player: Query<&Transform, (With<Player>, Without<LunaSkyShell>)>,
    mut shells: Query<&mut Transform, (With<LunaSkyShell>, Without<Player>)>,
) {
    if !matches!(region.active, RegionId::Luna) {
        return;
    }
    let Ok(player_tf) = player.get_single() else {
        return;
    };
    for mut tf in &mut shells {
        tf.translation = player_tf.translation;
    }
}
