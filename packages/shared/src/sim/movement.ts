import { WORLD_HALF } from "./constants.js";

/**
 * Clamp a body inside the square arena. Used by NPCs, projectiles and the
 * collision resolver. Player locomotion + parkour lives in `parkour.ts`
 * (`stepMovement`) which is the single source of truth for player movement.
 */
export function clampBody(e: {
  x: number;
  z: number;
  vx: number;
  vz: number;
  radius: number;
}): void {
  const lim = WORLD_HALF - e.radius;
  if (e.x < -lim) {
    e.x = -lim;
    e.vx = 0;
  } else if (e.x > lim) {
    e.x = lim;
    e.vx = 0;
  }
  if (e.z < -lim) {
    e.z = -lim;
    e.vz = 0;
  } else if (e.z > lim) {
    e.z = lim;
    e.vz = 0;
  }
}
