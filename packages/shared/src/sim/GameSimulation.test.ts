import { describe, expect, it } from "vitest";
import { GameSimulation } from "./GameSimulation.js";
import { WORLD_HALF } from "./constants.js";
import { emptyInput, encodeCraftIntent } from "./types.js";

function run(sim: GameSimulation, ticks: number) {
  for (let i = 0; i < ticks; i++) sim.step();
}

describe("GameSimulation", () => {
  it("adds and removes players", () => {
    const sim = new GameSimulation({ seed: 42 });
    sim.addPlayer("a", "scout");
    sim.addPlayer("b", "warrior");
    expect(sim.playerCount()).toBe(2);
    sim.removeEntity("a");
    expect(sim.playerCount()).toBe(1);
    expect(sim.getEntity("a")).toBeUndefined();
  });

  it("moves a player in the input direction", () => {
    const sim = new GameSimulation({ seed: 42 });
    const p = sim.addPlayer("a", "scout", 0, { x: 0, z: 0 });
    sim.queueInput("a", { ...emptyInput(1), moveX: 1, moveZ: 0 });
    run(sim, 30);
    expect(p.x).toBeGreaterThan(0.5);
    expect(Math.abs(p.z)).toBeLessThan(0.01);
    expect(sim.snapshot().lastProcessedInput["a"]).toBe(1);
  });

  it("clamps players inside world bounds", () => {
    const sim = new GameSimulation({ seed: 42 });
    const p = sim.addPlayer("a", "scout", 0, { x: 0, z: 0 });
    sim.queueInput("a", { ...emptyInput(1), moveX: 1, moveZ: 0 });
    run(sim, 1000);
    expect(p.x).toBeLessThanOrEqual(WORLD_HALF);
  });

  it("is deterministic for identical inputs and seed", () => {
    const make = () => {
      const s = new GameSimulation({ seed: 7 });
      s.addPlayer("a", "scout", 0, { x: 0, z: 0 });
      s.addPlayer("b", "mage", 0, { x: 3, z: 3 });
      return s;
    };
    const s1 = make();
    const s2 = make();
    for (let i = 0; i < 50; i++) {
      const input = { ...emptyInput(i), moveX: Math.sin(i), moveZ: Math.cos(i) };
      s1.queueInput("a", input);
      s2.queueInput("a", input);
      s1.step();
      s2.step();
    }
    expect(s1.snapshot()).toEqual(s2.snapshot());
  });

  it("separates overlapping players", () => {
    const sim = new GameSimulation({ seed: 1 });
    const a = sim.addPlayer("a", "warrior", 0, { x: 0, z: 0 });
    const b = sim.addPlayer("b", "warrior", 0, { x: 0.1, z: 0 });
    run(sim, 5);
    const dist = Math.hypot(b.x - a.x, b.z - a.z);
    expect(dist).toBeGreaterThan(0.1);
  });

  it("crafts authoritatively from encoded craft intent", () => {
    const sim = new GameSimulation({ seed: 99 });
    sim.addPlayer("p1", "scout", 0, { x: 0, z: 0 });
    const p = sim.getEntity("p1")!;
    p.inventory.push(
      { itemId: "wood", qty: 3 },
      { itemId: "sticks", qty: 2 }
    );
    sim.queueInput("p1", {
      ...emptyInput(1),
      abilities: encodeCraftIntent(0, 2),
    });
    sim.step();
    expect(p.inventory.find((i) => i.itemId === "wood_pickaxe")?.qty).toBe(1);
    expect(p.inventory.find((i) => i.itemId === "wood")).toBeUndefined();
  });

  it("keeps region deterministic by region id", () => {
    const a = new GameSimulation({ seed: 7, regionId: { kind: "overworld", gx: 0, gz: 0 } });
    const b = new GameSimulation({ seed: 7, regionId: { kind: "overworld", gx: 1, gz: 0 } });
    expect(a.worldSeed).not.toBe(b.worldSeed);
    expect(a.interactables).not.toEqual(b.interactables);
  });
});
