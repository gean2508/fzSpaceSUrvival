//! Physical Sun as the only natural light source — direction from catalog position.
//! Player-built lights use `CraftedLight` (lanterns, torches, future spotlights).

use bevy::prelude::*;

use ha_shared::{
    body_center, gameplay_world_to_flight, orbital_dock_flight_pos, sample_solar, CelestialBody,
    SolarSample, WorldContext,
};

use crate::camera::GameCamera;
use crate::materials::light_halo_material;
use crate::planet_world::ActivePlanet;
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

const MAX_LIGHT_HALOS: usize = 8;
const HALO_ACTIVE_INTENSITY: f32 = 500.0;

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
            *tf = Transform::from_rotation(Quat::from_rotation_arc(Vec3::Y, lit));
        }
        light.illuminance = sample.illuminance;
        light.color = Color::srgb(sample.color[0], sample.color[1], sample.color[2]);
        light.shadows_enabled = sample.disk_visibility > 0.15 && !in_cave && !flight.active;
    }

    // Starlight fill only — direct sun carries most energy (panels will read `physical_to_sun`).
    let star_fill = if in_cave {
        120.0
    } else if sample.disk_visibility < 0.05 {
        280.0
    } else {
        180.0 + sample.disk_visibility * 220.0
    };
    ambient.brightness = star_fill;
}

fn attach_crafted_light_halos(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    lights: Query<(Entity, &GlobalTransform), With<CraftedLight>>,
    existing: Query<Entity, With<LightHalo>>,
) {
    if !existing.is_empty() {
        return;
    }
    let halo_mesh = meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(1.0)));
    let halo_color = Color::srgb(1.0, 0.76, 0.42);
    let halo_mat = materials.add(light_halo_material(halo_color));

    for (source, tf) in lights.iter().take(MAX_LIGHT_HALOS) {
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
    mut halos: Query<(&LightHalo, &mut Transform, &mut Visibility), Without<CraftedLight>>,
) {
    let Ok(cam) = camera.get_single() else {
        return;
    };
    let cam_pos = cam.translation();

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
        *tf = Transform::from_translation(pos)
            .with_scale(Vec3::splat(halo.radius))
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
