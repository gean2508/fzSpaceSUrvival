import type { InventoryItem } from "./types.js";

/** Runtime state for one placed launch pad (extends passive machine placement). */
export interface LaunchPadState {
  uid: number;
  x: number;
  y: number;
  z: number;
  /** Modules installed — 0..3. */
  assembledModules: number;
  /** True when all three modules are installed. */
  ready: boolean;
}

export const LAUNCH_PAD_MODULE_COUNT = 3;

/** Per-module assembly cost — gate progression before ignition. */
export const LAUNCH_MODULE_COSTS: { itemId: string; qty: number }[][] = [
  [
    { itemId: "stone", qty: 15 },
    { itemId: "iron_ingot", qty: 5 },
  ],
  [
    { itemId: "wood", qty: 20 },
    { itemId: "sticks", qty: 8 },
  ],
  [
    { itemId: "iron_ingot", qty: 10 },
    { itemId: "gold_ingot", qty: 3 },
  ],
];

export function canAssembleModule(
  state: LaunchPadState,
  moduleIndex: number,
  inventory: InventoryItem[]
): boolean {
  if (moduleIndex < 0 || moduleIndex >= LAUNCH_PAD_MODULE_COUNT) return false;
  if (state.assembledModules > moduleIndex) return false;
  if (state.assembledModules !== moduleIndex) return false;
  const costs = LAUNCH_MODULE_COSTS[moduleIndex];
  if (!costs) return false;
  for (const req of costs) {
    const has = inventory.find((i) => i.itemId === req.itemId)?.qty ?? 0;
    if (has < req.qty) return false;
  }
  return true;
}

export function assembleModule(
  state: LaunchPadState,
  moduleIndex: number,
  inventory: InventoryItem[]
): boolean {
  if (!canAssembleModule(state, moduleIndex, inventory)) return false;
  const costs = LAUNCH_MODULE_COSTS[moduleIndex]!;
  for (const req of costs) {
    const slot = inventory.find((i) => i.itemId === req.itemId);
    if (!slot) return false;
    slot.qty -= req.qty;
  }
  state.assembledModules += 1;
  state.ready = state.assembledModules >= LAUNCH_PAD_MODULE_COUNT;
  return true;
}

export function createLaunchPadState(uid: number, x: number, y: number, z: number): LaunchPadState {
  return {
    uid,
    x,
    y,
    z,
    assembledModules: 0,
    ready: false,
  };
}
