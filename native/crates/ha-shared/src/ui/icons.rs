//! UI icon catalog — mirror of `packages/shared/src/content/ui/icons.ts`.

use crate::assets::PaletteKey;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiIconShape {
    Heart,
    Drumstick,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiIconVariant {
    Full,
    Half,
    Empty,
}

#[derive(Clone, Copy, Debug)]
pub struct UiIconCell {
    pub x: i32,
    pub y: i32,
    pub skip_in_half: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct UiIconDef {
    pub kind: &'static str,
    pub shape: UiIconShape,
    pub variant: UiIconVariant,
    pub fill_palette: PaletteKey,
    pub bg_palette: PaletteKey,
    pub grid: i32,
    pub px: u32,
    pub cells: &'static [UiIconCell],
}

const HEART_CELLS: &[UiIconCell] = &[
    UiIconCell { x: 1, y: 0, skip_in_half: false },
    UiIconCell { x: 3, y: 0, skip_in_half: true },
    UiIconCell { x: 0, y: 1, skip_in_half: false },
    UiIconCell { x: 1, y: 1, skip_in_half: false },
    UiIconCell { x: 2, y: 1, skip_in_half: false },
    UiIconCell { x: 3, y: 1, skip_in_half: true },
    UiIconCell { x: 4, y: 1, skip_in_half: true },
    UiIconCell { x: 0, y: 2, skip_in_half: false },
    UiIconCell { x: 1, y: 2, skip_in_half: false },
    UiIconCell { x: 2, y: 2, skip_in_half: false },
    UiIconCell { x: 3, y: 2, skip_in_half: true },
    UiIconCell { x: 4, y: 2, skip_in_half: true },
    UiIconCell { x: 1, y: 3, skip_in_half: false },
    UiIconCell { x: 2, y: 3, skip_in_half: false },
    UiIconCell { x: 3, y: 3, skip_in_half: true },
    UiIconCell { x: 2, y: 4, skip_in_half: false },
];

const DRUMSTICK_CELLS: &[UiIconCell] = &[
    UiIconCell { x: 1, y: 0, skip_in_half: false },
    UiIconCell { x: 2, y: 0, skip_in_half: false },
    UiIconCell { x: 3, y: 0, skip_in_half: false },
    UiIconCell { x: 0, y: 1, skip_in_half: false },
    UiIconCell { x: 1, y: 1, skip_in_half: false },
    UiIconCell { x: 2, y: 1, skip_in_half: false },
    UiIconCell { x: 3, y: 1, skip_in_half: false },
    UiIconCell { x: 1, y: 2, skip_in_half: false },
    UiIconCell { x: 2, y: 2, skip_in_half: false },
    UiIconCell { x: 3, y: 2, skip_in_half: false },
    UiIconCell { x: 3, y: 3, skip_in_half: false },
    UiIconCell { x: 4, y: 4, skip_in_half: false },
];

pub const HEART_FULL: UiIconDef = UiIconDef {
    kind: "heart_full",
    shape: UiIconShape::Heart,
    variant: UiIconVariant::Full,
    fill_palette: PaletteKey::Foe,
    bg_palette: PaletteKey::Night,
    grid: 5,
    px: 8,
    cells: HEART_CELLS,
};

pub const HEART_HALF: UiIconDef = UiIconDef {
    kind: "heart_half",
    shape: UiIconShape::Heart,
    variant: UiIconVariant::Half,
    fill_palette: PaletteKey::PotionRed,
    bg_palette: PaletteKey::Night,
    grid: 5,
    px: 8,
    cells: HEART_CELLS,
};

pub const HEART_EMPTY: UiIconDef = UiIconDef {
    kind: "heart_empty",
    shape: UiIconShape::Heart,
    variant: UiIconVariant::Empty,
    fill_palette: PaletteKey::Night,
    bg_palette: PaletteKey::Night,
    grid: 5,
    px: 8,
    cells: HEART_CELLS,
};

pub const DRUMSTICK_FULL: UiIconDef = UiIconDef {
    kind: "drumstick_full",
    shape: UiIconShape::Drumstick,
    variant: UiIconVariant::Full,
    fill_palette: PaletteKey::Gold,
    bg_palette: PaletteKey::Earth,
    grid: 5,
    px: 8,
    cells: DRUMSTICK_CELLS,
};

pub const DRUMSTICK_HALF: UiIconDef = UiIconDef {
    kind: "drumstick_half",
    shape: UiIconShape::Drumstick,
    variant: UiIconVariant::Half,
    fill_palette: PaletteKey::PotionGreen,
    bg_palette: PaletteKey::Earth,
    grid: 5,
    px: 8,
    cells: DRUMSTICK_CELLS,
};

pub const DRUMSTICK_EMPTY: UiIconDef = UiIconDef {
    kind: "drumstick_empty",
    shape: UiIconShape::Drumstick,
    variant: UiIconVariant::Empty,
    fill_palette: PaletteKey::Earth,
    bg_palette: PaletteKey::Earth,
    grid: 5,
    px: 8,
    cells: DRUMSTICK_CELLS,
};

pub const UI_ICON_CATALOG: &[UiIconDef] = &[
    HEART_FULL,
    HEART_HALF,
    HEART_EMPTY,
    DRUMSTICK_FULL,
    DRUMSTICK_HALF,
    DRUMSTICK_EMPTY,
];

pub fn get_ui_icon(kind: &str) -> Option<&'static UiIconDef> {
    UI_ICON_CATALOG.iter().find(|i| i.kind == kind)
}

pub fn require_ui_icon(kind: &str) -> &'static UiIconDef {
    get_ui_icon(kind).unwrap_or_else(|| panic!("unknown ui icon: {kind}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_six_hud_icons() {
        assert_eq!(UI_ICON_CATALOG.len(), 6);
    }

    #[test]
    fn require_ui_icon_works() {
        assert_eq!(require_ui_icon("heart_full").shape, UiIconShape::Heart);
    }
}
