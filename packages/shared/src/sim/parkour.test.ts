import { describe, expect, it } from "vitest";
import { STAMINA_CLIMB_DRAIN, STAMINA_MAX } from "./constants.js";
import type { BlockWorld } from "./blockWorld.js";
import { ensureParkourFields, stepMovement } from "./parkour.js";
import { emptyInput, type Obstacle } from "./types.js";
import { groundHeightForLocomotion, setGroundHeightMode } from "./world.js";

const DT = 0.05;
const RADIUS = 0.35;
const CLIMB_Z = 95;
const EMPTY_BLOCKS: BlockWorld = { blocks: new Map() };

/** Climbable wall on the beach (low terrain) — centered at (3, CLIMB_Z). */
const WALL: Obstacle = {
  id: "wall",
  x: 3,
  z: CLIMB_Z,
  halfW: 1,
  halfD: 1,
  height: 2.5,
};

setGroundHeightMode("flat");

/** Player overlapping the wall footprint (pressed against the left face). */
function atWallFace() {
  const x = WALL.x - WALL.halfW - RADIUS + 0.08;
  const z = CLIMB_Z;
  const y = groundHeightForLocomotion(x, z, [WALL], RADIUS, EMPTY_BLOCKS);
  return ensureParkourFields({
    x,
    z,
    y,
    vx: 0,
    vz: 0,
    vy: 0,
    rot: 0,
    speed: 6,
    radius: RADIUS,
    stamina: STAMINA_MAX,
    maxStamina: STAMINA_MAX,
    state: "idle",
  });
}

describe("parkour state machine", () => {
  it("transitions idle → running when moving", () => {
    const b = ensureParkourFields({
      x: 0,
      z: 0,
      y: 0,
      vx: 0,
      vz: 0,
      vy: 0,
      rot: 0,
      speed: 6,
      radius: RADIUS,
      stamina: STAMINA_MAX,
      maxStamina: STAMINA_MAX,
      state: "idle",
    });
    stepMovement(b, { ...emptyInput(1), moveX: 0, moveZ: 1 }, EMPTY_BLOCKS, [], DT);
    expect(b.state).toBe("running");
  });

  it("enters wall_trek or climbing when pressing into a climbable wall", () => {
    const b = atWallFace();
    const input = { ...emptyInput(1), moveX: 1, moveZ: 0, action: 0 };
    stepMovement(b, input, EMPTY_BLOCKS, [WALL], DT);
    expect(["wall_trek", "climbing"]).toContain(b.state);
  });

  it("enters climbing when holding action against a climbable wall", () => {
    const b = atWallFace();
    const startY = b.y;
    const input = { ...emptyInput(1), moveX: 1, moveZ: 0, action: 1 };
    stepMovement(b, input, EMPTY_BLOCKS, [WALL], DT);
    expect(b.state).toBe("climbing");
    stepMovement(b, input, EMPTY_BLOCKS, [WALL], DT);
    expect(b.y).toBeGreaterThan(startY);
  });

  it("reaches hanging at the top edge then mantles on action", () => {
    const b = atWallFace();
    const climb = { ...emptyInput(1), moveX: 1, moveZ: 0, action: 1 };
    for (let i = 0; i < 80; i++) {
      stepMovement(b, climb, EMPTY_BLOCKS, [WALL], DT);
      if (b.state === "hanging") break;
    }
    expect(b.state).toBe("hanging");

    stepMovement(b, { ...emptyInput(2), action: 1 }, EMPTY_BLOCKS, [WALL], DT);
    expect(b.state).not.toBe("hanging");
    expect(b.y).toBeGreaterThanOrEqual(WALL.height - 0.1);
  });

  it("drops to exhausted when stamina depletes while climbing", () => {
    const b = atWallFace();
    b.stamina = STAMINA_CLIMB_DRAIN * DT * 2;
    const input = { ...emptyInput(1), moveX: 1, moveZ: 0, action: 1 };
    for (let i = 0; i < 4; i++) stepMovement(b, input, EMPTY_BLOCKS, [WALL], DT);
    expect(b.state).toBe("exhausted");
    expect(b.stamina).toBe(0);
  });
});
