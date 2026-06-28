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
        "pickaxe" => "Pickaxe",
        "hand" => "Hand",
        "torch" => "Torch",
        "shield_wood" => "Wood Shield",
        "iron_ingot" => "Iron",
        "gold_ingot" => "Gold",
        "jetpack" => "Jetpack",
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
