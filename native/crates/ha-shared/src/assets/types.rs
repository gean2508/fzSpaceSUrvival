use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetClass {
    Terrain,
    Flora360,
    FloraFlat,
    Prop,
    Item,
    Character,
    Creature,
    Ui,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetCategory {
    Terrain,
    Flora,
    Rock,
    Construction,
    Equipment,
    Consumable,
    Material,
    Character,
    Creature,
    Ui,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaletteKey {
    Meadow,
    Earth,
    Bark,
    Hero,
    Foe,
    NeonPink,
    NeonCyan,
    Night,
    Fog,
    Gold,
    Crystal,
    PotionRed,
    PotionBlue,
    PotionGreen,
    Wood,
    Stone,
    Iron,
    Leather,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct AssetRenderHints {
    pub instanced: bool,
    pub billboard: bool,
    pub shadows: bool,
    pub scale: f32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct AssetDef {
    pub kind: &'static str,
    pub class: AssetClass,
    pub category: AssetCategory,
    pub palette: PaletteKey,
    pub hints: AssetRenderHints,
}
