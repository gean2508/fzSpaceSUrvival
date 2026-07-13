//! 2D follow camera — surface view (Terraria zoom) and SFS-style flight zoom.

use bevy::prelude::*;
use bevy::render::camera::ScalingMode;

use crate::rocket_flight::RocketFlight;
use crate::scene::{RegionId, RegionRuntime};
use crate::sim::Player;

/// Marker for the single in-game camera.
#[derive(Component)]
pub struct GameCamera;

/// Legacy first/third-person pitch clamps — kept for input compat (unused in 2D).
pub const FP_MIN_PITCH: f32 = -1.35;
pub const FP_MAX_PITCH: f32 = 1.35;

/// Viewport height (world metres) while walking on a surface.
const SURFACE_VIEW_H: f32 = 30.0;
/// Max viewport height (m) at high flight zoom-out (planets become circles).
const FLIGHT_VIEW_H_MAX: f32 = 9_000.0;

/// Facing direction on the 2D strip: rot 0 → +s, rot PI → -s.
pub fn facing_sign(rot: f32) -> f32 {
    if rot.abs() > std::f32::consts::FRAC_PI_2 {
        -1.0
    } else {
        1.0
    }
}

/// Legacy shim: flat aim from the look intent — fixed +s in 2D (aim is
/// pointer-driven now, see `targeting.rs`).
pub fn look_forward_flat(_look: &crate::input::CameraLookIntent) -> Vec2 {
    Vec2::new(1.0, 0.0)
}

/// Top-down: the stick maps straight to the ground plane (x = east, z = south).
pub fn camera_relative_move(mx: f32, mz: f32, _yaw: f32) -> (f32, f32) {
    (mx, mz)
}

/// Gamepad view-cycle plumbing (no-op in 2D, kept so bindings stay stable).
#[derive(Resource, Default)]
pub struct CameraCycleRequest(pub bool);

#[derive(Event)]
pub struct CameraShakeEvent {
    pub trauma: f32,
}

#[derive(Resource, Default)]
pub struct CameraTrauma(pub f32);

pub struct FollowCameraPlugin;

impl Plugin for FollowCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraCycleRequest>()
            .init_resource::<CameraTrauma>()
            .add_event::<CameraShakeEvent>()
            .add_systems(Startup, spawn_camera)
            .add_systems(Update, (absorb_shake_events, follow_player_camera).chain());
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        GameCamera,
        Camera2d,
        // Single camera: world AND UI (the old separate HUD camera re-drew
        // the whole 2D world as a mini overlay — and cost a second pass).
        bevy::ui::IsDefaultUiCamera,
        OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: SURFACE_VIEW_H,
            },
            near: -1000.0,
            far: 1000.0,
            ..OrthographicProjection::default_2d()
        },
        Transform::from_xyz(0.0, 0.0, 100.0),
    ));
}

fn absorb_shake_events(
    time: Res<Time>,
    mut events: EventReader<CameraShakeEvent>,
    mut trauma: ResMut<CameraTrauma>,
) {
    for e in events.read() {
        trauma.0 = (trauma.0 + e.trauma).min(1.0);
    }
    trauma.0 = (trauma.0 - time.delta_secs() * 1.8).max(0.0);
}

#[allow(clippy::type_complexity)]
fn follow_player_camera(
    time: Res<Time>,
    flight: Res<RocketFlight>,
    region: Res<RegionRuntime>,
    trauma: Res<CameraTrauma>,
    player: Query<&Transform, (With<Player>, Without<GameCamera>)>,
    mut camera: Query<(&mut Transform, &mut OrthographicProjection), With<GameCamera>>,
) {
    let Ok((mut cam_tf, mut proj)) = camera.get_single_mut() else {
        return;
    };

    let in_flight = flight.active && flight.phase != ha_shared::FlightPhase::Surface;
    let (target, view_h) = if in_flight || matches!(region.active, RegionId::Space) {
        // Flight: chase the rocket, zoom out with altitude (SFS feel).
        let alt = flight.altitude_m().max(0.0);
        let h = (SURFACE_VIEW_H + alt * 2.2).clamp(SURFACE_VIEW_H, FLIGHT_VIEW_H_MAX);
        (flight.render_pos_2d(), h)
    } else {
        let Ok(p) = player.get_single() else {
            return;
        };
        // Top-down: centre on the player (no look-ahead bias).
        (p.translation.truncate(), SURFACE_VIEW_H)
    };

    let blend = (time.delta_secs() * 6.0).clamp(0.0, 1.0);
    let current = cam_tf.translation.truncate();
    let mut next = current.lerp(target, blend);

    if trauma.0 > 0.0 {
        let t = time.elapsed_secs() * 40.0;
        let amp = trauma.0 * trauma.0 * 0.6;
        next += Vec2::new((t * 1.3).sin(), (t * 1.7).cos()) * amp;
    }
    cam_tf.translation = next.extend(100.0);

    let current_h = match proj.scaling_mode {
        ScalingMode::FixedVertical { viewport_height } => viewport_height,
        _ => SURFACE_VIEW_H,
    };
    let next_h = current_h + (view_h - current_h) * (time.delta_secs() * 3.0).clamp(0.0, 1.0);
    proj.scaling_mode = ScalingMode::FixedVertical {
        viewport_height: next_h,
    };
}
