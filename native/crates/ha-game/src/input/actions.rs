use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use ha_shared::input::ABILITY_SLOTS;
use ha_shared::ParkourButtonHint;

use super::layout::{self, TouchBlockRects, TouchLayout};
use super::look_interact::{AbilityCooldown, AbilityTapPulse, HoldInteractState};
use crate::sim::{SimState, WorldCollision, Player};
use crate::game_ui::UiGameplayBlock;
use crate::ui_editor::{UiEditable, UiEditorState, UiLayoutConfig};
use crate::ui_theme::{self, UiButtonState, UiButtonTone};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    Use,
    Jump,
}

#[derive(Component, Clone, Copy)]
pub struct ActionButton(pub ActionKind);

#[derive(Resource, Default)]
pub struct TouchActions {
    pub action: bool,
    pub use_edge: bool,
    pub jump: bool,
    pub jump_edge: bool,
    pub abilities: u32,
}

#[derive(Resource)]
pub struct ActionInputBuffer {
    use_remaining: f32,
    jump_remaining: f32,
}

impl Default for ActionInputBuffer {
    fn default() -> Self {
        Self {
            use_remaining: 0.0,
            jump_remaining: 0.0,
        }
    }
}

impl ActionInputBuffer {
    pub(crate) fn tap_use(&mut self) {
        self.use_remaining = USE_TAP_BUFFER_SECS;
    }

    pub(crate) fn hold_jump(&mut self) {
        self.jump_remaining = JUMP_TAP_BUFFER_SECS;
    }
}

#[derive(Resource, Default)]
pub struct CraftIntent {
    pub recipe_id: Option<u32>,
}

const CRAFT_INTENT_SHIFT: u32 = 8;
const CRAFT_INTENT_MASK: u32 = 0xff;
const USE_TAP_BUFFER_SECS: f32 = 0.12;
const JUMP_TAP_BUFFER_SECS: f32 = 0.16;

#[derive(Component)]
pub(crate) struct ActionButtonLabel;

#[derive(Component)]
pub(crate) struct JumpButtonLabel;

#[derive(Component)]
pub struct ActionUiRoot;

#[derive(Resource, Default, Clone)]
pub struct ActionButtonRects {
    pub use_rect: Option<[f32; 4]>,
    pub jump_rect: Option<[f32; 4]>,
}

/// Independent multi-touch tracking per action button (Bevy `Button` only tracks one finger).
#[derive(Resource, Default)]
pub(crate) struct ActionTouchState {
    use_pointer: Option<u64>,
    jump_pointer: Option<u64>,
}

#[derive(Resource, Default)]
pub struct ActionButtonsSpawned(bool);

pub fn spawn_action_ui(mut commands: Commands) {
    if !crate::platform::uses_touch_gameplay_ui() {
        return;
    }
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            display: Display::Flex,
            ..default()
        },
        ActionUiRoot,
    ));
}

pub fn ensure_action_buttons(
    mut commands: Commands,
    layout: Res<TouchLayout>,
    mut spawned: ResMut<ActionButtonsSpawned>,
    root: Query<Entity, With<ActionUiRoot>>,
) {
    if !crate::platform::uses_touch_gameplay_ui() {
        spawned.0 = true;
        return;
    }
    if spawned.0 {
        return;
    }
    let Ok(root) = root.get_single() else {
        return;
    };
    if layout.width < 100.0 {
        return;
    }
    spawned.0 = true;

    let r = layout.radius;
    let btn_h = r * 0.56;
    let use_w = r * 1.38;
    let jump_w = r * 1.08;
    let cluster_cx = layout.aim_anchor.x - r * 1.2;
    let cluster_cy = layout.aim_anchor.y - r * 0.28;

    spawn_button(
        &mut commands,
        root,
        cluster_cx - r * 0.72,
        cluster_cy,
        jump_w,
        btn_h,
        "JUMP",
        ActionKind::Jump,
        UiButtonTone::Jump,
    );
    spawn_button(
        &mut commands,
        root,
        cluster_cx,
        cluster_cy,
        use_w,
        btn_h,
        "USE",
        ActionKind::Use,
        UiButtonTone::Use,
    );
}

fn spawn_button(
    commands: &mut Commands,
    parent: Entity,
    cx: f32,
    cy: f32,
    width: f32,
    height: f32,
    label: &str,
    kind: ActionKind,
    tone: UiButtonTone,
) {
    let border_w = ui_theme::action_border_width();
    commands.entity(parent).with_children(|p| {
        p.spawn((
            ActionButton(kind),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(cx - width * 0.5),
                top: Val::Px(cy - height * 0.5),
                width: Val::Px(width),
                height: Val::Px(height),
                border: UiRect::all(Val::Px(border_w)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            FocusPolicy::Pass,
            BorderColor(ui_theme::button_border(tone)),
            BackgroundColor(ui_theme::button_bg(tone, UiButtonState::Normal)),
        ))
        .with_children(|c| {
            if kind == ActionKind::Jump {
                c.spawn((
                    Text::new(label),
                    TextFont {
                        font_size: (height * 0.38).clamp(11.0, 18.0),
                        ..default()
                    },
                    TextColor(ui_theme::text_primary()),
                    JumpButtonLabel,
                ));
            } else {
                c.spawn((
                    Text::new(label),
                    TextFont {
                        font_size: (height * 0.40).clamp(11.0, 18.0),
                        ..default()
                    },
                    TextColor(ui_theme::text_primary()),
                    ActionButtonLabel,
                ));
            }
        });
    });
}

pub fn layout_action_buttons(
    layout: Res<TouchLayout>,
    config: Res<UiLayoutConfig>,
    mut root: Query<&mut Node, With<ActionUiRoot>>,
    mut buttons: Query<(&ActionButton, &mut Node, &Children), Without<ActionUiRoot>>,
    mut labels: Query<
        &mut TextFont,
        (
            Or<(With<ActionButtonLabel>, With<JumpButtonLabel>)>,
            Without<ActionUiRoot>,
        ),
    >,
    mut blocks: ResMut<TouchBlockRects>,
    mut rects: ResMut<ActionButtonRects>,
) {
    let Ok(mut root_node) = root.get_single_mut() else {
        return;
    };
    root_node.width = Val::Px(layout.width);
    root_node.height = Val::Px(layout.height);

    let r = layout.radius;
    let mut block_rects = Vec::with_capacity(2);
    let mut use_rect = None;
    let mut jump_rect = None;
    for (button, mut node, children) in &mut buttons {
        let (editable, base_w, base_h) = match button.0 {
            ActionKind::Use => (UiEditable::UseButton, r * 1.38, r * 0.56),
            ActionKind::Jump => (UiEditable::JumpButton, r * 1.08, r * 0.56),
        };
        let slot = config.slot(editable);
        let width = base_w * slot.scale;
        let height = base_h * slot.scale;
        let center = config.center(&layout, editable, Vec2::new(width, height));
        let cx = center.x;
        let cy = center.y;
        node.left = Val::Px(cx - width * 0.5);
        node.top = Val::Px(cy - height * 0.5);
        node.width = Val::Px(width);
        node.height = Val::Px(height);
        node.border = UiRect::all(Val::Px(ui_theme::action_border_width()));
        let rect = [
            cx - width * 0.5,
            cy - height * 0.5,
            cx + width * 0.5,
            cy + height * 0.5,
        ];
        block_rects.push(rect);
        match button.0 {
            ActionKind::Use => use_rect = Some(rect),
            ActionKind::Jump => jump_rect = Some(rect),
        };
        for child in children.iter() {
            if let Ok(mut font) = labels.get_mut(*child) {
                font.font_size = (height * 0.38).clamp(11.0, 18.0);
            }
        }
    }
    blocks.0 = block_rects;
    rects.use_rect = use_rect;
    rects.jump_rect = jump_rect;
}

fn touch_in_action_rect(pos: Vec2, rect: [f32; 4]) -> bool {
    layout::point_in_rect(pos.x, pos.y, rect)
}

pub fn update_jump_button_label(
    world: Res<WorldCollision>,
    layout: Res<crate::sim::WorldLayout>,
    player: Query<&SimState, With<Player>>,
    mut labels: Query<&mut Text, With<JumpButtonLabel>>,
) {
    let Ok(sim) = player.get_single() else {
        return;
    };
    let _ = (layout, world);
    let hint = ha_shared::parkour_button_hint(&sim.body);
    let label = match hint {
        ParkourButtonHint::Jump => "JUMP",
        ParkourButtonHint::Climb => "CLIMB",
        ParkourButtonHint::Mantle => "MANTLE",
    };
    for mut text in &mut labels {
        if text.0 != label {
            text.0 = label.to_string();
        }
    }
}

const ACTION_HIT_PAD: f32 = 12.0;

pub fn touch_hits_action_button(pos: Vec2, rects: &ActionButtonRects) -> bool {
    fn pad(rect: [f32; 4]) -> [f32; 4] {
        [
            rect[0] - ACTION_HIT_PAD,
            rect[1] - ACTION_HIT_PAD,
            rect[2] + ACTION_HIT_PAD,
            rect[3] + ACTION_HIT_PAD,
        ]
    }
    if let Some(r) = rects.jump_rect {
        if touch_in_action_rect(pos, pad(r)) {
            return true;
        }
    }
    if let Some(r) = rects.use_rect {
        if touch_in_action_rect(pos, pad(r)) {
            return true;
        }
    }
    false
}

pub fn poll_action_buttons_held(
    time: Res<Time>,
    mut actions: ResMut<TouchActions>,
    mut craft: ResMut<CraftIntent>,
    mut buffer: ResMut<ActionInputBuffer>,
    mut touch_state: ResMut<ActionTouchState>,
    hold: Res<HoldInteractState>,
    tap: Res<AbilityTapPulse>,
    cooldown: Res<AbilityCooldown>,
    editor: Res<UiEditorState>,
    gameplay_block: Res<UiGameplayBlock>,
    button_rects: Res<ActionButtonRects>,
    mut touches: EventReader<TouchInput>,
) {
    actions.action = false;
    actions.use_edge = false;
    actions.jump = false;
    actions.jump_edge = false;
    actions.abilities = 0;
    let dt = time.delta_secs().min(0.08);
    buffer.use_remaining = (buffer.use_remaining - dt).max(0.0);
    buffer.jump_remaining = (buffer.jump_remaining - dt).max(0.0);

    if editor.enabled || editor.menu_open || gameplay_block.blocked {
        touch_state.use_pointer = None;
        touch_state.jump_pointer = None;
        return;
    }

    for ev in touches.read() {
        let pos = ev.position;
        match ev.phase {
            TouchPhase::Started => {
                if touch_state.use_pointer.is_none() {
                    if let Some(rect) = button_rects.use_rect {
                        if touch_in_action_rect(pos, rect) {
                            touch_state.use_pointer = Some(ev.id);
                            buffer.use_remaining = USE_TAP_BUFFER_SECS;
                            actions.use_edge = true;
                        }
                    }
                }
                if touch_state.jump_pointer.is_none() {
                    if let Some(rect) = button_rects.jump_rect {
                        if touch_in_action_rect(pos, rect) {
                            touch_state.jump_pointer = Some(ev.id);
                            buffer.jump_remaining = JUMP_TAP_BUFFER_SECS;
                            actions.jump_edge = true;
                        }
                    }
                }
            }
            TouchPhase::Ended | TouchPhase::Canceled => {
                if touch_state.use_pointer == Some(ev.id) {
                    touch_state.use_pointer = None;
                }
                if touch_state.jump_pointer == Some(ev.id) {
                    touch_state.jump_pointer = None;
                }
            }
            TouchPhase::Moved => {}
        }
    }

    if touch_state.use_pointer.is_some() {
        actions.action = true;
    }
    if touch_state.jump_pointer.is_some() {
        actions.jump = true;
    }
    if buffer.use_remaining > 0.0 {
        actions.action = true;
    }
    if buffer.jump_remaining > 0.0 {
        actions.jump = true;
    }
    if hold.active {
        actions.action = true;
    }
    if let Some(slot) = tap.slot {
        if slot < ABILITY_SLOTS {
            actions.abilities |= 1 << slot;
        }
    }
    if cooldown.remaining_ms <= 0.0 && cooldown.total_ms > 0.0 {
        // ready
    }
    if let Some(recipe_id) = craft.recipe_id.take() {
        actions.abilities |= (recipe_id & CRAFT_INTENT_MASK) << CRAFT_INTENT_SHIFT;
    }
}

pub fn highlight_action_buttons(
    touch_state: Res<ActionTouchState>,
    mut buttons: Query<
        (&ActionButton, &mut BackgroundColor, &mut BorderColor),
        With<ActionButton>,
    >,
) {
    for (button, mut bg, mut border) in &mut buttons {
        let pressed = match button.0 {
            ActionKind::Use => touch_state.use_pointer.is_some(),
            ActionKind::Jump => touch_state.jump_pointer.is_some(),
        };
        let tone = match button.0 {
            ActionKind::Use => UiButtonTone::Use,
            ActionKind::Jump => UiButtonTone::Jump,
        };
        let interaction = if pressed {
            Interaction::Pressed
        } else {
            Interaction::None
        };
        *bg = BackgroundColor(ui_theme::button_bg(
            tone,
            ui_theme::interaction_state(interaction, false),
        ));
        *border = BorderColor(ui_theme::button_border_pressed(tone, interaction));
    }
}
