import { describe, expect, it } from "vitest";

import {
  burnFactor,
  clampShipConfig,
  fuelMaxForTanks,
  shipConfigModules,
  shipConfigStats,
  shipMassT,
  thrustFactor,
  transitCruiseSecs,
  transitFuelCost,
  FUEL_CAP_PER_TANK,
  MASS_REF_T,
  SHIP_MAX_ENGINES,
  SHIP_MAX_EXTRA_TANKS,
  SHIP_MAX_LANDERS,
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

  it("ship builder: baseline config matches the reference ship exactly", () => {
    const stats = shipConfigStats({ tanks: 0, engines: 1, landers: 0 });
    expect(stats.massT).toBeCloseTo(MASS_REF_T);
    expect(stats.burnFactor).toBeCloseTo(1.0);
    expect(stats.fuelMax).toBe(FUEL_CAP_PER_TANK);
    expect(stats.thrustFactor).toBeCloseTo(1.0);
  });

  it("ship builder: more tanks = more fuel AND more burn (the trade-off)", () => {
    const base = shipConfigStats({ tanks: 0, engines: 1, landers: 0 });
    const heavy = shipConfigStats({ tanks: 2, engines: 1, landers: 0 });
    expect(heavy.fuelMax).toBe(FUEL_CAP_PER_TANK * 3);
    expect(heavy.massT).toBeGreaterThan(base.massT);
    expect(heavy.burnFactor).toBeGreaterThan(base.burnFactor);
  });

  it("ship builder: engines raise thrust, config clamps to limits", () => {
    expect(thrustFactor(1)).toBeCloseTo(1.0);
    expect(thrustFactor(3)).toBeCloseTo(1.3);
    expect(thrustFactor(99)).toBeCloseTo(thrustFactor(SHIP_MAX_ENGINES));
    const clamped = clampShipConfig({ tanks: 99, engines: 0, landers: 5 });
    expect(clamped.tanks).toBe(SHIP_MAX_EXTRA_TANKS);
    expect(clamped.engines).toBe(1);
    expect(clamped.landers).toBe(SHIP_MAX_LANDERS);
    expect(shipConfigModules(clamped).filter((m) => m === "fuel_tank")).toHaveLength(
      1 + SHIP_MAX_EXTRA_TANKS
    );
  });
});
