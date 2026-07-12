//! Item display names — mirror of `packages/shared` item catalog labels.

/// Human-readable label for HUD / inventory tooltips.
pub fn item_display_name(id: &str) -> &'static str {
    match id {
        "wood" => "Wood",
        "stone" => "Stone",
        "dirt" => "Dirt",
        "sand" => "Sand",
        "snow" => "Snow",
        "sticks" => "Sticks",
        "pickaxe" => "Iron Pickaxe",
        "wood_pickaxe" => "Wood Pickaxe",
        "stone_pickaxe" => "Stone Pickaxe",
        "wood_axe" => "Wood Axe",
        "stone_axe" => "Stone Axe",
        "axe" => "Iron Axe",
        "hand" => "Hand",
        "torch" => "Torch",
        "lantern" => "Lantern",
        "shield_wood" => "Wood Shield",
        "iron_ingot" => "Iron",
        "gold_ingot" => "Gold",
        "jetpack" => "Jetpack",
        "probe" => "Nav Probe",
        "part_tank" => "Fuel Tank",
        "part_engine" => "Rocket Engine",
        "part_lander" => "Lander Legs",
        "part_fin" => "Aero Fin",
        "part_nose" => "Nose Cone",
        "apple" => "Apple",
        "orange" => "Orange",
        "grape" => "Grapes",
        "bread" => "Bread",
        "pie" => "Fruit Pie",
        "meat" => "Meat",
        "soup" => "Soup",
        "gem_blue" => "Blue Gem",
        "potion_green" => "Green Potion",
        "potion_blue" => "Blue Potion",
        "leather" => "Leather",
        _ => "Item",
    }
}

/// Compact label for small hotbar slots (max ~5 chars).
pub fn item_hotbar_label(id: &str) -> String {
    let name = item_display_name(id);
    if name.len() <= 5 {
        return name.to_string();
    }
    name.chars().take(4).collect()
}
