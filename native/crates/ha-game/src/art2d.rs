//! 2D art library — drop-in PNG packs (Modern Exteriors/Interiors/UI, etc.)
//! over procedural fallbacks.
//!
//! Flow: put PNGs under `assets/art/<category>/<name>.png`, run
//! `npm run sync:art` (regenerates `assets/art/manifest.json`), rebuild.
//! The manifest is embedded at compile time (adding art requires a rebuild
//! anyway — the APK bundles `assets/`). Every consumer asks the library by
//! logical key ("decor/tree", "items/apple", "player/astronaut") and falls
//! back to the current procedural look when the key isn't shipped yet.
//!
//! Categories (see assets/art/README.md): tiles/ decor/ items/ machines/
//! player/ ship/ ui/ backgrounds/.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::hud::item_icons::ItemIconCache;
use ha_assets::{ArtAnim, ArtManifest, ArtValue};

const MANIFEST: &str = include_str!("../assets/art/manifest.json");

/// Una animación tira-de-frames dentro de una hoja: `frames` celdas de `cell` px
/// a partir de `origin` (px). `cell == ZERO` → derivar de la imagen completa.
#[derive(Clone, Copy)]
struct StripInfo {
    origin: UVec2,
    cell: UVec2,
    frames: u32,
    fps: f32,
}

/// Loaded drop-in sprites by logical key. Empty until the player ships art.
#[derive(Resource, Default)]
pub struct ArtLibrary {
    sprites: HashMap<String, Handle<Image>>,
    /// Animación de imagen completa (tira) — compat.
    anims: HashMap<String, ArtAnim>,
    /// Recorte estático px `[x, y, w, h]` (sheets con `rect`).
    rects: HashMap<String, [u32; 4]>,
    /// Animación de sub-región (sheets con `strip`).
    strips: HashMap<String, StripInfo>,
}

impl ArtLibrary {
    /// Sprite for a logical key ("decor/tree") if the pack ships one.
    pub fn get(&self, key: &str) -> Option<Handle<Image>> {
        self.sprites.get(key).cloned()
    }

    /// Animation for a key if its PNG is a frame-strip.
    pub fn anim(&self, key: &str) -> Option<ArtAnim> {
        self.anims.get(key).copied()
    }

    /// First key that exists from a fallback chain (most→least specific,
    /// e.g. `["decor/rock_moon", "decor/rock"]`).
    pub fn get_first(&self, keys: &[&str]) -> Option<Handle<Image>> {
        keys.iter().find_map(|k| self.get(k))
    }

    /// Sprite component for a key with a fixed world size, or the plain
    /// colored-quad fallback. Aplica el recorte `rect` si el slot es un sheet.
    pub fn sprite_or_color(&self, key: &str, color: Color, size: Vec2) -> Sprite {
        match self.get(key) {
            Some(image) => {
                let mut s = Sprite::from_image(image);
                s.custom_size = Some(size);
                if let Some([x, y, w, h]) = self.rects.get(key).copied() {
                    s.rect = Some(Rect::from_corners(
                        Vec2::new(x as f32, y as f32),
                        Vec2::new((x + w) as f32, (y + h) as f32),
                    ));
                }
                s
            }
            None => Sprite::from_color(color, size),
        }
    }

    /// Componente de animación para un sprite del slot, si trae animación (tira
    /// de sub-región `strip`, o imagen completa). Insertalo junto al sprite.
    pub fn animation(&self, key: &str) -> Option<ArtAnimated> {
        if let Some(s) = self.strips.get(key) {
            return Some(ArtAnimated::from_strip(*s));
        }
        self.anims.get(key).map(|a| ArtAnimated::new(*a))
    }
}

/// Sprite animado por tira horizontal de N frames a F fps. El atlas se arma solo
/// cuando la imagen termina de cargar (`setup_art_atlases`).
#[derive(Component)]
pub struct ArtAnimated {
    frames: u32,
    timer: Timer,
    origin: UVec2,
    cell: UVec2, // ZERO → derivar de la imagen completa
    ready: bool,
}

impl ArtAnimated {
    pub fn new(a: ArtAnim) -> Self {
        Self {
            frames: a.frames.max(1),
            timer: Timer::from_seconds(1.0 / a.fps.max(0.1), TimerMode::Repeating),
            origin: UVec2::ZERO,
            cell: UVec2::ZERO,
            ready: false,
        }
    }

    fn from_strip(s: StripInfo) -> Self {
        Self {
            frames: s.frames.max(1),
            timer: Timer::from_seconds(1.0 / s.fps.max(0.1), TimerMode::Repeating),
            origin: s.origin,
            cell: s.cell,
            ready: false,
        }
    }
}

pub struct Art2dPlugin;

impl Plugin for Art2dPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ArtLibrary>()
            .add_systems(PreStartup, load_art_manifest)
            .add_systems(Startup, preload_item_icons)
            .add_systems(Update, (setup_art_atlases, tick_art_animations).chain());
    }
}

/// Una vez que la imagen del sprite cargó, arma el `TextureAtlasLayout` (ancho de
/// frame = ancho/frames) y engancha el atlas. Corre hasta que `ready`.
fn setup_art_atlases(
    images: Res<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut q: Query<(&mut Sprite, &mut ArtAnimated)>,
) {
    for (mut sprite, mut anim) in &mut q {
        if anim.ready {
            continue;
        }
        let Some(img) = images.get(&sprite.image) else {
            continue; // aún cargando
        };
        let size = img.size();
        let frames = anim.frames.max(1);
        // Imagen completa → derivar celda; sub-región → usar origin/cell.
        let (cell_w, cell_h, origin) = if anim.cell == UVec2::ZERO {
            ((size.x / frames).max(1), size.y, UVec2::ZERO)
        } else {
            (anim.cell.x.max(1), anim.cell.y.max(1), anim.origin)
        };
        let mut layout = TextureAtlasLayout::new_empty(size);
        for i in 0..frames {
            let x = origin.x + i * cell_w;
            layout.add_texture(URect::from_corners(
                UVec2::new(x, origin.y),
                UVec2::new(x + cell_w, origin.y + cell_h),
            ));
        }
        sprite.texture_atlas = Some(TextureAtlas { layout: layouts.add(layout), index: 0 });
        anim.ready = true;
    }
}

/// Avanza el frame del atlas a `fps` (wrap).
fn tick_art_animations(time: Res<Time>, mut q: Query<(&mut Sprite, &mut ArtAnimated)>) {
    for (mut sprite, mut anim) in &mut q {
        if !anim.ready {
            continue;
        }
        anim.timer.tick(time.delta());
        if anim.timer.just_finished() {
            let frames = anim.frames.max(1) as usize;
            if let Some(atlas) = sprite.texture_atlas.as_mut() {
                atlas.index = (atlas.index + 1) % frames;
            }
        }
    }
}

fn load_art_manifest(asset_server: Res<AssetServer>, mut art: ResMut<ArtLibrary>) {
    // Formato del editor (ha-asset-tools): compatible con el viejo `{"key":"png"}`
    // y con el nuevo `{"key":{"png":"...","anim":{...}}}`.
    let manifest = match ArtManifest::from_json(MANIFEST) {
        Ok(m) => m,
        Err(err) => {
            eprintln!("ha-art: manifest.json unreadable ({err}) — procedural art only");
            return;
        }
    };
    for (key, value) in manifest.sprites {
        art.sprites.insert(key.clone(), asset_server.load(value.png().to_string()));
        // Recortes de sheet (rect / strip) → celdas a px con la grilla del entry.
        if let ArtValue::Full(e) = &value {
            let (gw, gh) = e.grid.unwrap_or((32, 32));
            if let Some(r) = e.rect {
                art.rects.insert(key.clone(), [r.col * gw, r.row * gh, r.w * gw, r.h * gh]);
            }
            if let Some(strip) = e.strip {
                let fps = e.anim.map(|a| a.fps).unwrap_or(6.0);
                art.strips.insert(
                    key.clone(),
                    StripInfo {
                        origin: UVec2::new(strip.col * gw, strip.row * gh),
                        cell: UVec2::new(gw, gh),
                        frames: strip.cells().max(1),
                        fps,
                    },
                );
            }
        }
        // Animación de imagen completa sólo si no hay strip explícito.
        if !art.strips.contains_key(&key) {
            if let Some(anim) = value.anim() {
                art.anims.insert(key, anim);
            }
        }
    }
    if !art.sprites.is_empty() {
        eprintln!(
            "ha-art: {} sprites ({} anim, {} recortes, {} strips)",
            art.sprites.len(),
            art.anims.len(),
            art.rects.len(),
            art.strips.len()
        );
    }
}

/// Item/UI icons: any `items/<item_id>.png` in the pack replaces the
/// procedural icon EVERYWHERE (hotbar, bag, craft list, machine sprites) by
/// pre-seeding the icon cache before the HUD first asks for it.
fn preload_item_icons(art: Res<ArtLibrary>, mut icons: ResMut<ItemIconCache>) {
    for (key, handle) in &art.sprites {
        if let Some(item_id) = key.strip_prefix("items/") {
            icons.preload(item_id, handle.clone());
        }
    }
}
