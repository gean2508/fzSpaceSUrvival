import { describe, expect, it } from "vitest";
import {
  computePathMask,
  generatePathGrid,
  isPathCell,
  listPathCells,
  pathGridSize,
} from "./paths.js";
import { placeWorldProps } from "./worldProps.js";
import { generateWorld } from "./world.js";

describe("generatePathGrid", () => {
  const seed = 0xabc123;
  const obstacles = generateWorld(seed).obstacles;

  it("is deterministic for same seed + obstacles", () => {
    const a = generatePathGrid(seed, obstacles);
    const b = generatePathGrid(seed, obstacles);
    expect(a.cells).toEqual(b.cells);
  });

  it("matches expected grid dimensions", () => {
    const grid = generatePathGrid(seed, obstacles);
    expect(grid.size).toBe(pathGridSize());
    expect(grid.cells.length).toBe(grid.size * grid.size);
  });

  it("clears arena center", () => {
    const grid = generatePathGrid(seed, obstacles);
    expect(isPathCell(grid, 0, 0)).toBe(false);
    expect(isPathCell(grid, 1, 1)).toBe(false);
  });

  it("includes east–west creek path", () => {
    const grid = generatePathGrid(seed, obstacles);
    const creek = listPathCells(grid).filter((c) => c.iz <= -10 && c.iz >= -12);
    expect(creek.length).toBeGreaterThan(10);
  });
});

describe("computePathMask", () => {
  it("returns cardinal bitmask for path neighbors", () => {
    const grid = {
      size: 5,
      origin: 0,
      cells: new Uint8Array([
        0, 1, 0, 0, 0,
        1, 1, 1, 0, 0,
        0, 1, 0, 0, 0,
        0, 0, 0, 0, 0,
        0, 0, 0, 0, 0,
      ]),
    };
    expect(computePathMask(grid, 1, 1)).toBe(1 + 2 + 4 + 8);
    expect(computePathMask(grid, 0, 1)).toBe(2);
    expect(computePathMask(grid, 1, 0)).toBe(4);
  });
});

describe("placeWorldProps", () => {
  it("places houses and bridge deterministically", () => {
    const seed = 0xcafe1234;
    const obstacles = generateWorld(seed).obstacles;
    const grid = generatePathGrid(seed, obstacles);
    const a = placeWorldProps(seed, grid, obstacles);
    const b = placeWorldProps(seed, grid, obstacles);
    expect(a).toEqual(b);
    expect(a.some((p) => p.kind === "house")).toBe(true);
    expect(a.some((p) => p.kind === "bridge")).toBe(true);
  });
});
