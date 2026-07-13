//! Procedural inventory item icons (CPU raster, cached per item id).

use std::collections::HashMap;

use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use ha_shared::{palette_rgb, PaletteKey};

/// Faint chrome for empty inventory / hotbar cells.
pub const SLOT_EMPTY_ID: &str = "_slot_empty";
pub const BAG_SLOT_ID: &str = "_bag";

#[derive(Resource, Default)]
pub struct ItemIconCache {
    cache: HashMap<String, Handle<Image>>,
}

impl ItemIconCache {
    /// Seed a drop-in PNG icon (assets/art/items/<id>.png) so every consumer
    /// gets the pack art instead of the procedural raster.
    pub fn preload(&mut self, item_id: &str, handle: Handle<Image>) {
        self.cache.insert(item_id.to_string(), handle);
    }

    pub fn get(&mut self, item_id: &str, images: &mut Assets<Image>) -> Handle<Image> {
        if let Some(hit) = self.cache.get(item_id) {
            return hit.clone();
        }
        let handle = images.add(rasterize_item_icon(item_id));
        self.cache.insert(item_id.to_string(), handle.clone());
        handle
    }

    pub fn empty_slot(&mut self, images: &mut Assets<Image>) -> Handle<Image> {
        self.get(SLOT_EMPTY_ID, images)
    }

    pub fn bag_slot(&mut self, images: &mut Assets<Image>) -> Handle<Image> {
        self.get(BAG_SLOT_ID, images)
    }
}

pub fn rasterize_item_icon(item_id: &str) -> Image {
    let size = 40u32;
    let mut data = vec![0u8; (size * size * 4) as usize];
    let bg = rgba(PaletteKey::Night, 0.92);
    for y in 0..size {
        for x in 0..size {
            put_px(&mut data, size, x, y, bg);
        }
    }
    match item_id {
        SLOT_EMPTY_ID => draw_empty_slot(&mut data, size),
        BAG_SLOT_ID => draw_bag(&mut data, size),
        "wood" => draw_log(&mut data, size),
        "stone" => draw_stone(&mut data, size),
        "sticks" => draw_sticks(&mut data, size),
        "pickaxe" | "wood_pickaxe" | "stone_pickaxe" => draw_pickaxe(&mut data, size),
        "hammer" => draw_hammer(&mut data, size),
        "lantern" => draw_lantern(&mut data, size),
        "shield_wood" => draw_shield(&mut data, size),
        "jetpack" => draw_jetpack(&mut data, size),
        "iron_ingot" => draw_ingot(&mut data, size, PaletteKey::Iron),
        "gold_ingot" => draw_ingot(&mut data, size, PaletteKey::Gold),
        "potion_blue" | "potion_green" => draw_potion(&mut data, size, PaletteKey::NeonCyan),
        "gem_blue" => draw_gem(&mut data, size, PaletteKey::NeonCyan),
        "leather" => draw_leather(&mut data, size),
        "axe" | "wood_axe" | "stone_axe" => draw_axe(&mut data, size),
        "apple" => draw_fruit(&mut data, size, PaletteKey::Foe),
        "orange" => draw_fruit(&mut data, size, PaletteKey::Gold),
        "grape" => draw_fruit(&mut data, size, PaletteKey::NeonPink),
        "bread" | "pie" | "meat" | "soup" => draw_meal(&mut data, size),
        _ => draw_generic(&mut data, size, item_id),
    }
    Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

fn rgba(key: PaletteKey, alpha: f32) -> [u8; 4] {
    let rgb = palette_rgb(key);
    [
        (rgb[0] * 255.0) as u8,
        (rgb[1] * 255.0) as u8,
        (rgb[2] * 255.0) as u8,
        (alpha * 255.0) as u8,
    ]
}

fn put_px(data: &mut [u8], size: u32, x: u32, y: u32, c: [u8; 4]) {
    let i = ((y * size + x) * 4) as usize;
    if i + 3 < data.len() {
        data[i..i + 4].copy_from_slice(&c);
    }
}

fn fill_rect(data: &mut [u8], size: u32, x0: u32, y0: u32, x1: u32, y1: u32, c: [u8; 4]) {
    for y in y0..y1.min(size) {
        for x in x0..x1.min(size) {
            put_px(data, size, x, y, c);
        }
    }
}

fn draw_log(data: &mut [u8], size: u32) {
    let wood = rgba(PaletteKey::Wood, 1.0);
    let dark = rgba(PaletteKey::Earth, 1.0);
    fill_rect(data, size, 8, 14, 32, 26, wood);
    fill_rect(data, size, 10, 16, 12, 24, dark);
    fill_rect(data, size, 28, 16, 30, 24, dark);
}

fn draw_stone(data: &mut [u8], size: u32) {
    let stone = rgba(PaletteKey::Stone, 1.0);
    let dark = rgba(PaletteKey::Fog, 1.0);
    fill_rect(data, size, 10, 12, 30, 28, stone);
    fill_rect(data, size, 12, 14, 22, 20, dark);
    fill_rect(data, size, 20, 22, 28, 26, dark);
}

fn draw_sticks(data: &mut [u8], size: u32) {
    let stick = rgba(PaletteKey::Gold, 1.0);
    for i in 0..18 {
        let x = 10 + i;
        let y = 28 - i / 2;
        fill_rect(data, size, x, y, x + 3, y + 3, stick);
    }
    for i in 0..14 {
        let x = 18 + i;
        let y = 12 + i / 2;
        fill_rect(data, size, x, y, x + 3, y + 3, stick);
    }
}

fn draw_pickaxe(data: &mut [u8], size: u32) {
    let head = rgba(PaletteKey::Stone, 1.0);
    let handle = rgba(PaletteKey::Wood, 1.0);
    fill_rect(data, size, 8, 8, 28, 14, head);
    for i in 0..20 {
        let x = 12 + i;
        let y = 14 + i;
        fill_rect(data, size, x, y, x + 4, y + 4, handle);
    }
}

fn draw_hammer(data: &mut [u8], size: u32) {
    let head = rgba(PaletteKey::Iron, 1.0);
    let handle = rgba(PaletteKey::Wood, 1.0);
    fill_rect(data, size, 8, 8, 30, 16, head);
    fill_rect(data, size, 18, 16, 24, 34, handle);
}

fn draw_lantern(data: &mut [u8], size: u32) {
    let metal = rgba(PaletteKey::Iron, 1.0);
    let glow = rgba(PaletteKey::Gold, 1.0);
    let flame = rgba(PaletteKey::PotionRed, 1.0);
    fill_rect(data, size, 16, 6, 24, 10, metal);
    fill_rect(data, size, 14, 10, 26, 26, metal);
    fill_rect(data, size, 16, 12, 24, 24, glow);
    fill_rect(data, size, 18, 14, 22, 22, flame);
    fill_rect(data, size, 18, 26, 22, 32, metal);
}

fn draw_shield(data: &mut [u8], size: u32) {
    let body = rgba(PaletteKey::Wood, 1.0);
    let rim = rgba(PaletteKey::Gold, 1.0);
    fill_rect(data, size, 10, 8, 30, 30, body);
    fill_rect(data, size, 10, 8, 30, 12, rim);
    fill_rect(data, size, 10, 26, 30, 30, rim);
    fill_rect(data, size, 10, 8, 14, 30, rim);
    fill_rect(data, size, 26, 8, 30, 30, rim);
}

fn draw_empty_slot(data: &mut [u8], size: u32) {
    let edge = rgba(PaletteKey::Fog, 0.55);
    let inner = rgba(PaletteKey::Night, 0.35);
    fill_rect(data, size, 6, 6, 34, 34, inner);
    for i in 0..4 {
        fill_rect(data, size, 6 + i, 6, 8 + i, 8, edge);
        fill_rect(data, size, 32 - i, 6, 34 - i, 8, edge);
        fill_rect(data, size, 6 + i, 32, 8 + i, 34, edge);
        fill_rect(data, size, 32 - i, 32, 34 - i, 34, edge);
    }
}

fn draw_bag(data: &mut [u8], size: u32) {
    let body = rgba(PaletteKey::Earth, 1.0);
    let flap = rgba(PaletteKey::Gold, 1.0);
    let strap = rgba(PaletteKey::Wood, 1.0);
    fill_rect(data, size, 10, 14, 30, 32, body);
    fill_rect(data, size, 9, 10, 31, 16, flap);
    fill_rect(data, size, 18, 8, 22, 14, strap);
}

fn draw_jetpack(data: &mut [u8], size: u32) {
    let tank = rgba(PaletteKey::Iron, 1.0);
    let flame = rgba(PaletteKey::NeonPink, 1.0);
    fill_rect(data, size, 10, 8, 17, 28, tank);
    fill_rect(data, size, 23, 8, 30, 28, tank);
    fill_rect(data, size, 12, 28, 16, 34, flame);
    fill_rect(data, size, 24, 28, 28, 34, flame);
    fill_rect(data, size, 16, 12, 24, 22, rgba(PaletteKey::Night, 0.9));
}

fn draw_ingot(data: &mut [u8], size: u32, metal: PaletteKey) {
    let c = rgba(metal, 1.0);
    let shine = rgba(PaletteKey::Fog, 0.85);
    fill_rect(data, size, 11, 14, 29, 26, c);
    fill_rect(data, size, 13, 16, 22, 20, shine);
}

fn draw_potion(data: &mut [u8], size: u32, liquid: PaletteKey) {
    let glass = rgba(PaletteKey::Fog, 0.75);
    let fill = rgba(liquid, 1.0);
    fill_rect(data, size, 16, 10, 24, 14, glass);
    fill_rect(data, size, 13, 14, 27, 30, glass);
    fill_rect(data, size, 15, 18, 25, 28, fill);
}

fn draw_gem(data: &mut [u8], size: u32, tone: PaletteKey) {
    let c = rgba(tone, 1.0);
    fill_rect(data, size, 20, 8, 24, 14, c);
    fill_rect(data, size, 14, 14, 30, 22, c);
    fill_rect(data, size, 18, 22, 26, 30, c);
}

fn draw_leather(data: &mut [u8], size: u32) {
    let c = rgba(PaletteKey::Leather, 1.0);
    fill_rect(data, size, 10, 12, 30, 28, c);
    fill_rect(data, size, 14, 16, 26, 24, rgba(PaletteKey::Bark, 1.0));
}

fn draw_axe(data: &mut [u8], size: u32) {
    let head = rgba(PaletteKey::Stone, 1.0);
    let handle = rgba(PaletteKey::Wood, 1.0);
    fill_rect(data, size, 8, 8, 22, 18, head);
    fill_rect(data, size, 18, 14, 24, 32, handle);
}

/// Round fruit body + stem — tone picks the fruit.
fn draw_fruit(data: &mut [u8], size: u32, tone: PaletteKey) {
    let body = rgba(tone, 1.0);
    let stem = rgba(PaletteKey::Bark, 1.0);
    fill_rect(data, size, 14, 14, 28, 30, body);
    fill_rect(data, size, 12, 18, 30, 26, body);
    fill_rect(data, size, 20, 8, 23, 14, stem);
}

/// Cooked meal — plate + warm mound.
fn draw_meal(data: &mut [u8], size: u32) {
    let plate = rgba(PaletteKey::Stone, 1.0);
    let food = rgba(PaletteKey::Gold, 1.0);
    fill_rect(data, size, 8, 24, 34, 30, plate);
    fill_rect(data, size, 14, 14, 28, 24, food);
}

fn draw_generic(data: &mut [u8], size: u32, item_id: &str) {
    let c = rgba(PaletteKey::NeonCyan, 1.0);
    fill_rect(data, size, 12, 12, 28, 28, c);
    let ch = item_id.chars().next().unwrap_or('?').to_ascii_uppercase();
    // Tiny letter hint in center (2x3 blocks).
    let letter: &[(i32, i32)] = match ch {
        'W' => &[(0, 0), (2, 0), (0, 1), (1, 1), (2, 1), (0, 2), (2, 2)],
        'S' => &[(1, 0), (0, 1), (1, 1), (2, 2), (1, 2)],
        'T' => &[(0, 0), (1, 0), (2, 0), (1, 1), (1, 2)],
        _ => &[(1, 1)],
    };
    let ink = rgba(PaletteKey::Night, 1.0);
    for &(dx, dy) in letter {
        fill_rect(
            data,
            size,
            (16 + dx * 3) as u32,
            (16 + dy * 3) as u32,
            (19 + dx * 3) as u32,
            (19 + dy * 3) as u32,
            ink,
        );
    }
}
