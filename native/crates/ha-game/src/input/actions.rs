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

impl ActionTouchState {
    /// ¿Hay un dedo sostenido sobre el botón USAR?
    pub(crate) fn usando(&self) -> bool {
        self.use_pointer.is_some()
    }
}

#[derive(Resource, Default)]
pub struct ActionButtonsSpawned(bool);

/// **El botón USAR ya no existe en tierra.** Tocar el mundo es la interacción
/// (`input/world_touch.rs`), así que en superficie el botón no hacía nada más que
/// ocupar el pulgar derecho. Sobrevive para lo único que todavía manda: BAJAR
/// mientras volás (el controlador de vuelo lee `TouchActions::action` sostenido).
#[derive(Resource, Default)]
pub struct UseButtonActive(pub bool);

/// Decide si el botón USAR pinta esta pantalla. Fuera del vuelo se esconde, y
/// `layout_action_buttons` deja de publicar su rectángulo — así el dedo que cae
/// donde ESTABA el botón le llega al mundo, que es lo que el ojo espera al no
/// verlo.
pub fn sync_use_button_active(
    fly: Res<crate::creative_fly::CreativeFlight>,
    flight: Res<crate::flight::Flight>,
    equipped: Res<super::EquippedItem>,
    inv: Res<crate::inventory_persist::PlayerInventory>,
    mut active: ResMut<UseButtonActive>,
) {
    // **Y el botón USAR vuelve a existir para lo que tenés en la mano**: con comida, ropa o un
    // cubo con agua dice COMER / VESTIR / BEBER y NO compite con el mantener del mundo, que
    // sigue siendo cavar, talar y picar (como el botón "usar" de Minecraft de bolsillo).
    let want = fly.flying() || flight.active || uso_del_objeto(&equipped.item_id, &inv).is_some();
    if active.0 != want {
        active.0 = want;
    }
}

/// Qué dice el botón USAR para el objeto en la mano (`None` = el objeto no se "usa" solo).
fn uso_del_objeto(item: &str, inv: &crate::inventory_persist::PlayerInventory) -> Option<&'static str> {
    if inv.qty(item) == 0 {
        return None;
    }
    crate::equipo::etiqueta_de_uso(item)
}

/// El texto del botón USAR: BAJAR volando, y la acción del objeto en tierra.
pub fn update_use_button_label(
    fly: Res<crate::creative_fly::CreativeFlight>,
    flight: Res<crate::flight::Flight>,
    equipped: Res<super::EquippedItem>,
    inv: Res<crate::inventory_persist::PlayerInventory>,
    mut labels: Query<&mut Text, With<ActionButtonLabel>>,
) {
    let label = if fly.flying() || flight.active {
        "BAJAR"
    } else {
        uso_del_objeto(&equipped.item_id, &inv).unwrap_or("USAR")
    };
    for mut text in &mut labels {
        if text.0 != label {
            text.0 = label.to_string();
        }
    }
}

/// **Mantener el botón USAR = usar el objeto** (comer, vestirse, beber): arranca el mismo
/// "mantener" que el escritorio hace con el clic derecho, y soltar lo cancela. Volando el
/// botón sigue siendo BAJAR y esto no hace nada.
#[allow(clippy::too_many_arguments)]
pub fn usar_objeto_con_boton(
    touch_state: Res<ActionTouchState>,
    fly: Res<crate::creative_fly::CreativeFlight>,
    flight: Res<crate::flight::Flight>,
    equipped: Res<super::EquippedItem>,
    inv: Res<crate::inventory_persist::PlayerInventory>,
    mut hold: ResMut<HoldInteractState>,
) {
    if fly.flying() || flight.active || uso_del_objeto(&equipped.item_id, &inv).is_none() {
        return;
    }
    if touch_state.use_pointer.is_some() {
        if !hold.active && !hold.completed {
            *hold = HoldInteractState {
                active: true,
                duration_ms: 1100,
                eating: true,
                ..default()
            };
        }
    } else if hold.eating && hold.active && !hold.completed {
        hold.release(false);
    }
}

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
        crate::game_ui::GameplayUi,
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
        // Lo pisa `update_jump_button_label` en cuanto hay jugador; acá va el
        // texto de reposo, ya en castellano (biblia §2, regla 6).
        "SALTAR",
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
        // Sólo aparece volando, y ahí significa una cosa sola.
        "BAJAR",
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
            // La letra se achica sola hasta entrar en el disco (`fuente.rs`).
            crate::fuente::BotonConTexto,
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
            // Redondo del todo: `MAX` = radio media-altura, así el nodo queda
            // circular cuando es cuadrado (ver `layout_action_buttons`).
            BorderRadius::MAX,
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
    use_active: Res<UseButtonActive>,
    mut root: Query<&mut Node, With<ActionUiRoot>>,
    mut buttons: Query<(&ActionButton, &mut Node, &Children), Without<ActionUiRoot>>,
    mut labels: Query<
        (&mut TextFont, Option<&crate::fuente::TamanoPedido>),
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
    // **Mismo cuidado que los sticks (`joystick::set_px`).** Este sistema corre
    // en la misma cadena de `Update` que los pintores del joystick y escribía
    // los MISMOS píxeles todos los frames: cada escritura marca el `Node` sucio
    // y taffy relayoutea el árbol de UI completo. Los sticks ya lo evitaban con
    // `bypass_change_detection`; los botones de acción se habían quedado afuera,
    // así que el relayout por frame seguía pagándose igual.
    {
        let node = root_node.bypass_change_detection();
        let mut dirty = crate::input::set_px(&mut node.width, layout.width);
        dirty |= crate::input::set_px(&mut node.height, layout.height);
        if dirty {
            root_node.set_changed();
        }
    }

    let r = layout.radius;
    let mut block_rects = Vec::with_capacity(2);
    let mut use_rect = None;
    let mut jump_rect = None;
    for (button, mut node, children) in &mut buttons {
        // **Circulares, no rectángulos.** Eran píldoras de 2.5:1 (r·1.38 × r·0.56):
        // el mismo botón gris de cualquier app, y encima el que más se mira. Un
        // disco es de la misma familia que el joystick y que los iconos redondos
        // de las tarjetas, y el área táctil no cambia de tamaño real (USE queda un
        // 20% más chico de ancho pero casi el doble de alto).
        let (editable, base_w, base_h) = match button.0 {
            ActionKind::Use => (UiEditable::UseButton, r * 0.96, r * 0.96),
            ActionKind::Jump => (UiEditable::JumpButton, r * 0.84, r * 0.84),
        };
        // USAR fuera del vuelo: nodo escondido y sin rectángulo táctil.
        let hidden = button.0 == ActionKind::Use && !use_active.0;
        let slot = config.slot(editable);
        let width = base_w * slot.scale;
        let height = base_h * slot.scale;
        let center = config.center(&layout, editable, Vec2::new(width, height));
        let cx = center.x;
        let cy = center.y;
        {
            let n = node.bypass_change_detection();
            let want_display = if hidden { Display::None } else { Display::Flex };
            let mut dirty = n.display != want_display;
            n.display = want_display;
            dirty |= crate::input::set_px(&mut n.left, cx - width * 0.5);
            dirty |= crate::input::set_px(&mut n.top, cy - height * 0.5);
            dirty |= crate::input::set_px(&mut n.width, width);
            dirty |= crate::input::set_px(&mut n.height, height);
            let border = UiRect::all(Val::Px(ui_theme::action_border_width()));
            if n.border != border {
                n.border = border;
                dirty = true;
            }
            if dirty {
                node.set_changed();
            }
        }
        if hidden {
            continue;
        }
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
        // El tamaño de fuente cuesta todavía más que un `Node`: cambiarlo obliga
        // a RE-MEDIR el texto antes de relayoutear. Sólo cuando cambió de verdad.
        //
        // Esto es lo que se PIDE; `fuente::encoger_texto_de_botones` lo achica
        // si la palabra no entra en el disco. Por eso se compara contra el
        // pedido y no contra el tamaño dibujado: si no, cada cuadro se pisarían.
        let font_size = (height * 0.38).clamp(11.0, 18.0);
        for child in children.iter() {
            if let Ok((mut font, ajuste)) = labels.get_mut(*child) {
                let pedido = ajuste.map_or(font.font_size, |a| a.pedido);
                if pedido != font_size {
                    font.font_size = font_size;
                }
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

/// **El texto del botón ES el detector de bordes.**
///
/// Trepar dejó de pasar solo (ver `escaleras::trepar_borde`): ahora hace falta
/// pulsar, y lo único que le dice al jugador que puede es esta palabra. Mirás la
/// pared del pozo y donde decía SALTAR dice TREPAR; te das vuelta y vuelve a
/// SALTAR. Sin ella, el botón tendría un segundo significado invisible.
pub fn update_jump_button_label(
    world: Res<WorldCollision>,
    layout: Res<crate::sim::WorldLayout>,
    trepada: Res<crate::escaleras::Trepada>,
    player: Query<&SimState, With<Player>>,
    mut labels: Query<&mut Text, With<JumpButtonLabel>>,
) {
    let Ok(sim) = player.get_single() else {
        return;
    };
    let _ = (layout, world);
    let label = if trepada.hacia.is_some() {
        "TREPAR"
    } else {
        match ha_shared::parkour_button_hint(&sim.body) {
            ParkourButtonHint::Jump => "SALTAR",
            ParkourButtonHint::Climb => "TREPAR",
            ParkourButtonHint::Mantle => "IZARSE",
        }
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
