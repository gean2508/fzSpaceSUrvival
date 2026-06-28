//! Continuous rocket flight — solar sandbox, parachute, RC retro motors.

use bevy::prelude::*;
use ha_shared::{
    altitude_above_body, altitude_above_body_at, altitude_above_luna_gameplay,
    altitude_above_planet_gameplay, atmosphere_drag, body_atmo_top, body_landable, body_radius,
    body_walk_radius, dominant_body, dominant_body_at, earth_center_at, effective_safe_landing_v,
    gravity_accel_at_time, gravity_accel_luna_gameplay, gravity_accel_planet_gameplay, in_vacuum,
    parachute_drag, planet_dir_from_tangent, planet_displacement_at_dir, planet_gameplay_center,
    planet_gameplay_world_pos, planet_ground_y_at, planet_tangent_from_dir,
    remap_luna_gameplay_to_orbital, remap_planet_gameplay_to_orbital, should_auto_deploy_parachute,
    sun_proximity_damage, surface_normal, surface_normal_at, surface_world_pos_at, CelestialBody,
    FlightPhase, EARTH_ATMO_TOP_M, LAUNCH_PAD_ZONE_X, LAUNCH_PAD_ZONE_Z, ROCKET_FUEL_BURN_PER_S,
    ROCKET_FUEL_MAX, ROCKET_MAX_V, ROCKET_RETRO_FUEL_BURN_PER_S, ROCKET_RETRO_THRUST_M_S2,
    ROCKET_ROT_SPEED, ROCKET_THRUST_M_S2, WorldContext,
};

use crate::solar_system_clock::SolarSystemClock;

use crate::game_time::fixed_delta_secs;

use crate::celestial_cleanup::CelestialCleanup;
use crate::input::{MovementIntent, TouchActions};
use crate::moon_world::MoonWorldState;
use crate::planet_world::{request_planet_spawn, PlanetWorldState};
use crate::scene::{RegionId, RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, PlayerVisual, SimState, WorldCollision, WorldLayout, earth_surface_y};
use crate::surface_placement::tangent_at_world;
use crate::space_world::SpaceWorldState;

pub struct RocketFlightPlugin;

impl Plugin for RocketFlightPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RocketFlight>()
            .add_systems(FixedUpdate, tick_rocket_flight)
            .add_systems(Update, (sync_flight_world_state, try_surface_launch));
    }
}

#[derive(Resource, Default)]
pub struct RocketFlight {
    pub active: bool,
    pub phase: FlightPhase,
    pub pos: Vec3,
    pub vel: Vec3,
    pub pitch: f32,
    pub yaw: f32,
    pub fuel: f32,
    pub body: CelestialBody,
    /// Flying on a walkable displaced shell (Luna, Mars, …).
    pub surface_flight: Option<CelestialBody>,
    pub parachute_open: bool,
}

impl RocketFlight {
    pub fn begin_at(&mut self, pos: Vec3) {
        self.active = true;
        self.phase = FlightPhase::Powered;
        self.pos = pos;
        self.vel = Vec3::ZERO;
        self.pitch = 0.0;
        self.yaw = 0.0;
        self.fuel = ROCKET_FUEL_MAX;
        self.parachute_open = false;
        self.body = self
            .surface_flight
            .unwrap_or_else(|| dominant_body(pos.to_array()));
        eprintln!(
            "ha-flight: rocket ignition {:?} at ({:.0},{:.0},{:.0}) surface={:?}",
            self.body, pos.x, pos.y, pos.z, self.surface_flight
        );
    }

    pub fn begin_surface_gameplay(&mut self, body: CelestialBody, pos: Vec3, vel: Vec3) {
        self.surface_flight = Some(body);
        self.body = body;
        self.begin_at(pos);
        self.vel = vel;
    }

    pub fn begin_luna_gameplay(&mut self, pos: Vec3, vel: Vec3) {
        self.begin_surface_gameplay(CelestialBody::Moon, pos, vel);
    }

    pub fn begin_coast_toward(&mut self, pos: Vec3, vel: Vec3, target: CelestialBody) {
        self.surface_flight = None;
        self.active = true;
        self.phase = FlightPhase::Coast;
        self.pos = pos;
        self.vel = vel;
        self.pitch = 0.0;
        self.yaw = 0.0;
        self.fuel = ROCKET_FUEL_MAX;
        self.parachute_open = false;
        self.body = target;
    }

    pub fn end_surface(&mut self) {
        self.active = false;
        self.phase = FlightPhase::Surface;
        self.vel = Vec3::ZERO;
        self.surface_flight = None;
        self.parachute_open = false;
    }

    pub fn blocks_walking(&self) -> bool {
        self.active && self.phase != FlightPhase::Surface
    }

    pub fn speed(&self) -> f32 {
        self.vel.length()
    }

    pub fn altitude(&self) -> f32 {
        altitude_above_body(self.pos.to_array(), self.body)
    }

    pub fn altitude_m(&self) -> f32 {
        self.altitude()
    }

    pub fn fuel_pct(&self) -> f32 {
        (self.fuel / ROCKET_FUEL_MAX * 100.0).clamp(0.0, 100.0)
    }

    pub fn in_vacuum(&self) -> bool {
        in_vacuum(self.pos.to_array())
    }

    pub fn render_pos(&self, origin: &crate::floating_origin::FloatingOrigin) -> Vec3 {
        origin.to_render(self.pos)
    }
}

fn try_surface_launch(
    actions: Res<TouchActions>,
    region: Res<RegionRuntime>,
    world_ctx: Res<WorldContextRuntime>,
    mut flight: ResMut<RocketFlight>,
    player: Query<&SimState, With<Player>>,
) {
    if flight.active {
        return;
    }
    if !actions.action {
        return;
    }
    let Ok(sim) = player.get_single() else {
        return;
    };
    match (region.active, world_ctx.active) {
        (RegionId::Luna, WorldContext::Luna) => {
            let pos = Vec3::from_array(planet_gameplay_world_pos(
                CelestialBody::Moon,
                sim.body.x,
                sim.body.z,
                2.0,
            ));
            let dir = planet_dir_from_tangent(CelestialBody::Moon, sim.body.x, sim.body.z);
            flight.begin_surface_gameplay(
                CelestialBody::Moon,
                pos,
                Vec3::from_array(dir) * 42.0,
            );
        }
        (RegionId::Planet(body), WorldContext::Planet) if body_landable(body) => {
            let pos = Vec3::from_array(planet_gameplay_world_pos(body, sim.body.x, sim.body.z, 2.0));
            let dir = planet_dir_from_tangent(body, sim.body.x, sim.body.z);
            flight.begin_surface_gameplay(body, pos, Vec3::from_array(dir) * 38.0);
        }
        _ => {}
    }
}

pub fn tick_rocket_flight(
    time: Res<Time<Fixed>>,
    clock: Res<SolarSystemClock>,
    intent: Res<MovementIntent>,
    actions: Res<TouchActions>,
    mut flight: ResMut<RocketFlight>,
    mut region: ResMut<RegionRuntime>,
    mut world_ctx: ResMut<WorldContextRuntime>,
    mut launch_atmo: ResMut<crate::scene::LaunchAtmosphere>,
    mut space: ResMut<SpaceWorldState>,
    mut moon: ResMut<MoonWorldState>,
    mut planet: ResMut<PlanetWorldState>,
    mut cleanup: ResMut<CelestialCleanup>,
    layout: Res<WorldLayout>,
    collision: Res<WorldCollision>,
    mut player: Query<(&mut SimState, &mut PlayerVisual), With<Player>>,
) {
    if !flight.active || flight.phase == FlightPhase::Surface {
        return;
    }

    let dt = fixed_delta_secs(&time);
    let t = clock.t;

    let pos = flight.pos.to_array();
    if flight.surface_flight.is_none() {
        flight.body = dominant_body_at(pos, t);
    }

    flight.yaw += intent.move_x * ROCKET_ROT_SPEED * dt;
    flight.pitch = (flight.pitch + intent.move_z * ROCKET_ROT_SPEED * dt).clamp(-1.2, 1.2);

    let thrust_dir = {
        let cp = flight.pitch.cos();
        let cy = flight.yaw.cos();
        let sy = flight.yaw.sin();
        Vec3::new(sy * cp, cp * cy.max(0.01), -cy * cp).normalize_or_zero()
    };

    let mut accel = match flight.surface_flight {
        Some(body) => Vec3::from_array(gravity_accel_planet_gameplay(body, pos)),
        None => Vec3::from_array(gravity_accel_at_time(pos, t)),
    };
    accel += Vec3::from_array(atmosphere_drag(pos, flight.vel.to_array()));

    if let Some(body) = flight.surface_flight {
        let alt_gp = altitude_above_planet_gameplay(body, pos);
        let escape_alt = body_walk_radius(body) * 0.2;
        if alt_gp > escape_alt || in_vacuum(pos) {
            flight.pos = Vec3::from_array(remap_planet_gameplay_to_orbital(body, pos));
            flight.surface_flight = None;
            flight.body = body;
            eprintln!("ha-flight: {:?} gameplay → orbital", body);
        }
    }

    let retro_hold = intent.sprint || actions.jump;
    let alt = if let Some(body) = flight.surface_flight {
        altitude_above_planet_gameplay(body, pos)
    } else {
        altitude_above_body_at(pos, flight.body, t)
    };
    if !flight.parachute_open
        && should_auto_deploy_parachute(flight.body, alt, flight.speed())
    {
        flight.parachute_open = true;
        eprintln!("ha-flight: parachute deployed");
    }
    if flight.parachute_open {
        accel += Vec3::from_array(parachute_drag(flight.vel.to_array(), 1.0));
    }

    if sun_proximity_damage(pos) {
        flight.vel += Vec3::from_array(surface_normal_at(pos, CelestialBody::Sun, t)) * 120.0 * dt;
        flight.fuel = (flight.fuel - 25.0 * dt).max(0.0);
    }

    if retro_hold && flight.fuel > 0.0 && flight.vel.length() > 0.5 {
        let retro = -flight.vel.normalize_or_zero();
        accel += retro * ROCKET_RETRO_THRUST_M_S2;
        flight.fuel = (flight.fuel - ROCKET_RETRO_FUEL_BURN_PER_S * dt).max(0.0);
        flight.phase = FlightPhase::Retro;
    } else if actions.action && flight.fuel > 0.0 {
        accel += thrust_dir * ROCKET_THRUST_M_S2;
        flight.fuel = (flight.fuel - ROCKET_FUEL_BURN_PER_S * dt).max(0.0);
        flight.phase = FlightPhase::Powered;
    } else {
        flight.phase = FlightPhase::Coast;
    }

    flight.vel += accel * dt;
    let speed = flight.vel.length();
    if speed > ROCKET_MAX_V {
        flight.vel = flight.vel * (ROCKET_MAX_V / speed);
    }
    let step = flight.vel * dt;
    flight.pos += step;

    let earth_alt = altitude_above_body_at(flight.pos.to_array(), CelestialBody::Earth, t);
    launch_atmo.active = earth_alt < EARTH_ATMO_TOP_M * 1.5 && earth_alt > -10.0;
    launch_atmo.blend = (1.0 - earth_alt / (EARTH_ATMO_TOP_M * 1.5)).clamp(0.0, 1.0);

    if flight.in_vacuum() && !space.spawned {
        space.needs_spawn = true;
        cleanup.despawn_moon = false;
    }
    if !flight.surface_flight.is_some() {
        let approach_alt = altitude_above_body_at(flight.pos.to_array(), flight.body, t);
        if approach_alt < 800.0 && body_landable(flight.body) {
            match flight.body {
                CelestialBody::Moon if !moon.spawned => {
                    moon.needs_spawn = true;
                    moon.spawn_after_frames = 0;
                }
                b if b != CelestialBody::Earth && b != CelestialBody::Moon && !planet.spawned => {
                    request_planet_spawn(&mut planet, b);
                }
                _ => {}
            }
        }
    }

    try_land(
        &mut flight,
        t,
        &mut region,
        &mut world_ctx,
        &mut moon,
        &mut planet,
        &layout,
        &collision,
        &mut player,
    );
}

fn try_land(
    flight: &mut RocketFlight,
    t: f32,
    region: &mut RegionRuntime,
    world_ctx: &mut WorldContextRuntime,
    moon: &mut MoonWorldState,
    planet: &mut PlanetWorldState,
    layout: &WorldLayout,
    collision: &WorldCollision,
    player: &mut Query<(&mut SimState, &mut PlayerVisual), With<Player>>,
) {
    let body = flight.body;
    if !body_landable(body) {
        return;
    }
    let alt = if let Some(gp) = flight.surface_flight {
        altitude_above_planet_gameplay(gp, flight.pos.to_array())
    } else {
        altitude_above_body_at(flight.pos.to_array(), body, t)
    };
    let v_down = flight.vel.length();
    if alt > 3.0 {
        return;
    }
    let safe_v = effective_safe_landing_v(flight.parachute_open);
    if v_down > safe_v {
        let n = if let Some(gp) = flight.surface_flight {
            let c = Vec3::from_array(planet_gameplay_center(gp));
            (flight.pos - c).normalize_or_zero()
        } else {
            Vec3::from_array(surface_normal_at(flight.pos.to_array(), body, t))
        };
        flight.vel *= -0.25;
        flight.pos += n * 2.0;
        flight.parachute_open = false;
        eprintln!("ha-flight: hard impact v={v_down:.1}");
        return;
    }

    if let Some(gp_body) = flight.surface_flight {
        land_on_gameplay_shell(flight, region, world_ctx, moon, planet, gp_body, player, v_down);
        return;
    }

    let terrain_h = terrain_height_for_landing(flight.pos, body, t, layout, collision);
    let surf = surface_world_pos_at(flight.pos.to_array(), body, terrain_h, t);
    flight.pos = Vec3::from_array(surf) + Vec3::from_array(surface_normal_at(surf, body, t)) * 1.2;
    flight.vel = Vec3::ZERO;
    flight.parachute_open = false;
    flight.end_surface();

    if let Ok((mut sim, mut visual)) = player.get_single_mut() {
        match body {
            CelestialBody::Earth => {
                region.active = RegionId::Overworld { gx: 0, gz: 0 };
                world_ctx.active = WorldContext::Overworld;
                let (tx, tz) = tangent_at_world(CelestialBody::Earth, flight.pos);
                sim.body.x = tx;
                sim.body.z = tz;
                sim.body.y = earth_surface_y(tx, tz);
                sim.body.vy = 0.0;
            }
            CelestialBody::Moon => {
                region.active = RegionId::Luna;
                world_ctx.active = WorldContext::Luna;
                moon.needs_spawn = true;
                moon.spawn_after_frames = 0;
                let dir = surface_normal_at(flight.pos.to_array(), body, t);
                let (tx, tz) = planet_tangent_from_dir(CelestialBody::Moon, dir);
                sim.body.x = tx;
                sim.body.z = tz;
                sim.body.y = planet_ground_y_at(CelestialBody::Moon, tx, tz);
                sim.body.vy = 0.0;
                sim.body.vx = 0.0;
                sim.body.vz = 0.0;
            }
            other => {
                region.active = RegionId::Planet(other);
                world_ctx.active = WorldContext::Planet;
                request_planet_spawn(planet, other);
                let dir = surface_normal_at(flight.pos.to_array(), body, t);
                let (tx, tz) = planet_tangent_from_dir(other, dir);
                sim.body.x = tx;
                sim.body.z = tz;
                sim.body.y = planet_ground_y_at(other, tx, tz);
                sim.body.vy = 0.0;
                sim.body.vx = 0.0;
                sim.body.vz = 0.0;
            }
        }
        visual.to_x = sim.body.x;
        visual.to_z = sim.body.z;
        visual.to_y = sim.body.y;
    }
    eprintln!("ha-flight: touchdown on {body:?} v={v_down:.1}");
}

fn land_on_gameplay_shell(
    flight: &mut RocketFlight,
    region: &mut RegionRuntime,
    world_ctx: &mut WorldContextRuntime,
    moon: &mut MoonWorldState,
    planet: &mut PlanetWorldState,
    body: CelestialBody,
    player: &mut Query<(&mut SimState, &mut PlayerVisual), With<Player>>,
    v_down: f32,
) {
    let c = Vec3::from_array(planet_gameplay_center(body));
    let n = (flight.pos - c).normalize_or_zero();
    let (tx, tz) = planet_tangent_from_dir(body, n.to_array());
    flight.end_surface();
    if let Ok((mut sim, mut visual)) = player.get_single_mut() {
        if body == CelestialBody::Moon {
            region.active = RegionId::Luna;
            world_ctx.active = WorldContext::Luna;
            moon.needs_spawn = true;
            moon.spawn_after_frames = 0;
        } else {
            region.active = RegionId::Planet(body);
            world_ctx.active = WorldContext::Planet;
            request_planet_spawn(planet, body);
        }
        sim.body.x = tx;
        sim.body.z = tz;
        sim.body.y = planet_ground_y_at(body, tx, tz);
        sim.body.vy = 0.0;
        sim.body.vx = 0.0;
        sim.body.vz = 0.0;
        visual.to_x = tx;
        visual.to_z = tz;
        visual.to_y = sim.body.y;
    }
    eprintln!("ha-flight: touchdown on {body:?} shell v={v_down:.1}");
}

fn terrain_height_for_landing(
    pos: Vec3,
    body: CelestialBody,
    t: f32,
    _layout: &WorldLayout,
    _collision: &WorldCollision,
) -> f32 {
    match body {
        CelestialBody::Earth => {
            let (tx, tz) = tangent_at_world(CelestialBody::Earth, pos);
            earth_surface_y(tx, tz).max(0.0)
        }
        CelestialBody::Moon => {
            let dir = surface_normal_at(pos.to_array(), body, t);
            ha_shared::luna_displacement_at_dir(dir)
        }
        other => {
            let dir = surface_normal_at(pos.to_array(), body, t);
            planet_displacement_at_dir(other, dir)
        }
    }
}

fn sync_flight_world_state(
    flight: Res<RocketFlight>,
    mut region: ResMut<RegionRuntime>,
    mut world_ctx: ResMut<WorldContextRuntime>,
) {
    if !flight.active || flight.phase == FlightPhase::Surface {
        return;
    }
    if flight.in_vacuum() {
        if !matches!(
            region.active,
            RegionId::Space | RegionId::Transit | RegionId::Luna | RegionId::Planet(_)
        ) {
            region.active = RegionId::Space;
            world_ctx.active = WorldContext::Space;
        }
    } else if flight.body == CelestialBody::Earth {
        world_ctx.active = WorldContext::Overworld;
        if matches!(region.active, RegionId::Space) {
            region.active = RegionId::Overworld { gx: 0, gz: 0 };
        }
    }
}

pub fn earth_chunk_load_radius(_rocket: &RocketFlight) -> Option<i32> {
    None
}

pub fn flight_world_transform(
    flight: &RocketFlight,
    origin: &crate::floating_origin::FloatingOrigin,
) -> (Vec3, Quat) {
    let rot = Quat::from_rotation_y(flight.yaw) * Quat::from_rotation_x(flight.pitch);
    (flight.render_pos(origin), rot)
}
