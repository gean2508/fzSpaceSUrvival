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
    flight: Res<crate::rocket_flight::RocketFlight>,
    mut capture: ResMut<DesktopMouseCapture>,
) {
    // Flight uses the mouse to AIM the ship, so don't treat the flight-caused
    // walk block as a reason to release the cursor — only real UI does.
    let ui_blocked = editor.enabled || editor.menu_open || (gameplay_block.blocked && !flight.active);
    if ui_blocked {
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
    flight: Res<crate::rocket_flight::RocketFlight>,
    capture: Res<DesktopMouseCapture>,
    mut look: ResMut<CameraLookIntent>,
) {
    // Allow look while flying (aim the ship) — only real UI blocks it.
    let ui_blocked = editor.enabled || editor.menu_open || (gameplay_block.blocked && !flight.active);
    if ui_blocked {
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

/// Minecraft-style desktop interaction: hold LEFT mouse = break/mine/gather the
/// look target; hold RIGHT mouse = place (block item, machine or build piece).
/// Drives the same `HoldInteractState` the touch look-zone gesture uses.
#[allow(clippy::too_many_arguments)]
pub fn desktop_hold_interact(
    mouse: Res<ButtonInput<MouseButton>>,
    editor: Res<UiEditorState>,
    gameplay_block: Res<UiGameplayBlock>,
    capture: Res<DesktopMouseCapture>,
    target: Res<crate::targeting::LookTarget>,
    equipped: Res<super::EquippedItem>,
    pending: Res<crate::machines::PendingMachinePlacement>,
    build_mode: Res<crate::building_world::BuildModeActive>,
    mut hold: ResMut<super::HoldInteractState>,
) {
    if editor.enabled || editor.menu_open || gameplay_block.blocked || !capture.active {
        return;
    }

    let break_pressed = mouse.pressed(MouseButton::Left);
    let place_pressed = mouse.pressed(MouseButton::Right);
    let any_pressed = break_pressed || place_pressed;
    let started = mouse.just_pressed(MouseButton::Left) || mouse.just_pressed(MouseButton::Right);

    if started && !hold.active {
        hold.progress_ms = 0;
        hold.cancelled = false;
        hold.completed = false;
        hold.placing_machine = false;
        hold.placing_build = false;
        hold.placing_block = false;
        hold.flight_hold = false;
        hold.eating = false;
        hold.locked_kind = None;

        if place_pressed {
            if ha_shared::is_block_item(&equipped.item_id) && target.place_cell.is_some() {
                hold.active = true;
                hold.duration_ms = 300;
                hold.placing_block = true;
            } else if ha_shared::food_restore(&equipped.item_id).is_some()
                && target.kind.is_none()
            {
                // Food equipped, nothing targeted → hold RIGHT mouse to eat.
                hold.active = true;
                hold.duration_ms = 1100;
                hold.eating = true;
            } else if equipped.item_id == "hammer" && target.kind.is_none() {
                if build_mode.active {
                    hold.active = true;
                    hold.duration_ms = 1200;
                    hold.placing_build = true;
                } else {
                    let def = ha_shared::require_machine(pending.machine_id);
                    hold.active = true;
                    hold.duration_ms = def.place_duration_ms;
                    hold.placing_machine = true;
                }
            }
        } else if break_pressed {
            if let Some(kind) = target.kind {
                if let Some(rule) = ha_shared::resolve_interaction(
                    Some(equipped.item_id.as_str()),
                    kind,
                ) {
                    if rule.kind != ha_shared::InteractActionKind::None && rule.duration_ms > 0 {
                        hold.active = true;
                        hold.duration_ms = rule.duration_ms;
                        hold.flight_hold = ha_shared::is_flight_hold_target(kind);
                        hold.locked_kind = Some(kind);
                    }
                }
            }
        }
        return;
    }

    // Released before completion → cancel, same as the touch gesture.
    if hold.active && !any_pressed && !hold.completed {
        hold.cancelled = true;
        hold.active = false;
        hold.progress_ms = 0;
    }
}

pub fn desktop_cursor_grab(
    editor: Res<UiEditorState>,
    gameplay_block: Res<UiGameplayBlock>,
    flight: Res<crate::rocket_flight::RocketFlight>,
    capture: Res<DesktopMouseCapture>,
    mut window: Query<&mut Window, With<PrimaryWindow>>,
) {
    let Ok(mut window) = window.get_single_mut() else {
        return;
    };
    // Grab the cursor while flying too — the ship aims with the mouse. Only
    // real UI (editor/menu/bag) releases it. THIS was the missing piece: the
    // cursor stayed loose in flight so mouse motion never drove the aim.
    let blocked = editor.enabled || editor.menu_open || (gameplay_block.blocked && !flight.active);
    if blocked || !capture.active {
        window.cursor_options.grab_mode = CursorGrabMode::None;
        window.cursor_options.visible = true;
    } else {
        window.cursor_options.grab_mode = CursorGrabMode::Confined;
        window.cursor_options.visible = false;
    }
}
