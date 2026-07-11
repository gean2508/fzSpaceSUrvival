//! Ship mass + fuel economy — numeric mirror of
//! `packages/shared/src/sim/shipMass.ts` (design truth; never fork values).
//!
//! Arcade rules (§23-compatible): mass is FIXED per configuration (never
//! varies in flight — no rocket equation), mass taxes fuel consumption only
//! (acceleration does NOT scale — heavy ships stay controllable), and fuel is
//! a real resource: transit burns cost fuel and only the Earth pad refuels.

/// Fixed mass per craftable ship module (arcade tonnes).
pub const MODULE_MASS_T: &[(&str, f32)] = &[
    ("hull", 3.0),
    ("fuel_tank", 2.0),
    ("engine", 2.5),
    ("life_support", 1.5),
    ("cargo", 2.0),
    ("lander", 1.0),
    ("scanner", 0.8),
];

/// Baseline ship (hull + fuel_tank + engine) — today's 3-module pad build.
/// `burn_factor` is exactly 1.0 here: zero balance regression.
pub const MASS_REF_T: f32 = 7.5;
pub const FUEL_CAP_PER_TANK: f32 = 100.0;
/// Autopilot burns slightly leaner — the incentive, never a requirement.
pub const AUTOPILOT_BURN_EFFICIENCY: f32 = 0.95;
/// Autopilot refuses to engage without this reserve margin over the transit
/// cost (manual flight is NEVER fuel-gated — pilot's risk, pilot's call).
pub const TRANSIT_FUEL_MARGIN: f32 = 1.2;
/// Transit burn cost scales with cruise TIME (the player-facing quantity):
/// Moon (~28 s) ≈ 10 fuel, Neptune (~180 s) ≈ 63 fuel at baseline mass.
pub const TRANSIT_FUEL_PER_CRUISE_SEC: f32 = 0.35;

/// Cruise-time curve (used by `solar_ephemeris::transit_cruise_secs`):
/// non-linear over compressed distance, ceiling 180 s at the far reference.
pub const TRANSIT_CRUISE_MAX_SECS: f32 = 180.0;
pub const TRANSIT_CRUISE_MIN_SECS: f32 = 18.0;
pub const TRANSIT_CRUISE_EXPONENT: f32 = 0.85;
/// Farthest catalog body sits ~448 km (flight metres) from Earth.
pub const TRANSIT_FAR_REF_M: f32 = 450_000.0;

pub fn module_mass_t(id: &str) -> f32 {
    MODULE_MASS_T
        .iter()
        .find(|(m, _)| *m == id)
        .map(|(_, t)| *t)
        .unwrap_or(0.0)
}

pub fn ship_mass_t(modules: &[&str]) -> f32 {
    modules.iter().map(|m| module_mass_t(m)).sum()
}

/// Linear, legible: 0.5 + 0.5 × (mass / reference). Baseline → 1.0 exact.
pub fn burn_factor(mass_t: f32) -> f32 {
    0.5 + 0.5 * (mass_t / MASS_REF_T).max(0.0)
}

pub fn fuel_max_for_tanks(n_tanks: u32) -> f32 {
    FUEL_CAP_PER_TANK * n_tanks.max(1) as f32
}

pub fn transit_fuel_cost(cruise_secs: f32, burn_factor: f32) -> f32 {
    cruise_secs * TRANSIT_FUEL_PER_CRUISE_SEC * burn_factor
}

// ---- Ship builder Fase 1 — mirror of shipMass.ts (count parts, stats emerge)
// The pad's 3 assembled modules ARE the baseline hull+tank+engine; the player
// adds crafted parts (`part_tank`/`part_engine`/`part_lander`) on top.

/// Extra fuel tanks on top of the baseline tank.
pub const SHIP_MAX_EXTRA_TANKS: u32 = 3;
/// Total engines (baseline has 1).
pub const SHIP_MAX_ENGINES: u32 = 4;
/// Lander leg sets (v1: 0..1).
pub const SHIP_MAX_LANDERS: u32 = 1;
/// Thrust multiplier gained per engine beyond the first.
pub const ENGINE_THRUST_BONUS: f32 = 0.15;

/// Player ship configuration — counters, not free placement (Fase 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShipConfig {
    /// Extra tanks beyond baseline (0..=SHIP_MAX_EXTRA_TANKS).
    pub tanks: u32,
    /// Total engines (1..=SHIP_MAX_ENGINES).
    pub engines: u32,
    /// Lander sets (0..=SHIP_MAX_LANDERS).
    pub landers: u32,
}

impl Default for ShipConfig {
    fn default() -> Self {
        Self { tanks: 0, engines: 1, landers: 0 }
    }
}

pub fn clamp_ship_config(cfg: ShipConfig) -> ShipConfig {
    ShipConfig {
        tanks: cfg.tanks.min(SHIP_MAX_EXTRA_TANKS),
        engines: cfg.engines.clamp(1, SHIP_MAX_ENGINES),
        landers: cfg.landers.min(SHIP_MAX_LANDERS),
    }
}

/// Arcade thrust multiplier — more engines, snappier burns (never less).
pub fn thrust_factor(engines: u32) -> f32 {
    let n = engines.clamp(1, SHIP_MAX_ENGINES);
    1.0 + ENGINE_THRUST_BONUS * (n - 1) as f32
}

/// Live panel stats — the SFS trade-off the player sees while building.
#[derive(Clone, Copy, Debug)]
pub struct ShipConfigStats {
    pub mass_t: f32,
    pub fuel_max: f32,
    pub burn_factor: f32,
    pub thrust_factor: f32,
}

pub fn ship_config_stats(cfg: ShipConfig) -> ShipConfigStats {
    let c = clamp_ship_config(cfg);
    let mut mass = module_mass_t("hull");
    mass += module_mass_t("fuel_tank") * (1 + c.tanks) as f32;
    mass += module_mass_t("engine") * c.engines as f32;
    mass += module_mass_t("lander") * c.landers as f32;
    ShipConfigStats {
        mass_t: mass,
        fuel_max: fuel_max_for_tanks(1 + c.tanks),
        burn_factor: burn_factor(mass),
        thrust_factor: thrust_factor(c.engines),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_ship_factor_is_one() {
        let mass = ship_mass_t(&["hull", "fuel_tank", "engine"]);
        assert!((mass - MASS_REF_T).abs() < 1e-6);
        assert!((burn_factor(mass) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn heavier_ship_burns_more_never_less() {
        let base = burn_factor(MASS_REF_T);
        assert!(burn_factor(MASS_REF_T * 2.0) > base);
        assert!(burn_factor(0.0) >= 0.5);
    }

    #[test]
    fn transit_costs_fit_one_tank_at_baseline() {
        // Ceiling cruise at baseline must be affordable with a single tank
        // plus the autopilot margin.
        let worst = transit_fuel_cost(TRANSIT_CRUISE_MAX_SECS, 1.0);
        assert!(worst * TRANSIT_FUEL_MARGIN < FUEL_CAP_PER_TANK);
    }

    #[test]
    fn ship_builder_baseline_config_matches_reference() {
        let stats = ship_config_stats(ShipConfig::default());
        assert!((stats.mass_t - MASS_REF_T).abs() < 1e-6);
        assert!((stats.burn_factor - 1.0).abs() < 1e-6);
        assert_eq!(stats.fuel_max, FUEL_CAP_PER_TANK);
        assert!((stats.thrust_factor - 1.0).abs() < 1e-6);
    }

    #[test]
    fn ship_builder_tanks_tradeoff_and_clamps() {
        let base = ship_config_stats(ShipConfig::default());
        let heavy = ship_config_stats(ShipConfig { tanks: 2, engines: 1, landers: 0 });
        assert_eq!(heavy.fuel_max, FUEL_CAP_PER_TANK * 3.0);
        assert!(heavy.mass_t > base.mass_t);
        assert!(heavy.burn_factor > base.burn_factor);

        assert!((thrust_factor(3) - 1.3).abs() < 1e-6);
        assert!((thrust_factor(99) - thrust_factor(SHIP_MAX_ENGINES)).abs() < 1e-6);
        let clamped = clamp_ship_config(ShipConfig { tanks: 99, engines: 0, landers: 5 });
        assert_eq!(clamped.tanks, SHIP_MAX_EXTRA_TANKS);
        assert_eq!(clamped.engines, 1);
        assert_eq!(clamped.landers, SHIP_MAX_LANDERS);
    }
}
