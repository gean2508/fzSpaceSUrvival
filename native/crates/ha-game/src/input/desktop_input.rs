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
    flight: Res<crate::flight::Flight>,
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
    // Hold F (or Shift) to DESCEND while airborne — the flight controller reads
    // continuous `actions.action` (mobile's USE button held). Airborne-only so the
    // ground tap-to-interact behaviour is untouched.
    if flight.active && (keys.pressed(KeyCode::KeyF) || keys.pressed(KeyCode::ShiftLeft)) {
        actions.action = true;
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
    flight: Res<crate::flight::Flight>,
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
    flight: Res<crate::flight::Flight>,
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
    build_mode: Res<crate::input::BuildModeActive>,
    fly: Res<crate::creative_fly::CreativeFlight>,
    mut hold: ResMut<super::HoldInteractState>,
) {
    if editor.enabled || editor.menu_open || gameplay_block.blocked || !capture.active {
        return;
    }
    if fly.flying() {
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
        hold.placing_chest = false;
        hold.placing_crop = false;
        hold.locked_kind = None;

        if place_pressed {
            if build_mode.active && target.place_cell.is_some() {
                // BUILD mode: right-click places the selected piece (any tool).
                hold.active = true;
                hold.duration_ms = 500;
                hold.placing_build = true;
            } else if ha_shared::is_block_item(&equipped.item_id) && target.place_cell.is_some() {
                hold.active = true;
                hold.duration_ms = 300;
                hold.placing_block = true;
            } else if crate::granja::Planta::de_semilla(&equipped.item_id).is_some()
                && target.place_cell.is_some()
            {
                hold.active = true;
                hold.duration_ms = 250;
                hold.placing_crop = true;
            } else if equipped.item_id == crate::chests::CHEST_ITEM && target.kind.is_none() {
                hold.active = true;
                hold.duration_ms = 450;
                hold.placing_chest = true;
            } else if ha_shared::food_restore(&equipped.item_id).is_some()
                && target.kind.is_none()
            {
                // Food equipped, nothing targeted → hold RIGHT mouse to eat.
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
    if !any_pressed {
        hold.release(false);
    }
}

/// Desktop: the mouse drives the free aim cursor (top-down), clamped to reach.
/// Mirrors the right joystick on mobile so targeting/highlight work with a mouse.
pub fn desktop_aim_cursor(
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<crate::camera::GameCamera>>,
    player: Query<&crate::sim::SimState, With<crate::sim::Player>>,
    mut cursor: ResMut<super::AimCursor>,
) {
    // When the touch UI is forced on desktop, the right stick owns the cursor.
    if crate::flags::var("HA_FORCE_TOUCH").is_ok() || cursor.locked {
        return;
    }
    let Ok(win) = window.get_single() else {
        return;
    };
    let Some(sp) = win.cursor_position() else {
        return;
    };
    let Ok((cam, tf)) = camera.get_single() else {
        return;
    };
    // Vía `window_to_world_2d` y no `viewport_to_world_2d` a secas: con escala de
    // render la cámara dibuja contra una imagen más chica que la ventana, y el
    // cursor viene en píxeles de ventana. Ver `render::scale`.
    let Some(render) = crate::render::scale::window_to_world_2d(cam, tf, win, sp) else {
        return;
    };
    // Screen (x, -z) → world (x, z).
    let mut world = Vec2::new(render.x, -render.y);
    if let Ok(sim) = player.get_single() {
        let p = Vec2::new(sim.body.x, sim.body.z);
        let d = world - p;
        if d.length() > super::AIM_MAX_REACH {
            world = p + d.normalize() * super::AIM_MAX_REACH;
        }
    }
    cursor.world = world;
    cursor.initialized = true;
    // En escritorio el mouse SIEMPRE está apuntando a algo (no hay "levantar el
    // dedo"), así que el objetivo se resuelve siempre: es el equivalente exacto
    // del dedo apoyado del táctil.
    cursor.active = true;
}

pub fn desktop_cursor_grab(
    editor: Res<UiEditorState>,
    gameplay_block: Res<UiGameplayBlock>,
    flight: Res<crate::flight::Flight>,
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
