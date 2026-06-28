import { WORLD_HALF } from "./constants.js";
import type { MovementBody } from "./movement.js";

export function worldWrapLimit(radius: number): number {
  return WORLD_HALF - radius;
}

export function wrapAxis(v: number, lim: number): number {
  if (v >= -lim && v <= lim) return v;
  const span = 2 * lim;
  return (((v + lim) % span) + span) % span - lim;
}

/** Toroidal wrap on the island — returns true if position changed. */
export function wrapWorldBody(b: MovementBody): boolean {
  const lim = worldWrapLimit(b.radius);
  const ox = b.x;
  const oz = b.z;
  b.x = wrapAxis(b.x, lim);
  b.z = wrapAxis(b.z, lim);
  return Math.abs(b.x - ox) > 0.001 || Math.abs(b.z - oz) > 0.001;
}
