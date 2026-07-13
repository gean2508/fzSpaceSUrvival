//! 2D SFS-style rocket flight — continuous thrust/coast/retro on the shared
//! orbital plane. Physics run in world-plane coords (the same plane the
//! ephemerides live on); render maps to the surface strip below escape
//! altitude and to the orbital view above it. Transitions are pure frame
//! changes (guard B8): no cutscenes, no input blocking, no velocity kicks.

use bevy::prelude::*;

use ha_shared::flight_physics::{PARACHUTE_DRAG, ROCKET_ROT_SPEED};
use ha_shared::solar_ephemeris::body_center_at;
use ha_shared::{
    altitude_above_body_at, body_atmo_top, body_landable, body_radius, dominant_body_at,
    gravity_accel_at_time, CelestialBody, FlightPhase, PARACHUTE_DEPLOY_ALT_M, ROCKET_CRUISE_MAX_V,
    ROCKET_FUEL_BURN_PER_S, ROCKET_FUEL_MAX, ROCKET_MAX_V, ROCKET_RETRO_FUEL_BURN_PER_S,
    ROCKET_RETRO_THRUST_M_S2, ROCKET_THRUST_M_S2, SAFE_LANDING_V,
};

/// Outward radial direction (in the XZ plane) at a given angle from a body
/// centre. Screen-up (−z) is angle −π/2; the rocket launches straight up.
fn radial_dir(angle: f32) -> Vec3 {
    Vec3::new(angle.cos(), 0.0, angle.sin())
}

/// Default launch heading: straight up on screen (−z).
const LAUNCH_ANGLE: f32 = -std::f32::consts::FRAC_PI_2;

use crate::input::{MovementIntent, TouchActions};
use crate::scene::{RegionId, RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, PlayerVisual, SimState};
use crate::solar_system_clock::SolarSystemClock;
use crate::world2d::ActivePlanet;

pub struct RocketFlightPlugin;

impl Plugin for RocketFlightPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RocketFlight>()
            .init_resource::<ShipLoadout>()
            .add_systems(FixedUpdate, tick_rocket_flight)
            .add_systems(
                Update,
                (try_surface_launch, sync_flight_world_state, sync_rocket_visual),
            );
    }
}

/// Altitude above a body where the surface strip hands over to the orbital
/// view (pure render/frame change — physics never notices).
pub fn escape_alt(body: CelestialBody) -> f32 {
    (body_atmo_top(body).max(90.0)) * 1.8
}

/// Orbital insertion point above Earth (nav-computer probes).
pub fn orbital_start_above_pad() -> Vec3 {
    let c = Vec3::from_array(body_center_at(CelestialBody::Earth, 0.0));
    let r = body_radius(CelestialBody::Earth) + escape_alt(CelestialBody::Earth) + 120.0;
    c + radial_dir(LAUNCH_ANGLE) * r
}

/// Launch a rocket straight up off a body's surface into the orbital plane.
pub fn launch_pos_vel(body: CelestialBody, t: f32, speed: f32) -> (Vec3, Vec3) {
    let c = Vec3::from_array(body_center_at(body, t));
    let dir = radial_dir(LAUNCH_ANGLE);
    let pos = c + dir * (body_radius(body) + 2.0);
    (pos, dir * speed)
}

/// Mass/fuel stats derived from the assembled pad modules — a LAYER over the
/// existing fuel system: mass taxes burn rate, never acceleration.
#[derive(Resource, Clone, Copy, Debug)]
pub struct ShipLoadout {
    pub mass_t: f32,
    pub fuel_max: f32,
    pub burn_factor: f32,
    pub thrust_factor: f32,
    pub drag_factor: f32,
    pub fin_align_rate: f32,
    pub safe_landing_bonus: f32,
}

impl Default for ShipLoadout {
    fn default() -> Self {
        let mass = ha_shared::ship_mass_t(&["hull", "fuel_tank", "engine"]);
        Self {
            mass_t: mass,
            fuel_max: ha_shared::fuel_max_for_tanks(1),
            burn_factor: ha_shared::burn_factor(mass),
            thrust_factor: 1.0,
            drag_factor: 1.0,
            fin_align_rate: 0.0,
            safe_landing_bonus: 0.0,
        }
    }
}

#[derive(Resource, Default)]
pub struct RocketFlight {
    pub active: bool,
    pub phase: FlightPhase,
    /// World-plane position (x, FLIGHT_PLANE_Y, z) — coplanar with ephemerides.
    pub pos: Vec3,
    pub vel: Vec3,
    pub pitch: f32,
    /// 2D heading of the nose in the orbital plane (radians, XZ).
    pub yaw: f32,
    pub fuel: f32,
    pub body: CelestialBody,
    /// Flying low over a walkable body's strip (render stays in surface frame).
    pub surface_flight: Option<CelestialBody>,
    pub parachute_open: bool,
    /// Nav-computer cruise: re-aims the velocity at the led intercept each
    /// second; any stick input disengages back to manual.
    pub autopilot: bool,
    pub fuel_max: f32,
    pub burn_factor: f32,
    pub thrust_factor: f32,
    pub drag_factor: f32,
    pub fin_align_rate: f32,
    pub safe_v_bonus: f32,
    /// Solar-clock snapshot from the last tick (for frame remaps off-tick).
    pub clock_secs: f32,
    /// Reference arc position for unwrap-consistent surface render.
    pub surface_ref_s: f32,
}

impl RocketFlight {
    pub fn begin_at(&mut self, pos: Vec3) {
        self.active = true;
        self.phase = FlightPhase::Powered;
        self.pos = pos;
        self.vel = Vec3::ZERO;
        self.pitch = 0.0;
        self.fuel = self.fuel.clamp(0.0, self.capacity());
        self.parachute_open = false;
        self.autopilot = false;
        self.body = self
            .surface_flight
            .unwrap_or_else(|| dominant_body_at(pos.to_array(), self.clock_secs));
        // Nose starts pointing radially out of the current body (XZ angle).
        let c = body_center_at(self.body, self.clock_secs);
        self.yaw = (pos.z - c[2]).atan2(pos.x - c[0]);
        eprintln!(
            "ha-flight: rocket ignition {:?} at ({:.0},{:.0})",
            self.body, pos.x, pos.z
        );
    }

    pub fn begin_surface_gameplay(&mut self, body: CelestialBody, pos: Vec3, vel: Vec3) {
        self.surface_flight = Some(body);
        self.body = body;
        self.begin_at(pos);
        self.vel = vel;
    }

    pub fn begin_coast_toward(&mut self, pos: Vec3, vel: Vec3, target: CelestialBody) {
        self.surface_flight = None;
        self.active = true;
        self.phase = FlightPhase::Coast;
        self.pos = pos;
        self.vel = vel;
        self.pitch = 0.0;
        self.fuel = self.fuel.clamp(0.0, self.capacity());
        self.parachute_open = false;
        self.autopilot = false;
        self.body = target;
    }

    pub fn capacity(&self) -> f32 {
        if self.fuel_max > 0.0 {
            self.fuel_max
        } else {
            ROCKET_FUEL_MAX
        }
    }

    pub fn burn_rate_factor(&self) -> f32 {
        if self.burn_factor > 0.0 {
            self.burn_factor
        } else {
            1.0
        }
    }

    pub fn thrust_rate_factor(&self) -> f32 {
        if self.thrust_factor > 0.0 {
            self.thrust_factor
        } else {
            1.0
        }
    }

    pub fn drag_rate_factor(&self) -> f32 {
        if self.drag_factor > 0.0 {
            self.drag_factor
        } else {
            1.0
        }
    }

    pub fn end_surface(&mut self) {
        self.active = false;
        self.phase = FlightPhase::Surface;
        self.vel = Vec3::ZERO;
        self.surface_flight = None;
        self.parachute_open = false;
        self.autopilot = false;
    }

    pub fn blocks_walking(&self) -> bool {
        self.active && self.phase != FlightPhase::Surface
    }

    pub fn speed(&self) -> f32 {
        self.vel.length()
    }

    pub fn altitude(&self) -> f32 {
        altitude_above_body_at(self.pos.to_array(), self.body, self.clock_secs)
    }

    pub fn altitude_m(&self) -> f32 {
        self.altitude()
    }

    pub fn fuel_pct(&self) -> f32 {
        (self.fuel / self.capacity() * 100.0).clamp(0.0, 100.0)
    }

    pub fn in_vacuum(&self) -> bool {
        self.altitude() > body_atmo_top(self.body)
    }

    /// Top-down: flight always renders on the orbital plane `(x, -z)`; the
    /// surface↔space transition is a camera zoom, not a render-frame swap.
    pub fn render_pos_2d(&self) -> Vec2 {
        Vec2::new(self.pos.x, -self.pos.z)
    }

    /// Nose angle for the 2D sprite: heading yaw in the world plane → screen.
    pub fn render_rot_2d(&self) -> f32 {
        -self.yaw + std::f32::consts::FRAC_PI_2
    }
}


/// One fixed-step of 2D flight physics.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
pub fn tick_rocket_flight(
    time: Res<Time<Fixed>>,
    clock: Res<SolarSystemClock>,
    intent: Res<MovementIntent>,
    actions: Res<TouchActions>,
    loadout: Res<ShipLoadout>,
    mut flight: ResMut<RocketFlight>,
    mut region: ResMut<RegionRuntime>,
    mut ctx: ResMut<WorldContextRuntime>,
    mut planet: ResMut<ActivePlanet>,
    mut player: Query<(&mut SimState, &mut PlayerVisual), With<Player>>,
) {
    let dt = crate::game_time::fixed_delta_secs(&time);
    let t = clock.t;
    flight.clock_secs = t;

    // Loadout mirror (ship builder tanks/engines/fins/nose).
    flight.fuel_max = loadout.fuel_max;
    flight.burn_factor = loadout.burn_factor;
    flight.thrust_factor = loadout.thrust_factor;
    flight.drag_factor = loadout.drag_factor;
    flight.fin_align_rate = loadout.fin_align_rate;
    flight.safe_v_bonus = loadout.safe_landing_bonus;

    if !flight.active || flight.phase == FlightPhase::Surface {
        return;
    }

    // Steering: move_x rotates the nose; manual input disengages autopilot.
    if intent.move_x.abs() > 0.15 {
        flight.autopilot = false;
    }
    if !flight.autopilot {
        flight.yaw += intent.move_x * ROCKET_ROT_SPEED * dt;
    }

    // Thrust (hold JUMP), retro (hold sprint).
    let thrusting = actions.jump && flight.fuel > 0.0;
    let retro = intent.sprint && flight.fuel > 0.0 && flight.vel.length() > 1.0;
    flight.phase = if thrusting {
        FlightPhase::Powered
    } else if retro {
        FlightPhase::Retro
    } else {
        FlightPhase::Coast
    };

    let mut accel = Vec3::from_array(gravity_accel_at_time(flight.pos.to_array(), t));
    if thrusting {
        let dir = Vec3::new(flight.yaw.cos(), 0.0, flight.yaw.sin());
        accel += dir * ROCKET_THRUST_M_S2 * flight.thrust_rate_factor();
        flight.fuel =
            (flight.fuel - ROCKET_FUEL_BURN_PER_S * flight.burn_rate_factor() * dt).max(0.0);
    } else if retro {
        let dir = -flight.vel.normalize_or_zero();
        accel += dir * ROCKET_RETRO_THRUST_M_S2;
        flight.fuel = (flight.fuel - ROCKET_RETRO_FUEL_BURN_PER_S * dt).max(0.0);
    }

    // Atmospheric drag + parachute (Earth-like bodies only).
    let alt = flight.altitude();
    let atmo_top = body_atmo_top(flight.body);
    if atmo_top > 0.0 && alt < atmo_top {
        let density = (1.0 - alt / atmo_top).clamp(0.0, 1.0);
        let mut drag = 0.055 * density * flight.drag_rate_factor();
        let descending = {
            let c = ha_shared::solar_ephemeris::body_center_at(flight.body, t);
            let radial = (flight.pos - Vec3::from_array(c)).normalize_or_zero();
            flight.vel.dot(radial) < 0.0
        };
        if descending && alt < PARACHUTE_DEPLOY_ALT_M && flight.speed() > 9.0 {
            flight.parachute_open = true;
        }
        if flight.parachute_open {
            drag += PARACHUTE_DRAG * density;
        }
        accel -= flight.vel * drag;
    } else {
        flight.parachute_open = false;
    }

    flight.vel += accel * dt;
    let max_v = if alt > escape_alt(flight.body) * 2.0 {
        ROCKET_CRUISE_MAX_V
    } else {
        ROCKET_MAX_V
    };
    if flight.vel.length() > max_v {
        flight.vel = flight.vel.normalize_or_zero() * max_v;
    }
    flight.vel.y = 0.0;
    let step = flight.vel * dt;
    flight.pos += step;
    flight.pos.y = ha_shared::celestial_bodies::FLIGHT_PLANE_Y;

    // Dominant-body handover (SOI).
    let dom = dominant_body_at(flight.pos.to_array(), t);
    if dom != flight.body {
        flight.body = dom;
        flight.surface_flight = None;
    }

    // Touchdown: the rocket reaches the body's surface radius → zoom into a
    // top-down surface patch. Radial descent speed gates a hard landing.
    if body_landable(flight.body) && flight.altitude() <= 0.4 {
        let c = Vec3::from_array(body_center_at(flight.body, t));
        let radial = (flight.pos - c).normalize_or_zero();
        let descent_v = -flight.vel.dot(radial);
        let safe_v =
            SAFE_LANDING_V + flight.safe_v_bonus + if flight.parachute_open { 8.0 } else { 0.0 };
        let hard = descent_v > safe_v;
        if let Ok((mut sim, mut visual)) = player.get_single_mut() {
            // Top-down: land on a walkable surface cell (patch, not a longitude).
            let (sx, sz) = crate::scene::walkable_spawn(flight.body, 0.0, 6.0);
            sim.body.x = sx;
            sim.body.z = sz;
            sim.body.y = 0.0;
            sim.body.vx = 0.0;
            sim.body.vz = 0.0;
            sim.body.vy = 0.0;
            visual.from_x = sx;
            visual.to_x = sx;
            visual.from_z = sz;
            visual.to_z = sz;
            if hard {
                sim.hp = (sim.hp - 25.0).max(1.0);
            }
        }
        // Enter the landed body's surface (top-down patch).
        set_surface_region(flight.body, &mut region, &mut ctx, &mut planet);
        eprintln!(
            "ha-flight: touchdown on {:?} (hard={hard}, descent_v={descent_v:.0})",
            flight.body
        );
        flight.end_surface();
    }
}

fn set_surface_region(
    body: CelestialBody,
    region: &mut RegionRuntime,
    ctx: &mut WorldContextRuntime,
    planet: &mut ActivePlanet,
) {
    use ha_shared::WorldContext;
    let (r, c) = match body {
        CelestialBody::Earth => (RegionId::Overworld { gx: 0, gz: 0 }, WorldContext::Overworld),
        CelestialBody::Moon => (RegionId::Luna, WorldContext::Luna),
        other => (RegionId::Planet(other), WorldContext::Planet),
    };
    region.active = r;
    ctx.active = c;
    if !matches!(body, CelestialBody::Earth | CelestialBody::Moon) {
        planet.body = Some(body);
    }
}

/// Relaunch from the Moon / a planet: USE with nothing targeted ignites the
/// lander (spends remaining fuel — only the Earth pad refuels).
fn try_surface_launch(
    actions: Res<TouchActions>,
    region: Res<RegionRuntime>,
    clock: Res<SolarSystemClock>,
    target: Res<crate::targeting::LookTarget>,
    hold: Res<crate::input::HoldInteractState>,
    mut flight: ResMut<RocketFlight>,
    player: Query<&SimState, With<Player>>,
) {
    if flight.active || !actions.action {
        return;
    }
    // USE while aiming at an interactable, over a tile (dig/place), or mid/just-
    // finished hold must interact, not ignite. `hold.active` sets actions.action
    // itself, so a completing dig/place hold leaves action=true for a frame with
    // hold.active already false — without the place_cell/completed guards that
    // residual USE would re-launch the lander the instant you place a block.
    if target.kind.is_some()
        || target.block_cell.is_some()
        || target.place_cell.is_some()
        || hold.active
        || hold.completed
    {
        return;
    }
    let body = match region.active {
        RegionId::Luna => CelestialBody::Moon,
        RegionId::Planet(b) if body_landable(b) => b,
        _ => return,
    };
    if player.get_single().is_err() {
        return;
    }
    flight.clock_secs = clock.t;
    let (pos, vel) = launch_pos_vel(body, clock.t, 42.0);
    flight.begin_surface_gameplay(body, pos, vel);
}

/// Keep RegionId/WorldContext in sync with the flight frame (B8: the ONLY
/// place surface↔space transitions happen besides cave portals).
fn sync_flight_world_state(
    flight: Res<RocketFlight>,
    mut region: ResMut<RegionRuntime>,
    mut ctx: ResMut<WorldContextRuntime>,
) {
    use ha_shared::WorldContext;
    // While flying (not on a surface) the view is the orbital plane = Space.
    // Touchdown sets the surface region directly (see `set_surface_region`).
    if !flight.active || flight.phase == FlightPhase::Surface {
        return;
    }
    if region.active != RegionId::Space {
        region.active = RegionId::Space;
    }
    if ctx.active != WorldContext::Space {
        ctx.active = WorldContext::Space;
    }
}

// ---------------------------------------------------------------------------
// 2D rocket sprite
// ---------------------------------------------------------------------------

#[derive(Component)]
struct FlightRocketVisual;

#[derive(Component)]
struct FlightRocketFlame;

fn sync_rocket_visual(
    mut commands: Commands,
    flight: Res<RocketFlight>,
    art: Res<crate::art2d::ArtLibrary>,
    mut rocket: Query<(Entity, &mut Transform), With<FlightRocketVisual>>,
    mut flame: Query<&mut Visibility, With<FlightRocketFlame>>,
) {
    let flying = flight.blocks_walking();
    if !flying {
        for (e, _) in &rocket {
            commands.entity(e).despawn_recursive();
        }
        return;
    }
    let pos = flight.render_pos_2d();
    let rot = flight.render_rot_2d();
    if let Ok((_, mut tf)) = rocket.get_single_mut() {
        tf.translation = pos.extend(3.0);
        // Sprite nose points +Y at rot 0; rotate so it matches heading.
        tf.rotation = Quat::from_rotation_z(-rot);
        // At high zoom the rocket sprite scales up so it stays visible.
        let scale = (flight.altitude_m() / 900.0).clamp(1.0, 26.0);
        tf.scale = Vec3::splat(scale);
        for mut vis in &mut flame {
            *vis = if flight.phase == FlightPhase::Powered {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
        return;
    }
    // Drop-in pack art (assets/art/ship/rocket.png) or procedural stack.
    let pack_art = art.get("ship/rocket");
    let body_sprite = match pack_art.clone() {
        Some(image) => {
            let mut s = Sprite::from_image(image);
            s.custom_size = Some(Vec2::new(2.2, 6.0));
            s
        }
        None => Sprite::from_color(Color::srgb(0.82, 0.84, 0.88), Vec2::new(1.6, 4.6)),
    };
    commands
        .spawn((
            FlightRocketVisual,
            body_sprite,
            Transform::from_translation(pos.extend(3.0)),
        ))
        .with_children(|p| {
            if pack_art.is_none() {
                // Nose cone (only for the procedural stack).
                p.spawn((
                    Sprite::from_color(Color::srgb(0.85, 0.45, 0.2), Vec2::new(1.2, 1.0)),
                    Transform::from_xyz(0.0, 2.6, 0.05),
                ));
            }
            // Engine flame.
            p.spawn((
                FlightRocketFlame,
                Sprite::from_color(Color::srgb(1.0, 0.72, 0.25), Vec2::new(0.9, 1.8)),
                Transform::from_xyz(0.0, -3.0, 0.05),
                Visibility::Hidden,
            ));
        });
}
