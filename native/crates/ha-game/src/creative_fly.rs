//! Creative free-flight — the CHARACTER (not a rocket) flies through the orbital
//! view, with a CONTINUOUS trajectory (no cut): ground → atmosphere → space and
//! back, SFS-style. God-mode debug: no orbits, no fuel, no gravity.
//!
//! Phases: Ground → Climb (rise through the sky; `LaunchAtmosphere.blend` fades
//! the atmosphere, camera zooms out) → Orbit (free 2D flight in the world plane)
//! → Descend (fall through the target body's atmosphere to the ground). Landing
//! reuses `teleport_to_body` for correct surface coords (no risky inverse remap).
//! Deliberately SEPARATE from the `RocketFlight` god-node (biblia §9).

use bevy::prelude::*;
use ha_shared::solar_ephemeris::body_center_at;
use ha_shared::{body_radius, CelestialBody, WorldContext};

use crate::camera::CameraZoom;
use crate::input::{MovementIntent, TouchActions};
use crate::rocket_flight::{launch_pos_vel, RocketFlight};
use crate::scene::{teleport_to_body, LaunchAtmosphere, RegionId, RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, PlayerVisual, SimState};
use crate::solar_system_clock::SolarSystemClock;
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

/// Climb/descent vertical speed (metres of altitude per second).
const VERT_SPEED: f32 = 900.0;
/// Altitude at which the surface view hands over to the orbital view (past the
/// 150→1200 m atmosphere fade, so the sky is already void → seamless swap).
const TRANSITION_ALT: f32 = 1_500.0;
/// Camera viewport height (m) at the transition — frames the body's circle.
const TRANSITION_VIEW_H: f32 = 5_000.0;
const GROUND_VIEW_H: f32 = 30.0;
/// Free-flight speed = view_h × this (auto-scales with zoom).
const FLY_SPEED_FACTOR: f32 = 0.55;
/// Auto-descend when the character drifts this close to a body's surface (m).
const APPROACH_LAND_M: f32 = 300.0;

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

/// Radial "up" unit direction off a body (the canonical launch bearing).
fn radial_up(body: CelestialBody, t: f32) -> Vec3 {
    let (pad, _) = launch_pos_vel(body, t, 0.0);
    let c = Vec3::from_array(body_center_at(body, t));
    (pad - c).normalize_or_zero()
}

/// JUMP off a surface (creative) → start CLIMBING (continuous ascent begins).
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

/// CLIMB: rise through the atmosphere. Stay in the surface region while the sky
/// fades (`LaunchAtmosphere.blend`) and the camera zooms out; hand over to the
/// orbital view once past `TRANSITION_ALT` (sky already void → no visible cut).
fn creative_fly_climb(
    time: Res<Time>,
    actions: Res<TouchActions>,
    intent: Res<MovementIntent>,
    clock: Res<SolarSystemClock>,
    mut fly: ResMut<CreativeFlight>,
    mut launch: ResMut<LaunchAtmosphere>,
    mut zoom: ResMut<CameraZoom>,
    mut region: ResMut<RegionRuntime>,
    mut ctx: ResMut<WorldContextRuntime>,
) {
    if fly.phase != Phase::Climb {
        return;
    }
    let dt = time.delta_secs();
    // JUMP held keeps climbing; sprint (retro) eases you back down.
    let up = if intent.sprint { -0.6 } else { 1.0 };
    fly.alt = (fly.alt + VERT_SPEED * up * dt).max(0.0);

    // Cancel the ascent if you sink back to the ground.
    if fly.alt <= 0.5 && intent.sprint {
        fly.phase = Phase::Ground;
        launch.active = false;
        zoom.view_h = GROUND_VIEW_H;
        return;
    }

    let f = (fly.alt / TRANSITION_ALT).clamp(0.0, 1.0);
    launch.active = true;
    launch.blend = f; // 0 = sky, 1 = void (sky2d reads this on the surface)
    zoom.view_h = GROUND_VIEW_H + (TRANSITION_VIEW_H - GROUND_VIEW_H) * f;

    if fly.alt >= TRANSITION_ALT {
        // Hand over to the orbital frame: place the character radially above the
        // body at the altitude reached. USE lets you aim once flying.
        let _ = &actions;
        let c = Vec3::from_array(body_center_at(fly.body, clock.t));
        fly.pos = c + radial_up(fly.body, clock.t) * (body_radius(fly.body) + fly.alt);
        fly.phase = Phase::Orbit;
        launch.active = false;
        region.active = RegionId::Space;
        ctx.active = WorldContext::Space;
        eprintln!("ha-creative: reached orbit ({:?})", fly.body);
    }
}

/// ORBIT: free 2D movement in the world plane. Auto-descend when you drift onto
/// a body, or press USE to descend onto the nearest one.
#[allow(clippy::too_many_arguments)]
fn creative_fly_orbit(
    time: Res<Time>,
    actions: Res<TouchActions>,
    intent: Res<MovementIntent>,
    clock: Res<SolarSystemClock>,
    zoom: Res<CameraZoom>,
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
    let speed = (zoom.view_h * FLY_SPEED_FACTOR).max(120.0);
    fly.pos.x += intent.move_x * speed * dt;
    fly.pos.z += intent.move_z * speed * dt;
    fly.pos.y = 0.0;

    // Nearest body + distance.
    let mut nearest = (CelestialBody::Earth, f32::MAX);
    for &body in CelestialBody::all() {
        let c = Vec3::from_array(body_center_at(body, clock.t));
        let d = fly.pos.distance(c);
        if d < nearest.1 {
            nearest = (body, d);
        }
    }
    let (body, dist) = nearest;
    let touching = dist < body_radius(body) + APPROACH_LAND_M;
    if !touching && !actions.use_edge {
        return;
    }
    // Begin a continuous DESCENT onto this body: drop into its surface region
    // (via teleport_to_body, correct coords) but start high + void, then fall.
    let Ok((mut sim, mut visual)) = player.get_single_mut() else {
        return;
    };
    teleport_to_body(body, &mut region, &mut ctx, &mut planet, &mut sim, &mut visual);
    fly.body = body;
    fly.alt = TRANSITION_ALT;
    fly.phase = Phase::Descend;
    eprintln!("ha-creative: descent onto {body:?}");
}

/// DESCEND: fall through the target atmosphere to the ground (sky fades in,
/// camera zooms in). Reaches `Ground` at altitude 0.
fn creative_fly_descend(
    time: Res<Time>,
    mut fly: ResMut<CreativeFlight>,
    mut launch: ResMut<LaunchAtmosphere>,
    mut zoom: ResMut<CameraZoom>,
) {
    if fly.phase != Phase::Descend {
        return;
    }
    let dt = time.delta_secs();
    fly.alt = (fly.alt - VERT_SPEED * dt).max(0.0);
    let f = (fly.alt / TRANSITION_ALT).clamp(0.0, 1.0);
    launch.active = true;
    launch.blend = f;
    zoom.view_h = GROUND_VIEW_H + (TRANSITION_VIEW_H - GROUND_VIEW_H) * f;
    if fly.alt <= 0.5 {
        fly.phase = Phase::Ground;
        fly.alt = 0.0;
        launch.active = false;
        zoom.view_h = GROUND_VIEW_H;
        eprintln!("ha-creative: touchdown on {:?}", fly.body);
    }
}
