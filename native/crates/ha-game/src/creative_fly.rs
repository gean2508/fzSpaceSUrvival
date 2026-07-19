//! Creative free-flight — the CHARACTER (not a rocket) flies through the orbital
//! view with a CONTINUOUS, PLAYER-DRIVEN trajectory (no auto-animation): hold
//! JUMP to rise, USE to descend, release to hover. The ascent/descent
//! choreography (sky fade, cover clouds/fade, zoom, frame swap altitudes) all
//! come from the SHARED per-body transition layer (`ha_shared::transition` +
//! `crate::transition::FlightTransition`) — same coreography as rocket flight.
//! God-mode debug: no orbits, no fuel, no gravity. SEPARATE from the
//! `RocketFlight` god-node (biblia §9).

use bevy::prelude::*;
use ha_shared::solar_ephemeris::body_center_at;
use ha_shared::{body_radius, transition_profile, CelestialBody, WorldContext};

use crate::camera::CameraZoom;
use crate::input::{MovementIntent, TouchActions};
use crate::rocket_flight::{launch_pos_vel, RocketFlight};
use crate::scene::{teleport_to_body, RegionId, RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, PlayerVisual, SimState};
use crate::solar_system_clock::SolarSystemClock;
use crate::transition::{FlightTransition, TransitionMode, SWAP_VIEW_H};
use crate::world2d::ActivePlanet;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Ground,
    Climb,
    Orbit,
    Descend,
}

#[derive(Resource, Default)]
pub struct CreativeFlight {
    pub phase: Phase,
    /// Orbital-plane position while in Orbit.
    pub pos: Vec3,
    /// Radial altitude above the (climb/descend) body surface, metres.
    pub alt: f32,
    /// The body we're climbing from / descending to.
    pub body: CelestialBody,
}

impl CreativeFlight {
    pub fn in_orbit(&self) -> bool {
        self.phase == Phase::Orbit
    }
    pub fn flying(&self) -> bool {
        self.phase != Phase::Ground
    }
}

/// Player-driven vertical speed (metres of altitude per second) — hold JUMP/USE.
/// Slow: ~10 s from the ground to space so you FEEL the world receding.
const VERT_SPEED: f32 = 32.0;
/// In orbit the camera zoom follows altitude (SFS reveal): the body's circle
/// shrinks as you climb. `view_h = (radius + alt) * this`, clamped. High enough
/// that the planet reads as a DISC below a tiny character (SFS scale), not a wall
/// you're "standing on".
const ORBIT_ZOOM_FACTOR: f32 = 4.5;
const ORBIT_ZOOM_MAX: f32 = 2_000_000.0;
/// Free-flight speed = view_h × this (auto-scales with zoom).
const FLY_SPEED_FACTOR: f32 = 0.35;
/// Auto-descend when the character drifts this close to a body's surface (m).
const APPROACH_LAND_M: f32 = 200.0;

pub struct CreativeFlyPlugin;

impl Plugin for CreativeFlyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CreativeFlight>().add_systems(
            Update,
            (
                creative_fly_enter,
                creative_fly_climb,
                creative_fly_orbit,
                creative_fly_descend,
            )
                .chain(),
        );
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

/// JUMP off a surface (creative) → begin a PLAYER-DRIVEN climb.
fn creative_fly_enter(
    editor: Res<crate::ui_editor::UiEditorState>,
    actions: Res<TouchActions>,
    flight: Res<RocketFlight>,
    region: Res<RegionRuntime>,
    mut fly: ResMut<CreativeFlight>,
) {
    if !editor.creative || fly.flying() || flight.active || !actions.jump_edge {
        return;
    }
    let Some(body) = active_body(&region) else {
        return;
    };
    fly.body = body;
    fly.alt = 0.0;
    fly.phase = Phase::Climb;
    eprintln!("ha-creative: climb start from {body:?}");
}

/// CLIMB (player-driven): hold JUMP to rise, USE to descend, release to hover.
/// Camera zoom, sky fade and cover all track the altitude YOU set (via the
/// shared transition layer). Hand over to orbit at the body's `swap_up`
/// (hidden under full cover). Return to Ground at altitude 0.
fn creative_fly_climb(
    time: Res<Time>,
    actions: Res<TouchActions>,
    clock: Res<SolarSystemClock>,
    mut fly: ResMut<CreativeFlight>,
    mut trans: ResMut<FlightTransition>,
    mut region: ResMut<RegionRuntime>,
    mut ctx: ResMut<WorldContextRuntime>,
) {
    if fly.phase != Phase::Climb {
        return;
    }
    let dt = time.delta_secs();
    let mut vy = 0.0;
    if actions.jump {
        vy += VERT_SPEED; // hold JUMP = up
    }
    if actions.action {
        vy -= VERT_SPEED; // hold USE = down
    }
    fly.alt = (fly.alt + vy * dt).max(0.0);

    // Back on the ground → land (only when actively descending, so hovering at 0
    // doesn't kick you out).
    if fly.alt <= 0.5 && vy < 0.0 {
        fly.phase = Phase::Ground;
        trans.clear();
        return;
    }

    trans.drive(TransitionMode::Ascent, fly.body, fly.alt);

    let p = transition_profile(fly.body);
    if fly.alt >= p.swap_up {
        // Hand over to the orbital frame — the swap itself happens under full
        // cover; the orbital view opens just past the clear band with the disc
        // already revealed (the quick "clouds part" approved by the captures).
        // Entry altitude is EXPLICIT now — it used to emerge from a y-flatten
        // quirk that inflated all orbit distances by the flight-plane offset.
        let entry_alt = p.clear_end + 60.0;
        let c = Vec3::from_array(body_center_at(fly.body, clock.t));
        fly.pos = c + radial_up(fly.body, clock.t) * (body_radius(fly.body) + entry_alt);
        fly.pos.y = 0.0;
        fly.alt = entry_alt;
        fly.phase = Phase::Orbit;
        region.active = RegionId::Space;
        ctx.active = WorldContext::Space;
        eprintln!("ha-creative: reached orbit ({:?})", fly.body);
    }
}

/// ORBIT: free 2D flight in the world plane; the leftover cover clears into
/// space. Auto-descend onto a body you drift into, or USE = land.
#[allow(clippy::too_many_arguments)]
fn creative_fly_orbit(
    time: Res<Time>,
    actions: Res<TouchActions>,
    intent: Res<MovementIntent>,
    clock: Res<SolarSystemClock>,
    mut zoom: ResMut<CameraZoom>,
    mut fly: ResMut<CreativeFlight>,
    mut trans: ResMut<FlightTransition>,
    mut region: ResMut<RegionRuntime>,
    mut ctx: ResMut<WorldContextRuntime>,
    mut planet: ResMut<ActivePlanet>,
    mut player: Query<(&mut SimState, &mut PlayerVisual), With<Player>>,
) {
    if fly.phase != Phase::Orbit {
        return;
    }
    let dt = time.delta_secs();
    let speed = (zoom.view_h * FLY_SPEED_FACTOR).max(120.0);
    fly.pos.x += intent.move_x * speed * dt;
    fly.pos.z += intent.move_z * speed * dt;
    fly.pos.y = 0.0;

    // Distances in the XZ plane only: `fly.pos.y` is flattened to 0 while body
    // centres sit at FLIGHT_PLANE_Y, so a 3D distance would inflate every
    // altitude by the plane offset (it used to make Moon landings untriggerable
    // and skip the cover band entirely).
    let mut nearest = (CelestialBody::Earth, f32::MAX);
    for &body in CelestialBody::all() {
        let c = Vec3::from_array(body_center_at(body, clock.t));
        let d = (Vec2::new(fly.pos.x, fly.pos.z) - Vec2::new(c.x, c.z)).length();
        if d < nearest.1 {
            nearest = (body, d);
        }
    }
    let (body, dist) = nearest;
    // Cover follows the altitude above the nearest body — clear in deep space,
    // rolling back in as you approach any body (per that body's profile: clouds
    // where there's an atmosphere, bare circle where there's none).
    let alt_orbit = (dist - body_radius(body)).max(0.0);
    fly.alt = alt_orbit;
    trans.drive(TransitionMode::Space, body, alt_orbit);
    // SFS reveal: the camera zoom follows altitude — the body's circle emerges
    // as the cover parts and shrinks as you climb; deep space frames the system.
    zoom.view_h = ((body_radius(body) + alt_orbit) * ORBIT_ZOOM_FACTOR)
        .clamp(SWAP_VIEW_H, ORBIT_ZOOM_MAX);
    let touching = dist < body_radius(body) + APPROACH_LAND_M;
    if !touching && !actions.use_edge {
        return;
    }
    let Ok((mut sim, mut visual)) = player.get_single_mut() else {
        return;
    };
    teleport_to_body(body, &mut region, &mut ctx, &mut planet, &mut sim, &mut visual);
    fly.body = body;
    fly.alt = transition_profile(body).swap_down;
    fly.phase = Phase::Descend;
    eprintln!("ha-creative: descent onto {body:?}");
}

/// DESCEND (player-driven): hold USE to keep falling, JUMP to slow/hold. Cover
/// rolls back in through the atmosphere and clears at the ground.
fn creative_fly_descend(
    time: Res<Time>,
    actions: Res<TouchActions>,
    mut fly: ResMut<CreativeFlight>,
    mut trans: ResMut<FlightTransition>,
) {
    if fly.phase != Phase::Descend {
        return;
    }
    let dt = time.delta_secs();
    // Default gentle fall; hold USE to speed it, JUMP to arrest.
    let mut vy = -VERT_SPEED * 0.6;
    if actions.action {
        vy = -VERT_SPEED;
    }
    if actions.jump {
        vy = 0.0;
    }
    let ceiling = transition_profile(fly.body).swap_down;
    fly.alt = (fly.alt + vy * dt).clamp(0.0, ceiling);

    trans.drive(TransitionMode::Descent, fly.body, fly.alt);

    if fly.alt <= 0.5 {
        fly.phase = Phase::Ground;
        fly.alt = 0.0;
        trans.clear();
        eprintln!("ha-creative: touchdown on {:?}", fly.body);
    }
}
