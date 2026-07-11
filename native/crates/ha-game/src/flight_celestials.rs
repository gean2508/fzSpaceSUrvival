//! Flight-scale solar system — sky moon from Earth, real-scale bodies during rocket cruise.

use bevy::prelude::*;

use ha_shared::{body_center_at, body_def, body_radius, body_soi,
    direction_to, gameplay_shell_center_at, CelestialBody, WorldContext};

use crate::earth_world::EarthDecor;
use crate::materials::{earth_globe_material, moon_globe_std, starfield_void_std, unlit_srgb};
use crate::floating_origin::FloatingOrigin;
use crate::rocket_flight::{flight_world_transform, RocketFlight};
use crate::view_target::ViewTarget;
use crate::scene::{RegionId, RegionRuntime, WorldContextRuntime};
use crate::sim::Player;
use crate::solar_system_clock::SolarSystemClock;
use crate::sky_clock::GameSkyClock;

/// Moon visible in the Earth sky (overworld + low ascent).
#[derive(Component)]
pub struct SkyMoon;

/// Real-scale body impostor during rocket flight (metres).
#[derive(Component)]
pub struct FlightCelestial {
    pub body: CelestialBody,
    pub mesh_unit_r: f32,
}

/// Inverted star shell centred on the pilot during vacuum cruise.
#[derive(Component)]
pub struct FlightStarfield;

/// Visible sun disc in the sky dome (Earth/Luna surfaces).
#[derive(Component)]
pub struct SkySun;

pub struct FlightCelestialsPlugin;

impl Plugin for FlightCelestialsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_flight_celestials)
            .add_systems(
                Update,
                (sync_flight_celestials, sync_sky_bodies, sync_sky_sun).chain(),
            );
    }
}

const EARTH_SKY_MOON_DISTANCE_M: f32 = 210.0;
const EARTH_SKY_MOON_RADIUS_M: f32 = 8.5;
const EARTH_SKY_SUN_DISTANCE_M: f32 = 380.0;
const EARTH_SKY_SUN_RADIUS_M: f32 = 22.0;
const FLIGHT_STARFIELD_RADIUS_M: f32 = 120_000.0;
const FLIGHT_MESH_UNIT_R: f32 = 1.0;

fn spawn_flight_celestials(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
) {
    let sun_mat = {
        let def = body_def(CelestialBody::Sun);
        let mut m = unlit_srgb(def.color);
        m.emissive = LinearRgba::new(def.color[0], def.color[1], def.color[2], 1.0) * 5.0;
        std_mats.add(m)
    };
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(EARTH_SKY_SUN_RADIUS_M))),
        MeshMaterial3d(sun_mat.clone()),
        Transform::IDENTITY,
        SkySun,
        EarthDecor,
        Name::new("SkySun"),
    ));

    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(EARTH_SKY_MOON_RADIUS_M))),
        MeshMaterial3d(std_mats.add(moon_globe_std())),
        Transform::IDENTITY,
        SkyMoon,
        EarthDecor,
        Name::new("SkyMoon"),
    ));

    for &body in CelestialBody::all() {
        let def = body_def(body);
        let mat = flight_body_material(body, &mut std_mats);
        commands.spawn((
            Mesh3d(meshes.add(Sphere::new(FLIGHT_MESH_UNIT_R))),
            MeshMaterial3d(mat),
            Transform::from_scale(Vec3::splat(body_radius(body))),
            Visibility::Hidden,
            FlightCelestial {
                body,
                mesh_unit_r: FLIGHT_MESH_UNIT_R,
            },
            Name::new(format!("Flight_{}", def.label)),
        ));
    }

    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(FLIGHT_STARFIELD_RADIUS_M))),
        MeshMaterial3d(std_mats.add(starfield_void_std())),
        Transform::from_scale(Vec3::new(-1.0, 1.0, -1.0)),
        Visibility::Hidden,
        FlightStarfield,
        Name::new("FlightStarfield"),
    ));
}

fn flight_body_material(
    body: CelestialBody,
    std_mats: &mut Assets<StandardMaterial>,
) -> Handle<StandardMaterial> {
    let def = body_def(body);
    if body == CelestialBody::Earth {
        return std_mats.add(earth_globe_material());
    }
    if body == CelestialBody::Moon {
        return std_mats.add(moon_globe_std());
    }
    if body == CelestialBody::Sun {
        let mut mat = unlit_srgb(def.color);
        mat.emissive = LinearRgba::new(def.color[0], def.color[1], def.color[2], 1.0) * 4.0;
        return std_mats.add(mat);
    }
    std_mats.add(unlit_srgb(def.color))
}

fn sync_flight_celestials(
    flight: Res<RocketFlight>,
    origin: Res<FloatingOrigin>,
    view: Res<ViewTarget>,
    clock: Res<SolarSystemClock>,
    player: Query<&GlobalTransform, With<Player>>,
    mut celestials: Query<(&FlightCelestial, &mut Transform, &mut Visibility)>,
    mut starfield: Query<
        (&mut Transform, &mut Visibility),
        (With<FlightStarfield>, Without<FlightCelestial>),
    >,
    mut was_active: Local<bool>,
) {
    let observer = observer_pos(&flight, &origin, &view, &player);
    let shift = origin.shift;
    let in_flight = flight.active;
    let t = clock.t;

    // UN SOLO UNIVERSO: son siempre los mismos cuerpos reales (esferas a
    // posición/escala por efemérides). Se ven al ACERCARTE volando (vacío) o por
    // cámara remota. Desde una superficie lo lejano NO se dibuja (culling por
    // distancia) — para navegar está el mapa 2D; al aproximarte, el cuerpo
    // aparece y crece hasta que aterrizás.
    let impostors_active = (in_flight && flight.in_vacuum()) || view.is_remote();

    if !impostors_active {
        if *was_active {
            for (_, _, mut vis) in celestials.iter_mut() {
                *vis = Visibility::Hidden;
            }
            for (_, mut vis) in starfield.iter_mut() {
                *vis = Visibility::Hidden;
            }
            *was_active = false;
        }
        return;
    }
    *was_active = true;

    for (fc, mut tf, mut vis) in celestials.iter_mut() {
        let center = Vec3::from_array(body_center_at(fc.body, t));
        tf.translation = center - shift;
        let r = body_radius(fc.body);
        tf.scale = Vec3::splat(r / fc.mesh_unit_r);

        // Culling por distancia — lo muy lejano no se rinde en la vista 3D.
        let dist = observer.distance(tf.translation);
        let max_dist = celestial_render_distance(fc.body, flight.body);
        let priority = fc.body == flight.body;
        let show = dist <= max_dist || priority;
        *vis = if show {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }

    let show_stars = impostors_active;
    for (mut tf, mut vis) in starfield.iter_mut() {
        tf.translation = observer;
        *vis = if show_stars {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

fn sync_sky_bodies(
    flight: Res<RocketFlight>,
    region: Res<RegionRuntime>,
    world_ctx: Res<WorldContextRuntime>,
    clock: Res<SolarSystemClock>,
    sky: Res<GameSkyClock>,
    player: Query<&GlobalTransform, With<Player>>,
    mut moon: Query<&mut Transform, (With<SkyMoon>, Without<SkySun>)>,
    mut sun: Query<&mut Transform, (With<SkySun>, Without<SkyMoon>)>,
    mut moon_vis: Query<&mut Visibility, With<SkyMoon>>,
) {
    let on_earth_surface = matches!(region.active, RegionId::Overworld { .. })
        && world_ctx.active == WorldContext::Overworld
        && !flight.active;
    let on_luna_surface =
        matches!(region.active, RegionId::Luna) && world_ctx.active == WorldContext::Luna;
    let low_ascent = flight.active
        && flight.body == CelestialBody::Earth
        && !flight.in_vacuum()
        && flight.altitude_m() < 900.0;

    let show_moon = on_earth_surface || low_ascent;
    for mut v in moon_vis.iter_mut() {
        *v = if show_moon {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }

    if !show_moon && !on_luna_surface && !on_earth_surface {
        return;
    }

    let Ok(player_tf) = player.get_single() else {
        return;
    };
    let eye = player_tf.translation();
    let t = clock.t;

    if show_moon {
        let earth_shell = Vec3::from_array(gameplay_shell_center_at(CelestialBody::Earth, t));
        let moon_c = Vec3::from_array(body_center_at(CelestialBody::Moon, t));
        let dir = (moon_c - earth_shell).normalize_or_zero();
        if let Ok(mut tf) = moon.get_single_mut() {
            tf.translation = eye + dir * EARTH_SKY_MOON_DISTANCE_M;
            tf.look_at(eye, Vec3::Y);
        }
    }

    if on_earth_surface || on_luna_surface || low_ascent {
        let shell_body = if on_luna_surface {
            CelestialBody::Moon
        } else {
            CelestialBody::Earth
        };
        let shell = Vec3::from_array(gameplay_shell_center_at(shell_body, t));
        let sun_c = Vec3::from_array(body_center_at(CelestialBody::Sun, t));
        let dir = direction_to(shell.to_array(), sun_c.to_array());
        let sun_dir = Vec3::from_array(dir);
        if let Ok(mut tf) = sun.get_single_mut() {
            tf.translation = eye + sun_dir * EARTH_SKY_SUN_DISTANCE_M;
            tf.look_at(eye, Vec3::Y);
        }
        let _ = sky.day_fraction;
    }
}

fn sync_sky_sun(
    region: Res<RegionRuntime>,
    world_ctx: Res<WorldContextRuntime>,
    sky: Res<GameSkyClock>,
    mut sun_vis: Query<&mut Visibility, With<SkySun>>,
) {
    let on_surface = matches!(
        region.active,
        RegionId::Overworld { .. } | RegionId::Luna
    ) && matches!(
        world_ctx.active,
        WorldContext::Overworld | WorldContext::Luna
    );
    // Hide sun mesh at night on Earth — sky shader still tints.
    let day = sky.day_fraction;
    let sun_up = day > 0.22 && day < 0.78;
    for mut v in sun_vis.iter_mut() {
        *v = if on_surface && sun_up {
            Visibility::Visible
        } else if on_surface {
            Visibility::Hidden
        } else {
            Visibility::Hidden
        };
    }
}

fn observer_pos(
    flight: &RocketFlight,
    origin: &FloatingOrigin,
    view: &ViewTarget,
    player: &Query<&GlobalTransform, With<Player>>,
) -> Vec3 {
    if view.is_remote() {
        return origin.to_render(view.observer_world);
    }
    if flight.active {
        flight.render_pos(origin)
    } else if let Ok(tf) = player.get_single() {
        tf.translation()
    } else {
        Vec3::ZERO
    }
}

fn celestial_render_distance(body: CelestialBody, flight_target: CelestialBody) -> f32 {
    match body {
        CelestialBody::Sun => 900_000.0,
        CelestialBody::Earth | CelestialBody::Moon => body_soi(body) * 14.0,
        _ if body == flight_target => 450_000.0,
        _ => 180_000.0,
    }
}
