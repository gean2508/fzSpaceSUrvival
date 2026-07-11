import { describe, expect, it } from "vitest";
import {
  applyFood,
  createSurvivalStats,
  foodRestore,
  stepSurvivalStats,
} from "./survivalStats.js";
import { HUNGER_DECAY, OXYGEN_DRAIN_VACUUM, OXYGEN_MAX, STARVE_DAMAGE } from "./constants.js";

const AIR = { vacuum: false, oxygenSupply: true };
const VACUUM = { vacuum: true, oxygenSupply: false };

describe("survival stats", () => {
  it("hunger decays over time and starving damages hp", () => {
    const s = createSurvivalStats();
    stepSurvivalStats(s, 10, AIR);
    expect(s.hunger).toBeCloseTo(100 - HUNGER_DECAY * 10, 5);

    s.hunger = 0.001;
    const dmg = stepSurvivalStats(s, 60, AIR);
    expect(s.hunger).toBe(0);
    expect(dmg).toBeCloseTo(STARVE_DAMAGE * 60, 5);
  });

  it("oxygen drains only in vacuum and recharges with supply", () => {
    const s = createSurvivalStats();
    stepSurvivalStats(s, 5, AIR);
    expect(s.oxygen).toBe(OXYGEN_MAX);

    stepSurvivalStats(s, 5, VACUUM);
    expect(s.oxygen).toBeCloseTo(OXYGEN_MAX - OXYGEN_DRAIN_VACUUM * 5, 5);

    stepSurvivalStats(s, 60, { vacuum: true, oxygenSupply: true });
    expect(s.oxygen).toBe(OXYGEN_MAX);
  });

  it("suffocation damages hp once oxygen is empty", () => {
    const s = createSurvivalStats();
    s.oxygen = 0;
    const dmg = stepSurvivalStats(s, 1, VACUUM);
    expect(dmg).toBeGreaterThan(0);
  });

  it("food restores hunger and thirst, capped at max", () => {
    const s = createSurvivalStats();
    s.hunger = 20;
    s.thirst = 20;
    expect(applyFood(s, "apple")).toBe(true);
    expect(s.hunger).toBe(50);
    expect(s.thirst).toBe(28);

    expect(applyFood(s, "pie")).toBe(true);
    expect(applyFood(s, "pie")).toBe(true);
    expect(s.hunger).toBe(100);

    expect(applyFood(s, "stone")).toBe(false);
    expect(foodRestore("wood")).toBeUndefined();
  });
});
