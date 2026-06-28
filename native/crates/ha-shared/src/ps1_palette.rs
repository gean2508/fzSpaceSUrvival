//! PlayStation 1 era palette — muted, banded, high-contrast (visual layer only).

#[derive(Clone, Copy, Debug)]
pub struct Ps1Retro;

impl Ps1Retro {
    /// Deep menu void — FF7 / MGS title screen navy.
    pub const VOID: [f32; 3] = hex("141820");
    pub const PANEL: [f32; 3] = hex("384868");
    pub const PANEL_HI: [f32; 3] = hex("6878a0");
    pub const PANEL_LO: [f32; 3] = hex("182028");
    pub const TEXT: [f32; 3] = hex("e8e8f0");
    pub const TEXT_DIM: [f32; 3] = hex("98a0b8");
    pub const SELECT: [f32; 3] = hex("e8d060");
    pub const ACCENT: [f32; 3] = hex("5080c0");

    /// World sky bands (no neon).
    pub const SKY_ZENITH: [f32; 3] = hex("506888");
    pub const SKY_HORIZON: [f32; 3] = hex("98a8c0");
    pub const SKY_GROUND: [f32; 3] = hex("485840");
    pub const FOG: [f32; 3] = hex("7888a0");

    /// Terrain tints (rich but 5-bit feel).
    pub const GRASS: [f32; 3] = hex("58a050");
    pub const DIRT: [f32; 3] = hex("907050");
    pub const STONE: [f32; 3] = hex("686868");
    pub const SAND: [f32; 3] = hex("c0a868");
    pub const SNOW: [f32; 3] = hex("d0d8e0");
    pub const WATER: [f32; 3] = hex("4088a8");

    pub const SUN: [f32; 3] = hex("f0d890");
    pub const AMBIENT: [f32; 3] = hex("606878");

    /// Vertex snap virtual resolution + fog metres + dither strength.
    pub const SNAP_RES: f32 = 180.0;
    pub const FOG_START: f32 = 42.0;
    pub const FOG_END: f32 = 128.0;
    pub const DITHER: f32 = 1.0;
}

const fn hex(s: &str) -> [f32; 3] {
    let b = s.as_bytes();
    [
        (n(b[0]) * 16 + n(b[1])) as f32 / 255.0,
        (n(b[2]) * 16 + n(b[3])) as f32 / 255.0,
        (n(b[4]) * 16 + n(b[5])) as f32 / 255.0,
    ]
}

const fn n(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        _ => 0,
    }
}
