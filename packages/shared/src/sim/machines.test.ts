import { describe, expect, it } from "vitest";
import { requireMachine } from "../content/machines/registry.js";
import {
  accrueOfflineMachines,
  canAffordMachine,
  collectMachineOutput,
  consumeBuildCost,
  createMachineSimState,
  placeMachine,
  stepMachines,
  stepMachinesTick,
} from "./machines.js";

describe("machines sim", () => {
  const def = requireMachine("wood_extractor");

  it("catalog has wood extractor", () => {
    expect(def.passive).toBe(true);
    expect(def.outputPerSecond).toBe(0.5);
    expect(def.buildCost.length).toBeGreaterThan(0);
  });

  it("place consumes build cost", () => {
    const inv = [
      { itemId: "wood", qty: 10 },
      { itemId: "stone", qty: 5 },
      { itemId: "sticks", qty: 3 },
    ];
    expect(canAffordMachine(inv, def)).toBe(true);
    expect(consumeBuildCost(inv, def)).toBe(true);
    expect(inv.find((i) => i.itemId === "wood")!.qty).toBe(5);
  });

  it("produces wood over sim ticks", () => {
    const state = createMachineSimState();
    placeMachine(state, "wood_extractor", 1, 0, 2, 0);
    for (let i = 0; i < 40; i++) stepMachinesTick(state);
    expect(state.placed[0]!.storedOutput).toBeGreaterThanOrEqual(1);
  });

  it("offline accrual is deterministic", () => {
    const state = createMachineSimState();
    placeMachine(state, "wood_extractor", 0, 0, 0, 1000);
    const produced = accrueOfflineMachines(state, 1000 + 4000);
    expect(produced).toBe(2);
    expect(state.placed[0]!.storedOutput).toBe(2);
  });

  it("collect pulls stored output", () => {
    const state = createMachineSimState();
    placeMachine(state, "wood_extractor", 0, 0, 0, 0);
    stepMachines(state, 4);
    const uid = state.placed[0]!.uid;
    const n = collectMachineOutput(state, uid);
    expect(n).toBe(2);
    expect(collectMachineOutput(state, uid)).toBe(0);
  });
});
