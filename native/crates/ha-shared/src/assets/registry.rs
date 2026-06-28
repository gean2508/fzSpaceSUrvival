use super::catalog::ASSET_CATALOG;
use super::types::AssetDef;

pub fn get_asset(kind: &str) -> Option<&'static AssetDef> {
    ASSET_CATALOG.iter().find(|a| a.kind == kind)
}

pub fn require_asset(kind: &str) -> &'static AssetDef {
    if let Some(asset) = get_asset(kind) {
        return asset;
    }
    eprintln!("ha-shared: unknown asset kind '{kind}', using fallback 'prop_crate'");
    get_asset("prop_crate").expect("fallback asset kind must exist")
}

pub fn assets_by_craft_tag(tag: &str) -> Vec<&'static AssetDef> {
    ASSET_CATALOG
        .iter()
        .filter(|a| {
            // craft tags live in TS catalog; Rust mirror uses kind prefixes for filters
            a.kind.contains(tag) || tag.is_empty()
        })
        .collect()
}

pub fn asset_kind_for_interactable(kind: &str, variant: u32) -> &'static str {
    match kind {
        "tree" if variant % 2 == 0 => "flora_tree_oak",
        "tree" => "flora_tree_pine",
        "rock" if variant % 2 == 0 => "rock_medium",
        "rock" => "rock_boulder",
        "ore_iron" => "rock_boulder",
        "ore_gold" => "crystal_yellow",
        "gem_blue" => "crystal_blue",
        "gem_red" => "crystal_purple",
        "container" if variant % 2 == 0 => "prop_crate",
        _ => "prop_barrel",
    }
}

pub fn asset_kind_for_world_prop(kind: &str) -> &'static str {
    match kind {
        "house" => "building_cottage",
        "bridge" => "prop_bridge",
        _ => "prop_crate",
    }
}

pub fn asset_kind_for_obstacle() -> &'static str {
    "terrain_grass"
}
