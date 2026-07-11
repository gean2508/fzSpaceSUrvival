import { describe, expect, it } from "vitest";

import {
  burnFactor,
  fuelMaxForTanks,
  shipMassT,
  transitCruiseSecs,
  transitFuelCost,
  FUEL_CAP_PER_TANK,
  MASS_REF_T,
  TRANSIT_CRUISE_MAX_SECS,
  TRANSIT_CRUISE_MIN_SECS,
  TRANSIT_FUEL_MARGIN,
} from "./shipMass.js";

describe("shipMass", () => {
  it("baseline ship (hull+tank+engine) has burn factor exactly 1.0", () => {
    const mass = shipMassT(["hull", "fuel_tank", "engine"]);
    expect(mass).toBeCloseTo(MASS_REF_T);
    expect(burnFactor(mass)).toBeCloseTo(1.0);
  });

  it("heavier ships burn more, never less than half", () => {
    expect(burnFactor(MASS_REF_T * 2)).toBeGreaterThan(1.0);
    expect(burnFactor(0)).toBeCloseTo(0.5);
  });

  it("fuel capacity scales with tanks, min one", () => {
    expect(fuelMaxForTanks(0)).toBe(FUEL_CAP_PER_TANK);
    expect(fuelMaxForTanks(2)).toBe(FUEL_CAP_PER_TANK * 2);
  });

  it("cruise curve: Moon short, far bodies capped at 180s", () => {
    const moon = transitCruiseSecs(52_000);
    expect(moon).toBeGreaterThanOrEqual(TRANSIT_CRUISE_MIN_SECS);
    expect(moon).toBeLessThan(40);
    expect(transitCruiseSecs(450_000)).toBeCloseTo(TRANSIT_CRUISE_MAX_SECS);
    expect(transitCruiseSecs(10_000_000)).toBe(TRANSIT_CRUISE_MAX_SECS);
  });

  it("worst-case transit cost fits one tank at baseline with margin", () => {
    const worst = transitFuelCost(TRANSIT_CRUISE_MAX_SECS, 1.0);
    expect(worst * TRANSIT_FUEL_MARGIN).toBeLessThan(FUEL_CAP_PER_TANK);
  });
});
