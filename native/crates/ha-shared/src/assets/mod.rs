//! Procedural asset catalog — mirrors `packages/shared/src/content/assets`.

mod catalog;
mod registry;
mod types;

pub use catalog::ASSET_CATALOG;
pub use registry::{
    asset_kind_for_interactable, asset_kind_for_obstacle, asset_kind_for_world_prop,
    assets_by_craft_tag, get_asset, require_asset,
};
pub use types::{AssetCategory, AssetClass, AssetDef, AssetRenderHints, PaletteKey};

pub fn palette_rgb(key: PaletteKey) -> [f32; 3] {
    use crate::palette::Palette;
    match key {
        PaletteKey::Meadow => Palette::MEADOW,
        PaletteKey::Earth => Palette::EARTH,
        PaletteKey::Bark => Palette::BARK,
        PaletteKey::Hero => Palette::HERO,
        PaletteKey::Foe => Palette::FOE,
        PaletteKey::NeonPink => Palette::NEON_PINK,
        PaletteKey::NeonCyan => Palette::NEON_CYAN,
        PaletteKey::Night => Palette::NIGHT,
        PaletteKey::Fog => Palette::FOG,
        PaletteKey::Gold => Palette::SPARK,
        PaletteKey::Crystal => Palette::NEON_CYAN,
        PaletteKey::PotionRed => Palette::FOE,
        PaletteKey::PotionBlue => Palette::HERO,
        PaletteKey::PotionGreen => Palette::MEADOW,
        PaletteKey::Wood => Palette::WOOD,
        PaletteKey::Stone => Palette::STONE,
        PaletteKey::Iron => Palette::STONE,
        PaletteKey::Leather => Palette::EARTH,
    }
}
