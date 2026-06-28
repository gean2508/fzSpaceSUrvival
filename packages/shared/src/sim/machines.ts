import { TICK_MS } from "./constants.js";
import { requireMachine } from "../content/machines/registry.js";
import type { InventoryItem } from "./types.js";
import type { MachineDef, MachineSimState, PlacedMachine } from "../content/machines/types.js";

export function createMachineSimState(): MachineSimState {
  return { nextUid: 1, placed: [] };
}

export function canAffordMachine(inventory: InventoryItem[], def: MachineDef): boolean {
  for (const req of def.buildCost) {
    const has = inventory.find((i) => i.itemId === req.itemId)?.qty ?? 0;
    if (has < req.qty) return false;
  }
  return true;
}

export function consumeBuildCost(inventory: InventoryItem[], def: MachineDef): boolean {
  if (!canAffordMachine(inventory, def)) return false;
  for (const req of def.buildCost) {
    const slot = inventory.find((i) => i.itemId === req.itemId);
    if (!slot) return false;
    slot.qty -= req.qty;
  }
  return true;
}

export function placeMachine(
  state: MachineSimState,
  machineId: string,
  x: number,
  y: number,
  z: number,
  nowMs: number
): PlacedMachine {
  requireMachine(machineId);
  const placed: PlacedMachine = {
    uid: state.nextUid++,
    machineId,
    x,
    y,
    z,
    placedAtMs: nowMs,
    lastAccruedMs: nowMs,
    outputAccumulator: 0,
    storedOutput: 0,
  };
  state.placed.push(placed);
  return placed;
}

/** Advance production for all passive machines (deterministic sim tick). */
export function stepMachines(state: MachineSimState, dtSec: number): number {
  let totalProduced = 0;
  for (const m of state.placed) {
    const def = requireMachine(m.machineId);
    if (!def.passive || def.outputPerSecond <= 0) continue;
    m.outputAccumulator += def.outputPerSecond * dtSec;
    const whole = Math.floor(m.outputAccumulator);
    if (whole > 0) {
      m.outputAccumulator -= whole;
      m.storedOutput += whole;
      totalProduced += whole;
    }
  }
  return totalProduced;
}

/**
 * Catch-up production when returning to a zone (offline / away from sim).
 * Uses elapsed wall time × rate — deterministic given `nowMs` and stored `lastAccruedMs`.
 */
export function accrueOfflineMachines(state: MachineSimState, nowMs: number): number {
  let total = 0;
  for (const m of state.placed) {
    const def = requireMachine(m.machineId);
    if (!def.passive || def.outputPerSecond <= 0) continue;
    const elapsedSec = Math.max(0, (nowMs - m.lastAccruedMs) / 1000);
    if (elapsedSec <= 0) continue;
    const produced = Math.floor(elapsedSec * def.outputPerSecond);
    if (produced > 0) {
      m.storedOutput += produced;
      total += produced;
      m.lastAccruedMs = nowMs;
    }
  }
  return total;
}

/** Pull stored output from a machine into inventory qty (returns amount collected). */
export function collectMachineOutput(
  state: MachineSimState,
  uid: number,
  maxTake = Number.MAX_SAFE_INTEGER
): number {
  const m = state.placed.find((p) => p.uid === uid);
  if (!m) return 0;
  const take = Math.min(m.storedOutput, maxTake);
  m.storedOutput -= take;
  return take;
}

/** Vitest helper — one fixed tick at 20 Hz. */
export function stepMachinesTick(state: MachineSimState): number {
  return stepMachines(state, TICK_MS / 1000);
}
