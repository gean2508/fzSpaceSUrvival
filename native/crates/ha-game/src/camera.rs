//! 2D follow camera — surface view (Terraria zoom) and SFS-style flight zoom.

use bevy::input::mouse::MouseWheel;
use bevy::input::touch::Touches;
use bevy::prelude::*;
use bevy::render::camera::ScalingMode;

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
/// User zoom clamps (viewport height, world m): from tight to whole-SYSTEM (the
/// max frames the Sun + planets + Moon; Neptune's orbit is ~630 km from the Sun).
const ZOOM_MIN_H: f32 = 18.0;
const ZOOM_MAX_H: f32 = 40_000_000.0;
/// On-foot pinch clamps: personal close-up ↔ panorama. The max stays inside the
/// streamed terrain ring (CHUNK_RANGE=3 → ~112 m wide) even on 19.5:9 phones.
const SURFACE_ZOOM_MIN_H: f32 = 12.0;
const SURFACE_ZOOM_MAX_H: f32 = 45.0;
/// On-foot pinch gesture: both fingers in the TOP band of the screen (above the
/// stick zones, `layout.zone_top`). While active the joysticks force-release
/// these pointer ids so the pinch never fights the twin-stick (see
/// `input/joystick.rs`). `ids` = the two participating touch ids.
#[derive(Resource, Default)]
pub struct PinchActive {
    pub ids: Option<(u64, u64)>,
}

/// User-controlled camera zoom (pinch on mobile, scroll on desktop). The camera
/// ALWAYS stays centred on the rocket/player — only the zoom is user-driven.
#[derive(Resource)]
pub struct CameraZoom {
    /// Desired viewport height in world metres — the ABSOLUTE user zoom, used on
    /// foot and (as the live value) whatever the projection lerps toward.
    pub(crate) view_h: f32,
    /// Encuadre REALMENTE en pantalla este frame (la proyección persigue a
    /// `view_h` con un lerp). Es el que deben mirar los sistemas que deciden
    /// qué nivel de detalle dibujar: si usaran el objetivo, el terreno se
    /// desvanecería antes de que la cámara llegue a alejarse.
    pub(crate) live_view_h: f32,
    /// Encuadre que pide la ALTURA, antes del pellizco del usuario. El sprite
    /// del personaje se dimensiona con éste, no con el vivo: así hacer zoom te
    /// ACERCA al personaje (crece en pantalla) en vez de dejarlo del mismo
    /// tamaño mientras el mundo cambia de escala — que era lo que hacía sentir
    /// que el zoom te "bajaba" del vuelo.
    pub(crate) auto_view_h: f32,
    /// While a flight transition owns the zoom (the altitude-driven reveal), the
    /// user's scroll/pinch adjusts THIS multiplier on top of the automatic zoom
    /// instead of the absolute `view_h` — so you can frame the ascent/orbit in or
    /// out without ever losing (or fighting) the auto-reveal. Reset to 1 on
    /// takeoff/landing.
    pub(crate) user_zoom_mult: f32,
    /// Previous two-finger distance (px) for pinch deltas.
    pinch_prev: Option<f32>,
}

impl Default for CameraZoom {
    fn default() -> Self {
        Self {
            view_h: SURFACE_VIEW_H,
            live_view_h: SURFACE_VIEW_H,
            auto_view_h: SURFACE_VIEW_H,
            user_zoom_mult: 1.0,
            pinch_prev: None,
        }
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

/// Cuánto se adelanta la cámara hacia el cuerpo al que te estás aproximando, en
/// metros de mundo. Lo escribe `creative_fly` (que es quien sabe hacia dónde
/// vas); la cámara sólo lo aplica, acotado a un cuarto de pantalla.
#[derive(Resource, Default)]
pub struct ApproachFraming {
    pub lead: Vec2,
}

pub struct FollowCameraPlugin;

impl Plugin for FollowCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraTrauma>()
            .init_resource::<ApproachFraming>()
            .init_resource::<CameraZoom>()
            .init_resource::<PinchActive>()
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

/// Zoom multiplier clamps while a flight owns the base zoom (the auto-reveal).
/// The user can pull the framing ~7× in or out on top of the automatic zoom
/// without ever losing it.
const FLY_MULT_MIN: f32 = 0.15;
const FLY_MULT_MAX: f32 = 8.0;

/// Pinch (mobile) / scroll (desktop) adjust the zoom; the camera stays centred on
/// the target and NEVER moves the player. On foot this drives the ABSOLUTE
/// `view_h`; while flying (the transition owns the altitude-driven auto-zoom) it
/// drives the `user_zoom_mult` MULTIPLIER on top — so the ascent still recedes
/// the world on its own (the "rising" feel), yet the user is never blocked from
/// framing in/out. Zoom only ever changes the viewport, never the character.
fn camera_zoom_input(
    region: Res<RegionRuntime>,
    fly: Res<crate::creative_fly::CreativeFlight>,
    trans: Res<crate::transition::FlightTransition>,
    layout: Res<crate::input::TouchLayout>,
    editor: Res<crate::ui_editor::UiEditorState>,
    mut pinch: ResMut<PinchActive>,
    mut zoom: ResMut<CameraZoom>,
    mut wheel: EventReader<MouseWheel>,
    touches: Res<Touches>,
) {
    let mut scroll = 0.0;
    for e in wheel.read() {
        scroll += e.y;
    }
    if editor.menu_open {
        pinch.ids = None;
        zoom.pinch_prev = None;
        return;
    }
    // Flying = a transition owns the base zoom (auto-reveal) OR we're in the
    // orbital frame. Then scroll/pinch tune the MULTIPLIER; on foot they set the
    // absolute `view_h`.
    let flying_view = fly.flying() || trans.active() || matches!(region.active, RegionId::Space);

    if flying_view {
        // Any two fingers pinch here (the left stick steers flight, so there's
        // no twin-stick conflict to gate against like on foot).
        pinch.ids = None;
        if trans.view_h.is_some() {
            // A flight transition is driving the base zoom continuously from
            // altitude (the ascent/orbit/descent auto-reveal — see
            // `flight_view_h`). Pinch/scroll adjust the MULTIPLIER on top so
            // the world keeps receding/approaching on its own; the user only
            // ever fine-tunes the framing, never fights or overrides the auto
            // zoom (that fight caused visible per-frame snapping before).
            if scroll.abs() > 0.0 {
                zoom.user_zoom_mult =
                    (zoom.user_zoom_mult * (1.0 - scroll * 0.12)).clamp(FLY_MULT_MIN, FLY_MULT_MAX);
            }
            let pts: Vec<Vec2> = touches.iter().map(|t| t.position()).collect();
            if pts.len() == 2 {
                let dist = pts[0].distance(pts[1]).max(1.0);
                if let Some(prev) = zoom.pinch_prev {
                    if (dist - prev).abs() > 1.5 {
                        let ratio = (prev / dist).clamp(0.5, 2.0);
                        zoom.user_zoom_mult =
                            (zoom.user_zoom_mult * ratio).clamp(FLY_MULT_MIN, FLY_MULT_MAX);
                    }
                }
                zoom.pinch_prev = Some(dist);
            } else {
                zoom.pinch_prev = None;
            }
        } else {
            // No transition running (teleported into Space without a flight,
            // e.g. a menu/nav jump) — plain absolute zoom, wide clamp.
            if scroll.abs() > 0.0 {
                zoom.view_h = (zoom.view_h * (1.0 - scroll * 0.12)).clamp(ZOOM_MIN_H, ZOOM_MAX_H);
            }
            let pts: Vec<Vec2> = touches.iter().map(|t| t.position()).collect();
            if pts.len() == 2 {
                let dist = pts[0].distance(pts[1]).max(1.0);
                if let Some(prev) = zoom.pinch_prev {
                    if (dist - prev).abs() > 1.5 {
                        let ratio = (prev / dist).clamp(0.5, 2.0);
                        zoom.view_h = (zoom.view_h * ratio).clamp(ZOOM_MIN_H, ZOOM_MAX_H);
                    }
                }
                zoom.pinch_prev = Some(dist);
            } else {
                zoom.pinch_prev = None;
            }
        }
    } else {
        if scroll.abs() > 0.0 {
            // Wheel up = zoom in (smaller viewport).
            zoom.view_h = (zoom.view_h * (1.0 - scroll * 0.12))
                .clamp(SURFACE_ZOOM_MIN_H, SURFACE_ZOOM_MAX_H);
        }
        // On foot: pellizco SÓLO cuando AMBOS dedos están en la banda superior
        // (arriba de las zonas de stick) — así el twin-stick de abajo nunca se
        // malinterpreta como pinch. `input/joystick.rs` consulta `PinchActive`.
        let top_band = layout.zone_top.max(1.0);
        let pts: Vec<(u64, Vec2)> = touches
            .iter()
            .map(|t| (t.id(), t.position()))
            .filter(|(_, p)| p.y < top_band)
            .collect();
        if pts.len() == 2 {
            let dist = pts[0].1.distance(pts[1].1).max(1.0);
            if pinch.ids == Some((pts[0].0, pts[1].0)) {
                if let Some(prev) = zoom.pinch_prev {
                    if (dist - prev).abs() > 1.5 {
                        let ratio = (prev / dist).clamp(0.5, 2.0);
                        zoom.view_h = (zoom.view_h * ratio)
                            .clamp(SURFACE_ZOOM_MIN_H, SURFACE_ZOOM_MAX_H);
                    }
                }
            }
            pinch.ids = Some((pts[0].0, pts[1].0));
            zoom.pinch_prev = Some(dist);
        } else {
            pinch.ids = None;
            zoom.pinch_prev = None;
        }
    }
}

#[allow(clippy::type_complexity)]
fn follow_player_camera(
    time: Res<Time>,
    fly: Res<crate::creative_fly::CreativeFlight>,
    trans: Res<crate::transition::FlightTransition>,
    region: Res<RegionRuntime>,
    trauma: Res<CameraTrauma>,
    approach: Res<ApproachFraming>,
    mut zoom: ResMut<CameraZoom>,
    mut was_flying: Local<bool>,
    player: Query<&Transform, (With<Player>, Without<GameCamera>)>,
    mut camera: Query<(&mut Transform, &mut OrthographicProjection), With<GameCamera>>,
) {
    let Ok((mut cam_tf, mut proj)) = camera.get_single_mut() else {
        return;
    };

    let flying = fly.flying() || trans.active() || matches!(region.active, RegionId::Space);

    // On entering/leaving a flight, reset the user multiplier so each new
    // flight starts at the automatic framing; on touchdown also pull the
    // absolute zoom back into the surface range for the next time we're on
    // foot (a pinched-far-out space view must not leak into walking).
    if flying != *was_flying {
        zoom.user_zoom_mult = 1.0;
        if !flying {
            zoom.view_h = zoom.view_h.clamp(SURFACE_ZOOM_MIN_H, SURFACE_ZOOM_MAX_H);
        }
        *was_flying = flying;
    }

    // ── ZOOM ───────────────────────────────────────────────────────────────
    // While a flight transition is running it OWNS the base zoom — ONE
    // continuous function of altitude (`flight_view_h`) shared by ascent,
    // orbit and descent, on every body. That is the "rising" feel: the ground
    // recedes as you climb, the cloud cover closes over the swap, and the
    // camera keeps zooming out into the orbital disc-frame that reveals the
    // planet (and any other bodies/orbit rings in view) — then reverses
    // symmetrically on approach/landing. The user's pinch/scroll only tunes a
    // multiplier on top (see `camera_zoom_input`), never overrides it. With no
    // transition running (on foot, or teleported into Space without a flight)
    // the zoom is the plain user-controlled `view_h`.
    let desired_h = if let Some(h) = trans.view_h {
        zoom.auto_view_h = h;
        (h * zoom.user_zoom_mult).clamp(ZOOM_MIN_H, ZOOM_MAX_H)
    } else {
        zoom.auto_view_h = zoom.view_h;
        let (min_h, max_h) = if flying {
            (ZOOM_MIN_H, ZOOM_MAX_H)
        } else {
            (SURFACE_ZOOM_MIN_H, SURFACE_ZOOM_MAX_H)
        };
        zoom.view_h.clamp(min_h, max_h)
    };
    zoom.view_h = desired_h;

    // ── FOLLOW ─────────────────────────────────────────────────────────────
    // SIEMPRE anclada al jugador. En el marco único (`universe.rs`) el personaje
    // nunca cambia de coordenadas al despegar, así que no hay ningún caso en el
    // que la cámara deba mirar a otro sitio — y no puede volver a pasar que se
    // vaya a un punto donde el jugador no está.
    let Ok(p) = player.get_single() else {
        return;
    };
    let mut target = p.translation.truncate();

    // ENCUADRE DE APROXIMACIÓN: volando, la cámara se adelanta hacia el cuerpo al
    // que vas, hasta un cuarto de pantalla. Sin esto el planeta te queda SIEMPRE
    // a un costado y la aproximación se lee como dar vueltas alrededor de su
    // centro hasta que de golpe crece; con esto lo tenés delante, creciendo, y
    // entrás en él — que es lo que uno espera al volar hacia un mundo.
    if flying {
        let lead = approach.lead;
        if lead.length_squared() > 0.0 {
            let max_lead = desired_h * 0.25;
            target += lead.clamp_length_max(max_lead);
        }
    }

    let blend = (time.delta_secs() * 6.0).clamp(0.0, 1.0);
    let current = cam_tf.translation.truncate();
    let mut next = current.lerp(target, blend);
    // SNAP when essentially arrived — the lerp is asymptotic and would otherwise
    // micro-jitter the camera every frame (reads as the whole world trembling at
    // the huge orbital zoom). Threshold scales with zoom → sub-pixel at any scale.
    let snap_eps = (desired_h * 0.0004).max(0.001);
    if (target - next).length() <= snap_eps {
        next = target;
    }

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
    let mut next_h = current_h + (desired_h - current_h) * (time.delta_secs() * 4.0).clamp(0.0, 1.0);
    // Same snap for the zoom: stop the perpetual micro-rescale that shimmers text.
    if (desired_h - next_h).abs() <= (desired_h * 0.0004).max(0.001) {
        next_h = desired_h;
    }
    proj.scaling_mode = ScalingMode::FixedVertical {
        viewport_height: next_h,
    };
    zoom.live_view_h = next_h;
}
