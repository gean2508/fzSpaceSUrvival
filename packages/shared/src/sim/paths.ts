import { WORLD_HALF } from "./constants.js";
import { Rng } from "./rng.js";
import type { Obstacle } from "./types.js";

/** Square path bitmap aligned to 1m ground cells. */
export interface PathGrid {
  size: number;
  origin: number;
  /** Row-major cells[iz * size + (ix - origin)] — 1 = path, 0 = grass */
  cells: Uint8Array;
}

const CENTER_CLEAR = 4.5;

export function pathGridSize(): number {
  return WORLD_HALF * 2;
}

function cellIndex(grid: PathGrid, ix: number, iz: number): number | null {
  const x = ix - grid.origin;
  const z = iz - grid.origin;
  if (x < 0 || z < 0 || x >= grid.size || z >= grid.size) return null;
  return z * grid.size + x;
}

export function isPathCell(grid: PathGrid, ix: number, iz: number): boolean {
  const idx = cellIndex(grid, ix, iz);
  if (idx == null) return false;
  return grid.cells[idx] !== 0;
}

/** Cardinal autotile mask N=1 E=2 S=4 W=8 when neighbor is path. */
export function computePathMask(grid: PathGrid, ix: number, iz: number): number {
  let mask = 0;
  if (isPathCell(grid, ix, iz - 1)) mask |= 1;
  if (isPathCell(grid, ix + 1, iz)) mask |= 2;
  if (isPathCell(grid, ix, iz + 1)) mask |= 4;
  if (isPathCell(grid, ix - 1, iz)) mask |= 8;
  return mask;
}

function blockedByObstacle(ix: number, iz: number, obstacles: Obstacle[]): boolean {
  for (const o of obstacles) {
    if (Math.abs(ix - o.x) <= o.halfW + 0.2 && Math.abs(iz - o.z) <= o.halfD + 0.2) {
      return true;
    }
  }
  return false;
}

function stampPath(
  grid: PathGrid,
  ix: number,
  iz: number,
  obstacles: Obstacle[]
): void {
  if (Math.hypot(ix, iz) < CENTER_CLEAR) return;
  if (blockedByObstacle(ix, iz, obstacles)) return;
  const idx = cellIndex(grid, ix, iz);
  if (idx != null) grid.cells[idx] = 1;
}

function widenPath(grid: PathGrid): void {
  const { size, cells } = grid;
  const next = new Uint8Array(cells);
  for (let z = 0; z < size; z++) {
    for (let x = 0; x < size; x++) {
      if (!cells[z * size + x]) continue;
      for (let dz = -1; dz <= 1; dz++) {
        for (let dx = -1; dx <= 1; dx++) {
          const nx = x + dx;
          const nz = z + dz;
          if (nx >= 0 && nz >= 0 && nx < size && nz < size) {
            next[nz * size + nx] = 1;
          }
        }
      }
    }
  }
  grid.cells = next;
}

/**
 * Deterministic meadow paths for parkour routes. Same seed + obstacles → same grid
 * on server, offline client, and landing showcase.
 */
export function generatePathGrid(seed: number, obstacles: Obstacle[]): PathGrid {
  const size = pathGridSize();
  const origin = -WORLD_HALF;
  const cells = new Uint8Array(size * size);
  const grid: PathGrid = { size, origin, cells };
  const rng = new Rng((seed ^ 0xbadc0ffe) >>> 0);

  // Meadow ring trail
  for (let a = 0; a < Math.PI * 2; a += 0.11) {
    const r = 12.5 + rng.range(-0.6, 0.6);
    stampPath(grid, Math.round(Math.cos(a) * r), Math.round(Math.sin(a) * r), obstacles);
  }

  // East–west creek-side path
  for (let x = -17; x <= 17; x++) {
    stampPath(grid, x, -11, obstacles);
    stampPath(grid, x, -10, obstacles);
  }

  // North spur toward arena
  for (let z = -9; z <= 8; z++) {
    stampPath(grid, 10, z, obstacles);
    stampPath(grid, 11, z, obstacles);
  }

  // Diagonal shortcut (parkour lane)
  for (let i = 0; i < 14; i++) {
    stampPath(grid, 5 + i, -5 + i, obstacles);
  }

  // RNG spurs off the ring
  for (let i = 0; i < 6; i++) {
    const angle = rng.range(0, Math.PI * 2);
    const len = Math.floor(rng.range(4, 9));
    const sx = Math.round(Math.cos(angle) * 12);
    const sz = Math.round(Math.sin(angle) * 12);
    for (let s = 0; s < len; s++) {
      stampPath(grid, sx + Math.round(Math.cos(angle) * s), sz + Math.round(Math.sin(angle) * s), obstacles);
    }
  }

  widenPath(grid);
  return grid;
}

/** List world cell coords that are path (for prop placement). */
export function listPathCells(grid: PathGrid): { ix: number; iz: number }[] {
  const out: { ix: number; iz: number }[] = [];
  for (let z = 0; z < grid.size; z++) {
    for (let x = 0; x < grid.size; x++) {
      if (grid.cells[z * grid.size + x]) {
        out.push({ ix: x + grid.origin, iz: z + grid.origin });
      }
    }
  }
  return out;
}
