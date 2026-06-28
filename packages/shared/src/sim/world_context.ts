import { GRAVITY, GRAVITY_SCALE } from "./constants.js";
import type { WorldContext } from "./types.js";

export function gravityScaleForContext(ctx: WorldContext): number {
  return GRAVITY_SCALE[ctx];
}

export function gravityForContext(ctx: WorldContext): number {
  return GRAVITY * gravityScaleForContext(ctx);
}
