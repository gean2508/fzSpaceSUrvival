//! Colonist sim v1 — mirror of `packages/shared/src/sim/colony.ts`.
//! Abstract crew: habitats give capacity, fed colonists boost drills ×1.5,
//! empty pantry idles them (gentle failure, nobody dies).

/// Colonists a single habitat supports.
pub const COLONISTS_PER_HABITAT: u32 = 2;
/// Food stock units one colonist consumes per second (1 unit ≈ 90 s).
pub const COLONIST_FOOD_PER_SEC: f32 = 1.0 / 90.0;
/// Output multiplier a fed colonist gives its assigned machine.
pub const COLONIST_BOOST: f32 = 1.5;
/// Apples it costs to send one colonist from Earth.
pub const COLONIST_SEND_COST_APPLES: u32 = 5;
/// Apples one supply shipment converts into food stock units.
pub const SUPPLY_SEND_APPLES: u32 = 3;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ColonyState {
    pub colonists: u32,
    /// Food stock units at the base (shipped from Earth).
    pub food_stock: f32,
}

/// Advance the colony by dt seconds — colonists eat from the stock.
pub fn step_colony(state: &mut ColonyState, dt: f32) {
    if state.colonists == 0 {
        return;
    }
    state.food_stock =
        (state.food_stock - state.colonists as f32 * COLONIST_FOOD_PER_SEC * dt).max(0.0);
}

/// Fed colonists work; an empty pantry idles them (never kills them).
pub fn colony_fed(state: &ColonyState) -> bool {
    state.colonists > 0 && state.food_stock > 0.0
}

pub fn colony_capacity(habitats: u32) -> u32 {
    habitats * COLONISTS_PER_HABITAT
}

pub fn can_request_colonist(state: &ColonyState, habitats: u32) -> bool {
    state.colonists < colony_capacity(habitats)
}

/// Boost for the nth drill (0-indexed, stable order): the first `colonists`
/// drills get ×COLONIST_BOOST while the colony is fed.
pub fn machine_boost(state: &ColonyState, drill_index: u32) -> f32 {
    if !colony_fed(state) {
        return 1.0;
    }
    if drill_index < state.colonists {
        COLONIST_BOOST
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_gates_requests() {
        let mut s = ColonyState::default();
        assert_eq!(colony_capacity(0), 0);
        assert_eq!(colony_capacity(2), 2 * COLONISTS_PER_HABITAT);
        assert!(!can_request_colonist(&s, 0));
        assert!(can_request_colonist(&s, 1));
        s.colonists = COLONISTS_PER_HABITAT;
        assert!(!can_request_colonist(&s, 1));
    }

    #[test]
    fn colonists_eat_stock_and_idle_when_empty() {
        let mut s = ColonyState {
            colonists: 2,
            food_stock: 1.0,
        };
        step_colony(&mut s, 10.0);
        assert!((s.food_stock - (1.0 - 2.0 * COLONIST_FOOD_PER_SEC * 10.0)).abs() < 1e-5);

        step_colony(&mut s, 100000.0);
        assert_eq!(s.food_stock, 0.0);
        assert_eq!(s.colonists, 2);
        assert!(!colony_fed(&s));
    }

    #[test]
    fn fed_colonists_boost_first_n_drills() {
        let s = ColonyState {
            colonists: 2,
            food_stock: 5.0,
        };
        assert!((machine_boost(&s, 0) - COLONIST_BOOST).abs() < 1e-6);
        assert!((machine_boost(&s, 1) - COLONIST_BOOST).abs() < 1e-6);
        assert_eq!(machine_boost(&s, 2), 1.0);

        let hungry = ColonyState {
            colonists: 2,
            food_stock: 0.0,
        };
        assert_eq!(machine_boost(&hungry, 0), 1.0);
    }
}
