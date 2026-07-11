//! Slot-based inventory UI — grid + hotbar interaction (select, move, split, hold-drag).

use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;

use crate::inventory_persist::{InventoryPersistState, PlayerInventory};

use super::BagPanelState;

pub const GRID_COLS: usize = 4;
/// 8 rows (32 slots) so the creative kit + a full survival haul fit; the grid
/// section scrolls (`BagInventorySection` overflow) past the visible rows.
pub const GRID_ROWS: usize = 8;
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
pub fn add_item_to_slots(state: &mut InventoryUiState, item_id: &str, qty: u32) -> bool {
    if qty == 0 {
        return false;
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
    remaining < qty
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
    let displaced = dest.clone();
    *dest = Stack {
        item_id: Some(held.item_id),
        qty: held.qty,
    };
    *stack_mut(state, held.from) = displaced;
}

/// Drop held stack to the world — item leaves inventory.
pub fn drop_held_to_world(state: &mut InventoryUiState) -> bool {
    state.held.take().is_some()
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

/// Drop held stack when finger lifts outside any inventory slot (world drop).
pub fn inventory_held_world_drop_system(
    mut touches: EventReader<TouchInput>,
    mut state: ResMut<InventoryUiState>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
    grid_slots: Query<&Interaction, With<InventoryGridSlot>>,
    hotbar_slots: Query<&Interaction, With<HotbarItemSlot>>,
) {
    if state.held.is_none() {
        return;
    }
    for ev in touches.read() {
        if ev.phase != TouchPhase::Ended {
            continue;
        }
        let over_slot = grid_slots.iter().any(|i| {
            matches!(*i, Interaction::Pressed | Interaction::Hovered)
        }) || hotbar_slots.iter().any(|i| {
            matches!(*i, Interaction::Pressed | Interaction::Hovered)
        });
        if !over_slot && drop_held_to_world(&mut state) {
            flush_slots_to_inventory(&state, &mut inv);
            persist.mark_dirty();
            state.press = None;
        }
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
