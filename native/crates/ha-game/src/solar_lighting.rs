//! Physical Sun as the only natural light source — direction from catalog position.
//! Player-built lights use `CraftedLight` (lanterns, torches, future spotlights).

use bevy::prelude::*;

use ha_shared::{
    body_center, gameplay_world_to_flight, orbital_dock_flight_pos, sample_solar, CelestialBody,
    SolarSample, WorldContext,
};

use crate::camera::GameCamera;
use crate::ghibli_atmosphere::PlayerLantern;
use crate::materials::light_halo_material;
use crate::planet_world::ActivePlanet;
use crate::quality::{enable_sun_shadows, max_light_halos};
use crate::rocket_flight::RocketFlight;
use crate::scene::{RegionId, RegionRuntime, SolarLight, WorldContextRuntime};
use crate::sim::{Player, SimState};
use crate::sky_clock::GameSkyClock;
use crate::surface_placement::{on_sphere_context, surface_body_for_context, world_pos_from_tangent};

/// Future: spotlights, lamps, emissive bases the player crafts. Not the Sun.
#[derive(Component)]
pub struct CraftedLight;

/// Additive billboard glow around a [`CraftedLight`] — max 8 active (Android fillrate).
#[derive(Component)]
pub struct LightHalo {
    pub source: Entity,
    pub radius: f32,
}

const HALO_ACTIVE_INTENSITY: f32 = 500.0;

/// FLAG: disabled — the halo quad billboards with `looking_at(cam, Y)`, which
/// points the plane's -Z at the camera while the visible face is +Y, so the
/// glow renders edge-on as a bugged floating plane. The PointLight itself is
/// untouched; re-enable only after re-doing the billboard from a clean base
/// (rotate +Y toward the camera, e.g. `Quat::from_rotation_arc`).
const ENABLE_LIGHT_HALOS: bool = false;

pub struct SolarLightingPlugin;

impl Plugin for SolarLightingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SolarLightingState>()
            .add_systems(PostStartup, attach_crafted_light_halos)
            .add_systems(Update, update_light_halos);
    }
}

#[derive(Resource, Clone, Debug)]
pub struct SolarLightingState {
    pub sample: SolarSample,
    pub observer_flight: Vec3,
    pub active_body: CelestialBody,
}

impl Default for SolarLightingState {
    fn default() -> Self {
        let observer = Vec3::from_array(orbital_dock_flight_pos());
        Self {
            sample: sample_solar(
                observer.to_array(),
                Some([0.0, 1.0, 0.0]),
                CelestialBody::Earth,
                0.44,
                false,
            ),
            observer_flight: observer,
            active_body: CelestialBody::Earth,
        }
    }
}

pub(crate) fn sync_solar_lighting(
    clock: Res<GameSkyClock>,
    sys_clock: Res<crate::solar_system_clock::SolarSystemClock>,
    region: Res<RegionRuntime>,
    world_ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    flight: Res<RocketFlight>,
    player: Query<&SimState, With<Player>>,
    mut ambient: ResMut<AmbientLight>,
    mut sun: Query<
        (&mut Transform, &mut DirectionalLight),
        (With<SolarLight>, Without<Player>),
    >,
    mut state: ResMut<SolarLightingState>,
) {
    let in_cave = matches!(region.active, RegionId::Cave(_));
    let body = active_body(&region, &world_ctx, &planet);

    let (observer_flight, surface_normal) = resolve_observer(&region, &world_ctx, &planet, &flight, &player);

    let sample = sample_solar(
        observer_flight.to_array(),
        surface_normal.map(|n| n.to_array()),
        body,
        clock.day_fraction,
        in_cave,
    );

    state.sample = sample;
    state.observer_flight = observer_flight;
    state.active_body = body;

    let lit = Vec3::from_array(sample.lit_to_sun);
    if let Ok((mut tf, mut light)) = sun.get_single_mut() {
        if lit.length_squared() > 1e-6 {
            // DirectionalLight emits along local -Z: forward must be observer→away-from-sun
            // so rays arrive FROM `lit` (rotating +Y toward the sun leaves -Z arbitrary).
            *tf = Transform::from_rotation(Quat::from_rotation_arc(
                Vec3::NEG_Z,
                (-lit).normalize(),
            ));
        }
        // Fade the directional sun out as it dips below the local horizon.
        // The shared `illuminance` keeps a 5% floor (soft ambient blend) whose
        // DIRECTION still points at the below-horizon sun, so it lit the
        // undersides of terrain/props — "light through the floor" at night.
        // Ambient starlight fill (below) covers night visibility instead.
        let horizon = (sample.disk_visibility / 0.10).clamp(0.0, 1.0);
        light.illuminance = sample.illuminance * horizon;
        light.color = Color::srgb(sample.color[0], sample.color[1], sample.color[2]);
        light.shadows_enabled = enable_sun_shadows(sample.disk_visibility, in_cave, flight.active);
    }

    // Night is never pitch black: MOONLIGHT (and earthshine on the Moon) give
    // a faint cool fill, like real life. Day = warm-neutral sun ambient.
    let night = (1.0 - (sample.disk_visibility / 0.14).clamp(0.0, 1.0)).clamp(0.0, 1.0);

    // Is the Moon above the observer's local horizon? If so it lights the
    // night more; a moonless sky is darker (but still faint starlight).
    let moon_up = if let Some(n) = surface_normal {
        let moon_c = Vec3::from_array(ha_shared::body_center_at(CelestialBody::Moon, sys_clock.t));
        let to_moon = (moon_c - observer_flight).normalize_or_zero();
        n.normalize_or_zero().dot(to_moon).max(0.0)
    } else {
        0.4
    };

    let (fill, tint) = if in_cave {
        (120.0, Vec3::new(0.62, 0.66, 0.82))
    } else if body == CelestialBody::Moon {
        // Airless: crisp terminator from the sun; the dark side gets faint
        // blue EARTHSHINE so you can still read the terrain past the line.
        let day = 180.0 + sample.disk_visibility * 240.0;
        let earthshine = 90.0 + 70.0 * night;
        let b = day * (1.0 - night) + earthshine * night;
        let tint = Vec3::new(0.60, 0.70, 1.0) * night + Vec3::splat(1.0) * (1.0 - night);
        (b, tint)
    } else {
        // Earth: warm sun ambient by day → cool moonlight by night, scaled by
        // how high the Moon sits.
        let day_fill = 190.0 + sample.disk_visibility * 210.0;
        let moon_fill = 70.0 + 150.0 * moon_up; // moonless ≈ 70, full moon ≈ 220
        let b = day_fill * (1.0 - night) + moon_fill * night;
        let moonlight = Vec3::new(0.70, 0.78, 1.0);
        let daylight = Vec3::new(1.0, 0.98, 0.94);
        let tint = daylight * (1.0 - night) + moonlight * night;
        (b, tint)
    };
    ambient.brightness = fill;
    ambient.color = Color::srgb(tint.x, tint.y, tint.z);
}

fn attach_crafted_light_halos(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    lights: Query<(Entity, &GlobalTransform), With<CraftedLight>>,
    existing: Query<Entity, With<LightHalo>>,
) {
    if !ENABLE_LIGHT_HALOS || !existing.is_empty() {
        return;
    }
    let halo_mesh = meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(1.0)));
    let halo_color = Color::srgb(1.0, 0.76, 0.42);
    let halo_mat = materials.add(light_halo_material(halo_color));

    for (source, tf) in lights.iter().take(max_light_halos()) {
        let radius = 1.05;
        commands.spawn((
            Mesh3d(halo_mesh.clone()),
            MeshMaterial3d(halo_mat.clone()),
            Transform::from_translation(tf.translation()).with_scale(Vec3::splat(radius)),
            LightHalo { source, radius },
            Visibility::Hidden,
            Name::new("LightHalo"),
        ));
    }
}

fn update_light_halos(
    camera: Query<&GlobalTransform, (With<GameCamera>, Without<LightHalo>)>,
    lights: Query<(Entity, &PointLight, &GlobalTransform), With<CraftedLight>>,
    player_lantern: Query<Entity, With<PlayerLantern>>,
    mut halos: Query<(&LightHalo, &mut Transform, &mut Visibility), Without<CraftedLight>>,
) {
    let Ok(cam) = camera.get_single() else {
        return;
    };
    let cam_pos = cam.translation();
    let player_lantern_entity = player_lantern.get_single().ok();

    for (halo, mut tf, mut vis) in &mut halos {
        let Some((_, light, light_tf)) = lights
            .iter()
            .find(|(entity, _, _)| *entity == halo.source)
        else {
            *vis = Visibility::Hidden;
            continue;
        };
        let active = light.intensity >= HALO_ACTIVE_INTENSITY;
        *vis = if active {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if !active {
            continue;
        }
        let pos = light_tf.translation();
        let scale = if player_lantern_entity == Some(halo.source) {
            2.4
        } else {
            halo.radius
        };
        *tf = Transform::from_translation(pos)
            .with_scale(Vec3::splat(scale))
            .looking_at(cam_pos, Vec3::Y);
    }
}

fn active_body(
    region: &RegionRuntime,
    world_ctx: &WorldContextRuntime,
    planet: &ActivePlanet,
) -> CelestialBody {
    match region.active {
        RegionId::Luna => CelestialBody::Moon,
        RegionId::Planet(b) => b,
        _ if on_sphere_context(world_ctx.active) => {
            surface_body_for_context(world_ctx.active, planet.body.unwrap_or(CelestialBody::Earth))
        }
        _ => CelestialBody::Earth,
    }
}

fn resolve_observer(
    region: &RegionRuntime,
    world_ctx: &WorldContextRuntime,
    planet: &ActivePlanet,
    flight: &RocketFlight,
    player: &Query<&SimState, With<Player>>,
) -> (Vec3, Option<Vec3>) {
    if flight.active && flight.phase != ha_shared::FlightPhase::Surface {
        let body_c = Vec3::from_array(body_center(flight.body));
        let up = (flight.pos - body_c).normalize_or_zero();
        return (flight.pos, Some(up));
    }

    let Ok(sim) = player.get_single() else {
        return (
            Vec3::from_array(orbital_dock_flight_pos()),
            Some(Vec3::Y),
        );
    };

    if matches!(region.active, RegionId::Space | RegionId::Transit) {
        return (
            Vec3::from_array(orbital_dock_flight_pos()),
            Some(Vec3::Y),
        );
    }

    if on_sphere_context(world_ctx.active) {
        let body = surface_body_for_context(world_ctx.active, planet.body.unwrap_or(CelestialBody::Earth));
        let world = world_pos_from_tangent(body, sim.body.x, sim.body.z, 1.6);
        let flight_pos = Vec3::from_array(gameplay_world_to_flight(body, world.to_array()));
        let normal = (world - Vec3::from_array(ha_shared::planet_gameplay_center(body))).normalize_or_zero();
        return (flight_pos, Some(normal));
    }

    let world = Vec3::new(sim.body.x, sim.body.y + 1.6, sim.body.z);
    (
        Vec3::from_array(gameplay_world_to_flight(CelestialBody::Earth, world.to_array())),
        Some(Vec3::Y),
    )
}
