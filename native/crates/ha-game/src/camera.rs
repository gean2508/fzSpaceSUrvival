use bevy::prelude::*;
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::render::camera::ClearColorConfig;

use crate::floating_origin::FloatingOrigin;
use crate::input::CameraLookIntent;
use crate::rocket_flight::{flight_world_transform, RocketFlight};
use crate::scene::{RegionId, RegionRuntime, WorldContextRuntime};
use crate::view_target::ViewTarget;
use crate::materials::OutlineShell;
use crate::sim::{Player, SimState};
use ha_shared::FlightPhase;

use crate::debug::{span_record, span_start, CpuSpanSlot, CpuSpanStats};
use crate::surface_placement::on_sphere_context;

pub struct FollowCameraPlugin;

impl Plugin for FollowCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraTrauma>()
            .init_resource::<CameraViewMode>()
            .add_event::<CameraShakeEvent>()
            .add_systems(Update, absorb_shake_events)
            .add_systems(Update, decay_camera_trauma)
            .add_systems(Startup, spawn_camera)
            .add_systems(
                Update,
                (
                    cycle_camera_view_mode,
                    sync_player_mesh_for_view,
                    lock_game_camera_post,
                    follow_player_camera,
                )
                    .chain(),
            );
    }
}

/// Minecraft-style F5 cycle — debug whether the navy overlay is camera/clear vs world.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraViewMode(pub CameraPerspective);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraPerspective {
    FirstPerson,
    /// Over-the-shoulder — see terrain + player capsule.
    ThirdPersonBack,
    /// Selfie — camera in front looking at the player (debug).
    ThirdPersonFront,
}

impl Default for CameraViewMode {
    fn default() -> Self {
        Self(CameraPerspective::FirstPerson)
    }
}

impl CameraViewMode {
    pub fn label(self) -> &'static str {
        match self.0 {
            CameraPerspective::FirstPerson => "1st person",
            CameraPerspective::ThirdPersonBack => "3rd back",
            CameraPerspective::ThirdPersonFront => "3rd front",
        }
    }

    fn cycle(self) -> Self {
        Self(match self.0 {
            CameraPerspective::FirstPerson => CameraPerspective::ThirdPersonBack,
            CameraPerspective::ThirdPersonBack => CameraPerspective::ThirdPersonFront,
            CameraPerspective::ThirdPersonFront => CameraPerspective::FirstPerson,
        })
    }
}

const TP_DISTANCE: f32 = 4.8;
const TP_EYE_LIFT: f32 = 0.55;
const TP_CAM_LIFT: f32 = 1.15;
/// Nudge FP eye forward so we never sit inside the inverted-hull outline shell.
const FP_EYE_FORWARD: f32 = 0.12;

#[derive(Component)]
pub(crate) struct GameCamera;

/// Minecraft-style vertical look clamp (~±89°).
pub const FP_MIN_PITCH: f32 = -std::f32::consts::FRAC_PI_2 + 0.05;
pub const FP_MAX_PITCH: f32 = std::f32::consts::FRAC_PI_2 - 0.05;
pub const FP_FOV_DEG: f32 = 72.0;
use ha_shared::{capsule_center_above_feet, eye_height_above_feet, WorldContext};

/// Clip distances — Android overworld was 220 m (moon/earth globes sit 300–800 m out).
pub fn camera_far_for_region(region: &RegionId) -> f32 {
    match region {
        RegionId::Space | RegionId::Transit => 1_400.0,
        RegionId::Luna | RegionId::Planet(_) => 900.0,
        RegionId::Cave(_) => 200.0,
        RegionId::Overworld { .. } => {
            #[cfg(target_os = "android")]
            {
                360.0
            }
            #[cfg(not(target_os = "android"))]
            {
                560.0
            }
        }
    }
}

pub fn camera_near() -> f32 {
    0.05
}

#[derive(Event, Clone, Copy)]
pub struct CameraShakeEvent {
    pub intensity: f32,
}

#[derive(Resource)]
struct CameraTrauma {
    value: f32,
    decay_per_sec: f32,
    max_rot_deg: f32,
}

impl Default for CameraTrauma {
    fn default() -> Self {
        Self {
            value: 0.0,
            decay_per_sec: 1.35,
            max_rot_deg: 1.8,
        }
    }
}

/// World-space view direction from yaw + pitch (Bevy camera looks down local -Z).
pub fn look_forward(look: &CameraLookIntent) -> Vec3 {
    let rot = Quat::from_rotation_y(look.yaw) * Quat::from_rotation_x(look.pitch);
    rot * Vec3::NEG_Z
}

/// Horizontal forward for movement / body facing (ignores pitch).
pub fn look_forward_flat(look: &CameraLookIntent) -> Vec2 {
    Vec2::new(look.yaw.sin(), look.yaw.cos())
}

/// Transform stick input (strafe X, forward Z) into world XZ using camera yaw.
pub fn camera_relative_move(move_x: f32, move_z: f32, yaw: f32) -> (f32, f32) {
    let fwd = look_forward_flat(&CameraLookIntent { yaw, pitch: 0.0 });
    let right = Vec2::new(fwd.y, -fwd.x);
    let wx = right.x * move_x + fwd.x * move_z;
    let wz = right.y * move_x + fwd.y * move_z;
    (wx, wz)
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        // TonyMcMapface LUT compiles ~1s after boot on NVK and blanks unlit meshes (hdr off).
        Tonemapping::None,
        DebandDither::Disabled,
        Camera {
            order: 0,
            is_active: true,
            hdr: false,
            clear_color: ClearColorConfig::Custom(Color::srgb(
                ha_shared::Ps1Retro::SKY_ZENITH[0],
                ha_shared::Ps1Retro::SKY_ZENITH[1],
                ha_shared::Ps1Retro::SKY_ZENITH[2],
            )),
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov: FP_FOV_DEG.to_radians(),
            near: camera_near(),
            far: camera_far_for_region(&RegionId::Overworld { gx: 0, gz: 0 }),
            ..default()
        }),
        Transform::from_xyz(0.0, eye_height_above_feet(), 0.0),
        Msaa::Off,
        GameCamera,
        Name::new("FirstPersonCamera"),
    ));
}

/// Bevy 0.15 auto-inserts TonyMcMapface — keep post off for unlit procedural art.
fn lock_game_camera_post(mut cam: Query<(&mut Tonemapping, &mut DebandDither), With<GameCamera>>) {
    for (mut tonemap, mut deband) in &mut cam {
        *tonemap = Tonemapping::None;
        *deband = DebandDither::Disabled;
    }
}

fn cycle_camera_view_mode(
    keys: Res<ButtonInput<KeyCode>>,
    mut mode: ResMut<CameraViewMode>,
    clear: Res<ClearColor>,
) {
    if keys.just_pressed(KeyCode::F5) {
        *mode = mode.cycle();
        let c = clear.0.to_srgba();
        eprintln!(
            "ha-cam: view={} clear=({:.2},{:.2},{:.2})",
            mode.label(),
            c.red,
            c.green,
            c.blue
        );
    }
}

/// Hide local player mesh in 1st person — inverted-hull outline was filling the viewport.
fn sync_player_mesh_for_view(
    view: Res<CameraViewMode>,
    mut body: Query<&mut Visibility, (With<Player>, With<Mesh3d>, Without<OutlineShell>)>,
    mut outline: Query<&mut Visibility, With<OutlineShell>>,
) {
    let show_body = !matches!(view.0, CameraPerspective::FirstPerson);
    let vis = if show_body {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    for mut v in &mut body {
        *v = vis;
    }
    for mut v in &mut outline {
        *v = vis;
    }
}

fn follow_player_camera(
    time: Res<Time>,
    view_mode: Res<CameraViewMode>,
    trauma: Res<CameraTrauma>,
    look: Res<CameraLookIntent>,
    flight: Res<RocketFlight>,
    origin: Res<FloatingOrigin>,
    view_target: Res<ViewTarget>,
    region: Res<RegionRuntime>,
    world_ctx: Res<WorldContextRuntime>,
    player: Query<(&SimState, &Transform), (With<Player>, Without<GameCamera>)>,
    mut cam: Query<
        (&mut Transform, &mut Projection),
        (With<GameCamera>, Without<Player>),
    >,
    mut span: ResMut<CpuSpanStats>,
) {
    let t0 = span_start();
    let Ok((mut transform, mut projection)) = cam.get_single_mut() else {
        span_record(&mut span, CpuSpanSlot::Camera, t0);
        return;
    };

    if view_target.is_remote() {
        let eye = origin.to_render(view_target.observer_world);
        let rot = Quat::from_rotation_y(look.yaw) * Quat::from_rotation_x(look.pitch);
        transform.translation = eye;
        transform.rotation = rot;
        if let Projection::Perspective(ref mut persp) = *projection {
            persp.far = camera_far_for_region(&RegionId::Luna);
        }
        span_record(&mut span, CpuSpanSlot::Camera, t0);
        return;
    }

    if flight.active && flight.phase != FlightPhase::Surface {
        let (pos, rot) = flight_world_transform(&flight, &origin);
        let back = rot * Vec3::new(0.0, 0.65, 2.2);
        transform.translation = pos + back;
        transform.rotation = rot;
        if let Projection::Perspective(ref mut persp) = *projection {
            persp.far = 180_000.0;
        }
        span_record(&mut span, CpuSpanSlot::Camera, t0);
        return;
    }

    if let Projection::Perspective(ref mut persp) = *projection {
        persp.far = camera_far_for_region(&region.active);
        persp.near = camera_near();
    }

    let Ok((sim, player_transform)) = player.get_single() else {
        span_record(&mut span, CpuSpanSlot::Camera, t0);
        return;
    };

    let on_sphere = on_sphere_context(world_ctx.active);
    let eye_lift = eye_height_above_feet() - capsule_center_above_feet(sim.body.radius);
    let (player_eye, surface_up) = if on_sphere {
        let up = player_transform.rotation * Vec3::Y;
        (player_transform.translation + up * eye_lift, up)
    } else {
        let eye_y = sim.body.y + eye_height_above_feet();
        (
            Vec3::new(sim.body.x, eye_y, sim.body.z),
            Vec3::Y,
        )
    };
    let look_at = player_eye + surface_up * TP_EYE_LIFT;

    let mut rot = Quat::from_rotation_y(look.yaw) * Quat::from_rotation_x(look.pitch);
    let forward = rot * Vec3::NEG_Z;

    match view_mode.0 {
        CameraPerspective::FirstPerson => {
            transform.translation = player_eye + forward * FP_EYE_FORWARD;
        }
        CameraPerspective::ThirdPersonBack => {
            let cam_pos = player_eye - forward * TP_DISTANCE + surface_up * TP_CAM_LIFT;
            transform.translation = cam_pos;
            transform.look_at(look_at, surface_up);
            span_record(&mut span, CpuSpanSlot::Camera, t0);
            return;
        }
        CameraPerspective::ThirdPersonFront => {
            let cam_pos = player_eye + forward * TP_DISTANCE + surface_up * TP_CAM_LIFT;
            transform.translation = cam_pos;
            transform.look_at(look_at, surface_up);
            span_record(&mut span, CpuSpanSlot::Camera, t0);
            return;
        }
    }

    let shake_pow = trauma.value * trauma.value * trauma.value;
    if shake_pow > 0.0001 {
        let s = time.elapsed_secs();
        let n1 = (s * 47.0).sin() * 0.7 + (s * 23.0).cos() * 0.3;
        let n2 = (s * 39.0 + 1.7).sin() * 0.65 + (s * 19.0 + 0.8).cos() * 0.35;
        let rot_rad = (trauma.max_rot_deg.to_radians()) * shake_pow;
        rot *= Quat::from_rotation_z(n1 * rot_rad);
        rot *= Quat::from_rotation_x(n2 * rot_rad * 0.55);
    }

    transform.rotation = rot;
    span_record(&mut span, CpuSpanSlot::Camera, t0);
}

fn absorb_shake_events(
    mut events: EventReader<CameraShakeEvent>,
    mut trauma: ResMut<CameraTrauma>,
    mut span: ResMut<CpuSpanStats>,
) {
    let t0 = span_start();
    for ev in events.read() {
        trauma.value = (trauma.value + ev.intensity.clamp(0.0, 1.0)).clamp(0.0, 1.0);
    }
    span_record(&mut span, CpuSpanSlot::Camera, t0);
}

fn decay_camera_trauma(time: Res<Time>, mut trauma: ResMut<CameraTrauma>, mut span: ResMut<CpuSpanStats>) {
    let t0 = span_start();
    trauma.value = (trauma.value - trauma.decay_per_sec * time.delta_secs()).max(0.0);
    span_record(&mut span, CpuSpanSlot::Camera, t0);
}
