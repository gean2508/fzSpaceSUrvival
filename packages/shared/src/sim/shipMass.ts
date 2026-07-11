// Ship mass + fuel economy — DESIGN TRUTH; `ha-shared/src/ship_mass.rs` is
// the numeric mirror (rule: sim in shared, Rust mirrors, never fork values).
//
// Arcade rules (§23-compatible): mass is FIXED per configuration (no rocket
// equation, no in-flight mass change), mass taxes fuel consumption only
// (acceleration never scales), fuel is a real resource (transit burns cost
// fuel; only the Earth pad refuels).

/** Fixed mass per craftable ship module (arcade tonnes). */
export const MODULE_MASS_T: Record<string, number> = {
  hull: 3.0,
  fuel_tank: 2.0,
  engine: 2.5,
  life_support: 1.5,
  cargo: 2.0,
  lander: 1.0,
  scanner: 0.8,
};

/** Baseline ship (hull + fuel_tank + engine) — burn factor exactly 1.0. */
export const MASS_REF_T = 7.5;
export const FUEL_CAP_PER_TANK = 100;
/** Autopilot burns slightly leaner — incentive, never a requirement. */
export const AUTOPILOT_BURN_EFFICIENCY = 0.95;
/** Autopilot refuses to engage without this reserve over the transit cost. */
export const TRANSIT_FUEL_MARGIN = 1.2;
/** Transit burn cost scales with cruise TIME (player-facing quantity). */
export const TRANSIT_FUEL_PER_CRUISE_SEC = 0.35;

/** Cruise-time curve: non-linear over compressed distance, 180 s ceiling. */
export const TRANSIT_CRUISE_MAX_SECS = 180;
export const TRANSIT_CRUISE_MIN_SECS = 18;
export const TRANSIT_CRUISE_EXPONENT = 0.85;
export const TRANSIT_FAR_REF_M = 450_000;

export function shipMassT(modules: readonly string[]): number {
  return modules.reduce((sum, m) => sum + (MODULE_MASS_T[m] ?? 0), 0);
}

/** Linear, legible: 0.5 + 0.5 × (mass / reference). Baseline → 1.0 exact. */
export function burnFactor(massT: number): number {
  return 0.5 + 0.5 * Math.max(0, massT / MASS_REF_T);
}

export function fuelMaxForTanks(nTanks: number): number {
  return FUEL_CAP_PER_TANK * Math.max(1, nTanks);
}

export function transitCruiseSecs(distanceM: number): number {
  const norm = Math.max(0, distanceM / TRANSIT_FAR_REF_M);
  const secs = TRANSIT_CRUISE_MAX_SECS * Math.pow(norm, TRANSIT_CRUISE_EXPONENT);
  return Math.min(TRANSIT_CRUISE_MAX_SECS, Math.max(TRANSIT_CRUISE_MIN_SECS, secs));
}

export function transitFuelCost(cruiseSecs: number, burnFactorValue: number): number {
  return cruiseSecs * TRANSIT_FUEL_PER_CRUISE_SEC * burnFactorValue;
}
