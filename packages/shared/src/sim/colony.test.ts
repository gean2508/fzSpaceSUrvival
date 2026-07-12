import { describe, expect, it } from "vitest";
import {
  canRequestColonist,
  colonyCapacity,
  colonyFed,
  createColonyState,
  machineBoost,
  stepColony,
  COLONIST_BOOST,
  COLONIST_FOOD_PER_SEC,
  COLONISTS_PER_HABITAT,
} from "./colony.js";

describe("colony v1", () => {
  it("capacity comes from habitats and gates requests", () => {
    const s = createColonyState();
    expect(colonyCapacity(0)).toBe(0);
    expect(colonyCapacity(2)).toBe(2 * COLONISTS_PER_HABITAT);
    expect(canRequestColonist(s, 0)).toBe(false);
    expect(canRequestColonist(s, 1)).toBe(true);
    s.colonists = COLONISTS_PER_HABITAT;
    expect(canRequestColonist(s, 1)).toBe(false);
  });

  it("colonists eat the stock; empty pantry idles them without deaths", () => {
    const s = createColonyState();
    s.colonists = 2;
    s.foodStock = 1;
    stepColony(s, 10);
    expect(s.foodStock).toBeCloseTo(1 - 2 * COLONIST_FOOD_PER_SEC * 10, 6);

    stepColony(s, 100000);
    expect(s.foodStock).toBe(0);
    expect(s.colonists).toBe(2);
    expect(colonyFed(s)).toBe(false);
  });

  it("fed colonists boost the first N drills only", () => {
    const s = createColonyState();
    s.colonists = 2;
    s.foodStock = 5;
    expect(machineBoost(s, 0)).toBeCloseTo(COLONIST_BOOST);
    expect(machineBoost(s, 1)).toBeCloseTo(COLONIST_BOOST);
    expect(machineBoost(s, 2)).toBe(1.0);

    s.foodStock = 0;
    expect(machineBoost(s, 0)).toBe(1.0);
  });
});
