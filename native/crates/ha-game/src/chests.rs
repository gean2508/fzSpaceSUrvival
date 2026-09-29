//! Cofres: almacenamiento colocable por el jugador.
//!
//! Un cofre se craftea (`recipes.rs` id 20), se equipa desde la hotbar y se
//! planta delante tuyo manteniendo USE. Guarda 20 slots propios, persiste en
//! `chests.json` (mismo patrón flush-on-pause que `machines.json`) y se abre
//! apuntándolo y manteniendo USE.
//!
//! El panel es **tap-to-transfer**: tocás un slot del cofre y su pila pasa a tu
//! inventario; tocás uno de tu inventario y pasa al cofre. Sin arrastre = sin
//! estados intermedios donde una pila pueda perderse (el mismo criterio con el
//! que se arregló el arrastre de la mochila en `hud/inventory_ui.rs`).

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use ha_shared::{body_def, CelestialBody, InteractTargetKind};
use serde::{Deserialize, Serialize};

use crate::art2d::{ArtAnimated, ArtLibrary};
use crate::game_ui::GameUiRoot;
use crate::hud::inventory_ui::{
    add_item_to_slots_rest, flush_slots_to_inventory, InventoryUiState, SlotId, Stack, GRID_LEN,
    HOTBAR_ITEM_SLOTS,
};
use crate::hud::item_icons::ItemIconCache;
use crate::input::{EquippedItem, HoldInteractState, TouchLayout};
use crate::inventory_persist::{persist_dir, InventoryPersistState, PlayerInventory};
use crate::machines::PlacementCtx;
use crate::platform::is_app_paused;
use crate::sim::{Player, SimState};
use crate::targeting::{InteractTarget, LookTarget};
use crate::ui_theme::{self, UiButtonState, UiButtonTone};
use crate::world2d::{render_xy, sort_z, TileWorld};

/// Id de ítem del cofre (en la hotbar y en el catálogo de recetas).
pub const CHEST_ITEM: &str = "chest";
/// Capacidad de un cofre: 4 columnas × 5 filas.
pub const CHEST_COLS: usize = 4;
pub const CHEST_ROWS: usize = 5;
pub const CHEST_SLOTS: usize = CHEST_COLS * CHEST_ROWS;
/// Distancia a la que el panel se cierra solo si te alejás (m).
const CHEST_USE_RANGE: f32 = 6.0;

/// Slot de arte del cofre. Si en el editor le guardás las poses `closed` y
/// `open` (cada una puede ser una TIRA de frames), el juego las conmuta solo.
const CHEST_ART: &str = "props/chest";

/// Cofre con arte de poses: recuerda el estado que ya está dibujado para tocar
/// el sprite SÓLO en el cambio (abrir/cerrar), nunca por frame.
#[derive(Component)]
struct ChestArt {
    open: bool,
}

pub struct ChestsPlugin;

impl Plugin for ChestsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChestWorld>()
            .init_resource::<ChestPanelState>()
            .add_systems(OnEnter(crate::game_phase::GamePhase::Loading), load_chests_on_startup)
            .add_systems(
                Update,
                (
                    place_chest_on_hold,
                    use_or_pick_up_chest.after(place_chest_on_hold),
                    sync_chest_visuals,
                    animate_chest_art.after(sync_chest_visuals),
                    close_chest_when_far,
                    chest_panel_lifecycle,
                    scroll_chest_panel,
                    tick_chest_press,
                    chest_slot_tap_system.after(scroll_chest_panel),
                    player_slot_tap_system.after(scroll_chest_panel),
                    chest_panel_buttons,
                    update_chest_slot_visuals,
                    update_split_bar.after(tick_chest_press),
                    debounced_chests_save,
                ),
            );
    }
}

#[derive(Clone, Debug)]
pub struct LocalChest {
    pub uid: u32,
    pub body: CelestialBody,
    pub x: f32,
    pub z: f32,
    pub slots: Vec<Stack>,
}

impl LocalChest {
    fn is_empty(&self) -> bool {
        self.slots.iter().all(|s| s.is_empty())
    }
}

#[derive(Resource, Default)]
pub struct ChestWorld {
    next_uid: u32,
    pub placed: Vec<LocalChest>,
    dirty: bool,
    was_paused: bool,
    debounce: f32,
}

impl ChestWorld {
    fn get_mut(&mut self, uid: u32) -> Option<&mut LocalChest> {
        self.placed.iter_mut().find(|c| c.uid == uid)
    }

    fn get(&self, uid: u32) -> Option<&LocalChest> {
        self.placed.iter().find(|c| c.uid == uid)
    }
}

/// Qué slot está bajo el dedo ahora mismo. La transferencia ocurre al SOLTAR,
/// no al tocar: el panel se desplaza arrastrando, y mover la pila en el
/// `Pressed` haría que cada scroll robara un ítem sin querer.
#[derive(Clone, Copy, PartialEq)]
enum PressedSlot {
    Chest(usize),
    Player(SlotId),
}

/// Píxeles de arrastre que convierten un toque en "estaba haciendo scroll".
const PRESS_DRAG_CANCEL_PX: f32 = 12.0;

/// A partir de acá el toque deja de ser "pasá todo" y empieza a MEDIR.
const SPLIT_HOLD_SECS: f32 = 0.22;
/// Cuánto tarda la barra en recorrer la pila entera desde que empieza a medir.
/// 1.1 s es suficiente para soltar donde querés sin que se sienta lento.
const SPLIT_SWEEP_SECS: f32 = 1.1;

/// Panel abierto y sobre qué cofre.
#[derive(Resource, Default)]
pub struct ChestPanelState {
    pub open_uid: Option<u32>,
    pressed: Option<PressedSlot>,
    press_drag: f32,
    /// Segundos que lleva el dedo apoyado en `pressed`.
    press_secs: f32,
}

impl ChestPanelState {
    pub fn is_open(&self) -> bool {
        self.open_uid.is_some()
    }

    /// ¿El toque que termina sobre `slot` cuenta como tap (y no como scroll)?
    fn take_tap(&mut self, slot: PressedSlot) -> bool {
        if self.pressed != Some(slot) {
            return false;
        }
        self.pressed = None;
        let ok = self.press_drag < PRESS_DRAG_CANCEL_PX;
        self.press_drag = 0.0;
        self.press_secs = 0.0;
        ok
    }

    fn begin_press(&mut self, slot: PressedSlot) {
        self.pressed = Some(slot);
        self.press_drag = 0.0;
        self.press_secs = 0.0;
    }

    /// Fracción de la pila que se está seleccionando ahora mismo en `slot`
    /// (`None` = todavía es un toque simple, o el dedo está en otro lado).
    fn split_fraction(&self, slot: PressedSlot) -> Option<f32> {
        if self.pressed != Some(slot) || self.press_secs < SPLIT_HOLD_SECS {
            return None;
        }
        Some(((self.press_secs - SPLIT_HOLD_SECS) / SPLIT_SWEEP_SECS).clamp(0.0, 1.0))
    }
}

/// **Cuántas unidades mueve este gesto.**
///
/// - **Toque** (soltás rápido) → la pila ENTERA. Es el 90 % de las veces:
///   vaciar el cofre no puede costar una ceremonia.
/// - **Mantener** → una barra recorre la pila de 1 hasta el total y soltás donde
///   quieras. Empieza en 1 a propósito: "quiero sacar UN palito" es justo lo que
///   no se podía hacer, y es lo primero que ofrece el gesto.
///
/// Un solo gesto continuo cubre 1, la mitad y todo, sin diálogos ni un modo
/// aparte que haya que recordar.
fn split_qty(total: u32, fraction: Option<f32>) -> u32 {
    match fraction {
        None => total,
        Some(f) => {
            if total <= 1 {
                return total;
            }
            // 1 … total, redondeando al más cercano.
            (1.0 + f * (total - 1) as f32).round().clamp(1.0, total as f32) as u32
        }
    }
}

#[derive(Component)]
struct ChestEntity {
    uid: u32,
}

// --- Colocar ------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn place_chest_on_hold(
    mut hold: ResMut<HoldInteractState>,
    equipped: Res<EquippedItem>,
    ctx: PlacementCtx,
    cursor: Res<crate::input::AimCursor>,
    player: Query<&SimState, With<Player>>,
    mut world: ResMut<ChestWorld>,
    mut inv: ResMut<PlayerInventory>,
    mut inv_ui: ResMut<InventoryUiState>,
    mut persist: ResMut<InventoryPersistState>,
    editor: Res<crate::ui_editor::UiEditorState>,
) {
    if !hold.completed || !hold.placing_chest {
        return;
    }
    hold.completed = false;
    hold.placing_chest = false;

    if equipped.item_id != CHEST_ITEM {
        return;
    }
    let Some(body) = ctx.body() else {
        return;
    };
    let Ok(sim) = player.get_single() else {
        return;
    };
    if !editor.creative && inv.qty(CHEST_ITEM) == 0 {
        return;
    }

    // **Donde tocaste.** Con la mira vieja el cofre caía "dos metros al frente" y
    // orientarse era girar el personaje; ahora el punto lo elegís con el dedo.
    // Sin toque válido (mando, atajo) se planta al frente como siempre.
    let (x, z) = crate::input::place_point(&cursor, sim, 2.0);
    let uid = world.next_uid;
    world.next_uid += 1;
    world.placed.push(LocalChest {
        uid,
        body,
        x,
        z,
        slots: vec![Stack::default(); CHEST_SLOTS],
    });
    world.dirty = true;

    if !editor.creative {
        // Se descuenta de los slots visibles (fuente de verdad de la UI) y de ahí
        // se vuelca al inventario — al revés quedarían desincronizados.
        remove_from_ui_slots(&mut inv_ui, CHEST_ITEM, 1);
        flush_slots_to_inventory(&inv_ui, &mut inv);
        persist.mark_immediate();
    }
}

/// Quita `qty` unidades de `item_id` de los slots visibles (hotbar primero).
fn remove_from_ui_slots(state: &mut InventoryUiState, item_id: &str, qty: u32) {
    let mut remaining = qty;
    for stack in state.hotbar.iter_mut().chain(state.grid.iter_mut()) {
        if remaining == 0 {
            break;
        }
        if stack.item_id.as_deref() != Some(item_id) {
            continue;
        }
        let take = remaining.min(stack.qty);
        stack.qty -= take;
        remaining -= take;
        if stack.qty == 0 {
            *stack = Stack::default();
        }
    }
}

// --- Abrir / cerrar -----------------------------------------------------------

/// **Un objeto colocado tiene dos verbos, y el gesto los separa solo.**
///
/// - **Toque** → lo USÁS (el cofre se abre).
/// - **Mantener** → lo LEVANTÁS: vuelve a tu mochila y lo volvés a plantar donde
///   quieras.
///
/// Es la misma gramática con la que ya picás un bloque, así que no hay nada
/// nuevo que aprender: tocar es interactuar, mantener es desarmar. Antes ABRIR
/// era el mantenido y no existía forma de recuperar un cofre mal puesto sin
/// vaciarlo y buscarle el botón dentro del panel.
///
/// El cofre sólo se levanta VACÍO: si se llevara su contenido, el inventario
/// tendría que representar "un cofre con cosas adentro" y no puede — y perder
/// lo guardado por un gesto sería imperdonable. Con cosas dentro se avisa.
fn use_or_pick_up_chest(
    mut hold: ResMut<HoldInteractState>,
    target: Res<LookTarget>,
    targets: Query<&InteractTarget>,
    mut panel: ResMut<ChestPanelState>,
    mut world: ResMut<ChestWorld>,
    mut inv_ui: ResMut<InventoryUiState>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
) {
    if target.kind != Some(InteractTargetKind::Chest) {
        return;
    }
    let tapped = std::mem::take(&mut hold.tapped);
    let held = hold.completed;
    if !tapped && !held {
        return;
    }
    if held {
        hold.completed = false;
    }
    let Some(entity) = target.entity else { return };
    let Ok(t) = targets.get(entity) else { return };

    if tapped {
        panel.open_uid = Some(t.id);
        return;
    }
    // Mantenido: levantar.
    let Some(chest) = world.get(t.id) else { return };
    // Con cosas dentro no se levanta (se perderían) y con la mochila llena
    // tampoco (no habría dónde ponerlo): en los dos casos el cofre se queda
    // donde está, que es lo único que no puede salir mal.
    if !chest.is_empty() || add_item_to_slots_rest(&mut inv_ui, CHEST_ITEM, 1) != 0 {
        return;
    }
    world.placed.retain(|c| c.uid != t.id);
    world.dirty = true;
    if panel.open_uid == Some(t.id) {
        panel.open_uid = None;
    }
    flush_slots_to_inventory(&inv_ui, &mut inv);
    persist.mark_dirty();
}

/// Alejarse (o perder el cofre) cierra el panel: nunca queda una UI apuntando a
/// un almacenamiento que ya no está al alcance.
fn close_chest_when_far(
    world: Res<ChestWorld>,
    player: Query<&SimState, With<Player>>,
    mut panel: ResMut<ChestPanelState>,
) {
    let Some(uid) = panel.open_uid else {
        return;
    };
    let Ok(sim) = player.get_single() else {
        panel.open_uid = None;
        return;
    };
    let Some(chest) = world.get(uid) else {
        panel.open_uid = None;
        return;
    };
    let d = Vec2::new(chest.x - sim.body.x, chest.z - sim.body.z).length();
    if d > CHEST_USE_RANGE {
        panel.open_uid = None;
    }
}

// --- Visuales en el mundo ------------------------------------------------------

fn sync_chest_visuals(
    ctx: PlacementCtx,
    world: Res<ChestWorld>,
    tiles: Res<TileWorld>,
    existing: Query<(Entity, &ChestEntity)>,
    mut commands: Commands,
    mut icons: ResMut<ItemIconCache>,
    mut images: ResMut<Assets<Image>>,
    art: Res<ArtLibrary>,
) {
    let surface = tiles.z == 0;
    let body = if surface { ctx.body() } else { None };
    let by_uid: HashMap<u32, &LocalChest> = world.placed.iter().map(|c| (c.uid, c)).collect();
    let mut spawned: HashSet<u32> = HashSet::new();
    for (e, ce) in &existing {
        let keep = body.is_some()
            && by_uid
                .get(&ce.uid)
                .map(|c| Some(c.body) == body)
                .unwrap_or(false);
        if keep {
            spawned.insert(ce.uid);
        } else {
            commands.entity(e).despawn_recursive();
        }
    }
    let Some(body) = body else {
        return;
    };
    for c in world.placed.iter().filter(|c| c.body == body) {
        if spawned.contains(&c.uid) {
            continue;
        }
        // Con arte de poses (`closed`/`open` en el editor) arrancamos cerrado y
        // `animate_chest_art` se encarga de la transición; si no, la imagen del
        // slot entero (o el icono procedural) como siempre.
        let mut anim = None;
        // **El cofre mide lo que mide su recorte.** Estaba clavado en 1,1 m —un
        // número elegido a ojo— cuando su arte es de una celda del pack, o sea 1,5
        // m. Un cofre más chico que su pixel no es un cofre más discreto: es el
        // mundo entero a escalas distintas (ver `ArtLibrary::tamano_real`).
        let medida = art.tamano_real(CHEST_ART, Some("closed"), Vec2::splat(1.1));
        let mut sprite = art
            .pose_sprite(CHEST_ART, "closed", medida)
            .unwrap_or_else(|| {
                // `world_sprite` respeta el recorte de atlas del slot; con el
                // handle a secas se dibujaba la hoja entera. Y la animación va
                // CON el sprite: un slot con tira no lleva recorte propio (lo
                // pone `ArtAnimated` cuadro a cuadro), así que soltarla dibujaría
                // todos los cuadros a la vez encima del cofre.
                match art.world_sprite(CHEST_ART, medida) {
                    Some((s, a)) => {
                        anim = a;
                        s
                    }
                    None => icons.icon(CHEST_ITEM, &mut images).sprite(medida),
                }
            });
        sprite.custom_size = Some(medida);
        let r = render_xy(c.x, c.z);
        let mut ent = commands.spawn((
            sprite,
            Transform::from_xyz(r.x, r.y + 0.35, sort_z(c.z)),
            ChestEntity { uid: c.uid },
            ChestArt { open: false },
            InteractTarget {
                kind: InteractTargetKind::Chest,
                id: c.uid,
            },
            Name::new(format!("Chest_{}", c.uid)),
        ));
        if let Some(a) = anim {
            ent.insert(a);
        }
        ent.with_children(|p| {
            p.spawn((
                Sprite::from_color(Color::srgba(0.0, 0.0, 0.0, 0.25), Vec2::new(1.0, 0.32)),
                Transform::from_xyz(0.0, -0.55, -0.05),
            ));
        });
    }
}

/// Abrir/cerrar el cofre: el ESTADO lo decide el juego (¿tenés su panel
/// abierto?), el arte sólo lo dibuja. Al cambiar de estado se conmuta la pose
/// (`closed` ⇄ `open`) y, si esa pose es una tira de frames, se reproduce en
/// modo `once`: la animación corre ENTERA y se clava en el último frame. Por eso
/// el cofre queda abierto mientras lo usás y al cerrar el panel se lo ve
/// cerrarse completo en vez de saltar de golpe.
///
/// Requisito de arte: poses `closed` y `open` del slot `props/chest` guardadas
/// en el editor (F2). Sin ellas el cofre se queda con su sprite fijo.
fn animate_chest_art(
    mut commands: Commands,
    art: Res<ArtLibrary>,
    panel: Res<ChestPanelState>,
    mut chests: Query<(Entity, &ChestEntity, &mut ChestArt, &mut Sprite)>,
) {
    for (e, ce, mut state, mut sprite) in &mut chests {
        let open = panel.open_uid == Some(ce.uid);
        if open == state.open {
            continue;
        }
        let pose = if open { "open" } else { "closed" };
        let Some(mut s) = art.pose_sprite(CHEST_ART, pose, Vec2::splat(1.1)) else {
            continue; // sin arte de poses: nada que animar
        };
        state.open = open;
        s.custom_size = sprite.custom_size;
        *sprite = s;
        match art.pose(CHEST_ART, pose).and_then(|(_, a)| a) {
            Some(anim) => commands.entity(e).try_insert(anim.once()),
            None => commands.entity(e).remove::<ArtAnimated>(),
        };
    }
}

// --- Panel --------------------------------------------------------------------

#[derive(Component)]
struct ChestPanelRoot;

#[derive(Component, Clone, Copy)]
struct ChestSlotButton {
    index: usize,
}

/// Slot del inventario del jugador dentro del panel del cofre.
#[derive(Component, Clone, Copy)]
struct ChestPlayerSlotButton {
    slot: SlotId,
}

#[derive(Component)]
struct ChestSlotIcon;

#[derive(Component)]
struct ChestSlotQty;

#[derive(Component)]
struct ChestCloseButton;

#[derive(Component)]
struct ChestTakeAllButton;

/// El simétrico de "sacar todo": volcar la mochila entera al cofre. Faltaba, y
/// es la mitad del trabajo que se hace frente a un cofre.
#[derive(Component)]
struct ChestStoreAllButton;

/// Barra de fracción que se dibuja SOBRE la celda mientras mantenés pulsado.
#[derive(Component)]
struct ChestSplitBar;

#[derive(Component)]
struct ChestPickUpButton;

fn chest_panel_lifecycle(
    mut commands: Commands,
    panel: Res<ChestPanelState>,
    layout: Res<TouchLayout>,
    root: Query<Entity, With<GameUiRoot>>,
    existing: Query<Entity, With<ChestPanelRoot>>,
) {
    let want = panel.is_open();
    let have = !existing.is_empty();
    if want == have {
        return;
    }
    for e in &existing {
        commands.entity(e).despawn_recursive();
    }
    if !want {
        return;
    }
    let Ok(root_e) = root.get_single() else {
        return;
    };
    spawn_chest_panel(&mut commands, root_e, &layout);
}

/// **El panel del cofre: dos columnas, no una torre.**
///
/// Antes era todo VERTICAL —cofre arriba, mochila abajo, y a hacer scroll—, y
/// eso rompe lo único que importa en una pantalla de transferencia: **ver los
/// dos lados a la vez**. Sin eso no sabés si te cabe lo que estás moviendo ni
/// qué te falta guardar, y cada intercambio cuesta dos scrolls.
///
/// Ahora: **MOCHILA a la izquierda, COFRE a la derecha**, lado a lado, con las
/// acciones abajo. En vertical (celular de pie) las dos columnas se estrechan
/// pero siguen siendo dos: es preferible una celda más chica a perder de vista
/// la mitad del intercambio.
fn spawn_chest_panel(commands: &mut Commands, root: Entity, layout: &TouchLayout) {
    let short = layout.width.min(layout.height);
    let gap = (short * 0.012).clamp(3.0, 7.0);
    let font = (short * 0.032).clamp(10.0, 15.0);
    // El ancho manda: dos rejillas de `CHEST_COLS` más los aires tienen que
    // entrar en la pantalla. La celda se calcula para eso en vez de ser fija.
    let avail = layout.width - short * 0.10;
    let cell = ((avail - gap * (CHEST_COLS as f32 * 2.0 + 8.0)) / (CHEST_COLS as f32 * 2.0))
        .clamp(30.0, 62.0);
    let col_w = cell * CHEST_COLS as f32 + gap * (CHEST_COLS as f32 + 1.0);
    let panel_w = col_w * 2.0 + gap * 5.0;

    let player_slots: Vec<SlotId> = (0..HOTBAR_ITEM_SLOTS)
        .map(SlotId::Hotbar)
        .chain((0..GRID_LEN).map(SlotId::Grid))
        .collect();

    commands.entity(root).with_children(|p| {
        p.spawn((
            ChestPanelRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(crate::game_ui::ui_center_left(layout, panel_w)),
                top: Val::Px(layout.insets.top as f32 + short * 0.05),
                width: Val::Px(panel_w),
                max_height: Val::Px(layout.height - short * 0.16),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(gap),
                padding: UiRect::all(Val::Px(gap * 1.6)),
                border: UiRect::all(Val::Px(ui_theme::panel_border_width())),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(ui_theme::panel_bg(0.97)),
            BorderColor(ui_theme::panel_bevel_hi()),
            ScrollPosition::default(),
            Name::new("ChestPanel"),
        ))
        .with_children(|scroll| {
            // Un solo hijo que envuelve todo: es lo que se MIDE para saber hasta
            // dónde llega el scroll (ver `hud::ScrollContent`).
            scroll
                .spawn((
                    Node {
                        width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(gap),
                        ..default()
                    },
                    crate::hud::ScrollContent,
                ))
                .with_children(|panel| {
            panel.spawn((
                Text::new("Toca una pila para moverla entera | manten pulsado para elegir cuantas"),
                TextFont { font_size: font * 0.8, ..default() },
                TextColor(ui_theme::text_muted()),
            ));

            // ── Las dos columnas ──────────────────────────────────────────
            panel
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(gap * 3.0),
                    ..default()
                })
                .with_children(|cols| {
                    // IZQUIERDA: la mochila del jugador.
                    cols.spawn(Node {
                        width: Val::Px(col_w),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(gap),
                        ..default()
                    })
                    .with_children(|c| {
                        c.spawn((
                            Text::new("MOCHILA"),
                            TextFont { font_size: font * 1.05, ..default() },
                            TextColor(ui_theme::text_primary()),
                        ));
                        for chunk in player_slots.chunks(CHEST_COLS) {
                            c.spawn(Node {
                                flex_direction: FlexDirection::Row,
                                column_gap: Val::Px(gap),
                                ..default()
                            })
                            .with_children(|r| {
                                for slot in chunk {
                                    spawn_cell(r, cell, font, ChestPlayerSlotButton { slot: *slot });
                                }
                            });
                        }
                    });

                    // DERECHA: el cofre.
                    cols.spawn(Node {
                        width: Val::Px(col_w),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(gap),
                        ..default()
                    })
                    .with_children(|c| {
                        c.spawn((
                            Text::new("COFRE"),
                            TextFont { font_size: font * 1.05, ..default() },
                            TextColor(ui_theme::text_primary()),
                        ));
                        for row in 0..CHEST_ROWS {
                            c.spawn(Node {
                                flex_direction: FlexDirection::Row,
                                column_gap: Val::Px(gap),
                                ..default()
                            })
                            .with_children(|r| {
                                for col in 0..CHEST_COLS {
                                    let index = row * CHEST_COLS + col;
                                    spawn_cell(r, cell, font, ChestSlotButton { index });
                                }
                            });
                        }
                    });
                });

            // ── Acciones ──────────────────────────────────────────────────
            panel
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(gap),
                    margin: UiRect::top(Val::Px(gap * 2.0)),
                    justify_content: JustifyContent::Center,
                    ..default()
                })
                .with_children(|r| {
                    spawn_text_button(r, "GUARDAR TODO ->", font, ChestStoreAllButton);
                    spawn_text_button(r, "<- SACAR TODO", font, ChestTakeAllButton);
                    spawn_text_button(r, "RECOGER", font, ChestPickUpButton);
                    spawn_text_button(r, "CERRAR", font, ChestCloseButton);
                });
                });
        });
    });
}

fn spawn_cell<M: Component>(parent: &mut ChildBuilder, cell: f32, font: f32, marker: M) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(cell),
                height: Val::Px(cell),
                border: UiRect::all(Val::Px(2.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                overflow: Overflow::clip(),
                ..default()
            },
            BorderColor(ui_theme::button_border(UiButtonTone::Inventory)),
            BackgroundColor(ui_theme::button_bg(
                UiButtonTone::Inventory,
                UiButtonState::Normal,
            )),
            marker,
        ))
        .with_children(|slot| {
            slot.spawn((
                Node {
                    width: Val::Percent(80.0),
                    height: Val::Percent(80.0),
                    position_type: PositionType::Absolute,
                    ..default()
                },
                ChestSlotIcon,
            ));
            slot.spawn((
                Text::new(""),
                TextFont {
                    font_size: font * 0.8,
                    ..default()
                },
                TextColor(ui_theme::text_primary()),
                Node {
                    position_type: PositionType::Absolute,
                    bottom: Val::Px(1.0),
                    right: Val::Px(3.0),
                    ..default()
                },
                ChestSlotQty,
            ));
            // Barra de fracción: crece de abajo hacia arriba mientras mantenés
            // pulsado. Es el feedback que convierte "mantener" en algo que se
            // puede APUNTAR en vez de adivinar.
            slot.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    bottom: Val::Px(0.0),
                    width: Val::Percent(100.0),
                    height: Val::Percent(0.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.35, 0.85, 1.0, 0.0)),
                ChestSplitBar,
            ));
        });
}

fn spawn_text_button<M: Component>(parent: &mut ChildBuilder, label: &str, font: f32, marker: M) {
    parent
        .spawn((
            Button,
            Node {
                padding: UiRect::axes(Val::Px(font * 0.8), Val::Px(font * 0.5)),
                border: UiRect::all(Val::Px(2.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BorderColor(ui_theme::button_border(UiButtonTone::Craft)),
            BackgroundColor(ui_theme::button_bg(UiButtonTone::Craft, UiButtonState::Normal)),
            marker,
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(label),
                TextFont {
                    font_size: font,
                    ..default()
                },
                TextColor(ui_theme::text_primary()),
            ));
        });
}

// --- Transferencias -----------------------------------------------------------

/// Mete `stack` en el cofre (apila sobre uno igual, si no al primer hueco).
/// Devuelve lo que NO entró.
fn insert_into_chest(chest: &mut LocalChest, stack: Stack) -> Stack {
    let Some(ref id) = stack.item_id else {
        return Stack::default();
    };
    if let Some(slot) = chest
        .slots
        .iter_mut()
        .find(|s| s.item_id.as_deref() == Some(id.as_str()))
    {
        slot.qty += stack.qty;
        return Stack::default();
    }
    if let Some(slot) = chest.slots.iter_mut().find(|s| s.is_empty()) {
        *slot = stack;
        return Stack::default();
    }
    stack // cofre lleno: la pila se queda donde estaba
}

fn chest_slot_tap_system(
    mut panel: ResMut<ChestPanelState>,
    mut world: ResMut<ChestWorld>,
    mut inv_ui: ResMut<InventoryUiState>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
    slots: Query<(&Interaction, &ChestSlotButton), Changed<Interaction>>,
) {
    let Some(uid) = panel.open_uid else {
        return;
    };
    for (interaction, slot) in &slots {
        if *interaction == Interaction::Pressed {
            panel.begin_press(PressedSlot::Chest(slot.index));
            continue;
        }
        // La fracción se lee ANTES de `take_tap`, que resetea el reloj.
        let fraction = panel.split_fraction(PressedSlot::Chest(slot.index));
        if !panel.take_tap(PressedSlot::Chest(slot.index)) {
            continue;
        }
        let Some(chest) = world.get_mut(uid) else {
            return;
        };
        let Some(stack) = chest.slots.get(slot.index).cloned() else {
            continue;
        };
        if stack.is_empty() {
            continue;
        }
        let id = stack.item_id.clone().unwrap();
        // Cuánto pidió el gesto: toque = todo, mantener = lo que marcó la barra.
        let want = split_qty(stack.qty, fraction);
        // Sólo se descuenta del cofre lo que REALMENTE entró en la mochila: si
        // está llena, el resto se queda en el cofre en vez de evaporarse.
        let rest = add_item_to_slots_rest(&mut inv_ui, &id, want);
        if rest == want {
            continue; // mochila llena
        }
        let left = stack.qty - (want - rest);
        chest.slots[slot.index] = if left == 0 {
            Stack::default()
        } else {
            Stack {
                item_id: Some(id),
                qty: left,
            }
        };
        world.dirty = true;
        flush_slots_to_inventory(&inv_ui, &mut inv);
        persist.mark_dirty();
    }
}

fn player_slot_tap_system(
    mut panel: ResMut<ChestPanelState>,
    mut world: ResMut<ChestWorld>,
    mut inv_ui: ResMut<InventoryUiState>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
    slots: Query<(&Interaction, &ChestPlayerSlotButton), Changed<Interaction>>,
) {
    let Some(uid) = panel.open_uid else {
        return;
    };
    for (interaction, slot) in &slots {
        if *interaction == Interaction::Pressed {
            panel.begin_press(PressedSlot::Player(slot.slot));
            continue;
        }
        let fraction = panel.split_fraction(PressedSlot::Player(slot.slot));
        if !panel.take_tap(PressedSlot::Player(slot.slot)) {
            continue;
        }
        let Some(chest) = world.get_mut(uid) else {
            return;
        };
        let stack = player_stack(&inv_ui, slot.slot).clone();
        if stack.is_empty() {
            continue;
        }
        // Mover una PARTE: se manda lo pedido y lo que sobra se queda en la
        // mano. Si el cofre está lleno, vuelve lo que no entró — nada se pierde
        // en el camino, que es la regla de todo este panel.
        let want = split_qty(stack.qty, fraction);
        let id = stack.item_id.clone();
        let leftover = insert_into_chest(
            chest,
            Stack { item_id: id.clone(), qty: want },
        );
        let moved = want - leftover.qty;
        if moved == 0 {
            continue;
        }
        let left = stack.qty - moved;
        *player_stack_mut(&mut inv_ui, slot.slot) = if left == 0 {
            Stack::default()
        } else {
            Stack { item_id: id, qty: left }
        };
        world.dirty = true;
        flush_slots_to_inventory(&inv_ui, &mut inv);
        persist.mark_dirty();
    }
}

/// Reloj de la pulsación: alimenta la barra de fracción. Vive aparte porque
/// `Interaction` sólo avisa de los CAMBIOS, y mantener el dedo quieto no genera
/// ninguno — sin este tick la barra nunca avanzaría.
fn tick_chest_press(time: Res<Time>, mut panel: ResMut<ChestPanelState>) {
    if panel.pressed.is_some() {
        panel.press_secs += time.delta_secs();
    }
}

/// Pinta la barra de fracción sobre la celda que estás manteniendo y muestra
/// cuántas unidades se van a mover si soltás AHORA.
fn update_split_bar(
    panel: Res<ChestPanelState>,
    world: Res<ChestWorld>,
    inv_ui: Res<InventoryUiState>,
    mut bars: Query<(&mut Node, &mut BackgroundColor, &Parent), With<ChestSplitBar>>,
    chest_slots: Query<&ChestSlotButton>,
    player_slots: Query<&ChestPlayerSlotButton>,
) {
    for (mut node, mut bg, parent) in &mut bars {
        let cell = parent.get();
        let slot = if let Ok(c) = chest_slots.get(cell) {
            PressedSlot::Chest(c.index)
        } else if let Ok(p) = player_slots.get(cell) {
            PressedSlot::Player(p.slot)
        } else {
            continue;
        };
        let f = panel.split_fraction(slot);
        let total = match slot {
            PressedSlot::Chest(i) => panel
                .open_uid
                .and_then(|uid| world.get(uid))
                .and_then(|c| c.slots.get(i))
                .map(|s| s.qty)
                .unwrap_or(0),
            PressedSlot::Player(s) => player_stack(&inv_ui, s).qty,
        };
        match f.filter(|_| total > 1) {
            Some(f) => {
                let q = split_qty(total, Some(f));
                node.height = Val::Percent(100.0 * q as f32 / total as f32);
                bg.0 = Color::srgba(0.35, 0.85, 1.0, 0.45);
            }
            None => {
                node.height = Val::Percent(0.0);
                bg.0 = Color::srgba(0.35, 0.85, 1.0, 0.0);
            }
        }
    }
}

fn player_stack(state: &InventoryUiState, slot: SlotId) -> &Stack {
    match slot {
        SlotId::Grid(i) => &state.grid[i],
        SlotId::Hotbar(i) => &state.hotbar[i],
    }
}

fn player_stack_mut(state: &mut InventoryUiState, slot: SlotId) -> &mut Stack {
    match slot {
        SlotId::Grid(i) => &mut state.grid[i],
        SlotId::Hotbar(i) => &mut state.hotbar[i],
    }
}

#[allow(clippy::too_many_arguments)]
fn chest_panel_buttons(
    mut panel: ResMut<ChestPanelState>,
    mut world: ResMut<ChestWorld>,
    mut inv_ui: ResMut<InventoryUiState>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
    close: Query<&Interaction, (With<ChestCloseButton>, Changed<Interaction>)>,
    take_all: Query<&Interaction, (With<ChestTakeAllButton>, Changed<Interaction>)>,
    store_all: Query<&Interaction, (With<ChestStoreAllButton>, Changed<Interaction>)>,
    pick_up: Query<&Interaction, (With<ChestPickUpButton>, Changed<Interaction>)>,
) {
    let Some(uid) = panel.open_uid else {
        return;
    };
    if close.iter().any(|i| *i == Interaction::Pressed) {
        panel.open_uid = None;
        return;
    }
    let mut changed = false;
    // GUARDAR TODO: vuelca la mochila entera al cofre. Lo que no entra se queda
    // donde está — el panel nunca hace desaparecer una pila.
    if store_all.iter().any(|i| *i == Interaction::Pressed) {
        if let Some(chest) = world.get_mut(uid) {
            let slots: Vec<SlotId> = (0..HOTBAR_ITEM_SLOTS)
                .map(SlotId::Hotbar)
                .chain((0..GRID_LEN).map(SlotId::Grid))
                .collect();
            for s in slots {
                let stack = player_stack(&inv_ui, s).clone();
                if stack.is_empty() {
                    continue;
                }
                let leftover = insert_into_chest(chest, stack.clone());
                let moved = stack.qty - leftover.qty;
                if moved == 0 {
                    continue;
                }
                let left = stack.qty - moved;
                *player_stack_mut(&mut inv_ui, s) = if left == 0 {
                    Stack::default()
                } else {
                    Stack { item_id: stack.item_id, qty: left }
                };
                changed = true;
            }
        }
    }
    if take_all.iter().any(|i| *i == Interaction::Pressed) {
        if let Some(chest) = world.get_mut(uid) {
            for i in 0..chest.slots.len() {
                let stack = chest.slots[i].clone();
                let Some(ref id) = stack.item_id else { continue };
                let rest = add_item_to_slots_rest(&mut inv_ui, id, stack.qty);
                if rest == stack.qty {
                    continue;
                }
                chest.slots[i] = if rest == 0 {
                    Stack::default()
                } else {
                    Stack {
                        item_id: Some(id.clone()),
                        qty: rest,
                    }
                };
                changed = true;
            }
        }
    }
    if pick_up.iter().any(|i| *i == Interaction::Pressed) {
        // Sólo se puede levantar vacío: así el cofre nunca se lleva su
        // contenido a un limbo del que no se pueda sacar.
        let empty = world.get(uid).map(|c| c.is_empty()).unwrap_or(false);
        if empty {
            if add_item_to_slots_rest(&mut inv_ui, CHEST_ITEM, 1) == 0 {
                world.placed.retain(|c| c.uid != uid);
                panel.open_uid = None;
                changed = true;
            }
        }
    }
    if changed {
        world.dirty = true;
        flush_slots_to_inventory(&inv_ui, &mut inv);
        persist.mark_immediate();
    }
}

#[allow(clippy::type_complexity)]
fn update_chest_slot_visuals(
    mut commands: Commands,
    panel: Res<ChestPanelState>,
    world: Res<ChestWorld>,
    inv_ui: Res<InventoryUiState>,
    mut images: ResMut<Assets<Image>>,
    mut icons: ResMut<ItemIconCache>,
    children_q: Query<&Children>,
    chest_slots: Query<(&ChestSlotButton, &Children)>,
    player_slots: Query<(&ChestPlayerSlotButton, &Children)>,
    icon_nodes: Query<Entity, With<ChestSlotIcon>>,
    mut image_nodes: Query<&mut ImageNode, With<ChestSlotIcon>>,
    mut qty_texts: Query<&mut Text, With<ChestSlotQty>>,
) {
    let Some(uid) = panel.open_uid else {
        return;
    };
    let Some(chest) = world.get(uid) else {
        return;
    };
    let mut paint = |children: &Children, stack: &Stack| {
        let mut stack_q: Vec<Entity> = children.iter().copied().collect();
        while let Some(entity) = stack_q.pop() {
            if let Ok(kids) = children_q.get(entity) {
                stack_q.extend(kids.iter().copied());
            }
            if icon_nodes.get(entity).is_ok() {
                let icon = if stack.is_empty() {
                    icons.empty_slot(&mut images)
                } else {
                    icons.icon(stack.item_id.as_deref().unwrap_or("wood"), &mut images)
                };
                if let Ok(mut node) = image_nodes.get_mut(entity) {
                    // El rect va SIEMPRE junto a la imagen: si sólo se cambia el
                    // handle, el slot se queda con el recorte del ítem anterior.
                    node.image = icon.image.clone();
                    node.rect = icon.rect;
                } else {
                    commands.entity(entity).try_insert(icon.node());
                }
            }
            if let Ok(mut text) = qty_texts.get_mut(entity) {
                let label = if stack.qty > 1 {
                    format!("{}", stack.qty)
                } else {
                    String::new()
                };
                if text.0 != label {
                    text.0 = label;
                }
            }
        }
    };

    for (slot, children) in &chest_slots {
        let empty = Stack::default();
        let stack = chest.slots.get(slot.index).unwrap_or(&empty);
        paint(children, stack);
    }
    for (slot, children) in &player_slots {
        let stack = player_stack(&inv_ui, slot.slot).clone();
        paint(children, &stack);
    }
}

/// El panel es más alto que un teléfono: arrastre vertical (o rueda) lo desplaza.
fn scroll_chest_panel(
    mut panel: ResMut<ChestPanelState>,
    touches: Res<Touches>,
    mut wheel: EventReader<bevy::input::mouse::MouseWheel>,
    mut sections: Query<(&mut ScrollPosition, &ComputedNode, &Children), With<ChestPanelRoot>>,
    contents: Query<&ComputedNode, With<crate::hud::ScrollContent>>,
) {
    if !panel.is_open() {
        wheel.clear();
        return;
    }
    let mut dy = 0.0f32;
    for ev in wheel.read() {
        dy -= ev.y * 26.0;
    }
    for t in touches.iter() {
        dy -= t.delta().y;
    }
    // El arrastre acumulado decide si el dedo estaba desplazando el panel: si
    // pasa el umbral, soltar sobre un slot NO transfiere nada.
    if panel.pressed.is_some() {
        panel.press_drag += dy.abs();
    }
    if dy.abs() < 0.5 {
        return;
    }
    for (mut sp, view, children) in &mut sections {
        // El tope sale de MEDIR el contenido, no de una cota "generosa": era
        // `1200.0`, y con la mochila llena el último tramo del cofre no se
        // alcanzaba. `ComputedNode::size()` es físico y `ScrollPosition` lógico.
        let view_h = view.size().y * view.inverse_scale_factor();
        let content_h = children
            .iter()
            .filter_map(|c| contents.get(*c).ok())
            .map(|c| c.size().y * c.inverse_scale_factor())
            .fold(0.0f32, f32::max);
        sp.offset_y = (sp.offset_y + dy).clamp(0.0, crate::hud::max_scroll(view_h, content_h));
    }
}

// --- Persistencia (chests.json) -----------------------------------------------

#[derive(Serialize, Deserialize)]
struct ChestsSave {
    next_uid: u32,
    placed: Vec<ChestSave>,
}

#[derive(Serialize, Deserialize)]
struct ChestSave {
    uid: u32,
    body: String,
    x: f32,
    z: f32,
    /// `(item_id, qty)` por slot; `None` = hueco.
    slots: Vec<Option<(String, u32)>>,
}

fn chests_path() -> std::path::PathBuf {
    persist_dir().join("chests.json")
}

fn load_chests_on_startup(mut world: ResMut<ChestWorld>) {
    // **Cada partida arranca con lo SUYO.** Esto corría en `Startup`, antes de
    // elegir partida: leía la carpeta base y lo de las pruebas viejas aparecía en
    // cualquier partida nueva. Ahora corre al entrar a la partida (`Loading`) y
    // parte de cero; ver `partida.rs`.
    *world = ChestWorld::default();
    let Ok(text) = std::fs::read_to_string(chests_path()) else {
        return;
    };
    let Ok(save) = serde_json::from_str::<ChestsSave>(&text) else {
        eprintln!("ha-chests: chests.json ilegible — arranco vacío");
        return;
    };
    world.next_uid = save.next_uid;
    for c in save.placed {
        let Some(&body) = CelestialBody::all()
            .iter()
            .find(|b| body_def(**b).label == c.body)
        else {
            continue;
        };
        let mut slots = vec![Stack::default(); CHEST_SLOTS];
        for (i, entry) in c.slots.into_iter().take(CHEST_SLOTS).enumerate() {
            if let Some((id, qty)) = entry {
                slots[i] = Stack {
                    item_id: Some(id),
                    qty,
                };
            }
        }
        world.placed.push(LocalChest {
            uid: c.uid,
            body,
            x: c.x,
            z: c.z,
            slots,
        });
    }
    if !world.placed.is_empty() {
        eprintln!("ha-chests: {} cofres cargados", world.placed.len());
    }
}

fn save_chests_to_disk(world: &ChestWorld) {
    let save = ChestsSave {
        next_uid: world.next_uid,
        placed: world
            .placed
            .iter()
            .map(|c| ChestSave {
                uid: c.uid,
                body: body_def(c.body).label.to_string(),
                x: c.x,
                z: c.z,
                slots: c
                    .slots
                    .iter()
                    .map(|s| {
                        s.item_id
                            .as_ref()
                            .filter(|_| s.qty > 0)
                            .map(|id| (id.clone(), s.qty))
                    })
                    .collect(),
            })
            .collect(),
    };
    match serde_json::to_string(&save) {
        Ok(json) => {
            if let Err(err) = std::fs::write(chests_path(), json) {
                eprintln!("ha-chests: guardado falló: {err}");
            }
        }
        Err(err) => eprintln!("ha-chests: serializar falló: {err}"),
    }
}

fn debounced_chests_save(time: Res<Time>, mut world: ResMut<ChestWorld>) {
    let paused = is_app_paused();
    if paused && !world.was_paused {
        save_chests_to_disk(&world);
        world.dirty = false;
    }
    world.was_paused = paused;

    if !world.dirty {
        return;
    }
    world.debounce += time.delta_secs();
    if world.debounce >= 5.0 {
        save_chests_to_disk(&world);
        world.dirty = false;
        world.debounce = 0.0;
    }
}
