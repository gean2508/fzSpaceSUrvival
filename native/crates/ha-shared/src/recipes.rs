//! Craft recipes mirror — keep in sync with `packages/shared/src/content/items/recipes.ts`.

use crate::inventory::PlayerInventory;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CraftRecipe {
    pub id: u32,
    pub display_name: &'static str,
    pub ingredients: &'static [(&'static str, u32)],
    pub output_id: &'static str,
    pub output_qty: u32,
}

pub const CRAFT_RECIPES: &[CraftRecipe] = &[
    CraftRecipe {
        id: 1,
        display_name: "Stone Pickaxe",
        ingredients: &[("wood", 3), ("stone", 2), ("sticks", 1)],
        output_id: "pickaxe",
        output_qty: 1,
    },
    CraftRecipe {
        id: 2,
        display_name: "Firefly Torch",
        ingredients: &[("wood", 2), ("gem_blue", 1), ("potion_green", 1)],
        output_id: "potion_blue",
        output_qty: 1,
    },
    CraftRecipe {
        id: 3,
        display_name: "Wood Shield",
        ingredients: &[("wood", 4), ("leather", 1)],
        output_id: "shield_wood",
        output_qty: 1,
    },
    CraftRecipe {
        id: 4,
        display_name: "Iron Ingot",
        ingredients: &[("stone", 3), ("wood", 1)],
        output_id: "iron_ingot",
        output_qty: 1,
    },
    CraftRecipe {
        id: 5,
        display_name: "Gold Ingot",
        ingredients: &[("iron_ingot", 2), ("stone", 2)],
        output_id: "gold_ingot",
        output_qty: 1,
    },
    CraftRecipe {
        id: 6,
        display_name: "Jetpack",
        ingredients: &[("iron_ingot", 2), ("gold_ingot", 1), ("sticks", 4)],
        output_id: "jetpack",
        output_qty: 1,
    },
];

pub fn get_craft_recipe(id: u32) -> Option<&'static CraftRecipe> {
    CRAFT_RECIPES.iter().find(|r| r.id == id)
}

pub fn can_craft(inv: &PlayerInventory, recipe: &CraftRecipe) -> bool {
    inv.has_ingredients(recipe.ingredients)
}

pub fn craft(inv: &mut PlayerInventory, recipe: &CraftRecipe) -> bool {
    if !inv.consume_ingredients(recipe.ingredients) {
        return false;
    }
    inv.add(recipe.output_id, recipe.output_qty);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pickaxe_recipe_matches_ts() {
        let recipe = get_craft_recipe(1).expect("pickaxe recipe");
        assert_eq!(recipe.ingredients.len(), 3);
        assert_eq!(recipe.output_id, "pickaxe");
    }

    #[test]
    fn craft_consumes_and_outputs() {
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
    fn craft_fails_without_materials() {
        let recipe = get_craft_recipe(2).unwrap();
        let mut inv = PlayerInventory::starter();
        assert!(!craft(&mut inv, recipe));
        assert_eq!(inv.qty("potion_blue"), 0);
    }
}
