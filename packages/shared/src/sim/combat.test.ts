import { describe, expect, it } from "vitest";
import { GameSimulation } from "./GameSimulation.js";
import { ABILITIES } from "./content/abilities.js";
import { emptyInput } from "./types.js";

const SLOT0 = 1 << 0; // first ability
const SLOT1 = 1 << 1; // second ability

function aimInput(seq: number, abilities: number, aimX = 1, aimZ = 0) {
  return { ...emptyInput(seq), aimX, aimZ, abilities };
}

describe("combat", () => {
  it("warrior cleave damages an enemy NPC in front", () => {
    const sim = new GameSimulation({ seed: 1 });
    sim.addPlayer("p", "warrior", 0, { x: 0, z: 0 });
    const npc = sim.addNpc("n", "scout", 1, { x: 1.2, z: 0 });
    const hp0 = npc.hp;
    sim.queueInput("p", aimInput(1, SLOT0, 1, 0));
    sim.step();
    expect(npc.hp).toBeLessThan(hp0);
  });

  it("respects ability cooldown", () => {
    const sim = new GameSimulation({ seed: 1 });
    sim.addPlayer("p", "warrior", 0, { x: 0, z: 0 });
    const npc = sim.addNpc("n", "warrior", 1, { x: 1.2, z: 0 });
    sim.queueInput("p", aimInput(1, SLOT0, 1, 0));
    sim.step();
    const afterFirst = npc.hp;
    // Hold the same ability next tick: still on cooldown, no extra damage.
    sim.queueInput("p", aimInput(2, SLOT0, 1, 0));
    sim.step();
    expect(npc.hp).toBe(afterFirst);
  });

  it("scout projectile travels and damages on impact", () => {
    const sim = new GameSimulation({ seed: 1 });
    sim.addPlayer("p", "scout", 0, { x: 0, z: 0 });
    const npc = sim.addNpc("n", "scout", 1, { x: 5, z: 0 });
    const hp0 = npc.hp;
    sim.queueInput("p", aimInput(1, SLOT0, 1, 0));
    sim.step(); // spawns projectile
    // release fire so it doesn't recast
    for (let s = 2; s < 40; s++) {
      sim.queueInput("p", aimInput(s, 0, 1, 0));
      sim.step();
      if (npc.hp < hp0) break;
    }
    expect(npc.hp).toBeLessThan(hp0);
  });

  it("mage fireball deals AoE damage to nearby enemies", () => {
    const sim = new GameSimulation({ seed: 1 });
    sim.addPlayer("p", "mage", 0, { x: 0, z: 0 });
    const a = sim.addNpc("a", "scout", 1, { x: 6, z: 0 });
    const b = sim.addNpc("b", "scout", 1, { x: 6, z: 1.5 });
    sim.queueInput("p", aimInput(1, SLOT0, 1, 0));
    sim.step();
    for (let s = 2; s < 60; s++) {
      sim.queueInput("p", aimInput(s, 0, 1, 0));
      sim.step();
      if (a.hp < a.maxHp) break;
    }
    expect(a.hp).toBeLessThan(a.maxHp);
    expect(b.hp).toBeLessThan(b.maxHp); // splash hit the second enemy
  });

  it("kills an entity when hp reaches zero", () => {
    const sim = new GameSimulation({ seed: 1 });
    sim.addPlayer("p", "warrior", 0, { x: 0, z: 0 });
    const npc = sim.addNpc("n", "scout", 1, { x: 1.0, z: 0 });
    npc.hp = 1;
    sim.queueInput("p", aimInput(1, SLOT0, 1, 0));
    sim.step();
    expect(npc.dead).toBe(true);
    expect(npc.hp).toBe(0);
  });

  it("does not damage same-team players (co-op)", () => {
    const sim = new GameSimulation({ seed: 1 });
    sim.addPlayer("p", "warrior", 0, { x: 0, z: 0 });
    const ally = sim.addPlayer("q", "scout", 0, { x: 1.0, z: 0 });
    const hp0 = ally.hp;
    sim.queueInput("p", aimInput(1, SLOT1, 1, 0)); // charge (has damage)
    sim.step();
    expect(ally.hp).toBe(hp0);
  });

  it("dash moves the caster a long distance quickly", () => {
    const sim = new GameSimulation({ seed: 1 });
    const p = sim.addPlayer("p", "scout", 0, { x: 0, z: 0 });
    const x0 = p.x;
    sim.queueInput("p", aimInput(1, SLOT1, 1, 0)); // dash
    for (let s = 1; s < 10; s++) {
      sim.queueInput("p", aimInput(s, s === 1 ? SLOT1 : 0, 1, 0));
      sim.step();
    }
    expect(p.x - x0).toBeGreaterThan(2);
    expect(ABILITIES.dash.kind).toBe("dash");
  });
});
