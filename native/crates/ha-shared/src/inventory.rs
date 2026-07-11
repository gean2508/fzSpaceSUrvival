//! Player inventory mirror — keep in sync with `packages/shared/src/sim/inventory.ts`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::building::BuildPieceKind;
use crate::machines::MachineDef;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayerInventory {
    pub items: HashMap<String, u32>,
}

impl PlayerInventory {
    /// Minecraft-style cold start: empty hands. Punch a tree → wood → sticks →
    /// wood pickaxe → mine stone → stone pickaxe → ore. No handouts.
    pub fn starter() -> Self {
        Self::default()
    }

    /// Kept for the load path; no longer tops up (empty-start progression).
    pub fn ensure_agency_demo_kit(&mut self) {}

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

    pub fn can_afford_build(&self, piece: BuildPieceKind) -> bool {
        self.has_ingredients(piece.build_cost())
    }

    pub fn consume_build_cost(&mut self, piece: BuildPieceKind) -> bool {
        self.consume_ingredients(piece.build_cost())
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
    fn starter_is_empty() {
        let inv = PlayerInventory::starter();
        assert_eq!(inv.qty("wood"), 0);
        assert_eq!(inv.qty("iron_ingot"), 0);
        assert!(inv.items.is_empty());
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
    fn craft_wood_pickaxe_parity_with_ts() {
        let recipe = get_craft_recipe(2).unwrap(); // wood pickaxe
        let mut inv = PlayerInventory::default();
        inv.add("wood", 3);
        inv.add("sticks", 2);
        assert!(craft(&mut inv, recipe));
        assert_eq!(inv.qty("wood_pickaxe"), 1);
        assert_eq!(inv.qty("wood"), 0);
    }

    #[test]
    fn insufficient_craft_leaves_inventory() {
        let recipe = get_craft_recipe(1).unwrap(); // sticks (needs wood 2)
        let mut inv = PlayerInventory::default();
        inv.add("wood", 1);
        assert!(!craft(&mut inv, recipe));
        assert_eq!(inv.qty("wood"), 1);
        assert_eq!(inv.qty("sticks"), 0);
    }
}
