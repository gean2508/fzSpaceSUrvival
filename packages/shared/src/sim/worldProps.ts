import { Rng } from "./rng.js";
import { listPathCells, type PathGrid } from "./paths.js";
import type { Obstacle, WorldPropKind, WorldPropSnapshot } from "./types.js";

const CENTER_CLEAR = 5.5;

function farFromCenter(x: number, z: number): boolean {
  return Math.hypot(x, z) > CENTER_CLEAR;
}

/**
 * Place decorative 360° props along generated paths (houses, bridges).
 * Deterministic from seed — render-only, no gameplay collision yet.
 */
export function placeWorldProps(
  seed: number,
  pathGrid: PathGrid,
  obstacles: Obstacle[]
): WorldPropSnapshot[] {
  const rng = new Rng((seed ^ 0x61e6a1) >>> 0);
  const props: WorldPropSnapshot[] = [];
  const cells = listPathCells(pathGrid).filter((c) => farFromCenter(c.ix, c.iz));

  const blocked = (x: number, z: number) =>
    obstacles.some(
      (o) => Math.abs(x - o.x) < o.halfW + 2 && Math.abs(z - o.z) < o.halfD + 2
    );

  const tooClose = (x: number, z: number, min: number) =>
    props.some((p) => Math.hypot(p.x - x, p.z - z) < min);

  const pickCell = (pred: (c: { ix: number; iz: number }) => boolean) => {
    const pool = cells.filter(pred);
    if (!pool.length) return null;
    return pool[Math.floor(rng.next() * pool.length) % pool.length];
  };

  // Two cottages along the ring trail
  for (let i = 0; i < 2; i++) {
    const cell = pickCell(
      (c) =>
        !blocked(c.ix, c.iz) &&
        !tooClose(c.ix, c.iz, 6) &&
        Math.hypot(c.ix, c.iz) > 9 &&
        Math.hypot(c.ix, c.iz) < 16
    );
    if (!cell) continue;
    props.push({
      id: `prop-house-${i}`,
      kind: "house",
      x: cell.ix + 0.5,
      z: cell.iz + 0.5,
      rot: rng.range(0, Math.PI * 2),
    });
  }

  // Bridge over the east–west creek path (z ≈ -11)
  const bridgeCell = pickCell(
    (c) => c.iz <= -10 && c.iz >= -12 && Math.abs(c.ix) > 4 && Math.abs(c.ix) < 14
  );
  if (bridgeCell && !blocked(bridgeCell.ix, bridgeCell.iz)) {
    props.push({
      id: "prop-bridge-0",
      kind: "bridge",
      x: bridgeCell.ix + 0.5,
      z: bridgeCell.iz + 0.5,
      rot: 0,
    });
  }

  return props;
}

export type { WorldPropKind };
