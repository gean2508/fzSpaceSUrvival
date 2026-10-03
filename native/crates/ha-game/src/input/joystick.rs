use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;
use super::actions::{self, TouchActions};
use super::layout::{self, TouchLayout, TouchBlockRects};
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
pub(crate) struct RawMovementIntent {
    pub(super) move_x: f32,
    pub(super) move_z: f32,
    pub(super) aim_x: f32,
    pub(super) aim_z: f32,
    pub(super) sprint: bool,
}

#[derive(Clone, Copy, Default)]
struct JoystickSide {
    active: bool,
    pointer_id: Option<u64>,
    anchor: Vec2,
    current: Vec2,
}

/// **Un solo stick.** El derecho manejaba la mira; ya no hay mira (el dedo va
/// directo al objeto — ver `input/world_touch.rs`), así que la mitad derecha de
/// la pantalla volvió a ser el mundo.
#[derive(Resource, Default)]
struct StickState {
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
            .init_resource::<super::gamepad::GamepadActive>()
            .init_resource::<super::gamepad::GamepadHotbarIndex>()
            .init_resource::<StickState>()
            .init_resource::<TouchLayout>()
            .init_resource::<TouchBlockRects>()
            .init_resource::<TouchActions>()
            .init_resource::<actions::CraftIntent>()
            .init_resource::<actions::ActionInputBuffer>()
            .init_resource::<actions::ActionButtonRects>()
            .init_resource::<actions::ActionTouchState>()
            .init_resource::<actions::ActionButtonsSpawned>()
            .init_resource::<actions::UseButtonActive>()
            .add_systems(Startup, (spawn_joystick_ui, actions::spawn_action_ui));
        #[cfg(not(target_os = "android"))]
        app.init_resource::<desktop_input::DesktopMouseCapture>()
            .add_systems(
                Update,
                (
                    desktop_input::desktop_keyboard_actions.after(actions::poll_action_buttons_held),
                    desktop_input::desktop_mouse_capture_toggle,
                    desktop_input::desktop_mouse_look,
                    desktop_input::desktop_hold_interact
                        .after(desktop_input::desktop_mouse_capture_toggle)
                        ,
                    desktop_input::desktop_aim_cursor,
                    desktop_input::desktop_cursor_grab,
                ),
            );
        // (Acá se registraba el traductor de teclado de ESCRITORIO también en Android, sólo
        // porque los drivers de QA sintetizaban `KeyCode` y en el celu no hay teclado que
        // los traduzca. Ya no hace falta: los drivers escriben `TouchActions` directamente
        // — ver `debug::autodrive::Accion`, "actuación neutra" —, que es el mismo camino
        // que recorre el dedo de una persona.)
        app.add_systems(
            Update,
            (actions::update_use_button_label, actions::usar_objeto_con_boton)
                .chain()
                .after(actions::poll_action_buttons_held),
        );
        app.add_systems(
            Update,
            (
                    layout::sync_touch_layout,
                    sync_stick_anchors,
                    actions::ensure_action_buttons,
                    actions::sync_use_button_active,
                    actions::layout_action_buttons,
                    actions::poll_action_buttons_held,
                    actions::update_jump_button_label,
                    keyboard_input,
                    super::gamepad::gamepad_input,
                    // MUST run after the hotbar (and editor) rects are appended
                    // to `TouchBlockRects`, or the sticks steal hotbar taps.
                    touch_move_stick
                        .after(actions::poll_action_buttons_held)
                        .after(crate::hud::append_hud_touch_blocks),
                    smooth_movement_intent,
                    update_joystick_ui,
                )
                    .chain(),
            );
        app.add_systems(
            Update,
            (
                super::gamepad::gamepad_hold_interact
                    ,
                super::gamepad::gamepad_menu_buttons,
            ),
        );
    }
}

fn sync_stick_anchors(
    layout: Res<TouchLayout>,
    config: Res<UiLayoutConfig>,
    mut state: ResMut<StickState>,
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
    state: Res<StickState>,
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
    pinch: Res<crate::camera::PinchActive>,
    mut touches: EventReader<TouchInput>,
    mut state: ResMut<StickState>,
    look: Res<CameraLookIntent>,
    mut raw: ResMut<RawMovementIntent>,
) {
    // Desktop: WASD via `keyboard_input` only — never zero stick output over keys.
    if !crate::platform::uses_touch_gameplay_ui() {
        return;
    }
    // Mismo motivo, del otro lado: con un driver de QA manejando, este sistema corre
    // DESPUÉS de `keyboard_input` en la cadena y, sin dedos en la pantalla, le escribía
    // ceros encima al movimiento sintético. Nadie está tocando el celu durante una corrida
    // automática, así que ceder el stick es exactamente lo correcto.
    if crate::flags::driver_active() {
        return;
    }
    if editor.enabled || editor.menu_open || gameplay_block.blocked {
        return;
    }
    // Pinch-zoom activo (dos dedos en la banda superior): soltar el stick si uno
    // de esos dedos lo había reclamado, y no reclamar dedos del gesto.
    let pinch_ids = pinch.ids;
    let in_pinch = |id: u64| pinch_ids.map(|(a, b)| id == a || id == b).unwrap_or(false);
    if let Some(id) = state.move_stick.pointer_id {
        if in_pinch(id) {
            state.move_stick.active = false;
            state.move_stick.pointer_id = None;
            state.move_stick.current = state.move_stick.anchor;
            raw.move_x = 0.0;
            raw.move_z = 0.0;
            raw.sprint = false;
        }
    }
    let r = (layout.radius * config.slot(UiEditable::MoveStick).scale).max(1.0);
    let move_sens = config.calibration.movement_sensitivity;
    let curve = config.calibration.stick_curve_exponent;

    for ev in touches.read() {
        let pos = ev.position;
        match ev.phase {
            TouchPhase::Started => {
                if actions::touch_hits_action_button(pos, &button_rects) || in_pinch(ev.id) {
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

pub(crate) fn smooth_movement_intent(
    time: Res<Time<Virtual>>,
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
pub struct JoystickBase;

fn spawn_stick(
    commands: &mut Commands,
    // `ImageNode` ya armado (no un handle): así trae el RECORTE del atlas si el
    // slot `ui/*` se curó desde una hoja — con el handle a secas el joystick se
    // dibujaba con el atlas entero.
    base_img: Option<ImageNode>,
    knob_img: Option<ImageNode>,
) {
    // **Con arte del pack, SÓLO el arte.** El disco de color, el borde y el
    // fondo blanco del pomo se pintaban igual DEBAJO de la imagen, que no llena
    // su nodo (el zócalo del pack ocupa ~80 % del cuadro): se veían tres círculos
    // desparejos —uno liso, uno pixelado y un aro blanco alrededor del pomo—.
    // El color queda sólo como reserva cuando el slot no tiene arte.
    let con_arte = base_img.is_some();
    let mut base = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            display: Display::Flex,
            border: UiRect::all(Val::Px(if con_arte { 0.0 } else { 3.2 })),
            ..default()
        },
        BorderColor(if con_arte { Color::NONE } else { ui_theme::joystick_border(false) }),
        BackgroundColor(if con_arte { Color::NONE } else { ui_theme::joystick_base() }),
        BorderRadius::MAX,
        JoystickBase,
        crate::game_ui::GameplayUi,
    ));
    if let Some(img) = base_img {
        base.insert((img.with_color(tinte_base(false)), JoystickConArte));
    }
    base.with_children(|p| {
        let mut knob = p.spawn((
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            BorderRadius::MAX,
            JoystickKnob,
        ));
        match knob_img {
            Some(img) => {
                knob.insert((img, BackgroundColor(Color::NONE)));
            }
            None => {
                knob.insert(BackgroundColor(ui_theme::joystick_knob()));
            }
        }
    });
}

/// El zócalo se dibuja con el arte del pack (no con el color de reserva).
#[derive(Component)]
struct JoystickConArte;

/// Con arte, "lo estás tocando" se lee porque el zócalo se enciende: medio
/// transparente en reposo (no tapa el mundo), opaco con el pulgar encima.
fn tinte_base(activo: bool) -> Color {
    Color::srgba(1.0, 1.0, 1.0, if activo { 0.95 } else { 0.62 })
}

fn spawn_joystick_ui(mut commands: Commands, art: Res<crate::art2d::ArtLibrary>) {
    if !crate::platform::uses_touch_gameplay_ui() {
        return;
    }
    let base = art.ui_image_node("ui/joystick_base");
    let knob = art.ui_image_node("ui/joystick_knob");
    spawn_stick(&mut commands, base, knob);
}

fn update_joystick_ui(
    layout: Res<TouchLayout>,
    config: Res<UiLayoutConfig>,
    state: Res<StickState>,
    mut bases: Query<
        (&mut Node, &mut BorderColor, Option<&mut ImageNode>, Has<JoystickConArte>),
        (With<JoystickBase>, Without<JoystickKnob>),
    >,
    mut knobs: Query<&mut Node, (With<JoystickKnob>, Without<JoystickBase>)>,
) {
    if !crate::platform::uses_touch_gameplay_ui() {
        return;
    }
    let slot = config.slot(UiEditable::MoveStick);
    let r = (layout.radius * slot.scale).max(1.0);
    let knob_r = (layout.knob_radius * slot.scale).max(8.0);
    let win_h = layout.height;

    // `bypass_change_detection` + `set_changed` a mano: los pintores escriben los
    // MISMOS píxeles casi todos los frames, y con el `DerefMut` normal cada
    // llamada marcaría `Node`/`BorderColor` como cambiado y Bevy relayoutearía el
    // árbol de UI completo por frame. Ver `set_px`.
    for (mut node, mut border, img, con_arte) in &mut bases {
        if paint_base(
            &state.move_stick,
            r,
            win_h,
            con_arte,
            node.bypass_change_detection(),
            border.bypass_change_detection(),
        ) {
            node.set_changed();
            border.set_changed();
        }
        if let Some(mut img) = img {
            let want = tinte_base(state.move_stick.active);
            if img.color != want {
                img.color = want;
            }
        }
    }
    for mut node in &mut knobs {
        if paint_knob(&state.move_stick, r, knob_r, node.bypass_change_detection()) {
            node.set_changed();
        }
    }
}

/// Escribe un `Val::Px` SÓLO si cambió.
///
/// No es micro-optimización: tocar un campo de `Node` marca el componente como
/// cambiado, y Bevy re-corre el layout (taffy) del árbol de UI entero. Estos
/// pintores corren cada frame, así que escribir siempre los mismos píxeles
/// forzaba un relayout completo por frame — con el HUD del juego (barras, hotbar,
/// botones, paneles) eso es un mordisco de CPU constante, y cuando el frame ya va
/// justo (volando, en el espacio, con el streaming de chunks trabajando) se ve
/// como que la UI TIEMBLA. Con esto, un stick quieto no cuesta nada.
pub(crate) fn set_px(slot: &mut Val, px: f32) -> bool {
    let next = Val::Px(px);
    if *slot == next {
        return false;
    }
    *slot = next;
    true
}

/// `true` si algo cambió de verdad (el llamador marca el componente).
fn paint_base(
    side: &JoystickSide,
    r: f32,
    win_h: f32,
    con_arte: bool,
    node: &mut Node,
    border: &mut BorderColor,
) -> bool {
    let mut dirty = false;
    if node.display != Display::Flex {
        node.display = Display::Flex;
        dirty = true;
    }
    let (left, bottom) = layout::base_ui_pos(side.anchor, r, win_h);
    dirty |= set_px(&mut node.left, left);
    dirty |= set_px(&mut node.bottom, bottom);
    dirty |= set_px(&mut node.width, r * 2.0);
    dirty |= set_px(&mut node.height, r * 2.0);
    let want = if con_arte { Color::NONE } else { ui_theme::joystick_border(side.active) };
    if border.0 != want {
        border.0 = want;
        dirty = true;
    }
    dirty
}

/// `true` si algo cambió de verdad (el llamador marca el componente).
fn paint_knob(side: &JoystickSide, r: f32, knob_r: f32, node: &mut Node) -> bool {
    let delta = clamped_delta(side, r);
    let (left, bottom) = layout::knob_offset_in_base(delta, r, knob_r);
    let mut dirty = set_px(&mut node.width, knob_r * 2.0);
    dirty |= set_px(&mut node.height, knob_r * 2.0);
    dirty |= set_px(&mut node.left, left);
    dirty |= set_px(&mut node.bottom, bottom);
    dirty
}
