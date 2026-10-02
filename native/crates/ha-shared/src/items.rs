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
        "chest" => "Chest",
        "sword_iron" => "Iron Sword",
        "sword_steel" => "Steel Sword",
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
        "leather_cap" => "Leather Cap",
        "leather_vest" => "Leather Vest",
        "wool_coat" => "Wool Coat",
        "leather_boots" => "Leather Boots",
        "wheat_seeds" => "Wheat Seeds",
        "wheat" => "Wheat",
        "carrot" => "Carrot",
        "pumpkin_seeds" => "Pumpkin Seeds",
        "pumpkin" => "Pumpkin",
        "beef" => "Raw Beef",
        "pork" => "Raw Pork",
        "chicken" => "Raw Chicken",
        "mutton" => "Raw Mutton",
        "rabbit_meat" => "Raw Rabbit",
        "feather" => "Feather",
        "wool" => "Wool",
        "bone" => "Bone",
        "egg" => "Egg",
        "name_tag" => "Name Tag",
        "bucket" => "Bucket",
        "water_bucket" => "Water Bucket",
        "milk_bucket" => "Milk Bucket",
        "lava_bucket" => "Lava Bucket",
        "supply" => "Supply Pack",
        "map_small" => "Small Map",
        "map_large" => "Large Map",
        // Ropa encontrable (Rust-only, ver `armadura::es_hallable`).
        id if id.starts_with("ropa_") => "Clothes",
        id if id.starts_with("gorra_") || id.starts_with("gorro_") => "Cap",
        id if id.starts_with("botas_") => "Footwear",
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
