//! Slot-based inventory UI — grid + hotbar interaction (select, move, split, hold-drag).

use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;

use crate::inventory_persist::{InventoryPersistState, PlayerInventory};

use super::BagPanelState;

/// 6 columnas: la mochila ahora ocupa la pantalla entera (ver `spawn_bag_panel`),
/// así que caben seis celdas por fila sin apretarlas — con 4 quedaba una columnita
/// de slots enormes en medio de un panel vacío.
pub const GRID_COLS: usize = 6;
/// 6 filas (36 slots) so the creative kit + a full survival haul fit; the grid
/// section scrolls (`BagInventorySection` overflow) past the visible rows.
pub const GRID_ROWS: usize = 6;
pub const GRID_LEN: usize = GRID_COLS * GRID_ROWS;
/// Item slots between hammer (0) and BAG (last) in the 10-slot hotbar.
pub const HOTBAR_ITEM_SLOTS: usize = 8;
pub const HOTBAR_SLOTS_TOTAL: usize = HOTBAR_ITEM_SLOTS + 2; // HAM + items + BAG
pub const LONG_PRESS_SECS: f32 = 0.42;
pub const DOUBLE_TAP_SECS: f32 = 0.32;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stack {
    pub item_id: Option<String>,
    pub qty: u32,
}

impl Stack {
    pub fn is_empty(&self) -> bool {
        self.item_id.is_none() || self.qty == 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotId {
    Grid(usize),
    Hotbar(usize),
}

#[derive(Clone, Debug)]
pub struct HeldStack {
    pub item_id: String,
    pub qty: u32,
    pub from: SlotId,
}

#[derive(Clone, Debug)]
struct PressTrack {
    slot: SlotId,
    timer: Timer,
    long_fired: bool,
}

#[derive(Resource, Default)]
pub struct InventoryUiState {
    pub grid: Vec<Stack>,
    pub hotbar: [Stack; HOTBAR_ITEM_SLOTS],
    pub selected: Option<SlotId>,
    pub held: Option<HeldStack>,
    press: Option<PressTrack>,
    last_tap: Option<(SlotId, f32)>,
    pub loaded_from_disk: bool,
}

#[derive(Component, Clone, Copy)]
pub struct InventoryGridSlot {
    pub index: usize,
}

#[derive(Component, Clone, Copy)]
pub struct HotbarItemSlot {
    pub index: usize,
}

#[derive(Component)]
pub struct InventoryGridRoot;

#[derive(Component)]
pub struct InventorySlotQty;

#[derive(Component)]
pub struct InventorySlotIcon;

#[derive(Component)]
pub struct HotbarSlotQty;

#[derive(Component)]
pub struct HotbarSlotName;

#[derive(Component)]
pub struct InventorySlotName;

#[derive(Component)]
pub struct HotbarSlotIcon;

#[derive(Component)]
pub struct HeldItemGhost;

pub fn ensure_inventory_ui_state(mut state: ResMut<InventoryUiState>) {
    if state.grid.len() != GRID_LEN {
        state.grid.resize(GRID_LEN, Stack::default());
    }
}

pub fn load_grid_from_inventory(state: &mut InventoryUiState, inv: &PlayerInventory) {
    state.grid.resize(GRID_LEN, Stack::default());
    for slot in state.grid.iter_mut() {
        *slot = Stack::default();
    }
    for stack in state.hotbar.iter_mut() {
        *stack = Stack::default();
    }
    let mut i = 0usize;
    for (id, qty) in inv.sorted_entries() {
        if id == "lantern" {
            continue;
        }
        if i < HOTBAR_ITEM_SLOTS {
            state.hotbar[i] = Stack {
                item_id: Some(id.to_string()),
                qty,
            };
            i += 1;
        } else if i - HOTBAR_ITEM_SLOTS < GRID_LEN {
            state.grid[i - HOTBAR_ITEM_SLOTS] = Stack {
                item_id: Some(id.to_string()),
                qty,
            };
            i += 1;
        } else {
            break;
        }
    }
    if inv.qty("lantern") > 0 {
        state.hotbar[0] = Stack {
            item_id: Some("lantern".to_string()),
            qty: inv.qty("lantern"),
        };
    }
}

pub fn flush_slots_to_inventory(state: &InventoryUiState, inv: &mut PlayerInventory) {
    inv.items.clear();
    for stack in state.grid.iter().chain(state.hotbar.iter()) {
        if let Some(ref id) = stack.item_id {
            if stack.qty > 0 {
                *inv.items.entry(id.clone()).or_insert(0) += stack.qty;
            }
        }
    }
    // La pila "en el dedo" sigue siendo del jugador: si no se contara aquí, un
    // guardado (o un save-on-pause) mientras arrastrás la borraría del mundo.
    if let Some(ref held) = state.held {
        if held.qty > 0 {
            *inv.items.entry(held.item_id.clone()).or_insert(0) += held.qty;
        }
    }
}

fn stack_mut(state: &mut InventoryUiState, slot: SlotId) -> &mut Stack {
    match slot {
        SlotId::Grid(i) => &mut state.grid[i],
        SlotId::Hotbar(i) => &mut state.hotbar[i],
    }
}

fn stack_ref(state: &InventoryUiState, slot: SlotId) -> &Stack {
    match slot {
        SlotId::Grid(i) => &state.grid[i],
        SlotId::Hotbar(i) => &state.hotbar[i],
    }
}

fn first_empty_grid(state: &InventoryUiState) -> Option<usize> {
    state.grid.iter().position(|s| s.is_empty())
}

fn first_empty_hotbar(state: &InventoryUiState) -> Option<usize> {
    state.hotbar.iter().position(|s| s.is_empty())
}

/// Remove recipe ingredients from visible slots (after `craft` on the flushed map).
pub fn consume_from_ui_slots(state: &mut InventoryUiState, ingredients: &[(&str, u32)]) {
    for (item_id, need) in ingredients {
        let mut remaining = *need;
        for stack in state.hotbar.iter_mut().chain(state.grid.iter_mut()) {
            if remaining == 0 {
                break;
            }
            if stack.item_id.as_deref() != Some(*item_id) {
                continue;
            }
            if stack.qty <= remaining {
                remaining -= stack.qty;
                *stack = Stack::default();
            } else {
                stack.qty -= remaining;
                remaining = 0;
            }
        }
    }
}

/// Prefer hotbar, then grid — for gather/craft output visible next to HAM.
/// `true` = entró algo (aunque sea parcial). Si necesitás saber CUÁNTO quedó
/// afuera (mover pilas entre cofre y mochila), usá [`add_item_to_slots_rest`].
pub fn add_item_to_slots(state: &mut InventoryUiState, item_id: &str, qty: u32) -> bool {
    add_item_to_slots_rest(state, item_id, qty) < qty
}

/// Igual que [`add_item_to_slots`] pero devuelve las unidades que NO entraron.
/// Quien mueve una pila desde otro contenedor debe descontar sólo lo aceptado —
/// borrar la pila de origen tras un guardado parcial la haría desaparecer.
pub fn add_item_to_slots_rest(state: &mut InventoryUiState, item_id: &str, qty: u32) -> u32 {
    if qty == 0 {
        return 0;
    }
    let mut remaining = qty;
    for stack in state.hotbar.iter_mut().chain(state.grid.iter_mut()) {
        if remaining == 0 {
            break;
        }
        if stack.item_id.as_deref() == Some(item_id) {
            stack.qty += remaining;
            remaining = 0;
        }
    }
    while remaining > 0 {
        let dest = first_empty_hotbar(state)
            .map(SlotId::Hotbar)
            .or_else(|| first_empty_grid(state).map(SlotId::Grid));
        let Some(dest) = dest else {
            break;
        };
        let put = remaining.min(99);
        *stack_mut(state, dest) = Stack {
            item_id: Some(item_id.to_string()),
            qty: put,
        };
        remaining -= put;
    }
    remaining
}

/// Quita `qty` unidades de `item_id` de los slots visibles (hotbar primero). Es
/// la contracara de [`add_item_to_slots`] y la usa todo lo que CONSUME un ítem
/// colocándolo en el mundo (mapa): descontar del `PlayerInventory` sin tocar los
/// slots los deja desincronizados y el ítem "vuelve" al siguiente guardado.
pub fn remove_from_slots(state: &mut InventoryUiState, item_id: &str, qty: u32) {
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

pub fn pick_up_stack(state: &mut InventoryUiState, slot: SlotId) -> bool {
    let stack = stack_ref(state, slot).clone();
    if stack.is_empty() {
        return false;
    }
    let item_id = stack.item_id.clone().unwrap();
    let qty = stack.qty;
    *stack_mut(state, slot) = Stack::default();
    state.held = Some(HeldStack {
        item_id,
        qty,
        from: slot,
    });
    state.selected = None;
    true
}

pub fn split_stack(state: &mut InventoryUiState, slot: SlotId) -> bool {
    let stack = stack_ref(state, slot).clone();
    if stack.qty <= 1 {
        return false;
    }
    let empty = match slot {
        SlotId::Grid(_) => first_empty_grid(state),
        SlotId::Hotbar(_) => first_empty_hotbar(state),
    };
    let Some(empty_idx) = empty else {
        return false;
    };
    let half = stack.qty / 2;
    let remain = stack.qty - half;
    let Some(item_id) = stack.item_id.clone() else {
        return false;
    };
    *stack_mut(state, slot) = Stack {
        item_id: Some(item_id.clone()),
        qty: remain,
    };
    let dest = match slot {
        SlotId::Grid(_) => SlotId::Grid(empty_idx),
        SlotId::Hotbar(_) => SlotId::Hotbar(empty_idx),
    };
    *stack_mut(state, dest) = Stack {
        item_id: Some(item_id),
        qty: half,
    };
    true
}

fn merge_or_swap(state: &mut InventoryUiState, from: SlotId, to: SlotId) {
    if from == to {
        return;
    }
    let src = stack_ref(state, from).clone();
    let dst = stack_ref(state, to).clone();
    if src.is_empty() {
        return;
    }
    if dst.is_empty() {
        *stack_mut(state, to) = src;
        *stack_mut(state, from) = Stack::default();
        return;
    }
    if src.item_id == dst.item_id {
        let merged = dst.qty + src.qty;
        *stack_mut(state, to) = Stack {
            item_id: dst.item_id.clone(),
            qty: merged,
        };
        *stack_mut(state, from) = Stack::default();
        return;
    }
    *stack_mut(state, from) = dst;
    *stack_mut(state, to) = src;
}

pub fn drop_held_on(state: &mut InventoryUiState, slot: SlotId) {
    let Some(held) = state.held.take() else {
        return;
    };
    let dest = stack_mut(state, slot);
    if dest.is_empty() {
        *dest = Stack {
            item_id: Some(held.item_id),
            qty: held.qty,
        };
        return;
    }
    if dest.item_id.as_deref() == Some(held.item_id.as_str()) {
        dest.qty += held.qty;
        return;
    }
    // Intercambio: lo que había en destino vuelve al slot de origen SÓLO si
    // sigue libre. Si mientras arrastrabas algo ocupó ese hueco, el desplazado
    // va al primer slot vacío — nunca se sobrescribe (eso borraba ítems).
    let displaced = dest.clone();
    *dest = Stack {
        item_id: Some(held.item_id),
        qty: held.qty,
    };
    place_stack_safely(state, displaced, held.from);
}

/// Guarda `stack` en `preferred` si está libre; si no, en el primer hueco libre
/// (hotbar → grid). Devuelve `false` sólo si el inventario está lleno, y en ese
/// caso el stack se conserva fusionándolo con uno igual.
fn place_stack_safely(state: &mut InventoryUiState, stack: Stack, preferred: SlotId) -> bool {
    if stack.is_empty() {
        return true;
    }
    if stack_ref(state, preferred).is_empty() {
        *stack_mut(state, preferred) = stack;
        return true;
    }
    if stack_ref(state, preferred).item_id == stack.item_id {
        stack_mut(state, preferred).qty += stack.qty;
        return true;
    }
    let dest = first_empty_hotbar(state)
        .map(SlotId::Hotbar)
        .or_else(|| first_empty_grid(state).map(SlotId::Grid));
    if let Some(dest) = dest {
        *stack_mut(state, dest) = stack;
        return true;
    }
    // Inventario lleno: último recurso, apilar sobre otro montón del mismo ítem.
    let same = state
        .hotbar
        .iter_mut()
        .chain(state.grid.iter_mut())
        .find(|s| s.item_id == stack.item_id);
    if let Some(slot) = same {
        slot.qty += stack.qty;
        return true;
    }
    false
}

/// Devuelve la pila arrastrada al inventario (su slot de origen si sigue libre).
/// Es el camino que garantiza que soltar el dedo NUNCA borre un ítem.
pub fn return_held_to_inventory(state: &mut InventoryUiState) -> bool {
    let Some(held) = state.held.take() else {
        return false;
    };
    let from = held.from;
    place_stack_safely(
        state,
        Stack {
            item_id: Some(held.item_id),
            qty: held.qty,
        },
        from,
    );
    true
}

pub fn handle_slot_tap(
    state: &mut InventoryUiState,
    inv: &mut PlayerInventory,
    persist: &mut InventoryPersistState,
    slot: SlotId,
    now_secs: f32,
) {
    if state.held.is_some() {
        drop_held_on(state, slot);
        flush_slots_to_inventory(state, inv);
        persist.mark_dirty();
        return;
    }

    if let Some((last_slot, last_time)) = state.last_tap {
        if last_slot == slot && now_secs - last_time < DOUBLE_TAP_SECS {
            state.last_tap = None;
            if split_stack(state, slot) {
                flush_slots_to_inventory(state, inv);
                persist.mark_dirty();
            }
            state.selected = None;
            return;
        }
    }
    state.last_tap = Some((slot, now_secs));

    if state.selected == Some(slot) {
        state.selected = None;
        return;
    }

    if let Some(selected) = state.selected {
        merge_or_swap(state, selected, slot);
        state.selected = None;
        flush_slots_to_inventory(state, inv);
        persist.mark_dirty();
        return;
    }

    if !stack_ref(state, slot).is_empty() {
        state.selected = Some(slot);
    }
}

fn slot_id_from_grid(slot: &InventoryGridSlot) -> SlotId {
    SlotId::Grid(slot.index)
}

fn slot_id_from_hotbar(slot: &HotbarItemSlot) -> SlotId {
    SlotId::Hotbar(slot.index)
}

pub fn inventory_slot_press_system(
    bag: Res<BagPanelState>,
    mut state: ResMut<InventoryUiState>,
    grid_slots: Query<(&InventoryGridSlot, &Interaction), (Changed<Interaction>, Without<HotbarItemSlot>)>,
    hotbar_slots: Query<(&HotbarItemSlot, &Interaction), (Changed<Interaction>, Without<InventoryGridSlot>)>,
) {
    if state.held.is_some() {
        return;
    }
    for (slot, interaction) in &grid_slots {
        if !bag.open || *interaction != Interaction::Pressed {
            continue;
        }
        let slot_id = slot_id_from_grid(slot);
        state.press = Some(PressTrack {
            slot: slot_id,
            timer: Timer::from_seconds(LONG_PRESS_SECS, TimerMode::Once),
            long_fired: false,
        });
    }
    for (slot, interaction) in &hotbar_slots {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let slot_id = slot_id_from_hotbar(slot);
        state.press = Some(PressTrack {
            slot: slot_id,
            timer: Timer::from_seconds(
                if bag.open { LONG_PRESS_SECS } else { 99.0 },
                TimerMode::Once,
            ),
            long_fired: false,
        });
    }
}

pub fn inventory_long_press_system(
    bag: Res<BagPanelState>,
    time: Res<Time>,
    mut state: ResMut<InventoryUiState>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
) {
    if !bag.open {
        return;
    }
    let Some(track) = state.press.as_mut() else {
        return;
    };
    track.timer.tick(time.delta());
    if track.timer.just_finished() && !track.long_fired {
        track.long_fired = true;
        let slot = track.slot;
        if pick_up_stack(&mut state, slot) {
            flush_slots_to_inventory(&state, &mut inv);
            persist.mark_dirty();
        }
    }
}

pub fn inventory_drop_while_held_system(
    bag: Res<BagPanelState>,
    mut state: ResMut<InventoryUiState>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
    grid_slots: Query<(&InventoryGridSlot, &Interaction), (Changed<Interaction>, Without<HotbarItemSlot>)>,
    hotbar_slots: Query<(&HotbarItemSlot, &Interaction), (Changed<Interaction>, Without<InventoryGridSlot>)>,
) {
    if !bag.open || state.held.is_none() {
        return;
    }
    for (slot, interaction) in &grid_slots {
        if *interaction == Interaction::Pressed {
            drop_held_on(&mut state, slot_id_from_grid(slot));
            flush_slots_to_inventory(&state, &mut inv);
            persist.mark_dirty();
            state.press = None;
            return;
        }
    }
    for (slot, interaction) in &hotbar_slots {
        if *interaction == Interaction::Pressed {
            drop_held_on(&mut state, slot_id_from_hotbar(slot));
            flush_slots_to_inventory(&state, &mut inv);
            persist.mark_dirty();
            state.press = None;
            return;
        }
    }
}

pub fn inventory_slot_release_system(
    bag: Res<BagPanelState>,
    time: Res<Time>,
    mut state: ResMut<InventoryUiState>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
    grid_slots: Query<(&InventoryGridSlot, &Interaction), (Changed<Interaction>, Without<HotbarItemSlot>)>,
    hotbar_slots: Query<(&HotbarItemSlot, &Interaction), (Changed<Interaction>, Without<InventoryGridSlot>)>,
) {
    if state.held.is_some() {
        return;
    }
    let now = time.elapsed_secs();

    for (slot, interaction) in &grid_slots {
        if !bag.open || *interaction != Interaction::None {
            continue;
        }
        let slot_id = slot_id_from_grid(slot);
        if let Some(track) = state.press.take() {
            if track.slot == slot_id && !track.long_fired {
                handle_slot_tap(&mut state, &mut inv, &mut persist, slot_id, now);
            }
        }
    }
    for (slot, interaction) in &hotbar_slots {
        if *interaction != Interaction::None {
            continue;
        }
        let slot_id = slot_id_from_hotbar(slot);
        if let Some(track) = state.press.take() {
            if track.slot == slot_id && !track.long_fired {
                if bag.open {
                    handle_slot_tap(&mut state, &mut inv, &mut persist, slot_id, now);
                } else if !stack_ref(&state, slot_id).is_empty() {
                    state.selected = Some(slot_id);
                }
            }
        }
    }
}

/// Levantar el dedo NO suelta la pila al vacío: queda "en la mano" (fantasma en
/// pantalla) hasta que toques un slot destino. Antes esto la tiraba al mundo —
/// que en la práctica era BORRARLA, porque no existe entidad de ítem caído.
///
/// Lo único que hace este sistema al soltar es limpiar el press track, para que
/// el siguiente toque cuente como "colocar aquí".
pub fn inventory_release_keeps_held_system(
    mut touches: EventReader<TouchInput>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut state: ResMut<InventoryUiState>,
) {
    if state.held.is_none() {
        return;
    }
    let released = mouse.just_released(MouseButton::Left)
        || touches
            .read()
            .any(|ev| matches!(ev.phase, TouchPhase::Ended | TouchPhase::Canceled));
    if released {
        state.press = None;
    }
}

/// Cerrar la mochila con una pila arrastrada la devuelve al inventario. Sin
/// esto, cerrar el panel (o el botón atrás de Android) evaporaba el stack.
pub fn inventory_return_held_on_close_system(
    bag: Res<BagPanelState>,
    mut state: ResMut<InventoryUiState>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
) {
    if bag.open || state.held.is_none() {
        return;
    }
    if return_held_to_inventory(&mut state) {
        state.press = None;
        flush_slots_to_inventory(&state, &mut inv);
        persist.mark_dirty();
    }
}

/// When `PlayerInventory` gains items (machines, etc.) keep hotbar layout and add deltas.
pub fn merge_inv_gains_into_ui(state: &mut InventoryUiState, inv: &PlayerInventory) -> bool {
    let mut flushed = PlayerInventory::default();
    flush_slots_to_inventory(state, &mut flushed);
    if flushed.items == inv.items {
        return false;
    }
    for (id, qty) in inv.sorted_entries() {
        let have = flushed.qty(id);
        if qty > have {
            add_item_to_slots(state, id, qty - have);
        }
    }
    flush_slots_to_inventory(state, &mut flushed);
    if flushed.items != inv.items {
        load_grid_from_inventory(state, inv);
    }
    true
}

pub fn inventory_signature(inv: &PlayerInventory) -> String {
    let entries = inv.sorted_entries();
    entries
        .iter()
        .map(|(id, qty)| format!("{id}:{qty}"))
        .collect::<Vec<_>>()
        .join(",")
}

pub fn grid_signature(state: &InventoryUiState) -> String {
    let mut parts = Vec::new();
    for (i, s) in state.grid.iter().enumerate() {
        if let Some(ref id) = s.item_id {
            parts.push(format!("g{i}:{id}:{}", s.qty));
        }
    }
    for (i, s) in state.hotbar.iter().enumerate() {
        if let Some(ref id) = s.item_id {
            parts.push(format!("h{i}:{id}:{}", s.qty));
        }
    }
    parts.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with(slot: SlotId, item: &str, qty: u32) -> InventoryUiState {
        let mut s = InventoryUiState {
            grid: vec![Stack::default(); GRID_LEN],
            ..default()
        };
        *stack_mut(&mut s, slot) = Stack {
            item_id: Some(item.to_string()),
            qty,
        };
        s
    }

    fn total(state: &InventoryUiState, item: &str) -> u32 {
        let mut inv = PlayerInventory::default();
        flush_slots_to_inventory(state, &mut inv);
        inv.qty(item)
    }

    /// El bug reportado: levantar una pila y soltar el dedo la borraba. Ahora la
    /// pila arrastrada sigue contando como inventario del jugador.
    #[test]
    fn held_stack_still_counts_as_inventory() {
        let mut s = state_with(SlotId::Grid(3), "wood", 12);
        assert!(pick_up_stack(&mut s, SlotId::Grid(3)));
        assert!(s.held.is_some());
        assert_eq!(total(&s, "wood"), 12, "la pila en la mano no puede evaporarse");
    }

    #[test]
    fn returning_held_restores_origin_slot() {
        let mut s = state_with(SlotId::Hotbar(2), "stone", 7);
        pick_up_stack(&mut s, SlotId::Hotbar(2));
        assert!(return_held_to_inventory(&mut s));
        assert_eq!(s.hotbar[2].qty, 7);
        assert_eq!(s.hotbar[2].item_id.as_deref(), Some("stone"));
        assert!(s.held.is_none());
    }

    /// Si mientras arrastrabas algo ocupó el slot de origen, el desplazado va a
    /// un hueco libre en vez de sobrescribirse (antes se perdía).
    #[test]
    fn swap_never_overwrites_a_refilled_origin() {
        let mut s = state_with(SlotId::Grid(0), "wood", 5);
        s.grid[1] = Stack { item_id: Some("stone".into()), qty: 3 };
        pick_up_stack(&mut s, SlotId::Grid(0));
        // Otro ítem ocupa el origen mientras la pila está en la mano.
        s.grid[0] = Stack { item_id: Some("apple".into()), qty: 2 };
        drop_held_on(&mut s, SlotId::Grid(1));

        assert_eq!(total(&s, "wood"), 5);
        assert_eq!(total(&s, "stone"), 3, "el stack desplazado sigue existiendo");
        assert_eq!(total(&s, "apple"), 2);
    }

    #[test]
    fn add_reports_what_did_not_fit() {
        let mut s = InventoryUiState {
            grid: vec![Stack::default(); GRID_LEN],
            ..default()
        };
        // Llenar cada hueco con un ítem distinto para que no haya apilado.
        for i in 0..HOTBAR_ITEM_SLOTS {
            s.hotbar[i] = Stack { item_id: Some(format!("h{i}")), qty: 1 };
        }
        for i in 0..GRID_LEN {
            s.grid[i] = Stack { item_id: Some(format!("g{i}")), qty: 1 };
        }
        assert_eq!(add_item_to_slots_rest(&mut s, "wood", 9), 9);
        assert!(!add_item_to_slots(&mut s, "wood", 9));
    }

    #[test]
    fn drop_on_empty_slot_moves_the_whole_stack() {
        let mut s = state_with(SlotId::Grid(4), "iron_ingot", 3);
        pick_up_stack(&mut s, SlotId::Grid(4));
        drop_held_on(&mut s, SlotId::Hotbar(5));
        assert!(s.grid[4].is_empty());
        assert_eq!(s.hotbar[5].qty, 3);
        assert_eq!(total(&s, "iron_ingot"), 3);
    }
}
