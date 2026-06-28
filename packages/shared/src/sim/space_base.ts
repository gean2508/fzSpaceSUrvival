import type { InventoryItem } from "./types.js";

/** Orbital station assembly — 3 modules (solar / habitat / comms). */
export const SPACE_STATION_MODULE_COUNT = 3;

export interface SpaceStationState {
  assembledModules: number;
  ready: boolean;
}

export function createSpaceStationState(): SpaceStationState {
  return { assembledModules: 0, ready: false };
}

export const SPACE_MODULE_COSTS: InventoryItem[][] = [
  [
    { itemId: "wood", qty: 8 },
    { itemId: "sticks", qty: 4 },
  ],
  [
    { itemId: "stone", qty: 10 },
    { itemId: "iron_ingot", qty: 3 },
  ],
  [
    { itemId: "iron_ingot", qty: 5 },
    { itemId: "gold_ingot", qty: 1 },
  ],
];

export function canAssembleSpaceModule(
  station: SpaceStationState,
  moduleIndex: number,
  inv: Record<string, number>
): boolean {
  if (moduleIndex >= SPACE_STATION_MODULE_COUNT) return false;
  if (station.assembledModules > moduleIndex) return false;
  if (station.assembledModules !== moduleIndex) return false;
  const costs = SPACE_MODULE_COSTS[moduleIndex];
  if (!costs) return false;
  return costs.every((c) => (inv[c.itemId] ?? 0) >= c.qty);
}

export function assembleSpaceModule(
  station: SpaceStationState,
  moduleIndex: number,
  inv: Record<string, number>
): boolean {
  if (!canAssembleSpaceModule(station, moduleIndex, inv)) return false;
  for (const c of SPACE_MODULE_COSTS[moduleIndex]!) {
    inv[c.itemId] = (inv[c.itemId] ?? 0) - c.qty;
    if (inv[c.itemId]! <= 0) delete inv[c.itemId];
  }
  station.assembledModules += 1;
  station.ready = station.assembledModules >= SPACE_STATION_MODULE_COUNT;
  return true;
}
