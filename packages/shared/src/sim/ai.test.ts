import { describe, expect, it } from "vitest";
import { GameSimulation } from "./GameSimulation.js";
import { emptyInput } from "./types.js";

function run(sim: GameSimulation, ticks: number) {
  for (let i = 0; i < ticks; i++) sim.step();
}

describe("NPC AI + survival", () => {
  it("an NPC chases a nearby player", () => {
    const sim = new GameSimulation({ seed: 3 });
    sim.addPlayer("p", "scout", 0, { x: 0, z: 0 });
    const npc = sim.spawnNpc("grunt", 1, { x: 6, z: 0 });
    const d0 = Math.hypot(npc.x - 0, npc.z - 0);
    sim.queueInput("p", emptyInput(1)); // player stands still
    run(sim, 40);
    const d1 = Math.hypot(npc.x - 0, npc.z - 0);
    expect(d1).toBeLessThan(d0); // it closed distance
  });

  it("an NPC damages the player when adjacent", () => {
    const sim = new GameSimulation({ seed: 3 });
    const p = sim.addPlayer("p", "scout", 0, { x: 0, z: 0 });
    sim.spawnNpc("grunt", 1, { x: 1, z: 0 });
    const hp0 = p.hp;
    run(sim, 30);
    expect(p.hp).toBeLessThan(hp0);
  });

  it("a cowardly low-hp NPC flees from the player", () => {
    const sim = new GameSimulation({ seed: 3 });
    sim.addPlayer("p", "scout", 0, { x: 0, z: 0 });
    const npc = sim.spawnNpc("skirmisher", 1, { x: 3, z: 0 });
    npc.hp = 3; // near death; skirmisher has low courage
    sim.queueInput("p", emptyInput(1));
    const d0 = Math.hypot(npc.x, npc.z);
    run(sim, 30);
    const d1 = Math.hypot(npc.x, npc.z);
    expect(d1).toBeGreaterThan(d0); // ran away
  });

  it("survival director spawns escalating waves", () => {
    const sim = new GameSimulation({ seed: 5, survival: true });
    sim.addPlayer("p", "warrior", 0, { x: 0, z: 0 });
    expect(sim.wave).toBe(0);
    run(sim, 60); // ~3s: first wave should have spawned
    expect(sim.wave).toBeGreaterThanOrEqual(1);
    expect(sim.aliveCount("npc")).toBeGreaterThan(0);
  });

  it("ends the match when all players are dead", () => {
    const sim = new GameSimulation({ seed: 5, survival: true });
    const p = sim.addPlayer("p", "scout", 0, { x: 0, z: 0 });
    run(sim, 40);
    p.hp = 0;
    p.dead = true;
    sim.step();
    expect(sim.status).toBe("ended");
  });
});
