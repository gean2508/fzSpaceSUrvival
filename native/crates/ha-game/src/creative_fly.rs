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
use crate::transition::{FlightTransition, TransitionMode};
use crate::world2d::ActivePlanet;

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
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
    /// Landing is only allowed once you've climbed clear of the cover band
    /// (past `clear_end`). Prevents the just-entered orbit from instantly
    /// re-landing on small/airless bodies where `swap_up < APPROACH_LAND_M`
    /// (the Moon "white square" re-teleport loop).
    pub land_armed: bool,
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
/// Free-flight speed = view_h × this (auto-scales with zoom). The camera zoom
/// itself comes from `transition::flight_view_h` (the SFS disc reveal).
const FLY_SPEED_FACTOR: f32 = 0.35;
/// Hard cap on orbital free-flight speed (m/s). WITHOUT this, zooming out to
/// the system view (`view_h` up to 2 000 000) made speed ~700 000 m/s → you
/// crossed to another planet in a blink ("salgo en 2 segundos"). At ~5500 m/s
/// the Earth→Moon hop (~52 km) takes ~10 s — travel FEELS like distance,
/// building a ship is worth it. Farther bodies take proportionally longer.
const MAX_ORBIT_SPEED: f32 = 5_500.0;
const MIN_ORBIT_SPEED: f32 = 120.0;
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
                sync_player_shadow,
            )
                .chain(),
        );
    }
}

/// Ground shadow while flying above visible terrain (Climb/Descend, any
/// body) — reinforces the sense of altitude. Hidden on foot (baseline look
/// untouched) and in Orbit (no ground in view to fall on). Full size/alpha at
/// alt 0, fades out approaching the swap (already hidden under cover there
/// anyway). Faded further by daylight on Earth ("con luz" — no per-body day
/// cycle exists yet elsewhere, so other bodies keep it constant; see plan).
fn sync_player_shadow(
    fly: Res<CreativeFlight>,
    day: Res<crate::sky2d::DayClock>,
    mut shadow: Query<(&mut Visibility, &mut Sprite), With<crate::sim::PlayerFlightShadow>>,
) {
    let Ok((mut vis, mut sprite)) = shadow.get_single_mut() else {
        return;
    };
    let grounded_flight = fly.flying() && !fly.in_orbit();
    if !grounded_flight {
        *vis = Visibility::Hidden;
        return;
    }
    *vis = Visibility::Visible;
    let p = transition_profile(fly.body);
    let f = (1.0 - fly.alt / p.swap_up.max(1.0)).clamp(0.0, 1.0);
    let light = if fly.body == CelestialBody::Earth { day.daylight().max(0.15) } else { 1.0 };
    sprite.color = Color::srgba(0.0, 0.0, 0.0, 0.5 * f * light);
    sprite.custom_size = Some(Vec2::new(1.2, 0.5) * (0.4 + 0.6 * f));
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
        // cover. `fly.alt` carries over UNCHANGED, and the camera zoom is the
        // SAME continuous `flight_view_h(alt)` on both sides of the swap, so
        // there's no jump. Above the swap the disc reveal (grows/shrinks with
        // altitude) takes over as the cover clears.
        let c = Vec3::from_array(body_center_at(fly.body, clock.t));
        fly.pos = c + radial_up(fly.body, clock.t) * (body_radius(fly.body) + fly.alt);
        fly.pos.y = 0.0;
        fly.phase = Phase::Orbit;
        fly.land_armed = false; // must climb clear of the cover before landing
        region.active = RegionId::Space;
        ctx.active = WorldContext::Space;
        eprintln!("ha-creative: reached orbit ({:?})", fly.body);
    }
}

/// Nearest body (XZ-plane distance) to a world-plane position. Y is ignored:
/// `fly.pos.y` is flattened to 0 while body centres sit at FLIGHT_PLANE_Y, so a
/// 3D distance would inflate every altitude by the plane offset.
fn nearest_body_xz(pos: Vec3, t: f32) -> (CelestialBody, f32) {
    let p = Vec2::new(pos.x, pos.z);
    let mut nearest = (CelestialBody::Earth, f32::MAX);
    for &body in CelestialBody::all() {
        let c = body_center_at(body, t);
        let d = (p - Vec2::new(c[0], c[2])).length();
        if d < nearest.1 {
            nearest = (body, d);
        }
    }
    nearest
}

/// ORBIT: free flight in the world plane with FULL directional control —
/// JUMP = climb radially away from the nearest body, USE = descend toward it,
/// stick = lateral. So holding JUMP from the ground is ONE continuous ascent:
/// through the swap (under full cover) and on OUT of the cover band, the planet
/// revealed gradually as a disc — no sudden "clouds cut, blue circle" jump.
/// Landing only once you've climbed clear (`land_armed`) and drift close.
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
    // Speed scales with zoom (so it feels right at every scale) but is CAPPED —
    // otherwise system-view zoom made travel instant. See MAX_ORBIT_SPEED.
    let speed = (zoom.view_h * FLY_SPEED_FACTOR).clamp(MIN_ORBIT_SPEED, MAX_ORBIT_SPEED);

    // Radial direction (away from the nearest body) for JUMP/USE vertical control.
    let (body0, _) = nearest_body_xz(fly.pos, clock.t);
    let c0 = body_center_at(body0, clock.t);
    let radial = (Vec2::new(fly.pos.x, fly.pos.z) - Vec2::new(c0[0], c0[2])).normalize_or_zero();
    let mut rv = 0.0;
    if actions.jump {
        rv += 1.0; // hold JUMP = climb out
    }
    if actions.action {
        rv -= 1.0; // hold USE = descend toward the body
    }
    fly.pos.x += (intent.move_x + radial.x * rv) * speed * dt;
    fly.pos.z += (intent.move_z + radial.y * rv) * speed * dt;
    fly.pos.y = 0.0;

    // Nearest body AFTER moving → cover, zoom, altitude, landing.
    let (body, dist) = nearest_body_xz(fly.pos, clock.t);
    let alt_orbit = (dist - body_radius(body)).max(0.0);
    fly.alt = alt_orbit;
    let p = transition_profile(body);
    // Arm landing once you've climbed clear of the cover band (revealed space).
    if alt_orbit > p.clear_end {
        fly.land_armed = true;
    }
    // Cover + camera zoom both come from the shared transition layer: cover
    // follows altitude above the nearest body (clouds where there's air, a bare
    // fade where there's none), and `flight_view_h` frames the body as a disc
    // that GROWS as you approach and shrinks as you climb — continuous through
    // the swap, no snap (see `transition::flight_view_h`). The camera applies
    // the user pinch multiplier on top.
    trans.drive(TransitionMode::Space, body, alt_orbit);

    // Land once armed and descended back into the covered swap band (swap_down,
    // where cover==1 → the zoom is continuous into the surface Descent frame).
    if !(fly.land_armed && alt_orbit < p.swap_down) {
        return;
    }
    let Ok((mut sim, mut visual)) = player.get_single_mut() else {
        return;
    };
    teleport_to_body(body, &mut region, &mut ctx, &mut planet, &mut sim, &mut visual);
    fly.body = body;
    // Continuous with the orbit altitude we just had — no jump entering Descend.
    fly.alt = alt_orbit;
    fly.phase = Phase::Descend;
    eprintln!("ha-creative: descent onto {body:?}");
}

/// DESCEND (player-driven, symmetric with Climb): default gentle fall so
/// landing feels natural without holding anything, USE speeds the fall — but
/// JUMP is a REAL control, not just a brake: hold it and you climb back out,
/// same as Climb, handing back to `Phase::Orbit` if you cross `swap_up` again.
/// (Used to only arrest the fall at vy=0 — "controls stop working" until you
/// touched down. Now you can change your mind at any altitude.) Cover rolls
/// back in through the atmosphere and clears at the ground.
fn creative_fly_descend(
    time: Res<Time>,
    actions: Res<TouchActions>,
    clock: Res<SolarSystemClock>,
    mut fly: ResMut<CreativeFlight>,
    mut trans: ResMut<FlightTransition>,
    mut region: ResMut<RegionRuntime>,
    mut ctx: ResMut<WorldContextRuntime>,
) {
    if fly.phase != Phase::Descend {
        return;
    }
    let dt = time.delta_secs();
    // Default gentle fall; hold USE to speed it, JUMP to climb back out.
    let mut vy = -VERT_SPEED * 0.6;
    if actions.action {
        vy = -VERT_SPEED;
    }
    if actions.jump {
        vy = VERT_SPEED;
    }
    fly.alt = (fly.alt + vy * dt).max(0.0);

    if fly.alt <= 0.5 && vy < 0.0 {
        fly.phase = Phase::Ground;
        fly.alt = 0.0;
        trans.clear();
        eprintln!("ha-creative: touchdown on {:?}", fly.body);
        return;
    }

    trans.drive(TransitionMode::Descent, fly.body, fly.alt);

    let p = transition_profile(fly.body);
    if fly.alt >= p.swap_up {
        // Changed your mind mid-descent: same continuous hand-back Climb uses.
        let c = Vec3::from_array(body_center_at(fly.body, clock.t));
        fly.pos = c + radial_up(fly.body, clock.t) * (body_radius(fly.body) + fly.alt);
        fly.pos.y = 0.0;
        fly.phase = Phase::Orbit;
        region.active = RegionId::Space;
        ctx.active = WorldContext::Space;
        eprintln!("ha-creative: aborted descent, back to orbit ({:?})", fly.body);
    }
}
