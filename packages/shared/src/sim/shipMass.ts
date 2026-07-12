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
  fin: 0.4,
  nose: 0.6,
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

// ---- Ship builder Fase 1 (SFS-style: count parts, stats emerge) -------------
// The pad's 3 assembled modules ARE the baseline hull+tank+engine; the player
// then adds crafted parts on top. Mirror: `ha-shared/src/ship_mass.rs`.

/** Extra fuel tanks on top of the baseline tank. */
export const SHIP_MAX_EXTRA_TANKS = 3;
/** Total engines (baseline has 1). */
export const SHIP_MAX_ENGINES = 4;
/** Lander leg sets (raises safe touchdown speed; v1: 0..1). */
export const SHIP_MAX_LANDERS = 1;
/** Aerodynamic fins (in-atmosphere velocity alignment; 0..4). */
export const SHIP_MAX_FINS = 4;
/** Nose cone (drag reduction; 0..1). */
export const SHIP_MAX_NOSE = 1;
/** Thrust multiplier gained per engine beyond the first. */
export const ENGINE_THRUST_BONUS = 0.15;
/** Atmospheric drag multiplier with a nose cone fitted. */
export const NOSE_DRAG_FACTOR = 0.75;
/** Velocity→nose alignment rate per fin per second (in atmosphere). */
export const FIN_ALIGN_RATE_PER_FIN = 0.12;
/** Extra safe-touchdown m/s per lander set. */
export const LANDER_SAFE_V_BONUS = 6.0;

/** Player ship configuration — counters, not free placement (Fase 1/2). */
export interface ShipConfig {
  /** Extra tanks beyond baseline (0..SHIP_MAX_EXTRA_TANKS). */
  tanks: number;
  /** Total engines (1..SHIP_MAX_ENGINES). */
  engines: number;
  /** Lander sets (0..SHIP_MAX_LANDERS). */
  landers: number;
  /** Aerodynamic fins (0..SHIP_MAX_FINS). Optional: pre-Fase 2 configs. */
  fins?: number;
  /** Nose cone (0..SHIP_MAX_NOSE). Optional: pre-Fase 2 configs. */
  nose?: number;
}

export function clampShipConfig(cfg: ShipConfig): ShipConfig {
  return {
    tanks: Math.min(SHIP_MAX_EXTRA_TANKS, Math.max(0, Math.floor(cfg.tanks))),
    engines: Math.min(SHIP_MAX_ENGINES, Math.max(1, Math.floor(cfg.engines))),
    landers: Math.min(SHIP_MAX_LANDERS, Math.max(0, Math.floor(cfg.landers))),
    fins: Math.min(SHIP_MAX_FINS, Math.max(0, Math.floor(cfg.fins ?? 0))),
    nose: Math.min(SHIP_MAX_NOSE, Math.max(0, Math.floor(cfg.nose ?? 0))),
  };
}

/** Module list the config resolves to (feeds shipMassT). */
export function shipConfigModules(cfg: ShipConfig): string[] {
  const c = clampShipConfig(cfg);
  const modules: string[] = ["hull"];
  for (let i = 0; i < 1 + c.tanks; i++) modules.push("fuel_tank");
  for (let i = 0; i < c.engines; i++) modules.push("engine");
  for (let i = 0; i < c.landers; i++) modules.push("lander");
  for (let i = 0; i < (c.fins ?? 0); i++) modules.push("fin");
  for (let i = 0; i < (c.nose ?? 0); i++) modules.push("nose");
  return modules;
}

/** Arcade thrust multiplier — more engines, snappier burns (never less). */
export function thrustFactor(engines: number): number {
  const n = Math.min(SHIP_MAX_ENGINES, Math.max(1, Math.floor(engines)));
  return 1 + ENGINE_THRUST_BONUS * (n - 1);
}

export interface ShipConfigStats {
  massT: number;
  fuelMax: number;
  burnFactor: number;
  thrustFactor: number;
  /** Atmospheric drag multiplier (1.0 = bare, NOSE_DRAG_FACTOR with cone). */
  dragFactor: number;
  /** Velocity→nose alignment rate in atmosphere (0 = ballistic drift). */
  finAlignRate: number;
  /** Extra m/s of safe touchdown speed from lander legs. */
  safeLandingBonus: number;
}

/** Live panel stats — the SFS trade-off the player sees while building. */
export function shipConfigStats(cfg: ShipConfig): ShipConfigStats {
  const c = clampShipConfig(cfg);
  const massT = shipMassT(shipConfigModules(c));
  return {
    massT,
    fuelMax: fuelMaxForTanks(1 + c.tanks),
    burnFactor: burnFactor(massT),
    thrustFactor: thrustFactor(c.engines),
    dragFactor: (c.nose ?? 0) > 0 ? NOSE_DRAG_FACTOR : 1.0,
    finAlignRate: (c.fins ?? 0) * FIN_ALIGN_RATE_PER_FIN,
    safeLandingBonus: c.landers * LANDER_SAFE_V_BONUS,
  };
}
