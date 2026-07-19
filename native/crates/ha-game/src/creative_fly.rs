//! Creative free-flight — the CHARACTER (not a rocket) flies through the orbital
//! view with a CONTINUOUS, PLAYER-DRIVEN trajectory (no auto-animation): hold
//! JUMP to rise, USE to descend, release to hover. As you climb the sky fades and
//! CLOUDS roll in to hide the ground (and the surface→orbit swap), then clear into
//! space. Landing falls back through the clouds. God-mode debug: no orbits, no
//! fuel, no gravity. SEPARATE from the `RocketFlight` god-node (biblia §9).

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
    /// Cloud-cover alpha 0..1 (read by `sky2d` to draw the atmosphere layer).
    pub cloud: f32,
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
const VERT_SPEED: f32 = 320.0;
/// Altitude where the surface view hands over to orbit. Kept LOW + under full
/// clouds so the flat-tile view never zooms out enough to reveal its finite edge
/// — then the (infinitely-zoomable) orbital circle takes over. Hidden by clouds.
const SWAP_ALT: f32 = 300.0;
/// Camera viewport height (m) at the swap — small, matching the local ground scale
/// so the swap to the orbital frame is scale-continuous.
const SWAP_VIEW_H: f32 = 450.0;
const GROUND_VIEW_H: f32 = 30.0;
/// In orbit the camera zoom follows altitude (SFS reveal): the body's circle
/// shrinks as you climb. `view_h = (radius + alt) * this`, clamped.
const ORBIT_ZOOM_FACTOR: f32 = 1.2;
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

/// Cloud alpha as a function of altitude: none on the ground, full EARLY (~180 m,
/// before the finite streamed terrain reveals its empty edge — the clouds are the
/// optimisation that lets us stop drawing far terrain), staying thick through the
/// frame swap, clearing toward space. Same curve in atmosphere (Climb/Descend) and
/// near a body in Orbit, so approaching any body brings its clouds back.
fn cloud_from_alt(alt: f32) -> f32 {
    let rising = ((alt - 40.0) / 90.0).clamp(0.0, 1.0); // full by ~130 m
    let clearing = ((900.0 - alt) / 400.0).clamp(0.0, 1.0); // clear by ~900 m
    rising.min(clearing)
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
    fly.cloud = 0.0;
    fly.phase = Phase::Climb;
    eprintln!("ha-creative: climb start from {body:?}");
}

/// CLIMB (player-driven): hold JUMP to rise, USE to descend, release to hover.
/// Camera zoom, sky fade and clouds all track the altitude YOU set. Hand over to
/// orbit at `SWAP_ALT` (hidden under the clouds). Return to Ground at altitude 0.
#[allow(clippy::too_many_arguments)]
fn creative_fly_climb(
    time: Res<Time>,
    actions: Res<TouchActions>,
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
        fly.cloud = 0.0;
        launch.active = false;
        zoom.view_h = GROUND_VIEW_H;
        return;
    }

    let f = (fly.alt / 900.0).clamp(0.0, 1.0); // sky fully void by ~900 m
    launch.active = true;
    launch.blend = f; // sky fade (surface region) — sky2d reads this
    fly.cloud = cloud_from_alt(fly.alt);
    let zf = (fly.alt / SWAP_ALT).clamp(0.0, 1.0);
    zoom.view_h = GROUND_VIEW_H + (SWAP_VIEW_H - GROUND_VIEW_H) * zf;

    if fly.alt >= SWAP_ALT {
        // Hand over to the orbital frame — under full cloud cover, so no cut.
        let c = Vec3::from_array(body_center_at(fly.body, clock.t));
        fly.pos = c + radial_up(fly.body, clock.t) * (body_radius(fly.body) + fly.alt);
        fly.phase = Phase::Orbit;
        launch.active = false;
        region.active = RegionId::Space;
        ctx.active = WorldContext::Space;
        eprintln!("ha-creative: reached orbit ({:?})", fly.body);
    }
}

/// ORBIT: free 2D flight in the world plane; the leftover clouds clear into space
/// over the first moment. Auto-descend onto a body you drift into, or USE = land.
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
    let speed = (zoom.view_h * FLY_SPEED_FACTOR).max(120.0);
    fly.pos.x += intent.move_x * speed * dt;
    fly.pos.z += intent.move_z * speed * dt;
    fly.pos.y = 0.0;

    let mut nearest = (CelestialBody::Earth, f32::MAX);
    for &body in CelestialBody::all() {
        let c = Vec3::from_array(body_center_at(body, clock.t));
        let d = fly.pos.distance(c);
        if d < nearest.1 {
            nearest = (body, d);
        }
    }
    let (body, dist) = nearest;
    // Clouds follow the altitude above the nearest body — clear in deep space,
    // roll back in as you approach any body (Earth, Moon, planets).
    let alt_orbit = (dist - body_radius(body)).max(0.0);
    fly.alt = alt_orbit;
    fly.cloud = cloud_from_alt(alt_orbit);
    // SFS reveal: the camera zoom follows altitude — the body's circle emerges as
    // the clouds part and shrinks as you climb; deep space frames the system.
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
    fly.alt = SWAP_ALT;
    fly.phase = Phase::Descend;
    eprintln!("ha-creative: descent onto {body:?}");
}

/// DESCEND (player-driven): hold USE to keep falling, JUMP to slow/hold. Clouds
/// roll back in through the atmosphere and clear at the ground.
fn creative_fly_descend(
    time: Res<Time>,
    actions: Res<TouchActions>,
    mut fly: ResMut<CreativeFlight>,
    mut launch: ResMut<LaunchAtmosphere>,
    mut zoom: ResMut<CameraZoom>,
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
    fly.alt = (fly.alt + vy * dt).clamp(0.0, SWAP_ALT);

    let f = (fly.alt / 900.0).clamp(0.0, 1.0); // sky fully void by ~900 m
    launch.active = true;
    launch.blend = f;
    fly.cloud = cloud_from_alt(fly.alt);
    let zf = (fly.alt / SWAP_ALT).clamp(0.0, 1.0);
    zoom.view_h = GROUND_VIEW_H + (SWAP_VIEW_H - GROUND_VIEW_H) * zf;

    if fly.alt <= 0.5 {
        fly.phase = Phase::Ground;
        fly.alt = 0.0;
        fly.cloud = 0.0;
        launch.active = false;
        zoom.view_h = GROUND_VIEW_H;
        eprintln!("ha-creative: touchdown on {:?}", fly.body);
    }
}
