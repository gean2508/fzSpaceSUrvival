import { describe, expect, it } from "vitest";
import { requireMachine } from "../content/machines/registry.js";
import {
  assembleModule,
  canAssembleModule,
  createLaunchPadState,
  LAUNCH_MODULE_COSTS,
} from "./launch_pad.js";

describe("launch pad assembly", () => {
  const def = requireMachine("launch_pad");

  it("catalog entry is non-passive with high build cost", () => {
    expect(def.passive).toBe(false);
    expect(def.outputPerSecond).toBe(0);
    expect(def.buildCost.length).toBeGreaterThanOrEqual(4);
    expect(def.assetKind).toBe("machine_launch_pad");
  });

  it("modules must be assembled in order", () => {
    const pad = createLaunchPadState(1, 28, 0, -28);
    const inv = [
      { itemId: "stone", qty: 20 },
      { itemId: "iron_ingot", qty: 20 },
      { itemId: "wood", qty: 30 },
      { itemId: "sticks", qty: 10 },
      { itemId: "gold_ingot", qty: 5 },
    ];
    expect(canAssembleModule(pad, 1, inv)).toBe(false);
    expect(canAssembleModule(pad, 0, inv)).toBe(true);
    expect(assembleModule(pad, 0, inv)).toBe(true);
    expect(pad.assembledModules).toBe(1);
    expect(pad.ready).toBe(false);
    expect(assembleModule(pad, 1, inv)).toBe(true);
    expect(assembleModule(pad, 2, inv)).toBe(true);
    expect(pad.ready).toBe(true);
    expect(pad.assembledModules).toBe(3);
  });

  it("module costs match catalog length", () => {
    expect(LAUNCH_MODULE_COSTS.length).toBe(3);
  });
});
