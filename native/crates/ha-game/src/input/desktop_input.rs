//! Desktop-only keyboard + mouse — Android touch paths unchanged.

use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, PrimaryWindow};

use super::actions::ActionInputBuffer;
use super::CameraLookIntent;
use crate::camera::{FP_MAX_PITCH, FP_MIN_PITCH};
use crate::game_ui::UiGameplayBlock;
use crate::ui_editor::UiEditorState;

const MOUSE_SENS_X: f32 = 0.0022;
const MOUSE_SENS_Y: f32 = 0.0018;

/// Left-click captures the cursor for look; Esc releases. Avoids `Locked` on Wayland/NVK
/// where the cursor disappears but motion events never arrive.
#[derive(Resource, Default)]
pub struct DesktopMouseCapture {
    pub active: bool,
}

/// WASD supplements `keyboard_input`; jump/use/ability via keyboard on PC.
pub fn desktop_keyboard_actions(
    keys: Res<ButtonInput<KeyCode>>,
    editor: Res<UiEditorState>,
    gameplay_block: Res<UiGameplayBlock>,
    mut buffer: ResMut<ActionInputBuffer>,
    mut actions: ResMut<super::TouchActions>,
) {
    if editor.enabled || editor.menu_open || gameplay_block.blocked {
        return;
    }
    if keys.just_pressed(KeyCode::KeyF) {
        buffer.tap_use();
        actions.action = true;
        actions.use_edge = true;
    }
    if keys.just_pressed(KeyCode::Space) {
        buffer.hold_jump();
        actions.jump = true;
        actions.jump_edge = true;
    } else if keys.pressed(KeyCode::Space) {
        buffer.hold_jump();
        actions.jump = true;
    }
    if keys.just_pressed(KeyCode::KeyQ) {
        actions.abilities |= 1;
    }
}

pub fn desktop_mouse_capture_toggle(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    editor: Res<UiEditorState>,
    gameplay_block: Res<UiGameplayBlock>,
    mut capture: ResMut<DesktopMouseCapture>,
) {
    if editor.enabled || editor.menu_open || gameplay_block.blocked {
        capture.active = false;
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        capture.active = false;
    }
    if mouse.just_pressed(MouseButton::Left) {
        capture.active = true;
    }
}

pub fn desktop_mouse_look(
    mut motion: EventReader<MouseMotion>,
    editor: Res<UiEditorState>,
    gameplay_block: Res<UiGameplayBlock>,
    capture: Res<DesktopMouseCapture>,
    mut look: ResMut<CameraLookIntent>,
) {
    if editor.enabled || editor.menu_open || gameplay_block.blocked {
        motion.clear();
        return;
    }
    // Free look while captured; J/L I/K still work via `keyboard_input` when not captured.
    if !capture.active {
        motion.clear();
        return;
    }
    let mut delta = Vec2::ZERO;
    for ev in motion.read() {
        delta += ev.delta;
    }
    if delta == Vec2::ZERO {
        return;
    }
    look.yaw -= delta.x * MOUSE_SENS_X;
    look.pitch = (look.pitch - delta.y * MOUSE_SENS_Y).clamp(FP_MIN_PITCH, FP_MAX_PITCH);
}

pub fn desktop_cursor_grab(
    editor: Res<UiEditorState>,
    gameplay_block: Res<UiGameplayBlock>,
    capture: Res<DesktopMouseCapture>,
    mut window: Query<&mut Window, With<PrimaryWindow>>,
) {
    let Ok(mut window) = window.get_single_mut() else {
        return;
    };
    let blocked = editor.enabled || editor.menu_open || gameplay_block.blocked;
    if blocked || !capture.active {
        window.cursor_options.grab_mode = CursorGrabMode::None;
        window.cursor_options.visible = true;
    } else {
        window.cursor_options.grab_mode = CursorGrabMode::Confined;
        window.cursor_options.visible = false;
    }
}
