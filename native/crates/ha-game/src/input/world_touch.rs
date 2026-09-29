//! **Tocar el mundo ES la interacción** (Minecraft Pocket, aplicado al cenital).
//!
//! No hay mira. No hay stick derecho. No hay botón USAR. El dedo va directo al
//! objeto: tocás el árbol y lo talás, tocás la piedra y la picás, con un bloque
//! en la mano tocás el suelo y lo colocás. El gesto es UNO solo y el contexto
//! decide qué significa — igual que el clic de Minecraft, que nunca preguntó si
//! querías picar o construir.
//!
//! Por qué se fue la mira: era un intermediario. Con el stick derecho había que
//! **conducir** un cursor hasta el objeto y recién ahí apretar USAR — dos
//! controles, dos pasos y dos cosas más en pantalla para hacer lo que el dedo ya
//! sabía hacer solo. Y en un táctil el dedo YA es el puntero.
//!
//! Reglas del gesto (`world_touch_gesture`):
//!
//! 1. **Hay algo tocado** (árbol, roca, veta, cofre, módulo…) → el mantener
//!    arranca EN EL ACTO, sin retardo: la barra de progreso empieza en el mismo
//!    frame en que apoyás el dedo. Soltar antes cancela.
//! 2. **Con algo colocable en la mano y celda libre** → lo colocás ahí.
//! 3. **Sin objetivo**, lo que llevás en la mano decide (comer, plantar cofre /
//!    antorcha / mapa / máquina). Esto SÍ pide un toque sostenido
//!    (`GESTURE_HOLD_START_MS`), porque el toque corto ya significa "golpear".
//! 4. **Toque corto que no interactuó con nada** → habilidad de la clase (el
//!    golpe). Es lo único que quedó del viejo "tap" del stick.
//!
//! Arrastrar el dedo mueve el punto de interacción: cancela lo que estabas
//! haciendo y re-engancha en lo nuevo (así se pica una veta entera o se coloca
//! una hilera de bloques sin levantar el dedo).

use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use ha_shared::{
    is_flight_hold_target, resolve_interaction, InteractActionKind,
    GESTURE_FLIGHT_MOVE_CANCEL_PX, GESTURE_HOLD_START_MS, GESTURE_MACHINE_HOLD_DELAY_MS,
    GESTURE_MOVE_CANCEL_PX, GESTURE_TAP_MAX_MS,
};

use super::actions::{self, ActionButtonRects};
use super::layout::{self, TouchBlockRects, TouchLayout};
use super::look_interact::{
    AbilityCooldown, AbilityTapPulse, BuildModeActive, EquippedItem, HoldInteractState,
};
use crate::camera::{GameCamera, PinchActive};
use crate::game_ui::UiGameplayBlock;
use crate::machines::PendingMachinePlacement;
use crate::targeting::LookTarget;
use crate::ui_editor::UiEditorState;

/// Punto del mundo sobre el que actúa el jugador: **debajo del dedo** (o del
/// mouse en escritorio). Conserva el nombre histórico porque medio motor lo lee
/// (targeting, resaltado de celda, ghost de construcción, sociedad), pero ya no
/// es una mira que se conduce: es dónde estás tocando.
#[derive(Resource)]
pub struct AimCursor {
    /// Suelo del mundo `(x, z)`.
    pub world: Vec2,
    /// Alguna vez hubo un punto válido (antes del primer toque no hay nada).
    pub initialized: bool,
    /// Punto FIJADO por código (drivers de QA / autodrive): ni el dedo ni el
    /// mouse lo mueven. Mismo idioma que `LookTarget::locked`.
    pub locked: bool,
    /// **Hay un dedo apoyado en el mundo, al alcance del brazo.** Cuando es
    /// `false` no hay objetivo ni resaltado: la pantalla queda limpia, que es
    /// justamente lo que se ganó al sacar la mira.
    pub active: bool,
}

impl Default for AimCursor {
    fn default() -> Self {
        Self {
            world: Vec2::ZERO,
            initialized: false,
            locked: false,
            active: false,
        }
    }
}

impl AimCursor {
    /// ¿Se debe resolver un objetivo con este punto? Los drivers de QA fijan
    /// `locked` sin dedo ni mouse, y ahí el punto vale igual (por eso `locked`
    /// va primero, sin pedir `initialized`: lo acaban de escribir a mano).
    pub fn targeting(&self) -> bool {
        self.locked || (self.initialized && self.active)
    }
}

/// **Dónde se coloca lo que llevás en la mano.** El punto tocado si lo hay; si
/// no (mando, atajo de teclado, driver de QA), `ahead_m` metros al frente, que es
/// como se colocaba todo antes de que el dedo pudiera elegir.
pub fn place_point(cursor: &AimCursor, sim: &crate::sim::SimState, ahead_m: f32) -> (f32, f32) {
    if cursor.targeting() {
        return (cursor.world.x, cursor.world.y);
    }
    (
        sim.body.x + sim.body.rot.cos() * ahead_m,
        sim.body.z + sim.body.rot.sin() * ahead_m,
    )
}

/// Alcance del brazo (m): más allá de esto, tocar no hace nada. Es el MISMO
/// número que usa `targeting` para los tiles, a propósito — si el resaltado
/// llegara más lejos que la acción, el jugador vería marcado algo que no puede
/// tocar, que es la peor forma de explicar un límite.
pub const AIM_MAX_REACH: f32 = 5.6;

/// Cuánto tarda en re-armarse una acción REPETIBLE (picar/colocar en cadena)
/// tras completar la anterior sin levantar el dedo.
const REPEAT_DELAY_MS: f32 = 90.0;

/// Margen extra (s) para que un `completed` sin consumir sobreviva a que el
/// jugador levante el dedo justo al terminar. Ver `release_pending`.
const RELEASE_GRACE_S: f32 = 0.35;

#[derive(Resource, Default)]
struct WorldTouchRuntime {
    /// Dedo que está tocando el mundo (uno solo: el resto son sticks/HUD).
    pointer: Option<u64>,
    /// Píxel donde arrancó el intento ACTUAL (se re-ancla al arrastrar).
    anchor: Vec2,
    last: Vec2,
    start_ms: f32,
    /// El dedo se arrastró: nunca dispara el golpe del toque corto.
    dragged: bool,
    /// Este contacto ya armó al menos una acción: tampoco dispara el golpe.
    interacted: bool,
    /// **Un contacto, una intención.** `Some(true)` = este dedo está COLOCANDO
    /// (o usando lo que lleva en la mano); `Some(false)` = está actuando sobre lo
    /// tocado (picar, talar, abrir). No se cambia de bando sin levantar el dedo:
    /// si no, arrastrar una hilera de bloques sobre un muro existente lo empezaba
    /// a demoler, y colocar un bloque encadenaba con "picar el que acabo de
    /// poner" — el objetivo cambia solo, la intención no.
    class: Option<bool>,
    /// Identidad de lo último sobre lo que se actuó: la cadena sólo avanza a un
    /// objetivo DISTINTO. Es lo que evita comerse tres manzanas de un toque o
    /// gastar madera colocando el mismo piso una y otra vez.
    last_key: u64,
    /// Momento en que terminará la acción en curso (para `REPEAT_DELAY_MS`).
    last_done_ms: f32,
    /// El dedo se levantó pero quedó un `completed` sin consumir: se sostiene el
    /// punto unos frames para que su consumidor todavía vea el objetivo.
    release_pending: bool,
    release_t: f32,
    /// Toque corto recién soltado que no llegó a interactuar con nada: lo
    /// consume el gesto para tirar el golpe de la clase.
    tap_pending: bool,
}

pub struct WorldTouchPlugin;

impl Plugin for WorldTouchPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AimCursor>()
            .init_resource::<WorldTouchRuntime>()
            .add_systems(
                Update,
                track_world_touch
                    .in_set(super::InputSet::Pointer)
                    .after(actions::poll_action_buttons_held)
                    .after(crate::hud::append_hud_touch_blocks),
            )
            .add_systems(
                Update,
                gamepad_aim_cursor
                    .in_set(super::InputSet::Pointer)
                    .after(super::gamepad::gamepad_input),
            )
            .add_systems(Update, world_touch_gesture.in_set(super::InputSet::Gesture));
    }
}

/// Con MANDO no hay dedo que apoyar, así que el punto se para solo un par de
/// metros al frente y se queda ahí: el stick derecho gira el rumbo y el objetivo
/// va con él. Es el equivalente honesto del "mirar hacia" de un mando, y sin esto
/// un pad no podría talar nada — el objetivo sólo existe mientras algo apunta.
fn gamepad_aim_cursor(
    pad: Res<super::gamepad::GamepadActive>,
    runtime: Res<WorldTouchRuntime>,
    look: Res<super::CameraLookIntent>,
    player: Query<&crate::sim::SimState, With<crate::sim::Player>>,
    mut cursor: ResMut<AimCursor>,
) {
    if !pad.0 || cursor.locked || runtime.pointer.is_some() {
        return;
    }
    let Ok(sim) = player.get_single() else {
        return;
    };
    let forward = crate::camera::look_forward_flat(&look);
    cursor.world = Vec2::new(sim.body.x, sim.body.z) + forward * GAMEPAD_AIM_AHEAD;
    cursor.initialized = true;
    cursor.active = true;
}

/// A qué distancia se para el punto del mando (m).
const GAMEPAD_AIM_AHEAD: f32 = 2.2;

/// ¿Este píxel es "mundo"? Todo lo que no sea HUD, botones o el stick de mover.
fn touch_is_world(
    pos: Vec2,
    layout: &TouchLayout,
    blocks: &TouchBlockRects,
    buttons: &ActionButtonRects,
) -> bool {
    !actions::touch_hits_action_button(pos, buttons)
        && !layout::touch_blocked(pos, &blocks.0)
        && !layout::touch_in_move_zone(pos, layout)
}

/// Sigue al dedo que toca el mundo y publica su punto en `AimCursor`.
#[allow(clippy::too_many_arguments)]
fn track_world_touch(
    time: Res<Time>,
    layout: Res<TouchLayout>,
    blocks: Res<TouchBlockRects>,
    buttons: Res<ActionButtonRects>,
    editor: Res<UiEditorState>,
    gameplay_block: Res<UiGameplayBlock>,
    pinch: Res<PinchActive>,
    fly: Res<crate::creative_fly::CreativeFlight>,
    hold: Res<HoldInteractState>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    player: Query<&crate::sim::SimState, With<crate::sim::Player>>,
    mut touches: EventReader<TouchInput>,
    mut runtime: ResMut<WorldTouchRuntime>,
    mut cursor: ResMut<AimCursor>,
) {
    if !crate::platform::uses_touch_gameplay_ui() {
        return;
    }
    if cursor.locked {
        touches.clear();
        return;
    }
    // Con un panel abierto (mochila, menú, editor) el mundo no se toca, y
    // volando el dedo no interactúa con el suelo.
    if editor.enabled || editor.menu_open || gameplay_block.blocked || fly.flying() {
        touches.clear();
        release(&mut runtime, &mut cursor);
        return;
    }
    // Pellizco de zoom: los dos dedos del gesto son de la cámara, no del mundo.
    let in_pinch = |id: u64| pinch.ids.map(|(a, b)| id == a || id == b).unwrap_or(false);
    if let Some(id) = runtime.pointer {
        if in_pinch(id) {
            release(&mut runtime, &mut cursor);
        }
    }

    let now_ms = time.elapsed_secs() * 1000.0;
    let mut lifted = false;
    for ev in touches.read() {
        let pos = ev.position;
        match ev.phase {
            TouchPhase::Started => {
                if runtime.pointer.is_some() || in_pinch(ev.id) {
                    continue;
                }
                if !touch_is_world(pos, &layout, &blocks, &buttons) {
                    continue;
                }
                runtime.pointer = Some(ev.id);
                runtime.anchor = pos;
                runtime.last = pos;
                runtime.start_ms = now_ms;
                runtime.dragged = false;
                runtime.interacted = false;
                runtime.class = None;
                runtime.last_key = 0;
                runtime.last_done_ms = 0.0;
                runtime.release_pending = false;
            }
            TouchPhase::Moved => {
                if runtime.pointer == Some(ev.id) {
                    runtime.last = pos;
                }
            }
            TouchPhase::Ended | TouchPhase::Canceled => {
                if runtime.pointer == Some(ev.id) {
                    runtime.pointer = None;
                    lifted = true;
                    // El golpe se decide al SOLTAR: apoyar el dedo sobre un
                    // árbol tiene que empezar a talar, no repartir un hachazo al
                    // aire por si acaso.
                    runtime.tap_pending = !runtime.dragged
                        && !runtime.interacted
                        && (now_ms - runtime.start_ms) < GESTURE_TAP_MAX_MS as f32;
                }
            }
        }
    }

    if lifted {
        // Levantar el dedo NO borra el punto al instante si quedó una acción
        // recién completada sin consumir: su consumidor (cosecha, colocación)
        // lee `LookTarget` en el frame siguiente y sin punto no encontraría nada
        // — el hachazo se perdía justo cuando terminabas de darlo.
        if hold.completed {
            runtime.release_pending = true;
            runtime.release_t = RELEASE_GRACE_S;
        } else {
            release(&mut runtime, &mut cursor);
        }
    }
    if runtime.release_pending {
        runtime.release_t -= time.delta_secs();
        if !hold.completed || runtime.release_t <= 0.0 {
            release(&mut runtime, &mut cursor);
        }
    }

    let Some(_) = runtime.pointer else {
        return;
    };
    // Píxel → mundo. Vía `window_to_world_2d` (y no `viewport_to_world_2d` a
    // secas) porque con escala de render la cámara dibuja contra una imagen más
    // chica que la ventana: ver `render::scale`.
    let (Ok(win), Ok((cam, cam_tf))) = (window.get_single(), camera.get_single()) else {
        return;
    };
    let Some(render) = crate::render::scale::window_to_world_2d(cam, cam_tf, win, runtime.last)
    else {
        return;
    };
    // Pantalla `(x, -z)` → mundo `(x, z)`.
    let world = Vec2::new(render.x, -render.y);
    cursor.world = world;
    cursor.initialized = true;
    // Fuera del alcance del brazo el toque no vale: sin objetivo, sin resaltado.
    // (El punto se publica igual — el golpe del toque corto sí llega hasta ahí.)
    cursor.active = player
        .get_single()
        .map(|sim| Vec2::new(sim.body.x, sim.body.z).distance(world) <= AIM_MAX_REACH)
        .unwrap_or(false);
}

fn release(runtime: &mut WorldTouchRuntime, cursor: &mut AimCursor) {
    runtime.pointer = None;
    runtime.release_pending = false;
    runtime.interacted = false;
    runtime.class = None;
    runtime.last_key = 0;
    cursor.active = false;
}

/// El gesto: decide y sostiene la acción del dedo. Corre DESPUÉS del targeting
/// (ver `InputSet`), así el primer frame del contacto ya sabe qué tocaste.
#[allow(clippy::too_many_arguments)]
fn world_touch_gesture(
    time: Res<Time>,
    target: Res<LookTarget>,
    equipped: Res<EquippedItem>,
    build_mode: Res<BuildModeActive>,
    pending: Res<PendingMachinePlacement>,
    cursor: Res<AimCursor>,
    mut runtime: ResMut<WorldTouchRuntime>,
    mut hold: ResMut<HoldInteractState>,
    mut cooldown: ResMut<AbilityCooldown>,
    mut tap: ResMut<AbilityTapPulse>,
    mut actions: ResMut<super::TouchActions>,
) {
    tap.slot = None;
    if !crate::platform::uses_touch_gameplay_ui() || cursor.locked {
        return;
    }
    let now_ms = time.elapsed_secs() * 1000.0;

    // Golpe de la clase: toque corto que no interactuó con nada.
    if runtime.tap_pending {
        runtime.tap_pending = false;
        if cooldown.remaining_ms <= 0.0 {
            let slot = equipped.class.tap_ability_slot;
            tap.slot = Some(slot);
            cooldown.slot = slot;
            cooldown.total_ms = tap_cooldown_ms(&equipped.class);
            cooldown.remaining_ms = cooldown.total_ms;
            // El bit va también a `TouchActions`: quien lo lee para la sim
            // (`poll_action_buttons_held`) corre ANTES que este gesto, así que si
            // no se escribe acá el golpe nunca llegaría a la simulación.
            if slot < ha_shared::input::ABILITY_SLOTS {
                actions.abilities |= 1 << slot;
            }
        }
    }

    let Some(_) = runtime.pointer else {
        // Dedo levantado: cancelar lo que quedara a medias.
        hold.release(runtime.dragged);
        return;
    };

    // ── Arrastre: mueve el punto de acción. Cancela lo de antes y re-ancla, así
    // el mismo dedo sigue picando la veta de al lado o colocando la hilera.
    let drift_limit = if hold.flight_hold {
        GESTURE_FLIGHT_MOVE_CANCEL_PX
    } else {
        GESTURE_MOVE_CANCEL_PX
    };
    if (runtime.last - runtime.anchor).length() > drift_limit {
        runtime.dragged = true;
        runtime.anchor = runtime.last;
        runtime.start_ms = now_ms;
        // El respiro de la cadena se cuenta desde ACÁ: si siguiera contando desde
        // el final teórico de la acción abortada, arrastrar hasta el árbol de al
        // lado se quedaba esperando el hachazo entero que ya no va a pasar.
        runtime.last_done_ms = now_ms;
        if hold.active && !hold.completed {
            hold.cancelled = true;
            hold.active = false;
            hold.progress_ms = 0;
        }
    }

    if hold.active || hold.completed {
        return;
    }
    if !cursor.active {
        return;
    }
    // ── Encadenar sin levantar el dedo (picar una veta entera, colocar una
    // hilera). Tres condiciones, y las tres hacen falta: que haya pasado el
    // respiro, que la intención sea la MISMA, y que el objetivo sea OTRO.
    if runtime.interacted && now_ms - runtime.last_done_ms < REPEAT_DELAY_MS {
        return;
    }
    let held_ms = now_ms - runtime.start_ms;
    let Some(next) = resolve_hold(&target, &equipped, build_mode.active, &pending, held_ms) else {
        return;
    };
    let hand_class = next.is_placement() || next.eating;
    if runtime.class.map(|c| c != hand_class).unwrap_or(false) {
        return;
    }
    let key = target_key(&target);
    if runtime.interacted && key == runtime.last_key {
        return;
    }
    runtime.interacted = true;
    runtime.class = Some(hand_class);
    runtime.last_key = key;
    // El próximo eslabón no puede armarse antes de que ÉSTE termine: se cuenta
    // desde su final, no desde ahora.
    runtime.last_done_ms = now_ms + next.duration_ms as f32;
    *hold = next;
}

/// Identidad de lo apuntado, para no repetir la acción sobre lo mismo. Sin
/// objetivo devuelve 0 — y "nada" es siempre el mismo nada, así que plantar un
/// cofre o comer no se encadena solo.
fn target_key(target: &LookTarget) -> u64 {
    if let Some((x, z, y)) = target.block_cell {
        return cell_bits(1, x, z, y);
    }
    if let Some((x, z, y)) = target.place_cell {
        return cell_bits(2, x, z, y);
    }
    if let Some(e) = target.entity {
        // Los bits de una entidad ya son únicos; no hace falta etiquetarlos.
        return e.to_bits();
    }
    0
}

/// Etiqueta + celda empaquetadas (20 bits por eje: de sobra para el mundo
/// streameado, y sin desbordes silenciosos al desplazar).
fn cell_bits(tag: u64, x: i32, z: i32, y: i32) -> u64 {
    const M: u64 = 0xF_FFFF;
    tag << 60 | (x as i64 as u64 & M) | (z as i64 as u64 & M) << 20 | (y as i64 as u64 & M) << 40
}

fn tap_cooldown_ms(class: &ha_shared::ClassDef) -> f32 {
    match class.tap_ability_slot {
        0 => 650.0,
        1 => 2200.0,
        _ => 900.0,
    }
}

/// **Qué significa este toque.** Mismo orden de prioridades que el clic de
/// Minecraft: primero lo que hay bajo el dedo, después lo que llevás en la mano.
pub(crate) fn resolve_hold(
    target: &LookTarget,
    equipped: &EquippedItem,
    build_mode: bool,
    pending: &PendingMachinePlacement,
    held_ms: f32,
) -> Option<HoldInteractState> {
    // 1. Modo CONSTRUIR: celda libre apuntada = pieza modular (cualquier herramienta).
    if build_mode && target.place_cell.is_some() {
        return Some(HoldInteractState {
            active: true,
            duration_ms: 500,
            placing_build: true,
            ..default()
        });
    }
    // 2. Bloque en la mano + celda libre → colocar ahí (donde tocaste).
    if ha_shared::is_block_item(&equipped.item_id) && target.place_cell.is_some() {
        return Some(HoldInteractState {
            active: true,
            duration_ms: 300,
            placing_block: true,
            ..default()
        });
    }
    // 2b. Semilla en la mano + celda sembrable → sembrar ahí (`granja.rs`).
    if crate::granja::Planta::de_semilla(&equipped.item_id).is_some() && target.place_cell.is_some() {
        return Some(HoldInteractState {
            active: true,
            duration_ms: 250,
            placing_crop: true,
            ..default()
        });
    }
    // 3. Algo tocado con regla propia → talar / picar / recoger / instalar.
    //    Arranca EN EL ACTO: el dedo ya está sobre el objeto, no hace falta
    //    ningún retardo para adivinar la intención.
    if let Some(kind) = target.kind {
        if let Some(rule) = resolve_interaction(Some(equipped.item_id.as_str()), kind) {
            if rule.kind != InteractActionKind::None && rule.duration_ms > 0 {
                return Some(HoldInteractState {
                    active: true,
                    duration_ms: rule.duration_ms,
                    flight_hold: is_flight_hold_target(kind),
                    locked_kind: Some(kind),
                    ..default()
                });
            }
        }
        // Hay objetivo pero la herramienta no sirve: NO inventar otra acción
        // (colocar una antorcha "a través" del árbol que querías talar).
        return None;
    }

    // 4. Sin objetivo: manda lo que llevás en la mano, y se coloca donde tocaste.
    //    Acá sí se pide sostener: el toque corto ya significa golpear.
    if held_ms < GESTURE_HOLD_START_MS as f32 {
        return None;
    }
    if crate::torches::torch_variant(&equipped.item_id).is_some() {
        // Corto a propósito (250 ms): iluminar una cueva es ir dejando antorchas
        // al caminar, y un mantener largo convierte eso en una tarea.
        return Some(HoldInteractState {
            active: true,
            duration_ms: 250,
            placing_torch: true,
            ..default()
        });
    }
    if crate::map_item::map_kind(&equipped.item_id).is_some() {
        return Some(HoldInteractState {
            active: true,
            duration_ms: 400,
            placing_map: true,
            ..default()
        });
    }
    if equipped.item_id == crate::chests::CHEST_ITEM {
        return Some(HoldInteractState {
            active: true,
            duration_ms: 450,
            placing_chest: true,
            ..default()
        });
    }
    if ha_shared::food_restore(&equipped.item_id).is_some() {
        return Some(HoldInteractState {
            active: true,
            duration_ms: 1100,
            eating: true,
            ..default()
        });
    }
    if equipped.item_id == "hammer"
        && !build_mode
        && held_ms >= GESTURE_MACHINE_HOLD_DELAY_MS as f32
    {
        let def = ha_shared::require_machine(pending.machine_id);
        return Some(HoldInteractState {
            active: true,
            duration_ms: def.place_duration_ms,
            placing_machine: true,
            placing_stone: pending.machine_id == ha_shared::STONE_EXTRACTOR.id,
            ..default()
        });
    }
    None
}
