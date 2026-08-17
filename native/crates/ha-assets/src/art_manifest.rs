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

/// Capa que le corresponde a una clave por **convención de namespace**, cuando
/// el slot no la declaró explícitamente. Un `floor/*` es un piso: no hay caso en
/// que un piso deba dibujarse encima del jugador, y esperar a que alguien tilde
/// la casilla asset por asset sólo garantiza que el bug reaparezca en el próximo
/// piso que se agregue. El resto queda sin default (→ `Object`) porque ahí la
/// intención SÍ es ambigua: un `decor/*` puede ser un arbusto o un charco.
pub fn namespace_layer(key: &str) -> Option<ArtLayer> {
    match key.split('/').next()? {
        "floor" | "stairs" => Some(ArtLayer::Floor),
        // Un `room/*` es una PLANTA: el sprite es el cuarto visto desde arriba,
        // piso y paredes en la misma imagen. Y-sorteado te tapaba entero al
        // entrar — plantabas un cuarto y desaparecías dentro de él. Va al piso;
        // lo que te frena en sus paredes es la huella pintada, no la capa
        // (ver "Capa != colision" en `ha-game::layers`).
        "room" => Some(ArtLayer::Floor),
        // Un techo SIEMPRE tiene que poder desvanecerse cuando estas debajo: si
        // cae en la banda Y-sort no hay forma de entrar a lo que cubre.
        "roof" => Some(ArtLayer::Roof),
        // Una pared es una pared: Y-sorted igual que un objeto, pero la capa es
        // lo que hace que frene por defecto sin tener que tildar slot por slot.
        "wall" => Some(ArtLayer::Wall),
        _ => None,
    }
}

/// **Que se puede HACER con un asset, y desde donde.**
///
/// Hasta ahora lo interactuable era un enum fijo de siete cosas en `ha-shared`
/// (arbol, roca, cofre, tres menas y dos gemas): un banco, una puerta, un
/// surtidor o un muelle NO podian ser interactuables porque el sistema no tenia
/// forma de representarlos — por eso no aparecia el boton al acercarse. Ahora lo
/// declara el SLOT, igual que la capa y la huella, y lo pinta el usuario en HA
/// Studio.
///
/// La `zona` es lo que hace posible el muelle: el asset entero no es
/// interactuable, pero un par de sub-celdas en su punta si, y ahi es donde se
/// pesca. Vacia = toda la huella del asset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Interaccion {
    /// Que se hace. En castellano porque es lo que el usuario elige de una lista
    /// en el editor y lo que el juego muestra en el boton ("SENTARSE", "PICAR").
    pub verbo: String,
    /// **Que hace falta para que el boton aparezca.** Sin requisitos, el boton
    /// sale siempre; con ellos, sale sólo cuando se cumplen todos.
    ///
    /// Es lo que separa "puedo cocinar" de "tengo con que cocinar": una cocina
    /// sin ingredientes muestra el objeto pero no la accion, que es mas honesto
    /// que ofrecerte un boton que no hace nada.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requiere: Vec<Requisito>,
    /// Sub-celdas desde donde se puede interactuar, en la misma grilla que
    /// `CollisionFootprint` (mismo `w`,`h`,`res` del slot). Vacio = todo el asset.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub zona: Vec<(u32, u32)>,
}

/// **Una condicion para poder interactuar.**
///
/// Deliberadamente pocas y combinables: cada una se evalua sola y se exigen
/// TODAS. Empezar con una lista corta y clara es mejor que un mini-lenguaje que
/// nadie va a poder leer despues.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Requisito {
    /// Hay que tener esto EN LA MANO (una pala para cavar).
    EnMano(String),
    /// Alcanza con tenerlo en la mochila (un hacha para talar: no hace falta
    /// equiparla, tenerla basta).
    EnMochila(String),
    /// Hay que conocer la receta (cocinar algo que todavia no aprendiste).
    Receta(String),
    /// Solo de dia / solo de noche (una fogata se enciende de noche).
    DeNoche(bool),
    /// La entidad tiene que estar en este estado ("quieto", "dormido", "vivo").
    /// Vale para animales y NPCs: no ordeñas una vaca corriendo.
    Estado(String),
}

/// Los estados que ofrece el editor para `Requisito::Estado`.
pub const ESTADOS: &[&str] = &["quieto", "dormido", "despierto", "vivo", "domesticado", "adulto"];

/// Los verbos que ofrece el editor. Lista corta a proposito: si hace falta uno
/// nuevo se agrega aca y aparece en HA Studio sin tocar nada mas.
pub const VERBOS: &[&str] = &[
    "PICAR",     // roca, mena
    "TALAR",     // arbol
    "ABRIR",     // puerta, cofre, heladera
    "SENTARSE",  // banco, silla, sillon
    "DORMIR",    // cama
    "USAR",      // maquina, surtidor, cajero
    "PESCAR",    // muelle, orilla
    "COCINAR",   // cocina, fogon
    "SUBIR",     // vehiculo, escalera
    "HABLAR",    // NPC
];

/// Huella de colisión en el PLANO del suelo (vista cenital). El editor pinta una
/// grilla de `w×h` TILES —subdividida en `res` sub-celdas por tile— sobre el
/// sprite, marca la celda ANCLA (la que coincide con la posición del asset en el
/// mundo) y qué sub-celdas son sólidas. Permite montañas sólidas, cráteres
/// caminables (anillo sólido + centro libre), assets grandes que bloquean su base
/// y, con `res > 1`, colisión **perfilada al sprite** en vez de al tile: un poste
/// de farol frena en sus 25 cm, no en el metro entero. `None`/vacío = colisión
/// default por tipo (1 celda). Retro-compatible (`biblia.md` §8): sin
/// `res` se lee como 1 y `solid` vuelve a ser una celda por tile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollisionFootprint {
    pub w: u32,
    pub h: u32,
    /// Celda ancla dentro de la grilla (col,row) — coincide con la posición mundo.
    pub ax: u32,
    pub az: u32,
    /// Celdas sólidas dentro de la grilla `(w*res)×(h*res)`, en SUB-CELDAS.
    pub solid: Vec<(u32, u32)>,
    /// **ESCALONES**: sub-celdas que son un desnivel que se sube y se baja a pie.
    /// No frenan a quien camina —por eso no están en `solid`— pero sí quedan
    /// marcadas como el borde que son, para que las use quien no puede treparlas
    /// (vehículos, y la navegación de los NPC cuando la haya).
    ///
    /// Es el equivalente autorado del bordillo del relieve
    /// (`ha_shared::relief::climbable_edge`): lo mismo que el terreno genera solo
    /// en sus terrazas, acá lo pintás a mano sobre el sprite de una roca, una
    /// tarima o un muro bajo.
    ///
    /// **Se sube y se baja por el mismo sitio, a propósito.** Un escalón que sólo
    /// se pudiera bajar es una trampa: te dejás caer y quedás encerrado abajo. La
    /// simetría es lo que garantiza que marcar escalones nunca pueda dejar a
    /// nadie atrapado, ni en un lado recto ni en una esquina.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub step: Vec<(u32, u32)>,
    /// Sub-celdas por tile con las que se pintó `solid`: 1 = una celda por tile
    /// (formato viejo), 2/4 = colisión más fina que el tile — la que hace que la
    /// caja siga la silueta del sprite (un farol delgado no bloquea el metro
    /// entero). Ausente en manifests viejos → 1, o sea el comportamiento de antes.
    #[serde(default = "fp_res")]
    pub res: u32,
}

fn fp_res() -> u32 {
    1
}

impl Default for CollisionFootprint {
    fn default() -> Self {
        Self { w: 0, h: 0, ax: 0, az: 0, solid: Vec::new(), step: Vec::new(), res: 1 }
    }
}

impl CollisionFootprint {
    /// Sub-celdas por tile, saneado (nunca 0).
    pub fn res(&self) -> u32 {
        self.res.max(1)
    }

    /// Offsets `(dx,dz)` de los TILES sólidos relativos al ancla. Con `res > 1`
    /// un tile cuenta como sólido si alguna de sus sub-celdas lo es — es la vista
    /// gruesa, para quien razona por celdas (constructor, IA de navegación).
    pub fn solid_offsets(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        let (res, ax, az) = (self.res() as i32, self.ax as i32, self.az as i32);
        let mut seen = std::collections::BTreeSet::new();
        self.solid
            .iter()
            .map(move |&(c, r)| (c as i32 / res - ax, r as i32 / res - az))
            .filter(move |o| seen.insert(*o))
    }

    /// Cajas sólidas en TILES (1 tile = 1 m), como `[x0, z0, x1, z1]` relativas a
    /// la esquina mínima del tile ANCLA. Es la colisión fina: con `res = 4` cada
    /// caja mide 0.25 m y el conjunto perfila al sprite.
    pub fn solid_rects(&self) -> impl Iterator<Item = [f32; 4]> + '_ {
        let s = 1.0 / self.res() as f32;
        let (ax, az) = (self.ax as f32, self.az as f32);
        self.solid.iter().map(move |&(c, r)| {
            let (x0, z0) = (c as f32 * s - ax, r as f32 * s - az);
            [x0, z0, x0 + s, z0 + s]
        })
    }

    /// Cajas de ESCALÓN en tiles, mismo sistema de coordenadas que
    /// [`Self::solid_rects`]. No frenan al que camina; existen para quien no
    /// puede treparlas.
    pub fn step_rects(&self) -> impl Iterator<Item = [f32; 4]> + '_ {
        let s = 1.0 / self.res() as f32;
        let (ax, az) = (self.ax as f32, self.az as f32);
        self.step.iter().map(move |&(c, r)| {
            let (x0, z0) = (c as f32 * s - ax, r as f32 * s - az);
            [x0, z0, x0 + s, z0 + s]
        })
    }

    /// ¿Tiene al menos una celda sólida? (vacío = como si no hubiera huella).
    /// Los escalones no cuentan: una huella con SÓLO escalones no frena a nadie
    /// a pie, así que para la colisión de caminado es como no tener huella.
    pub fn is_empty(&self) -> bool {
        self.solid.is_empty()
    }

    /// Celda ANCLA por defecto de un asset de `w×h` tiles: columna del medio, fila
    /// del pie. Es donde "se para" el asset, o sea la celda que apuntás con el
    /// mouse en el editor de mundo.
    pub fn default_anchor(w: u32, h: u32) -> (u32, u32) {
        (w.saturating_sub(1) / 2, h.saturating_sub(1))
    }
}

/// Esquina mínima (en tiles) que ocupa un asset de `cells = (w,h)` tiles anclado
/// en la celda `(tx, tz)` con ancla `(ax, az)`.
///
/// **Regla única de colocación**: el sprite ocupa los tiles
/// `[tx-ax, tx-ax+w) × [tz-az, tz-az+h)`, así que la grilla de la huella cae
/// SIEMPRE alineada al grid del mundo (nada de medios tiles con anchos pares) y
/// lo que ves en el editor es exactamente lo que el juego dibuja y colisiona.
pub fn placement_min_tile(tx: i64, tz: i64, anchor: (u32, u32)) -> (i64, i64) {
    (tx - anchor.0 as i64, tz - anchor.1 as i64)
}

/// Centro del sprite, en TILES (1 tile = 1 m), para dibujarlo anclado en la celda
/// `(tx, tz)`: la celda ANCLA del asset queda centrada sobre la celda apuntada.
///
/// A tamaño natural (`size == cells`) equivale a [`placement_min_tile`] — el
/// sprite cae clavado en el grid. Escalado, el ancla sigue en el mismo punto (el
/// asset crece alrededor de donde lo pusiste, no se va de paseo).
pub fn placement_center_tiles(
    tx: i64,
    tz: i64,
    cells: (u32, u32),
    anchor: (u32, u32),
    size: [f32; 2],
) -> [f32; 2] {
    let (w, h) = (cells.0.max(1) as f32, cells.1.max(1) as f32);
    let fx = (anchor.0 as f32 + 0.5) / w;
    let fz = (anchor.1 as f32 + 0.5) / h;
    [
        tx as f32 + 0.5 + size[0] * (0.5 - fx),
        tz as f32 + 0.5 + size[1] * (0.5 - fz),
    ]
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
    /// precisa (relieves/cráteres) — ver `biblia.md` §8.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collides: Option<bool>,
    /// Huella de colisión multi-celda (A1). `None` = colisión de 1 celda por el
    /// campo `collides`/default de tipo. Ver `CollisionFootprint`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footprint: Option<CollisionFootprint>,
    /// Luz que emite este asset (halo suave). `None` = no ilumina. Ver `Emission`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emission: Option<Emission>,
    /// Capa de dibujo del SLOT entero (no de una pose): decide en qué banda z
    /// se planta el asset en el mundo. `Floor` va DEBAJO del jugador (charcos,
    /// caminos, alfombras, plataformas pintadas en el editor de mundo), `Roof`
    /// va encima de todo y se transparenta cuando el jugador está debajo, y
    /// `Object`/`Wall` es el Y-sort de siempre. `None` = `Object` (compat).
    ///
    /// Existe aparte de `Pose::layer` porque la mayoría de los decor NO tienen
    /// poses, y sin esto no había manera de declarar "esto es piso" — que es lo
    /// que hacía que lo dibujado en el editor de mundo tapara al jugador.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<ArtLayer>,
    /// Que se puede hacer con esto y desde donde. `None` = no es interactuable
    /// (un poste, un muro): no muestra boton al acercarse.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaccion: Option<Interaccion>,
    /// **Borde de 9 partes, en píxeles de la hoja.** Sólo para marcos de
    /// interfaz. `None` = el borde se adivina con la heurística de "una celda de
    /// la grilla" (ver `art2d::ui_frame_node`), que es lo correcto para los
    /// marcos alineados a 32 y lo INCORRECTO para las piezas del pack de UI, que
    /// empiezan en píxeles sueltos y tienen bordes de 2 a 11 px. Lo publica
    /// `scripts/import-ui-kit.py`, medido; no se escribe a ojo (regla 7).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slice: Option<SliceBorder>,
}

/// Los cuatro bordes de un marco de 9 partes, en píxeles de la hoja.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SliceBorder {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
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
            interaccion: None,
            emission: None,
            layer: None,
            slice: None,
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

// ---------------------------------------------------------------------------
// VARIANTES — un objeto lógico, varios materiales/skins
// ---------------------------------------------------------------------------

/// Separador de variante dentro de una clave de sprite: `"floor/piso#madera"`.
/// Elegido porque NUNCA aparece en una clave lógica del juego (`categoria/nombre`)
/// ni en una ruta de archivo, así que la clave de una variante sigue siendo una
/// clave normal del mapa `sprites` → el juego, el loader y el editor la leen sin
/// ninguna rama nueva. Ver `variant_key` / `split_variant`.
pub const VARIANT_SEP: char = '#';

/// Clave de sprite de una variante: `("floor/piso", "madera")` → `"floor/piso#madera"`.
pub fn variant_key(base: &str, variant: &str) -> String {
    format!("{base}{VARIANT_SEP}{variant}")
}

/// Parte una clave en `(objeto base, variante)`. Sin `#` → `(clave, None)`.
pub fn split_variant(key: &str) -> (&str, Option<&str>) {
    match key.split_once(VARIANT_SEP) {
        Some((base, v)) if !v.is_empty() => (base, Some(v)),
        _ => (key, None),
    }
}

/// Una variante declarada de un objeto: `id` es lo que va a la clave del sprite
/// (`madera` → `floor/piso#madera`) y lo que el juego pide en runtime; `label` es
/// el nombre lindo para el editor y para el menú de construcción.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariantDef {
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
}

impl VariantDef {
    /// Nombre a mostrar: el label si lo pusiste, si no el id.
    pub fn display(&self) -> &str {
        if self.label.is_empty() {
            &self.id
        } else {
            &self.label
        }
    }
}

/// Declaración de un objeto CON variantes (piso, pared, techo…). Vive aparte de
/// `sprites` a propósito: así una variante existe (y aparece en el editor y en el
/// juego) desde el momento en que la creás, incluso antes de tener arte asignado.
/// El arte de cada variante es la entrada `"<objeto>#<id>"` de `sprites`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ObjectDef {
    /// Nombre a mostrar del objeto (`"Piso"`). Vacío = usar la clave.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    /// Variantes en el orden en que las querés ver/ofrecer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<VariantDef>,
}

/// El art-manifest del juego.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ArtManifest {
    #[serde(default)]
    pub sprites: BTreeMap<String, ArtValue>,
    /// Objetos con variantes, por clave base (`"floor/piso"`). Ausente en los
    /// manifests viejos → se omite al serializar si está vacío (diff cero).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub objects: BTreeMap<String, ObjectDef>,
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

    /// Devuelve la entrada completa del slot (creándola/normalizándola).
    ///
    /// Cambiar de hoja resetea SÓLO lo que está atado a los píxeles de la hoja
    /// vieja (`grid`/`rect`/`strip`): esas coordenadas no significan nada en la
    /// hoja nueva. Todo lo demás —poses guardadas, huella de colisión, luz,
    /// capa, override de colisión— es del OBJETO, no de la imagen, y sobrevive.
    ///
    /// Antes acá había un `*entry = ArtEntry::new(png)` que borraba la entrada
    /// entera: guardabas la animación "abierta" de una puerta desde otro PNG y
    /// perdías la "cerrada", su huella y su luz de un saque. Ése era el
    /// "no me guarda las dos animaciones".
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
            // Las poses que no traían hoja propia vivían en la hoja VIEJA: se la
            // anclamos explícitamente para que sigan dibujándose igual.
            let old_png = std::mem::replace(&mut entry.png, png);
            let old_grid = entry.grid;
            if let Some(poses) = entry.poses.as_mut() {
                for pose in poses.values_mut() {
                    if pose.png.is_none() {
                        pose.png = Some(old_png.clone());
                        pose.grid = pose.grid.or(old_grid);
                    }
                }
            }
            entry.grid = None;
            entry.rect = None;
            entry.strip = None;
            entry.strip_dir = None;
            entry.anim = None;
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

    /// Tamaño del slot en TILES según su recorte (`rect`/pose/`strip`) — una casa
    /// de 5×4 celdas mide 5×4 tiles. `None` = el slot es un PNG suelto sin recorte
    /// y hay que deducirlo de sus píxeles (el que llama tiene la textura).
    pub fn slot_cells(&self, key: &str) -> Option<(u32, u32)> {
        let ArtValue::Full(e) = self.sprites.get(key)? else { return None };
        e.rect
            .map(|r| (r.w, r.h))
            .or_else(|| {
                e.poses
                    .as_ref()
                    .and_then(|m| {
                        m.get("idle_down").or_else(|| m.get("center")).or_else(|| m.values().next())
                    })
                    .map(|p| (p.rect.w, p.rect.h))
            })
            .or_else(|| e.strip.map(|s| (s.w, s.h)))
            .map(|(w, h)| (w.max(1), h.max(1)))
    }

    /// Celda ANCLA del slot: la que coincide con la celda que apuntás al colocarlo.
    /// La de su huella si la pintó el usuario; si no, el pie/centro de `cells`.
    pub fn anchor(&self, key: &str, cells: (u32, u32)) -> (u32, u32) {
        match self.footprint(key) {
            Some(f) => (f.ax.min(cells.0.saturating_sub(1)), f.az.min(cells.1.saturating_sub(1))),
            None => CollisionFootprint::default_anchor(cells.0, cells.1),
        }
    }

    /// Que se puede hacer con este slot, si el usuario lo declaro en HA Studio.
    pub fn interaccion(&self, key: &str) -> Option<&Interaccion> {
        let (base, _) = split_variant(key);
        // Las variantes heredan del objeto base: `puerta#roja` se abre igual.
        match self.sprites.get(key).or_else(|| self.sprites.get(base)) {
            Some(ArtValue::Full(e)) => e.interaccion.as_ref(),
            _ => None,
        }
    }

    /// Setea/limpia la interaccion del slot (editor).
    pub fn set_interaccion(&mut self, key: &str, i: Option<Interaccion>) {
        let Some(slot) = self.sprites.get_mut(key) else { return };
        if let ArtValue::Path(p) = slot {
            *slot = ArtValue::Full(ArtEntry::new(std::mem::take(p)));
        }
        if let ArtValue::Full(e) = slot {
            e.interaccion = i.filter(|i| !i.verbo.is_empty());
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

    /// Capa de dibujo del slot. Prioridad: la del slot (`ArtEntry::layer`), si no
    /// la de su única pose etiquetada (assets viejos), si no la convención de
    /// namespace ([`namespace_layer`]), si no `None` = `Object`.
    pub fn slot_layer(&self, key: &str) -> Option<ArtLayer> {
        let (base, _) = split_variant(key);
        let Some(ArtValue::Full(e)) = self.sprites.get(key).or_else(|| self.sprites.get(base))
        else {
            return namespace_layer(base);
        };
        if e.layer.is_some() {
            return e.layer;
        }
        // Un slot de una sola pose etiquetada ES esa capa (p.ej. un decal que se
        // recortó como pose "floor"). Con varias poses no se asume nada: ahí la
        // capa es de cada parte y la decide quien las instancia (building.rs).
        if let Some(poses) = e.poses.as_ref().filter(|m| m.len() == 1) {
            if let Some(l) = poses.values().next().and_then(|p| p.layer) {
                return Some(l);
            }
        }
        namespace_layer(base)
    }

    /// Setea/limpia la capa del slot (combo del editor de assets). Normaliza
    /// `Path`→`Full` para poder guardarla.
    pub fn set_slot_layer(&mut self, key: &str, layer: Option<ArtLayer>) {
        let Some(slot) = self.sprites.get_mut(key) else { return };
        if let ArtValue::Path(p) = slot {
            *slot = ArtValue::Full(ArtEntry::new(std::mem::take(p)));
        }
        if let ArtValue::Full(e) = slot {
            e.layer = layer;
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

    // ── variantes ───────────────────────────────────────────────────────────

    /// Variantes DECLARADAS de un objeto (tengan arte o no), en orden.
    pub fn variants(&self, base: &str) -> &[VariantDef] {
        self.objects.get(base).map(|o| o.variants.as_slice()).unwrap_or(&[])
    }

    /// Nombre a mostrar del objeto (label declarado, o la hoja de la clave).
    pub fn object_label<'a>(&'a self, base: &'a str) -> &'a str {
        match self.objects.get(base).map(|o| o.label.as_str()) {
            Some(l) if !l.is_empty() => l,
            _ => base.rsplit('/').next().unwrap_or(base),
        }
    }

    /// Declara (o renombra) una variante. Idempotente: si el `id` ya existe sólo
    /// actualiza el label, así el orden que ves en el editor no salta. Devuelve la
    /// clave de sprite de la variante (`"floor/piso#madera"`).
    pub fn declare_variant(&mut self, base: &str, id: &str, label: &str) -> String {
        let obj = self.objects.entry(base.to_string()).or_default();
        match obj.variants.iter_mut().find(|v| v.id == id) {
            Some(v) => v.label = label.to_string(),
            None => obj.variants.push(VariantDef { id: id.to_string(), label: label.to_string() }),
        }
        variant_key(base, id)
    }

    /// Borra una variante: su declaración Y su arte. Si el objeto queda sin
    /// variantes, se va del mapa `objects` (nada de basura en el JSON).
    pub fn remove_variant(&mut self, base: &str, id: &str) {
        if let Some(obj) = self.objects.get_mut(base) {
            obj.variants.retain(|v| v.id != id);
            if obj.variants.is_empty() && obj.label.is_empty() {
                self.objects.remove(base);
            }
        }
        self.sprites.remove(&variant_key(base, id));
    }

    /// Mueve una variante en la lista (`dir` = -1 arriba / +1 abajo). El orden es
    /// el que verá el jugador en el menú de materiales.
    pub fn move_variant(&mut self, base: &str, id: &str, dir: i32) {
        let Some(obj) = self.objects.get_mut(base) else { return };
        let Some(i) = obj.variants.iter().position(|v| v.id == id) else { return };
        let j = (i as i32 + dir).clamp(0, obj.variants.len() as i32 - 1) as usize;
        if i != j {
            obj.variants.swap(i, j);
        }
    }

    /// Variantes del objeto que YA tienen arte asignado, en el orden declarado.
    /// Es lo que el juego puede dibujar de verdad (una variante declarada sin PNG
    /// no se ofrece: se caería al arte base).
    pub fn variants_with_art(&self, base: &str) -> Vec<&VariantDef> {
        self.variants(base)
            .iter()
            .filter(|v| self.sprites.contains_key(&variant_key(base, &v.id)))
            .collect()
    }

    /// Arte efectivo de `base` para la variante pedida: la entrada de la variante
    /// si existe, si no la del objeto base (fallback: una variante sin arte propio
    /// se ve como el objeto genérico en vez de desaparecer).
    pub fn resolve(&self, base: &str, variant: Option<&str>) -> Option<&ArtValue> {
        variant
            .and_then(|v| self.sprites.get(&variant_key(base, v)))
            .or_else(|| self.sprites.get(base))
    }

    /// Clave de sprite a usar para `(base, variante)` — la de la variante si tiene
    /// arte, si no la base. Devolver la CLAVE (y no el valor) es lo que necesita el
    /// juego: con ella pide sprite, poses, colisión, huella y luz por los mismos
    /// caminos de siempre.
    pub fn resolved_key(&self, base: &str, variant: Option<&str>) -> String {
        match variant {
            Some(v) if self.sprites.contains_key(&variant_key(base, v)) => variant_key(base, v),
            _ => base.to_string(),
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
            step: Vec::new(), res: 1,
        };
        let offs: std::collections::HashSet<(i32, i32)> = fp.solid_offsets().collect();
        assert!(offs.contains(&(-1, -1)) && offs.contains(&(1, 1))); // esquinas del anillo
        assert!(!offs.contains(&(0, 0))); // el centro (ancla) es caminable
        assert_eq!(offs.len(), 8);
        assert!(!fp.is_empty());
    }

    #[test]
    fn subtile_footprint_hugs_the_sprite() {
        // Farol de 1×2 tiles perfilado a 4 sub-celdas: sólo el poste (media celda
        // de ancho, apoyado en el suelo) bloquea — no el metro entero.
        let fp = CollisionFootprint {
            w: 1,
            h: 2,
            ax: 0,
            az: 1,
            solid: vec![(1, 6), (2, 6), (1, 7), (2, 7)],
            step: Vec::new(), res: 4,
        };
        // Vista gruesa: un solo tile, el del pie (offset 0,0 respecto del ancla).
        let offs: Vec<(i32, i32)> = fp.solid_offsets().collect();
        assert_eq!(offs, vec![(0, 0)]);
        // Vista fina: cuatro cajas de 0.25 m en el cuarto inferior-centro del tile.
        let rects: Vec<[f32; 4]> = fp.solid_rects().collect();
        assert_eq!(rects.len(), 4);
        assert_eq!(rects[0], [0.25, 0.5, 0.5, 0.75]);
        assert_eq!(rects[3], [0.5, 0.75, 0.75, 1.0]);
        // El borde del tile queda LIBRE: pasás rozando el farol sin frenarte.
        assert!(rects.iter().all(|r| r[0] >= 0.25 && r[2] <= 0.75));
    }

    #[test]
    fn placement_is_grid_aligned_for_even_widths() {
        // Un asset de 2 tiles de ancho tiene ancla en su columna izquierda: al
        // colocarlo en (10, 5) ocupa (10..12, 5), nunca medio tile corrido.
        assert_eq!(CollisionFootprint::default_anchor(2, 3), (0, 2));
        assert_eq!(placement_min_tile(10, 5, (0, 2)), (10, 3));
        // Ancho impar: la columna del medio cae justo sobre la celda apuntada.
        assert_eq!(CollisionFootprint::default_anchor(3, 4), (1, 3));
        assert_eq!(placement_min_tile(10, 5, (1, 3)), (9, 2));
    }

    #[test]
    fn old_footprints_without_res_still_load() {
        let json = r#"{ "sprites": { "decor/x": { "png": "a.png",
            "footprint": { "w": 1, "h": 1, "ax": 0, "az": 0, "solid": [[0,0]] } } } }"#;
        let m = ArtManifest::from_json(json).unwrap();
        let fp = m.footprint("decor/x").unwrap();
        assert_eq!(fp.res(), 1);
        assert_eq!(fp.solid_rects().collect::<Vec<_>>(), vec![[0.0, 0.0, 1.0, 1.0]]);
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
    fn variants_live_in_one_object_and_fall_back_to_base() {
        // El caso que pidió el usuario: UN objeto "piso" con varias variantes,
        // cada una con SU asset, todo centralizado en el mismo objeto.
        let mut m = ArtManifest::default();
        m.objects.entry("floor/piso".into()).or_default().label = "Piso".into();
        let k_wood = m.declare_variant("floor/piso", "madera", "Madera");
        let k_stone = m.declare_variant("floor/piso", "piedra", "Piedra");
        m.declare_variant("floor/piso", "metal", "Metal"); // declarada, sin arte todavía
        assert_eq!(k_wood, "floor/piso#madera");

        m.set(&k_wood, "art/floor/piso__madera.png".into(), None);
        m.set(&k_stone, "art/floor/piso__piedra.png".into(), None);
        m.set("floor/piso", "art/floor/piso.png".into(), None); // genérico

        assert_eq!(m.variants("floor/piso").len(), 3, "las 3 se declaran");
        assert_eq!(m.variants_with_art("floor/piso").len(), 2, "sólo 2 tienen PNG");
        assert_eq!(m.object_label("floor/piso"), "Piso");

        // Resolución: variante con arte → la suya; sin arte → cae al base.
        assert_eq!(m.resolve("floor/piso", Some("madera")).unwrap().png(), "art/floor/piso__madera.png");
        assert_eq!(m.resolve("floor/piso", Some("metal")).unwrap().png(), "art/floor/piso.png");
        assert_eq!(m.resolved_key("floor/piso", Some("piedra")), "floor/piso#piedra");
        assert_eq!(m.resolved_key("floor/piso", Some("metal")), "floor/piso");
        assert_eq!(m.resolved_key("floor/piso", None), "floor/piso");

        // Una variante es una entrada normal: se le puede recortar, animar y dar
        // colisión/luz como a cualquier slot (misma API, cero ramas nuevas).
        m.set_rect(&k_stone, "art/atlas/pisos.png".into(), (32, 32), CellRect { col: 2, row: 1, w: 1, h: 1 });
        m.set_collides(&k_stone, Some(false));
        assert_eq!(m.get(&k_stone).unwrap().rect().unwrap().col, 2);
        assert_eq!(m.collides(&k_stone), Some(false));

        // Orden (= el que verá el jugador en el menú de materiales).
        m.move_variant("floor/piso", "piedra", -1);
        assert_eq!(m.variants("floor/piso")[0].id, "piedra");

        // Round-trip por JSON.
        let json = m.to_json().unwrap();
        assert_eq!(m, ArtManifest::from_json(&json).unwrap());

        // Borrar la variante se lleva su declaración y su arte.
        m.remove_variant("floor/piso", "piedra");
        assert!(m.get(&k_stone).is_none());
        assert_eq!(m.variants("floor/piso").len(), 2);
    }

    #[test]
    fn variant_keys_split_and_old_manifests_stay_identical() {
        assert_eq!(split_variant("floor/piso#madera"), ("floor/piso", Some("madera")));
        assert_eq!(split_variant("floor/piso"), ("floor/piso", None));
        assert_eq!(split_variant("floor/piso#"), ("floor/piso#", None), "sufijo vacío = sin variante");
        // Un manifest sin objetos no gana NI UN campo en el JSON (retro-compat).
        let mut m = ArtManifest::default();
        m.set("decor/tree", "art/decor/tree.png".into(), None);
        assert!(!m.to_json().unwrap().contains("objects"));
        // Y un manifest viejo sigue cargando (sin `objects`).
        let old = r#"{ "sprites": { "decor/tree": "art/decor/tree.png" } }"#;
        assert!(ArtManifest::from_json(old).unwrap().objects.is_empty());
    }

    #[test]
    fn footprint_roundtrip_and_default_absent() {
        let mut m = ArtManifest::default();
        m.set("decor/mountain", "art/decor/mountain.png".into(), None);
        // Sin huella: el campo no aparece en el JSON (retro-compat).
        assert!(!m.to_json().unwrap().contains("footprint"));
        m.set_footprint(
            "decor/mountain",
            Some(CollisionFootprint {
                w: 2,
                h: 2,
                ax: 0,
                az: 1,
                solid: vec![(0, 1), (1, 1)],
                step: Vec::new(), res: 1,
            }),
        );
        let back = ArtManifest::from_json(&m.to_json().unwrap()).unwrap();
        assert_eq!(m, back);
        assert_eq!(back.footprint("decor/mountain").unwrap().solid.len(), 2);
        // Huella vacía se descarta (equivale a sin huella).
        m.set_footprint("decor/mountain", Some(CollisionFootprint::default()));
        assert!(m.footprint("decor/mountain").is_none());
    }

    #[test]
    fn slot_layer_roundtrip_and_fallbacks() {
        let mut m = ArtManifest::default();
        m.set("decor/charco", "art/decor/charco.png".into(), None);
        // Sin capa: no ensucia el JSON y el juego lo trata como objeto.
        assert!(!m.to_json().unwrap().contains("\"layer\""));
        assert_eq!(m.slot_layer("decor/charco"), None);

        m.set_slot_layer("decor/charco", Some(ArtLayer::Floor));
        let back = ArtManifest::from_json(&m.to_json().unwrap()).unwrap();
        assert_eq!(back.slot_layer("decor/charco"), Some(ArtLayer::Floor));
        // Las variantes heredan la capa del objeto base: `#madera` y `#piedra`
        // son el mismo mueble, no dos capas.
        assert_eq!(back.slot_layer("decor/charco#hielo"), Some(ArtLayer::Floor));

        // Convención de namespace: un piso es piso aunque nadie lo etiquete —
        // incluso si el slot todavía no existe en el manifest.
        assert_eq!(m.slot_layer("floor/wood"), Some(ArtLayer::Floor));
        assert_eq!(m.slot_layer("decor/tree_roble"), None);

        // Un cuarto es una PLANTA vista desde arriba: va al suelo. Y-sorteado
        // tapaba al jugador entero apenas entraba en él.
        assert_eq!(m.slot_layer("room/room_medium"), Some(ArtLayer::Floor));
        // Un techo tiene que poder desvanecerse: nunca en la banda Y-sort.
        assert_eq!(m.slot_layer("roof/teja"), Some(ArtLayer::Roof));
        // Una pared es pared — de ahí sale que frene sin tildar slot por slot.
        assert_eq!(m.slot_layer("wall/piedra"), Some(ArtLayer::Wall));
        // Lo ambiguo NO se adivina: un `agua/*` es un bote o un nenúfar, o sea
        // un objeto que flota, no la superficie del agua.
        assert_eq!(m.slot_layer("agua/bote"), None);
        assert_eq!(m.slot_layer("decor/charco_sin_etiquetar"), None);

        m.set_slot_layer("decor/charco", None);
        assert_eq!(m.slot_layer("decor/charco"), None);
    }

    #[test]
    fn single_tagged_pose_sets_the_slot_layer() {
        // Assets que ya venían etiquetados por pose (antes de que existiera la
        // capa por slot) siguen funcionando sin re-etiquetar nada.
        let mut m = ArtManifest::default();
        m.set("decor/alfombra", "art/decor/alfombra.png".into(), None);
        m.set_pose(
            "decor/alfombra",
            "art/decor/alfombra.png".into(),
            (32, 32),
            "floor",
            Pose { layer: Some(ArtLayer::Floor), ..Pose::still(CellRect { col: 0, row: 0, w: 2, h: 2 }) },
        );
        assert_eq!(m.slot_layer("decor/alfombra"), Some(ArtLayer::Floor));
    }
}
