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
use ha_assets::{ArtAnim, ArtManifest, ArtValue, CellRect, StripDir};

const MANIFEST: &str = include_str!("../assets/art/manifest.json");

/// Una animación tira-de-frames dentro de una hoja: `frames` frames de tamaño
/// `cell` px, empezando en `origin` (px) y avanzando `step` px por frame.
/// `cell == ZERO` → derivar de la imagen completa (tira horizontal simple).
/// `step == ZERO` con `cell != ZERO` → avanzar horizontal por el ancho del frame.
#[derive(Clone, Copy)]
struct StripInfo {
    origin: UVec2,
    cell: UVec2,
    step: UVec2,
    frames: u32,
    fps: f32,
}

/// Resuelve un frame en celdas (`first_frame`) + cantidad + dirección a un
/// `StripInfo` en píxeles: cada frame ocupa `w×h` celdas y los siguientes se
/// pegan a la derecha o hacia abajo según `dir`.
fn strip_info(first_frame: CellRect, grid: (u32, u32), frames: u32, fps: f32, dir: StripDir) -> StripInfo {
    let (gw, gh) = grid;
    let (fw, fh) = (first_frame.w.max(1), first_frame.h.max(1));
    let cell = UVec2::new(fw * gw, fh * gh);
    let step = match dir {
        StripDir::Right => UVec2::new(cell.x, 0),
        StripDir::Down => UVec2::new(0, cell.y),
    };
    StripInfo {
        origin: UVec2::new(first_frame.col * gw, first_frame.row * gh),
        cell,
        step,
        frames: frames.max(1),
        fps,
    }
}

/// Un frame/rango de la hoja para una pose nombrada (idle_down, walk_left…):
/// rect px base + animación de sub-región si la pose es una tira.
#[derive(Clone)]
struct PoseInfo {
    rect_px: [u32; 4],
    anim: Option<StripInfo>,
    /// Capa de la pose (edificios: Floor/Wall/Object/Roof). None = Object.
    layer: Option<ha_assets::ArtLayer>,
    /// Hoja PROPIA de la pose (assets armados con varios PNG: uno por dirección
    /// o estado). `None` = la hoja del slot. Ver `ArtLibrary::pose_image`.
    image: Option<Handle<Image>>,
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
    /// Poses nombradas por slot (aplica a CUALQUIER atlas, no sólo el jugador):
    /// clave del slot → nombre de pose ("idle_down", "walk_left"…) → frame.
    poses: HashMap<String, HashMap<String, PoseInfo>>,
    /// Override de colisión de caminado por slot (editor: casilla "bloquea el
    /// paso"). `None` en el mapa = sin override → el juego usa su default por
    /// tipo. Ver `collides_override`.
    collides: HashMap<String, bool>,
    /// Huella de colisión multi-celda por slot (A1). Vacío = ningún asset la
    /// declara → el juego salta el escaneo de vecindad (cero costo). Ver
    /// `collision_footprint` / `has_footprints`.
    footprints: HashMap<String, ha_assets::CollisionFootprint>,
    /// Luz que emite cada slot (editor: casilla "emite luz" / pincel de LUZ del
    /// Image Lab). Vacío = ningún asset ilumina → `light2d` no cuelga nada. Ver
    /// `emission` / `emission_first`.
    emissions: HashMap<String, ha_assets::Emission>,
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

    /// Override de colisión que el usuario marcó en el editor para este slot:
    /// `Some(true/false)` si lo fijó, `None` si no → el que llama usa su default
    /// por tipo. Prueba las claves en orden (específica → genérica).
    pub fn collides_override(&self, keys: &[&str]) -> Option<bool> {
        keys.iter().find_map(|k| self.collides.get(*k).copied())
    }

    /// Huella de colisión multi-celda del primer slot de la cadena que la tenga
    /// (A1). `None` = ninguno la declara → colisión de 1 celda por defecto.
    pub fn collision_footprint(&self, keys: &[&str]) -> Option<&ha_assets::CollisionFootprint> {
        keys.iter().find_map(|k| self.footprints.get(*k))
    }

    /// ¿Algún slot declara huella multi-celda? Si no, el juego evita el escaneo
    /// de vecindad en la colisión de decor (cero costo hasta que se pinte una).
    pub fn has_footprints(&self) -> bool {
        !self.footprints.is_empty()
    }

    /// Luz declarada por el slot, si el usuario la activó en el editor.
    pub fn emission(&self, key: &str) -> Option<&ha_assets::Emission> {
        self.emissions.get(key)
    }

    /// Luz del primer slot de la cadena de fallback que la declare (misma cadena
    /// que usás para el sprite). `None` = ese asset no ilumina.
    pub fn emission_first(&self, keys: &[&str]) -> Option<&ha_assets::Emission> {
        if self.emissions.is_empty() {
            return None; // atajo: ningún asset del pack ilumina
        }
        keys.iter().find_map(|k| self.emissions.get(*k))
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

    /// `true` si el slot trae poses guardadas (personaje CON direcciones, o
    /// cualquier otro atlas con estados). Genérico — no sólo el jugador.
    pub fn has_poses(&self, key: &str) -> bool {
        self.poses.get(key).map(|m| !m.is_empty()).unwrap_or(false)
    }

    pub fn has_pose(&self, key: &str, name: &str) -> bool {
        self.poses.get(key).map(|m| m.contains_key(name)).unwrap_or(false)
    }

    /// Rect px de la pose + animación lista para insertar si es una tira.
    /// `None` si el slot/pose no existe.
    pub fn pose(&self, key: &str, name: &str) -> Option<([u32; 4], Option<ArtAnimated>)> {
        let p = self.poses.get(key)?.get(name)?;
        Some((p.rect_px, p.anim.map(ArtAnimated::from_strip)))
    }

    /// Imagen que le corresponde a una pose: la hoja PROPIA de la pose si el
    /// asset se armó con varios PNG (uno por dirección/estado), si no la del
    /// slot. Quien cambia de pose debe asignar esto a `sprite.image` — si no, se
    /// vería el recorte de la pose nueva sobre la hoja vieja.
    pub fn pose_image(&self, key: &str, name: &str) -> Option<Handle<Image>> {
        match self.poses.get(key).and_then(|m| m.get(name)) {
            Some(PoseInfo { image: Some(img), .. }) => Some(img.clone()),
            _ => self.get(key),
        }
    }

    /// Capa etiquetada de una pose (editor F2). `None` = sin etiqueta (Object).
    pub fn pose_layer(&self, key: &str, name: &str) -> Option<ha_assets::ArtLayer> {
        self.poses.get(key)?.get(name)?.layer
    }

    /// Primera pose del slot etiquetada con `layer` (p.ej. el techo de un
    /// edificio, sin importar cómo la nombró el usuario): su nombre y su rect px.
    /// El nombre hace falta para pedirle su hoja a `pose_image` cuando el asset
    /// está repartido en varios PNG.
    pub fn pose_with_layer_named(
        &self,
        key: &str,
        layer: ha_assets::ArtLayer,
    ) -> Option<(&str, [u32; 4])> {
        let m = self.poses.get(key)?;
        // Nombre canónico primero ("roof"/"base"), después cualquier etiquetada.
        let canonical = match layer {
            ha_assets::ArtLayer::Roof => "roof",
            ha_assets::ArtLayer::Floor => "floor",
            ha_assets::ArtLayer::Basement => "basement",
            ha_assets::ArtLayer::Wall => "base",
            ha_assets::ArtLayer::Object => "object",
            ha_assets::ArtLayer::Light => "light",
        };
        if let Some(p) = m.get(canonical) {
            if p.layer.map(|l| l == layer).unwrap_or(true) {
                return Some((canonical, p.rect_px));
            }
        }
        m.iter()
            .find(|(_, p)| p.layer == Some(layer))
            .map(|(n, p)| (n.as_str(), p.rect_px))
    }

    /// Sprite recortado de una pose concreta con tamaño en mundo, si existe.
    pub fn pose_sprite(&self, key: &str, name: &str, size: Vec2) -> Option<Sprite> {
        let (rect_px, _) = self.pose(key, name)?;
        let image = self.pose_image(key, name)?;
        let mut s = Sprite::from_image(image);
        s.custom_size = Some(size);
        let [x, y, w, h] = rect_px;
        s.rect = Some(Rect::from_corners(
            Vec2::new(x as f32, y as f32),
            Vec2::new((x + w) as f32, (y + h) as f32),
        ));
        Some(s)
    }

    /// Sprite "tal como se ve este asset" para plantarlo en el mundo: aplica su
    /// recorte de atlas, o su pose representativa (`idle_down`/`center`/la 1ª, con
    /// la hoja que le corresponda), y devuelve la animación si la tiene. Sin esto,
    /// un slot recortado de un atlas se dibujaría con el ATLAS ENTERO. `None` =
    /// el slot no tiene arte.
    pub fn world_sprite(&self, key: &str, size: Vec2) -> Option<(Sprite, Option<ArtAnimated>)> {
        // Pose representativa primero (personajes/NPC colocados a mano).
        if let Some(name) = ["idle_down", "center", "base"]
            .into_iter()
            .find(|n| self.has_pose(key, n))
            .or_else(|| self.poses.get(key).and_then(|m| m.keys().next().map(|s| s.as_str())))
        {
            if let Some((rect_px, anim)) = self.pose(key, name) {
                let image = self.pose_image(key, name)?;
                let mut s = Sprite::from_image(image);
                s.custom_size = Some(size);
                if anim.is_none() {
                    let [x, y, w, h] = rect_px;
                    s.rect = Some(Rect::from_corners(
                        Vec2::new(x as f32, y as f32),
                        Vec2::new((x + w) as f32, (y + h) as f32),
                    ));
                }
                return Some((s, anim));
            }
        }
        let image = self.get(key)?;
        let mut s = Sprite::from_image(image);
        s.custom_size = Some(size);
        let anim = self.animation(key);
        // El recorte estático sólo aplica si no hay tira (la tira maneja su rect).
        if anim.is_none() {
            if let Some([x, y, w, h]) = self.rects.get(key).copied() {
                s.rect = Some(Rect::from_corners(
                    Vec2::new(x as f32, y as f32),
                    Vec2::new((x + w) as f32, (y + h) as f32),
                ));
            }
        }
        Some((s, anim))
    }

    /// `ImageNode` de UI para un slot `ui/*` (respeta el recorte de sheet si el
    /// usuario lo recortó de un atlas en el editor). `None` = sin arte → el HUD
    /// mantiene su dibujo procedural (cero regresión).
    pub fn ui_image_node(&self, key: &str) -> Option<bevy::ui::widget::ImageNode> {
        let image = self.get(key)?;
        let mut node = bevy::ui::widget::ImageNode::new(image);
        if let Some([x, y, w, h]) = self.rects.get(key).copied() {
            node.rect = Some(Rect::from_corners(
                Vec2::new(x as f32, y as f32),
                Vec2::new((x + w) as f32, (y + h) as f32),
            ));
        }
        Some(node)
    }

    /// `true` si el material de tile trae arte "vivo" (autotile por poses o una
    /// animación): sólo entonces el mundo lo dibuja como sprite en vez de dejar
    /// el quad vertex-color barato. Un PNG estático plano NO califica (evita
    /// spawnear miles de sprites por el suelo base). Ver `world2d::spawn_chunk_tile_art`.
    pub fn tile_has_art(&self, key: &str) -> bool {
        self.has_poses(key) || self.anims.contains_key(key) || self.strips.contains_key(key)
    }

    /// Sprite (+ animación) para un tile de material `key` con la forma autotile
    /// `shape` (center/edge_n/corner_ne/…). Cae a la pose "center", y si no hay
    /// poses, al sprite plano del slot. `None` = sin arte (usar vertex-color).
    pub fn tile_sprite(&self, key: &str, shape: &str, size: Vec2) -> Option<(Sprite, Option<ArtAnimated>)> {
        // La forma pedida puede venir de su propia hoja (autotile repartido en
        // varios PNG) → la imagen se elige DESPUÉS de saber qué pose se usa.
        let used = [shape, "center"].into_iter().find(|n| self.has_pose(key, n));
        let image = match used {
            Some(name) => self.pose_image(key, name)?,
            None => self.get(key)?,
        };
        let mut s = Sprite::from_image(image);
        s.custom_size = Some(size);
        // Preferí la forma pedida; si falta, probá "center"; si no hay poses, plano.
        if let Some((rect_px, anim)) = self.pose(key, shape).or_else(|| self.pose(key, "center")) {
            if anim.is_none() {
                let [x, y, w, h] = rect_px;
                s.rect = Some(Rect::from_corners(
                    Vec2::new(x as f32, y as f32),
                    Vec2::new((x + w) as f32, (y + h) as f32),
                ));
            }
            return Some((s, anim));
        }
        // Sin poses: recorte estático del slot (si tiene) + animación de tira.
        if let Some([x, y, w, h]) = self.rects.get(key).copied() {
            s.rect = Some(Rect::from_corners(
                Vec2::new(x as f32, y as f32),
                Vec2::new((x + w) as f32, (y + h) as f32),
            ));
        }
        Some((s, self.animation(key)))
    }
}

/// Cómo se reproduce una tira de frames.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayMode {
    /// Cicla para siempre (agua, fuego, caminar): el default de todo el arte.
    #[default]
    Loop,
    /// Corre UNA vez y se queda clavada en el último frame. Es lo que hace que
    /// un cofre/puerta "termine de abrirse" y se mantenga abierto mientras lo
    /// usás: el estado lo decide el juego, la animación sólo es la transición.
    OnceHold,
}

/// Sprite animado por tira horizontal de N frames a F fps. El atlas se arma solo
/// cuando la imagen termina de cargar (`setup_art_atlases`).
#[derive(Component)]
pub struct ArtAnimated {
    frames: u32,
    timer: Timer,
    origin: UVec2,
    cell: UVec2, // ZERO → derivar de la imagen completa
    step: UVec2, // desplazamiento px por frame (ZERO → derivar horizontal)
    ready: bool,
    mode: PlayMode,
    /// Sólo con `OnceHold`: ya llegó al último frame y ahí se queda.
    done: bool,
}

impl ArtAnimated {
    pub fn new(a: ArtAnim) -> Self {
        Self {
            frames: a.frames.max(1),
            timer: Timer::from_seconds(1.0 / a.fps.max(0.1), TimerMode::Repeating),
            origin: UVec2::ZERO,
            cell: UVec2::ZERO,
            step: UVec2::ZERO,
            ready: false,
            mode: PlayMode::Loop,
            done: false,
        }
    }

    fn from_strip(s: StripInfo) -> Self {
        Self {
            frames: s.frames.max(1),
            timer: Timer::from_seconds(1.0 / s.fps.max(0.1), TimerMode::Repeating),
            origin: s.origin,
            cell: s.cell,
            step: s.step,
            ready: false,
            mode: PlayMode::Loop,
            done: false,
        }
    }

    /// Reproducir una sola vez y quedarse en el último frame (abrir/cerrar).
    pub fn once(mut self) -> Self {
        self.mode = PlayMode::OnceHold;
        self
    }

    /// `true` cuando una `OnceHold` ya terminó su recorrido.
    pub fn done(&self) -> bool {
        self.done
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
        // Imagen completa → derivar celda y paso horizontal; sub-región → usar
        // origin/cell/step del strip (frame multi-celda + dirección).
        let (cell_w, cell_h, origin, step) = if anim.cell == UVec2::ZERO {
            let cw = (size.x / frames).max(1);
            (cw, size.y, UVec2::ZERO, UVec2::new(cw, 0))
        } else {
            let cell = UVec2::new(anim.cell.x.max(1), anim.cell.y.max(1));
            // step ZERO (compat) → avanzar horizontal por el ancho del frame.
            let step = if anim.step == UVec2::ZERO { UVec2::new(cell.x, 0) } else { anim.step };
            (cell.x, cell.y, anim.origin, step)
        };
        let mut layout = TextureAtlasLayout::new_empty(size);
        for i in 0..frames {
            let x = origin.x + i * step.x;
            let y = origin.y + i * step.y;
            layout.add_texture(URect::from_corners(
                UVec2::new(x, y),
                UVec2::new(x + cell_w, y + cell_h),
            ));
        }
        sprite.texture_atlas = Some(TextureAtlas { layout: layouts.add(layout), index: 0 });
        anim.ready = true;
    }
}

/// Avanza el frame del atlas a `fps` (wrap en `Loop`; en `OnceHold` frena y se
/// queda en el último frame — de ahí sale el cofre que "queda abierto").
fn tick_art_animations(time: Res<Time>, mut q: Query<(&mut Sprite, &mut ArtAnimated)>) {
    for (mut sprite, mut anim) in &mut q {
        if !anim.ready || anim.done {
            continue;
        }
        anim.timer.tick(time.delta());
        if anim.timer.just_finished() {
            let frames = anim.frames.max(1) as usize;
            if let Some(atlas) = sprite.texture_atlas.as_mut() {
                match anim.mode {
                    PlayMode::Loop => atlas.index = (atlas.index + 1) % frames,
                    PlayMode::OnceHold => {
                        if atlas.index + 1 >= frames {
                            anim.done = true;
                        } else {
                            atlas.index += 1;
                        }
                    }
                }
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
            if let Some(c) = e.collides {
                art.collides.insert(key.clone(), c);
            }
            if let Some(fp) = &e.footprint {
                if !fp.is_empty() {
                    art.footprints.insert(key.clone(), fp.clone());
                }
            }
            if let Some(em) = e.emission.filter(|em| !em.is_off()) {
                art.emissions.insert(key.clone(), em);
            }
            if let Some(r) = e.rect {
                art.rects.insert(key.clone(), [r.col * gw, r.row * gh, r.w * gw, r.h * gh]);
            }
            if let Some(strip) = e.strip {
                // `strip` es el rect (celdas) del PRIMER frame → su w×h es el
                // tamaño de cada frame. La cantidad de frames viene de `anim`
                // (default: 1 si falta). Los frames avanzan en `strip_dir`.
                let fps = e.anim.map(|a| a.fps).unwrap_or(6.0);
                let frames = e.anim.map(|a| a.frames).unwrap_or(1).max(1);
                let dir = e.strip_dir.unwrap_or_default();
                art.strips.insert(
                    key.clone(),
                    strip_info(strip, (gw, gh), frames, fps, dir),
                );
            }
            if let Some(poses) = &e.poses {
                // Autotile ANIMADO (p.ej. agua): cada frame de la animación es un
                // BLOQUE entero (`strip` = 3×3), y cada pose es una SUBCELDA de ese
                // bloque. Por eso los frames de una pose están separados por el
                // ancho/alto del BLOQUE, no por el de su subcelda — sin esto la
                // animación pisa las celdas vecinas (bordes/esquinas) y el agua se
                // ve como rayas. El paso viene del `strip` del entry (default 1×1).
                let (block_w, block_h) = e
                    .strip
                    .map(|s| (s.w.max(1), s.h.max(1)))
                    .unwrap_or((1, 1));
                let mut m = HashMap::new();
                for (name, pose) in poses {
                    // Hoja propia de la pose (asset armado con varios PNG: uno
                    // por dirección/estado) → se carga aparte y con SU grilla.
                    let own = pose.png.as_ref().map(|p| asset_server.load(p.to_string()));
                    let (pw, ph) = pose.grid_or((gw, gh));
                    let rect_px = [pose.rect.col * pw, pose.rect.row * ph, pose.rect.w * pw, pose.rect.h * ph];
                    let dir = pose.dir.unwrap_or_default();
                    let anim = pose.anim.map(|a| {
                        let mut si = strip_info(pose.rect, (pw, ph), a.frames.max(1), a.fps, dir);
                        // El paso de BLOQUE (autotile animado) sólo aplica cuando la
                        // pose comparte la hoja del slot; con hoja propia los frames
                        // van pegados como en cualquier tira.
                        if pose.png.is_none() {
                            si.step = match dir {
                                StripDir::Right => UVec2::new(block_w * pw, 0),
                                StripDir::Down => UVec2::new(0, block_h * ph),
                            };
                        }
                        si
                    });
                    m.insert(name.clone(), PoseInfo { rect_px, anim, layer: pose.layer, image: own });
                }
                if !m.is_empty() {
                    art.poses.insert(key.clone(), m);
                }
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
            "ha-art: {} sprites ({} anim, {} recortes, {} strips, {} con luz)",
            art.sprites.len(),
            art.anims.len(),
            art.rects.len(),
            art.strips.len(),
            art.emissions.len()
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
