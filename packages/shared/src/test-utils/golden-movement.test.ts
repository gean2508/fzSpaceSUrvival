/**
 * Golden determinism harness — same input sequence on TS parkour vs committed snapshot.
 * Rust ha-shared mirrors this test against the same golden-movement.json.
 */
import { readFileSync, writeFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { ensureParkourFields, stepMovement } from "../sim/parkour.js";
import type { InputState } from "../sim/types.js";
import { setGroundHeightMode } from "../sim/world.js";
import type { BlockWorld } from "../sim/blockWorld.js";

const EMPTY_BLOCKS: BlockWorld = { blocks: new Map() };

const GOLDEN_PATH = resolve(
  dirname(fileURLToPath(import.meta.url)),
  "../../golden-movement.json"
);

export type GoldenMovementSpec = {
  version: 1;
  seed: number;
  tickHz: number;
  ticks: number;
  input: Omit<InputState, "seq">;
  result: {
    x: number;
    z: number;
    vx: number;
    vz: number;
    grounded: boolean;
  };
};

function runGoldenSequence(spec: Omit<GoldenMovementSpec, "result">): GoldenMovementSpec["result"] {
  // Flat ground only — isolates friction/speed/dt parity without terrain noise drift.
  setGroundHeightMode("flat");
  const obstacles: [] = [];
  const body = ensureParkourFields({
    x: 0,
    y: 0,
    z: 0,
    vx: 0,
    vz: 0,
    speed: 6,
    radius: 0.35,
  });
  const dt = 1 / spec.tickHz;
  for (let seq = 0; seq < spec.ticks; seq++) {
    stepMovement(body, { ...spec.input, seq }, EMPTY_BLOCKS, obstacles, dt, false);
  }
  const grounded = body.vy === 0;
  return {
    x: body.x,
    z: body.z,
    vx: body.vx,
    vz: body.vz,
    grounded,
  };
}

const DEFAULT_SPEC: Omit<GoldenMovementSpec, "result"> = {
  version: 1,
  seed: 42,
  tickHz: 20,
  ticks: 80,
  input: {
    moveX: 0,
    moveZ: 1,
    aimX: 0,
    aimZ: 1,
    abilities: 0,
    sprint: 0,
    action: 0,
  },
};

describe("golden movement determinism", () => {
  it("TS parkour matches committed golden snapshot", () => {
    if (process.env.UPDATE_GOLDEN_MOVEMENT === "1") {
      const result = runGoldenSequence(DEFAULT_SPEC);
      const golden: GoldenMovementSpec = { ...DEFAULT_SPEC, result };
      writeFileSync(GOLDEN_PATH, `${JSON.stringify(golden, null, 2)}\n`);
      expect(true).toBe(true);
      return;
    }
    const golden = JSON.parse(readFileSync(GOLDEN_PATH, "utf8")) as GoldenMovementSpec;
    const result = runGoldenSequence(golden);
    expect(result.x).toBeCloseTo(golden.result.x, 4);
    expect(result.z).toBeCloseTo(golden.result.z, 4);
    expect(result.vx).toBeCloseTo(golden.result.vx, 4);
    expect(result.vz).toBeCloseTo(golden.result.vz, 4);
    expect(result.grounded).toBe(golden.result.grounded);
  });
});
