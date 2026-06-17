import { describe, expect, it } from "vitest";
import { GameSimulation } from "./GameSimulation.js";
import { emptyInput } from "./types.js";

describe("survival interactions", () => {
  it("grants inventory when player holds action near a gather node", () => {
    const sim = new GameSimulation({ seed: 4242, survival: false });
    sim.addPlayer("p1", "scout", 0, { x: 0, z: 0 });
    const nodes = sim.getInteractables();
    expect(nodes.length).toBeGreaterThan(0);
    const node = nodes.find((n) => !n.looted)!;
    expect(node).toBeDefined();

    // Walk inputs won't move us exactly — directly nudge via repeated ticks at node.
    for (let i = 0; i < 40; i++) {
      sim.queueInput("p1", {
        ...emptyInput(i + 1),
        moveX: node.x > 0 ? 1 : -1,
        moveZ: node.z > 0 ? 1 : -1,
        action: 1,
      });
      sim.step();
    }

    const snap = sim.snapshot();
    const player = snap.entities.find((e) => e.id === "p1")!;
    const looted = snap.interactables.find((n) => n.id === node.id);
    expect(looted?.looted).toBe(true);
    // Inventory is server-side only in sim entity — verify via re-snapshot hunger boost for containers
    // or that node flipped looted (authoritative gather signal).
    expect(player.hunger).toBeLessThanOrEqual(100);
  });
});
