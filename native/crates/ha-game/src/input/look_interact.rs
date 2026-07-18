use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;
use ha_shared::{
    get_class, resolve_interaction, ClassDef, DEFAULT_CLASS_ID, GESTURE_FLIGHT_MOVE_CANCEL_PX,
    GESTURE_HOLD_START_MS, GESTURE_MACHINE_HOLD_DELAY_MS, GESTURE_MOVE_CANCEL_PX,
    GESTURE_TAP_MAX_MS, InteractActionKind, is_flight_hold_target,
};

use super::layout::{self, TouchBlockRects, TouchLayout};
use super::actions::{self, TouchActions};
use super::calibration::CAMERA_LOOK_SMOOTH_RATE;
use super::CameraLookIntent;
use crate::camera::{FP_MAX_PITCH, FP_MIN_PITCH};
use crate::machines::PendingMachinePlacement;
use crate::targeting::LookTarget;
use crate::game_ui::UiGameplayBlock;
use crate::ui_editor::{UiEditorState, UiLayoutConfig};

/// Build-mode toggle (modular pieces) — legacy switch kept for the HUD.
#[derive(Resource, Default)]
pub struct BuildModeActive {
    pub active: bool,
}

/// World-space pointer for 2D targeting: the active look-zone touch position
/// (screen px). Desktop targeting falls back to the window cursor.
#[derive(Resource, Default, Clone, Copy)]
pub struct LookPointer {
    pub pos: Option<Vec2>,
}

#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct HoldInteractState {
    pub active: bool,
    pub progress_ms: u32,
    pub duration_ms: u32,
    pub cancelled: bool,
    pub completed: bool,
    /// Hammer build gesture without interact target.
    pub placing_machine: bool,
    /// Hammer + build mode — modular snap placement.
    pub placing_build: bool,
    /// Launch / orbital beacon hold — looser drift, no machine steal.
    pub flight_hold: bool,
    pub locked_kind: Option<ha_shared::InteractTargetKind>,
    /// Hold without target + hammer: place stone extractor instead of wood.
    pub placing_stone: bool,
    /// Block item equipped + valid place slot — Minecraft-style place hold.
    pub placing_block: bool,
    /// Food equipped + no target — hold to eat (consumed in `survival_stats.rs`).
    pub eating: bool,
}

#[derive(Resource, Default)]
pub struct AbilityTapPulse {
    pub slot: Option<usize>,
}

#[derive(Resource, Default)]
pub struct AbilityCooldown {
    pub slot: usize,
    pub remaining_ms: f32,
    pub total_ms: f32,
}

#[derive(Resource, Default)]
struct LookGestureRuntime {
    pointer_id: Option<u64>,
    /// Where the finger landed — MC uses touch origin, not screen center.
    anchor_pos: Vec2,
    last_pos: Vec2,
    start_ms: f32,
    moved: bool,
    hold_started: bool,
}

#[derive(Resource, Default)]
struct SmoothedLookDelta {
    pending_yaw: f32,
    pending_pitch: f32,
}

pub struct LookInteractPlugin;

impl Plugin for LookInteractPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LookGestureRuntime>()
            .init_resource::<SmoothedLookDelta>()
            .init_resource::<BuildModeActive>()
            .init_resource::<LookPointer>()
            .init_resource::<HoldInteractState>()
            .init_resource::<AbilityTapPulse>()
            .init_resource::<AbilityCooldown>()
            .init_resource::<EquippedItem>()
            .add_systems(
                Update,
                (
                    use_button_hold_interact,
                    apply_smoothed_look,
                    actions::highlight_action_buttons,
                )
                    .chain()
                    .run_if(|editor: Res<UiEditorState>, block: Res<UiGameplayBlock>| {
                        !editor.enabled && !editor.menu_open && !block.blocked
                    }),
            )
            .add_systems(FixedUpdate, (tick_hold_interact, tick_ability_cooldown));
    }
}

/// Top-down interaction: holding the USE button acts on the tile/object under
/// the aim cursor (mine/chop/pickup, place block, place machine, eat) — context
/// -driven, one action per press. Replaces the old right-side drag-hold gesture
/// (that zone is now the aim joystick). Advances via `tick_hold_interact`.
#[allow(clippy::too_many_arguments)]
fn use_button_hold_interact(
    time: Res<Time>,
    actions: Res<super::TouchActions>,
    target: Res<crate::targeting::LookTarget>,
    equipped: Res<EquippedItem>,
    pending: Res<crate::machines::PendingMachinePlacement>,
    build_mode: Res<BuildModeActive>,
    mut hold: ResMut<HoldInteractState>,
    mut started: Local<bool>,
    mut held_ms: Local<f32>,
) {
    // Desktop keeps its mouse path (`desktop_hold_interact`).
    if !crate::platform::uses_touch_gameplay_ui() {
        return;
    }
    if !actions.action {
        // Released: cancel an in-progress hold and re-arm for the next press.
        if hold.active && !hold.completed {
            hold.cancelled = true;
            hold.active = false;
            hold.progress_ms = 0;
        }
        *started = false;
        *held_ms = 0.0;
        return;
    }
    *held_ms += time.delta_secs() * 1000.0;
    if *started || hold.active || hold.completed {
        return;
    }
    let held = *held_ms;
    // BUILD mode: aiming at an empty cell places the selected piece (any tool).
    let build_place = build_mode.active && target.place_cell.is_some();
    let block_place =
        ha_shared::is_block_item(&equipped.item_id) && target.place_cell.is_some();
    if build_place {
        *started = true;
        *hold = HoldInteractState {
            active: true,
            duration_ms: 500,
            placing_build: true,
            ..default()
        };
    } else if block_place {
        *started = true;
        *hold = HoldInteractState {
            active: true,
            duration_ms: 300,
            placing_block: true,
            ..default()
        };
    } else if let Some(kind) = target.kind {
        if let Some(rule) = resolve_interaction(Some(equipped.item_id.as_str()), kind) {
            if rule.kind != InteractActionKind::None && rule.duration_ms > 0 {
                *started = true;
                *hold = HoldInteractState {
                    active: true,
                    duration_ms: rule.duration_ms,
                    flight_hold: is_flight_hold_target(kind),
                    locked_kind: Some(kind),
                    ..default()
                };
            }
        }
    } else if ha_shared::food_restore(&equipped.item_id).is_some() {
        *started = true;
        *hold = HoldInteractState {
            active: true,
            duration_ms: 1100,
            eating: true,
            ..default()
        };
    } else if equipped.item_id == "hammer"
        && !build_mode.active
        && held >= GESTURE_MACHINE_HOLD_DELAY_MS as f32
    {
        // Hammer (outside build mode) places the pending machine.
        *started = true;
        let def = ha_shared::require_machine(pending.machine_id);
        *hold = HoldInteractState {
            active: true,
            duration_ms: def.place_duration_ms,
            placing_machine: true,
            placing_stone: pending.machine_id == ha_shared::STONE_EXTRACTOR.id,
            ..default()
        };
    }
}

#[derive(Resource, Clone)]
pub struct EquippedItem {
    pub item_id: String,
    pub class: ClassDef,
}

impl Default for EquippedItem {
    fn default() -> Self {
        Self {
            item_id: "hammer".to_string(),
            class: get_class(DEFAULT_CLASS_ID),
        }
    }
}

fn look_gesture_system(
    time: Res<Time>,
    layout: Res<TouchLayout>,
    config: Res<UiLayoutConfig>,
    blocks: Res<TouchBlockRects>,
    button_rects: Res<actions::ActionButtonRects>,
    target: Res<LookTarget>,
    equipped: Res<EquippedItem>,
    pending: Res<PendingMachinePlacement>,
    build_mode: Res<BuildModeActive>,
    mut touches: EventReader<TouchInput>,
    mut runtime: ResMut<LookGestureRuntime>,
    mut look_delta: ResMut<SmoothedLookDelta>,
    mut pointer: ResMut<LookPointer>,
    mut hold: ResMut<HoldInteractState>,
    mut cooldown: ResMut<AbilityCooldown>,
    mut tap: ResMut<AbilityTapPulse>,
) {
    tap.slot = None;

    let sens = layout::look_sensitivity(&layout) * config.calibration.camera_sensitivity;
    let now_ms = time.elapsed_secs() * 1000.0;

    for ev in touches.read() {
        let pos = ev.position;
        match ev.phase {
            TouchPhase::Started => {
                if actions::touch_hits_action_button(pos, &button_rects) {
                    continue;
                }
                if !layout::touch_in_look_zone(pos, &layout)
                    || layout::touch_blocked(pos, &blocks.0)
                {
                    continue;
                }
                runtime.pointer_id = Some(ev.id);
                pointer.pos = Some(pos);
                runtime.anchor_pos = pos;
                runtime.last_pos = pos;
                runtime.start_ms = now_ms;
                runtime.moved = false;
                runtime.hold_started = false;
                hold.active = false;
                hold.progress_ms = 0;
                hold.cancelled = false;
                hold.completed = false;
                hold.placing_machine = false;
                hold.placing_build = false;
                hold.placing_block = false;
                hold.flight_hold = false;
                hold.eating = false;
                hold.locked_kind = None;
            }
            TouchPhase::Moved => {
                if runtime.pointer_id != Some(ev.id) {
                    continue;
                }
                let delta = pos - runtime.last_pos;
                runtime.last_pos = pos;
                pointer.pos = Some(pos);
                let from_anchor = pos - runtime.anchor_pos;
                let drift_limit = if hold.flight_hold {
                    GESTURE_FLIGHT_MOVE_CANCEL_PX
                } else {
                    GESTURE_MOVE_CANCEL_PX
                };
                if from_anchor.length() > drift_limit {
                    runtime.moved = true;
                    if runtime.hold_started {
                        hold.cancelled = true;
                        hold.active = false;
                        hold.progress_ms = 0;
                    }
                }
                // Accumulate look delta — applied with short smoothing in apply_smoothed_look.
                look_delta.pending_yaw -= delta.x * sens;
                look_delta.pending_pitch -= delta.y * sens;
            }
            TouchPhase::Ended | TouchPhase::Canceled => {
                if runtime.pointer_id != Some(ev.id) {
                    continue;
                }
                let held_ms = now_ms - runtime.start_ms;
                if !runtime.moved
                    && held_ms < GESTURE_TAP_MAX_MS as f32
                    && cooldown.remaining_ms <= 0.0
                {
                    tap.slot = Some(equipped.class.tap_ability_slot);
                    cooldown.slot = equipped.class.tap_ability_slot;
                    cooldown.total_ms = tap_cooldown_ms(&equipped.class);
                    cooldown.remaining_ms = cooldown.total_ms;
                }
                runtime.pointer_id = None;
                pointer.pos = None;
                runtime.hold_started = false;
            }
        }
    }

    if runtime.pointer_id.is_some() && !runtime.moved {
        let held_ms = now_ms - runtime.start_ms;
        if !runtime.hold_started && held_ms >= GESTURE_HOLD_START_MS as f32 {
            // Block item equipped + valid slot → place (even against a block face,
            // Minecraft-style: to break, equip hand/pickaxe instead).
            let block_place = ha_shared::is_block_item(&equipped.item_id)
                && target.place_cell.is_some();
            if block_place {
                runtime.hold_started = true;
                hold.active = true;
                hold.duration_ms = 300;
                hold.progress_ms = 0;
                hold.cancelled = false;
                hold.completed = false;
                hold.placing_machine = false;
                hold.placing_build = false;
                hold.placing_block = true;
                hold.flight_hold = false;
                hold.locked_kind = None;
            } else if let Some(kind) = target.kind {
                if let Some(rule) = resolve_interaction(Some(equipped.item_id.as_str()), kind) {
                    if rule.kind != InteractActionKind::None && rule.duration_ms > 0 {
                        runtime.hold_started = true;
                        hold.active = true;
                        hold.duration_ms = rule.duration_ms;
                        hold.progress_ms = 0;
                        hold.cancelled = false;
                        hold.completed = false;
                        hold.placing_machine = false;
                        hold.placing_stone = false;
                        hold.flight_hold = is_flight_hold_target(kind);
                        hold.locked_kind = Some(kind);
                    }
                }
            } else if ha_shared::food_restore(&equipped.item_id).is_some()
                && target.kind.is_none()
            {
                // Food equipped, nothing targeted → hold to eat.
                runtime.hold_started = true;
                hold.active = true;
                hold.duration_ms = 1100;
                hold.progress_ms = 0;
                hold.cancelled = false;
                hold.completed = false;
                hold.placing_machine = false;
                hold.placing_build = false;
                hold.placing_block = false;
                hold.flight_hold = false;
                hold.eating = true;
                hold.locked_kind = None;
            } else if equipped.item_id == "hammer"
                && held_ms >= GESTURE_MACHINE_HOLD_DELAY_MS as f32
                && target.kind.is_none()
            {
                runtime.hold_started = true;
                hold.active = true;
                hold.progress_ms = 0;
                hold.cancelled = false;
                hold.completed = false;
                hold.flight_hold = false;
                hold.locked_kind = None;
                if build_mode.active {
                    hold.placing_build = true;
                    hold.placing_machine = false;
                    hold.placing_stone = false;
                    hold.duration_ms = 1200;
                } else {
                    let def = ha_shared::require_machine(pending.machine_id);
                    hold.duration_ms = def.place_duration_ms;
                    hold.placing_machine = true;
                    hold.placing_build = false;
                    hold.placing_stone = pending.machine_id == ha_shared::STONE_EXTRACTOR.id;
                }
            }
        }
    }
}

fn apply_smoothed_look(
    time: Res<Time>,
    mut look: ResMut<CameraLookIntent>,
    mut pending: ResMut<SmoothedLookDelta>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let k = 1.0 - (-CAMERA_LOOK_SMOOTH_RATE * dt).exp();
    if pending.pending_yaw.abs() > 1e-7 || pending.pending_pitch.abs() > 1e-7 {
        look.yaw += pending.pending_yaw * k;
        look.pitch = (look.pitch + pending.pending_pitch * k).clamp(FP_MIN_PITCH, FP_MAX_PITCH);
        pending.pending_yaw *= 1.0 - k;
        pending.pending_pitch *= 1.0 - k;
    }
}

fn tap_cooldown_ms(class: &ClassDef) -> f32 {
    match class.tap_ability_slot {
        0 => 650.0,
        1 => 2200.0,
        _ => 900.0,
    }
}

pub(crate) fn tick_hold_interact(
    time: Res<Time<Fixed>>,
    mut hold: ResMut<HoldInteractState>,
    target: Res<LookTarget>,
) {
    if hold.completed || hold.cancelled {
        hold.active = false;
        return;
    }
    if !hold.active {
        return;
    }
    let target_valid = target.entity.is_some()
        || target.block_cell.is_some()
        || target.flora_cell.is_some()
        || hold.placing_machine
        || hold.placing_build
        || hold.eating
        || (hold.placing_block && target.place_cell.is_some())
        || (hold.flight_hold && hold.locked_kind.is_some());
    let delta_ms = (time.delta_secs() * 1000.0) as u32;
    let next = ha_shared::advance_hold_progress(
        ha_shared::HoldProgress {
            active: hold.active,
            progress_ms: hold.progress_ms,
            duration_ms: hold.duration_ms,
            cancelled: hold.cancelled,
            completed: hold.completed,
            placing_machine: hold.placing_machine,
            placing_build: hold.placing_build,
        },
        delta_ms,
        target_valid,
    );
    hold.active = next.active;
    hold.progress_ms = next.progress_ms;
    hold.completed = next.completed;
}

fn tick_ability_cooldown(time: Res<Time<Fixed>>, mut cd: ResMut<AbilityCooldown>) {
    let delta_ms = time.delta_secs() * 1000.0;
    cd.remaining_ms = ha_shared::tick_ability_cooldown(cd.remaining_ms, delta_ms);
}
