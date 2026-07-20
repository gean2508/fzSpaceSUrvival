//! 2D follow camera — surface view (Terraria zoom) and SFS-style flight zoom.

use bevy::input::mouse::MouseWheel;
use bevy::input::touch::Touches;
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
/// Default flight zoom — close enough to read the rocket + local horizon, far
/// enough to feel airborne. The player pinches/scrolls out to see the whole body.
const FLIGHT_DEFAULT_H: f32 = 320.0;
/// User zoom clamps (viewport height, world m): from tight to whole-SYSTEM (the
/// max frames the Sun + planets + Moon; Neptune's orbit is ~630 km from the Sun).
const ZOOM_MIN_H: f32 = 18.0;
const ZOOM_MAX_H: f32 = 2_000_000.0;

/// User-controlled camera zoom (pinch on mobile, scroll on desktop). The camera
/// ALWAYS stays centred on the rocket/player — only the zoom is user-driven.
#[derive(Resource)]
pub struct CameraZoom {
    /// Desired viewport height in world metres.
    pub(crate) view_h: f32,
    /// Previous two-finger distance (px) for pinch deltas.
    pinch_prev: Option<f32>,
    /// User multiplier on top of creative-flight's automatic orbital reveal
    /// zoom (`creative_fly_orbit`) — pinch/scroll adjust THIS instead of
    /// `view_h` directly while in orbit, so the auto-formula (which re-derives
    /// `view_h` from altitude every frame) doesn't fight and overwrite it.
    /// Default 1.0 = the automatic framing, untouched until the user zooms.
    pub(crate) user_zoom_mult: f32,
}

impl Default for CameraZoom {
    fn default() -> Self {
        Self { view_h: SURFACE_VIEW_H, pinch_prev: None, user_zoom_mult: 1.0 }
    }
}

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

#[derive(Event)]
pub struct CameraShakeEvent {
    pub trauma: f32,
}

#[derive(Resource, Default)]
pub struct CameraTrauma(pub f32);

pub struct FollowCameraPlugin;

impl Plugin for FollowCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraTrauma>()
            .init_resource::<CameraZoom>()
            .add_event::<CameraShakeEvent>()
            .add_systems(Startup, spawn_camera)
            .add_systems(
                Update,
                (absorb_shake_events, camera_zoom_input, follow_player_camera).chain(),
            );
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

/// Pinch (mobile) / scroll (desktop) adjust the user zoom; the camera stays
/// centred on the target. Pinch is gated to flight/space so the on-foot
/// twin-stick (two fingers) isn't misread as a pinch.
fn camera_zoom_input(
    flight: Res<RocketFlight>,
    region: Res<RegionRuntime>,
    fly: Res<crate::creative_fly::CreativeFlight>,
    trans: Res<crate::transition::FlightTransition>,
    mut zoom: ResMut<CameraZoom>,
    mut wheel: EventReader<MouseWheel>,
    touches: Res<Touches>,
) {
    use crate::transition::TransitionMode;
    let mut scroll = 0.0;
    for e in wheel.read() {
        scroll += e.y;
    }
    // While a flight transition owns the zoom, the surface frame (Ascent/
    // Descent) is fully automatic — skip user input there (letting the pinch
    // fight the per-frame auto-zoom caused the biblia §11 flicker). In the
    // orbital frame (Space — creative OR rocket), pinch/scroll adjust the
    // `user_zoom_mult` MULTIPLIER on top of the automatic reveal so the framing
    // is user-tunable without losing it.
    if trans.active() {
        match trans.mode {
            TransitionMode::Ascent | TransitionMode::Descent => {
                zoom.pinch_prev = None;
                return;
            }
            TransitionMode::Space => {
                if scroll.abs() > 0.0 {
                    zoom.user_zoom_mult =
                        (zoom.user_zoom_mult * (1.0 - scroll * 0.12)).clamp(0.2, 6.0);
                }
                let pts: Vec<Vec2> = touches.iter().map(|t| t.position()).collect();
                if pts.len() == 2 {
                    let dist = pts[0].distance(pts[1]).max(1.0);
                    if let Some(prev) = zoom.pinch_prev {
                        if (dist - prev).abs() > 1.5 {
                            let ratio = (prev / dist).clamp(0.5, 2.0);
                            zoom.user_zoom_mult =
                                (zoom.user_zoom_mult * ratio).clamp(0.2, 6.0);
                        }
                    }
                    zoom.pinch_prev = Some(dist);
                } else {
                    zoom.pinch_prev = None;
                }
                return;
            }
            TransitionMode::None => {}
        }
    }
    let _ = &fly;
    if scroll.abs() > 0.0 {
        // Wheel up = zoom in (smaller viewport).
        zoom.view_h = (zoom.view_h * (1.0 - scroll * 0.12)).clamp(ZOOM_MIN_H, ZOOM_MAX_H);
    }

    let space_view = (flight.active && flight.phase != ha_shared::FlightPhase::Surface)
        || matches!(region.active, RegionId::Space);
    if !space_view {
        zoom.pinch_prev = None;
        return;
    }
    let pts: Vec<Vec2> = touches.iter().map(|t| t.position()).collect();
    if pts.len() == 2 {
        let dist = pts[0].distance(pts[1]).max(1.0);
        if let Some(prev) = zoom.pinch_prev {
            if (dist - prev).abs() > 1.5 {
                // Fingers apart (dist up) → zoom in (view_h down).
                let ratio = (prev / dist).clamp(0.5, 2.0);
                zoom.view_h = (zoom.view_h * ratio).clamp(ZOOM_MIN_H, ZOOM_MAX_H);
            }
        }
        zoom.pinch_prev = Some(dist);
    } else {
        zoom.pinch_prev = None;
    }
}

#[allow(clippy::type_complexity)]
fn follow_player_camera(
    time: Res<Time>,
    flight: Res<RocketFlight>,
    fly: Res<crate::creative_fly::CreativeFlight>,
    trans: Res<crate::transition::FlightTransition>,
    region: Res<RegionRuntime>,
    trauma: Res<CameraTrauma>,
    mut zoom: ResMut<CameraZoom>,
    mut was_space: Local<bool>,
    player: Query<&Transform, (With<Player>, Without<GameCamera>)>,
    mut camera: Query<(&mut Transform, &mut OrthographicProjection), With<GameCamera>>,
) {
    let Ok((mut cam_tf, mut proj)) = camera.get_single_mut() else {
        return;
    };

    let in_flight = flight.active && flight.phase != ha_shared::FlightPhase::Surface;
    let space_view = in_flight || fly.in_orbit() || matches!(region.active, RegionId::Space);

    // While a flight is running the transition OWNS the zoom (one continuous
    // function of altitude — single writer, no pinch fights, no snaps). In the
    // orbital frame the user's pinch/scroll multiplier still applies on top so
    // they can pull the framing in/out without losing the automatic reveal.
    if let Some(h) = trans.view_h {
        let mult = if matches!(trans.mode, crate::transition::TransitionMode::Space) {
            zoom.user_zoom_mult
        } else {
            1.0
        };
        zoom.view_h = (h * mult).clamp(ZOOM_MIN_H, ZOOM_MAX_H);
    }

    // Reset the zoom to a sane default entering/leaving space WITHOUT a
    // transition (MENU teleports, nav jumps). A running transition keeps the
    // zoom continuous across the frame swap by construction.
    if !fly.flying() && !trans.active() {
        if space_view && !*was_space {
            zoom.view_h = FLIGHT_DEFAULT_H;
        } else if !space_view && *was_space {
            zoom.view_h = SURFACE_VIEW_H;
        }
    }
    *was_space = space_view;

    // The camera is ALWAYS centred on: the character flying in orbit (creative),
    // the rocket over its surface anchor (ascent/descent below the swap), the
    // rocket in the orbital frame, or the player.
    let target = if fly.in_orbit() {
        Vec2::new(fly.pos.x, -fly.pos.z)
    } else if in_flight && trans.in_surface_frame() {
        trans.surface_anchor
    } else if space_view {
        flight.render_pos_2d()
    } else {
        let Ok(p) = player.get_single() else {
            return;
        };
        p.translation.truncate()
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
    let next_h = current_h + (zoom.view_h - current_h) * (time.delta_secs() * 4.0).clamp(0.0, 1.0);
    proj.scaling_mode = ScalingMode::FixedVertical {
        viewport_height: next_h,
    };
}
