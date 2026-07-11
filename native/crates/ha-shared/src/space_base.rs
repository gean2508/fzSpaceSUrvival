//! Orbital station assembly — mirror of `packages/shared/src/sim/space_base.ts`.

use crate::inventory::PlayerInventory;

pub const SPACE_STATION_MODULE_COUNT: u8 = 3;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpaceStationState {
    pub assembled_modules: u8,
    pub ready: bool,
}

pub struct ModuleCost {
    pub item_id: &'static str,
    pub qty: u32,
}

pub static SPACE_MODULE_COSTS: &[&[ModuleCost]] = &[
    &[
        ModuleCost {
            item_id: "wood",
            qty: 8,
        },
        ModuleCost {
            item_id: "sticks",
            qty: 4,
        },
    ],
    &[
        ModuleCost {
            item_id: "stone",
            qty: 10,
        },
        ModuleCost {
            item_id: "iron_ingot",
            qty: 3,
        },
    ],
    &[
        ModuleCost {
            item_id: "iron_ingot",
            qty: 5,
        },
        ModuleCost {
            item_id: "gold_ingot",
            qty: 1,
        },
    ],
];

pub fn create_space_station_state() -> SpaceStationState {
    SpaceStationState {
        assembled_modules: 0,
        ready: false,
    }
}

pub fn can_assemble_space_module(
    station: &SpaceStationState,
    module_index: u8,
    inv: &PlayerInventory,
) -> bool {
    if module_index >= SPACE_STATION_MODULE_COUNT {
        return false;
    }
    if station.assembled_modules > module_index {
        return false;
    }
    if station.assembled_modules != module_index {
        return false;
    }
    let Some(costs) = SPACE_MODULE_COSTS.get(module_index as usize) else {
        return false;
    };
    let ingredients: Vec<(&str, u32)> = costs.iter().map(|c| (c.item_id, c.qty)).collect();
    inv.has_ingredients(&ingredients)
}

pub fn assemble_space_module(
    station: &mut SpaceStationState,
    module_index: u8,
    inv: &mut PlayerInventory,
) -> bool {
    if !can_assemble_space_module(station, module_index, inv) {
        return false;
    }
    let costs = SPACE_MODULE_COSTS[module_index as usize];
    let ingredients: Vec<(&str, u32)> = costs.iter().map(|c| (c.item_id, c.qty)).collect();
    if !inv.consume_ingredients(&ingredients) {
        return false;
    }
    station.assembled_modules += 1;
    station.ready = station.assembled_modules >= SPACE_STATION_MODULE_COUNT;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn station_modules_in_order() {
        let mut station = create_space_station_state();
        let mut inv = PlayerInventory::starter();
        inv.add("wood", 12);
        inv.add("sticks", 6);
        inv.add("stone", 20);
        inv.add("iron_ingot", 20);
        inv.add("gold_ingot", 5);
        assert!(assemble_space_module(&mut station, 0, &mut inv));
        assert!(assemble_space_module(&mut station, 1, &mut inv));
        assert!(assemble_space_module(&mut station, 2, &mut inv));
        assert!(station.ready);
    }
}
