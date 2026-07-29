import { BLOCK_STEP_HEIGHT, LAUNCH_PAD_ZONE, WORLD_HALF } from "./constants.js";
import type { MovementBody } from "./parkour.js";
import { Rng } from "./rng.js";
import {
  BLOCK_TERRAIN,
  blockGroundForLocomotion,
  blockSurfaceHeightAt,
  generateBlockWorld,
  type BlockWorld,
} from "./blockWorld.js";
import type { InteractableKind, InteractableSnapshot, Obstacle } from "./types.js";
import { wrapWorldBody } from "./worldWrap.js";

export interface GeneratedWorld {
  blocks: BlockWorld;
  /** Legacy — always empty; terrain is voxels. */
  obstacles: Obstacle[];
  interactables: InteractableSnapshot[];
}

const CENTER_CLEAR = 8;
const NODE_TARGET = 72;

function farFromCenter(x: number, z: number, pad: number): boolean {
  return Math.hypot(x, z) > CENTER_CLEAR + pad;
}

function farFromLaunchPad(x: number, z: number, pad: number): boolean {
  return (
    Math.hypot(x - LAUNCH_PAD_ZONE.x, z - LAUNCH_PAD_ZONE.z) >
    LAUNCH_PAD_ZONE.radius + pad
  );
}

export function generateWorld(seed: number): GeneratedWorld {
  const blocks = generateBlockWorld(seed);
  const rng = new Rng((seed ^ 0x9e3779b9) >>> 0);
  const interactables: InteractableSnapshot[] = [];
  let attempts = 0;
  while (interactables.length < NODE_TARGET && attempts < 1200) {
    attempts++;
    const bx = Math.floor(rng.range(-WORLD_HALF + 4, WORLD_HALF - 4));
    const bz = Math.floor(rng.range(-WORLD_HALF + 4, WORLD_HALF - 4));
    const x = bx + 0.5;
    const z = bz + 0.5;
    if (!farFromCenter(x, z, 1)) continue;
    if (!farFromLaunchPad(x, z, 1)) continue;
    if (blockSurfaceHeightAt(blocks, x, z) < 1.5) continue;
    if (interactables.some((n) => Math.hypot(n.x - x, n.z - z) < 3)) continue;
    const roll = rng.next();
    const kind: InteractableKind =
      roll < 0.45 ? "tree" : roll < 0.8 ? "rock" : "container";
    interactables.push({
      id: `node-${interactables.length}`,
      kind,
      x,
      z,
      radius: 1.4,
      looted: false,
    });
  }
  return { blocks, obstacles: [], interactables };
}

export type GroundHeightMode = "terrain" | "flat";

let groundHeightMode: GroundHeightMode = "terrain";

export function setGroundHeightMode(mode: GroundHeightMode): void {
  groundHeightMode = mode;
}

export function obstacleSurfaceY(b: Obstacle, blocks: BlockWorld): number {
  if (groundHeightMode === "flat") return b.height;
  return blockSurfaceHeightAt(blocks, b.x, b.z) + b.height;
}

function groundHeightAtPoint(
  x: number,
  z: number,
  blocks: BlockWorld,
  obstacles: Obstacle[]
): number {
  let h = groundHeightMode === "flat" ? 0 : blockSurfaceHeightAt(blocks, x, z);
  for (const b of obstacles) {
    if (Math.abs(x - b.x) <= b.halfW && Math.abs(z - b.z) <= b.halfD) {
      const top = obstacleSurfaceY(b, blocks);
      if (top > h) h = top;
    }
  }
  return h;
}

export function groundHeightForLocomotion(
  x: number,
  z: number,
  obstacles: Obstacle[],
  footprintRadius: number,
  blocks: BlockWorld
): number {
  if (groundHeightMode === "flat") {
    return groundHeightAtPoint(x, z, blocks, obstacles);
  }
  if (BLOCK_TERRAIN) {
    return blockGroundForLocomotion(blocks, x, z, footprintRadius);
  }
  return groundHeightAtPoint(x, z, blocks, obstacles);
}

export function blockSteepTerrainStep(
  body: { x: number; z: number; vx: number; vz: number },
  blocks: BlockWorld,
  obstacles: Obstacle[],
  fromX: number,
  fromZ: number
): void {
  if (groundHeightMode === "flat") return;
  if (!BLOCK_TERRAIN) return;
  const h0 = groundHeightAtPoint(fromX, fromZ, blocks, obstacles);
  const h1 = groundHeightAtPoint(body.x, body.z, blocks, obstacles);
  if (h1 - h0 > BLOCK_STEP_HEIGHT + 0.001) {
    body.x = fromX;
    body.z = fromZ;
    body.vx = 0;
    body.vz = 0;
    return;
  }
  for (let i = 1; i <= 4; i++) {
    const t = i / 4;
    const sx = fromX + (body.x - fromX) * t;
    const sz = fromZ + (body.z - fromZ) * t;
    if (groundHeightAtPoint(sx, sz, blocks, obstacles) - h0 > BLOCK_STEP_HEIGHT + 0.001) {
      body.x = fromX;
      body.z = fromZ;
      body.vx = 0;
      body.vz = 0;
      return;
    }
  }
}

export function blockUnwalkableTerrainAscent(
  _body: { x: number; z: number; vx: number; vz: number },
  _fromX: number,
  _fromZ: number
): void {
  /* block terrain — no slope guard */
}

export function wrapAndResnapBody(
  body: MovementBody,
  blocks: BlockWorld,
  obstacles: Obstacle[]
): void {
  if (wrapWorldBody(body)) {
    body.y = groundHeightForLocomotion(body.x, body.z, obstacles, body.radius, blocks);
    body.vy = 0;
  }
}
