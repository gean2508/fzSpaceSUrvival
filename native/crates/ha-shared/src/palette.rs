//! Port of `packages/client-gfx/src/art/palette.ts` + HYBRID_LIGHTS.

/// Linear RGB in 0..1 — identical hex values to TypeScript PALETTE.
#[derive(Clone, Copy, Debug)]
pub struct Palette;

impl Palette {
    pub const SKY_ZENITH: [f32; 3] = hex_rgb("4a2d6e");
    pub const SKY_TOP: [f32; 3] = hex_rgb("6eb8e8");
    pub const SKY_HORIZON: [f32; 3] = hex_rgb("f5c8e8");
    pub const SKY_WARM: [f32; 3] = hex_rgb("ffb8d0");
    pub const SKY_NADIR: [f32; 3] = hex_rgb("5a9e5a");
    pub const SUNSET: [f32; 3] = hex_rgb("f4a259");
    pub const FOG: [f32; 3] = hex_rgb("b8c8e8");
    pub const MEADOW: [f32; 3] = hex_rgb("8ed67a");
    pub const MEADOW_DEEP: [f32; 3] = hex_rgb("4a8e44");
    pub const EARTH: [f32; 3] = hex_rgb("caa472");
    /// Ink outline — inverted hull color.
    pub const BARK: [f32; 3] = hex_rgb("2b2f3a");
    pub const WATER: [f32; 3] = hex_rgb("5eead4");
    pub const HERO: [f32; 3] = hex_rgb("00e5ff");
    pub const RIVAL: [f32; 3] = hex_rgb("ff6b9d");
    pub const FOE: [f32; 3] = hex_rgb("ff8c42");
    pub const SPARK: [f32; 3] = hex_rgb("fff59a");
    pub const CREAM: [f32; 3] = hex_rgb("fff7e6");
    pub const NIGHT: [f32; 3] = hex_rgb("1a0a24");
    pub const STONE: [f32; 3] = hex_rgb("8a7a9e");
    pub const WOOD: [f32; 3] = hex_rgb("b6894f");
    pub const PETAL: [f32; 3] = hex_rgb("ff8ec8");
    pub const MUSHROOM: [f32; 3] = hex_rgb("e8d4f8");
    pub const NEON_PINK: [f32; 3] = hex_rgb("ff2a6d");
    pub const NEON_CYAN: [f32; 3] = hex_rgb("00f5ff");
    pub const VOID_PURPLE: [f32; 3] = hex_rgb("1a0033");
    pub const TWILIGHT: [f32; 3] = hex_rgb("7b2ff7");
    pub const LASER: [f32; 3] = hex_rgb("05d9e8");
}

#[derive(Clone, Copy, Debug)]
pub struct HybridLights;

impl HybridLights {
    pub const AMBIENT: [f32; 3] = hex_rgb("8a7ab8");
    pub const HEMI_SKY: [f32; 3] = hex_rgb("9ad4ff");
    pub const HEMI_GROUND: [f32; 3] = hex_rgb("3d7a3a");
    pub const SUN: [f32; 3] = hex_rgb("ffb347");
    pub const SUN_NEON_MIX: f32 = 0.28;
    pub const FOG_DENSITY: f32 = 0.011;
}

pub fn palette() -> Palette {
    Palette
}

pub fn hybrid_lights() -> HybridLights {
    HybridLights
}

const fn hex_rgb(hex: &str) -> [f32; 3] {
    let bytes = hex.as_bytes();
    let r = (hex_nibble(bytes[0]) * 16 + hex_nibble(bytes[1])) as f32 / 255.0;
    let g = (hex_nibble(bytes[2]) * 16 + hex_nibble(bytes[3])) as f32 / 255.0;
    let b = (hex_nibble(bytes[4]) * 16 + hex_nibble(bytes[5])) as f32 / 255.0;
    [r, g, b]
}

const fn hex_nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bark_matches_ts_palette() {
        assert!((Palette::BARK[0] - 0x2b as f32 / 255.0).abs() < 0.001);
        assert!((Palette::BARK[1] - 0x2f as f32 / 255.0).abs() < 0.001);
        assert!((Palette::BARK[2] - 0x3a as f32 / 255.0).abs() < 0.001);
    }
}
