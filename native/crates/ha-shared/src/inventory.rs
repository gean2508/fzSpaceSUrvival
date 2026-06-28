//! Player inventory mirror — keep in sync with `packages/shared/src/sim/inventory.ts`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::machines::MachineDef;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayerInventory {
    pub items: HashMap<String, u32>,
}

impl PlayerInventory {
    pub fn starter() -> Self {
        let mut inv = Self::default();
        inv.add("wood", 48);
        inv.add("stone", 40);
        inv.add("dirt", 48);
        inv.add("sticks", 18);
        inv.add("iron_ingot", 30);
        inv.add("gold_ingot", 5);
        inv
    }

    /// Top up legacy saves that predate agency demo kit.
    pub fn ensure_agency_demo_kit(&mut self) {
        if self.qty("iron_ingot") >= 15 {
            return;
        }
        self.add("wood", 40);
        self.add("stone", 35);
        self.add("dirt", 32);
        self.add("sticks", 16);
        self.add("iron_ingot", 30);
        self.add("gold_ingot", 5);
    }

    pub fn qty(&self, item_id: &str) -> u32 {
        *self.items.get(item_id).unwrap_or(&0)
    }

    pub fn add(&mut self, item_id: &str, qty: u32) {
        if qty == 0 {
            return;
        }
        *self.items.entry(item_id.to_string()).or_insert(0) += qty;
    }

    pub fn remove(&mut self, item_id: &str, qty: u32) -> bool {
        if qty == 0 {
            return true;
        }
        let has = self.qty(item_id);
        if has < qty {
            return false;
        }
        let next = has - qty;
        if next == 0 {
            self.items.remove(item_id);
        } else {
            self.items.insert(item_id.to_string(), next);
        }
        true
    }

    pub fn has_ingredients(&self, ingredients: &[(&str, u32)]) -> bool {
        ingredients
            .iter()
            .all(|(id, qty)| self.qty(id) >= *qty)
    }

    pub fn consume_ingredients(&mut self, ingredients: &[(&str, u32)]) -> bool {
        if !self.has_ingredients(ingredients) {
            return false;
        }
        for (id, qty) in ingredients {
            self.remove(id, *qty);
        }
        true
    }

    pub fn can_afford_machine(&self, def: &MachineDef) -> bool {
        self.qty("wood") >= def.cost_wood
            && self.qty("stone") >= def.cost_stone
            && self.qty("sticks") >= def.cost_sticks
            && self.qty("iron_ingot") >= def.cost_iron
    }

    pub fn consume_machine_cost(&mut self, def: &MachineDef) -> bool {
        if !self.can_afford_machine(def) {
            return false;
        }
        self.remove("wood", def.cost_wood);
        self.remove("stone", def.cost_stone);
        self.remove("sticks", def.cost_sticks);
        if def.cost_iron > 0 {
            self.remove("iron_ingot", def.cost_iron);
        }
        true
    }

    pub fn sorted_entries(&self) -> Vec<(&str, u32)> {
        let mut out: Vec<_> = self
            .items
            .iter()
            .map(|(k, v)| (k.as_str(), *v))
            .collect();
        out.sort_by(|a, b| a.0.cmp(b.0));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipes::{craft, get_craft_recipe};

    #[test]
    fn starter_matches_ts_defaults() {
        let inv = PlayerInventory::starter();
        assert_eq!(inv.qty("wood"), 48);
        assert_eq!(inv.qty("iron_ingot"), 30);
        assert_eq!(inv.qty("gold_ingot"), 5);
    }

    #[test]
    fn add_remove_and_clear_slot() {
        let mut inv = PlayerInventory::default();
        inv.add("wood", 3);
        inv.add("wood", 2);
        assert_eq!(inv.qty("wood"), 5);
        assert!(inv.remove("wood", 5));
        assert_eq!(inv.qty("wood"), 0);
        assert!(!inv.items.contains_key("wood"));
    }

    #[test]
    fn craft_pickaxe_parity_with_ts() {
        let recipe = get_craft_recipe(1).unwrap();
        let mut inv = PlayerInventory::default();
        inv.add("wood", 3);
        inv.add("stone", 2);
        inv.add("sticks", 1);
        assert!(craft(&mut inv, recipe));
        assert_eq!(inv.qty("pickaxe"), 1);
        assert_eq!(inv.qty("wood"), 0);
    }

    #[test]
    fn insufficient_craft_leaves_inventory() {
        let recipe = get_craft_recipe(1).unwrap();
        let mut inv = PlayerInventory::default();
        inv.add("wood", 1);
        assert!(!craft(&mut inv, recipe));
        assert_eq!(inv.qty("wood"), 1);
        assert_eq!(inv.qty("pickaxe"), 0);
    }
}
