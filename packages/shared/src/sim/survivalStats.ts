// Deterministic survival-stats tick — hunger / thirst / oxygen bars.
// Mirrored in `native/crates/ha-shared/src/survival.rs` (keep numbers in sync).

import {
  HUNGER_DECAY,
  HUNGER_MAX,
  OXYGEN_DRAIN_VACUUM,
  OXYGEN_MAX,
  OXYGEN_RECHARGE,
  STARVE_DAMAGE,
  SUFFOCATE_DAMAGE,
  THIRST_DECAY,
  THIRST_MAX,
} from "./constants.js";

export interface SurvivalStats {
  hunger: number; // 0..100 (100 = full)
  thirst: number; // 0..100 (100 = hydrated)
  oxygen: number; // 0..100 (100 = full tank)
}

export interface SurvivalEnv {
  /** Exposed to vacuum this tick (Luna surface, space EVA, airless planet). */
  vacuum: boolean;
  /** O₂ supply reachable (inside capsule, near pad/base, or in atmosphere). */
  oxygenSupply: boolean;
}

export function createSurvivalStats(): SurvivalStats {
  return { hunger: HUNGER_MAX, thirst: THIRST_MAX, oxygen: OXYGEN_MAX };
}

/**
 * Advance survival bars by `dt` seconds. Mutates `stats` and returns the
 * damage to apply to HP this tick (starvation and/or suffocation).
 *
 * Thirst decays at half rate off-Earth food covers it early game — fruit
 * restores thirst until water sources land (see FOOD_RESTORE).
 */
export function stepSurvivalStats(stats: SurvivalStats, dt: number, env: SurvivalEnv): number {
  stats.hunger = Math.max(0, stats.hunger - HUNGER_DECAY * dt);
  stats.thirst = Math.max(0, stats.thirst - THIRST_DECAY * 0.5 * dt);

  if (env.oxygenSupply) {
    stats.oxygen = Math.min(OXYGEN_MAX, stats.oxygen + OXYGEN_RECHARGE * dt);
  } else if (env.vacuum) {
    stats.oxygen = Math.max(0, stats.oxygen - OXYGEN_DRAIN_VACUUM * dt);
  }

  let damage = 0;
  if (stats.hunger <= 0 || stats.thirst <= 0) damage += STARVE_DAMAGE * dt;
  if (env.vacuum && !env.oxygenSupply && stats.oxygen <= 0) damage += SUFFOCATE_DAMAGE * dt;
  return damage;
}

/** hunger / thirst restored when a consumable is eaten. */
export interface FoodRestore {
  hunger: number;
  thirst: number;
}

/** Consumable stats — ids match the item catalog (`item_food_*` assets). */
export const FOOD_RESTORE: Record<string, FoodRestore> = {
  apple: { hunger: 30, thirst: 8 },
  orange: { hunger: 18, thirst: 28 },
  grape: { hunger: 12, thirst: 22 },
  bread: { hunger: 45, thirst: 0 },
  pie: { hunger: 60, thirst: 5 },
  meat: { hunger: 55, thirst: 0 },
  soup: { hunger: 40, thirst: 35 },
  carrot: { hunger: 20, thirst: 6 },
  beef: { hunger: 30, thirst: 0 },
  pork: { hunger: 30, thirst: 0 },
  mutton: { hunger: 25, thirst: 0 },
  chicken: { hunger: 20, thirst: 0 },
  rabbit_meat: { hunger: 18, thirst: 0 },
  egg: { hunger: 12, thirst: 4 },
  milk_bucket: { hunger: 10, thirst: 35 },
  water_bucket: { hunger: 0, thirst: 40 },
};

export function foodRestore(itemId: string): FoodRestore | undefined {
  return FOOD_RESTORE[itemId];
}

/** Apply a consumable to the bars. Returns false when the id is not food. */
export function applyFood(stats: SurvivalStats, itemId: string): boolean {
  const food = foodRestore(itemId);
  if (!food) return false;
  stats.hunger = Math.min(HUNGER_MAX, stats.hunger + food.hunger);
  stats.thirst = Math.min(THIRST_MAX, stats.thirst + food.thirst);
  return true;
}
