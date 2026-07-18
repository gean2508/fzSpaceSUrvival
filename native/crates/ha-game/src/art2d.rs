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
use ha_assets::{ArtAnim, ArtManifest};

const MANIFEST: &str = include_str!("../assets/art/manifest.json");

/// Loaded drop-in sprites by logical key. Empty until the player ships art.
#[derive(Resource, Default)]
pub struct ArtLibrary {
    sprites: HashMap<String, Handle<Image>>,
    /// Animation metadata for keys whose PNG is a frame-strip (agua, personajes).
    anims: HashMap<String, ArtAnim>,
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
    /// colored-quad fallback.
    pub fn sprite_or_color(&self, key: &str, color: Color, size: Vec2) -> Sprite {
        match self.get(key) {
            Some(image) => {
                let mut s = Sprite::from_image(image);
                s.custom_size = Some(size);
                s
            }
            None => Sprite::from_color(color, size),
        }
    }

    /// Componente de animación para un sprite hecho de un PNG tira-de-frames,
    /// si `key` trae animación. Insertalo junto al sprite; los sistemas de abajo
    /// arman el atlas y ciclan los frames.
    pub fn animation(&self, key: &str) -> Option<ArtAnimated> {
        self.anims.get(key).map(|a| ArtAnimated::new(*a))
    }
}

/// Sprite animado por tira horizontal de N frames a F fps. El atlas se arma solo
/// cuando la imagen termina de cargar (`setup_art_atlases`).
#[derive(Component)]
pub struct ArtAnimated {
    frames: u32,
    timer: Timer,
    ready: bool,
}

impl ArtAnimated {
    pub fn new(a: ArtAnim) -> Self {
        let fps = a.fps.max(0.1);
        Self {
            frames: a.frames.max(1),
            timer: Timer::from_seconds(1.0 / fps, TimerMode::Repeating),
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
        let frame_w = (size.x / frames).max(1);
        let layout = TextureAtlasLayout::from_grid(UVec2::new(frame_w, size.y), frames, 1, None, None);
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
        if let Some(anim) = value.anim() {
            art.anims.insert(key, anim);
        }
    }
    if !art.sprites.is_empty() {
        eprintln!(
            "ha-art: {} drop-in sprites loaded ({} animados)",
            art.sprites.len(),
            art.anims.len()
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
