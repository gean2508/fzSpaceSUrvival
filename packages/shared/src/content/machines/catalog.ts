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
  {
    // Act II — lunar camp: slow passive iron from regolith (airless bodies only).
    id: "regolith_drill",
    displayName: "Regolith drill",
    assetKind: "machine_stone_extractor",
    buildCost: [
      { itemId: "stone", qty: 8 },
      { itemId: "iron_ingot", qty: 4 },
    ],
    outputItemId: "iron_ingot",
    outputPerSecond: 0.08,
    passive: true,
    placeDurationMs: 3000,
  },
  {
    // Act II — uplink: enables remote base monitoring from Earth (no output).
    id: "antenna",
    displayName: "Antenna",
    assetKind: "machine_wood_extractor",
    buildCost: [
      { itemId: "iron_ingot", qty: 8 },
      { itemId: "stone", qty: 4 },
    ],
    outputItemId: "wood",
    outputPerSecond: 0,
    passive: false,
    placeDurationMs: 3500,
  },
  {
    // Act V — sealed habitat: each one houses COLONISTS_PER_HABITAT colonists.
    id: "habitat",
    displayName: "Habitat",
    assetKind: "machine_wood_extractor",
    buildCost: [
      { itemId: "wood", qty: 20 },
      { itemId: "iron_ingot", qty: 10 },
    ],
    outputItemId: "wood",
    outputPerSecond: 0,
    passive: false,
    placeDurationMs: 5000,
  },
];
