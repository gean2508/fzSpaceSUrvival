import { BLOCK_SIZE, BLOCK_STEP_HEIGHT, LAUNCH_PAD_ZONE, WORLD_HALF } from "./constants.js";
import { Rng } from "./rng.js";

/** Solid block types (air = empty). */
export type BlockId =
  | "air"
  | "bedrock"
  | "stone"
  | "dirt"
  | "grass"
  | "sand"
  | "snow"
  | "water";

export const BLOCK_TERRAIN = true;

export type BlockKind = "grass" | "dirt" | "stone" | "sand" | "snow";

function insideIsland(wx: number, wz: number): boolean {
  return Math.abs(wx) <= WORLD_HALF && Math.abs(wz) <= WORLD_HALF;
}

function hash2(x: number, z: number, seed: number): number {
  const s = Math.sin(x * 127.1 + z * 311.7 + seed * 0.13) * 43758.545;
  return s - Math.floor(s);
}

function smoothNoise(x: number, z: number, seed: number): number {
  const ix = Math.floor(x);
  const iz = Math.floor(z);
  const fx = x - ix;
  const fz = z - iz;
  const ux = fx * fx * (3 - 2 * fx);
  const uz = fz * fz * (3 - 2 * fz);
  const a = hash2(ix, iz, seed);
  const b = hash2(ix + 1, iz, seed);
  const c = hash2(ix, iz + 1, seed);
  const d = hash2(ix + 1, iz + 1, seed);
  return a + (b - a) * ux + (c - a) * uz + (a - b - c + d) * ux * uz;
}

function fbm(x: number, z: number, seed: number, octaves: number): number {
  let amp = 1;
  let freq = 1;
  let sum = 0;
  let norm = 0;
  for (let i = 0; i < octaves; i++) {
    sum += smoothNoise(x * freq, z * freq, seed + i * 7919) * amp;
    norm += amp;
    amp *= 0.5;
    freq *= 2;
  }
  return sum / Math.max(norm, 0.0001);
}

function inLaunchPadZone(wx: number, wz: number): boolean {
  return (
    Math.hypot(wx - LAUNCH_PAD_ZONE.x, wz - LAUNCH_PAD_ZONE.z) <= LAUNCH_PAD_ZONE.radius
  );
}

function columnHeightBlocks(bx: number, bz: number, seed: number): number {
  const wx = bx + 0.5;
  const wz = bz + 0.5;
  if (inLaunchPadZone(wx, wz)) {
    return 5;
  }
  const dist = Math.hypot(wx, wz);
  const edge = Math.max(0.08, 1 - Math.max(0, (dist - (WORLD_HALF - 18)) / 18));
  const hills = fbm(wx * 0.035, wz * 0.032, seed, 5);
  const ridges = fbm(wx * 0.08 + 40, wz * 0.07, seed ^ 0xa5a5, 3);
  const h = 2 + hills * 14 * edge + ridges * 6 * edge;
  return Math.floor(Math.min(28, Math.max(1, h)));
}

function layerBlock(by: number, top: number, wx: number, wz: number, seed: number): BlockId {
  if (by === 0) return "bedrock";
  const depth = top - by;
  if (depth === 0) {
    const beach = wx * wx + wz * wz > (WORLD_HALF - 22) ** 2;
    if (beach) return "sand";
    if (top >= 18 && fbm(wx * 0.1, wz * 0.1, seed, 2) > 0.55) return "snow";
    return "grass";
  }
  if (depth <= 3) return "dirt";
  return "stone";
}

export interface BlockWorld {
  readonly blocks: Map<string, BlockId>;
}

function blockKey(x: number, y: number, z: number): string {
  return `${x},${y},${z}`;
}

export function generateBlockWorld(seed: number): BlockWorld {
  const blocks = new Map<string, BlockId>();
  const half = Math.floor(WORLD_HALF);
  for (let bz = -half; bz < half; bz++) {
    for (let bx = -half; bx < half; bx++) {
      const wx = bx + 0.5;
      const wz = bz + 0.5;
      if (!insideIsland(wx, wz)) continue;
      const top = columnHeightBlocks(bx, bz, seed);
      for (let by = 0; by <= top; by++) {
        blocks.set(blockKey(bx, by, bz), layerBlock(by, top, wx, wz, seed));
      }
    }
  }
  return { blocks };
}

export function blockGet(world: BlockWorld, x: number, y: number, z: number): BlockId {
  return world.blocks.get(blockKey(x, y, z)) ?? "air";
}

export function blockIsSolid(id: BlockId): boolean {
  return id !== "air" && id !== "water";
}

export function blockSurfaceHeightAt(world: BlockWorld, x: number, z: number): number {
  const bx = Math.floor(x);
  const bz = Math.floor(z);
  for (let by = 64; by >= -4; by--) {
    if (blockIsSolid(blockGet(world, bx, by, bz))) {
      return (by + 1) * BLOCK_SIZE;
    }
  }
  return 0;
}

export function blockGroundForLocomotion(
  world: BlockWorld,
  x: number,
  z: number,
  footprintRadius: number
): number {
  const center = blockSurfaceHeightAt(world, x, z);
  const r = Math.max(0.2, footprintRadius);
  let support = center;
  for (const [ox, oz] of [
    [r, 0],
    [-r, 0],
    [0, r],
    [0, -r],
    [r * 0.7, r * 0.7],
    [-r * 0.7, r * 0.7],
  ] as const) {
    support = Math.max(support, blockSurfaceHeightAt(world, x + ox, z + oz));
  }
  const rise = support - center;
  return rise <= BLOCK_STEP_HEIGHT ? support : center + BLOCK_STEP_HEIGHT;
}
