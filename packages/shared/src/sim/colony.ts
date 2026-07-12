// Colonist sim v1 — abstract crew (vision doc: needs as bars, no pathfinding).
// Mirrored in `native/crates/ha-shared/src/colony.rs` (keep numbers in sync).
//
// Loop: habitat (sealed O₂) gives capacity → request colonists from Earth
// (via the antenna uplink, costs food) → fed colonists boost drills ×1.5 →
// food stock drains per colonist; empty stock = unfed (no boost, nobody dies
// — gentle failure, matches the survival respawn philosophy).

/** Colonists a single habitat supports. */
export const COLONISTS_PER_HABITAT = 2;
/** Food stock units one colonist consumes per second (1 unit ≈ 90 s). */
export const COLONIST_FOOD_PER_SEC = 1 / 90;
/** Output multiplier a fed colonist gives its assigned machine. */
export const COLONIST_BOOST = 1.5;
/** Apples it costs to send one colonist from Earth. */
export const COLONIST_SEND_COST_APPLES = 5;
/** Apples one supply shipment converts into food stock units. */
export const SUPPLY_SEND_APPLES = 3;

export interface ColonyState {
  colonists: number;
  /** Food stock units at the base (shipped from Earth). */
  foodStock: number;
}

export function createColonyState(): ColonyState {
  return { colonists: 0, foodStock: 0 };
}

/** Advance the colony by dt seconds — colonists eat from the stock. */
export function stepColony(state: ColonyState, dt: number): void {
  if (state.colonists <= 0) return;
  state.foodStock = Math.max(0, state.foodStock - state.colonists * COLONIST_FOOD_PER_SEC * dt);
}

/** Fed colonists work; an empty pantry idles them (never kills them). */
export function colonyFed(state: ColonyState): boolean {
  return state.colonists > 0 && state.foodStock > 0;
}

export function colonyCapacity(habitats: number): number {
  return Math.max(0, Math.floor(habitats)) * COLONISTS_PER_HABITAT;
}

export function canRequestColonist(state: ColonyState, habitats: number): boolean {
  return state.colonists < colonyCapacity(habitats);
}

/**
 * Boost for the nth drill (0-indexed, stable order): the first `colonists`
 * drills get ×COLONIST_BOOST while the colony is fed.
 */
export function machineBoost(state: ColonyState, drillIndex: number): number {
  if (!colonyFed(state)) return 1.0;
  return drillIndex < state.colonists ? COLONIST_BOOST : 1.0;
}
