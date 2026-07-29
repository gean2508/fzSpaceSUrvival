//! Modelo del **art-manifest del juego** (`native/crates/ha-game/assets/art/
//! manifest.json`): el mapeo clave-lógica → PNG (+ animación) que el juego lee
//! en boot para reskinearse. El editor escribe esto; `ha-game::art2d` lo lee.
//!
//! Compatible hacia atrás: el formato viejo era `{"key": "ruta.png"}` (string).
//! El nuevo permite `{"key": {"png": "...", "anim": {"frames": N, "fps": F}}}`.
//! Se leen los dos; el editor siempre escribe el nuevo.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Animación de un sprite tira-de-frames: N frames reproducidos a `fps`. `frames`
/// es la **cantidad de frames**, NO la cantidad de celdas — cada frame puede ocupar
/// un bloque de varias celdas (ver `CellRect`/`StripDir`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ArtAnim {
    pub frames: u32,
    pub fps: f32,
}

/// Dirección en la que se disponen los frames contiguos de una tira: a la derecha
/// (columna+) o hacia abajo (fila+). El primer frame lo define el `CellRect`; los
/// siguientes son bloques del MISMO tamaño pegados en esta dirección.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StripDir {
    #[default]
    Right,
    Down,
}

/// Capa lógica de un recorte/pose dentro de una estructura (edificio, casa):
/// el juego la usa para instanciar cada parte a la altura correcta — `Floor`
/// bajo el jugador, `Wall`/`Object` Y-sorted, `Roof` encima (y translúcido
/// cuando el jugador está dentro del footprint). Opcional y retro-compatible:
/// sin capa, todo se comporta como hoy (`Object`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ArtLayer {
    Floor,
    /// Subsuelo/sótano: se ve al bajar de nivel (z<0 interior). Ver H4.
    Basement,
    Wall,
    #[default]
    Object,
    Roof,
    /// Luces: se renderiza como GLOW aditivo, visible de noche (día/noche, H4).
    Light,
}

/// Huella de colisión en el PLANO del suelo (vista cenital), en celdas de tile.
/// El editor pinta una grilla `w×h`, marca la celda ANCLA (la que coincide con la
/// posición del asset en el mundo) y qué celdas son sólidas. El juego, al colocar
/// el asset en la celda ancla, marca sólidas las celdas `(ancla + offset)`. Permite
/// montañas sólidas, cráteres caminables (anillo sólido + centro libre) y assets
/// grandes que bloquean su base — no solo 1 celda. `None`/vacío = colisión default
/// por tipo (1 celda). Opcional y retro-compatible (A1, `PLAN_MAESTRO §3.A`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CollisionFootprint {
    pub w: u32,
    pub h: u32,
    /// Celda ancla dentro de la grilla (col,row) — coincide con la posición mundo.
    pub ax: u32,
    pub az: u32,
    /// Celdas sólidas (col,row) dentro de la grilla `w×h`.
    pub solid: Vec<(u32, u32)>,
}

impl CollisionFootprint {
    /// Offsets `(dx,dz)` de las celdas sólidas RELATIVOS al ancla (los que el juego
    /// suma a la celda de colocación para marcar sólido).
    pub fn solid_offsets(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        let (ax, az) = (self.ax as i32, self.az as i32);
        self.solid.iter().map(move |&(c, r)| (c as i32 - ax, r as i32 - az))
    }
    /// ¿Tiene al menos una celda sólida? (vacío = como si no hubiera huella).
    pub fn is_empty(&self) -> bool {
        self.solid.is_empty()
    }
}

/// **Emisión de luz** de un slot: el asset ilumina su entorno con un halo suave
/// (aura difuminada, NO luz volumétrica — ver `ha-game::light2d`). El editor la
/// escribe (casilla "emite luz" + color/radio, o derivada de los píxeles que
/// pintaste con el pincel de LUZ en el Image Lab); el juego la lee y le cuelga
/// un glow aditivo al sprite. `None` = el asset no ilumina (coste cero).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Emission {
    /// Tinte del halo (sRGB 0..1). Cálido = fuego/lámpara, frío = cristal/Ki.
    #[serde(default = "em_color")]
    pub color: [f32; 3],
    /// Radio del halo en METROS de mundo (1 tile = 1 m). ~2 = vela, ~8 = farola.
    #[serde(default = "em_radius")]
    pub radius: f32,
    /// Fuerza del glow (0..3). 1 = normal; >1 quema el centro.
    #[serde(default = "em_intensity")]
    pub intensity: f32,
    /// Desplazamiento del foco respecto al centro del sprite, en metros
    /// `[x, y]` (y+ = arriba en pantalla). La ventana de una casa no está en su
    /// centro geométrico — esto lo corrige sin tocar el arte.
    #[serde(default)]
    pub offset: [f32; 2],
    /// Parpadeo 0..1: 0 = luz fija (lámpara), 1 = llama viva (fogata/antorcha).
    #[serde(default)]
    pub flicker: f32,
    /// `true` = se ve también de día (cristal mágico, pantalla). Por defecto la
    /// luz sólo aparece de noche / bajo tierra / en el vacío, que es lo barato y
    /// lo que se ve bien.
    #[serde(default)]
    pub day: bool,
}

fn em_color() -> [f32; 3] {
    [1.0, 0.82, 0.45]
}
fn em_radius() -> f32 {
    4.0
}
fn em_intensity() -> f32 {
    1.0
}

impl Default for Emission {
    fn default() -> Self {
        Self {
            color: em_color(),
            radius: em_radius(),
            intensity: em_intensity(),
            offset: [0.0, 0.0],
            flicker: 0.0,
            day: false,
        }
    }
}

impl Emission {
    /// Emisión sin efecto visible (radio o intensidad en cero) → se trata como
    /// "no emite" para no pagar un quad por nada.
    pub fn is_off(&self) -> bool {
        self.radius <= 0.0 || self.intensity <= 0.0
    }
}

/// Rectángulo en **celdas** dentro de una hoja (no píxeles): el editor lo elige
/// arrastrando en la grilla; el juego lo resuelve a px con `grid`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellRect {
    pub col: u32,
    pub row: u32,
    pub w: u32,
    pub h: u32,
}

impl CellRect {
    /// Cantidad de celdas que ocupa el rango (w×h). Es el TAMAÑO de un frame
    /// cuando el rango representa un frame multi-celda — NO la cantidad de frames.
    pub fn cells(&self) -> u32 {
        self.w.max(1) * self.h.max(1)
    }
}

/// Una pose direccional/nombrada de un personaje (idle_down, run_left, sit…):
/// un recorte estático, o una tira animada si trae `anim`. Cuando es animada, el
/// `rect` es el PRIMER frame (su tamaño = tamaño de cada frame) y `anim.frames` la
/// cantidad; los frames siguientes son bloques contiguos en `dir`.
///
/// **Multi-hoja**: una pose puede traer SU PROPIO `png` (y su `grid`). Así un
/// mismo asset se arma con VARIAS imágenes — el caso real del pack: `idle_left`
/// es un PNG, `idle_down` es otro. Sin `png`, la pose recorta la hoja del slot
/// (comportamiento de siempre, retro-compatible).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pose {
    pub rect: CellRect,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anim: Option<ArtAnim>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<StripDir>,
    /// Capa de esta pose dentro de una estructura (ver `ArtLayer`). `None` =
    /// comportamiento actual (`Object`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<ArtLayer>,
    /// Hoja propia de esta pose (ruta relativa a los assets del juego). `None` =
    /// la hoja del slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub png: Option<String>,
    /// Celda px de ESA hoja (una hoja de 16 px conviviendo con una de 32).
    /// `None` = la grilla del slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid: Option<(u32, u32)>,
}

impl Pose {
    /// Recorte estático simple (sin animación, sin capa, hoja del slot).
    pub fn still(rect: CellRect) -> Self {
        Self { rect, anim: None, dir: None, layer: None, png: None, grid: None }
    }

    /// Hoja efectiva de la pose: la suya si la tiene, si no la del slot.
    pub fn png_or<'a>(&'a self, slot_png: &'a str) -> &'a str {
        self.png.as_deref().unwrap_or(slot_png)
    }

    /// Grilla efectiva de la pose: la suya si la tiene, si no la del slot.
    pub fn grid_or(&self, slot_grid: (u32, u32)) -> (u32, u32) {
        self.grid.unwrap_or(slot_grid)
    }
}

/// Entrada completa (formato nuevo). Todos los campos nuevos son opcionales →
/// el juego viejo y las entradas simples siguen funcionando.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtEntry {
    pub png: String,
    /// Tamaño de celda px de esta hoja (default 32 si falta).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid: Option<(u32, u32)>,
    /// Recorte estático del sprite dentro de la hoja (si es un sheet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rect: Option<CellRect>,
    /// Animación de una tira. `anim.frames` = CANTIDAD de frames (no celdas).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anim: Option<ArtAnim>,
    /// Rect (en celdas) del PRIMER frame de la tira: su tamaño (w×h) es el tamaño
    /// de cada frame. Los frames siguientes son bloques contiguos en `strip_dir`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strip: Option<CellRect>,
    /// Dirección en que avanzan los frames de `strip` (default: a la derecha).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strip_dir: Option<StripDir>,
    /// Poses direccionales/nombradas (personajes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poses: Option<std::collections::BTreeMap<String, Pose>>,
    /// Colisión de caminado: si el asset (decor) bloquea el paso del jugador.
    /// `None` = default por tipo (árboles bloquean, el resto no). El editor lo
    /// expone como casilla por slot. Primer paso hacia la colisión por-celda
    /// precisa (relieves/cráteres) — ver `PLAN_COLISIONES_Y_CIUDADES.md`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collides: Option<bool>,
    /// Huella de colisión multi-celda (A1). `None` = colisión de 1 celda por el
    /// campo `collides`/default de tipo. Ver `CollisionFootprint`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footprint: Option<CollisionFootprint>,
    /// Luz que emite este asset (halo suave). `None` = no ilumina. Ver `Emission`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emission: Option<Emission>,
}

impl ArtEntry {
    pub fn new(png: String) -> Self {
        Self {
            png,
            grid: None,
            rect: None,
            anim: None,
            strip: None,
            strip_dir: None,
            poses: None,
            collides: None,
            footprint: None,
            emission: None,
        }
    }
}

/// Valor de una clave: string suelto (viejo) u objeto (nuevo). `untagged` prueba
/// string primero y objeto después.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArtValue {
    Path(String),
    Full(ArtEntry),
}

impl ArtValue {
    pub fn png(&self) -> &str {
        match self {
            ArtValue::Path(p) => p,
            ArtValue::Full(e) => &e.png,
        }
    }
    pub fn anim(&self) -> Option<ArtAnim> {
        match self {
            ArtValue::Path(_) => None,
            ArtValue::Full(e) => e.anim,
        }
    }
    /// Dirección de la tira (`strip`) si la entrada la trae; `None` en el formato
    /// viejo o si el slot no es una animación de sub-región.
    pub fn strip_dir(&self) -> Option<StripDir> {
        match self {
            ArtValue::Path(_) => None,
            ArtValue::Full(e) => e.strip_dir,
        }
    }
    /// Grilla (celda px) usada para interpretar `rect`/`strip`/`poses` de esta
    /// entrada. Default 32×32 si no está fijada.
    pub fn grid(&self) -> (u32, u32) {
        match self {
            ArtValue::Path(_) => (32, 32),
            ArtValue::Full(e) => e.grid.unwrap_or((32, 32)),
        }
    }
    /// Recorte estático (celdas) del slot, si es un sheet cropeado. `None` si el
    /// PNG es el asset entero.
    pub fn rect(&self) -> Option<CellRect> {
        match self {
            ArtValue::Full(e) => e.rect,
            ArtValue::Path(_) => None,
        }
    }
    /// Normaliza a la forma completa (lo que escribe el editor).
    pub fn into_full(self) -> ArtEntry {
        match self {
            ArtValue::Path(png) => ArtEntry::new(png),
            ArtValue::Full(e) => e,
        }
    }
}

/// El art-manifest del juego.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ArtManifest {
    #[serde(default)]
    pub sprites: BTreeMap<String, ArtValue>,
}

impl ArtManifest {
    pub fn from_json(s: &str) -> serde_json::Result<Self> {
        serde_json::from_str(s)
    }

    /// JSON legible y ordenado (BTreeMap) → diffs mínimos.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// Asigna/actualiza un slot con PNG + animación opcional (forma completa).
    /// Conserva rect/strip/poses/grid si el slot ya existía con el mismo PNG.
    pub fn set(&mut self, key: &str, png: String, anim: Option<ArtAnim>) {
        let e = self.full_mut(key, png);
        e.anim = anim;
    }

    /// Devuelve la entrada completa del slot (creándola/normalizándola). Si el
    /// PNG cambió, resetea los recortes (eran de otra hoja).
    pub fn full_mut(&mut self, key: &str, png: String) -> &mut ArtEntry {
        let slot = self
            .sprites
            .entry(key.to_string())
            .or_insert_with(|| ArtValue::Full(ArtEntry::new(png.clone())));
        // Normalizar a Full.
        if let ArtValue::Path(p) = slot {
            *slot = ArtValue::Full(ArtEntry::new(std::mem::take(p)));
        }
        let ArtValue::Full(entry) = slot else { unreachable!() };
        if entry.png != png {
            *entry = ArtEntry::new(png);
        }
        entry
    }

    /// Override de colisión del slot (casilla "bloquea el paso" del editor).
    /// `None` = usar el default por tipo del juego. Sólo aplica a slots que ya
    /// tienen arte (normaliza `Path`→`Full` para poder guardar el flag).
    pub fn set_collides(&mut self, key: &str, collides: Option<bool>) {
        let Some(slot) = self.sprites.get_mut(key) else { return };
        if let ArtValue::Path(p) = slot {
            *slot = ArtValue::Full(ArtEntry::new(std::mem::take(p)));
        }
        if let ArtValue::Full(e) = slot {
            e.collides = collides;
        }
    }

    /// Override de colisión actual del slot, si lo hay.
    pub fn collides(&self, key: &str) -> Option<bool> {
        match self.sprites.get(key) {
            Some(ArtValue::Full(e)) => e.collides,
            _ => None,
        }
    }

    /// Huella de colisión multi-celda del slot (A1), si tiene una con celdas.
    pub fn footprint(&self, key: &str) -> Option<&CollisionFootprint> {
        match self.sprites.get(key) {
            Some(ArtValue::Full(e)) => e.footprint.as_ref().filter(|f| !f.is_empty()),
            _ => None,
        }
    }

    /// Setea/limpia la huella del slot (editor). `None` o vacía la borra.
    /// Normaliza `Path`→`Full` para poder guardarla.
    pub fn set_footprint(&mut self, key: &str, footprint: Option<CollisionFootprint>) {
        let Some(slot) = self.sprites.get_mut(key) else { return };
        if let ArtValue::Path(p) = slot {
            *slot = ArtValue::Full(ArtEntry::new(std::mem::take(p)));
        }
        if let ArtValue::Full(e) = slot {
            e.footprint = footprint.filter(|f| !f.is_empty());
        }
    }

    /// Luz que emite el slot, si el usuario la activó y tiene efecto visible.
    pub fn emission(&self, key: &str) -> Option<&Emission> {
        match self.sprites.get(key) {
            Some(ArtValue::Full(e)) => e.emission.as_ref().filter(|em| !em.is_off()),
            _ => None,
        }
    }

    /// Setea/limpia la emisión del slot (editor). `None` o una emisión apagada la
    /// borra. Normaliza `Path`→`Full` para poder guardarla.
    pub fn set_emission(&mut self, key: &str, emission: Option<Emission>) {
        let Some(slot) = self.sprites.get_mut(key) else { return };
        if let ArtValue::Path(p) = slot {
            *slot = ArtValue::Full(ArtEntry::new(std::mem::take(p)));
        }
        if let ArtValue::Full(e) = slot {
            e.emission = emission.filter(|em| !em.is_off());
        }
    }

    /// Marca un recorte estático (rect en celdas) para el slot.
    pub fn set_rect(&mut self, key: &str, png: String, grid: (u32, u32), rect: CellRect) {
        let e = self.full_mut(key, png);
        e.grid = Some(grid);
        e.rect = Some(rect);
        e.strip = None; // rect estático y strip son excluyentes
    }

    /// Marca una tira de animación. `first_frame` = rect (en celdas) del PRIMER
    /// frame; su tamaño (w×h) es el tamaño de cada frame. `frames` = CANTIDAD de
    /// frames (no celdas). Los frames avanzan contiguos en `dir`.
    pub fn set_strip(
        &mut self,
        key: &str,
        png: String,
        grid: (u32, u32),
        first_frame: CellRect,
        frames: u32,
        fps: f32,
        dir: StripDir,
    ) {
        let e = self.full_mut(key, png);
        e.grid = Some(grid);
        e.strip = Some(first_frame);
        e.strip_dir = Some(dir);
        e.anim = Some(ArtAnim { frames: frames.max(1), fps });
        e.rect = None;
    }

    /// Agrega/actualiza una pose direccional/nombrada, **sin destruir las demás
    /// aunque venga de otra hoja**. Si el slot todavía no tiene arte, `png` pasa a
    /// ser su hoja principal; si ya tiene otra, la pose se guarda con SU hoja y su
    /// grilla (assets armados con varios PNG: uno por dirección/estado).
    pub fn set_pose(&mut self, key: &str, png: String, grid: (u32, u32), name: &str, pose: Pose) {
        let slot = self
            .sprites
            .entry(key.to_string())
            .or_insert_with(|| ArtValue::Full(ArtEntry::new(png.clone())));
        if let ArtValue::Path(p) = slot {
            *slot = ArtValue::Full(ArtEntry::new(std::mem::take(p)));
        }
        let ArtValue::Full(entry) = slot else { unreachable!() };
        let mut pose = pose;
        if entry.png == png {
            // Misma hoja que el slot → la pose no necesita repetirla.
            entry.grid = Some(grid);
            pose.png = None;
            pose.grid = None;
        } else {
            // Otra hoja: viaja con la pose. El resto del slot queda intacto.
            pose.png = Some(png);
            pose.grid = Some(grid);
        }
        entry.poses.get_or_insert_with(Default::default).insert(name.to_string(), pose);
    }

    /// Todas las hojas que usa el slot: la principal + las propias de cada pose.
    /// El juego las carga todas para poder cambiar de imagen al cambiar de pose.
    pub fn sheets(&self, key: &str) -> Vec<String> {
        let mut out = Vec::new();
        let Some(v) = self.sprites.get(key) else { return out };
        out.push(v.png().to_string());
        if let ArtValue::Full(e) = v {
            for pose in e.poses.iter().flat_map(|p| p.values()) {
                if let Some(p) = &pose.png {
                    if !out.iter().any(|x| x == p) {
                        out.push(p.clone());
                    }
                }
            }
        }
        out
    }

    pub fn remove_pose(&mut self, key: &str, name: &str) {
        if let Some(ArtValue::Full(e)) = self.sprites.get_mut(key) {
            if let Some(p) = e.poses.as_mut() {
                p.remove(name);
                if p.is_empty() {
                    e.poses = None;
                }
            }
        }
    }

    /// Quita el binding de un slot (vuelve a procedural en el juego).
    pub fn clear(&mut self, key: &str) {
        self.sprites.remove(key);
    }

    pub fn get(&self, key: &str) -> Option<&ArtValue> {
        self.sprites.get(key)
    }

    /// Poses guardadas de un slot (si es una entrada completa con poses).
    pub fn poses(&self, key: &str) -> Option<&std::collections::BTreeMap<String, Pose>> {
        match self.sprites.get(key) {
            Some(ArtValue::Full(e)) => e.poses.as_ref(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_old_string_format() {
        let old = r#"{ "sprites": { "decor/tree": "art/decor/tree.png" } }"#;
        let m = ArtManifest::from_json(old).unwrap();
        let v = m.get("decor/tree").unwrap();
        assert_eq!(v.png(), "art/decor/tree.png");
        assert_eq!(v.anim(), None);
    }

    #[test]
    fn reads_new_object_format_with_anim() {
        let new = r#"{ "sprites": { "tiles/water": { "png": "art/tiles/water.png", "anim": { "frames": 8, "fps": 6.0 } } } }"#;
        let m = ArtManifest::from_json(new).unwrap();
        let v = m.get("tiles/water").unwrap();
        assert_eq!(v.png(), "art/tiles/water.png");
        assert_eq!(v.anim(), Some(ArtAnim { frames: 8, fps: 6.0 }));
    }

    #[test]
    fn strip_frames_are_count_not_cells() {
        // Un frame de 3×3 celdas, 5 frames a la derecha: frames = 5 (NO 45 celdas).
        let mut m = ArtManifest::default();
        let first = CellRect { col: 0, row: 0, w: 3, h: 3 };
        m.set_strip("player/character", "art/atlas/hero.png".into(), (16, 16), first, 5, 8.0, StripDir::Right);
        let ArtValue::Full(e) = m.get("player/character").unwrap() else { panic!("full") };
        assert_eq!(e.strip, Some(first), "el strip guarda el rect del PRIMER frame");
        assert_eq!(e.anim.unwrap().frames, 5, "frames = cantidad, no celdas (9)");
        assert_eq!(e.strip_dir, Some(StripDir::Right));
        // Round-trip por JSON.
        let json = m.to_json().unwrap();
        assert_eq!(m, ArtManifest::from_json(&json).unwrap());
    }

    #[test]
    fn pose_layer_roundtrips_and_is_backward_compatible() {
        let mut m = ArtManifest::default();
        let rect = CellRect { col: 2, row: 1, w: 4, h: 3 };
        m.set_pose(
            "building/casa_chica",
            "art/atlas/casas.png".into(),
            (32, 32),
            "roof",
            Pose { layer: Some(ArtLayer::Roof), ..Pose::still(rect) },
        );
        m.set_pose(
            "building/casa_chica",
            "art/atlas/casas.png".into(),
            (32, 32),
            "base",
            Pose::still(rect),
        );
        let json = m.to_json().unwrap();
        let back = ArtManifest::from_json(&json).unwrap();
        assert_eq!(m, back);
        let poses = back.poses("building/casa_chica").unwrap();
        assert_eq!(poses["roof"].layer, Some(ArtLayer::Roof));
        assert_eq!(poses["base"].layer, None, "sin capa = compat (Object)");
        // Un manifest viejo (sin `layer`) sigue cargando.
        assert!(!json.contains("\"layer\": null"), "layer ausente se omite");
    }

    #[test]
    fn poses_from_several_pngs_coexist_in_one_slot() {
        // El caso real: el cuerpo tiene UN PNG por dirección. Antes, guardar la
        // 2ª pose reseteaba la entrada y se perdía la 1ª.
        let mut m = ArtManifest::default();
        let r = CellRect { col: 0, row: 0, w: 1, h: 1 };
        m.set_pose("npc/cuerpo", "art/cuerpo_izq.png".into(), (32, 32), "idle_left", Pose::still(r));
        m.set_pose("npc/cuerpo", "art/cuerpo_abajo.png".into(), (16, 16), "idle_down", Pose::still(r));
        m.set_pose("npc/cuerpo", "art/cuerpo_arriba.png".into(), (32, 32), "idle_up", Pose::still(r));

        let poses = m.poses("npc/cuerpo").expect("el slot conserva las tres poses");
        assert_eq!(poses.len(), 3, "ninguna pose pisa a las otras");
        // La 1ª hoja es la del slot → su pose no repite el png.
        assert_eq!(poses["idle_left"].png, None);
        assert_eq!(m.get("npc/cuerpo").unwrap().png(), "art/cuerpo_izq.png");
        // Las otras viajan con SU hoja y SU grilla.
        assert_eq!(poses["idle_down"].png.as_deref(), Some("art/cuerpo_abajo.png"));
        assert_eq!(poses["idle_down"].grid, Some((16, 16)), "cada hoja con su celda px");
        assert_eq!(poses["idle_up"].png.as_deref(), Some("art/cuerpo_arriba.png"));
        // Resolución efectiva (lo que hace el juego).
        let slot_png = m.get("npc/cuerpo").unwrap().png().to_string();
        assert_eq!(poses["idle_left"].png_or(&slot_png), "art/cuerpo_izq.png");
        assert_eq!(poses["idle_down"].png_or(&slot_png), "art/cuerpo_abajo.png");
        assert_eq!(poses["idle_left"].grid_or((32, 32)), (32, 32));
        // El juego debe cargar las 3 hojas.
        let sheets = m.sheets("npc/cuerpo");
        assert_eq!(sheets.len(), 3, "{sheets:?}");
        // Round-trip.
        assert_eq!(m, ArtManifest::from_json(&m.to_json().unwrap()).unwrap());
    }

    #[test]
    fn single_sheet_poses_stay_lean_and_backward_compatible() {
        // Un slot de una sola hoja no gana campos nuevos en el JSON.
        let mut m = ArtManifest::default();
        let r = CellRect { col: 1, row: 2, w: 1, h: 1 };
        m.set_pose("player/character", "art/hero.png".into(), (32, 32), "idle_down", Pose::still(r));
        let json = m.to_json().unwrap();
        assert!(!json.contains("\"png\": \"art/hero.png\"\n        }"), "{json}");
        assert_eq!(json.matches("art/hero.png").count(), 1, "la hoja no se duplica por pose");
        // Un manifest viejo (poses sin png/grid) sigue cargando.
        let old = r#"{ "sprites": { "player/character": { "png": "a.png", "grid": [32,32],
            "poses": { "idle_down": { "rect": { "col": 0, "row": 0, "w": 1, "h": 1 } } } } } }"#;
        let back = ArtManifest::from_json(old).unwrap();
        let p = &back.poses("player/character").unwrap()["idle_down"];
        assert_eq!(p.png, None);
        assert_eq!(p.png_or("a.png"), "a.png");
    }

    #[test]
    fn set_and_roundtrip() {
        let mut m = ArtManifest::default();
        m.set("decor/tree", "art/decor/oak.png".into(), None);
        m.set("tiles/water", "art/tiles/water.png".into(), Some(ArtAnim { frames: 4, fps: 8.0 }));
        let json = m.to_json().unwrap();
        let back = ArtManifest::from_json(&json).unwrap();
        assert_eq!(m, back);
        // El editor escribe forma completa (objeto), legible por el juego nuevo.
        assert!(json.contains("\"png\""));
    }

    #[test]
    fn footprint_offsets_relative_to_anchor() {
        // Cráter 3x3: anillo sólido, centro (=ancla) libre.
        let fp = CollisionFootprint {
            w: 3,
            h: 3,
            ax: 1,
            az: 1,
            solid: vec![(0, 0), (1, 0), (2, 0), (0, 1), (2, 1), (0, 2), (1, 2), (2, 2)],
        };
        let offs: std::collections::HashSet<(i32, i32)> = fp.solid_offsets().collect();
        assert!(offs.contains(&(-1, -1)) && offs.contains(&(1, 1))); // esquinas del anillo
        assert!(!offs.contains(&(0, 0))); // el centro (ancla) es caminable
        assert_eq!(offs.len(), 8);
        assert!(!fp.is_empty());
    }

    #[test]
    fn emission_roundtrip_and_absent_by_default() {
        let mut m = ArtManifest::default();
        m.set("decor/farola", "art/decor/farola.png".into(), None);
        // Sin luz: el campo no aparece (retro-compat con manifests viejos).
        assert!(!m.to_json().unwrap().contains("emission"));
        assert!(m.emission("decor/farola").is_none());
        m.set_emission(
            "decor/farola",
            Some(Emission { radius: 7.5, flicker: 0.3, ..Default::default() }),
        );
        let back = ArtManifest::from_json(&m.to_json().unwrap()).unwrap();
        assert_eq!(m, back);
        let em = back.emission("decor/farola").unwrap();
        assert_eq!(em.radius, 7.5);
        assert_eq!(em.color, [1.0, 0.82, 0.45], "default cálido");
        assert!(!em.day, "por defecto la luz es nocturna (barato)");
        // Radio 0 = apagada → se descarta (no cuesta un quad por nada).
        m.set_emission("decor/farola", Some(Emission { radius: 0.0, ..Default::default() }));
        assert!(m.emission("decor/farola").is_none());
    }

    #[test]
    fn emission_reads_partial_json() {
        // El usuario (o un script) puede escribir sólo lo que le importa: el
        // resto toma defaults sensatos en vez de fallar el parseo.
        let raw = r#"{ "sprites": { "decor/vela": { "png": "a.png",
            "emission": { "radius": 2.0, "flicker": 1.0 } } } }"#;
        let m = ArtManifest::from_json(raw).unwrap();
        let em = m.emission("decor/vela").unwrap();
        assert_eq!(em.radius, 2.0);
        assert_eq!(em.intensity, 1.0);
        assert_eq!(em.offset, [0.0, 0.0]);
    }

    #[test]
    fn footprint_roundtrip_and_default_absent() {
        let mut m = ArtManifest::default();
        m.set("decor/mountain", "art/decor/mountain.png".into(), None);
        // Sin huella: el campo no aparece en el JSON (retro-compat).
        assert!(!m.to_json().unwrap().contains("footprint"));
        m.set_footprint(
            "decor/mountain",
            Some(CollisionFootprint { w: 2, h: 2, ax: 0, az: 1, solid: vec![(0, 1), (1, 1)] }),
        );
        let back = ArtManifest::from_json(&m.to_json().unwrap()).unwrap();
        assert_eq!(m, back);
        assert_eq!(back.footprint("decor/mountain").unwrap().solid.len(), 2);
        // Huella vacía se descarta (equivale a sin huella).
        m.set_footprint("decor/mountain", Some(CollisionFootprint::default()));
        assert!(m.footprint("decor/mountain").is_none());
    }
}
