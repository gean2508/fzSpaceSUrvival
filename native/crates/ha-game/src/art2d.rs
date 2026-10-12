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
    /// Tamaño del slot en TILES según su recorte (casa 5×4 → `(5,4)`). Es la
    /// grilla sobre la que se pintó la huella y la base de la colocación
    /// alineada al grid. Ver `slot_cells` / `anchor`.
    cells: HashMap<String, (u32, u32)>,
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
    /// Lado de la huella más grande declarada (celdas). Lo consulta la colisión en
    /// cada paso para acotar el escaneo de vecindad — por eso está cacheado.
    footprint_reach: i64,
    /// Luz que emite cada slot (editor: casilla "emite luz" / pincel de LUZ del
    /// Image Lab). Vacío = ningún asset ilumina → `light2d` no cuelga nada. Ver
    /// `emission` / `emission_first`.
    emissions: HashMap<String, ha_assets::Emission>,
    /// **Capa de dibujo por slot** (editor de assets: combo "capa"). Decide en
    /// qué banda z se planta el asset — `Floor` bajo los pies, `Roof` arriba con
    /// fade. Ausente = `Object` (Y-sort de siempre). Ver `crate::layers`.
    layers: HashMap<String, ha_assets::ArtLayer>,
    /// **Variantes** declaradas por objeto (`"floor/piso"` → madera, piedra…), en
    /// el orden del editor. Incluye las que todavía no tienen arte (para poder
    /// ofrecerlas en un menú y que caigan al genérico). Ver `variants`.
    objects: HashMap<String, Vec<ha_assets::VariantDef>>,
    /// Claves de sprite de las variantes que SÍ tienen arte, por objeto — lo que
    /// se puede dibujar de verdad. Ver `variant_key_seeded`.
    variant_art: HashMap<String, Vec<String>>,
    /// Claves cuyo dibujo lo generó `procedural_art` porque el slot estaba vacío.
    /// Se comportan como arte normal (se dibujan, autotilean, colisionan igual)
    /// pero se distinguen para dos cosas: no pisar nunca arte real, y que
    /// `tile_has_art` diga que sí — si no, el mundo saltaría el material entero
    /// y el respaldo no se vería jamás.
    procedural: std::collections::HashSet<String>,
}

impl ArtLibrary {
    /// **Todos** los PNG que el manifest pidió cargar: los del slot y los de las
    /// poses con hoja propia. La pantalla de carga los recorre para no levantar
    /// la cortina con arte a medio llegar — si entra el gameplay antes, el
    /// sprite se dibuja sin textura y aparece de golpe cuando el PNG termina de
    /// decodificarse. Se llama unas pocas veces por partida, no por frame.
    pub fn image_handles(&self) -> Vec<Handle<Image>> {
        let mut out: Vec<Handle<Image>> = self.sprites.values().cloned().collect();
        out.extend(
            self.poses
                .values()
                .flat_map(|m| m.values())
                .filter_map(|p| p.image.clone()),
        );
        out
    }

    /// Sprite for a logical key ("decor/tree") if the pack ships one.
    pub fn get(&self, key: &str) -> Option<Handle<Image>> {
        self.sprites.get(key).cloned()
    }

    /// ¿Este slot tiene arte ASIGNADA (real o de respaldo)?
    pub fn has_art(&self, key: &str) -> bool {
        self.sprites.contains_key(key)
    }

    /// Registra un dibujo generado por código para un slot vacío. No hace nada si
    /// el slot ya tiene arte: **lo curado siempre gana**.
    pub fn set_procedural(&mut self, key: &str, image: Handle<Image>) {
        if self.sprites.contains_key(key) {
            return;
        }
        self.sprites.insert(key.to_string(), image);
        self.procedural.insert(key.to_string());
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

    // ── variantes (un objeto, varios materiales/skins) ──────────────────────

    /// Variantes declaradas del objeto, en el orden del editor. Es la lista que un
    /// menú (materiales de construcción, skins) debe ofrecer. Vacío = objeto simple.
    pub fn variants(&self, base: &str) -> &[ha_assets::VariantDef] {
        self.objects.get(base).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Clave de sprite para `(objeto, variante)`: la de la variante si tiene arte,
    /// si no la del objeto genérico. Con la clave que devuelve pedís sprite, poses,
    /// colisión, huella y luz por los caminos de siempre — nada más cambia.
    pub fn resolved_key(&self, base: &str, variant: Option<&str>) -> String {
        match variant {
            Some(v) => {
                let k = ha_assets::variant_key(base, v);
                if self.sprites.contains_key(&k) {
                    k
                } else {
                    base.to_string()
                }
            }
            None => base.to_string(),
        }
    }

    /// Variante elegida **determinísticamente** por una semilla (la celda del
    /// mundo): es lo que hace que un objeto con 3 variantes de arte se vea variado
    /// en el mundo sin guardar nada por instancia — la misma celda elige siempre la
    /// misma. Sin variantes con arte devuelve la clave base tal cual.
    pub fn variant_key_seeded(&self, base: &str, seed: u64) -> String {
        match self.variant_art.get(base) {
            Some(keys) if !keys.is_empty() => {
                // Mezcla barata (splitmix64) para que celdas vecinas no caigan
                // siempre en la misma variante.
                let mut h = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
                h ^= h >> 29;
                h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
                h ^= h >> 32;
                keys[(h % keys.len() as u64) as usize].clone()
            }
            _ => base.to_string(),
        }
    }

    /// Como `get_first`, pero por cada clave de la cadena prueba primero una de sus
    /// variantes (elegida por `seed`). Así el decor procedural (rocas, árboles) usa
    /// las variantes que cargaste en el editor sin tocar la generación del mundo.
    pub fn get_first_seeded(&self, keys: &[&str], seed: u64) -> Option<Handle<Image>> {
        keys.iter().find_map(|k| {
            let vk = self.variant_key_seeded(k, seed);
            self.get(&vk).or_else(|| self.get(k))
        })
    }

    /// Sprite de la primera clave con arte, **con su recorte aplicado**.
    ///
    /// Es el hermano correcto de `get_first_seeded`: ése devuelve sólo el
    /// `Handle<Image>`, y quien lo usaba armaba el sprite a mano con
    /// `Sprite::from_image` — o sea, SIN el `rect`. Mientras cada slot tenía su PNG
    /// propio eso funcionaba de casualidad (la imagen ERA el sprite). Al empaquetar
    /// la decoración en una hoja (`scripts/pack-world-atlas.py`) dejó de serlo y el
    /// árbol pasó a dibujar el atlas entero encogido: fragmentos de basura.
    pub fn sprite_first_seeded(&self, keys: &[&str], seed: u64, size: Vec2) -> Option<Sprite> {
        keys.iter().find_map(|k| {
            let vk = self.variant_key_seeded(k, seed);
            for cand in [vk.as_str(), k] {
                if let Some(image) = self.get(cand) {
                    let mut s = Sprite::from_image(image);
                    s.custom_size = Some(size);
                    if let Some([x, y, w, h]) = self.rects.get(cand).copied() {
                        s.rect = Some(Rect::from_corners(
                            Vec2::new(x as f32, y as f32),
                            Vec2::new((x + w) as f32, (y + h) as f32),
                        ));
                    }
                    return Some(s);
                }
            }
            None
        })
    }

    /// `world_sprite` de una variante concreta (o del genérico si esa variante no
    /// tiene arte).
    pub fn world_sprite_variant(
        &self,
        base: &str,
        variant: Option<&str>,
        size: Vec2,
    ) -> Option<(Sprite, Option<ArtAnimated>)> {
        self.world_sprite(&self.resolved_key(base, variant), size)
    }

    /// `world_sprite` con la variante elegida por la celda (variedad ambiental).
    pub fn world_sprite_seeded(
        &self,
        base: &str,
        seed: u64,
        size: Vec2,
    ) -> Option<(Sprite, Option<ArtAnimated>)> {
        self.world_sprite(&self.variant_key_seeded(base, seed), size)
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

    /// Cuántas celdas alrededor hay que mirar para no perderse una huella: la más
    /// grande declarada. 0 = ninguna huella → la colisión sólo mira la celda de al
    /// lado (lo barato, que es lo que corre en el Redmi).
    pub fn footprint_reach(&self) -> i64 {
        self.footprint_reach
    }

    /// Tamaño del slot en TILES según su recorte (una casa de 5×4 celdas mide
    /// 5×4). `None` = PNG suelto sin recorte: el que llama lo deduce del tamaño
    /// con el que lo dibuja.
    pub fn slot_cells(&self, key: &str) -> Option<(u32, u32)> {
        self.cells.get(key).copied()
    }

    /// Celda ANCLA del slot dentro de su grilla: la que queda sobre la celda que
    /// apuntaste al colocarlo. La de su huella si el usuario la pintó; si no, la
    /// columna del medio y la fila del pie.
    pub fn anchor(&self, key: &str, cells: (u32, u32)) -> (u32, u32) {
        match self.footprints.get(key) {
            Some(f) => (f.ax.min(cells.0.saturating_sub(1)), f.az.min(cells.1.saturating_sub(1))),
            None => ha_assets::CollisionFootprint::default_anchor(cells.0, cells.1),
        }
    }

    /// **Capa de dibujo del slot** (editor de assets). `None` = sin etiquetar →
    /// quien la use debe tratarlo como `Object` (ver `crate::layers::depth`).
    /// La variante hereda la capa del objeto base: `floor/piso#madera` y
    /// `floor/piso#piedra` son el mismo mueble, no dos capas distintas.
    pub fn slot_layer(&self, key: &str) -> Option<ha_assets::ArtLayer> {
        if let Some(l) = self.layers.get(key) {
            return Some(*l);
        }
        let (base, _) = ha_assets::split_variant(key);
        self.layers
            .get(base)
            .copied()
            // Convención de namespace: un `floor/*` es piso aunque nadie lo haya
            // etiquetado. Ver `ha_assets::namespace_layer`.
            .or_else(|| ha_assets::namespace_layer(base))
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

    /// `(frames, fps)` de una pose animada. Lo usa quien necesita saber CUÁNTO
    /// dura la animación para sincronizar el estado del juego con ella — la
    /// transición de una puerta, por ejemplo: el juego la da por abierta cuando
    /// termina de verse abriéndose, no un número fijo de segundos después.
    pub fn pose_frames(&self, key: &str, name: &str) -> Option<(u32, f32)> {
        let s = self.poses.get(key)?.get(name)?.anim?;
        Some((s.frames.max(1), s.fps))
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

    /// **Un marco de UI en 9 PARTES** (`ui/panel`, `ui/button`, `ui/slot_empty`).
    ///
    /// Un `ImageNode` a secas ESTIRA la imagen: un panel de 96×64 px estirado a
    /// 800×400 sale con el borde deformado y las esquinas hechas puré. Por eso el
    /// arte de marco recortado en el editor no se estaba usando y todo el HUD
    /// seguía dibujándose con los colores planos de `ui_theme` — el arte estaba,
    /// faltaba la forma de estirarlo bien.
    ///
    /// El corte sale del recorte que hizo el usuario: se reserva **una celda de
    /// la grilla** (típicamente 32 px) para el borde y el resto es el centro, que
    /// es lo que se repite. Si el recorte mide menos de tres celdas de lado, el
    /// borde se achica para que quede centro: un marco 1×1 no tiene 9 partes.
    pub fn ui_frame_node(&self, key: &str) -> Option<bevy::ui::widget::ImageNode> {
        use bevy::ui::widget::NodeImageMode;
        use bevy::sprite::{BorderRect, SliceScaleMode, TextureSlicer};
        let mut node = self.ui_image_node(key)?;
        let [_, _, w, h] = self.rects.get(key).copied()?;
        // La celda de la grilla es el borde natural del marco. Sin recorte
        // declarado no hay nada que cortar: se devuelve el nodo estirado.
        let cell = self.cells.get(key).copied().unwrap_or((1, 1));
        let (cw, ch) = (w / cell.0.max(1), h / cell.1.max(1));
        let bx = (cw as f32).min(w as f32 / 3.0).max(1.0);
        let by = (ch as f32).min(h as f32 / 3.0).max(1.0);
        node.image_mode = NodeImageMode::Sliced(TextureSlicer {
            border: BorderRect { left: bx, right: bx, top: by, bottom: by },
            // El centro y los bordes se ESTIRAN, no se azulejan: con pixel art a
            // escalas no enteras, repetir deja costuras visibles entre copias.
            center_scale_mode: SliceScaleMode::Stretch,
            sides_scale_mode: SliceScaleMode::Stretch,
            // 4.0 dejaba una esquina de 128 px de arte a 32: el marco se comía
            // el panel. 2.0 sigue el escalado de UI del teléfono sin engordar.
            max_corner_scale: 2.0,
        });
        Some(node)
    }

    /// **El icono de UI de un asset, tal como se ve.** Es el gemelo de
    /// [`Self::world_sprite`] para un `ImageNode`: resuelve el PRIMER frame si el
    /// slot es una tira, la pose representativa si tiene poses (el `center` de un
    /// autotile, el `idle_down` de un personaje) y el recorte de atlas si se curó
    /// desde una hoja. Sin esto, una tarjeta de craft o de construir mostraba la
    /// HOJA ENTERA escalada a 40 px, que es justo el bug de "usa mal los assets".
    pub fn ui_icon(&self, key: &str) -> Option<bevy::ui::widget::ImageNode> {
        let px = |a: UVec2, b: UVec2| Some(Rect::from_corners(a.as_vec2(), b.as_vec2()));
        // 1) Tira de animación: un solo frame, no la fila completa.
        if let Some(s) = self.strips.get(key) {
            if s.cell.x > 0 && s.cell.y > 0 {
                let mut node = bevy::ui::widget::ImageNode::new(self.get(key)?);
                node.rect = px(s.origin, s.origin + s.cell);
                return Some(node);
            }
        }
        // 2) Pose representativa. El orden es el mismo criterio que en el mundo, y
        //    si el usuario nombró sus poses de otra forma se toma la primera.
        let pose_name = ["center", "idle_down", "base", "closed"]
            .into_iter()
            .find(|n| self.has_pose(key, n))
            .map(|n| n.to_string())
            .or_else(|| self.poses.get(key).and_then(|m| m.keys().next().cloned()));
        if let Some(name) = pose_name {
            if let Some(p) = self.poses.get(key).and_then(|m| m.get(&name)) {
                let mut node = bevy::ui::widget::ImageNode::new(self.pose_image(key, &name)?);
                node.rect = match p.anim {
                    Some(s) if s.cell.x > 0 && s.cell.y > 0 => px(s.origin, s.origin + s.cell),
                    _ => {
                        let [x, y, w, h] = p.rect_px;
                        px(UVec2::new(x, y), UVec2::new(x + w, y + h))
                    }
                };
                return Some(node);
            }
        }
        // 3) Slot plano (con su recorte si lo tiene).
        self.ui_image_node(key)
    }

    /// `true` si el material de tile trae arte "vivo" (autotile por poses o una
    /// animación): sólo entonces el mundo lo dibuja como sprite en vez de dejar
    /// el quad vertex-color barato. Un PNG estático plano NO califica (evita
    /// spawnear miles de sprites por el suelo base). Ver `world2d::spawn_chunk_tile_art`.
    pub fn tile_has_art(&self, key: &str) -> bool {
        self.has_poses(key)
            || self.anims.contains_key(key)
            || self.strips.contains_key(key)
            // Los respaldos dibujados por código no tienen poses ni tira, pero SÍ
            // hay que dibujarlos: sin esta rama el mundo se saltaba el material y
            // un acantilado seguía siendo el cuadrado marrón de siempre.
            || self.procedural.contains(key)
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
    // Variantes declaradas (`objects` del manifest): la lista que el juego puede
    // ofrecer (materiales de un piso, skins) y, de esas, las que tienen arte.
    for (base, obj) in &manifest.objects {
        let with_art: Vec<String> = obj
            .variants
            .iter()
            .map(|v| ha_assets::variant_key(base, &v.id))
            .filter(|k| manifest.sprites.contains_key(k))
            .collect();
        if !with_art.is_empty() {
            art.variant_art.insert(base.clone(), with_art);
        }
        if !obj.variants.is_empty() {
            art.objects.insert(base.clone(), obj.variants.clone());
        }
    }
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
                    art.footprint_reach = art.footprint_reach.max(fp.w.max(fp.h) as i64);
                    art.footprints.insert(key.clone(), fp.clone());
                }
            }
            if let Some(em) = e.emission.filter(|em| !em.is_off()) {
                art.emissions.insert(key.clone(), em);
            }
            // Capa del slot: la explícita del combo del editor; si no la tiene,
            // la de su ÚNICA pose etiquetada (assets viejos que declararon
            // "floor"/"roof" como pose suelta). Con varias poses no se asume: ahí
            // la capa es de cada parte y la elige quien las instancia.
            if let Some(l) = e.layer.or_else(|| {
                e.poses
                    .as_ref()
                    .filter(|m| m.len() == 1)
                    .and_then(|m| m.values().next()?.layer)
            }) {
                art.layers.insert(key.clone(), l);
            }
            if let Some(r) = e.rect {
                art.rects.insert(key.clone(), [r.col * gw, r.row * gh, r.w * gw, r.h * gh]);
            }
            // Tamaño en tiles del slot (recorte, pose representativa o tira).
            let cells = e
                .rect
                .map(|r| (r.w, r.h))
                .or_else(|| {
                    e.poses
                        .as_ref()
                        .and_then(|m| {
                            m.get("idle_down").or_else(|| m.get("center")).or_else(|| m.values().next())
                        })
                        .map(|p| (p.rect.w, p.rect.h))
                })
                .or_else(|| e.strip.map(|s| (s.w, s.h)));
            if let Some((cw, ch)) = cells {
                art.cells.insert(key.clone(), (cw.max(1), ch.max(1)));
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
        if !art.objects.is_empty() {
            eprintln!(
                "ha-art: {} objetos con variantes ({} con arte)",
                art.objects.len(),
                art.variant_art.values().map(|v| v.len()).sum::<usize>()
            );
        }
    }
}

/// Item/UI icons: any `items/<item_id>.png` in the pack replaces the
/// procedural icon EVERYWHERE (hotbar, bag, craft list, machine sprites) by
/// pre-seeding the icon cache before the HUD first asks for it.
/// **Con su recorte.** Un slot `items/*` curado desde un ATLAS guarda el rect en
/// `art/manifest.json`; sin pasarlo acá, la mochila y la mano dibujaban la hoja entera
/// escalada a 40 px (era el "a veces el juego usa mal los assets"). Un PNG suelto
/// no tiene rect y se comporta como siempre.
fn preload_item_icons(art: Res<ArtLibrary>, mut icons: ResMut<ItemIconCache>) {
    for (key, handle) in &art.sprites {
        if let Some(item_id) = key.strip_prefix("items/") {
            let rect = art.rects.get(key).copied().map(|[x, y, w, h]| {
                Rect::from_corners(
                    Vec2::new(x as f32, y as f32),
                    Vec2::new((x + w) as f32, (y + h) as f32),
                )
            });
            icons.preload(item_id, handle.clone(), rect);
        }
    }
}

#[cfg(test)]
impl ArtLibrary {
    /// Biblioteca mínima con la huella de un slot — para probar la colisión de
    /// decor sin levantar Bevy ni cargar PNGs.
    pub(crate) fn test_footprint(key: &str, fp: ha_assets::CollisionFootprint) -> Self {
        let mut a = Self::default();
        a.footprint_reach = fp.w.max(fp.h) as i64;
        a.footprints.insert(key.to_string(), fp);
        a
    }

    /// Biblioteca mínima con el flag "bloquea el paso" de un slot.
    pub(crate) fn test_collides(key: &str, collides: bool) -> Self {
        let mut a = Self::default();
        a.collides.insert(key.to_string(), collides);
        a
    }
}
