import { describe, expect, it } from "vitest";
import { GRAVITY, GRAVITY_SCALE } from "./constants.js";
import { gravityForContext, gravityScaleForContext } from "./world_context.js";

describe("world context gravity", () => {
  it("luna gravity scale is 0.17", () => {
    expect(GRAVITY_SCALE.luna).toBe(0.17);
    expect(GRAVITY_SCALE.space).toBe(0.05);
    expect(gravityScaleForContext("luna")).toBe(0.17);
    expect(gravityScaleForContext("space")).toBe(0.05);
    expect(gravityScaleForContext("overworld")).toBe(1);
  });

  it("gravityForContext is deterministic", () => {
    expect(gravityForContext("luna")).toBeCloseTo(GRAVITY * 0.17);
    expect(gravityForContext("space")).toBeCloseTo(GRAVITY * 0.05);
    expect(gravityForContext("overworld")).toBe(GRAVITY);
  });
});
