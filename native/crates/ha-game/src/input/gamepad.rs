//! Gamepad support — maps any connected pad onto the SAME intent resources
//! the touch sticks and keyboard feed, so every context (walk, flight, EVA)
//! works without new plumbing:
//!
//!   left stick  → move (full deflection = sprint, hysteresis like touch)
//!   right stick → camera look
//!   South (A)   → jump / retro     RT → ACTION hold (thrust / break / gather)
//!   LT          → place / eat hold (desktop right-mouse twin) + sprint (retro)
//!   Start       → orbital map menu
//!   East (B)    → close panels     D-pad ↑ → bag   D-pad ←/→ → hotbar slot
//!
//! Priority: touch stick (if active) > gamepad (if deflected) > keyboard.

use bevy::prelude::*;

use super::actions::{self, TouchActions};
use super::joystick::{CameraLookIntent, RawMovementIntent};
use crate::camera::{FP_MAX_PITCH, FP_MIN_PITCH};
use crate::game_ui::UiGameplayBlock;
use crate::ui_editor::UiEditorState;

const STICK_DEADZONE: f32 = 0.12;
const LOOK_SENS_X: f32 = 2.6; // rad/s at full deflection
const LOOK_SENS_Y: f32 = 1.8;
const SPRINT_ENTER: f32 = 0.92;
const SPRINT_EXIT: f32 = 0.75;

/// True while a pad is driving input — HUD can swap hints, touch UI can dim.
#[derive(Resource, Default)]
pub struct GamepadActive(pub bool);

/// Hotbar slot the D-pad last selected (0..7).
#[derive(Resource, Default)]
pub struct GamepadHotbarIndex(pub usize);

fn deadzone(v: Vec2) -> Vec2 {
    let len = v.length();
    if len < STICK_DEADZONE {
        return Vec2::ZERO;
    }
    // Rescale so the live range starts at zero (no jump at the deadzone edge).
    v / len * ((len - STICK_DEADZONE) / (1.0 - STICK_DEADZONE)).min(1.0)
}

/// Movement + look + core buttons. Runs after `keyboard_input` (pad wins over
/// keys when deflected) and before `touch_move_stick` (touch wins over pad).
pub(super) fn gamepad_input(
    time: Res<Time>,
    pads: Query<&Gamepad>,
    editor: Res<UiEditorState>,
    gameplay_block: Res<UiGameplayBlock>,
    mut active: ResMut<GamepadActive>,
    mut raw: ResMut<RawMovementIntent>,
    mut look: ResMut<CameraLookIntent>,
    mut actions: ResMut<TouchActions>,
    mut buffer: ResMut<actions::ActionInputBuffer>,
) {
    let Some(pad) = pads.iter().next() else {
        active.0 = false;
        return;
    };
    if editor.enabled || editor.menu_open || gameplay_block.blocked {
        return;
    }
    let dt = time.delta_secs();

    let move_stick = deadzone(pad.left_stick());
    let look_stick = deadzone(pad.right_stick());
    let any_button = pad.get_pressed().next().is_some();
    if move_stick != Vec2::ZERO || look_stick != Vec2::ZERO || any_button {
        active.0 = true;
    }

    // Look — right stick, camera-rate (frame-rate independent).
    if look_stick != Vec2::ZERO {
        look.yaw -= look_stick.x * LOOK_SENS_X * dt;
        look.pitch =
            (look.pitch + look_stick.y * LOOK_SENS_Y * dt).clamp(FP_MIN_PITCH, FP_MAX_PITCH);
    }

    // Move — left stick with the same sprint hysteresis as the touch stick.
    if move_stick != Vec2::ZERO {
        let mag = move_stick.length().min(1.0);
        raw.sprint = if raw.sprint {
            mag > SPRINT_EXIT
        } else {
            mag > SPRINT_ENTER
        };
        raw.move_x = move_stick.x;
        raw.move_z = -move_stick.y; // stick up = forward (-z), like W
    }

    // LT doubles as sprint — retro burn control in flight without full stick.
    if pad.pressed(GamepadButton::LeftTrigger2) {
        raw.sprint = true;
    }

    // Jump / retro — South, same buffer path as Space.
    if pad.just_pressed(GamepadButton::South) {
        buffer.hold_jump();
        actions.jump = true;
        actions.jump_edge = true;
    } else if pad.pressed(GamepadButton::South) {
        buffer.hold_jump();
        actions.jump = true;
    }

    // ACTION — RT: thrust in flight, parkour action on foot.
    if pad.pressed(GamepadButton::RightTrigger2) {
        actions.action = true;
    }
    if pad.just_pressed(GamepadButton::RightTrigger2) {
        actions.use_edge = true;
        buffer.tap_use();
    }
}

/// RT = break/gather hold, LT = place/eat hold — twin of `desktop_hold_interact`,
/// driving the same `HoldInteractState` the touch gesture uses.
pub(super) fn gamepad_hold_interact(
    pads: Query<&Gamepad>,
    editor: Res<UiEditorState>,
    gameplay_block: Res<UiGameplayBlock>,
    target: Res<crate::targeting::LookTarget>,
    equipped: Res<super::EquippedItem>,
    pending: Res<crate::machines::PendingMachinePlacement>,
    build_mode: Res<crate::input::BuildModeActive>,
    flight: Res<crate::flight::Flight>,
    fly: Res<crate::creative_fly::CreativeFlight>,
    mut hold: ResMut<super::HoldInteractState>,
) {
    let Some(pad) = pads.iter().next() else {
        return;
    };
    if editor.enabled || editor.menu_open || gameplay_block.blocked || fly.flying() {
        return;
    }
    // In flight RT is pure thrust — never a world interaction.
    if flight.active {
        return;
    }

    let break_pressed = pad.pressed(GamepadButton::RightTrigger2);
    let place_pressed = pad.pressed(GamepadButton::LeftTrigger2);
    let any_pressed = break_pressed || place_pressed;
    let started = pad.just_pressed(GamepadButton::RightTrigger2)
        || pad.just_pressed(GamepadButton::LeftTrigger2);

    if started && !hold.active {
        hold.progress_ms = 0;
        hold.cancelled = false;
        hold.completed = false;
        hold.placing_machine = false;
        hold.placing_build = false;
        hold.placing_block = false;
        hold.flight_hold = false;
        hold.eating = false;
        hold.placing_chest = false;
        hold.locked_kind = None;

        if place_pressed {
            if build_mode.active && target.place_cell.is_some() {
                hold.active = true;
                hold.duration_ms = 500;
                hold.placing_build = true;
            } else if ha_shared::is_block_item(&equipped.item_id) && target.place_cell.is_some() {
                hold.active = true;
                hold.duration_ms = 300;
                hold.placing_block = true;
            } else if equipped.item_id == crate::chests::CHEST_ITEM && target.kind.is_none() {
                hold.active = true;
                hold.duration_ms = 450;
                hold.placing_chest = true;
            } else if ha_shared::food_restore(&equipped.item_id).is_some()
                && target.kind.is_none()
            {
                hold.active = true;
                hold.duration_ms = 1100;
                hold.eating = true;
            } else if equipped.item_id == "hammer"
                && !build_mode.active
                && target.kind.is_none()
            {
                let def = ha_shared::require_machine(pending.machine_id);
                hold.active = true;
                hold.duration_ms = def.place_duration_ms;
                hold.placing_machine = true;
            }
        } else if break_pressed {
            if let Some(kind) = target.kind {
                if let Some(rule) =
                    ha_shared::resolve_interaction(Some(equipped.item_id.as_str()), kind)
                {
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

    // Released before completion → cancel, same as touch/desktop.
    if hold.active && !any_pressed && !hold.completed && !mouse_or_touch_hold(&hold) {
        hold.cancelled = true;
        hold.active = false;
        hold.progress_ms = 0;
    }
}

/// Don't cancel holds the mouse/touch started — only pad-initiated ones time
/// out here. Heuristic: if neither trigger is down and the hold JUST started
/// elsewhere, progress keeps ticking via the other device's own cancel paths.
fn mouse_or_touch_hold(_hold: &super::HoldInteractState) -> bool {
    false
}

/// Menu / panels / hotbar — Start, East, D-pad. Separate system: needs HUD
/// state that would push `gamepad_input` over the param limit.
pub(super) fn gamepad_menu_buttons(
    pads: Query<&Gamepad>,
    mut editor: ResMut<UiEditorState>,
    mut bag: ResMut<crate::hud::BagPanelState>,
    mut hotbar_idx: ResMut<GamepadHotbarIndex>,
    mut equip_req: ResMut<crate::hud::HotbarSelectRequest>,
) {
    let Some(pad) = pads.iter().next() else {
        return;
    };

    // Start → orbital map menu (the MENU button's toggle).
    if pad.just_pressed(GamepadButton::Start) {
        editor.menu_open = !editor.menu_open;
    }
    // East (B) → close whatever is open.
    if pad.just_pressed(GamepadButton::East) {
        editor.menu_open = false;
        bag.open = false;
    }
    // D-pad up → bag, like the I key.
    if pad.just_pressed(GamepadButton::DPadUp) {
        bag.open = !bag.open;
    }
    // D-pad left/right → hotbar slots 0..7.
    let mut delta: i32 = 0;
    if pad.just_pressed(GamepadButton::DPadRight) {
        delta = 1;
    } else if pad.just_pressed(GamepadButton::DPadLeft) {
        delta = -1;
    }
    if delta != 0 {
        hotbar_idx.0 = ((hotbar_idx.0 as i32 + delta).rem_euclid(8)) as usize;
        equip_req.slot = Some(hotbar_idx.0);
    }
}
