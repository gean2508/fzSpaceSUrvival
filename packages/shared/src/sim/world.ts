import { MAX_CLIMB_HEIGHT, WORLD_HALF } from "./constants.js";
import { Rng } from "./rng.js";
import type {
  InteractableKind,
  InteractableSnapshot,
  Obstacle,
} from "./types.js";

export interface GeneratedWorld {
  obstacles: Obstacle[];
  interactables: InteractableSnapshot[];
}

/** Keep the spawn area in the middle clear of geometry. */
const CENTER_CLEAR = 4.5;

function farFromCenter(x: number, z: number, pad: number): boolean {
  return Math.hypot(x, z) > CENTER_CLEAR + pad;
}

function overlapsAny(
  x: number,
  z: number,
  pad: number,
  boxes: Obstacle[]
): boolean {
  for (const b of boxes) {
    if (
      Math.abs(x - b.x) < b.halfW + pad &&
      Math.abs(z - b.z) < b.halfD + pad
    ) {
      return true;
    }
  }
  return false;
}

/**
 * Deterministic world layout. Uses an INDEPENDENT rng derived from the sim seed
 * so it never disturbs the main simulation rng stream (spawns, AI, waves).
 * Same seed → identical obstacles/interactables on server and offline client.
 */
export function generateWorld(seed: number): GeneratedWorld {
  const rng = new Rng((seed ^ 0x9e3779b9) >>> 0);
  const obstacles: Obstacle[] = [];

  // Scatter climbable boxes / walls around the arena.
  const target = 10;
  let attempts = 0;
  while (obstacles.length < target && attempts < 200) {
    attempts++;
    const x = rng.range(-WORLD_HALF + 3, WORLD_HALF - 3);
    const z = rng.range(-WORLD_HALF + 3, WORLD_HALF - 3);
    const halfW = rng.range(0.8, 2.2);
    const halfD = rng.range(0.8, 2.2);
    if (!farFromCenter(x, z, Math.max(halfW, halfD))) continue;
    if (overlapsAny(x, z, 1.5, obstacles)) continue;
    // Most are climbable; a few are tall blockers.
    const tall = rng.next() < 0.25;
    const height = tall
      ? rng.range(MAX_CLIMB_HEIGHT + 1, MAX_CLIMB_HEIGHT + 2.5)
      : rng.range(1.0, MAX_CLIMB_HEIGHT);
    obstacles.push({
      id: `obs-${obstacles.length}`,
      x,
      z,
      halfW,
      halfD,
      height: Math.round(height * 100) / 100,
    });
  }

  // Scatter gatherable nodes, avoiding obstacle footprints.
  const interactables: InteractableSnapshot[] = [];
  const kinds: InteractableKind[] = ["tree", "rock", "container"];
  const nodeTarget = 14;
  attempts = 0;
  while (interactables.length < nodeTarget && attempts < 300) {
    attempts++;
    const x = rng.range(-WORLD_HALF + 2, WORLD_HALF - 2);
    const z = rng.range(-WORLD_HALF + 2, WORLD_HALF - 2);
    if (!farFromCenter(x, z, 1)) continue;
    if (overlapsAny(x, z, 1.0, obstacles)) continue;
    if (
      interactables.some((n) => Math.hypot(n.x - x, n.z - z) < 2.2)
    ) {
      continue;
    }
    const kind = kinds[Math.floor(rng.next() * kinds.length) % kinds.length];
    interactables.push({
      id: `node-${interactables.length}`,
      kind,
      x,
      z,
      radius: 1.4,
      looted: false,
    });
  }

  return { obstacles, interactables };
}

/** Top surface height of any obstacle whose footprint contains (x,z), else 0. */
export function groundHeightAt(
  x: number,
  z: number,
  obstacles: Obstacle[]
): number {
  let h = 0;
  for (const b of obstacles) {
    if (Math.abs(x - b.x) <= b.halfW && Math.abs(z - b.z) <= b.halfD) {
      if (b.height > h) h = b.height;
    }
  }
  return h;
}
