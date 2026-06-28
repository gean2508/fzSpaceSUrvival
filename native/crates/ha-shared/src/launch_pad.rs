//! Launch pad assembly — mirror of `packages/shared/src/sim/launch_pad.ts`.

use crate::inventory::PlayerInventory;

pub const LAUNCH_PAD_MODULE_COUNT: u8 = 3;

#[derive(Clone, Debug, PartialEq)]
pub struct LaunchPadState {
    pub uid: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub assembled_modules: u8,
    pub ready: bool,
}

pub struct ModuleCost {
    pub item_id: &'static str,
    pub qty: u32,
}

pub static LAUNCH_MODULE_COSTS: &[&[ModuleCost]] = &[
    &[
        ModuleCost {
            item_id: "stone",
            qty: 15,
        },
        ModuleCost {
            item_id: "iron_ingot",
            qty: 5,
        },
    ],
    &[
        ModuleCost {
            item_id: "wood",
            qty: 20,
        },
        ModuleCost {
            item_id: "sticks",
            qty: 8,
        },
    ],
    &[
        ModuleCost {
            item_id: "iron_ingot",
            qty: 10,
        },
        ModuleCost {
            item_id: "gold_ingot",
            qty: 3,
        },
    ],
];

pub fn create_launch_pad_state(uid: u32, x: f32, y: f32, z: f32) -> LaunchPadState {
    LaunchPadState {
        uid,
        x,
        y,
        z,
        assembled_modules: 0,
        ready: false,
    }
}

pub fn can_assemble_module(pad: &LaunchPadState, module_index: u8, inv: &PlayerInventory) -> bool {
    if module_index >= LAUNCH_PAD_MODULE_COUNT {
        return false;
    }
    if pad.assembled_modules > module_index {
        return false;
    }
    if pad.assembled_modules != module_index {
        return false;
    }
    let Some(costs) = LAUNCH_MODULE_COSTS.get(module_index as usize) else {
        return false;
    };
    let ingredients: Vec<(&str, u32)> = costs
        .iter()
        .map(|c| (c.item_id, c.qty))
        .collect();
    inv.has_ingredients(&ingredients)
}

pub fn assemble_module(pad: &mut LaunchPadState, module_index: u8, inv: &mut PlayerInventory) -> bool {
    if !can_assemble_module(pad, module_index, inv) {
        return false;
    }
    let costs = LAUNCH_MODULE_COSTS[module_index as usize];
    let ingredients: Vec<(&str, u32)> = costs
        .iter()
        .map(|c| (c.item_id, c.qty))
        .collect();
    if !inv.consume_ingredients(&ingredients) {
        return false;
    }
    pad.assembled_modules += 1;
    pad.ready = pad.assembled_modules >= LAUNCH_PAD_MODULE_COUNT;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modules_assemble_in_order() {
        let mut pad = create_launch_pad_state(1, 28.0, 0.0, -28.0);
        let mut inv = PlayerInventory::default();
        inv.add("stone", 20);
        inv.add("iron_ingot", 20);
        inv.add("wood", 30);
        inv.add("sticks", 10);
        inv.add("gold_ingot", 5);
        assert!(!can_assemble_module(&pad, 1, &inv));
        assert!(assemble_module(&mut pad, 0, &mut inv));
        assert!(assemble_module(&mut pad, 1, &mut inv));
        assert!(assemble_module(&mut pad, 2, &mut inv));
        assert!(pad.ready);
        assert_eq!(pad.assembled_modules, 3);
    }
}
