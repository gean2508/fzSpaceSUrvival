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
}
