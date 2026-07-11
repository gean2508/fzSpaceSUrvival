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

// Minecraft-style tech tree — MUST mirror packages/shared/.../recipes.ts.
// Early tier (sticks → wood tools → stone tools) bootstraps from an empty
// inventory; tools gate what you can mine via the interaction rules.
pub const CRAFT_RECIPES: &[CraftRecipe] = &[
    CraftRecipe {
        id: 1,
        display_name: "Sticks",
        ingredients: &[("wood", 2)],
        output_id: "sticks",
        output_qty: 4,
    },
    CraftRecipe {
        id: 2,
        display_name: "Wood Pickaxe",
        ingredients: &[("wood", 3), ("sticks", 2)],
        output_id: "wood_pickaxe",
        output_qty: 1,
    },
    CraftRecipe {
        id: 3,
        display_name: "Wood Axe",
        ingredients: &[("wood", 3), ("sticks", 2)],
        output_id: "wood_axe",
        output_qty: 1,
    },
    CraftRecipe {
        id: 4,
        display_name: "Stone Pickaxe",
        ingredients: &[("stone", 3), ("sticks", 2)],
        output_id: "stone_pickaxe",
        output_qty: 1,
    },
    CraftRecipe {
        id: 5,
        display_name: "Stone Axe",
        ingredients: &[("stone", 3), ("sticks", 2)],
        output_id: "stone_axe",
        output_qty: 1,
    },
    CraftRecipe {
        id: 6,
        display_name: "Iron Ingot",
        ingredients: &[("stone", 3), ("wood", 1)],
        output_id: "iron_ingot",
        output_qty: 1,
    },
    CraftRecipe {
        id: 7,
        display_name: "Gold Ingot",
        ingredients: &[("iron_ingot", 2), ("stone", 2)],
        output_id: "gold_ingot",
        output_qty: 1,
    },
    CraftRecipe {
        id: 8,
        display_name: "Lantern",
        ingredients: &[("wood", 2), ("iron_ingot", 1), ("sticks", 2)],
        output_id: "lantern",
        output_qty: 1,
    },
    CraftRecipe {
        id: 9,
        display_name: "Firefly Torch",
        ingredients: &[("wood", 2), ("gem_blue", 1), ("sticks", 2)],
        output_id: "potion_blue",
        output_qty: 1,
    },
    CraftRecipe {
        id: 10,
        display_name: "Wood Shield",
        ingredients: &[("wood", 4), ("iron_ingot", 1)],
        output_id: "shield_wood",
        output_qty: 1,
    },
    CraftRecipe {
        id: 11,
        display_name: "Jetpack",
        ingredients: &[("iron_ingot", 2), ("gold_ingot", 1), ("sticks", 4)],
        output_id: "jetpack",
        output_qty: 1,
    },
    CraftRecipe {
        id: 12,
        display_name: "Nav Probe",
        ingredients: &[("iron_ingot", 2), ("gold_ingot", 1), ("stone", 2)],
        output_id: "probe",
        output_qty: 1,
    },
    CraftRecipe {
        id: 13,
        display_name: "Fruit Pie",
        ingredients: &[("apple", 3), ("wood", 1)],
        output_id: "pie",
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
    fn sticks_recipe_bootstraps_from_wood() {
        let recipe = get_craft_recipe(1).expect("sticks recipe");
        assert_eq!(recipe.output_id, "sticks");
        assert_eq!(recipe.output_qty, 4);
    }

    #[test]
    fn wood_pickaxe_craftable_from_tree_drops_only() {
        // Empty-start bootstrap: wood + sticks (both from punching trees) →
        // wood pickaxe. No stone/metal needed.
        let recipe = get_craft_recipe(2).unwrap();
        let mut inv = PlayerInventory::default();
        inv.add("wood", 3);
        inv.add("sticks", 2);
        assert!(craft(&mut inv, recipe));
        assert_eq!(inv.qty("wood_pickaxe"), 1);
        assert_eq!(inv.qty("wood"), 0);
    }

    #[test]
    fn craft_fails_without_materials() {
        let recipe = get_craft_recipe(4).unwrap(); // stone pickaxe
        let mut inv = PlayerInventory::starter();
        assert!(!craft(&mut inv, recipe));
        assert_eq!(inv.qty("stone_pickaxe"), 0);
    }
}
