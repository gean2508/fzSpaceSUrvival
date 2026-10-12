//! Top-Down 3/4 world model (LimeZu Modern) — successor to `flat2d.rs`.
//!
//! Migration to a Stardew/top-down view. The walkable ground is the XZ plane
//! (the same plane the solar system lives on): `body.x` = East(+)/West(−),
//! `body.z` = North(−)/South(+), `body.y` = height (0 on foot). Screen render
//! is `(x, -z)` — identical to `orbital2d.rs`, so surface and space share a
//! projection and launching is a zoom-out, not a coordinate change.
//!
//! Depth is a discrete **Z-Level** (`z_level: i32`): 0 = surface, negative =
//! underground mine floors, each its own deterministic top-down grid. Space is
//! NOT a z-level — it is the orbital mode. Ver `biblia.md` §4.

use crate::celestial_bodies::{body_atmo_top, body_terrain_seed, CelestialBody};
use crate::constants::{FRICTION, SPRINT_MULT, STAMINA_SPRINT_DRAIN};
use crate::input::InputState;
use crate::movement::MovementBody;
use crate::types::{MOV_IDLE, MOV_RUNNING, MOV_SPRINTING, MOV_SWIMMING};

pub const TILE_M: f32 = 1.0;
pub const PLAYER_RADIUS: f32 = 0.34;
/// Deepest mine floor reachable (z_level).
pub const MIN_Z_LEVEL: i32 = -8;

// ---------------------------------------------------------------------------
// Tiles
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Tile {
    // Surface (z=0)
    Grass,
    Dirt,
    Sand,
    /// Líquido de POCO FONDO: no corta el paso, **se nada** (ver [`Tile::swim`]).
    /// Es el nivel −1 del relieve: la orilla, los ríos, los lagos de pescar y la
    /// pileta del patio. El color sale del cuerpo (`relief::liquid_color`), así que
    /// el mismo tile es agua en la Tierra, metano en Titán y lava en Ío.
    Water,
    /// Líquido HONDO (nivel −2 y más abajo): bloquea. Es lo que hace que el océano
    /// siga siendo un límite y que cruzar de continente pida puente, vuelo o barco.
    WaterDeep,
    Regolith,
    /// Hielo: superficie helada CAMINABLE — casquetes polares, cumbres nevadas
    /// y los mundos de hielo (Europa). Resbala a la vista, no en el paso.
    Ice,
    /// Basalto: llanura oscura de roca volcánica. CAMINABLE — es suelo, no
    /// muro. Es lo que dibuja los mares de la Luna y las planicies oscuras de
    /// Marte, que son justo los rasgos que hacen reconocible a cada cuerpo
    /// desde el espacio.
    Basalt,
    RockRed,
    /// **Cara de acantilado**: el borde de una terraza contra la de abajo. Sólida
    /// y alta — no se trepa, se rodea o se sube por una [`Tile::Slope`]. Es lo que
    /// convierte una cordillera en una barrera de verdad. Ver `crate::relief`.
    Cliff,
    /// **Rampa**: el mismo borde de terraza, pero abierto. Se sube caminando (y
    /// conduciendo: las carreteras siempre cruzan por acá).
    Slope,
    /// Cerca de parcela: corta el paso y se levanta del suelo, pero es delgada —
    /// el arte la dibuja como listones, no como muro.
    Fence,
    /// **Roca desnuda de superficie**: la cumbre y la ladera alta. CAMINABLE.
    ///
    /// Es distinto de [`Tile::Stone`], que es la roca MACIZA del subsuelo y
    /// bloquea. Confundirlas convertía cada montaña en una mole impasable de
    /// kilómetros —medido: 29 % del terreno alrededor del origen— y eso anula el
    /// sistema de terrazas entero: si la montaña ya es un muro, sobra el
    /// acantilado. Lo que corta el paso en altura es [`Tile::Cliff`]; la cumbre se
    /// pisa, se acampa en ella y se mira el mundo desde arriba.
    Rock,
    /// **Pavimento**: acera, vereda, losa de plaza, borde de pileta, patio.
    /// CAMINABLE.
    ///
    /// Existe porque hasta ahora la obra civil pintaba las aceras con
    /// [`Tile::Stone`], que es la ROCA del subsuelo y por tanto sólida: las
    /// veredas de todas las ciudades del juego eran muros infranqueables. Nadie lo
    /// notó porque a pie se camina por la calzada, pero cualquier cosa que
    /// intentara cruzar de la calle al portal chocaba con una pared invisible.
    Pavement,
    // Underground (z<0)
    Stone,
    RockDark,
    Bedrock,
    CaveFloor,
    // Player-placed / shared
    WoodFloor,
    Wall,
    // Build pieces (construction system) — placed only, never generated
    // (except Door/WoodWall/WoodFloor, reused by generated settlements).
    WoodWall,
    StoneBrick,
    Door,
    /// Window: blocks walking like a wall, but art shows through (see `window/basic`).
    Window,
    /// Stairs placed by the player: stepping on them swaps `tiles.z` one level
    /// down/up (building basements / upper floors — see `layer_transition`).
    StairsDown,
    StairsUp,
    Air,
}

impl Tile {
    /// Blocks walking (walls, deep liquid, solid rock — cave floors are walkable).
    /// Build walls block; wood floors and doors are walkable.
    ///
    /// **`Water` ya NO es sólido**: el bajío se nada (ver [`Tile::swim`]). Lo que
    /// bloquea es [`Tile::WaterDeep`]. Si estás leyendo esto porque algo dejó de
    /// frenar al jugador en el agua: el sitio a mirar es quién quería decir "no se
    /// camina" (sigue igual) y quién quería decir "no es suelo edificable" (ésos
    /// tuvieron que pasar a comprobar `!= Water` explícitamente).
    pub fn solid(self) -> bool {
        matches!(
            self,
            Tile::WaterDeep
                | Tile::Wall
                | Tile::Bedrock
                | Tile::Stone
                | Tile::RockDark
                | Tile::RockRed
                | Tile::Cliff
                | Tile::Fence
                | Tile::WoodWall
                | Tile::StoneBrick
                | Tile::Window
        )
    }

    /// Líquido de poco fondo: **se nada**. No corta el paso, pero el que lo cruza
    /// va más lento y con la pose de natación (`swim_*`) en vez de la de caminar.
    pub fn swim(self) -> bool {
        matches!(self, Tile::Water)
    }

    /// ¿Es suelo firme sobre el que se puede FUNDAR algo (una casa, una máquina,
    /// una parcela)? Distinto de `!solid()`: el bajío se camina pero no se
    /// construye a secas, y una rampa es un paso, no una parcela.
    ///
    /// Existe porque cuando `Water` dejó de ser sólido, todos los sitios que
    /// usaban `!solid()` para decir "acá se puede edificar" empezaron a plantar
    /// casas dentro del lago.
    pub fn buildable_ground(self) -> bool {
        !self.solid() && !self.swim() && !matches!(self, Tile::Slope | Tile::Air)
    }

    /// ¿Este material se levanta del suelo (ocluye lo que tiene detrás)?
    ///
    /// **No es lo mismo que [`Tile::solid`]**, y confundirlos es lo que hacía
    /// que el AGUA tapara al jugador: el agua bloquea el paso (es `solid`) pero
    /// es PLANA, así que debe dibujarse en la banda de suelo, nunca Y-sorted.
    /// `solid` decide si podés caminar; `upright` decide si te tapa.
    pub fn upright(self) -> bool {
        matches!(
            self,
            Tile::Wall
                | Tile::Bedrock
                | Tile::Stone
                | Tile::RockDark
                | Tile::RockRed
                // El acantilado y la cerca se LEVANTAN del suelo: te tapan lo que
                // tienen detrás, que es justamente cómo se lee un desnivel en 3/4.
                // El agua (bajío u hondo) NO: es plana, y confundir "bloquea" con
                // "es alto" es de donde salía el bug del agua tapando al jugador.
                | Tile::Cliff
                | Tile::Fence
                | Tile::WoodWall
                | Tile::StoneBrick
                | Tile::Window
                | Tile::Door
        )
    }

    /// Diggable (removable) → yields this item id, if any. Build pieces refund
    /// their material so deconstruction is just "dig the piece".
    pub fn dug_item(self) -> Option<&'static str> {
        match self {
            Tile::Grass | Tile::Dirt => Some("dirt"),
            Tile::Sand => Some("sand"),
            Tile::Regolith => Some("regolith"),
            Tile::Basalt => Some("stone"),
            Tile::Ice => Some("stone"),
            Tile::Stone | Tile::RockRed | Tile::Pavement | Tile::Rock => Some("stone"),
            Tile::RockDark => Some("stone"),
            // Picar un acantilado abre el paso: es la alternativa "a lo Minecraft"
            // a buscar la rampa, y cuesta el tiempo de picar toda la pared.
            Tile::Cliff => Some("stone"),
            Tile::Slope => Some("dirt"),
            Tile::Fence => Some("wood"),
            Tile::WoodFloor | Tile::Wall | Tile::WoodWall | Tile::Door => Some("wood"),
            Tile::Window | Tile::StairsDown | Tile::StairsUp => Some("wood"),
            Tile::StoneBrick => Some("stone"),
            _ => None,
        }
    }

    /// **Slot de arte** de la celda: la clave lógica que el juego le pide a
    /// `ArtLibrary` (y que vos reskineás en el editor de assets).
    ///
    /// Vive acá, al lado del tile, y NO en el cliente: es la única forma de que
    /// el editor pueda auditar que toda pieza de construcción tiene su slot con
    /// nombre humano (`ha_assets::game_slots`). Si un tile estrena arte, la
    /// clave se agrega acá y el test de auditoría exige el slot correspondiente.
    ///
    /// `None` = el tile se dibuja SIEMPRE por color ([`Tile::color`]): el
    /// subsuelo, el aire, la roca del fondo de cueva. Y aunque devuelva `Some`,
    /// si el slot no tiene PNG asignado el juego cae al color — una puerta sin
    /// arte es un cuadrado café, nunca un hueco.
    pub fn art_key(self) -> Option<&'static str> {
        match self {
            Tile::Water => Some("tiles/water"),
            Tile::WaterDeep => Some("tiles/water_deep"),
            Tile::Grass => Some("tiles/grass"),
            Tile::Sand => Some("tiles/sand"),
            Tile::Regolith => Some("tiles/regolith"),
            Tile::Basalt => Some("tiles/basalt"),
            Tile::Ice => Some("tiles/ice"),
            Tile::Stone => Some("tiles/stone"),
            Tile::Dirt => Some("tiles/dirt"),
            // Relieve. La clave genérica es el fallback; `cliff_art_key` elige antes
            // la variante por bioma (`cliff/arena` en un desierto, `cliff/nieve` en la
            // cumbre) para que la misma pared se lea distinta según dónde esté.
            Tile::Cliff => Some("cliff/roca"),
            Tile::Slope => Some("slope/roca"),
            Tile::Fence => Some("fence/madera"),
            Tile::Pavement => Some("floor/vereda"),
            Tile::Rock => Some("tiles/roca"),
            // Piezas de construcción (autotile de Room_Builder). El muro legacy
            // (`Wall`) comparte el arte de madera.
            Tile::WoodFloor => Some("floor/wood"),
            Tile::WoodWall | Tile::Wall => Some("wall/wood"),
            Tile::StoneBrick => Some("wall/stone"),
            Tile::Door => Some("door/wood"),
            Tile::Window => Some("window/basic"),
            Tile::StairsDown => Some("stairs/down"),
            Tile::StairsUp => Some("stairs/up"),
            _ => None,
        }
    }

    /// Flat sRGB colour (procedural fallback until LimeZu tilesets land).
    pub fn color(self) -> [f32; 3] {
        match self {
            Tile::Grass => [0.36, 0.62, 0.29],
            Tile::Dirt => [0.47, 0.33, 0.22],
            Tile::Sand => [0.78, 0.70, 0.45],
            // Colores GENÉRICOS del líquido. El color real lo pone el cuerpo
            // (`relief::liquid_color`): el mismo tile es agua, metano o lava según
            // dónde estés. Estos son el fallback de quien no sabe en qué cuerpo
            // está (íconos de inventario, tarjetas de la mochila).
            Tile::Water => [0.35, 0.62, 0.80],
            Tile::WaterDeep => [0.16, 0.34, 0.58],
            Tile::Cliff => [0.38, 0.33, 0.29],
            Tile::Slope => [0.52, 0.42, 0.30],
            Tile::Fence => [0.52, 0.38, 0.22],
            Tile::Pavement => [0.60, 0.60, 0.62],
            Tile::Rock => [0.52, 0.50, 0.47],
            Tile::Regolith => [0.62, 0.62, 0.64],
            Tile::Basalt => [0.29, 0.28, 0.31],
            Tile::Ice => [0.82, 0.88, 0.93],
            Tile::RockRed => [0.60, 0.34, 0.24],
            Tile::Stone => [0.46, 0.47, 0.50],
            Tile::RockDark => [0.30, 0.30, 0.34],
            Tile::Bedrock => [0.16, 0.16, 0.18],
            Tile::CaveFloor => [0.24, 0.22, 0.26],
            Tile::WoodFloor => [0.58, 0.44, 0.26],
            Tile::Wall => [0.38, 0.34, 0.30],
            Tile::WoodWall => [0.45, 0.31, 0.17],
            Tile::StoneBrick => [0.55, 0.55, 0.58],
            Tile::Door => [0.30, 0.20, 0.10],
            Tile::Window => [0.55, 0.65, 0.75],
            Tile::StairsDown => [0.32, 0.24, 0.16],
            Tile::StairsUp => [0.62, 0.50, 0.32],
            Tile::Air => [0.0, 0.0, 0.0],
        }
    }
}

// ---------------------------------------------------------------------------
// Construction system — build pieces catalogue (deterministic, shared)
// ---------------------------------------------------------------------------

/// A placeable structural piece. Placed on an empty surface cell via the build
/// mode (hammer + `BuildModeActive`), snapping to the aimed cell. Removing a
/// piece is just digging it — `Tile::dug_item` refunds the material.
#[derive(Clone, Copy, Debug)]
pub struct BuildPiece {
    pub id: &'static str,
    pub label: &'static str,
    /// The tile this piece places.
    pub tile: Tile,
    /// Resource consumed to place one (item id + quantity).
    pub cost_item: &'static str,
    pub cost_qty: u32,
}

/// Ordered palette of build pieces (index = palette slot).
///
/// **Sólo piezas de CELDA**: pisos y escaleras. Paredes, puertas y ventanas
/// dejaron de ser celdas — viven en el borde entre celdas (`build_grid::
/// EDGE_PIECES`, estilo Last Day on Earth). Los tiles `WoodWall`/`Door`/… siguen
/// existiendo para los asentamientos generados y las partidas viejas.
pub static BUILD_PIECES: &[BuildPiece] = &[
    BuildPiece {
        id: "build_wood_floor",
        label: "Piso madera",
        tile: Tile::WoodFloor,
        cost_item: "wood",
        cost_qty: 1,
    },
    BuildPiece {
        id: "build_stone_floor",
        label: "Piso piedra",
        tile: Tile::StoneBrick,
        cost_item: "stone",
        cost_qty: 1,
    },
    BuildPiece {
        id: "build_pavement",
        label: "Vereda",
        tile: Tile::Pavement,
        cost_item: "stone",
        cost_qty: 1,
    },
    BuildPiece {
        id: "build_stairs_down",
        label: "Gradas abajo",
        tile: Tile::StairsDown,
        cost_item: "wood",
        cost_qty: 4,
    },
    BuildPiece {
        id: "build_stairs_up",
        label: "Gradas arriba",
        tile: Tile::StairsUp,
        cost_item: "wood",
        cost_qty: 4,
    },
];

/// Number of pieces in the build palette.
pub fn build_piece_count() -> usize {
    BUILD_PIECES.len()
}

/// Palette piece by index (clamped-safe: returns None if out of range).
pub fn build_piece(idx: usize) -> Option<&'static BuildPiece> {
    BUILD_PIECES.get(idx)
}

// ---------------------------------------------------------------------------
// Prefabs — multi-tile stamps (rooms/houses placed in ONE hold). Body-agnostic:
// they place wherever building is allowed (Earth, Moon, Mars, stations). Art is
// optional (`room/*` slots + autotile `floor/*`/`wall/*`); logic is tiles only.
// ---------------------------------------------------------------------------

/// A rectangular room prefab: perimeter = wall, interior = floor, one door on
/// the south wall. `interior_walls` adds inner partitions (offsets rel. to the
/// prefab min corner); an inner `Door` splits rooms without leaving the house.
#[derive(Clone, Copy, Debug)]
pub struct Prefab {
    pub id: &'static str,
    pub label: &'static str,
    /// Footprint in tiles (w along +x, h along +z).
    pub w: i64,
    pub h: i64,
    /// Door offset on the perimeter (rel. to min corner).
    pub door: (i64, i64),
    /// Inner partition cells (rel. offsets) stamped as `WoodWall`.
    pub interior_walls: &'static [(i64, i64)],
    /// Inner door cells (rel. offsets) stamped as `Door`.
    pub interior_doors: &'static [(i64, i64)],
    /// Material charged per wall cell (floors cost 1 of the same item).
    pub cost_item: &'static str,
}

/// Curated presets — small enough to read on a phone screen. `casa_1` derives
/// from the LimeZu `Home_Designs` layout: two rooms split by a partition.
pub static PREFABS: &[Prefab] = &[
    Prefab {
        id: "room_small",
        label: "Cuarto 5x4",
        w: 5,
        h: 4,
        door: (2, 3),
        interior_walls: &[],
        interior_doors: &[],
        cost_item: "wood",
    },
    Prefab {
        id: "room_medium",
        label: "Cuarto 7x6",
        w: 7,
        h: 6,
        door: (3, 5),
        interior_walls: &[],
        interior_doors: &[],
        cost_item: "wood",
    },
    Prefab {
        id: "room_large",
        label: "Salon 9x8",
        w: 9,
        h: 8,
        door: (4, 7),
        interior_walls: &[],
        interior_doors: &[],
        cost_item: "wood",
    },
    Prefab {
        id: "casa_1",
        label: "Casa 9x7",
        w: 9,
        h: 7,
        door: (4, 6),
        // Partition wall at x=5 splitting living room / bedroom…
        interior_walls: &[(5, 1), (5, 2), (5, 4), (5, 5)],
        // …with an inner doorway at (5,3).
        interior_doors: &[(5, 3)],
        cost_item: "wood",
    },
];

pub fn prefab_count() -> usize {
    PREFABS.len()
}

pub fn prefab(idx: usize) -> Option<&'static Prefab> {
    PREFABS.get(idx)
}

/// Tile stamped at offset `(dx, dz)` of the prefab, or `None` outside the
/// footprint. Perimeter → wall, door offsets → `Door`, interior → floor.
pub fn prefab_cell(p: &Prefab, dx: i64, dz: i64) -> Option<Tile> {
    if dx < 0 || dz < 0 || dx >= p.w || dz >= p.h {
        return None;
    }
    if p.door == (dx, dz) || p.interior_doors.contains(&(dx, dz)) {
        return Some(Tile::Door);
    }
    if p.interior_walls.contains(&(dx, dz)) {
        return Some(Tile::WoodWall);
    }
    let perimeter = dx == 0 || dz == 0 || dx == p.w - 1 || dz == p.h - 1;
    Some(if perimeter { Tile::WoodWall } else { Tile::WoodFloor })
}

/// Total material cost of stamping the prefab: 2 per wall cell, 3 per door,
/// 1 per floor cell (mirrors the single-piece prices).
pub fn prefab_cost(p: &Prefab) -> u32 {
    let mut qty = 0;
    for dz in 0..p.h {
        for dx in 0..p.w {
            qty += match prefab_cell(p, dx, dz) {
                Some(Tile::WoodWall) => 2,
                Some(Tile::Door) => 3,
                Some(Tile::WoodFloor) => 1,
                _ => 0,
            };
        }
    }
    qty
}

/// Un "asset grande" colocable: un sprite ÚNICO (p.ej. 4×4) con footprint de
/// colisión. A diferencia de un prefab (que estampa tiles), esto coloca UNA
/// estructura-sprite que se registra en `BuiltStructures` (juego); su colisión
/// sale de la huella del art (manifest A1) o, si no hay huella pintada, de la
/// base completa `w×h`. El jugador camina ALREDEDOR (Y-sort), no adentro.
/// Agnóstico al cuerpo. (`biblia.md` §5.)
#[derive(Clone, Copy, Debug)]
pub struct LargeAsset {
    pub id: &'static str,
    pub label: &'static str,
    /// Slot de arte lógico (el usuario le asigna la textura en el editor). Sin
    /// arte, cae a un quad de color (como el resto del decor).
    pub art: &'static str,
    /// Sección del menú CONSTRUIR donde aparece. Con 120+ colocables la lista
    /// plana dejó de ser navegable en un celular — ver `placeables::PlaceCat`.
    pub cat: crate::placeables::PlaceCat,
    /// Footprint en tiles (`w` en +x, `h` en +z) — tamaño visual y base de
    /// colisión por defecto.
    pub w: i64,
    pub h: i64,
    pub cost_item: &'static str,
    pub cost_qty: u32,
}

/// Catálogo de assets grandes colocables. **Vive en `placeables.rs`**: acá quedan
/// sólo los accesos que usa el motor.
///
/// Eran tres entradas escritas a mano (peñasco, estatua, monumento) mientras el
/// catálogo de arte tenía 150 objetos con nombre. Mover la tabla no cambió una
/// línea de la colocación — es la MISMA `LargeAsset` por el MISMO camino.
pub use crate::placeables::PLACEABLES as LARGE_ASSETS;

pub fn large_asset_count() -> usize {
    LARGE_ASSETS.len()
}

pub fn large_asset(idx: usize) -> Option<&'static LargeAsset> {
    LARGE_ASSETS.get(idx)
}

// ---------------------------------------------------------------------------
// Deterministic settlements — auto-generated houses (per-body gated table).
// Building MECHANICS are body-agnostic; only this auto-generation is gated.
// ---------------------------------------------------------------------------

/// Una posible aldea por celda de SITE_PERIOD×SITE_PERIOD tiles.
///
/// ESCALA GTA-V (29 jul 2026): con un mundo de 60 km de lado, una aldea cada
/// 48 m era una calle continua de casas. A 1.8 km salen ~1 100 asentamientos
/// repartidos por el planeta — se cruzan volando cada tanto, que es lo que
/// hace que el mapa se sienta habitado en vez de saturado.
pub use crate::geography::SITE_PERIOD;

/// Which bodies auto-generate settlements (easy to extend: add a match arm).
pub fn body_generates_buildings(body: CelestialBody) -> bool {
    matches!(body, CelestialBody::Earth)
}

pub use crate::geography::{building_at, building_tile, BuildingSite};

/// Compat: el "sitio" de una celda de asentamiento es ahora el primer edificio
/// del núcleo que la ocupa. La generación de verdad vive en `crate::geography`
/// (manzanas dentro de la trama urbana) — esto sólo mantiene en pie a los
/// llamadores antiguos.
pub fn building_site_in(body: CelestialBody, sx: i64, sz: i64) -> Option<BuildingSite> {
    let s = crate::geography::settlement_in(body, sx, sz)?;
    // El primer edificio que aparece barriendo en anillos desde el centro del
    // núcleo. Antes se pedía "el edificio de la manzana (0,0)", pero con parcelas
    // de tamaño variable la manzana central bien puede ser una plaza o el patio de
    // un hospital: buscar el primero que haya es lo que esta función siempre quiso
    // decir ("dame UN edificio de este núcleo, para plantarle props alrededor").
    for r in 0..24i64 {
        for dz in -r..=r {
            for dx in -r..=r {
                if r > 0 && dx.abs() != r && dz.abs() != r {
                    continue;
                }
                let (tx, tz) = (s.center.0 + dx * 9, s.center.1 + dz * 9);
                if let Some(b) = crate::geography::building_at(body, tx, tz) {
                    return Some(b);
                }
            }
        }
    }
    None
}

/// El tile de edificio generado en esta celda, si lo hay.
pub fn building_cell(body: CelestialBody, tx: i64, tz: i64) -> Option<Tile> {
    crate::geography::building_tile(body, tx, tz)
}

// ---------------------------------------------------------------------------
// Deterministic hash noise (2D)
// ---------------------------------------------------------------------------

fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

fn hash2(seed: u32, a: i64, b: i64) -> u32 {
    let a = (a as i32 as u32).wrapping_mul(0x9e37_79b9);
    let b = (b as i32 as u32).wrapping_mul(0x85eb_ca6b);
    hash_u32(seed ^ a ^ hash_u32(b))
}

fn hash01(seed: u32, a: i64, b: i64) -> f32 {
    (hash2(seed, a, b) & 0x00ff_ffff) as f32 / 16_777_215.0
}

/// 2D value noise with smooth interpolation, lattice every `period` tiles.
fn value_noise_2d(seed: u32, tx: i64, tz: i64, period: i64) -> f32 {
    let period = period.max(2);
    let cx = tx.div_euclid(period);
    let cz = tz.div_euclid(period);
    let fx = (tx.rem_euclid(period)) as f32 / period as f32;
    let fz = (tz.rem_euclid(period)) as f32 / period as f32;
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sz = fz * fz * (3.0 - 2.0 * fz);
    let n00 = hash01(seed, cx, cz);
    let n10 = hash01(seed, cx + 1, cz);
    let n01 = hash01(seed, cx, cz + 1);
    let n11 = hash01(seed, cx + 1, cz + 1);
    let a = n00 + (n10 - n00) * sx;
    let b = n01 + (n11 - n01) * sx;
    a + (b - a) * sz
}

/// Fractal (2-octave) 2D noise in 0..1.
fn fbm2(seed: u32, tx: i64, tz: i64, period: i64) -> f32 {
    let a = value_noise_2d(seed, tx, tz, period);
    let b = value_noise_2d(seed ^ 0x51ed_2b37, tx, tz, (period / 3).max(2));
    (a * 0.68 + b * 0.32).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// Terrain generation per (body, z_level)
// ---------------------------------------------------------------------------

/// El tile natural de una celda de superficie. La geografía (relieve, clima,
/// biomas, ríos) vive en `crate::geography` — ÚNICA fuente de verdad, compartida
/// con la decoración y con el mapa satelital del cliente.
fn surface_tile(body: CelestialBody, tx: i64, tz: i64) -> Tile {
    crate::geography::natural_tile(body, tx, tz)
}

/// True where an underground floor has an open (walkable) cave cell.
fn cave_open(body: CelestialBody, z: i32, tx: i64, tz: i64) -> bool {
    let seed = body_terrain_seed(body) ^ 0x0cae_0000u32 ^ (z as u32).wrapping_mul(0x9e37);
    // Deeper floors are more open (worm-like tunnels via thresholded noise):
    // lower threshold ⇒ more cells pass ⇒ more open cave.
    let depth = (-z as f32) / (-MIN_Z_LEVEL as f32);
    let threshold = 0.52 - depth * 0.12;
    fbm2(seed, tx, tz, 9) > threshold
}


/// Fuera del cuerpo: la celda cae más allá del radio caminable. La orilla se
/// ondula un poco con ruido para que la costa no sea un círculo perfecto.
pub fn beyond_world_edge(body: CelestialBody, tx: i64, tz: i64) -> bool {
    let r = crate::celestial_bodies::body_walk_radius(body);
    if r <= 0.0 {
        return false;
    }
    let d = ((tx * tx + tz * tz) as f64).sqrt() as f32 * TILE_M;
    let wobble = (fbm2(body_terrain_seed(body) ^ 0xC0A5_7000, tx / 24, tz / 24, 3) - 0.5) * r * 0.03;
    d > r * 0.985 + wobble
}

/// The natural (un-edited) tile at a grid cell of a given z_level.
pub fn topdown_tile(body: CelestialBody, z: i32, tx: i64, tz: i64) -> Tile {
    // Portals + a 1-ring around them are always open so you never arrive stuck.
    let shafts = shafts_around(body, z, tx, tz);
    if portal_from(&shafts, tx, tz).is_some() || portal_ring(&shafts, tx, tz) {
        return if z == 0 { Tile::Dirt } else { Tile::CaveFloor };
    }
    if z == 0 {
        // MUNDO FINITO (29 jul 2026): más allá del radio caminable del cuerpo no
        // hay más mundo. El mapa es procedural pero ACOTADO — 60 km de punta a
        // punta en la Tierra —, así que el disco que ves desde el espacio es
        // literalmente todo el terreno que existe, y el borde es agua (o roca en
        // los cuerpos sin aire) en vez de una llanura infinita.
        if beyond_world_edge(body, tx, tz) {
            return if body_atmo_top(body) > 0.0 { Tile::Water } else { Tile::RockDark };
        }
        // OBRA CIVIL primero, y dentro de ella **la CALZADA es sagrada**: gana
        // sobre cualquier cosa edificada.
        //
        // Antes los edificios iban antes que la obra civil, y bastaba con que la
        // geometría de una parcela se pasara un tile para dejar un muro cruzado en
        // mitad de la avenida. Discutir cada caso de redondeo es una batalla
        // interminable; declarar que la calzada no se pisa la termina de una vez:
        // por muy mal que salga una parcela, la carretera sigue siendo carretera y
        // el mapa sigue siendo transitable. Lo cazó `no_road_is_ever_blocked_by_a_cliff`.
        //
        // La ACERA (`Pavement`) no gana: va por debajo, porque una acera es el
        // suelo que rodea al edificio y no tiene sentido que le coma el portal.
        let civil = crate::geography::civil_at(body, tx, tz);
        if let Some(c @ (crate::geography::Civil::Road | crate::geography::Civil::Bridge)) = civil {
            return crate::geography::civil_tile(c);
        }
        // Generated settlements override the natural ground (Earth-only table;
        // the tiles behave like any other — collide, dig, persist edits).
        if let Some(t) = building_cell(body, tx, tz) {
            return t;
        }
        if let Some(c) = civil {
            return crate::geography::civil_tile(c);
        }
        return surface_tile(body, tx, tz);
    }
    if z > 0 {
        // **Los pisos altos son VACÍO, no roca.**
        //
        // Antes esto devolvía `Bedrock`, que es sólido: `place_tile` rechaza toda
        // celda sólida y `break_tile` no pica bedrock, así que arriba no se podía
        // ni construir ni cavar. El único z>0 que existía era la sala 7×7 que
        // tallaba `carve_stair_room` al llegar por la grada — o sea que "poner una
        // escalera y armar el segundo piso" (Project Zomboid) era imposible: subías
        // a un cuarto fijo del que no se podía salir ni agrandar.
        //
        // `Air` no es sólido, así que la celda admite piso, y a la vez no es suelo
        // (`buildable_ground` la excluye): el que camina sobre vacío se CAE al piso
        // de abajo (`world2d::fall_through_void`). Ésa es la regla entera de la
        // verticalidad, y es la misma que hace que un balcón se sienta un balcón.
        return Tile::Air;
    }
    if z <= MIN_Z_LEVEL {
        return Tile::Bedrock;
    }
    if cave_open(body, z, tx, tz) {
        Tile::CaveFloor
    } else {
        match body {
            CelestialBody::Moon => Tile::RockDark,
            _ => Tile::Stone,
        }
    }
}

// ---------------------------------------------------------------------------
// Z-Level portals (mine entrance / ladders) — deterministic vertical shafts
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Portal {
    /// Hole down to floor `z-1`.
    Descend,
    /// Ladder up to floor `z+1`.
    Ascend,
}

/// Anchor cell of the surface mine entrance (findable, near the origin/pad).
/// Lado de la celda de sistema de cuevas, en tiles. Una boca cada ~900 m donde
/// el terreno lo permite: en un mundo de 60 km te cruzás con una cada tanto
/// caminando, igual que en Minecraft, en vez de tener UNA sola mina junto al
/// origen (que a esta escala es no tener ninguna).
pub const CAVE_PERIOD: i64 = 900;

/// Boca de cueva de una celda de cuevas, si el terreno la admite: un agujero
/// visible en el suelo por el que se BAJA andando — sin prompt, sin pantalla de
/// carga, sin cambio de escena. Sólo se abre donde hay galería debajo y donde
/// el suelo es pisable (nada de bocas en mitad del océano).
pub fn cave_mouth_in(body: CelestialBody, rx: i64, rz: i64) -> Option<(i64, i64)> {
    let seed = body_terrain_seed(body) ^ 0x0cae_5000;
    let h = hash2(seed, rx, rz);
    // ~2 de cada 3 celdas tienen boca; el resto son macizos sin entrada.
    if h % 3 == 0 {
        return None;
    }
    let margin = 24;
    let span = (CAVE_PERIOD - margin * 2).max(1);
    let tx = rx * CAVE_PERIOD + margin + ((h >> 7) as i64).rem_euclid(span);
    let tz = rz * CAVE_PERIOD + margin + ((h >> 17) as i64).rem_euclid(span);
    // Tierra firme arriba y galería abierta abajo: si no, la boca sería un
    // agujero a roca maciza o al fondo del mar. Prueba CONTINENTAL barata — esto
    // se consulta para cuatro celdas de cada tile del mundo.
    // El nivel del mar sale del CUERPO, no de un 0.46 hardcodeado: con la
    // plantilla de mundo cada cuerpo tiene el suyo (Marte 0.26, Titán 0.495) y
    // aquel número fijo dejaba planetas enteros sin una sola boca de cueva.
    let sea = crate::relief::sea_level(body);
    let h = crate::geography::continental_at(body, tx, tz);
    if h < sea + 0.02 || h > sea + 0.34 {
        return None;
    }
    if !cave_open(body, -1, tx, tz) {
        return None;
    }
    Some((tx, tz))
}

/// El pozo de bajada del piso `z` para la celda de cuevas `(rx, rz)`. Cada piso
/// desplaza el pozo unos tiles, así bajar es recorrer galería, no caer por un
/// tubo recto.
fn shaft_cell(body: CelestialBody, z: i32, rx: i64, rz: i64) -> Option<(i64, i64)> {
    let mut cell = cave_mouth_in(body, rx, rz)?;
    let seed = body_terrain_seed(body) ^ 0x5afe_0001;
    let mut f = 0;
    while f > z {
        f -= 1;
        let h = hash2(seed, rx * 31 + f as i64, rz * 17 + 77);
        cell = (cell.0 + (h % 13) as i64 - 6, cell.1 + ((h >> 8) % 13) as i64 - 6);
    }
    Some(cell)
}

/// La celda de cuevas a la que pertenece un tile (y sus vecinas, porque los
/// pozos se desplazan y pueden cruzar el borde).
fn cave_regions(tx: i64, tz: i64) -> [(i64, i64); 4] {
    let (rx, rz) = (tx.div_euclid(CAVE_PERIOD), tz.div_euclid(CAVE_PERIOD));
    [(rx, rz), (rx - 1, rz), (rx, rz - 1), (rx - 1, rz - 1)]
}

/// El pozo de bajada del piso `z` más cercano al origen — compatibilidad con
/// código viejo que pedía "la mina" en singular.
pub fn descend_portal(body: CelestialBody, z: i32) -> (i64, i64) {
    // Busca en anillos la región con pozo MÁS CERCANA al origen.
    //
    // El fallback fijo que había —`unwrap_or((6, 4))`— **mentía**: si la región
    // (0,0) no tenía boca devolvía una celda que no es ningún portal, así que
    // quien confiara en "la mina" caminaba hasta un sitio donde no se baja. No se
    // notaba porque con la geografía vieja la región (0,0) casi siempre calificaba;
    // en cuanto la plantilla de mundo puso el origen en otro terreno, se cayó.
    for ring in 0..16i64 {
        for rz in -ring..=ring {
            for rx in -ring..=ring {
                // Sólo el perímetro del anillo: el interior ya se miró.
                if ring > 0 && rx.abs() != ring && rz.abs() != ring {
                    continue;
                }
                if let Some(c) = shaft_cell(body, z, rx, rz) {
                    return c;
                }
            }
        }
    }
    (6, 4)
}

/// Escalera de subida del piso `z` (sólo bajo la superficie): está donde caés
/// desde `z+1`, o sea en el pozo del piso de arriba.
pub fn ascend_portal(body: CelestialBody, z: i32) -> Option<(i64, i64)> {
    if z >= 0 {
        return None;
    }
    Some(descend_portal(body, z + 1))
}

/// Los pozos relevantes alrededor de un tile: bajada de este piso y subida al
/// de arriba, para las cuatro celdas de cuevas vecinas.
///
/// Se calcula UNA vez y lo comparten `portal_here` y `portal_adjacent`: antes
/// cada tile del mundo repetía este trabajo diez veces (una por vecino), y con
/// la geografía nueva eso hacía que generar un chunk tardara una eternidad.
fn shafts_around(body: CelestialBody, z: i32, tx: i64, tz: i64) -> [Option<(i64, i64)>; 8] {
    let mut out = [None; 8];
    if z > 0 {
        return out;
    }
    for (i, (rx, rz)) in cave_regions(tx, tz).into_iter().enumerate() {
        if z > MIN_Z_LEVEL {
            out[i] = shaft_cell(body, z, rx, rz);
        }
        if z < 0 {
            out[4 + i] = shaft_cell(body, z + 1, rx, rz);
        }
    }
    out
}

/// Portal en esta celda exacta, si lo hay. Bocas de cueva en superficie y pozos
/// escalonados hacia abajo, repartidos por todo el mundo.
pub fn portal_here(body: CelestialBody, z: i32, tx: i64, tz: i64) -> Option<Portal> {
    portal_from(&shafts_around(body, z, tx, tz), tx, tz)
}

fn portal_from(shafts: &[Option<(i64, i64)>; 8], tx: i64, tz: i64) -> Option<Portal> {
    for (i, s) in shafts.iter().enumerate() {
        if *s == Some((tx, tz)) {
            return Some(if i < 4 { Portal::Descend } else { Portal::Ascend });
        }
    }
    None
}

/// ¿Hay un portal en el anillo de 1 celda alrededor? (Se deja abierto para no
/// llegar nunca atrapado.) Comparte los pozos ya calculados.
fn portal_ring(shafts: &[Option<(i64, i64)>; 8], tx: i64, tz: i64) -> bool {
    shafts
        .iter()
        .flatten()
        .any(|&(sx, sz)| (sx - tx).abs() <= 1 && (sz - tz).abs() <= 1 && (sx, sz) != (tx, tz))
}

pub fn solid_at(body: CelestialBody, z: i32, tx: i64, tz: i64) -> bool {
    topdown_tile(body, z, tx, tz).solid()
}

// ---------------------------------------------------------------------------
// Deposits (ore/gem veins) — deterministic, denser underground
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deposit {
    OreIron,
    OreGold,
    GemBlue,
    GemRed,
}

/// Deterministic deposit at a walkable cell, if any.
pub fn deposit_at(body: CelestialBody, z: i32, tx: i64, tz: i64) -> Option<Deposit> {
    // Surface: sparse rocks/ore. Underground: richer, deeper = rarer gems.
    let seed = body_terrain_seed(body) ^ 0xde90_0000u32 ^ (z as u32).wrapping_mul(0x2545);
    let h = hash2(seed, tx, tz);
    let roll = h % 1000;
    let deep = z < 0;
    match body {
        CelestialBody::Moon => match roll {
            0..=3 if deep => Some(Deposit::GemRed),
            4..=14 if deep => Some(Deposit::OreIron),
            0..=1 => Some(Deposit::OreIron),
            _ => None,
        },
        CelestialBody::Earth => match roll {
            0..=6 if deep => Some(Deposit::OreIron),
            7..=10 if deep => Some(Deposit::OreGold),
            11..=13 if deep => Some(Deposit::GemBlue),
            0..=2 => Some(Deposit::OreIron),
            _ => None,
        },
        _ => match roll {
            0..=8 if deep => Some(Deposit::OreIron),
            9..=12 if deep => Some(Deposit::GemBlue),
            0..=1 => Some(Deposit::OreIron),
            _ => None,
        },
    }
}

// ---------------------------------------------------------------------------
// 8-direction locomotion (no gravity, capsule vs tile grid, axis-separated)
// ---------------------------------------------------------------------------

/// Nearest of 8 facing directions (0..8, 0 = East, CCW) for animation.
pub fn dir8(rot: f32) -> u8 {
    let tau = std::f32::consts::TAU;
    (((rot.rem_euclid(tau) / tau) * 8.0).round() as u8) % 8
}

/// Step the player on a top-down floor. `body.x`/`body.z` are the ground
/// plane; `move_x`→x, `move_z`→z. `solid(tx,tz)` answers for the ACTIVE floor
/// including player edits.
pub fn step_topdown_movement(
    body: &mut MovementBody,
    input: &InputState,
    dt: f32,
    solid: &dyn Fn(i64, i64) -> bool,
) {
    step_topdown_movement_ex(body, input, dt, solid, &|_x, _z| false)
}

/// Cuánto del paso normal se avanza NADANDO. Bastante lento como para que cruzar
/// un lago sea una decisión (rodearlo suele salir más barato) y no tanto como para
/// que sea un castigo.
pub const SWIM_SPEED_MULT: f32 = 0.45;

/// Igual que `step_topdown_movement`, pero además contra las **paredes de
/// borde** (`build_grid`): `walls(x, z)` responde si la cápsula centrada ahí
/// pisa una pared que corta el paso. Separado del grid de celdas porque una
/// pared vive en la ARISTA entre dos celdas, no en una celda.
pub fn step_topdown_movement_ex(
    body: &mut MovementBody,
    input: &InputState,
    dt: f32,
    solid: &dyn Fn(i64, i64) -> bool,
    walls: &dyn Fn(f32, f32) -> bool,
) {
    step_topdown_movement_full(body, input, dt, solid, walls, &|_x, _z| false)
}

/// La versión completa: además del grid y las paredes de borde, `swim(tx,tz)`
/// responde si esa celda es líquido de poco fondo.
///
/// Nadar no es un modo aparte —no hay estado que entrar ni salir—: es el MISMO
/// paso, más lento, sin esprintar y con otra pose. Por eso se puede entrar al agua
/// caminando desde la playa sin ningún corte, que es lo que hace que la costa se
/// sienta continua en vez de un borde con un cartel.
pub fn step_topdown_movement_full(
    body: &mut MovementBody,
    input: &InputState,
    dt: f32,
    solid: &dyn Fn(i64, i64) -> bool,
    walls: &dyn Fn(f32, f32) -> bool,
    swim: &dyn Fn(i64, i64) -> bool,
) {
    let swimming = swim(
        (body.x / TILE_M).floor() as i64,
        (body.z / TILE_M).floor() as i64,
    );
    let mut mx = input.move_x;
    let mut mz = input.move_z;
    let len = (mx * mx + mz * mz).sqrt();
    if len > 1.0 {
        mx /= len;
        mz /= len;
    }
    let moving = len > 0.1;
    // Nadando no se esprinta: la carrera es de pies, no de brazos.
    let wants_sprint = input.sprint == 1 && moving && body.stamina > 0.0 && !swimming;
    let speed_mult = if wants_sprint {
        SPRINT_MULT
    } else if swimming {
        SWIM_SPEED_MULT
    } else {
        1.0
    };
    let target_vx = mx * body.speed * speed_mult;
    let target_vz = mz * body.speed * speed_mult;
    let blend = (FRICTION * dt).clamp(0.0, 1.0);
    body.vx += (target_vx - body.vx) * blend;
    body.vz += (target_vz - body.vz) * blend;

    if wants_sprint {
        body.stamina = (body.stamina - STAMINA_SPRINT_DRAIN * dt).max(0.0);
    } else {
        body.stamina = (body.stamina + STAMINA_SPRINT_DRAIN * 0.6 * dt).min(body.max_stamina);
    }

    // X primero y Z después, resolviendo contra los sólidos (deslizar por la
    // pared) — y con CORRECCIÓN DE ESQUINA en los dos ejes (ver `corner_slip`).
    let free = |x: f32, z: f32| !capsule_hits(x, z, solid) && !walls(x, z);
    // Cuánto se permite correr de lado en este frame: nunca más de lo que el
    // propio jugador avanza, así el asistente jamás se siente un empujón.
    let slack = (body.speed * speed_mult * dt).min(CORNER_SLIP);

    let new_x = body.x + body.vx * dt;
    if free(new_x, body.z) {
        body.x = new_x;
    } else if let Some(z) = corner_slip(body.z, slack, &|z| free(new_x, z)) {
        body.x = new_x;
        body.z = z;
    } else {
        body.vx = 0.0;
    }
    let new_z = body.z + body.vz * dt;
    if free(body.x, new_z) {
        body.z = new_z;
    } else if let Some(x) = corner_slip(body.x, slack, &|x| free(x, new_z)) {
        body.z = new_z;
        body.x = x;
    } else {
        body.vz = 0.0;
    }

    // El estado de natación se mantiene también quieto en el agua: flotar es una
    // pose, no una pausa — si no, pararse en mitad del lago te devolvía a la pose
    // de estar de pie sobre el agua.
    body.state = if swimming {
        MOV_SWIMMING
    } else if !moving {
        MOV_IDLE
    } else if wants_sprint {
        MOV_SPRINTING
    } else {
        MOV_RUNNING
    };
    if moving {
        body.rot = mz.atan2(mx);
    }
    body.y = 0.0;
}

/// Cuánto se puede correr al jugador de lado, como MUCHO, para librar el pico de
/// una esquina (metros por frame).
///
/// 0.16 sobre un radio de 0.34: alcanza para que un roce de un par de
/// centímetros no te frene, y es demasiado poco como para atravesar nada — un
/// hueco de verdad mide un tile entero.
const CORNER_SLIP: f32 = 0.16;

/// **Que no te comas la esquina.**
///
/// El problema, que en un juego cenital se siente todo el tiempo: vas caminando
/// pegado a una pared y el pico de un bloque te roza dos centímetros. La cápsula
/// choca, el eje se cancela y quedás CLAVADO contra una arista, empujando sin
/// avanzar. No es que no quepas: te sobra casi todo el ancho, te falta un pelo.
///
/// La corrección es la de los plataformeros: si el movimiento está bloqueado
/// pero corriéndose un poco de lado deja de estarlo, se corre y se pasa. Sólo
/// se prueban desplazamientos **pequeños** (`CORNER_SLIP`, y nunca más de lo
/// que el jugador avanza en ese frame), así que jamás puede meterte por un
/// hueco que no existe ni teletransportarte: si hace falta más que un roce,
/// devuelve `None` y chocás como siempre.
///
/// Se prueba de menos a más y en los dos sentidos, quedándose con el
/// desplazamiento MÁS CHICO que funciona — el que menos se nota. Y se aplica en
/// los dos ejes, así que sirve igual yendo en horizontal que en vertical, y en
/// las esquinas de 90° que es donde más engancha.
fn corner_slip(cur: f32, slack: f32, free_at: &dyn Fn(f32) -> bool) -> Option<f32> {
    if slack <= 0.0 {
        return None;
    }
    const STEPS: i32 = 4;
    for i in 1..=STEPS {
        let d = slack * i as f32 / STEPS as f32;
        for cand in [cur + d, cur - d] {
            if free_at(cand) {
                return Some(cand);
            }
        }
    }
    None
}

fn capsule_hits(x: f32, z: f32, solid: &dyn Fn(i64, i64) -> bool) -> bool {
    let x0 = ((x - PLAYER_RADIUS) / TILE_M).floor() as i64;
    let x1 = ((x + PLAYER_RADIUS - 1e-4) / TILE_M).floor() as i64;
    let z0 = ((z - PLAYER_RADIUS) / TILE_M).floor() as i64;
    let z1 = ((z + PLAYER_RADIUS - 1e-4) / TILE_M).floor() as i64;
    for tx in x0..=x1 {
        for tz in z0..=z1 {
            if solid(tx, tz) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::empty_input;

    fn natural_solid(body: CelestialBody, z: i32) -> impl Fn(i64, i64) -> bool {
        move |tx, tz| solid_at(body, z, tx, tz)
    }

    /// **No quedarse comido por una esquina.** Un bloque suelto con el jugador
    /// rozándolo por un pelo: antes el eje se cancelaba y te quedabas empujando
    /// contra la arista sin avanzar nunca. Ahora se corre lo justo y pasa.
    #[test]
    fn a_corner_does_not_swallow_you() {
        // Un único tile sólido en (2, 2).
        let solid = |tx: i64, tz: i64| (tx, tz) == (2, 2);
        let nowalls = |_x: f32, _z: f32| false;
        let mut b = MovementBody::new(4.0, PLAYER_RADIUS);
        // Caminando hacia +x justo al sur del bloque (que ocupa z ∈ [2,3]), tan
        // pegado que la cápsula lo roza por 5 cm. Eso es un ROCE, no un choque:
        // sobra el 93 % del ancho del jugador.
        b.x = 0.5;
        b.z = 3.0 + PLAYER_RADIUS - 0.05;
        let mut input = empty_input(0);
        input.move_x = 1.0;
        let start = b.x;
        for _ in 0..120 {
            step_topdown_movement_ex(&mut b, &input, 1.0 / 60.0, &solid, &nowalls);
        }
        assert!(
            b.x > start + 2.0,
            "se quedó trabado en la esquina: avanzó sólo {:.2} m",
            b.x - start
        );
        // Y NO lo atravesó: la cápsula sigue enteramente al sur del bloque.
        assert!(b.z >= 3.0 - 1e-3, "se metió dentro del sólido (z={})", b.z);
    }

    /// El asistente de esquina no puede abrir puertas que no existen: contra una
    /// pared LLENA se choca igual que siempre.
    #[test]
    fn corner_slip_never_opens_a_solid_wall() {
        // Pared vertical completa en x = 2.
        let solid = |tx: i64, _tz: i64| tx == 2;
        let nowalls = |_x: f32, _z: f32| false;
        let mut b = MovementBody::new(4.0, PLAYER_RADIUS);
        b.x = 0.5;
        b.z = 5.5;
        let mut input = empty_input(0);
        input.move_x = 1.0;
        for _ in 0..240 {
            step_topdown_movement_ex(&mut b, &input, 1.0 / 60.0, &solid, &nowalls);
        }
        assert!(b.x < 2.0, "atravesó la pared (x={})", b.x);
    }

    #[test]
    fn terrain_is_deterministic() {
        let a = topdown_tile(CelestialBody::Earth, 0, 12, -7);
        let b = topdown_tile(CelestialBody::Earth, 0, 12, -7);
        assert_eq!(a, b);
        // La Luna ya no es un gris uniforme: regolito, mares de basalto y algún
        // pico. Lo que importa es que sea determinista y lunar.
        // `Cliff`/`Slope` entraron con el relieve por terrazas: una celda lunar
        // puede caer en el borde de una terraza, y eso sigue siendo terreno lunar.
        assert!(matches!(
            topdown_tile(CelestialBody::Moon, 0, 3, 3),
            Tile::Regolith | Tile::Basalt | Tile::Stone | Tile::Cliff | Tile::Slope
        ));
        assert_eq!(topdown_tile(CelestialBody::Earth, -20, 0, 0), Tile::Bedrock);
    }

    #[test]
    fn surface_has_variety() {
        // Los biomas ahora son REGIONES de kilómetros (continentes, cordilleras,
        // desiertos), no manchas de un tile: hay que barrer el planeta, no un
        // cuadrado de 40 m, para verlos. Que un patch chico sea homogéneo es la
        // intención, no un fallo.
        let mut kinds = std::collections::HashSet::new();
        for i in -14..14 {
            for j in -14..14 {
                kinds.insert(topdown_tile(CelestialBody::Earth, 0, i * 1_000, j * 1_000));
            }
        }
        assert!(kinds.len() >= 4, "expected biome variety, got {kinds:?}");
        assert!(kinds.contains(&Tile::Water), "un planeta sin oceanos");
        assert!(kinds.contains(&Tile::Grass), "un planeta sin praderas");
    }

    /// GUARDA DE COSTE, medida en RELATIVO.
    ///
    /// Un chunk son 256 celdas y el streaming construye decenas por segundo,
    /// además del mapa satelital (miles de muestras por reconstrucción). Cuando
    /// la geografía nueva entró sin optimizar, cada celda repetía diez veces el
    /// cálculo de los pozos de cueva y el juego se quedaba colgado arrancando.
    ///
    /// Se compara contra el coste de UNA evaluación de relieve medida en la
    /// misma pasada, no contra un número de microsegundos: así el test dice
    /// "generar una celda no puede costar más que unas pocas consultas de
    /// terreno" y no depende de lo rápida ni de lo cargada que esté la máquina
    /// (un umbral absoluto se volvía intermitente en el pipeline de publicación).
    #[test]
    fn generating_a_cell_costs_only_a_few_terrain_probes() {
        use std::time::Instant;
        let body = CelestialBody::Earth;
        const N: i64 = 4_000;
        // Precalentar ambos caminos.
        for i in 0..256 {
            let _ = topdown_tile(body, 0, i, i);
            let _ = crate::geography::elevation_at(body, i, i);
        }

        // BASE: el campo de relieve crudo, SIN caché (`relief_field`, no
        // `relief_level`). Es la unidad honesta —"una evaluación de ruido"— y
        // además no calienta la caché de niveles, que era lo que hacía que el caso
        // disperso midiera más barato que el secuencial.
        let t0 = Instant::now();
        let mut sink = 0.0f32;
        for i in 0..N {
            sink += crate::relief::relief_field(body, 900_000 + i * 7, 900_000 + i * 13);
        }
        let base = t0.elapsed().as_nanos().max(1) as f64;

        // (a) BARRIDO SECUENCIAL — el camino caliente de verdad: es como
        // `build_chunk_mesh` recorre un chunk, 60 veces por segundo en el Redmi.
        // La caché de niveles de `relief` cobra aquí: cada celda se consulta cinco
        // veces (como centro y como vecina de sus cuatro contiguas) y se calcula
        // una sola.
        let t1 = Instant::now();
        for cz in 0..(N / 64) {
            for cx in 0..64 {
                sink += topdown_tile(body, 0, cx, cz) as u8 as f32;
            }
        }
        let sequential = t1.elapsed().as_nanos() as f64;

        // (b) CONSULTAS DISPERSAS — colisión, IA, el ítem-mapa. **Las mismas
        // celdas** que (a), sólo que recorridas en orden salteado: así la única
        // variable entre las dos medidas es la localidad, y no el terreno que a
        // cada una le tocó (un barrido que se sale del mundo choca con el
        // `beyond_world_edge` y sale gratis — medir eso no dice nada).
        let cells = 64 * (N / 64);
        let t2 = Instant::now();
        for i in 0..cells {
            let k = (i * 2_657) % cells; // 2657 es primo con 64·62: recorre todo
            sink += topdown_tile(body, 0, k % 64, k / 64) as u8 as f32;
        }
        let scattered = t2.elapsed().as_nanos() as f64;
        assert!(sink.is_finite());

        let seq_ratio = sequential / base;
        let sca_ratio = scattered / base;
        // Generar una celda mira relieve (con sus cuatro vecinas), humedad, río,
        // obra civil y pozos de cueva: medido en una máquina en reposo da ~8x
        // secuencial y ~11x disperso.
        //
        // **El umbral es deliberadamente holgado (40x).** Esto corre en paralelo
        // con el resto de la suite, así que la relación oscila entre 8x y 16x según
        // la contención — apretarlo a 13x lo volvía intermitente, que es peor que
        // no tenerlo. Lo que este guardián tiene que cazar es la regresión de ORDEN
        // DE MAGNITUD: la primera versión de la caché de relieve, que agrupaba los
        // niveles en bloques de 16×16 y recalculaba las 256 celdas para leer una,
        // marcaba 225x. Eso lo detecta con 40x y sin falsos positivos.
        assert!(
            seq_ratio < 40.0,
            "generar un chunk cuesta {seq_ratio:.1}x por celda (disperso: {sca_ratio:.1}x)"
        );
        assert!(
            sca_ratio < 40.0,
            "una consulta suelta cuesta {sca_ratio:.1}x: alguien perdió la localidad"
        );
    }

    #[test]
    fn cave_mouths_are_scattered_and_lead_somewhere() {
        // En un mundo de 60 km tiene que haber MUCHAS bocas, no una sola mina
        // junto al origen — y cada una tiene que caer en galería abierta y en
        // suelo pisable, o sería un agujero a roca maciza.
        let body = CelestialBody::Earth;
        let mut mouths = 0;
        for rx in -20..20 {
            for rz in -20..20 {
                let Some((tx, tz)) = cave_mouth_in(body, rx, rz) else { continue };
                mouths += 1;
                assert_eq!(portal_here(body, 0, tx, tz), Some(Portal::Descend));
                assert!(!topdown_tile(body, 0, tx, tz).solid(), "boca sobre roca maciza");
                assert!(!solid_at(body, -1, tx, tz), "boca sin galeria debajo");
            }
        }
        // Una boca cada ~2.5 km en la zona muestreada. El 200 de antes era un
        // número redondo sin fundamento; lo que este test defiende es que HAYA
        // muchas bocas repartidas y no una sola mina junto al origen, y 120 en un
        // cuadrado de 36 km lo cumple de sobra. Con la plantilla de mundo, parte
        // del muestreo cae en océano y ahí no hay minas — eso es correcto.
        assert!(mouths > 120, "solo {mouths} bocas de cueva en 40x40 celdas");
    }

    #[test]
    fn every_descend_has_a_way_back_up() {
        // Bajar sin poder subir es el soft-lock clásico de las cuevas.
        let body = CelestialBody::Earth;
        for rx in -6..6 {
            for rz in -6..6 {
                let Some(mouth) = cave_mouth_in(body, rx, rz) else { continue };
                assert_eq!(portal_here(body, -1, mouth.0, mouth.1), Some(Portal::Ascend));
            }
        }
    }

    #[test]
    fn caves_open_up_with_depth() {
        let count_open = |z: i32| {
            let mut n = 0;
            for tx in 0..60 {
                for tz in 0..60 {
                    if !solid_at(CelestialBody::Earth, z, tx, tz) {
                        n += 1;
                    }
                }
            }
            n
        };
        // Deeper floors have at least as much open space (looser threshold).
        assert!(count_open(-6) >= count_open(-1));
    }

    #[test]
    fn dir8_maps_cardinals() {
        use std::f32::consts::PI;
        assert_eq!(dir8(0.0), 0); // East
        assert_eq!(dir8(PI / 2.0), 2); // South (+z)
        assert_eq!(dir8(PI), 4); // West
        assert_eq!(dir8(-PI / 2.0), 6); // North (-z)
    }

    #[test]
    fn walks_in_8_directions_no_gravity() {
        let solid = |_tx: i64, _tz: i64| false; // terreno despejado: se prueba el MOVIMIENTO
        let mut b = MovementBody::new(5.0, PLAYER_RADIUS);
        b.x = 100.0;
        b.z = 100.0;
        let mut input = empty_input(1);
        // Move north-east (x+, z-).
        input.move_x = 1.0;
        input.move_z = -1.0;
        let (x0, z0, y0) = (b.x, b.z, b.y);
        for _ in 0..60 {
            step_topdown_movement(&mut b, &input, 0.05, &solid);
        }
        assert!(b.x > x0 + 1.0, "should move east");
        assert!(b.z < z0 - 1.0, "should move north");
        assert_eq!(b.y, y0, "no vertical drift in top-down");
    }

    #[test]
    fn collision_blocks_into_wall() {
        // A solid column at tx=105: walking east must stop before it.
        let solid = |tx: i64, _tz: i64| tx >= 105;
        let mut b = MovementBody::new(5.0, PLAYER_RADIUS);
        b.x = 100.0;
        b.z = 0.0;
        let mut input = empty_input(1);
        input.move_x = 1.0;
        for _ in 0..120 {
            step_topdown_movement(&mut b, &input, 0.05, &solid);
        }
        assert!(b.x < 105.0, "player must not pass into solid tiles, x={}", b.x);
        assert!(b.x > 103.0, "player should reach near the wall, x={}", b.x);
    }

    #[test]
    fn prefab_pattern_and_cost() {
        for p in PREFABS {
            // Door sits on the perimeter (or inner partition) and is walkable.
            assert_eq!(prefab_cell(p, p.door.0, p.door.1), Some(Tile::Door), "{}", p.id);
            // Corners are walls; strictly interior (non-partition) cells are floor.
            assert_eq!(prefab_cell(p, 0, 0), Some(Tile::WoodWall));
            assert_eq!(prefab_cell(p, p.w - 1, p.h - 1), Some(Tile::WoodWall));
            assert_eq!(prefab_cell(p, 1, 1), Some(Tile::WoodFloor), "{}", p.id);
            // Outside the footprint: nothing.
            assert_eq!(prefab_cell(p, -1, 0), None);
            assert_eq!(prefab_cell(p, p.w, 0), None);
            // Cost = walls×2 + doors×3 + floors×1 and is stable/deterministic.
            assert!(prefab_cost(p) > 0);
        }
        // room_small 5×4: perimeter 14 cells − 1 door = 13 walls (26) + door (3)
        // + 6 floors = 35.
        let small = &PREFABS[0];
        assert_eq!((small.w, small.h), (5, 4));
        assert_eq!(prefab_cost(small), 35);
        // casa_1 has an inner doorway connecting both rooms.
        let casa = PREFABS.iter().find(|p| p.id == "casa_1").unwrap();
        assert_eq!(prefab_cell(casa, 5, 3), Some(Tile::Door));
        assert_eq!(prefab_cell(casa, 5, 2), Some(Tile::WoodWall));
    }

    #[test]
    fn tile_serde_roundtrip() {
        // world_edits.json (editor de mundo H0) serializa tiles → deben ir y volver.
        for t in [Tile::Water, Tile::WoodFloor, Tile::Grass, Tile::StoneBrick, Tile::Door] {
            let j = serde_json::to_string(&t).unwrap();
            let back: Tile = serde_json::from_str(&j).unwrap();
            assert_eq!(t, back);
        }
    }

    #[test]
    fn large_assets_catalog_sane() {
        assert!(large_asset_count() >= 1);
        for i in 0..large_asset_count() {
            let a = large_asset(i).unwrap();
            assert!(a.w > 0 && a.h > 0, "{}", a.id);
            assert!(!a.art.is_empty() && a.cost_qty > 0, "{}", a.id);
        }
        assert!(large_asset(large_asset_count()).is_none());
    }

    #[test]
    fn settlements_are_deterministic_and_earth_only() {
        // Same site cell → same site, twice (pure function of the seed).
        for sx in -8..8 {
            for sz in -8..8 {
                assert_eq!(
                    building_site_in(CelestialBody::Earth, sx, sz),
                    building_site_in(CelestialBody::Earth, sx, sz)
                );
                assert!(building_site_in(CelestialBody::Moon, sx, sz).is_none());
                assert!(building_site_in(CelestialBody::Mars, sx, sz).is_none());
            }
        }
        // Find a generated house and sanity-check its tiles + door.
        let mut found = false;
        for sx in -20..20 {
            for sz in -20..20 {
                let Some(site) = building_site_in(CelestialBody::Earth, sx, sz) else {
                    continue;
                };
                found = true;
                let (ox, oz) = site.min;
                // El perímetro es MURO — de madera en una casa, de ladrillo en un
                // hospital: la parcela decide el material según su programa, así
                // que el test comprueba la clase de tile, no una variante concreta.
                let corner = building_cell(CelestialBody::Earth, ox, oz);
                assert!(
                    matches!(
                        corner,
                        Some(Tile::WoodWall | Tile::StoneBrick | Tile::Window | Tile::Door)
                    ),
                    "la esquina del edificio no es pared: {corner:?}"
                );
                // El interior es piso pisable.
                let inside = building_cell(CelestialBody::Earth, ox + 2, oz + 2);
                assert!(
                    inside.is_some_and(|t| !t.solid()),
                    "el interior del edificio no se puede pisar: {inside:?}"
                );
                // Y se entra por la puerta.
                assert!(!solid_at(CelestialBody::Earth, 0, site.door.0, site.door.1));
            }
        }
        assert!(found, "some settlement should generate in a 40×40-site sweep");
    }

    #[test]
    fn new_build_tiles_behave() {
        assert!(Tile::Window.solid());
        assert!(!Tile::StairsDown.solid());
        assert!(!Tile::StairsUp.solid());
        for t in [Tile::Window, Tile::StairsDown, Tile::StairsUp] {
            assert_eq!(t.dug_item(), Some("wood"), "build pieces refund material");
        }
        // **El piso alto es VACÍO, no roca.** Si volviera a ser sólido (bedrock),
        // el segundo piso dejaría de poderse construir: `place_tile` rechaza toda
        // celda sólida. Es la invariante entera de la verticalidad.
        for z in 1..=3 {
            let t = topdown_tile(CelestialBody::Earth, z, 3, 3);
            assert_eq!(t, Tile::Air, "piso {z}");
            assert!(!t.solid(), "el vacio no puede frenar ni bloquear la obra");
            assert!(!t.buildable_ground(), "y tampoco es suelo: sobre el vacio se CAE");
        }
        // Sin portales de mina fantasma allá arriba.
        assert!(portal_here(CelestialBody::Earth, 1, 3, 3).is_none());
    }

    #[test]
    fn portals_round_trip_between_floors() {
        let body = CelestialBody::Earth;
        // The ladder up on floor z is exactly the hole down on floor z+1
        // (a coherent vertical shaft): descend then ascend returns you.
        for z in -6..0 {
            assert_eq!(
                ascend_portal(body, z),
                Some(descend_portal(body, z + 1)),
                "floor {z}: ladder up must match the hole above"
            );
        }
        // Surface has a descend hole but no ascend ladder.
        assert!(ascend_portal(body, 0).is_none());
        assert!(matches!(
            portal_here(body, 0, descend_portal(body, 0).0, descend_portal(body, 0).1),
            Some(Portal::Descend)
        ));
    }

    #[test]
    fn portal_cells_are_walkable() {
        let body = CelestialBody::Earth;
        for z in [-1, -3, -6] {
            let (px, pz) = descend_portal(body, z);
            assert!(
                !solid_at(body, z, px, pz),
                "descend portal on floor {z} must be walkable"
            );
        }
    }

    #[test]
    fn deposits_are_deterministic_and_deeper_is_richer() {
        let a = deposit_at(CelestialBody::Earth, -5, 42, 17);
        assert_eq!(a, deposit_at(CelestialBody::Earth, -5, 42, 17));
        let deep = (0..80)
            .flat_map(|tx| (0..80).map(move |tz| (tx, tz)))
            .filter(|(tx, tz)| deposit_at(CelestialBody::Earth, -5, *tx, *tz).is_some())
            .count();
        let surf = (0..80)
            .flat_map(|tx| (0..80).map(move |tz| (tx, tz)))
            .filter(|(tx, tz)| deposit_at(CelestialBody::Earth, 0, *tx, *tz).is_some())
            .count();
        assert!(deep > surf, "underground should have more deposits ({deep} vs {surf})");
    }
}
