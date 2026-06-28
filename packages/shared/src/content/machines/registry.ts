import { MACHINE_CATALOG } from "./catalog.js";
import type { MachineDef } from "./types.js";

const byId = new Map<string, MachineDef>(MACHINE_CATALOG.map((m) => [m.id, m]));

export function getMachine(id: string): MachineDef | undefined {
  return byId.get(id);
}

export function requireMachine(id: string): MachineDef {
  const def = byId.get(id);
  if (!def) throw new Error(`Unknown machine: ${id}`);
  return def;
}

export function machinesByOutputItem(itemId: string): MachineDef[] {
  return MACHINE_CATALOG.filter((m) => m.outputItemId === itemId);
}
