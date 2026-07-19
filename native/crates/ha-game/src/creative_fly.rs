//! Creative free-flight — TRUE-SFS model: the flight happens ENTIRELY in the
//! circular orbital view (the body is always a circle), with NO mid-flight swap.
//! You lift off the planet's edge, rise radially (the circle shrinks as the
//! camera zooms out), cross the atmosphere into space, navigate freely and
//! descend onto any body's edge to land.
//!
//! The top-down surface stays a separate "zone"; you only cross the boundary at
//! lift-off / touchdown, covered by the atmosphere fade (blue on air bodies) or a
//! brief dark fade (airless bodies — Moon/asteroids — SFS-correct: bare circle on
//! black). God-mode debug: no orbits, no fuel, no gravity. SEPARATE from the
//! `RocketFlight` god-node (biblia §9).

use bevy::prelude::*;
use ha_shared::solar_ephemeris::body_center_at;
use ha_shared::{body_atmo_top, body_radius, CelestialBody, WorldContext};

use crate::camera::CameraZoom;
use crate::input::{MovementIntent, TouchActions};
use crate::rocket_flight::{launch_pos_vel, RocketFlight};
use crate::scene::{teleport_to_body, RegionId, RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, PlayerVisual, SimState};
use crate::solar_system_clock::SolarSystemClock;
use crate::world2d::ActivePlanet;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Ground,
    Orbit,
}

#[derive(Resource, Default)]
pub struct CreativeFlight {
    pub phase: Phase,
    /// Orbital-plane position (world x/z) of the flying character.
    pub pos: Vec3,
    /// Reference body for altitude / atmosphere (nearest).
    pub body: CelestialBody,
    /// Altitude above the reference body's surface (metres).
    pub alt: f32,
    /// Sky/atmosphere overlay alpha 0..1 (read by `sky2d`).
    pub atmo: f32,
    /// Sky/atmosphere overlay colour (rgb).
    pub atmo_col: Vec3,
    /// Brief lift-off / touch-down cover (fades the top-down↔circular boundary).
    fade: f32,
    /// Landing only arms once you've climbed away, so lift-off doesn't re-land.
    armed: bool,
}

impl CreativeFlight {
    pub fn in_orbit(&self) -> bool {
        self.phase == Phase::Orbit
    }
    pub fn flying(&self) -> bool {
        self.phase != Phase::Ground
    }
}

/// Radial (up/down) speed from holding JUMP/USE (m/s).
const RADIAL_SPEED: f32 = 260.0;
/// Lateral free-flight speed = view_h × this (auto-scales with zoom).
const LATERAL_FACTOR: f32 = 0.35;
/// Altitude just above the surface where lift-off drops you into orbit.
const LIFTOFF_ALT: f32 = 6.0;
/// Land when the character descends within this of a body's surface.
const LAND_ALT: f32 = 24.0;
/// Camera view height in orbit = (radius + alt) × this (SFS reveal).
const ORBIT_ZOOM_FACTOR: f32 = 1.3;
const ORBIT_ZOOM_MAX: f32 = 2_000_000.0;
const GROUND_VIEW_H: f32 = 30.0;

pub struct CreativeFlyPlugin;

impl Plugin for CreativeFlyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CreativeFlight>()
            .add_systems(Update, (creative_fly_enter, creative_fly_orbit).chain());
    }
}

fn active_body(region: &RegionRuntime) -> Option<CelestialBody> {
    match region.active {
        RegionId::Overworld { .. } => Some(CelestialBody::Earth),
        RegionId::Luna => Some(CelestialBody::Moon),
        RegionId::Planet(b) => Some(b),
        _ => None,
    }
}

fn radial_up(body: CelestialBody, t: f32) -> Vec3 {
    let (pad, _) = launch_pos_vel(body, t, 0.0);
    let c = Vec3::from_array(body_center_at(body, t));
    (pad - c).normalize_or_zero()
}

/// Atmosphere colour of a body (air bodies get their sky tint; airless = dark).
fn atmo_color(body: CelestialBody) -> Vec3 {
    match body {
        CelestialBody::Earth => Vec3::new(0.45, 0.65, 0.90),
        _ => Vec3::new(0.55, 0.42, 0.32),
    }
}

/// JUMP off a surface (creative) → lift straight into the circular flight view.
fn creative_fly_enter(
    editor: Res<crate::ui_editor::UiEditorState>,
    actions: Res<TouchActions>,
    clock: Res<SolarSystemClock>,
    flight: Res<RocketFlight>,
    mut fly: ResMut<CreativeFlight>,
    mut region: ResMut<RegionRuntime>,
    mut ctx: ResMut<WorldContextRuntime>,
) {
    if !editor.creative || fly.flying() || flight.active || !actions.jump_edge {
        return;
    }
    let Some(body) = active_body(&region) else {
        return;
    };
    let c = Vec3::from_array(body_center_at(body, clock.t));
    fly.body = body;
    fly.alt = LIFTOFF_ALT;
    fly.pos = c + radial_up(body, clock.t) * (body_radius(body) + LIFTOFF_ALT);
    fly.phase = Phase::Orbit;
    fly.fade = 1.0; // cover the top-down → circular boundary
    fly.atmo = 1.0;
    fly.atmo_col = atmo_color(body);
    fly.armed = false;
    region.active = RegionId::Space; // the circular orbital view (orbital2d)
    ctx.active = WorldContext::Space;
    eprintln!("ha-creative: lift-off from {body:?} (circular flight)");
}

/// Circular flight (SFS): JUMP = rise (radially out of the nearest body), USE =
/// descend (radially in), left stick = lateral. Camera zoom follows altitude so
/// the body's circle emerges and shrinks. Atmosphere/fade drive the sky overlay.
#[allow(clippy::too_many_arguments)]
fn creative_fly_orbit(
    time: Res<Time>,
    actions: Res<TouchActions>,
    intent: Res<MovementIntent>,
    clock: Res<SolarSystemClock>,
    mut zoom: ResMut<CameraZoom>,
    mut fly: ResMut<CreativeFlight>,
    mut region: ResMut<RegionRuntime>,
    mut ctx: ResMut<WorldContextRuntime>,
    mut planet: ResMut<ActivePlanet>,
    mut player: Query<(&mut SimState, &mut PlayerVisual), With<Player>>,
) {
    if fly.phase != Phase::Orbit {
        return;
    }
    let dt = time.delta_secs();

    // Nearest body → radial "up" + altitude.
    let mut nearest = (CelestialBody::Earth, f32::MAX, Vec3::ZERO);
    for &body in CelestialBody::all() {
        let c = Vec3::from_array(body_center_at(body, clock.t));
        let d = fly.pos.distance(c);
        if d < nearest.1 {
            nearest = (body, d, c);
        }
    }
    let (body, _, center) = nearest;
    fly.body = body;
    let radial = (fly.pos - center).normalize_or_zero();

    let lateral_speed = (zoom.view_h * LATERAL_FACTOR).max(120.0);
    // Lateral (stick) — free 2D in the plane.
    fly.pos.x += intent.move_x * lateral_speed * dt;
    fly.pos.z += intent.move_z * lateral_speed * dt;
    // Radial (JUMP up / USE down), scaled a touch with zoom for reach.
    let radial_v = (if actions.jump { 1.0 } else { 0.0 } - if actions.action { 1.0 } else { 0.0 })
        * RADIAL_SPEED
        * (1.0 + zoom.view_h / 4000.0);
    fly.pos += radial * radial_v * dt;
    fly.pos.y = 0.0;

    let alt = (fly.pos.distance(center) - body_radius(body)).max(0.0);
    fly.alt = alt;

    // Atmosphere overlay: thick near an air body's surface, fading to space; none
    // on airless bodies. Plus the brief lift-off/touch-down fade cover.
    let atmo_top = body_atmo_top(body);
    let atmo = if atmo_top > 0.0 {
        (1.0 - alt / (atmo_top * 5.0)).clamp(0.0, 0.85)
    } else {
        0.0
    };
    fly.fade = (fly.fade - dt * 2.0).max(0.0);
    fly.atmo = atmo.max(fly.fade);
    fly.atmo_col = atmo_color(body);

    // SFS reveal: camera zoom follows altitude.
    zoom.view_h = ((body_radius(body) + alt) * ORBIT_ZOOM_FACTOR).clamp(GROUND_VIEW_H, ORBIT_ZOOM_MAX);

    // Arm landing once you've climbed away, so lift-off doesn't instantly re-land.
    if alt > 150.0 {
        fly.armed = true;
    }
    // Touchdown: descend onto the body's surface (covered by the fade).
    if fly.armed && alt <= LAND_ALT {
        let Ok((mut sim, mut visual)) = player.get_single_mut() else {
            return;
        };
        teleport_to_body(body, &mut region, &mut ctx, &mut planet, &mut sim, &mut visual);
        fly.phase = Phase::Ground;
        fly.atmo = 0.0;
        fly.fade = 0.0;
        zoom.view_h = GROUND_VIEW_H;
        eprintln!("ha-creative: touchdown on {body:?}");
    }
}
