//! Rasterize catalog UI icons into cached `Image` textures (CPU, once per kind).

use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use ha_shared::{palette_rgb, UiIconDef, UiIconVariant};

pub fn rasterize_icon(def: &UiIconDef) -> Image {
    let size = (def.grid as u32 * def.px) as usize;
    let mut data = vec![0u8; size * size * 4];
    let bg = palette_to_rgba(def.bg_palette, def.variant == UiIconVariant::Empty);
    for y in 0..def.grid {
        for x in 0..def.grid {
            blit_cell(&mut data, size as u32, x, y, def.px, bg);
        }
    }
    if def.variant != UiIconVariant::Empty {
        let fill = palette_to_rgba(def.fill_palette, false);
        for cell in def.cells {
            if def.variant == UiIconVariant::Half && cell.skip_in_half {
                continue;
            }
            blit_cell(&mut data, size as u32, cell.x, cell.y, def.px, fill);
        }
    }
    Image::new(
        Extent3d {
            width: size as u32,
            height: size as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

fn palette_to_rgba(key: ha_shared::PaletteKey, transparent_fill: bool) -> [u8; 4] {
    let rgb = palette_rgb(key);
    let a = if transparent_fill { 0.0 } else { 1.0 };
    posterize_rgba([
        (rgb[0] * 255.0) as u8,
        (rgb[1] * 255.0) as u8,
        (rgb[2] * 255.0) as u8,
        (a * 255.0) as u8,
    ])
}

/// Crush channels to ~5-bit PS1 feel (8 levels per channel).
fn posterize_rgba(rgba: [u8; 4]) -> [u8; 4] {
    let q = |c: u8| (c >> 3) << 3;
    [q(rgba[0]), q(rgba[1]), q(rgba[2]), rgba[3]]
}

fn blit_cell(data: &mut [u8], tex: u32, cx: i32, cy: i32, px: u32, rgba: [u8; 4]) {
    for py in 0..px {
        for px_i in 0..px {
            let x = (cx as u32 * px + px_i) as usize;
            let y = (cy as u32 * px + py) as usize;
            let i = (y * tex as usize + x) * 4;
            if i + 3 < data.len() {
                data[i..i + 4].copy_from_slice(&rgba);
            }
        }
    }
}
