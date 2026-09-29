//! Survival-stats tick — mirror of `packages/shared/src/sim/survivalStats.ts`.
//! Hunger / thirst / oxygen bars + food restore table. Keep numbers in sync.

use crate::constants::{HUNGER_DECAY, HUNGER_MAX, STARVE_DAMAGE, THIRST_DECAY, THIRST_MAX};

pub const OXYGEN_MAX: f32 = 100.0;
/// Drain per second while exposed to vacuum (~83 s full tank).
pub const OXYGEN_DRAIN_VACUUM: f32 = 1.2;
/// Recharge per second inside a capsule / near a pad / in atmosphere.
pub const OXYGEN_RECHARGE: f32 = 25.0;
/// Damage per second once oxygen hits zero.
pub const SUFFOCATE_DAMAGE: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurvivalStats {
    pub hunger: f32,
    pub thirst: f32,
    pub oxygen: f32,
}

impl Default for SurvivalStats {
    fn default() -> Self {
        Self {
            hunger: HUNGER_MAX,
            thirst: THIRST_MAX,
            oxygen: OXYGEN_MAX,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SurvivalEnv {
    /// Exposed to vacuum this tick (Luna surface, space EVA, airless planet).
    pub vacuum: bool,
    /// O₂ supply reachable (inside capsule, near pad/base, or in atmosphere).
    pub oxygen_supply: bool,
}

/// Advance survival bars by `dt` seconds. Mutates `stats` and returns the
/// damage to apply to HP this tick (starvation and/or suffocation).
///
/// Thirst decays at half rate — fruit restores thirst until water sources
/// land (see `food_restore`).
pub fn step_survival_stats(stats: &mut SurvivalStats, dt: f32, env: SurvivalEnv) -> f32 {
    stats.hunger = (stats.hunger - HUNGER_DECAY * dt).max(0.0);
    stats.thirst = (stats.thirst - THIRST_DECAY * 0.5 * dt).max(0.0);

    if env.oxygen_supply {
        stats.oxygen = (stats.oxygen + OXYGEN_RECHARGE * dt).min(OXYGEN_MAX);
    } else if env.vacuum {
        stats.oxygen = (stats.oxygen - OXYGEN_DRAIN_VACUUM * dt).max(0.0);
    }

    let mut damage = 0.0;
    if stats.hunger <= 0.0 || stats.thirst <= 0.0 {
        damage += STARVE_DAMAGE * dt;
    }
    if env.vacuum && !env.oxygen_supply && stats.oxygen <= 0.0 {
        damage += SUFFOCATE_DAMAGE * dt;
    }
    damage
}

/// hunger / thirst restored when a consumable is eaten — mirror of `FOOD_RESTORE`.
pub fn food_restore(item_id: &str) -> Option<(f32, f32)> {
    Some(match item_id {
        "apple" => (30.0, 8.0),
        "orange" => (18.0, 28.0),
        "grape" => (12.0, 22.0),
        "bread" => (45.0, 0.0),
        "pie" => (60.0, 5.0),
        "meat" => (55.0, 0.0),
        "soup" => (40.0, 35.0),
        "carrot" => (20.0, 6.0),
        // Carne cruda de la granja (Minecraft: la cruda llena poco; la res y el
        // cerdo más que el pollo). El huevo, un bocado.
        "beef" => (30.0, 0.0),
        "pork" => (30.0, 0.0),
        "mutton" => (25.0, 0.0),
        "chicken" => (20.0, 0.0),
        "rabbit_meat" => (18.0, 0.0),
        "egg" => (12.0, 4.0),
        _ => return None,
    })
}

/// Apply a consumable to the bars. Returns false when the id is not food.
pub fn apply_food(stats: &mut SurvivalStats, item_id: &str) -> bool {
    let Some((hunger, thirst)) = food_restore(item_id) else {
        return false;
    };
    stats.hunger = (stats.hunger + hunger).min(HUNGER_MAX);
    stats.thirst = (stats.thirst + thirst).min(THIRST_MAX);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const AIR: SurvivalEnv = SurvivalEnv { vacuum: false, oxygen_supply: true };
    const VACUUM: SurvivalEnv = SurvivalEnv { vacuum: true, oxygen_supply: false };

    #[test]
    fn hunger_decays_and_starving_damages() {
        let mut s = SurvivalStats::default();
        step_survival_stats(&mut s, 10.0, AIR);
        assert!((s.hunger - (100.0 - HUNGER_DECAY * 10.0)).abs() < 1e-4);

        s.hunger = 0.001;
        let dmg = step_survival_stats(&mut s, 60.0, AIR);
        assert_eq!(s.hunger, 0.0);
        assert!((dmg - STARVE_DAMAGE * 60.0).abs() < 1e-4);
    }

    #[test]
    fn oxygen_drains_in_vacuum_recharges_with_supply() {
        let mut s = SurvivalStats::default();
        step_survival_stats(&mut s, 5.0, AIR);
        assert_eq!(s.oxygen, OXYGEN_MAX);

        step_survival_stats(&mut s, 5.0, VACUUM);
        assert!((s.oxygen - (OXYGEN_MAX - OXYGEN_DRAIN_VACUUM * 5.0)).abs() < 1e-4);

        step_survival_stats(
            &mut s,
            60.0,
            SurvivalEnv { vacuum: true, oxygen_supply: true },
        );
        assert_eq!(s.oxygen, OXYGEN_MAX);
    }

    #[test]
    fn suffocation_damages_once_empty() {
        let mut s = SurvivalStats::default();
        s.oxygen = 0.0;
        let dmg = step_survival_stats(&mut s, 1.0, VACUUM);
        assert!(dmg > 0.0);
    }

    #[test]
    fn food_restores_capped() {
        let mut s = SurvivalStats::default();
        s.hunger = 20.0;
        s.thirst = 20.0;
        assert!(apply_food(&mut s, "apple"));
        assert_eq!(s.hunger, 50.0);
        assert_eq!(s.thirst, 28.0);

        assert!(apply_food(&mut s, "pie"));
        assert!(apply_food(&mut s, "pie"));
        assert_eq!(s.hunger, 100.0);

        assert!(!apply_food(&mut s, "stone"));
        assert!(food_restore("wood").is_none());
    }
}
