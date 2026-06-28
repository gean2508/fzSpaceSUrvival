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

#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct HoldInteractState {
    pub active: bool,
    pub progress_ms: u32,
    pub duration_ms: u32,
    pub cancelled: bool,
    pub completed: bool,
    /// Hammer build gesture without interact target.
    pub placing_machine: bool,
    /// Launch / orbital beacon hold — looser drift, no machine steal.
    pub flight_hold: bool,
    pub locked_kind: Option<ha_shared::InteractTargetKind>,
    /// Hold without target + hammer: place stone extractor instead of wood.
    pub placing_stone: bool,
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
            .init_resource::<HoldInteractState>()
            .init_resource::<AbilityTapPulse>()
            .init_resource::<AbilityCooldown>()
            .init_resource::<EquippedItem>()
            .add_systems(
                Update,
                (
                    look_gesture_system,
                    apply_smoothed_look.after(look_gesture_system),
                    actions::highlight_action_buttons,
                )
                    .chain(),
            )
            .add_systems(FixedUpdate, (tick_hold_interact, tick_ability_cooldown));
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
    editor: Res<UiEditorState>,
    gameplay_block: Res<UiGameplayBlock>,
    target: Res<LookTarget>,
    equipped: Res<EquippedItem>,
    pending: Res<PendingMachinePlacement>,
    mut touches: EventReader<TouchInput>,
    mut runtime: ResMut<LookGestureRuntime>,
    mut look_delta: ResMut<SmoothedLookDelta>,
    mut hold: ResMut<HoldInteractState>,
    mut cooldown: ResMut<AbilityCooldown>,
    mut tap: ResMut<AbilityTapPulse>,
) {
    tap.slot = None;
    if editor.enabled || editor.menu_open || gameplay_block.blocked {
        return;
    }

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
                hold.flight_hold = false;
                hold.locked_kind = None;
            }
            TouchPhase::Moved => {
                if runtime.pointer_id != Some(ev.id) {
                    continue;
                }
                let delta = pos - runtime.last_pos;
                runtime.last_pos = pos;
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
                runtime.hold_started = false;
            }
        }
    }

    if runtime.pointer_id.is_some() && !runtime.moved {
        let held_ms = now_ms - runtime.start_ms;
        if !runtime.hold_started && held_ms >= GESTURE_HOLD_START_MS as f32 {
            if let Some(kind) = target.kind {
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
            } else if equipped.item_id == "hammer"
                && held_ms >= GESTURE_MACHINE_HOLD_DELAY_MS as f32
                && target.kind.is_none()
            {
                runtime.hold_started = true;
                hold.active = true;
                let def = ha_shared::require_machine(pending.machine_id);
                hold.duration_ms = def.place_duration_ms;
                hold.progress_ms = 0;
                hold.cancelled = false;
                hold.completed = false;
                hold.placing_machine = true;
                hold.placing_stone = pending.machine_id == ha_shared::STONE_EXTRACTOR.id;
                hold.flight_hold = false;
                hold.locked_kind = None;
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
        || hold.placing_machine
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
