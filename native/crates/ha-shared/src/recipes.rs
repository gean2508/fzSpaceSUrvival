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
    CraftRecipe {
        id: 14,
        display_name: "Fuel Tank",
        ingredients: &[("iron_ingot", 6), ("stone", 2)],
        output_id: "part_tank",
        output_qty: 1,
    },
    CraftRecipe {
        id: 15,
        display_name: "Rocket Engine",
        ingredients: &[("iron_ingot", 8), ("gold_ingot", 2)],
        output_id: "part_engine",
        output_qty: 1,
    },
    CraftRecipe {
        id: 16,
        display_name: "Lander Legs",
        ingredients: &[("iron_ingot", 4), ("sticks", 6)],
        output_id: "part_lander",
        output_qty: 1,
    },
    CraftRecipe {
        id: 17,
        display_name: "Aero Fin",
        ingredients: &[("iron_ingot", 3), ("wood", 4)],
        output_id: "part_fin",
        output_qty: 1,
    },
    CraftRecipe {
        id: 18,
        display_name: "Nose Cone",
        ingredients: &[("iron_ingot", 5), ("stone", 3)],
        output_id: "part_nose",
        output_qty: 1,
    },
    // Tier temprano de luz + almacenamiento + combate cuerpo a cuerpo: todo se
    // equipa desde la hotbar y se ve EN LA MANO del personaje (`held_item.rs`).
    CraftRecipe {
        id: 19,
        display_name: "Torch",
        ingredients: &[("wood", 1), ("sticks", 1)],
        output_id: "torch",
        output_qty: 4,
    },
    CraftRecipe {
        id: 20,
        display_name: "Chest",
        ingredients: &[("wood", 8)],
        output_id: "chest",
        output_qty: 1,
    },
    CraftRecipe {
        id: 21,
        display_name: "Iron Sword",
        ingredients: &[("iron_ingot", 2), ("sticks", 1)],
        output_id: "sword_iron",
        output_qty: 1,
    },
    CraftRecipe {
        id: 22,
        display_name: "Steel Sword",
        ingredients: &[("iron_ingot", 3), ("gold_ingot", 1), ("sticks", 1)],
        output_id: "sword_steel",
        output_qty: 1,
    },
    // **El mapa se craftea y se lleva en la mano** (estilo Minecraft): dejó de ser
    // una pantalla del menú —donde en el teléfono no se veía— y pasó a ser un
    // objeto. El chico cubre tu valle; el grande, la región entera, y cuesta lo
    // que corresponde a algo que te ahorra perderte.
    CraftRecipe {
        id: 23,
        display_name: "Mapa chico",
        ingredients: &[("wood", 2), ("leather", 1)],
        output_id: "map_small",
        output_qty: 1,
    },
    CraftRecipe {
        id: 24,
        display_name: "Mapa grande",
        ingredients: &[("map_small", 1), ("leather", 2), ("iron_ingot", 1)],
        output_id: "map_large",
        output_qty: 1,
    },
    // **Granja** (Minecraft): tres de trigo hacen un pan; una calabaza se parte
    // en cuatro semillas; calabaza + trigo es la tarta.
    CraftRecipe {
        id: 25,
        display_name: "Pan",
        ingredients: &[("wheat", 3)],
        output_id: "bread",
        output_qty: 1,
    },
    CraftRecipe {
        id: 26,
        display_name: "Semillas de calabaza",
        ingredients: &[("pumpkin", 1)],
        output_id: "pumpkin_seeds",
        output_qty: 4,
    },
    CraftRecipe {
        id: 27,
        display_name: "Tarta de calabaza",
        ingredients: &[("pumpkin", 1), ("wheat", 1)],
        output_id: "pie",
        output_qty: 1,
    },
    // **Etiqueta** para ponerle nombre a un animal (en Minecraft no se craftea;
    // acá sí, pedido del usuario: es la forma de "hacerlo tuyo").
    CraftRecipe {
        id: 28,
        display_name: "Etiqueta",
        ingredients: &[("leather", 1), ("sticks", 1)],
        output_id: "name_tag",
        output_qty: 1,
    },
    // La torta de la granja: trigo, huevo y la leche que no hay → pan dulce.
    CraftRecipe {
        id: 29,
        display_name: "Pan dulce",
        ingredients: &[("wheat", 2), ("egg", 1)],
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
