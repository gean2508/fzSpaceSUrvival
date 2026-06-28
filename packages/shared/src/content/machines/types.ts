/** Machine definition — automation buildings (shared catalog). */
export interface MachineDef {
  id: string;
  displayName: string;
  /** Procedural mesh kind in assets catalog. */
  assetKind: string;
  buildCost: { itemId: string; qty: number }[];
  outputItemId: string;
  /** Items produced per sim second while active. */
  outputPerSecond: number;
  /** If true, produces without player attendance. */
  passive: boolean;
  /** Hold-interact duration (ms) to place after valid gesture. */
  placeDurationMs: number;
}

export interface PlacedMachine {
  uid: number;
  machineId: string;
  x: number;
  y: number;
  z: number;
  placedAtMs: number;
  lastAccruedMs: number;
  /** Fractional accumulator for deterministic tick output. */
  outputAccumulator: number;
  storedOutput: number;
}

export interface MachineSimState {
  nextUid: number;
  placed: PlacedMachine[];
}
