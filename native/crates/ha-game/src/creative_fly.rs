//! Creative free-flight — the CHARACTER (not a rocket) flies through the orbital
//! view. In creative, JUMP off any surface lifts you into space as yourself;
//! move freely in 2D (no fuel, no gravity); fly to another body and touch it (or
//! press USE) to land. Landing reuses `teleport_to_body`, so the surface coords
//! are always correct — no risky inverse remap. Deliberately SEPARATE from the
//! `RocketFlight` god-node (biblia §9): this never ignites the rocket.

use bevy::prelude::*;
use ha_shared::solar_ephemeris::body_center_at;
use ha_shared::{body_radius, CelestialBody, WorldContext};

use crate::camera::CameraZoom;
use crate::input::{MovementIntent, TouchActions};
use crate::rocket_flight::{launch_pos_vel, RocketFlight};
use crate::scene::{teleport_to_body, RegionId, RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, PlayerVisual, SimState};
use crate::solar_system_clock::SolarSystemClock;
use crate::world2d::ActivePlanet;

/// Where the player-as-spacecraft is in the orbital world plane.
#[derive(Resource, Default)]
pub struct CreativeFlight {
    pub active: bool,
    pub pos: Vec3,
}

/// Fly speed = view_h × this (auto-scales with zoom: fast zoomed-out, precise in).
const FLY_SPEED_FACTOR: f32 = 0.55;
/// Land when the character comes this close to a body's surface (metres).
const LAND_MARGIN_M: f32 = 40.0;
/// Lift-off altitude above the surface so you don't instantly re-land (metres).
const LIFTOFF_ALT_M: f32 = 340.0;

pub struct CreativeFlyPlugin;

impl Plugin for CreativeFlyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CreativeFlight>().add_systems(
            Update,
            (creative_fly_enter, creative_fly_move, creative_fly_land).chain(),
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

/// JUMP off a surface (creative only) → lift the character into the orbital view.
fn creative_fly_enter(
    editor: Res<crate::ui_editor::UiEditorState>,
    actions: Res<TouchActions>,
    clock: Res<SolarSystemClock>,
    flight: Res<RocketFlight>,
    mut fly: ResMut<CreativeFlight>,
    mut region: ResMut<RegionRuntime>,
    mut ctx: ResMut<WorldContextRuntime>,
) {
    if !editor.creative || fly.active || flight.active || !actions.jump_edge {
        return;
    }
    let Some(body) = active_body(&region) else {
        return;
    };
    // Start LIFTOFF_ALT_M straight up from the body surface (radial), so the
    // proximity-land check below doesn't fire on the same frame.
    let (pad, _) = launch_pos_vel(body, clock.t, 0.0);
    let c = Vec3::from_array(body_center_at(body, clock.t));
    let dir = (pad - c).normalize_or_zero();
    fly.pos = c + dir * (body_radius(body) + LIFTOFF_ALT_M);
    fly.active = true;
    region.active = RegionId::Space;
    ctx.active = WorldContext::Space;
    eprintln!("ha-creative: character lift-off from {body:?}");
}

/// Free 2D movement in the orbital plane; speed scales with the zoom level.
fn creative_fly_move(
    time: Res<Time>,
    intent: Res<MovementIntent>,
    zoom: Res<CameraZoom>,
    mut fly: ResMut<CreativeFlight>,
) {
    if !fly.active {
        return;
    }
    let dt = time.delta_secs();
    let speed = (zoom.view_h * FLY_SPEED_FACTOR).max(120.0);
    // Screen-up = −z (render is `(x, −z)`), so +move_z (stick down) = +z.
    fly.pos.x += intent.move_x * speed * dt;
    fly.pos.z += intent.move_z * speed * dt;
    fly.pos.y = 0.0;
}

/// Land when the character reaches a body (proximity) or presses USE — reuses
/// `teleport_to_body` so the surface spawn is always valid.
#[allow(clippy::too_many_arguments)]
fn creative_fly_land(
    actions: Res<TouchActions>,
    clock: Res<SolarSystemClock>,
    mut fly: ResMut<CreativeFlight>,
    mut region: ResMut<RegionRuntime>,
    mut ctx: ResMut<WorldContextRuntime>,
    mut planet: ResMut<ActivePlanet>,
    mut player: Query<(&mut SimState, &mut PlayerVisual), With<Player>>,
) {
    if !fly.active {
        return;
    }
    // Nearest body + whether we're touching it.
    let mut nearest: Option<(CelestialBody, f32)> = None;
    for &body in CelestialBody::all() {
        let c = Vec3::from_array(body_center_at(body, clock.t));
        let d = fly.pos.distance(c);
        if nearest.map(|(_, nd)| d < nd).unwrap_or(true) {
            nearest = Some((body, d));
        }
    }
    let Some((body, dist)) = nearest else {
        return;
    };
    let touching = dist < body_radius(body) + LAND_MARGIN_M;
    // USE = "land now" on the nearest body (fly down to it).
    if !touching && !actions.use_edge {
        return;
    }
    let Ok((mut sim, mut visual)) = player.get_single_mut() else {
        return;
    };
    teleport_to_body(body, &mut region, &mut ctx, &mut planet, &mut sim, &mut visual);
    fly.active = false;
    eprintln!("ha-creative: landed on {body:?}");
}
