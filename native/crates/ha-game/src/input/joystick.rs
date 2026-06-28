use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;
use super::actions::{self, TouchActions};
use super::layout::{self, TouchLayout, TouchBlockRects};
use super::look_zone_ui;
#[cfg(not(target_os = "android"))]
use super::desktop_input;
use crate::camera::{camera_relative_move, look_forward_flat, FP_MAX_PITCH, FP_MIN_PITCH};
use crate::input::calibration::{
    adaptive_deadzone_norm, apply_stick_curve_vec, apply_stick_deadzone,
};
use crate::game_ui::UiGameplayBlock;
use crate::ui_editor::{UiEditable, UiEditorState, UiLayoutConfig};
use crate::ui_theme;

#[derive(Resource, Clone, Copy, Default)]
pub struct MovementIntent {
    pub move_x: f32,
    pub move_z: f32,
    pub aim_x: f32,
    pub aim_z: f32,
    pub sprint: bool,
}

#[derive(Resource, Clone, Copy)]
pub struct CameraLookIntent {
    pub yaw: f32,
    pub pitch: f32,
}

impl Default for CameraLookIntent {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
        }
    }
}

#[derive(Resource, Clone, Copy, Default)]
struct RawMovementIntent {
    move_x: f32,
    move_z: f32,
    aim_x: f32,
    aim_z: f32,
    sprint: bool,
}

#[derive(Clone, Copy, Default)]
struct JoystickSide {
    active: bool,
    pointer_id: Option<u64>,
    anchor: Vec2,
    current: Vec2,
}

#[derive(Resource, Default)]
struct DualJoystickState {
    move_stick: JoystickSide,
}

const DEADZONE: f32 = 0.07; // legacy fallback; runtime uses adaptive_deadzone_norm
const INPUT_SMOOTH_RATE: f32 = 42.0;
const SPRINT_ENTER_RADIUS: f32 = 0.92;
const WALK_STICK_CAP: f32 = 0.78;
const SPRINT_EXIT_RADIUS: f32 = 0.75;
const DEFAULT_AIM: Vec2 = Vec2::new(0.0, 1.0);
const LOOK_SENS_X: f32 = 0.0055;
const LOOK_SENS_Y: f32 = 0.0044;

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MovementIntent>()
            .init_resource::<CameraLookIntent>()
            .init_resource::<RawMovementIntent>()
            .init_resource::<DualJoystickState>()
            .init_resource::<TouchLayout>()
            .init_resource::<TouchBlockRects>()
            .init_resource::<TouchActions>()
            .init_resource::<actions::CraftIntent>()
            .init_resource::<actions::ActionInputBuffer>()
            .init_resource::<actions::ActionButtonRects>()
            .init_resource::<actions::ActionTouchState>()
            .init_resource::<actions::ActionButtonsSpawned>()
            .init_resource::<look_zone_ui::FpCrosshairSpawned>()
            .add_systems(Startup, (spawn_joystick_ui, actions::spawn_action_ui));
        #[cfg(not(target_os = "android"))]
        app.init_resource::<desktop_input::DesktopMouseCapture>()
            .add_systems(
                Update,
                (
                    desktop_input::desktop_keyboard_actions.after(actions::poll_action_buttons_held),
                    desktop_input::desktop_mouse_capture_toggle,
                    desktop_input::desktop_mouse_look,
                    desktop_input::desktop_cursor_grab,
                ),
            );
        app.add_systems(
            Update,
            (
                    layout::sync_touch_layout,
                    look_zone_ui::ensure_fp_crosshair.after(crate::game_ui::sync_game_ui_root),
                    look_zone_ui::sync_fp_crosshair,
                    sync_stick_anchors,
                    look_zone_ui::update_fp_crosshair,
                    actions::ensure_action_buttons,
                    actions::layout_action_buttons,
                    actions::poll_action_buttons_held,
                    actions::update_jump_button_label,
                    keyboard_input,
                    touch_move_stick.after(actions::poll_action_buttons_held),
                    smooth_movement_intent,
                    update_joystick_ui,
                )
                    .chain(),
            );
    }
}

fn sync_stick_anchors(
    layout: Res<TouchLayout>,
    config: Res<UiLayoutConfig>,
    mut state: ResMut<DualJoystickState>,
) {
    let slot = config.slot(UiEditable::MoveStick);
    let size = Vec2::splat(layout.radius * 2.0 * slot.scale);
    state.move_stick.anchor = config.center(&layout, UiEditable::MoveStick, size);
}

fn apply_deadzone(v: Vec2, deadzone_norm: f32) -> Vec2 {
    apply_stick_deadzone(v, deadzone_norm)
}

fn stick_vector(side: &JoystickSide, radius: f32, curve_exponent: f32) -> Vec2 {
    if !side.active {
        return Vec2::ZERO;
    }
    let delta = side.current - side.anchor;
    let clamped = if delta.length() > radius {
        delta.normalize() * radius
    } else {
        delta
    };
    let deadzone = adaptive_deadzone_norm(radius);
    let normalized = apply_deadzone(Vec2::new(clamped.x / radius, clamped.y / radius), deadzone);
    apply_stick_curve_vec(normalized, curve_exponent)
}

fn clamped_delta(side: &JoystickSide, radius: f32) -> Vec2 {
    if !side.active {
        return Vec2::ZERO;
    }
    let delta = side.current - side.anchor;
    if delta.length() > radius {
        delta.normalize() * radius
    } else {
        delta
    }
}

fn keyboard_input(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<DualJoystickState>,
    mut look: ResMut<CameraLookIntent>,
    mut raw: ResMut<RawMovementIntent>,
) {
    let mut key_move_x = 0.0f32;
    let mut key_move_z = 0.0f32;
    if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
        key_move_x -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
        key_move_x += 1.0;
    }
    if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
        key_move_z -= 1.0;
    }
    if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
        key_move_z += 1.0;
    }
    let key_move_len = (key_move_x * key_move_x + key_move_z * key_move_z).sqrt();
    if key_move_len > 1.0 {
        key_move_x /= key_move_len;
        key_move_z /= key_move_len;
    }
    let _keyboard_move_active = key_move_len > 0.01;

    if !state.move_stick.active {
        raw.move_x = key_move_x;
        raw.move_z = key_move_z;
        raw.sprint = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    }

    // I/J/K/L look fallback — touch builds only (desktop uses mouse + I opens inventory).
    if crate::platform::uses_touch_gameplay_ui() {
        if keys.pressed(KeyCode::KeyI) {
            look.pitch = (look.pitch + time.delta_secs() * 1.4).clamp(FP_MIN_PITCH, FP_MAX_PITCH);
        }
        if keys.pressed(KeyCode::KeyK) {
            look.pitch = (look.pitch - time.delta_secs() * 1.4).clamp(FP_MIN_PITCH, FP_MAX_PITCH);
        }
        if keys.pressed(KeyCode::KeyJ) {
            look.yaw += time.delta_secs() * 2.2;
        }
        if keys.pressed(KeyCode::KeyL) {
            look.yaw -= time.delta_secs() * 2.2;
        }
    }

    let flat = look_forward_flat(&*look);
    raw.aim_x = flat.x;
    raw.aim_z = flat.y;

    if keys.just_pressed(KeyCode::KeyE) || keys.just_pressed(KeyCode::Space) {
        // Desktop interact — one-shot handled via TouchActions pattern in future.
    }
}

fn touch_move_stick(
    layout: Res<TouchLayout>,
    config: Res<UiLayoutConfig>,
    blocks: Res<TouchBlockRects>,
    button_rects: Res<actions::ActionButtonRects>,
    editor: Res<UiEditorState>,
    gameplay_block: Res<UiGameplayBlock>,
    mut touches: EventReader<TouchInput>,
    mut state: ResMut<DualJoystickState>,
    look: Res<CameraLookIntent>,
    mut raw: ResMut<RawMovementIntent>,
) {
    // Desktop: WASD via `keyboard_input` only — never zero stick output over keys.
    if !crate::platform::uses_touch_gameplay_ui() {
        return;
    }
    if editor.enabled || editor.menu_open || gameplay_block.blocked {
        return;
    }
    let r = (layout.radius * config.slot(UiEditable::MoveStick).scale).max(1.0);
    let move_sens = config.calibration.movement_sensitivity;
    let curve = config.calibration.stick_curve_exponent;

    for ev in touches.read() {
        let pos = ev.position;
        match ev.phase {
            TouchPhase::Started => {
                if actions::touch_hits_action_button(pos, &button_rects) {
                    continue;
                }
                if layout::touch_in_move_zone(pos, &layout)
                    && !layout::touch_blocked(pos, &blocks.0)
                    && !state.move_stick.active
                {
                    state.move_stick.active = true;
                    state.move_stick.pointer_id = Some(ev.id);
                    state.move_stick.current = pos;
                }
            }
            TouchPhase::Moved => {
                if state.move_stick.active && state.move_stick.pointer_id == Some(ev.id) {
                    state.move_stick.current = pos;
                }
            }
            TouchPhase::Ended | TouchPhase::Canceled => {
                if state.move_stick.pointer_id == Some(ev.id) {
                    state.move_stick.active = false;
                    state.move_stick.pointer_id = None;
                    state.move_stick.current = state.move_stick.anchor;
                    raw.move_x = 0.0;
                    raw.move_z = 0.0;
                    raw.sprint = false;
                }
            }
        }
    }

    let move_vec = stick_vector(&state.move_stick, r, curve);
    let mut mx = move_vec.x * move_sens;
    let mut mz = move_vec.y * move_sens;
    if state.move_stick.active {
        let sprint_ratio = clamped_delta(&state.move_stick, r).length() / r;
        raw.sprint = if raw.sprint {
            sprint_ratio > SPRINT_EXIT_RADIUS
        } else {
            sprint_ratio > SPRINT_ENTER_RADIUS
        };
        if !raw.sprint {
            let len = (mx * mx + mz * mz).sqrt();
            if len > WALK_STICK_CAP {
                let s = WALK_STICK_CAP / len;
                mx *= s;
                mz *= s;
            }
        }
        raw.move_x = mx;
        raw.move_z = mz;
    }

    let camera_aim = look_forward_flat(&look);
    raw.aim_x = camera_aim.x;
    raw.aim_z = camera_aim.y;
}

fn smooth_movement_intent(
    time: Res<Time<Virtual>>,
    config: Res<UiLayoutConfig>,
    look: Res<CameraLookIntent>,
    raw: Res<RawMovementIntent>,
    mut intent: ResMut<MovementIntent>,
) {
    let k = 1.0 - (-INPUT_SMOOTH_RATE * time.delta_secs()).exp();
    let (wx, wz) = camera_relative_move(raw.move_x, raw.move_z, look.yaw);
    intent.move_x = intent.move_x.lerp(wx, k);
    intent.move_z = intent.move_z.lerp(wz, k);
    intent.aim_x = intent.aim_x.lerp(raw.aim_x, k);
    intent.aim_z = intent.aim_z.lerp(raw.aim_z, k);
    intent.sprint = raw.sprint;
}

#[derive(Component)]
struct JoystickKnob;

#[derive(Component)]
struct JoystickBase;

#[derive(Component)]
struct MoveStick;

fn spawn_move_stick(commands: &mut Commands) {
    let mut entity = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            display: Display::Flex,
            border: UiRect::all(Val::Px(3.2)),
            ..default()
        },
        BorderColor(ui_theme::joystick_border(false)),
        BackgroundColor(ui_theme::joystick_base()),
        BorderRadius::MAX,
        JoystickBase,
    ));
    entity.insert(MoveStick);
    entity.with_children(|p| {
        p.spawn((
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            BackgroundColor(ui_theme::joystick_knob()),
            BorderRadius::MAX,
            JoystickKnob,
            MoveStick,
        ));
    });
}

fn spawn_joystick_ui(mut commands: Commands) {
    if !crate::platform::uses_touch_gameplay_ui() {
        return;
    }
    spawn_move_stick(&mut commands);
}

fn update_joystick_ui(
    layout: Res<TouchLayout>,
    config: Res<UiLayoutConfig>,
    state: Res<DualJoystickState>,
    mut move_bases: Query<
        (&mut Node, &mut BorderColor),
        (
            With<JoystickBase>,
            With<MoveStick>,
            Without<JoystickKnob>,
        ),
    >,
    mut move_knobs: Query<
        &mut Node,
        (
            With<JoystickKnob>,
            With<MoveStick>,
            Without<JoystickBase>,
        ),
    >,
) {
    if !crate::platform::uses_touch_gameplay_ui() {
        return;
    }
    let slot = config.slot(UiEditable::MoveStick);
    let r = (layout.radius * slot.scale).max(1.0);
    let knob_r = (layout.knob_radius * slot.scale).max(8.0);
    let win_h = layout.height;

    for (mut node, mut border) in &mut move_bases {
        paint_base(
            &state.move_stick,
            r,
            win_h,
            &layout,
            true,
            &mut node,
            &mut border,
        );
    }
    for mut node in &mut move_knobs {
        paint_knob(&state.move_stick, r, knob_r, &mut node);
    }
}

fn paint_base(
    side: &JoystickSide,
    r: f32,
    win_h: f32,
    layout: &TouchLayout,
    _is_move: bool,
    node: &mut Node,
    border: &mut BorderColor,
) {
    node.display = Display::Flex;
    let (left, bottom) = layout::base_ui_pos(side.anchor, r, win_h);
    node.left = Val::Px(left);
    node.bottom = Val::Px(bottom);
    node.width = Val::Px(r * 2.0);
    node.height = Val::Px(r * 2.0);
    border.0 = ui_theme::joystick_border(side.active);
    let _ = layout;
}

fn paint_knob(side: &JoystickSide, r: f32, knob_r: f32, node: &mut Node) {
    let delta = clamped_delta(side, r);
    let (left, bottom) = layout::knob_offset_in_base(delta, r, knob_r);
    node.width = Val::Px(knob_r * 2.0);
    node.height = Val::Px(knob_r * 2.0);
    node.left = Val::Px(left);
    node.bottom = Val::Px(bottom);
}
