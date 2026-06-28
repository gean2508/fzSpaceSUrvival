import type { MachineDef } from "./types.js";

/** Automation machines — Act I survival slice. */
export const MACHINE_CATALOG: MachineDef[] = [
  {
    id: "wood_extractor",
    displayName: "Wood extractor",
    assetKind: "machine_wood_extractor",
    buildCost: [
      { itemId: "wood", qty: 5 },
      { itemId: "stone", qty: 2 },
      { itemId: "sticks", qty: 1 },
    ],
    outputItemId: "wood",
    outputPerSecond: 0.5,
    passive: true,
    placeDurationMs: 2000,
  },
  {
    id: "stone_extractor",
    displayName: "Stone extractor",
    assetKind: "machine_stone_extractor",
    buildCost: [
      { itemId: "wood", qty: 3 },
      { itemId: "stone", qty: 4 },
      { itemId: "sticks", qty: 2 },
    ],
    outputItemId: "stone",
    outputPerSecond: 0.35,
    passive: true,
    placeDurationMs: 2500,
  },
  {
    id: "launch_pad",
    displayName: "Launch pad",
    assetKind: "machine_launch_pad",
    buildCost: [
      { itemId: "wood", qty: 40 },
      { itemId: "stone", qty: 35 },
      { itemId: "sticks", qty: 20 },
      { itemId: "iron_ingot", qty: 15 },
    ],
    outputItemId: "wood",
    outputPerSecond: 0,
    passive: false,
    placeDurationMs: 4500,
  },
];
