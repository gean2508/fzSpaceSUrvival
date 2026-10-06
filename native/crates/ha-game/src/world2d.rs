//! Top-Down 3/4 tile world (LimeZu) — successor to the side-scroller strip.
//!
//! Ground plane = XZ. Grid cell `(tx, tz)` renders to screen `(tx*TILE,
//! -tz*TILE)` — same `(x, -z)` projection as `orbital2d.rs`. Depth is a
//! discrete Z-Level (`z`): 0 = surface, negative = mine floors (Fase 3).
//! Chunks are 2D grids of `Mesh2d` quads (vertex colors); decor/deposits are
//! y-sorted sprites. Ver `biblia.md` §5.
//!
//! Module keeps the old public names (`TileWorld`, `ActivePlanet`,
//! `active_surface_body`, …) so machines/launch_pad/targeting compile through
//! the migration; the side-scroller `flat2d`/strip logic is gone from here.

use std::collections::{HashMap, HashSet};

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use serde::{Deserialize, Serialize};

use ha_shared::topdown::{
    deposit_at, topdown_tile, Deposit, Tile, MIN_Z_LEVEL, TILE_M,
};
use ha_shared::{CelestialBody, InteractTargetKind, WorldContext};

use crate::art2d::ArtLibrary;
use crate::harvest::{FloraHarvest, TreeInstance};
use crate::inventory_persist::persist_dir;
use crate::platform::is_app_paused;
use crate::scene::{RegionId, RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, SimState};
use crate::targeting::InteractTarget;

/// Tiles per chunk side (a chunk is CHUNK×CHUNK cells).
pub const CHUNK: i64 = 16;
/// Chunks kept alive around the player (Chebyshev radius).
const CHUNK_RANGE: i64 = 3;
/// **MARGEN DE CONSTRUCCIÓN — el anillo que se arma FUERA DE CUADRO.**
///
/// El bug que cierra: `range` se calcula para llenar exactamente la pantalla
/// (`live_view_h * 1.25`) y los chunks se construían justo hasta ahí. O sea que
/// un chunk se armaba **el frame en que ya se veía**, a 2 por frame: caminando,
/// el jugador miraba los rectángulos de suelo aparecer uno por uno en el borde.
/// Es el "titila verde" —el pasto es un quad de color plano mientras no tenga
/// arte asignada, así que cada chunk nuevo es un rectángulo verde que aparece de
/// la nada.
///
/// Con margen, el suelo se arma un anillo ANTES de entrar en cuadro y para
/// cuando se ve ya está puesto. El envolvente de supervivencia
/// (`range + BUILD_MARGIN + 1`) queda un paso más afuera para que siga habiendo
/// histéresis y el borde no se construya/destruya en frames alternos.
///
/// Cuesta memoria: con `range` 3 pasa de 7×7 = 49 chunks a 9×9 = 81. Por eso el
/// margen sólo trae el SUELO y la decoración gruesa (`want_detail` ya daba
/// `Coarse` hasta `range + 1`, un anillo que hasta ahora no existía); la flora
/// chica sigue acotada a `DECOR_FULL_RANGE`.
///
/// `MAX_CHUNK_RANGE` sigue siendo el techo duro: volando con el zoom al máximo
/// el margen se come, pero ahí el detalle fino ya se está fundiendo contra el
/// mapa satelital (`FINE_OFF_VIEW_H`) y no hay rectángulo que mirar.
const BUILD_MARGIN: i64 = 1;
/// **Anillo de DETALLE COMPLETO** — donde se puebla la flora chica (matas,
/// hongos, props de adorno), que el propio `stream_tile_chunks` describe como "lo
/// MÁS numeroso por chunk". Antes compartía constante con el anillo de suelo
/// (`CHUNK_RANGE`), que es una decisión distinta: el suelo no puede faltar nunca,
/// la florecita a 100 m mide un píxel.
///
/// En Android baja a 2 (5×5 = 25 chunks en vez de 7×7 = 49, la mitad de sprites
/// chicos). A la vista de caminata no se nota: con `SURFACE_VIEW_H` de 30 m la
/// pantalla del Redmi muestra ~30×67 m y este anillo cubre 80×80 m — sigue
/// terminando fuera de cuadro. Sólo se asoma el borde al alejar el zoom del todo.
///
/// Motivo: el teléfono reportó **2916 sprites de decoración** con frames de
/// 356 ms y jitter de sólo 13.5 ms — un costo PAREJO, la firma de miles de draw
/// calls, no de un tirón. Menos sprites es menos draw calls, menos extracción y
/// menos trabajo de iluminación, todo a la vez.
#[cfg(target_os = "android")]
const DECOR_FULL_RANGE: i64 = 2;
#[cfg(not(target_os = "android"))]
const DECOR_FULL_RANGE: i64 = CHUNK_RANGE;

// **Se probó aclarar la siembra en Android (uno de cada dos adornos) y NO vale
// la pena.** Medido en el Redmi: la aldea pasó de 4.813 a 4.665 entidades —
// 148, un 3 %— sin mover los fps. El adorno chico no es de donde salen las
// entidades de una aldea: son los árboles y las rocas (`surface_decor_at`, que
// no es "chico"), los edificios y la gente. Media flora menos en el teléfono
// que en el escritorio es un precio visible por un ahorro que no se mide.
//
// Lo que sí sirvió, en orden: juntar lo sembrado en las páginas `mundo_N` del
// atlas y bajar la densidad de vereda y playa (ver `worldgen`). Si mañana hay
// que bajar más el conteo, el sitio a mirar es el adorno GRANDE y el arte de
// tile, no éste. Ver biblia §10.
/// Cuántos chunks se les puede construir mesh+decoración en UN frame. Sin este
/// tope, un salto grande de `range` (aterrizar, teletransportarse, entrar a
/// una ciudad densa) intenta poblar cientos de chunks de una vez y el frame se
/// congela (medido: 250 ms/4 fps). Con el tope se reparte en varios frames,
/// más cerca primero — imperceptible en vez de un tirón. Mismo patrón que
/// streaming de chunks en Minecraft/GTA.
/// **Medido: cada chunk cuesta ~6-12 ms de construir** (malla + decoración +
/// arte de tile, con `spawn_chunk_decor` recorriendo las 256 celdas y adjuntando
/// luces). Con el presupuesto en 10, un frame que llenaba el anillo se iba a
/// 60-124 ms — confirmado en el log (`ha-span chunk=59583us`, picos de
/// `chk=124817us`), y eso es UN SOLO sistema comiéndose de 4 a 8 frames enteros.
/// Era, de lejos, el mayor gasto del juego: las luces medían 424 µs y el HUD 19 µs.
///
/// Con 2 el mismo trabajo se reparte en cinco veces más frames y el pico baja a
/// ~12-24 ms. Lo que se paga es que el anillo tarda más en llenarse volando
/// rápido; lo que se gana es que deja de haber un frame de 124 ms, que es
/// exactamente el tirón que se siente. El fondo del asunto —que construir un
/// chunk cueste 6-12 ms— sigue abierto: ahí está la próxima gran ganancia.

const CHUNK_BUILD_BUDGET_PER_FRAME: usize = 2;
/// **Y LOS CHUNKS SON UN PRESUPUESTO FALSO, mismo defecto que
/// `universe::DiscBakeRows` antes del 16 ago 2026**: contar chunks sólo acota el
/// frame si todos cuestan lo mismo, y "6-12 ms" (picos medidos en 24 ms con el
/// tope en 2) es la medida en el Redmi en release — en un build de depuración o
/// una máquina más lenta el mismo trabajo cuesta un orden de magnitud más y el
/// tope de 2 dejaría de proteger nada. Se mira el reloj igual que el disco: se
/// construye al menos un chunk por frame (el anillo siempre avanza) y se sigue
/// mientras quede plazo, con `CHUNK_BUILD_BUDGET_PER_FRAME` como techo por
/// tanda. **30 ms** deja margen sobre el pico ya medido (24 ms): en el Redmi de
/// referencia el conteo sigue siendo el que corta primero, tal cual hoy: esto
/// es una red para el caso lento, no un recorte del caso normal.
const CHUNK_BUILD_MS_PER_FRAME: f32 = 30.0;
/// **Techo del anillo al volar.** El presupuesto de arriba reparte el COSTE de
/// construir en el tiempo, pero no acota el TOTAL vivo: sólo demora llegar al
/// pico. Con el tope viejo (12) el anillo llegaba a 25×25 = 625 chunks, cada uno
/// con su malla, su decoración y su arte de tile — de ahí salían los "19 k
/// entities volando" que menciona `stream_tile_chunks`, y es el único punto del
/// vuelo donde el consumo de memoria crece sin freno.
///
/// 7 son 15×15 = 225 chunks: un 64 % menos en el pico. Lo que queda más allá no
/// se pierde, lo cubre el mapa satelital de `universe.rs`, y los dos se cruzan
/// con el fundido de `fade_surface_lod` — que es exactamente el mecanismo que
/// existe para esto.
const MAX_CHUNK_RANGE: i64 = 7;
/// Tope duro de sprites de decoración vivos. Al alcanzarlo se deja de poblar
/// hasta que se libere: convierte un crecimiento sin límite (que termina en que
/// Android mate el proceso) en una degradación visual que se recupera sola al
/// alejarse. Holgado a propósito — en juego normal no se toca nunca.
///
/// ⚠ **NO BAJARLO PARA GANAR FPS — está medido que no sirve.** El 2026-07-31, con el
/// cable, se probó 3.200 en el Redmi: el frame pasó de 22-29 ms a **74 ms**, o sea
/// PEOR con la MITAD de sprites. En la misma sesión el espacio iba a 60 fps con 2.852
/// entidades. El cuello no es cuántos sprites hay, es que **cada especie es una
/// textura distinta** y el lote se rompe (ver `spawn_chunk_decor`). Se arregla con
/// atlas, no recortando el mundo.
const MAX_LIVE_DECOR: usize = 6_000;
/// Cuánta decoración y cuántos chunks se pueden DESTRUIR por frame al apagar el
/// detalle fino. Existe por la misma razón que `CHUNK_BUILD_BUDGET_PER_FRAME`:
/// destruir miles de entidades con hijos en un solo frame es un tirón igual de
/// grande que construirlas. Medido en el Redmi: el cruce a espacio marcaba picos de
/// 200-490 ms. Repartido, el mismo trabajo se reparte en ~15 frames y no se siente.
const DECOR_DESPAWN_BUDGET_PER_FRAME: usize = 420;
const CHUNK_DESPAWN_BUDGET_PER_FRAME: usize = 24;

/// Memoria del apagado del detalle fino (histéresis del umbral `FINE_OFF_VIEW_H`).
#[derive(Resource, Default)]
pub struct FineDetailOff(bool);

/// Chunks que se pueden construir por frame. Es [`CHUNK_BUILD_BUDGET_PER_FRAME`]
/// mientras se juega — el número está medido y no se toca (ver la nota larga de
/// esa constante) — pero la **pantalla de carga** lo sube a lo bestia: ahí nadie
/// está mirando el mundo, así que un frame de 200 ms no es un tirón, es tiempo
/// bien gastado. `crate::loading` lo levanta al entrar y lo baja al salir.
#[derive(Resource)]
pub struct ChunkBudget {
    /// Techo de chunks por tanda.
    pub count: usize,
    /// Plazo de la tanda, en milisegundos. **Es el que manda.**
    pub ms: f32,
}

impl Default for ChunkBudget {
    fn default() -> Self {
        Self::normal()
    }
}

impl ChunkBudget {
    pub const fn normal() -> Self {
        Self { count: CHUNK_BUILD_BUDGET_PER_FRAME, ms: CHUNK_BUILD_MS_PER_FRAME }
    }
}

/// El medidor y la histéresis, juntos en un solo parámetro: `stream_tile_chunks` ya
/// está en el tope de 16 que admite un sistema de Bevy, y perder el span sería perder
/// justo la instrumentación con la que se diagnostica este sistema (`RENDIMIENTO.md`).
#[derive(bevy::ecs::system::SystemParam)]
pub struct StreamCtl<'w> {
    span: Res<'w, crate::debug::CpuSpanStats>,
    fine_off: ResMut<'w, FineDetailOff>,
    budget: Res<'w, ChunkBudget>,
    /// Cuántos chunks quedaron sin construir en el último pase. La pantalla de
    /// carga espera a que esto sea 0: es la definición honesta de "el anillo
    /// visible ya está armado".
    backlog: ResMut<'w, ChunkBacklog>,
    /// Flora chica de chunks Full que todavía no se pobló. Ver [`SmallDecorQueue`].
    small_queue: ResMut<'w, SmallDecorQueue>,
}

/// Cola de la flora chica (matas, props de calle, muebles) por chunk `Full`.
///
/// Es la parte más cara de armar un chunk —recorre las 256 celdas por la cadena
/// entera de tablas de `worldgen`— y antes se hacía DENTRO de la tanda del chunk,
/// así que aterrizar sobre una ciudad (25 chunks `Full` de golpe) costaba cientos
/// de milisegundos en el Redmi en un solo frame. Ahora el suelo, los árboles y las
/// piedras entran en su tanda y esto se va llenando con el resto del presupuesto de
/// cada frame, lo más cercano al jugador primero. `ChunkBacklog` la cuenta, así
/// que la pantalla de carga sigue esperando a que se vacíe.
#[derive(Resource, Default)]
pub struct SmallDecorQueue(Vec<(CelestialBody, i32, i64, i64)>);

/// Chunks pendientes tras el último pase de streaming (0 = anillo completo).
#[derive(Resource, Default)]
pub struct ChunkBacklog(pub usize);

/// Screen render position of a world ground point `(x, z)` (metres).
/// North (−z) is up. Shared with the orbital view.
pub fn render_xy(x: f32, z: f32) -> Vec2 {
    Vec2::new(x, -z)
}

/// Y-sort depth for a sprite at world z (more south / larger z draws in front).
/// Reexport de [`crate::layers::ysort`] — la escalera completa de bandas z vive
/// en `layers`, éste es el nombre histórico que usa medio juego.
pub fn sort_z(world_z: f32) -> f32 {
    crate::layers::ysort(world_z)
}

/// Hash estable de una celda → id u32 para el set de cosecha/flora. ANTES truncaba
/// a `i16` (±32 km), así que lejos del origen las celdas COLISIONABAN con el
/// removed-set y la decoración dejaba de aparecer ("se cargaban las bases"). Ahora
/// mezcla los i64 completos: colisiones ~1/4e9 por par → despreciables, sin repetir.
pub fn pack_cell(tx: i64, tz: i64) -> u32 {
    let mut h = (tx as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (tz as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 29;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 32;
    h as u32
}

// ---------------------------------------------------------------------------
// Active body/layer
// ---------------------------------------------------------------------------

#[derive(Resource, Default, Clone, Copy)]
pub struct ActivePlanet {
    pub body: Option<CelestialBody>,
}

pub fn active_surface_body(ctx: &WorldContextRuntime, planet: &ActivePlanet) -> CelestialBody {
    match ctx.active {
        WorldContext::Luna => CelestialBody::Moon,
        WorldContext::Planet => planet.body.unwrap_or(CelestialBody::Mars),
        _ => CelestialBody::Earth,
    }
}

/// Reverso de `CelestialBody as u8` (para overrides autorados / persistencia por
/// u8, ej. estructuras y `world_edits.json`).
pub fn body_from_u8(v: u8) -> Option<CelestialBody> {
    CelestialBody::all().iter().copied().find(|b| *b as u8 == v)
}

pub fn active_landable_body(
    region: &RegionRuntime,
    ctx: &WorldContextRuntime,
    flight: &crate::flight::Flight,
    planet: &ActivePlanet,
) -> Option<CelestialBody> {
    if flight.blocks_walking() {
        return None;
    }
    surface_region_active(region)?;
    Some(active_surface_body(ctx, planet))
}

// ---------------------------------------------------------------------------
// Tile storage (natural terrain + sparse edits per (body, z, tx, tz))
// ---------------------------------------------------------------------------

fn tile_name(t: Tile) -> &'static str {
    match t {
        Tile::Air => "air",
        Tile::Grass => "grass",
        Tile::Dirt => "dirt",
        Tile::Sand => "sand",
        Tile::Water => "water",
        Tile::WaterDeep => "water_deep",
        Tile::Cliff => "cliff",
        Tile::Slope => "slope",
        Tile::Fence => "fence",
        Tile::Pavement => "pavement",
        Tile::Asphalt => "asphalt",
        Tile::Rock => "rock",
        Tile::Regolith => "regolith",
        Tile::Basalt => "basalt",
        Tile::Ice => "ice",
        Tile::RockRed => "rock_red",
        Tile::Stone => "stone",
        Tile::Subsoil => "subsoil",
        Tile::RockDark => "rock_dark",
        Tile::Bedrock => "bedrock",
        Tile::CaveFloor => "cave_floor",
        Tile::WoodFloor => "wood_floor",
        Tile::StoneFloor => "stone_floor",
        Tile::Wall => "wall",
        Tile::WoodWall => "wood_wall",
        Tile::StoneBrick => "stone_brick",
        Tile::Door => "door",
        Tile::Window => "window",
        Tile::Step => "step",
        Tile::Stairwell => "stairwell",
    }
}

fn tile_from_name(name: &str) -> Tile {
    match name {
        "grass" => Tile::Grass,
        "dirt" => Tile::Dirt,
        "sand" => Tile::Sand,
        "water" => Tile::Water,
        "water_deep" => Tile::WaterDeep,
        "cliff" => Tile::Cliff,
        "slope" => Tile::Slope,
        "fence" => Tile::Fence,
        "pavement" => Tile::Pavement,
        "asphalt" => Tile::Asphalt,
        "rock" => Tile::Rock,
        "regolith" => Tile::Regolith,
        "basalt" => Tile::Basalt,
        "ice" => Tile::Ice,
        "rock_red" => Tile::RockRed,
        "stone" => Tile::Stone,
        "rock_dark" => Tile::RockDark,
        "bedrock" => Tile::Bedrock,
        "cave_floor" => Tile::CaveFloor,
        "wood_floor" => Tile::WoodFloor,
        "stone_floor" => Tile::StoneFloor,
        "wall" => Tile::Wall,
        "wood_wall" => Tile::WoodWall,
        "stone_brick" => Tile::StoneBrick,
        "door" => Tile::Door,
        "window" => Tile::Window,
        "step" => Tile::Step,
        "stairwell" => Tile::Stairwell,
        // **Partidas viejas**: las gradas-teletransporte (`stairs_up`/`stairs_down`)
        // se borraron. Su celda vuelve a ser un rellano de madera y NO un hueco: en
        // el mapa guardado esa celda era pisable, y degradarla a `Air` abriría un
        // pozo por el que el jugador se cae al cargar la partida.
        "stairs_down" | "stairs_up" => Tile::WoodFloor,
        _ => Tile::Air,
    }
}

/// Tile a placeable block item becomes.
pub fn tile_for_block_item(item_id: &str) -> Option<Tile> {
    match item_id {
        "wood" => Some(Tile::WoodFloor),
        "stone" => Some(Tile::Wall),
        "dirt" => Some(Tile::Dirt),
        "sand" => Some(Tile::Sand),
        "regolith" => Some(Tile::Regolith),
        "basalt" => Some(Tile::Basalt),
        "ice" => Some(Tile::Ice),
        _ => None,
    }
}

#[derive(Resource, Default)]
pub struct TileWorld {
    /// (body u8, z, tx, tz) → override tile.
    edits: HashMap<(u8, i32, i64, i64), Tile>,
    /// Active depth layer (0 = surface).
    pub z: i32,
    pub version: u64,
    /// Losas que dejaron de serlo (picadas o pisadas por otro material). Las
    /// drena `integrity::collapse_unsupported`: lo que se apoyaba en ellas se
    /// viene abajo. Ver C5 en biblia §13.
    removed_floors: Vec<(u8, i32, i64, i64)>,
    /// Celdas editadas desde el último refresh de arte (autotile por vecinos):
    /// `rebuild_edited_tile_art` la drena y re-spawnea el arte de los chunks
    /// tocados (la edición cambia la forma de los 8 vecinos, no sólo la celda).
    edit_log: Vec<(i64, i64)>,
    /// Regeneración de césped: celdas superficiales cuyo edit es `Dirt` sobre
    /// césped natural — tras `timer` segundos el edit se borra y el pasto
    /// vuelve (Minecraft-style). Ver `tick_grass_regrowth`.
    regrow: Vec<RegrowCell>,
    dirty: bool,
    was_paused: bool,
    debounce: f32,
}

/// Una celda esperando volver a césped.
struct RegrowCell {
    body: u8,
    tx: i64,
    tz: i64,
    timer: f32,
}

impl TileWorld {
    /// Una copia liviana (sólo las ediciones y el nivel) para muestrear el terreno desde un hilo
    /// de fondo: la malla lejana (`universe::LodBake`) no necesita nada más del `TileWorld`.
    pub fn copia_para_lod(&self) -> TileWorld {
        TileWorld {
            edits: self.edits.clone(),
            z: self.z,
            version: self.version,
            removed_floors: Vec::new(),
            edit_log: Vec::new(),
            regrow: Vec::new(),
            dirty: false,
            was_paused: false,
            debounce: 0.0,
        }
    }

    /// Cuántas celdas editó el jugador (pozos, bloques, tierra picada) en todos
    /// los cuerpos. Una partida NUEVA tiene cero.
    pub fn edit_count(&self) -> usize {
        self.edits.len()
    }

    pub fn tile_at(&self, body: CelestialBody, tx: i64, tz: i64) -> Tile {
        self.tile_at_level(body, self.z, tx, tz)
    }

    /// Celdas EDITADAS de un nivel, con su tile. Es lo que permite recorrer "lo
    /// que el jugador construyó en la planta de arriba" sin barrer un cuadrado de
    /// miles de celdas por si acaso: sin ediciones, el bucle no tiene vueltas.
    pub fn edited_cells_at_level(
        &self,
        body: CelestialBody,
        level: i32,
    ) -> impl Iterator<Item = (i64, i64, Tile)> + '_ {
        let b = body as u8;
        self.edits
            .iter()
            .filter(move |((eb, ez, _, _), _)| *eb == b && *ez == level)
            .map(|((_, _, tx, tz), t)| (*tx, *tz, *t))
    }

    /// Vacía la lista de losas que dejaron de existir desde la última consulta.
    pub fn take_removed_floors(&mut self) -> Vec<(u8, i32, i64, i64)> {
        std::mem::take(&mut self.removed_floors)
    }

    /// Saca el edit de una celda de CUALQUIER nivel y deja lo que corresponda.
    /// Lo usa el derrumbe: la losa que se cae no es la del nivel activo.
    pub fn clear_floor_at_level(&mut self, body: CelestialBody, level: i32, tx: i64, tz: i64) {
        let key = (body as u8, level, tx, tz);
        if !ha_shared::build_grid::is_build_floor(self.tile_at_level(body, level, tx, tz)) {
            return;
        }
        if level > 0 {
            self.edits.insert(key, Tile::Air);
        } else {
            self.edits.remove(&key);
        }
        self.version = self.version.wrapping_add(1);
        self.dirty = true;
        self.edit_log.push((tx, tz));
        self.removed_floors.push((body as u8, level, tx, tz));
    }

    /// **Qué se DIBUJA en esta celda** — que no es lo mismo que en qué nivel
    /// estás parado. Es la regla del "un solo mundo" (§4), y es **la planta en la
    /// que estás**, en los tres sentidos:
    ///
    /// - Bajo tierra (`z < 0`) se ve el SUBSUELO: tierra maciza
    ///   ([`Tile::Subsoil`]) hasta donde llega la vista, y recortado en ella lo
    ///   que se excavó — la galería, tu sótano, el pozo. Eso es estar debajo.
    /// - Arriba (`z > 0`) manda tu planta, que para eso la construiste, y por el
    ///   vacío del borde se ve la calle de abajo atenuada
    ///   ([`tile_render_color`]): desde la azotea se ve por dónde entraste.
    /// - En la superficie, el mundo.
    ///
    /// **Por qué ya no se pinta la superficie estando abajo** (7 ago 2026): la
    /// idea era no cortar la escena —bajabas y seguías viendo tu casa y la calle,
    /// con tu túnel abierto al lado como el corte de un hormiguero—. En la
    /// práctica se leía al revés: caminabas por el subsuelo rodeado de pasto a
    /// pleno sol, con árboles y mariposas, y un agujero negro en el medio. No
    /// parecía estar abajo; parecía un mapa roto. La continuidad no la daba el
    /// dibujo —la dan los cuatro escalones que se andan y el gradiente de luz
    /// (`interior.rs`)—, así que lo que se dibuja pasa a decir la verdad.
    pub fn render_tile(&self, body: CelestialBody, tx: i64, tz: i64) -> Tile {
        if self.z != 0 {
            return self.tile_at(body, tx, tz);
        }
        self.tile_at_level(body, 0, tx, tz)
    }

    /// ¿Esta celda se dibuja como HUECO (pozo) en vez de como suelo? El hueco es
    /// [`Tile::Air`] y nada más: bajo tierra, el suelo de la galería es SUELO —se
    /// camina— y dibujarlo como un pozo era lo que convertía la cueva en una
    /// mancha de agujeros negros.
    pub fn renders_as_pit(&self, body: CelestialBody, tx: i64, tz: i64) -> bool {
        self.render_tile(body, tx, tz) == Tile::Air
    }

    /// El tile de OTRO nivel (el activo lo da `tile_at`). Lo pide la
    /// verticalidad: qué hay DEBAJO de la losa que querés colocar, y qué se ve
    /// por el hueco del piso alto (`tile_render_color`).
    pub fn tile_at_level(&self, body: CelestialBody, level: i32, tx: i64, tz: i64) -> Tile {
        if let Some(t) = self.edits.get(&(body as u8, level, tx, tz)) {
            return *t;
        }
        topdown_tile(body, level, tx, tz)
    }

    /// ¿La celda del nivel `level` aguanta una LOSA encima? Es la mitad "tile" de
    /// `build_grid::floor_supported` (la otra mitad son las paredes de borde y las
    /// estructuras, que las suma quien construye).
    ///
    /// Cuenta lo CONSTRUIDO —tuyo o del pueblo generado—, nunca el terreno pelado:
    /// sobre el césped no se empieza un piso flotante, sobre tu casa (o sobre la
    /// casa del mapa, que es justo lo que el jugador quiere ampliar) sí.
    pub fn supports_upper_floor(&self, body: CelestialBody, level: i32, tx: i64, tz: i64) -> bool {
        matches!(
            self.tile_at_level(body, level, tx, tz),
            Tile::WoodFloor
                | Tile::StoneFloor
                | Tile::StoneBrick
                | Tile::Pavement
                | Tile::WoodWall
                | Tile::Window
                | Tile::Door
                // Y el tramo de escalera: sostiene su propio rellano, que es lo
                // único que hace que una escalera lleve a algún sitio.
                | Tile::Step
                | Tile::Stairwell
        )
    }

    pub fn solid_at(&self, body: CelestialBody, tx: i64, tz: i64) -> bool {
        self.tile_at(body, tx, tz).solid()
    }

    /// ¿Esta celda la editó el jugador (picó, rellenó, construyó)? Lo autorado a
    /// mano manda sobre cualquier regla del terreno procedural.
    pub fn is_edited(&self, body: CelestialBody, tx: i64, tz: i64) -> bool {
        self.edits.contains_key(&(body as u8, self.z, tx, tz))
    }

    /// **¿Esta celda frena a alguien que va a PIE?**
    ///
    /// Igual que `solid_at`, salvo por los acantilados: uno de un solo escalón es
    /// un bordillo y se trepa o se baja de un impulso, en cualquier dirección y
    /// también por las esquinas (ver `relief::climbable_edge`). Los de dos
    /// escalones o más siguen siendo pared y hay que ir a buscar el paso.
    ///
    /// Que la excepción viva acá y no en `Tile::solid()` importa: picar, construir
    /// y la navegación de vehículos siguen viendo el acantilado como lo que es —
    /// esto sólo afecta a quien camina. Y una celda que EDITASTE nunca se relaja:
    /// si pusiste roca ahí, es roca.
    pub fn walk_blocked(&self, body: CelestialBody, tx: i64, tz: i64) -> bool {
        let t = self.tile_at(body, tx, tz);
        if !t.solid() {
            return false;
        }
        t != Tile::Cliff
            || self.z != 0
            || self.is_edited(body, tx, tz)
            || !ha_shared::relief::climbable_edge(body, tx, tz)
    }

    pub fn break_tile(&mut self, body: CelestialBody, tx: i64, tz: i64) -> Option<Tile> {
        let t = self.tile_at(body, tx, tz);
        // El líquido no se pica (ni el bajío ni el fondo): se rellena colocando
        // bloques encima, que es lo que `place_tile` sí permite.
        if matches!(t, Tile::Bedrock | Tile::Air | Tile::Water | Tile::WaterDeep) {
            return None;
        }
        // Qué queda cuando sacás lo que había. En superficie, tierra; bajo tierra,
        // suelo de cueva.
        //
        // **Arriba queda VACÍO, no suelo de cueva.** Picar la losa de tu segundo
        // piso tiene que abrir un agujero por el que se cae —eso es sacar el
        // piso—; dejar `CaveFloor` te dejaba caminando por el aire sobre una
        // planta que ya no existe.
        let revealed = if self.z == 0 {
            Tile::Dirt
        } else if self.z > 0 {
            Tile::Air
        } else {
            Tile::CaveFloor
        };
        let was_floor = ha_shared::build_grid::is_build_floor(t);
        self.edits
            .insert((body as u8, self.z, tx, tz), revealed);
        self.version = self.version.wrapping_add(1);
        self.dirty = true;
        self.edit_log.push((tx, tz));
        if was_floor {
            self.removed_floors.push((body as u8, self.z, tx, tz));
        }
        self.maybe_schedule_regrow(body, tx, tz);
        Some(t)
    }

    /// **Cavar hacia abajo** — el mundo es voxel (biblia §5).
    ///
    /// Picar ([`Self::break_tile`]) saca el bloque que tenés AL LADO y te deja la
    /// celda abierta en tu mismo nivel: es cavar en horizontal, y es lo único que
    /// existía. Cavar saca el suelo que PISÁS: la celda queda en [`Tile::Air`]
    /// —el hueco, que se ve y por el que se cae— y el nivel de abajo se abre en
    /// esa misma celda, que es la **cavidad**. De ahí sale la verticalidad
    /// entera: bajar es cavar, no encontrar la boca de cueva que el mundo puso.
    ///
    /// Lo que hace y no se ve: **talla la cavidad** si abajo hay roca maciza. Sin
    /// esto caerías dentro de la piedra, porque el nivel de abajo es un grid
    /// propio y su celda podía estar sólida.
    ///
    /// **Del pozo se sale TREPANDO**, no por una grada que aparece sola. Antes el
    /// fondo se estampaba con un tile-grada que al pisarlo te subía el pozo entero
    /// de un salto; era el teletransporte que se borró. Ahora se camina contra el
    /// borde y se trepa un nivel, como se sale de cualquier bordillo
    /// (`escaleras::trepar_borde`).
    ///
    /// No se cava el líquido (se rellena), ni el vacío (ya es hueco), ni contra
    /// el lecho de roca del fondo de la mina.
    pub fn dig_down(&mut self, body: CelestialBody, tx: i64, tz: i64) -> Option<Tile> {
        let t = self.tile_at(body, tx, tz);
        if !t.diggable_down() {
            return None;
        }
        let below = self.z - 1;
        // Contra el fondo no se cava: `MIN_Z_LEVEL` y más abajo es lecho de roca.
        if below <= MIN_Z_LEVEL {
            return None;
        }
        let under = self.tile_at_level(body, below, tx, tz);
        if under.solid() {
            self.edits.insert((body as u8, below, tx, tz), Tile::CaveFloor);
            self.edit_log.push((tx, tz));
        }
        let was_floor = ha_shared::build_grid::is_build_floor(t);
        self.edits.insert((body as u8, self.z, tx, tz), Tile::Air);
        self.version = self.version.wrapping_add(1);
        self.dirty = true;
        self.edit_log.push((tx, tz));
        if was_floor {
            self.removed_floors.push((body as u8, self.z, tx, tz));
        }
        Some(t)
    }

    pub fn place_tile(&mut self, body: CelestialBody, tx: i64, tz: i64, kind: Tile) -> bool {
        let cur = self.tile_at(body, tx, tz);
        // Sobre LÍQUIDO sí se coloca: rellenar un charco y tender un pontón sobre
        // el mar hondo son dos de las cosas que uno espera poder hacer con un
        // montón de piedra en la mochila. Sobre roca o pared, no.
        if cur.solid() && cur != Tile::WaterDeep {
            return false;
        }
        if ha_shared::build_grid::is_build_floor(cur) && !ha_shared::build_grid::is_build_floor(kind) {
            self.removed_floors.push((body as u8, self.z, tx, tz));
        }
        self.edits.insert((body as u8, self.z, tx, tz), kind);
        self.version = self.version.wrapping_add(1);
        self.dirty = true;
        self.edit_log.push((tx, tz));
        // Tierra colocada sobre césped natural también termina en pasto (el
        // césped "se come" la tierra con el tiempo, como en Minecraft).
        if kind == Tile::Dirt {
            self.maybe_schedule_regrow(body, tx, tz);
        }
        true
    }

    /// Si la celda quedó con edit `Dirt` en superficie y su tile NATURAL es
    /// césped (sin edificio generado encima), programa el rebrote con un delay
    /// pseudo-aleatorio por celda (45–120 s) para que no sea uniforme.
    fn maybe_schedule_regrow(&mut self, body: CelestialBody, tx: i64, tz: i64) {
        if self.z != 0 {
            return;
        }
        if self.edits.get(&(body as u8, 0, tx, tz)) != Some(&Tile::Dirt) {
            return;
        }
        if ha_shared::topdown::building_cell(body, tx, tz).is_some() {
            return;
        }
        // Natural = lo que habría sin ediciones. Sólo el césped rebrota.
        if topdown_tile(body, 0, tx, tz) != Tile::Grass {
            return;
        }
        if self
            .regrow
            .iter()
            .any(|r| r.body == body as u8 && (r.tx, r.tz) == (tx, tz))
        {
            return;
        }
        let delay = 45.0 + (hash_cell(0x9e9e_0001, tx, tz) % 7500) as f32 * 0.01;
        self.regrow.push(RegrowCell { body: body as u8, tx, tz, timer: delay });
    }

    /// Overwrite a cell at the active z UNCONDITIONALLY (even over solid rock).
    /// Used to carve the ship interior room; deliberately does NOT mark `dirty`
    /// (interior tiles are derived on boarding, never persisted — see the
    /// `SHIP_INTERIOR_Z` filter in `debounced_tiles_save`).
    pub fn force_tile(&mut self, body: CelestialBody, tx: i64, tz: i64, kind: Tile) {
        self.edits.insert((body as u8, self.z, tx, tz), kind);
        self.version = self.version.wrapping_add(1);
    }

    /// Escribe un tile **autorado en el editor de mundo** en su capa `z` (no en
    /// la activa) y sin condiciones. `place_tile` no sirve para esto: se niega
    /// sobre celdas sólidas (no podrías reemplazar roca por pasto), escribe
    /// siempre en la capa activa (el subsuelo terminaba pisando la superficie) y
    /// programa el rebrote de césped, que a los ~60 s deshacía tu diseño.
    pub fn author_tile(&mut self, body: CelestialBody, z: i32, tx: i64, tz: i64, kind: Tile) {
        self.edits.insert((body as u8, z, tx, tz), kind);
        self.version = self.version.wrapping_add(1);
    }

    /// Todas las celdas editadas (cuerpo, nivel, tx, tz) → tile. Lo lee el
    /// multijugador para ver qué cambió desde la última vez (`mundo_compartido`).
    pub fn ediciones(&self) -> &HashMap<(u8, i32, i64, i64), Tile> {
        &self.edits
    }

    /// **Una celda que cambió en el mundo de OTRO jugador de la sala.** Pisa sin
    /// condiciones (el otro ya validó contra su mundo, que es el mismo), se guarda
    /// con la partida y refresca el arte de los vecinos. `None` = la celda vuelve
    /// a lo que da la semilla. Si era una losa y deja de serlo, lo de arriba se
    /// derrumba igual que si la hubieras picado vos.
    pub fn aplicar_remoto(&mut self, key: (u8, i32, i64, i64), tile: Option<Tile>) {
        let antes = self.edits.get(&key).copied();
        if antes == tile {
            return;
        }
        match tile {
            Some(t) => {
                self.edits.insert(key, t);
            }
            None => {
                self.edits.remove(&key);
            }
        }
        let era_losa = antes.is_some_and(ha_shared::build_grid::is_build_floor);
        if era_losa && !tile.is_some_and(ha_shared::build_grid::is_build_floor) {
            self.removed_floors.push(key);
        }
        self.version = self.version.wrapping_add(1);
        self.dirty = true;
        self.edit_log.push((key.2, key.3));
    }
}

/// Celdas que el usuario AUTORÓ en el editor de mundo y en las que el juego no
/// debe sembrar decor procedural: donde pintaste terreno (lo tuyo reemplaza al
/// mundo) y donde borraste decor a mano. Sin editor de mundo está vacío y no
/// cuesta nada. Lo llena `world_edits::apply_world_edits`.
#[derive(Resource, Default)]
pub struct DecorFreeCells(std::collections::HashSet<(u8, i32, i64, i64)>);

impl DecorFreeCells {
    pub fn set(&mut self, cells: std::collections::HashSet<(u8, i32, i64, i64)>) {
        self.0 = cells;
    }

    /// ¿Esta celda quedó libre de decor procedural?
    pub fn contains(&self, body: CelestialBody, z: i32, tx: i64, tz: i64) -> bool {
        !self.0.is_empty() && self.0.contains(&(body as u8, z, tx, tz))
    }
}

// ---------------------------------------------------------------------------
// Decor (trees/rocks) + surface deposits — top-down, per grid cell
// ---------------------------------------------------------------------------

// Generación DETERMINISTA de decor extraída a `ha_shared::worldgen` (fuente ÚNICA para el
// juego y el editor de mundo). Re-exportada acá para no tocar los call sites internos.
pub use ha_shared::worldgen::{
    hash_cell, nature_decor_at, pick_tree_species_in, prop_decor_at, surface_decor_at, DecorKind,
};

/// ¿La decoración de esta celda bloquea el paso del jugador? Default por tipo
/// (árbol = sí, para no pisar el tronco; roca/flora = no, son props chicos),
/// pero el usuario puede FIJARLO por asset con la casilla "bloquea el paso" del
/// editor (`collides` en el manifest). Determinista → coincide con lo que
/// `spawn_chunk_decor` colocó. Primer paso hacia la colisión por-celda precisa
/// (relieves/cráteres) — ver `biblia.md` §8.
pub fn decor_blocks_walk(
    art: &crate::art2d::ArtLibrary,
    body: CelestialBody,
    tx: i64,
    tz: i64,
) -> bool {
    // La celda entera como "cápsula": si alguna caja del decor la toca, bloquea.
    let cell = [
        tx as f32 * TILE_M,
        tz as f32 * TILE_M,
        (tx + 1) as f32 * TILE_M,
        (tz + 1) as f32 * TILE_M,
    ];
    decor_hits_box(art, None, None, body, 0, cell)
}

/// ¿La caja `[x0, z0, x1, z1]` (metros) choca con la colisión de algún decor?
///
/// Ésta es la colisión FINA: cada asset aporta las cajas de su huella pintada en
/// el editor (`CollisionFootprint`, sub-celdas de hasta ¼ de tile), no el tile
/// entero. Un farol delgado frena sólo en su poste y pasar rozando un árbol deja
/// de sentirse un bug. Sin huella pintada se cae al comportamiento de siempre:
/// una celda completa, según `collides` o el default por tipo.
///
/// `authored` = decor colocado en el editor de mundo (`None` para consultar sólo
/// el procedural). `flora` = lo ya cosechado, que deja de estorbar (talar un árbol
/// abre el paso). Determinista → coincide con lo que `spawn_chunk_decor` puso.
pub fn decor_hits_box(
    art: &crate::art2d::ArtLibrary,
    authored: Option<&AuthoredDecor>,
    flora: Option<&FloraHarvest>,
    body: CelestialBody,
    z: i32,
    aabb: [f32; 4],
) -> bool {
    // Celdas ancla candidatas: las que toca la caja, más el alcance de las huellas
    // multi-celda (un asset anclado lejos puede tener una caja acá). Sin huellas
    // declaradas el escaneo es la vecindad inmediata — el costo de antes.
    if (z == 0 || ha_shared::interiores::es_interior(z)) && muebles_frenan(flora, body, z, aabb) {
        return true;
    }
    // **El alcance depende de QUIÉN se siembra.** Lo procedural (árboles y rocas) casi no tiene
    // huella: escanear con el alcance de la casa autorada más grande (7 celdas) eran ~300
    // celdas por pregunta, y cada vecino pregunta varias veces por cuadro (medido en el
    // Redmi: `ob_decor` = 10-20 ms). Lo autorado, que sí puede ser grande, se busca por lista.
    let rp: i64 = art.decor_reach();
    let x0 = (aabb[0] / TILE_M).floor() as i64 - rp;
    let x1 = (aabb[2] / TILE_M).floor() as i64 + rp;
    let z0 = (aabb[1] / TILE_M).floor() as i64 - rp;
    let z1 = (aabb[3] / TILE_M).floor() as i64 + rp;
    for cz in z0..=z1 {
        for cx in x0..=x1 {
            if decor_cell_hits(art, authored, flora, body, z, cx, cz, aabb) {
                return true;
            }
        }
    }
    if let Some(a) = authored.filter(|a| !a.is_empty()) {
        let reach = art.footprint_reach();
        let (ax0, ax1) = ((aabb[0] / TILE_M).floor() as i64 - reach, (aabb[2] / TILE_M).floor() as i64 + reach);
        let (az0, az1) = ((aabb[1] / TILE_M).floor() as i64 - reach, (aabb[3] / TILE_M).floor() as i64 + reach);
        for d in &a.list {
            if d.body == body as u8
                && d.z == z
                && (ax0..=ax1).contains(&d.x)
                && (az0..=az1).contains(&d.zc)
                && decor_cell_hits(art, authored, flora, body, z, d.x, d.zc, aabb)
            {
                return true;
            }
        }
    }
    false
}

/// ¿El dibujo de un decor de `size` metros anclado en `(tx, tz)` (pie en el borde
/// sur de la celda, centrado en ella) pisa alguna celda de edificio?
fn pisa_un_edificio(tiles: &TileWorld, body: CelestialBody, tx: i64, tz: i64, size: [f32; 2]) -> bool {
    let medio = size[0] * 0.5;
    let (x0, x1) = ((tx as f32 + 0.5 - medio).floor() as i64, (tx as f32 + 0.5 + medio - 0.01).floor() as i64);
    let (z0, z1) = ((tz as f32 + 1.0 - size[1]).floor() as i64, tz);
    (z0..=z1).any(|z| {
        (x0..=x1).any(|x| {
            matches!(
                tiles.tile_at(body, x, z),
                Tile::WoodFloor | Tile::WoodWall | Tile::StoneBrick | Tile::Window | Tile::Door
            )
        })
    })
}

/// **Los muebles frenan** (29 sep 2026): su pie, medido por `amueblado`. Antes no
/// tenían colisión y se atravesaban camas y roperos. Lo que el jugador levantó ya
/// no está y no frena.
fn muebles_frenan(flora: Option<&FloraHarvest>, body: CelestialBody, z: i32, aabb: [f32; 4]) -> bool {
    ha_shared::amueblado::muebles_cerca_z(body, z, aabb).iter().any(|m| {
        let (ax, az) = m.ancla();
        let (kx, kz) = crate::targeting::PROP_REMOVED_KEY;
        let levantado = flora.is_some_and(|f| f.is_removed(body, kx, kz, pack_cell(ax, az)));
        !levantado && overlaps(m.caja(), aabb)
    })
}

/// Cajas de colisión del decor anclado en `(cx, cz)` contra `aabb`.
fn decor_cell_hits(
    art: &crate::art2d::ArtLibrary,
    authored: Option<&AuthoredDecor>,
    flora: Option<&FloraHarvest>,
    body: CelestialBody,
    z: i32,
    cx: i64,
    cz: i64,
    aabb: [f32; 4],
) -> bool {
    // Lo autorado en el editor de mundo gana sobre el procedural (misma regla que
    // el dibujo). Sólo bloquea si el asset lo declara: huella pintada o la casilla
    // "bloquea el paso" — así plantar flores decorativas no levanta un muro.
    if let Some(d) = authored.and_then(|a| a.at(body, z, cx, cz)) {
        let keys = [d.key.as_str()];
        if let Some(fp) = art.collision_footprint(&keys) {
            // La huella se pintó sobre el sprite a su tamaño natural; si lo
            // escalaste al colocarlo, la colisión escala con él.
            let (sx, sz) = match d.size {
                Some([w, h]) if fp.w > 0 && fp.h > 0 => (w / fp.w as f32, h / fp.h as f32),
                _ => (1.0, 1.0),
            };
            return footprint_hits(fp, cx, cz, (sx, sz), aabb);
        }
        // Sin huella pintada, el default sale de la CAPA del slot: una pared
        // frena, un piso y un techo no. Antes el default era "no frena" para
        // todo, asi que una `wall/*` plantada desde el editor de mundo se
        // atravesaba caminando hasta que alguien se acordaba de tildarle la
        // casilla — el mismo bug que la capa por namespace resuelve del lado del
        // dibujo. El override explicito del editor sigue ganando.
        let por_capa = matches!(art.slot_layer(&d.key), Some(ha_assets::ArtLayer::Wall));
        return art.collides_override(&keys).unwrap_or(por_capa) && cell_hits(cx, cz, aabb);
    }
    if z != 0 {
        return false;
    }
    // Lo que ya talaste/picaste no frena a nadie.
    if flora.is_some_and(|f| f.is_removed(body, 0, 0, pack_cell(cx, cz))) {
        return false;
    }
    match surface_decor_at(body, cx, cz) {
        Some(DecorKind::Tree) => {
            let key = pick_tree_species_in(body, cx, cz).key;
            match art.collision_footprint(&[key]) {
                Some(fp) => footprint_hits(fp, cx, cz, (1.0, 1.0), aabb),
                // Sin huella: el tronco ocupa su celda (default histórico).
                None => art.collides_override(&[key]).unwrap_or(true) && cell_hits(cx, cz, aabb),
            }
        }
        Some(DecorKind::Rock) => {
            // Sin `format!`: esto corre por celda y por tick al caminar.
            let rock_key = match body_art_suffix(body) {
                "_moon" => "decor/rock_moon",
                "_mars" => "decor/rock_mars",
                _ => "decor/rock",
            };
            let keys = [rock_key, "decor/rock"];
            match art.collision_footprint(&keys) {
                Some(fp) => footprint_hits(fp, cx, cz, (1.0, 1.0), aabb),
                None => art.collides_override(&keys).unwrap_or(false) && cell_hits(cx, cz, aabb),
            }
        }
        None => false,
    }
}

/// ¿Alguna caja de la huella anclada en `(cx, cz)` (escalada `scale`) toca `aabb`?
fn footprint_hits(
    fp: &ha_assets::CollisionFootprint,
    cx: i64,
    cz: i64,
    scale: (f32, f32),
    aabb: [f32; 4],
) -> bool {
    let (ox, oz) = (cx as f32 * TILE_M, cz as f32 * TILE_M);
    fp.solid_rects().any(|r| {
        let bx0 = ox + r[0] * scale.0 * TILE_M;
        let bz0 = oz + r[1] * scale.1 * TILE_M;
        let bx1 = ox + r[2] * scale.0 * TILE_M;
        let bz1 = oz + r[3] * scale.1 * TILE_M;
        overlaps([bx0, bz0, bx1, bz1], aabb)
    })
}

fn cell_hits(cx: i64, cz: i64, aabb: [f32; 4]) -> bool {
    let (x, z) = (cx as f32 * TILE_M, cz as f32 * TILE_M);
    overlaps([x, z, x + TILE_M, z + TILE_M], aabb)
}

fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}

/// ¿Se puede construir sobre esta celda? NO si tiene decor COSECHABLE aún
/// presente (árbol / roca / depósito de mineral): hay que talar o picar primero
/// — no se edifica encima (ej.: un cuarto 5×4 no cabe sobre un árbol). Los props
/// pequeños (flora visual sin `InteractTarget`) no bloquean. Determinista →
/// coincide con lo que `spawn_chunk_decor` colocó, y respeta lo ya cosechado.
pub fn cell_build_blocked(
    flora: &FloraHarvest,
    body: CelestialBody,
    z: i32,
    tx: i64,
    tz: i64,
) -> bool {
    // Terreno que no es suelo firme: líquido (el bajío se camina pero no se
    // cimenta) y rampas (son el paso de una terraza a otra, taparlas sellaría el
    // relieve). Va ANTES de la comprobación de cosecha porque no depende de ella:
    // haber talado el árbol no seca el lago.
    if z == 0 && !ha_shared::topdown::topdown_tile(body, 0, tx, tz).buildable_ground() {
        return true;
    }
    if flora.is_removed(body, 0, 0, pack_cell(tx, tz)) {
        return false;
    }
    // Mismo criterio que al spawnear (`spawn_chunk_decor`): el decor de superficie
    // existe también mientras cavás debajo, porque el mundo de arriba sigue ahí.
    (z <= 0 && surface_decor_at(body, tx, tz).is_some()) || deposit_at(body, z, tx, tz).is_some()
}

fn deposit_target_kind(d: Deposit) -> InteractTargetKind {
    match d {
        Deposit::OreIron => InteractTargetKind::OreIron,
        Deposit::OreGold => InteractTargetKind::OreGold,
        Deposit::GemBlue => InteractTargetKind::GemBlue,
        Deposit::GemRed => InteractTargetKind::GemRed,
    }
}

// ---------------------------------------------------------------------------
// Chunk streaming + meshing (2D grid)
// ---------------------------------------------------------------------------

#[derive(Component)]
pub struct TileChunk {
    pub body: CelestialBody,
    pub z: i32,
    pub cx: i64,
    pub cz: i64,
    pub built_version: u64,
    /// Cuánta decoración tiene poblada este chunk. Los chunks se crean con el
    /// nivel que les toca por distancia y lo SUBEN al acercarse (bajaste volando):
    /// ver `stream_tile_chunks` y [`DecorDetail`].
    pub decor: DecorDetail,
}

/// Cuánto detalle de decoración lleva un chunk. Es un nivel ORDENADO: subir de
/// nivel repuebla el chunk, bajar no hace nada (lo que sobra ya se ve como una
/// mota y el fundido de LOD lo está apagando igual).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub enum DecorDetail {
    /// Sin nada: fuera del alcance de la capa fina.
    #[default]
    None,
    /// Lo que se lee desde el aire: árboles, piedras, vetas y portales.
    Coarse,
    /// Todo: además, la flora chica, los props de adorno y el arte de tile
    /// (agua, autotiles, piezas construidas). Sólo de cerca, porque son las dos
    /// categorías con MÁS entidades por chunk —el arte de tile puede llegar a una
    /// por celda, 256 en un chunk de costa— y a distancia miden un píxel. El
    /// color del agua y de las piezas ya lo pinta la malla del chunk
    /// (`build_chunk_mesh`), así que lejos no falta nada: falta el brillo.
    Full,
}

#[derive(Component)]
pub struct SurfaceDecor {
    pub cx: i64,
    pub cz: i64,
    pub z: i32,
    /// Flora/props de adorno: es la categoría más numerosa y la primera que se
    /// tira cuando el chunk sólo llega a [`DecorDetail::Coarse`]. Árboles,
    /// piedras, vetas y portales van con `false` (viven en todo el anillo).
    pub small: bool,
}

#[derive(Resource, Default)]
pub struct ChunkMaterial(pub Handle<ColorMaterial>);

/// Fundido al cambiar de capa (subir/bajar un piso, entrar en una cueva).
///
/// El cambio de capa NUNCA mueve al jugador ni carga una escena: caminás sobre
/// la boca de la cueva o la grada y ya estás abajo, en las mismas coordenadas.
/// Lo único que hacía falta era que el mundo nuevo APAREZCA en vez de aparecer
/// de golpe.
///
/// **Son dos fundidos y no uno**, porque hay dos maneras de cambiar de planta y
/// duran cosas distintas:
///
/// - [`Self::pulso`] — **caer** por un hueco. Es instantáneo por definición, así
///   que lo único posible es un cuarto de segundo por detrás para que el ojo siga
///   la caída. Se dispara y decae solo.
/// - [`Self::tramo`] — **andar una escalera**. Acá no hay nada que perseguir: el
///   cambio dura lo que dura el paso, así que el fundido es una **función de dónde
///   estás en el tramo** ([`ha_shared::escaleras::meseta_del_tramo`]) y no un
///   temporizador. Subiendo, la planta de abajo se desvanece mientras subís y la
///   de arriba entra ya entera al llegar al rellano; bajando, al revés. Y como es
///   función de la posición, **desandar medio escalón devuelve el dibujo**: no es
///   una animación disparada que haya que esperar a que termine.
///
/// Se combinan con `max` y no sumando: son dos formas de decir "esto se está
/// yendo", no dos velos apilados.
#[derive(Resource, Default)]
pub struct LayerFade {
    pub pulso: f32,
    pub tramo: f32,
}

impl LayerFade {
    /// Cuánto se esconde la planta que se está dibujando, de 0 (entera) a 1.
    pub fn total(&self) -> f32 {
        self.pulso.max(self.tramo)
    }
}

/// Duración del fundido de capa al CAER, en segundos.
const LAYER_FADE_SECS: f32 = 0.28;

/// **Cuánto llega a desvanecerse la planta en mitad de un tramo.**
///
/// No es 1.0 a propósito. A uno la planta se borra del todo y lo que queda es el
/// color de fondo de la cámara — un fotograma de vacío, que es exactamente la
/// "cortina" que §4 prohíbe. A 0.85 se lee como un disolvido: la planta que se va
/// queda apenas insinuada justo en el frame en que la otra la reemplaza.
pub const FUNDIDO_TRAMO: f32 = 0.85;

/// Decor AUTORADO (árboles/props colocados a mano en el editor de mundo, `world_edits.json`).
/// Se pinta junto al decor procedural al streamear cada chunk. Lo llena `world_edits.rs`.
#[derive(Resource, Default)]
pub struct AuthoredDecor {
    pub list: Vec<ha_shared::world_edits::PlacedDecor>,
    /// `(cuerpo, z, x, zc)` → índice en `list`. La colisión lo consulta por celda
    /// varias veces por frame; recorrer el vector entero no escala.
    by_cell: std::collections::HashMap<(u8, i32, i64, i64), usize>,
}

impl AuthoredDecor {
    pub fn set(&mut self, list: Vec<ha_shared::world_edits::PlacedDecor>) {
        self.by_cell = list
            .iter()
            .enumerate()
            .map(|(i, d)| ((d.body, d.z, d.x, d.zc), i))
            .collect();
        self.list = list;
    }

    /// El asset autorado en esa celda, si hay uno.
    pub fn at(
        &self,
        body: CelestialBody,
        z: i32,
        tx: i64,
        tz: i64,
    ) -> Option<&ha_shared::world_edits::PlacedDecor> {
        if self.by_cell.is_empty() {
            return None;
        }
        self.by_cell.get(&(body as u8, z, tx, tz)).map(|&i| &self.list[i])
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}

pub struct World2dPlugin;

impl Plugin for World2dPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TileWorld>()
            .init_resource::<ActivePlanet>()
            .init_resource::<ChunkBudget>()
            .init_resource::<ChunkBacklog>()
            .init_resource::<SmallDecorQueue>()
            .init_resource::<ChunkMaterial>()
            .init_resource::<LayerFade>()
            .init_resource::<AuthoredDecor>()
            .init_resource::<DecorFreeCells>()
            .init_resource::<FineDetailOff>()
            .add_systems(
                Startup,
                (init_chunk_material, spawn_cursor_highlight),
            )
            .add_systems(OnEnter(crate::game_phase::GamePhase::Loading), load_tiles_on_startup)
            .add_systems(
                Update,
                (
                    sync_chunk_material,
                    apply_tile_hold.before(crate::hud::update_interact_hud),
                    try_place_tile_on_hold.before(crate::hud::update_interact_hud),
                    fall_through_void,
                    tick_grass_regrowth,
                    stream_tile_chunks,
                    rebuild_dirty_chunks,
                    rebuild_edited_tile_art.after(rebuild_dirty_chunks),
                    // Colonos "tontos" reemplazados por NPCs con cerebro + memoria
                    // (`society.rs`, F6). `sync_colonists` queda inactivo.
                    sync_cursor_highlight,
                    canopy_translucency,
                    fade_surface_lod.after(canopy_translucency),
                    debounced_tiles_save,
                )
                    // Todo esto necesita un jugador en el mundo, y el jugador
                    // recién nace al entrar a `Loading` (`scene::spawn_player`).
                    // Antes corrían igual en el splash y el menú: cada uno
                    // resolvía su query, no encontraba jugador y salía — barato
                    // por sistema, pero son doce, todos los frames, mientras la
                    // pantalla de carga es justo donde el arranque va más justo.
                    .run_if(crate::game_phase::world_active),
            );
    }
}

/// Tree canopies fade to translucent when the player walks BEHIND them, so a
/// big crown never hides the character (gamer UX). A canopy occludes (draws over)
/// the player by Y-sort when the tree is more "south" (larger sort z); in that
/// case, if the player overlaps the crown box on screen, we lower the sprite's
/// alpha smoothly and restore it when they step clear. Only nearby trees are in
/// the world (chunk streaming), so this is cheap.
fn canopy_translucency(
    time: Res<Time>,
    player: Query<&Transform, With<crate::sim::Player>>,
    mut trees: Query<(&Transform, &mut Sprite, &InteractTarget), Without<crate::sim::Player>>,
) {
    let Ok(pt) = player.get_single() else { return };
    let pp = pt.translation;
    let rate = (time.delta_secs() * 9.0).min(1.0);
    for (tt, mut sprite, target) in &mut trees {
        if target.kind != InteractTargetKind::Tree {
            continue;
        }
        let size = sprite.custom_size.unwrap_or(Vec2::splat(1.0));
        // Crown box (render space): the sprite spans `size` centred on its y.
        let overlap = (pp.x - tt.translation.x).abs() < size.x * 0.5
            && (pp.y - tt.translation.y).abs() < size.y * 0.5;
        // Player is BEHIND when it sorts under the tree (tree draws over it).
        let behind = pp.z < tt.translation.z;
        let target_a = if overlap && behind { 0.42 } else { 1.0 };
        let a = sprite.color.alpha();
        sprite.color.set_alpha(a + (target_a - a) * rate);
    }
}

/// Corchetes amarillos sobre lo que estás TOCANDO — el único adorno que quedó
/// del viejo sistema de mira, y sólo mientras el dedo está apoyado: es la
/// confirmación de "acá va a pasar", no un cursor que vive en pantalla.
///
/// **Abrazan al OBJETO, no a la celda** (15 ago 2026). Con la celda de 1×1 el
/// resaltado nunca contestaba la pregunta que el jugador hace de verdad: entre
/// tres árboles juntos, ¿cuál agarré? Los corchetes se estiran ahora al
/// rectángulo del sprite apuntado, así que el árbol elegido se ve elegido. Sin
/// objetivo con cuerpo —cavar, colocar un bloque— vuelven a la celda, que ahí sí
/// es lo que va a pasar.
#[derive(Component)]
struct CursorHighlight;

/// Un brazo del corchete. Guarda EN QUÉ esquina vive y si corre horizontal, que
/// es todo lo que hace falta para recolocarlo cuando el rectángulo cambia de
/// tamaño. Los brazos no se escalan con el padre a propósito: escalar el nodo
/// engorda el trazo, y sobre una casa de cuatro tiles el corchete pasaba a ser
/// un marco gordo en vez de una marca fina.
#[derive(Component, Clone, Copy)]
struct CursorHighlightArm {
    cx: f32,
    cz: f32,
    horizontal: bool,
}

/// Largo del brazo del corchete (m) y su grosor.
const BRACKET_ARM: f32 = 0.28;
const BRACKET_THICK: f32 = 0.09;

fn spawn_cursor_highlight(mut commands: Commands) {
    let yellow = Color::srgb(1.0, 0.9, 0.15);
    // Cuatro corchetes en L; nacen sobre una celda de 1×1 (esquinas en ±0.5) y
    // `sync_cursor_highlight` los reubica según lo que haya bajo el dedo.
    commands
        .spawn((
            CursorHighlight,
            Transform::from_xyz(0.0, 0.0, crate::layers::OBRA),
            Visibility::Hidden,
            InheritedVisibility::default(),
            ViewVisibility::default(),
        ))
        .with_children(|p| {
            for (cx, cz) in [(-1.0, 1.0), (1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
                let corner = Vec2::new(cx * 0.5, cz * 0.5);
                p.spawn((
                    Sprite::from_color(yellow, Vec2::new(BRACKET_ARM, BRACKET_THICK)),
                    Transform::from_xyz(corner.x - cx * BRACKET_ARM * 0.5, corner.y, 0.0),
                    CursorHighlightArm { cx, cz, horizontal: true },
                ));
                p.spawn((
                    Sprite::from_color(yellow, Vec2::new(BRACKET_THICK, BRACKET_ARM)),
                    Transform::from_xyz(corner.x, corner.y - cz * BRACKET_ARM * 0.5, 0.0),
                    CursorHighlightArm { cx, cz, horizontal: false },
                ));
            }
        });
}

#[allow(clippy::type_complexity)]
fn sync_cursor_highlight(
    cursor: Res<crate::input::AimCursor>,
    hold: Res<crate::input::HoldInteractState>,
    region: Res<RegionRuntime>,
    target: Res<crate::targeting::LookTarget>,
    cuerpos: Query<(&GlobalTransform, &Sprite), Without<CursorHighlightArm>>,
    mut hl: Query<(&mut Transform, &mut Visibility), With<CursorHighlight>>,
    mut brazos: Query<(&mut Transform, &mut Sprite, &CursorHighlightArm), Without<CursorHighlight>>,
) {
    let Ok((mut tf, mut vis)) = hl.get_single_mut() else {
        return;
    };
    // Se sostiene mientras dura la acción aunque el dedo se levante justo al
    // final: si no, el resaltado parpadea en el último frame del hachazo.
    let showing = cursor.targeting() || hold.active;
    if surface_region_active(&region).is_none() || !showing {
        *vis = Visibility::Hidden;
        return;
    }
    *vis = Visibility::Visible;

    // ¿Hay un CUERPO apuntado? Entonces el resaltado es suyo: centro y medio
    // tamaño salen del sprite real, así que sigue a lo animado y a lo que cambia
    // de pose sin que haya que avisarle nada.
    let cuerpo = target
        .entity
        .and_then(|e| cuerpos.get(e).ok())
        .and_then(|(gtf, s)| s.custom_size.map(|size| (gtf.translation().truncate(), size)));

    let (centro, half) = match cuerpo {
        Some((c, size)) => (c, size.abs() * 0.5),
        None => {
            // Sin cuerpo, la celda de siempre: es lo que se va a cavar o donde
            // se va a plantar el bloque.
            let tx = cursor.world.x.floor();
            let tz = cursor.world.y.floor();
            (render_xy(tx + 0.5, tz + 0.5), Vec2::splat(0.5))
        }
    };
    tf.translation = Vec3::new(centro.x, centro.y, 6.0);

    // Los brazos nunca se cruzan: en un prop de medio tile el corchete se acorta
    // en vez de dibujar una X.
    let arm = BRACKET_ARM.min(half.x.min(half.y) * 0.9).max(0.06);
    for (mut btf, mut sprite, brazo) in &mut brazos {
        let corner = Vec2::new(brazo.cx * half.x, brazo.cz * half.y);
        let (pos, size) = if brazo.horizontal {
            (
                Vec3::new(corner.x - brazo.cx * arm * 0.5, corner.y, 0.0),
                Vec2::new(arm, BRACKET_THICK),
            )
        } else {
            (
                Vec3::new(corner.x, corner.y - brazo.cz * arm * 0.5, 0.0),
                Vec2::new(BRACKET_THICK, arm),
            )
        };
        btf.translation = pos;
        sprite.custom_size = Some(size);
    }
}

fn init_chunk_material(
    mut mat: ResMut<ChunkMaterial>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    // Arranca OPACO, que es el estado de a pie. `fade_surface_lod` es el dueño
    // del modo de acá en más: lo pasa a Blend sólo mientras funde hacia el mapa
    // satelital, que es el único momento en que el alpha de este material baja
    // de 1.0. Ver el porqué del costo en esa función.
    mat.0 = materials.add(ColorMaterial {
        alpha_mode: bevy::sprite::AlphaMode2d::Opaque,
        ..default()
    });
}

/// **Le engancha al material del chunk la hoja del suelo**, una sola vez.
///
/// No se hace en `init_chunk_material` porque la textura la trae la carga
/// perezosa de `ArtLibrary` y en `Startup` todavía no hay a quién pedírsela. Con
/// el material ya texturizado, la malla del chunk dibuja el pixel del pack en el
/// mismo draw call de siempre — sin esto las UVs no tendrían de dónde leer.
fn sync_chunk_material(
    art: Res<ArtLibrary>,
    mat: Res<ChunkMaterial>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut hecho: Local<bool>,
) {
    if *hecho {
        return;
    }
    let Some(hoja) = art.suelo_pagina() else {
        return;
    };
    if let Some(m) = materials.get_mut(&mat.0) {
        m.texture = Some(hoja);
        *hecho = true;
    }
}

/// Funde la capa de tiles finos (suelo + decoración) hacia el mapa satelital a
/// medida que la cámara se aleja. Es la mitad visible de la continuidad
/// superficie↔espacio: sin esto se vería el BORDE del anillo streameado, que es
/// exactamente lo que antes había que tapar con una pantalla opaca de nubes.
#[allow(clippy::type_complexity)]
fn fade_surface_lod(
    time: Res<Time>,
    zoom: Res<crate::camera::CameraZoom>,
    mat: Res<ChunkMaterial>,
    mut layer: ResMut<LayerFade>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut was_fading: Local<bool>,
    mut decor: Query<&mut Sprite, Or<(With<SurfaceDecor>, With<TileArt>)>>,
) {
    // El traspaso fino <-> satelital: es el momento exacto que el usuario reporta
    // como "al salir al espacio da un tirón en cada cambio".
    let _probe = crate::debug::probe("fade_surface_lod");
    layer.pulso = (layer.pulso - time.delta_secs() / LAYER_FADE_SECS).max(0.0);
    let f = crate::universe::fine_tile_alpha(zoom.live_view_h) * (1.0 - layer.total());
    let fading = f < 0.999;
    // El suelo tapa la pantalla entera, así que su modo de mezcla es la cuenta
    // más cara del frame. A pie `f` vale 1.0: en Blend la GPU igual lee el
    // framebuffer por cada fragmento y no puede descartar los tapados, para
    // mezclar un alpha que no mezcla nada. Medido en el Redmi: 81 ms -> 28 ms
    // con sólo pasarlo a Opaque, a pixel idéntico (la malla no tiene textura,
    // su único alpha es el de esta línea). Ablación ronda 3, biblia §10.
    //
    // El modo se escribe SÓLO al cruzar el umbral: cambiarlo respecializa el
    // pipeline, y hacerlo cada frame sería un tirón permanente en vez de uno.
    let want_mode = if fading {
        bevy::sprite::AlphaMode2d::Blend
    } else {
        bevy::sprite::AlphaMode2d::Opaque
    };
    if let Some(m) = materials.get_mut(&mat.0) {
        m.color = Color::srgba(1.0, 1.0, 1.0, f);
        if m.alpha_mode != want_mode {
            m.alpha_mode = want_mode;
        }
    }
    // A pie (f == 1) no tocamos los sprites: `canopy_translucency` es la dueña
    // del alpha de los árboles. Sólo se hace una pasada final al volver.
    if !fading && !*was_fading {
        return;
    }
    for mut sprite in &mut decor {
        sprite.color.set_alpha(f);
    }
    *was_fading = fading;
}

pub fn surface_region_active(region: &RegionRuntime) -> Option<()> {
    match region.active {
        RegionId::Overworld { .. }
        | RegionId::Luna
        | RegionId::Planet(_)
        | RegionId::Cave(_)
        | RegionId::ShipInterior => Some(()),
        _ => None,
    }
}

fn apply_tile_hold(
    mut hold: ResMut<crate::input::HoldInteractState>,
    target: Res<crate::targeting::LookTarget>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    mut tiles: ResMut<TileWorld>,
    mut inv: ResMut<crate::inventory_persist::PlayerInventory>,
    mut persist: ResMut<crate::inventory_persist::InventoryPersistState>,
    mut sfx: EventWriter<crate::audio::PlaySfxAt>,
) {
    if !hold.completed || target.kind != Some(InteractTargetKind::Block) {
        return;
    }
    let Some((tx, tz, _)) = target.block_cell else {
        return;
    };
    hold.completed = false;
    let body = active_surface_body(&ctx, &planet);
    // Picar (de costado) y cavar (hacia abajo) son el MISMO gesto: lo que cambia
    // es qué había apuntado. Ver `TileWorld::dig_down`.
    let dug = if target.dig_down {
        tiles.dig_down(body, tx as i64, tz as i64)
    } else {
        tiles.break_tile(body, tx as i64, tz as i64)
    };
    if let Some(t) = dug {
        sfx.send(crate::audio::PlaySfxAt(
            crate::audio::Sfx::Dig,
            render_xy((tx as f32 + 0.5) * TILE_M, (tz as f32 + 0.5) * TILE_M),
        ));
        if let Some(item) = t.dug_item() {
            inv.add(item, 1);
            // Las VETAS (`materias.rs`): picar donde hay mineral a veces da una pizca de más.
            if let Some(extra) = crate::materias::extra_al_picar(ha_shared::partida::semilla(), t, tx as i64, tz as i64) {
                inv.add(extra, 1);
                crate::causal::registrar("veta", format!("{extra} al picar ({tx},{tz})"));
            }
            persist.mark_immediate();
        }
    }
}

fn try_place_tile_on_hold(
    mut hold: ResMut<crate::input::HoldInteractState>,
    target: Res<crate::targeting::LookTarget>,
    equipped: Res<crate::input::EquippedItem>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    mut tiles: ResMut<TileWorld>,
    mut inv: ResMut<crate::inventory_persist::PlayerInventory>,
    mut persist: ResMut<crate::inventory_persist::InventoryPersistState>,
    mut sfx: EventWriter<crate::audio::PlaySfxAt>,
) {
    if !hold.completed || !hold.placing_block {
        return;
    }
    let Some((tx, tz, _)) = target.place_cell else {
        hold.completed = false;
        return;
    };
    hold.completed = false;
    let Some(kind) = tile_for_block_item(&equipped.item_id) else {
        return;
    };
    if inv.qty(&equipped.item_id) == 0 {
        return;
    }
    let body = active_surface_body(&ctx, &planet);
    if tiles.place_tile(body, tx as i64, tz as i64, kind) {
        sfx.send(crate::audio::PlaySfxAt(
            crate::audio::Sfx::Place,
            render_xy((tx as f32 + 0.5) * TILE_M, (tz as f32 + 0.5) * TILE_M),
        ));
        inv.remove(&equipped.item_id, 1);
        persist.mark_immediate();
    }
}

/// **Por el hueco se CAE, en cualquier nivel.** Todo lo que es [`Tile::Air`] —el
/// vacío de tu piso alto, y también el pozo que cavaste en el pasto— te baja un
/// nivel en el acto, en el mismo `(x, z)`. Si abajo también hay hueco seguís
/// bajando hasta pisar algo.
///
/// Que valga para `z <= 0` es lo que conecta cavar con las cuevas **sin cambio de
/// escena** (§4): el pozo que abrís en la superficie te deja en la galería de
/// abajo sin cargar nada. No hay dos sistemas — hay un hueco.
///
/// Por qué CAER y no una pared invisible: la alternativa era hacer sólido el
/// vacío, y eso es exactamente el "atravesar/chocar con nada" que el jugador
/// reporta como bug. Un balcón sin baranda tiene que dejarte caer.
///
/// Sin daño: la caída es feedback (impulso del sprite, polvo, golpe y un tirón de
/// cámara, los mismos que descolgarse de un bordillo), no un castigo — misma
/// política que la "muerte amable" del survival.
fn fall_through_void(
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    fly: Res<crate::creative_fly::CreativeFlight>,
    mut tiles: ResMut<TileWorld>,
    mut fade: ResMut<LayerFade>,
    mut bob: ResMut<crate::vault::VaultBob>,
    mut sfx: EventWriter<crate::audio::PlaySfxAt>,
    mut shake: EventWriter<crate::camera::CameraShakeEvent>,
    player: Query<&SimState, With<Player>>,
) {
    // Volando no se cae: el piso alto es justo por donde entrás volando. Y en el
    // interior de la nave las celdas son derivadas, no un mundo con niveles.
    if tiles.z <= MIN_Z_LEVEL
        || fly.flying()
        || region.active == RegionId::ShipInterior
        || surface_region_active(&region).is_none()
    {
        return;
    }
    let Ok(sim) = player.get_single() else {
        return;
    };
    let body = active_surface_body(&ctx, &planet);
    let tx = (sim.body.x / TILE_M).floor() as i64;
    let tz = (sim.body.z / TILE_M).floor() as i64;
    if tiles.tile_at(body, tx, tz) != Tile::Air {
        return;
    }
    // Bajar hasta pisar algo (el lecho de roca del fondo siempre frena).
    let mut z = tiles.z;
    while z > MIN_Z_LEVEL && tiles.tile_at_level(body, z, tx, tz) == Tile::Air {
        z -= 1;
    }
    if z == tiles.z {
        return;
    }
    tiles.z = z;
    tiles.version = tiles.version.wrapping_add(1);
    // Caer cambia lo que se dibuja —de un piso alto y también hacia el subsuelo,
    // que desde el 7 ago se pinta como lo que es—, así que la planta nueva ENTRA
    // en vez de aparecer de golpe. No es una transición ni tapa nada: es un cuarto
    // de segundo para que el ojo siga la caída.
    fade.pulso = 1.0;
    bob.drop_impact();
    let feet = render_xy(sim.body.x, sim.body.z);
    sfx.send(crate::audio::PlaySfxAt(crate::audio::Sfx::DropLedge, feet));
    shake.send(crate::camera::CameraShakeEvent { trauma: 0.14 });
}

#[allow(clippy::too_many_arguments)]
fn stream_tile_chunks(
    mut commands: Commands,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    art: Res<ArtLibrary>,
    mat: Res<ChunkMaterial>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut flora: ResMut<FloraHarvest>,
    authored: Res<AuthoredDecor>,
    decor_free: Res<DecorFreeCells>,
    zoom: Res<crate::camera::CameraZoom>,
    player: Query<&Transform, With<Player>>,
    mut chunks: Query<(Entity, &mut TileChunk)>,
    decor: Query<(Entity, &SurfaceDecor)>,
    mut ctl: StreamCtl,
) {
    let _guard = crate::debug::span_guard(&ctl.span, crate::debug::CpuSpanSlot::Chunk);
    let _probe = crate::debug::probe("stream_tile_chunks");
    // Los tiles de 1 m sólo existen mientras se VEN. Pasado `FINE_OFF_VIEW_H` ya
    // los reemplazó el mapa satelital (`universe.rs`), así que streamear más
    // sería trabajo puro para nada — y era de donde salían los 19 k entities
    // volando con un anillo enorme que igual no llenaba la pantalla.
    // APAGADO CON HISTÉRESIS Y CON PRESUPUESTO — es la mitad del tirón al despegar.
    //
    // Antes esto era un `if` sin memoria que, en UN frame, hacía `despawn_recursive`
    // de hasta 225 chunks (con su malla de 256 quads cada uno) y de hasta
    // `MAX_LIVE_DECOR` sprites con sus hijos de luz. Había presupuesto para
    // CONSTRUIR (`CHUNK_BUILD_BUDGET_PER_FRAME`) y ninguno para DESTRUIR.
    //
    // Y sin histéresis: flotando justo en el umbral, un frame por encima y otro por
    // debajo tiraba y reconstruía el anillo entero, sin parar.
    let off = crate::universe::FINE_OFF_VIEW_H;
    let want_off = if ctl.fine_off.0 {
        // Ya apagado: para volver a encender hay que bajar bastante del umbral.
        zoom.live_view_h > off * 0.82
    } else {
        zoom.live_view_h > off
    };
    if surface_region_active(&region).is_none() || want_off {
        ctl.fine_off.0 = true;
        // Nada que construir acá: si quedara un backlog viejo anotado, la pantalla
        // de carga esperaría para siempre un anillo que no se va a armar.
        ctl.backlog.0 = 0;
        ctl.small_queue.0.clear();
        // Se destruye A PLAZOS, lo más lejano primero: el jugador ya no lo mira
        // (está subiendo) y el satelital de `universe.rs` ya cubre ese hueco.
        let mut budget = DECOR_DESPAWN_BUDGET_PER_FRAME;
        for (e, _) in &decor {
            if budget == 0 {
                break;
            }
            commands.entity(e).despawn_recursive();
            budget -= 1;
        }
        let mut cbudget = CHUNK_DESPAWN_BUDGET_PER_FRAME;
        for (e, chunk) in &chunks {
            if cbudget == 0 {
                break;
            }
            commands.entity(e).despawn_recursive();
            // El censo de árboles del chunk se va CON el chunk. Sin esto, cada
            // despegue dejaba en `FloraHarvest` la lista de todos los chunks que
            // acababa de tirar, y el mapa crecía sin freno durante toda la
            // partida — la ruta de salida más transitada del juego.
            flora.clear_chunk(chunk.cx as i32, chunk.cz as i32);
            cbudget -= 1;
        }
        return;
    }
    ctl.fine_off.0 = false;
    let Ok(player_tf) = player.get_single() else {
        return;
    };
    let body = active_surface_body(&ctx, &planet);
    let z = tiles.z;
    // Player render pos is (x, -z); recover grid chunk from world coords.
    let px = player_tf.translation.x;
    let pz = -player_tf.translation.y;
    let pcx = (px / (CHUNK as f32 * TILE_M)).floor() as i64;
    let pcz = (pz / (CHUNK as f32 * TILE_M)).floor() as i64;

    // El anillo sólo tiene que llenar la pantalla hasta `FINE_OFF_VIEW_H`; más
    // allá manda el satelital. Se ensancha con el zoom en vez de saltar a un
    // valor fijo por "estar volando". El ancho de pantalla en apaisado de móvil
    // es ~1.2× el alto por lado, de ahí el 1.25.
    let need_m = zoom.live_view_h * 1.25;
    // `visible` es lo que llena la pantalla; `range` le suma el margen que se
    // arma fuera de cuadro (ver `BUILD_MARGIN`) para que ningún chunk nazca a la
    // vista. `MAX_CHUNK_RANGE` sigue siendo el techo duro de todo el anillo.
    let visible =
        ((need_m / (CHUNK as f32 * TILE_M)).ceil() as i64).clamp(CHUNK_RANGE, MAX_CHUNK_RANGE);
    let range = (visible + BUILD_MARGIN).min(MAX_CHUNK_RANGE);

    // LA DECORACIÓN LLENA EL MISMO ANILLO QUE EL SUELO.
    //
    // Antes vivía sólo en `CHUNK_RANGE` (48 m) mientras los tiles se estiraban
    // hasta 192 m al volar: el resultado era un CUADRADO de bosque de 96 m de
    // lado con el terreno pelado alrededor — el borde de render que se veía al
    // despegar. Ahora árboles y piedras cubren exactamente lo que cubre el suelo,
    // y los tres (suelo + decor + arte de tile) se apagan JUNTOS con el fundido
    // de `fade_surface_lod`, que es lo que hace que al seguir subiendo el detalle
    // desaparezca sin que se note dónde termina.
    //
    // El coste se acota por otro lado: la flora chica (matas, hongos, props de
    // adorno) es lo MÁS numeroso por chunk y a esa distancia mide un píxel, así
    // que sólo se puebla en el anillo cercano (`in_near`). Lo que el jugador
    // nombró —árboles, piedras, vetas— va en todo el anillo (`in_decor`).
    // El nivel que le toca a un chunk. `Coarse` llega exactamente hasta `range`
    // — que YA incluye el margen de construcción (`BUILD_MARGIN`), o sea un
    // anillo más que lo que se ve. Antes decía `range + 1` para atarse al
    // envolvente de supervivencia y que un pelo de zoom no poblara/despoblara el
    // borde; ahora ese anillo de más existiría de verdad (el suelo llega hasta
    // `range`) y serían ~640 sprites de decoración extra por nada, con el borde
    // igual de fuera de cuadro. El anti-parpadeo sigue en pie por otro lado: el
    // nivel de un chunk NUNCA baja (ver el ascenso Coarse→Full más abajo), así
    // que cruzar el borde ida y vuelta no despuebla nada.
    let want_detail = |cx: i64, cz: i64| -> DecorDetail {
        let cheb = (cx - pcx).abs().max((cz - pcz).abs());
        if cheb <= DECOR_FULL_RANGE {
            DecorDetail::Full
        } else if cheb <= range {
            DecorDetail::Coarse
        } else {
            DecorDetail::None
        }
    };

    let mut budget = ctl.budget.count;
    // Mismo criterio que `universe::bake_body_textures`: el CONTEO es sólo el
    // techo por tanda, lo que manda es el RELOJ — un chunk cuesta 6-12 ms en el
    // Redmi en release y un orden de magnitud más en depuración o en una máquina
    // lenta, así que un número fijo de chunks no acota el frame en todos lados.
    let plazo = std::time::Duration::from_secs_f32(ctl.budget.ms.max(0.5) / 1000.0);
    let reloj = std::time::Instant::now();
    // Presupuesto de MEMORIA, aparte del de trabajo por frame: mientras esté
    // agotado se sigue construyendo el suelo (que es lo que no puede faltar) pero
    // no se puebla decoración nueva. Ver `MAX_LIVE_DECOR`.
    let decor_room = decor.iter().count() < MAX_LIVE_DECOR;
    let mut live: HashSet<(i64, i64)> = HashSet::new();
    // Chunks cuyo nivel de decoración hay que SUBIR (te acercaste volando). Se
    // ordenan por distancia al jugador antes de gastar presupuesto: si se agota,
    // lo que queda a medias es lo más lejano (menos visible), no una franja al
    // azar según el orden de iteración del ECS.
    let mut pending_decor: Vec<(i64, i64, Entity, DecorDetail, DecorDetail)> = Vec::new();
    // Nivel vigente por chunk — lo consulta el barrido de decor de abajo para
    // tirar sólo lo que sobra, no todo.
    let mut detail_of: HashMap<(i64, i64), DecorDetail> = HashMap::new();
    for (e, mut chunk) in &mut chunks {
        let stale = chunk.body != body
            || chunk.z != z
            || (chunk.cx - pcx).abs() > range + 1
            || (chunk.cz - pcz).abs() > range + 1;
        if stale {
            commands.entity(e).despawn_recursive();
            flora.clear_chunk(chunk.cx as i32, chunk.cz as i32);
            continue;
        }
        live.insert((chunk.cx, chunk.cz));
        // Sólo se SUBE de nivel mientras el chunk viva: bajar de Full a Coarse
        // borraría la flora chica cada vez que cruzás un borde de anillo.
        let want = chunk.decor.max(want_detail(chunk.cx, chunk.cz));
        detail_of.insert((chunk.cx, chunk.cz), want);
        if want > chunk.decor {
            pending_decor.push((chunk.cx, chunk.cz, e, chunk.decor, want));
            chunk.decor = want;
        }
    }
    pending_decor.sort_by_key(|&(cx, cz, ..)| (cx - pcx).pow(2) + (cz - pcz).pow(2));
    // Índice chunk → decoración viva, sólo si hay algún chunk que ASCIENDE de
    // nivel (el caso normal es que no haya ninguno y esto no cuesta nada).
    let mut decor_of: HashMap<(i64, i64), Vec<Entity>> = HashMap::new();
    if pending_decor.iter().any(|&(.., had, _)| had != DecorDetail::None) {
        for (e, d) in &decor {
            decor_of.entry((d.cx, d.cz)).or_default().push(e);
        }
    }
    let mut deferred: Vec<(i64, i64)> = Vec::new();
    for (cx, cz, e, had, want) in pending_decor {
        if budget == 0 || !decor_room || reloj.elapsed() >= plazo {
            // No entró en el presupuesto: hay que DESHACER la marca, o el chunk
            // se queda diciendo que está decorado y nunca se puebla.
            deferred.push((cx, cz));
            if let Ok((_, mut chunk)) = chunks.get_mut(e) {
                chunk.decor = had;
            }
            continue;
        }
        budget -= 1;
        if had != DecorDetail::None {
            // **Subir de Coarse a Full REHACE el chunk.** Antes sólo se añadía lo
            // de cerca, y eso dejaba dos bugs vivos:
            //   1. los árboles/piedras poblados a distancia son SILUETAS —un quad
            //      de color plano, sin textura— y se quedaban así para siempre. Es
            //      el "salgo al espacio, vuelvo y el árbol es un rectángulo verde":
            //      al volver, los chunks nacen lejos (Coarse) y nadie los ascendía
            //      de verdad.
            //   2. el arte de tile (agua, autotiles, piezas) ya se spawnea en
            //      Coarse, así que volver a llamarlo lo DUPLICABA encima.
            // Tirar la decoración del chunk y repoblarla entera arregla las dos, y
            // sólo pasa una vez por chunk (el nivel nunca baja).
            for &e in decor_of.get(&(cx, cz)).into_iter().flatten() {
                commands.entity(e).despawn_recursive();
            }
        }
        spawn_chunk_decor(
            &mut commands, &mut flora, &art, &tiles, &authored.list, &decor_free, body, z, cx, cz,
            want, &mut ctl.small_queue,
        );
        // `spawn_chunk_decor` ya spawnea la flora chica cuando el nivel es Full;
        // el arte de tile va aparte (y sólo una vez, porque acabamos de limpiar).
        spawn_chunk_tile_art(&mut commands, &art, &tiles, body, z, cx, cz);
    }
    let deferred_count = deferred.len();
    for (cx, cz) in deferred {
        detail_of.remove(&(cx, cz));
    }
    for (e, d) in &decor {
        let level = detail_of.get(&(d.cx, d.cz)).copied().unwrap_or_default();
        // Sin chunk (o fuera del anillo) se va todo; con chunk, la flora chica
        // sobrevive mientras el chunk siga en nivel Full.
        let keep =
            d.z == z && level != DecorDetail::None && (!d.small || level == DecorDetail::Full);
        if !keep {
            commands.entity(e).despawn_recursive();
        }
    }

    // Chunks faltantes del anillo, más cercanos primero — el mismo motivo:
    // bajo presupuesto, lo que se demora un frame más es lo más lejano.
    let mut missing: Vec<(i64, i64)> = Vec::new();
    for cx in (pcx - range)..=(pcx + range) {
        for cz in (pcz - range)..=(pcz + range) {
            if !live.contains(&(cx, cz)) {
                missing.push((cx, cz));
            }
        }
    }
    missing.sort_by_key(|&(cx, cz)| (cx - pcx).pow(2) + (cz - pcz).pow(2));
    // Cuántos de `missing` entraron de verdad. Con presupuesto puro de CONTEO
    // alcanzaba con restar `budget` de antemano; con el reloj de por medio no se
    // sabe cuántos van a entrar hasta después de recorrerlos (el plazo puede
    // cortar antes de agotar el conteo), así que el backlog se cuenta AL SALIR.
    let mut missing_done = 0usize;
    let mut built_full: HashSet<(i64, i64)> = HashSet::new();
    for &(cx, cz) in &missing {
        if budget == 0 || reloj.elapsed() >= plazo {
            break;
        }
        missing_done += 1;
        budget -= 1;
        // Sin sitio para decoración el chunk nace pelado, pero NACE: el suelo es
        // lo que no puede faltar. `decor` se queda en `None`, así que en cuanto
        // haya sitio otra vez el ascenso de nivel normal lo puebla — no queda
        // marcado como decorado y olvidado.
        let decor_level = if decor_room {
            want_detail(cx, cz)
        } else {
            DecorDetail::None
        };
        let mesh = build_chunk_mesh(&tiles, &art, body, cx, cz);
        commands.spawn((
            TileChunk {
                body,
                z,
                cx,
                cz,
                built_version: tiles.version,
                decor: decor_level,
            },
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(mat.0.clone()),
            Transform::from_xyz(0.0, 0.0, 0.0),
        ));
        if decor_level == DecorDetail::Full {
            built_full.insert((cx, cz));
        }
        if decor_level != DecorDetail::None {
            spawn_chunk_decor(
                &mut commands,
                &mut flora,
                &art,
                &tiles,
                &authored.list,
                &decor_free,
                body,
                z,
                cx,
                cz,
                decor_level,
                &mut ctl.small_queue,
            );
            // Tiles con arte "vivo" (agua/autotile/piezas construidas) como
            // sprites — en TODAS las capas (subsuelos/pisos usan floor/wall/*).
            spawn_chunk_tile_art(&mut commands, &art, &tiles, body, z, cx, cz);
        }
    }
    // Flora chica de los chunks Full con lo que sobre del plazo, lo más cercano
    // primero. Siempre avanza al menos un chunk por frame aunque el suelo haya
    // gastado el plazo, o una cola larga no se vaciaría nunca.
    if !ctl.small_queue.0.is_empty() {
        let queue = &mut ctl.small_queue.0;
        queue.sort_by_key(|&(_, _, cx, cz)| std::cmp::Reverse((cx - pcx).pow(2) + (cz - pcz).pow(2)));
        let mut hechos = 0usize;
        while let Some(&(qbody, qz, cx, cz)) = queue.last() {
            if hechos > 0 && reloj.elapsed() >= plazo {
                break;
            }
            queue.pop();
            let vigente = qbody == body
                && qz == z
                && (detail_of.get(&(cx, cz)) == Some(&DecorDetail::Full)
                    || built_full.contains(&(cx, cz)));
            if !vigente {
                continue;
            }
            spawn_chunk_small_decor(&mut commands, &flora, &art, &tiles, &decor_free, body, z, cx, cz);
            hechos += 1;
        }
    }
    // La pantalla de carga no levanta la cortina hasta que esto llega a 0 (y
    // `pending_decor` también, porque un anillo con el suelo puesto pero sin
    // árboles se ve a medio hacer).
    ctl.backlog.0 = (missing.len() - missing_done) + deferred_count + ctl.small_queue.0.len();
}

fn body_art_suffix(body: CelestialBody) -> &'static str {
    match body {
        CelestialBody::Earth => "",
        CelestialBody::Moon => "_moon",
        _ => "_mars",
    }
}

fn rebuild_dirty_chunks(
    tiles: Res<TileWorld>,
    art: Res<ArtLibrary>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut chunks: Query<(&mut TileChunk, &Mesh2d)>,
) {
    if !tiles.is_changed() {
        return;
    }
    for (mut chunk, mesh2d) in &mut chunks {
        if chunk.built_version == tiles.version || chunk.z != tiles.z {
            continue;
        }
        let mesh = build_chunk_mesh(&tiles, &art, chunk.body, chunk.cx, chunk.cz);
        meshes.insert(mesh2d.0.id(), mesh);
        chunk.built_version = tiles.version;
    }
}

/// **La sombra que proyecta una terraza sobre la de abajo.** Devuelve el factor
/// por el que se multiplica el color del suelo (1 = sin sombra).
///
/// Es lo que hace que el relieve se LEA como relieve en una vista cenital. Sin
/// ella, un acantilado es una línea de otro color y el ojo no sabe si la terraza
/// de al lado está más alta o más baja — podrían ser dos materiales distintos en
/// el mismo plano. Con una franja oscura al pie de la pared, el cerebro resuelve
/// la ambigüedad al instante: hay algo alto ahí.
///
/// **No sigue al sol y es a propósito.** La malla del chunk se hornea una vez y
/// rehornear el mundo entero cada minuto de juego para girar las sombras costaría
/// muchísimo más de lo que aporta. Ésta es la sombra AMBIENTE del pie de la
/// pared —la que existe a cualquier hora, la que un tileset pintaría en el
/// propio tile—; las sombras que sí giran con el día son las de los sprites, que
/// las lleva `light2d` con su textura direccional.
///
/// Coste: cuatro consultas de nivel por celda, todas servidas por la caché de
/// `relief` (la propia celda ya se consultó justo antes).
///
/// # LA SOMBRA CAÍA DENTRO DE LA ROCA, QUE ES POR DONDE NO SE VE
///
/// Éste era el motivo de fondo de *"sólo se pone el borde del acantilado y arriba
/// se ve vacío"*. La sombra se pedía mirando al vecino del NORTE, y el vecino del
/// norte de la primera celda visible de una terraza no es el canto de arriba: es
/// el PIE del muro, que tiene el nivel de la terraza de abajo (es
/// [`ha_shared::relief::ReliefCell::Pared`], la huella que el dibujo tapa). O sea
/// que la comparación daba cero y la franja oscura se pintaba entera **debajo del
/// sprite del acantilado**, donde no la ve nadie: en pantalla, dos praderas del
/// mismo verde separadas por una banda de roca, sin ninguna pista de cuál está
/// más alta.
///
/// Ahora se busca el canto hacia el norte y se cuenta desde el PIE DE SU DIBUJO
/// ([`ha_shared::relief::alto_de_pared`]), así la franja cae sobre el suelo que sí
/// se ve. Y el largo y la fuerza salen del alto del muro: un bordillo tira una
/// línea, un farallón tira una sombra que se camina.
fn terrace_shadow(body: CelestialBody, tx: i64, tz: i64, level: i8) -> f32 {
    // El agua no recibe sombra de terraza: su superficie ya es un plano distinto y
    // oscurecer la orilla la haría parecer más profunda de lo que es.
    if level < 0 {
        return 1.0;
    }
    // Los costados y el sur: caras que se ven de canto, una línea y nada más. **El
    // norte no está acá** — la pared que tenés encima la resuelve
    // [`sombra_de_muro_en_la_columna`], que sabe dónde termina su dibujo y por eso
    // pinta sobre el suelo que se ve en vez de sobre la roca.
    let mut drop = 0i32;
    for (dx, dz, w) in [(-1i64, 0i64, 2), (1, 0, 2), (0, 1, 1)] {
        let n = ha_shared::relief::relief_level(body, tx + dx, tz + dz);
        if n > level {
            drop += (n - level).min(3) as i32 * w;
        }
    }
    if drop == 0 {
        return 1.0;
    }
    // Hasta un 34 % más oscuro al pie de una pared alta. Más que eso y la franja
    // se lee como un agujero negro pegado al acantilado.
    (1.0 - (drop as f32 * 0.035)).max(0.66)
}

/// Hasta cuántas filas de la terraza de abajo alcanza la sombra de un farallón.
/// Cuatro: lo bastante para que un muro alto se lea alto, y no tanto como para
/// apagar el valle entero.
const SOMBRA_FILAS: i64 = 4;

/// Lo más oscuro que llega a ponerse el pie de un muro. Igual que
/// [`terrace_shadow`]: pasado esto la franja se lee como un agujero negro.
const SOMBRA_PISO: f32 = 0.66;

/// **LA SOMBRA QUE TIRA EL MURO, DE UNA PASADA POR COLUMNA.**
///
/// Devuelve el factor de sombra de las [`CHUNK`] celdas de una columna del chunk.
///
/// # Por qué por columna y no por celda
///
/// La pregunta *"¿tengo un muro encima y a cuántas filas?"* se contesta buscando
/// hacia el norte hasta el primer canto, y en una llanura eso son quince consultas
/// de nivel **por celda** — el primer intento, y se notó enseguida: el chunk
/// tardaba tanto en hornearse que la captura del driver salía con el mundo entero
/// sin cargar, verde liso.
///
/// Barriendo la columna de norte a sur la misma información sale de **dos
/// consultas por fila**: cada canto que aparece sabe dónde cae su pie
/// ([`ha_shared::relief::pie_del_muro`]) y escribe su franja hacia abajo. El
/// barrido arranca `MAX_PARED + 1 + SOMBRA_FILAS` filas por encima del chunk para
/// que un muro que nace justo antes del borde tire igual su sombra dentro.
///
/// La sombra sólo se pinta sobre la terraza que el muro DOMINA (la de su pie): si
/// antes de las cuatro filas el terreno vuelve a cambiar de escalón, ahí se corta
/// — es la misma poda que aplica el dibujo en [`pared_apilada`].
fn sombra_de_muro_en_la_columna(body: CelestialBody, tx: i64, z0: i64) -> [f32; CHUNK as usize] {
    use ha_shared::relief::{alto_de_pared, pie_del_muro, relief_level, MAX_PARED};
    let mut out = [1.0f32; CHUNK as usize];
    let arranque = z0 - (MAX_PARED + 1 + SOMBRA_FILAS);
    for tz in arranque..z0 + CHUNK {
        let Some(pie) = pie_del_muro(body, tx, tz) else { continue };
        if pie + 1 >= z0 + CHUNK {
            continue;
        }
        let alto = alto_de_pared(body, tx, tz);
        // La terraza sobre la que cae: la que tiene el muro debajo.
        let abajo = relief_level(body, tx, tz + 1);
        // Un bordillo tira una línea; un farallón de diez, la franja entera.
        let largo = alto.clamp(1, SOMBRA_FILAS);
        for f in 0..largo {
            let fila = pie + 1 + f;
            if fila < z0 || fila >= z0 + CHUNK {
                continue;
            }
            if relief_level(body, tx, fila) != abajo {
                break;
            }
            let caida = (largo - f) as f32 / largo as f32;
            let drop = caida * (3 + alto.min(6)) as f32 * 2.0;
            let k = (1.0 - drop * 0.035).max(SOMBRA_PISO);
            let i = (fila - z0) as usize;
            out[i] = out[i].min(k);
        }
    }
    out
}

/// Cuánto detalle de sombreado se calcula por celda. La diferencia es sólo de
/// COSTE: el color base y el aclarado por altura son iguales en las dos.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TileShade {
    /// Todo: incluye la sombra de terraza (4 consultas de nivel más por celda).
    /// Es la de los tiles de 1 m — a esa escala la franja oscura al pie de un
    /// acantilado es lo que hace que el desnivel se lea.
    Full,
    /// Sin sombra de terraza. Para el mapa satelital y el disco del planeta: un
    /// quad de 64 m promedia decenas de celdas y la franja de un tile se pierde
    /// en el promedio igual, así que pagarla sería tirar tiempo.
    Coarse,
}

/// **EL color del terreno en una celda.** Color base del tile (con el líquido
/// propio del cuerpo), ruido por celda, aclarado por altura de relieve y —si se
/// pide— sombra de terraza.
///
/// Es la ÚNICA fuente de color del suelo y por eso la comparten las TRES capas
/// del LOD: los tiles de 1 m (`build_chunk_mesh`), el mapa satelital y el disco
/// del planeta (`universe.rs`). Que sea una sola función es el requisito de que
/// el zoom sea continuo.
///
/// Antes cada capa lo calculaba por su lado: la fina aplicaba relieve y terraza,
/// y las de arriba sólo `Tile::color()` pelado. Resultado: en cuanto despegabas
/// —y durante TODA la reentrada, hasta bajar de ~58 m— el mundo perdía de golpe
/// su relieve, sus costas y sus biomas y se volvía un manchón café o verde
/// uniforme. Ésa era la "niebla" al salir de la Tierra y volver.
/// Cuánto se apaga el mundo que se ve por el hueco de un piso alto. Bastante
/// oscuro como para que la losa donde estás parado se lea de un vistazo, y no
/// tanto como para perder de vista dónde está la calle.
const VOID_DIM: f32 = 0.42;

/// **Qué suelo se VE en esta celda, de qué planta sale y cuánto se atenúa.**
///
/// Es la regla del hueco, factorizada: si en tu planta hay materia, es eso, sin
/// atenuar; si hay vacío ([`Tile::Air`]), se mira hacia abajo hasta encontrar
/// algo y se dibuja apagado ([`VOID_DIM`]) — la lectura de Project Zomboid, tu
/// planta nítida y el mundo de abajo en penumbra.
///
/// Devuelve también el NIVEL de donde salió, que es lo que le permite al decor
/// preguntar "¿lo que se ve acá es la superficie?" sin repetir la búsqueda: los
/// árboles y los coches se plantan en el mundo (nivel 0) y se ven desde la
/// azotea, pero no crecen dentro de una galería que se asoma por un pozo.
pub fn suelo_a_la_vista(
    tiles: &TileWorld,
    body: CelestialBody,
    tx: i64,
    tz: i64,
) -> (Tile, i32, f32) {
    let t = tiles.render_tile(body, tx, tz);
    if t != Tile::Air {
        return (t, tiles.z, 1.0);
    }
    let mut below = tiles.z - 1;
    let mut under = tiles.tile_at_level(body, below, tx, tz);
    while under == Tile::Air && below > MIN_Z_LEVEL {
        below -= 1;
        under = tiles.tile_at_level(body, below, tx, tz);
    }
    (under, below, VOID_DIM)
}

pub fn tile_render_color(
    tiles: &TileWorld,
    body: CelestialBody,
    tx: i64,
    tz: i64,
    detail: TileShade,
) -> [f32; 3] {
    let (rgb, shade) = tile_render_color_y_sombra(tiles, body, tx, tz, detail);
    [rgb[0] * shade, rgb[1] * shade, rgb[2] * shade]
}

/// **EL SOMBREADO DE UNA CELDA, APARTE DE SU COLOR.**
///
/// Lo mismo que [`tile_render_color`] pero devuelto en dos: el color base del
/// material y el factor que lo modula (grano por celda, aclarado por altura,
/// sombra de terraza, atenuación del vacío).
///
/// **Hace falta porque la losa del pack se estaba comiendo TODO el sombreado.**
/// `build_chunk_mesh` pone el color de vértice en blanco cuando la celda tiene
/// losa —si no, la textura sale teñida por el color del material, que ya está
/// dentro de la propia losa—, y con eso se iba también el grano, el aclarado por
/// escalón y la sombra que un farallón proyecta sobre la terraza de abajo. En
/// pantalla: la cumbre nevada como **una plancha blanca sin un solo matiz**
/// (`relieve-3-cumbre.png`), y el relieve de todo el mundo texturizado ilegible
/// desde el aire, que es justo lo que el aclarado por altura existe para
/// resolver.
///
/// El comentario de más abajo sobre recortar el aclarado para que la nieve no
/// saturara describe un arreglo que nunca llegó a correr en la nieve: con losa
/// el factor ni se aplicaba. Ahora se aplica —como escala gris sobre la
/// textura— y el tope sigue siendo el que evita aplanarla.
pub fn tile_render_color_y_sombra(
    tiles: &TileWorld,
    body: CelestialBody,
    tx: i64,
    tz: i64,
    detail: TileShade,
) -> ([f32; 3], f32) {
    // **Se dibuja la planta en la que estás** (ver `TileWorld::render_tile`): bajo
    // tierra, tierra; arriba, tu piso; en el suelo, el mundo.
    //
    // **El hueco de un piso alto deja ver lo de abajo.** En `z > 0` todo lo que no
    // construiste es vacío (`Tile::Air`): pintarlo de negro convertía el segundo
    // piso en una habitación flotando en la nada y no había forma de saber si el
    // borde de tu losa daba al patio o a la calle. Se busca hacia abajo el primer
    // nivel con algo y se dibuja ATENUADO — la lectura de Project Zomboid: tu
    // planta nítida, el mundo de abajo apagado.
    //
    // Vale para CUALQUIER nivel, no sólo para los pisos altos: el pozo que cavás
    // en el pasto es el mismo hueco, y verle el fondo —la cueva, apagada— es lo
    // que hace que se lea como un agujero en el suelo y no como una mancha negra.
    let (t, _, void_dim) = suelo_a_la_vista(tiles, body, tx, tz);
    // El líquido se pinta con el color del CUERPO: agua en la Tierra, metano
    // ámbar en Titán, lava en Ío. Un solo tile, un solo autotile, una sola
    // animación — y aun así cada mundo se reconoce de un vistazo.
    let rgb = match t {
        Tile::Water => ha_shared::relief::liquid_color(body, false),
        Tile::WaterDeep => ha_shared::relief::liquid_color(body, true),
        // El suelo de un acantilado es la TERRAZA, no la pared (ver
        // [`suelo_del_relieve`]): la roca la dibuja el sprite que cuelga.
        Tile::Cliff | Tile::Slope => suelo_del_relieve(body, tx, tz).color(),
        _ => t.color(),
    };
    // Ruido por celda para que un relleno grande no se vea plano.
    //
    // **La mitad de lo que era, y por eso.** Iba de 0,93 a 1,02 (±4,5 % sobre
    // 0,975) y con eso encima de una losa de COLOR PLANO —que es lo que el pack
    // dibuja: `Grass_1_22` es un único (71,151,87) en sus 1.024 píxeles— el ruido
    // no se leía como grano sino como **damero**: cada celda entera un tono, la
    // frontera exactamente en la grilla de 32 px, y un llano de nieve o de arena
    // se veía a cuadros desde el aire. Con ±2,25 % sigue rompiendo el liso y deja
    // de dibujar la cuadrícula.
    //
    // El sitio correcto para la variedad no es esto: es que la celda tenga
    // DIBUJO. Lo pone la siembra de props (`pack_siembra`) y, donde el pack la
    // dibujó, la variante de losa (`suelo_uv_seeded`). Este número sólo evita que
    // un relleno grande quede muerto.
    let n = (hash_cell(0x51ce, tx, tz) % 100) as f32 / 100.0;
    let grano = 0.9525 + n * 0.045;
    let mut shade = grano * void_dim;
    // SOMBREADO POR ALTURA: cada escalón por encima del mar aclara un poco el
    // suelo y cada uno por debajo lo oscurece. Sin esto, una meseta y la llanura
    // de al lado se pintan del mismo verde y el relieve sólo se lee por la línea
    // del acantilado — con esto, se lee de un vistazo desde el aire, que es
    // donde el jugador pasa la mitad del tiempo.
    //
    // 4.5 % por escalón: a 12 escalones la cumbre queda ~54 % más clara que la
    // costa. Más que esto y el mundo parece un mapa de calor.
    //
    // Era 3,5 % y no alcanzaba: entre dos terrazas seguidas la diferencia era de
    // un 3 %, o sea menos que el ruido por celda (±4,5 %) que se aplica justo
    // arriba — el escalón quedaba literalmente por debajo del grano del suelo, y
    // por eso arriba y abajo del acantilado se veía el mismo verde.
    let level = ha_shared::relief::relief_level(body, tx, tz);
    let aclarado = 1.0 + (level as f32).clamp(-4.0, 12.0) * 0.045;
    // **PERO ACLARAR NO PUEDE PASARSE DE BLANCO.**
    //
    // `ColorMaterial` multiplica la losa por este color, y el resultado se
    // recorta a 1: en cuanto el canal más alto pasa de ahí, todos los píxeles
    // claros de la textura caen en el mismo blanco y **la losa se aplana**. Con
    // suelos oscuros no se notaba —el pasto es (0.36, 0.62, 0.29) y aguanta el
    // +40 % de sobra—, pero la nieve de la cumbre es (0.82, 0.88, 0.93) en el
    // escalón 9-12: 0,93 × 1,38 = 1,28, o sea que **la cima salía como una
    // plancha lisa blanca** con toda su textura recortada. Se vio en
    // `relieve-3-cumbre.png` el 14 ago 2026 y ninguna medición lo iba a cazar.
    //
    // El tope se calcula sobre el canal más alto para que el TONO no se mueva:
    // recortar canal por canal viraría el color hacia el gris.
    //
    // **Y SE RECORTA EL ACLARADO, NO EL RESULTADO.** El arreglo anterior recortaba
    // `shade` entero —grano incluido—, así que en la nieve TODAS las celdas caían
    // exactamente en el mismo tope y el ±4,5 % de grano por celda desaparecía: la
    // cumbre seguía siendo la plancha lisa que se quería arreglar, sólo que ahora
    // sin saturar. Recortando el aclarado y dejando que el grano module alrededor,
    // el techo se respeta igual (el grano no pasa de 1,02, y por eso el margen) y
    // el suelo sin arte conserva lo único que lo hace suelo.
    let alto = rgb[0].max(rgb[1]).max(rgb[2]).max(1e-3);
    let techo = (1.0 / alto) / 1.02;
    shade = grano * void_dim * aclarado.min(techo);
    if detail == TileShade::Full {
        shade *= terrace_shadow(body, tx, tz, level);
    }
    (rgb, shade)
}

/// **EL GRIS CON EL QUE SE MULTIPLICA TODO LO QUE SE DIBUJE SOBRE ESTA CELDA.**
///
/// La malla lo aplica al color de vértice de la losa; cualquier sprite que TAPE
/// esa misma celda —el acantilado, el bordillo, la rampa— tiene que aplicar el
/// **mismo** número, o se ve un anillo siguiendo el canto de cada meseta.
///
/// # De dónde salía el anillo
///
/// La pieza del acantilado del pack **trae su meseta dibujada** (filas 0-2 del
/// recorte, ver `scripts/import-acantilado.py`) y el importador la tiñe al color
/// **crudo** de la losa que la corona. Pero la malla no pinta la losa cruda:
/// la multiplica por `aclarado = 1 + nivel × 4,5 %` (hasta ×1,54 en la cumbre) y
/// por la sombra de terraza (hasta ×0,66). En el escalón 9 el relleno salía un
/// **40 % más claro** que la franja de meseta del sprite de al lado: dos verdes
/// distintos para el mismo pasto, con la frontera calcada al borde de la terraza.
/// Eso es lo que se lee como "la meseta no tiene su terreno, el borde es sólo un
/// margen".
///
/// `sombra_muro` es la franja oscura al pie de los muros de la columna, que el
/// llamador ya calculó de una pasada ([`sombra_de_muro_en_la_columna`]); `1.0`
/// si esta celda no la lleva.
///
/// **Siempre `TileShade::Full`**: `Coarse` es del mapa satelital y del disco del
/// planeta, que no spawnean sprites. Un sprite teñido con `Coarse` sobre una
/// malla pintada con `Full` sería este mismo bug otra vez, con otro nombre.
pub(crate) fn gris_de_celda(
    tiles: &TileWorld,
    body: CelestialBody,
    tx: i64,
    tz: i64,
    sombra_muro: f32,
) -> f32 {
    tile_render_color_y_sombra(tiles, body, tx, tz, TileShade::Full).1 * sombra_muro
}

/// **Una ciudad tiene que verse desde el aire.**
///
/// A partir de `universe::FINE_OFF_VIEW_H` (170 m) el mundo lo pinta la malla
/// satelital, y su quad mide `view_h/40` redondeado a potencia de dos: a 900 m de
/// altura son **64 m por quad, muestreado 2×2**. Una calle mide tres tiles, así
/// que la probabilidad de que una muestra caiga sobre asfalto es de un 5 % — el
/// promedio se come la ciudad entera y desde el aire quedaba un degradado del
/// bioma, sin retícula, sin veredas y sin manzanas. Es lo que reportó
/// `HA_CITYTEST` a 900 m: "sale casi vacío".
///
/// No se arregla muestreando más: cada tap de más son 37.000 evaluaciones de
/// terreno por reconstrucción de malla, y ese camino ya devolvió el tirón del
/// despegue una vez (ver `universe::average_tile_color`). Se arregla preguntando
/// **una sola vez por vértice** si ese punto cae dentro de un casco urbano
/// (`settlement_covering`, memoizado) y tiñendo el color hacia el gris de la
/// ciudad. Desde un avión una ciudad ES una mancha gris; lo que faltaba no era
/// resolución, era que el color supiera que ahí hay ciudad.
///
/// El tinte se desvanece en el último quinto del casco para que el borde no sea
/// un círculo pintado.
pub fn tinte_urbano(body: CelestialBody, rx: f32, ry: f32, rgb: [f32; 3]) -> [f32; 3] {
    let tx = (rx / TILE_M).floor() as i64;
    let tz = (-ry / TILE_M).floor() as i64;
    let Some(s) = ha_shared::geography::settlement_covering(body, tx, tz) else {
        return rgb;
    };
    let r = s.radius.max(1) as f32;
    let d = (tx - s.center.0).abs().max((tz - s.center.1).abs()) as f32 / r;
    // 1 en el centro, 0 en la orilla; el último 20 % se funde.
    let k = ((1.0 - d) / 0.2).clamp(0.0, 1.0) * TINTE_URBANO;
    let urbano = ha_shared::topdown::Tile::Pavement.color();
    [
        rgb[0] + (urbano[0] - rgb[0]) * k,
        rgb[1] + (urbano[1] - rgb[1]) * k,
        rgb[2] + (urbano[2] - rgb[2]) * k,
    ]
}

/// Cuánto se tiñe el centro de un casco urbano hacia el gris de la vereda.
///
/// No es 1: una ciudad tiene parques, patios y tejados de colores, y pintarla de
/// gris pleno la volvería una mancha muerta. Con 0,45 se lee la mancha urbana
/// sobre el bioma y el bioma sigue asomando por debajo — que es como se ve una
/// ciudad desde un avión.
const TINTE_URBANO: f32 = 0.45;

/// Cuánto del borde de la celda ocupa el corte de tierra del pozo.
const PIT_RIM: f32 = 0.17;
/// Y cuánto la pared, por dentro del corte.
const PIT_WALL: f32 = 0.15;

/// **Dibuja un pozo**: corte de tierra → pared → fondo. Tres tonos concéntricos,
/// que es como se lee un agujero desde arriba cuando no hay perspectiva.
///
/// El corte sale del terreno que SEGUIRÍA ahí si no hubieras cavado (`topdown_tile`
/// del nivel activo), así que un pozo en el pasto tiene borde de pasto oscuro y
/// uno en la arena, de arena: el agujero pertenece al sitio donde está.
/// **El borde del hueco se dibuja donde el hueco TERMINA**, no en cada celda.
///
/// `bordes` son los cuatro lados —sur, norte, oeste, este, en ese orden— que
/// tocan algo que NO es hueco. Existe porque el anillo por celda sólo tiene
/// sentido para un pozo de un tile: en cuanto el vacío es grande —el que rodea a
/// una azotea, o una galería excavada a lo ancho— dibujar cuatro tiras en cada
/// celda pinta una CUADRÍCULA de bordes sobre el suelo de abajo, y la calle que
/// se ve desde el techo queda rayada como un plano en vez de leerse como calle.
/// Con el borde sólo en la orilla, un pozo sigue siendo un pozo y un descampado
/// visto desde arriba sigue siendo el barrio.
#[allow(clippy::too_many_arguments)]
fn pit_quads(
    quad: &mut impl FnMut(f32, f32, f32, f32, [f32; 4]),
    tiles: &TileWorld,
    body: CelestialBody,
    tx: i64,
    tz: i64,
    x0: f32,
    y0: f32,
    fondo: [f32; 3],
    bordes: [bool; 4],
    orilla_con_arte: bool,
) {
    let hondo = [fondo[0], fondo[1], fondo[2], 1.0];
    if bordes == [false; 4] {
        // En medio del vacío no hay orilla que dibujar: sólo el fondo.
        quad(x0, y0, TILE_M, TILE_M, hondo);
        return;
    }
    let suelo = ha_shared::topdown::topdown_tile(body, tiles.z.max(0), tx, tz);
    let s = if suelo == Tile::Air { fondo } else { suelo.color() };
    // Corte de tierra: el suelo de al lado, más oscuro — se lee como tierra
    // recién abierta. Pared: la sombra del pozo, lo más oscuro de los tres.
    // Fondo: lo que ya calculaba `tile_render_color`, o sea el nivel de abajo
    // atenuado — el hueco DEJA VER, que es el punto de todo esto.
    let corte = [s[0] * 0.60, s[1] * 0.60, s[2] * 0.60, 1.0];
    let pared = [fondo[0] * 0.45, fondo[1] * 0.45, fondo[2] * 0.45, 1.0];

    // El fondo primero, de celda entera; encima van las tiras de la orilla. No se
    // solapan entre sí porque cada una ocupa su lado y las esquinas se ceden a
    // las horizontales.
    quad(x0, y0, TILE_M, TILE_M, hondo);
    let mut orilla = |inset: f32, ancho: f32, c: [f32; 4]| {
        let lado = TILE_M - inset * 2.0;
        if bordes[0] {
            quad(x0 + inset, y0 + inset, lado, ancho, c); // sur (abajo en pantalla)
        }
        if bordes[1] {
            quad(x0 + inset, y0 + TILE_M - inset - ancho, lado, ancho, c); // norte
        }
        if bordes[2] {
            quad(x0 + inset, y0 + inset, ancho, lado, c); // oeste
        }
        if bordes[3] {
            quad(x0 + TILE_M - inset - ancho, y0 + inset, ancho, lado, c); // este
        }
    };
    // **EL CORTE DE TIERRA LO DIBUJA LA ORILLA SI HAY ARTE.**
    //
    // Esta tira oscura es el "corte de tierra recién abierta" — y es lo que el
    // usuario ve como *"unas líneas negras"* alrededor del pozo. Existía porque
    // no había ninguna pieza de transición entre la tierra y el pasto; ahora sí
    // (`borde/tierra`, ver `spawn_chunk_orilla`), y el pack dibuja esa costura
    // mucho mejor que un rectángulo al 60 % de luminancia.
    //
    // **La PARED se queda igual.** Es la sombra del pozo, no una costura: es lo
    // único que dice que el agujero tiene profundidad, y la orilla no la dibuja.
    if !orilla_con_arte {
        orilla(0.0, PIT_RIM, corte);
    }
    orilla(PIT_RIM, PIT_WALL, pared);
}

/// Qué parte de la celda ocupa la cara de una pared del subsuelo (la de abajo, la que mira a la galería).
const CARA_DE_PARED: f32 = 0.42;

/// ¿Es la celda un macizo del subsuelo (roca o tierra de la capa, no una pared que construiste)?
fn es_macizo(t: Tile) -> bool {
    matches!(t, Tile::Subsoil | Tile::Stone | Tile::RockDark | Tile::Bedrock | Tile::Rock | Tile::Basalt | Tile::RockRed)
}

/// **¿Esta celda es la pared de una galería vista de frente?** Macizo con el hueco al SUR (en la
/// vista 3/4 la cara que se ve es la que mira hacia vos). Sólo bajo tierra.
fn es_cara_de_pared_del_subsuelo(tiles: &TileWorld, body: CelestialBody, suelo: Tile, tx: i64, tz: i64) -> bool {
    tiles.z < 0 && es_macizo(suelo) && !tiles.tile_at(body, tx, tz + 1).solid()
}

/// **La sombra de rincón del subsuelo.** Una celda abierta se oscurece hacia las paredes que la
/// rodean y una de roca con hueco al lado se apaga un poco: el macizo se hunde y la galería queda
/// "excavada". Cada vértice promedia las cuatro celdas que lo tocan (ver `esquina`), así que sale
/// un degradé y no un escalón. Cuesta ocho consultas por celda y sólo bajo tierra.
fn oclusion_del_subsuelo(tiles: &TileWorld, body: CelestialBody, tx: i64, tz: i64) -> f32 {
    if tiles.z >= 0 {
        return 1.0;
    }
    let sol = |dx: i64, dz: i64| tiles.tile_at(body, tx + dx, tz + dz).solid();
    if tiles.tile_at(body, tx, tz).solid() {
        // roca con hueco pegado: más oscura, así la pared se despega de la tapa
        let abierto = !sol(0, 1) || !sol(0, -1) || !sol(1, 0) || !sol(-1, 0);
        return if abierto { 0.82 } else { 1.0 };
    }
    let lados = sol(0, 1) as u8 + sol(0, -1) as u8 + sol(1, 0) as u8 + sol(-1, 0) as u8;
    let diag = sol(1, 1) as u8 + sol(-1, 1) as u8 + sol(1, -1) as u8 + sol(-1, -1) as u8;
    (1.0 - 0.09 * lados as f32 - 0.04 * diag as f32).max(0.55)
}

/// One `Mesh2d` covering a CHUNK×CHUNK grid of tiles, each a colored quad at
/// render pos `(tx*TILE, -tz*TILE)`.
fn build_chunk_mesh(
    tiles: &TileWorld,
    art: &ArtLibrary,
    body: CelestialBody,
    cx: i64,
    cz: i64,
) -> Mesh {
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(1024);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(1024);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(1024);
    let mut indices: Vec<u32> = Vec::with_capacity(1536);
    let blanca = crate::art2d::suelo_uv_blanca();
    // ¿Hay arte de orilla para el borde de un pozo? Se pregunta UNA vez por chunk:
    // la respuesta es del manifest, no de la celda.
    //
    // **Y sólo en la planta baja o más abajo**, con el mismo límite que
    // `spawn_chunk_orilla`: en un piso alto el "hueco" es el vacío que rodea a una
    // azotea, ahí no se siembra orilla, y si además se apagara el corte oscuro el
    // borde del tejado se quedaría sin ninguna de las dos cosas.
    let orilla_de_pozo = tiles.z <= 0 && art.has_poses("borde/tierra");
    // La vereda de la ciudad de este chunk. Una consulta por chunk — ver
    // `suelo_de_la_celda`.
    let vereda = vereda_de_la_ciudad(art, body, cx * CHUNK + CHUNK / 2, cz * CHUNK + CHUNK / 2);

    // `uv` es el recorte de la hoja del suelo que le toca a este quad; la celda
    // blanca deja el color de vértice como estaba (ver `art2d::suelo_uv_blanca`).
    // `esquinas` = color de cada vértice en el orden abajo-izq, abajo-der,
    // arriba-der, arriba-izq (ver `sombra_de_la_esquina`).
    let mut quad_esquinas = |x0: f32, y0: f32, w: f32, h: f32, esquinas: [[f32; 4]; 4], uv: [f32; 4]| {
        let base = positions.len() as u32;
        positions.push([x0, y0, 0.0]);
        positions.push([x0 + w, y0, 0.0]);
        positions.push([x0 + w, y0 + h, 0.0]);
        positions.push([x0, y0 + h, 0.0]);
        // V crece hacia ABAJO en la textura y hacia ARRIBA en el mundo: el
        // vértice de abajo del quad lleva la V de abajo de la celda.
        uvs.push([uv[0], uv[3]]);
        uvs.push([uv[2], uv[3]]);
        uvs.push([uv[2], uv[1]]);
        uvs.push([uv[0], uv[1]]);
        for rgba in esquinas {
            let lin = Color::srgba(rgba[0], rgba[1], rgba[2], rgba[3]).to_linear();
            colors.push([lin.red, lin.green, lin.blue, lin.alpha]);
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    };

    // **LA SOMBRA SE INTERPOLA ENTRE ESQUINAS, no se pinta por baldosa.**
    //
    // Cada celda tenía UN tono, así que la franja al pie de un muro y el aclarado
    // por escalón salían en escalones de 1 m: sobre nieve se veían rectángulos
    // grises pixelados en vez de una sombra (captura del driver de relieve, 27 sep
    // 2026). Ahora se calcula la sombra de cada celda del chunk MÁS un anillo de una
    // celda alrededor —así dos chunks vecinos dan el mismo valor en el borde y no
    // hay costura— y cada vértice promedia las cuatro celdas que lo tocan; la GPU
    // interpola y el degradé sale liso.
    const LADO: usize = CHUNK as usize + 2;
    let mut sombras = [[1.0f32; LADO]; LADO];
    for (ix, fila) in sombras.iter_mut().enumerate() {
        let tx = cx * CHUNK + ix as i64 - 1;
        // la columna de sombra de muro, de z0−1 a z0+CHUNK (dos llamadas: la
        // función devuelve CHUNK filas desde donde se le pida)
        let arriba = sombra_de_muro_en_la_columna(body, tx, cz * CHUNK - 1);
        let abajo = sombra_de_muro_en_la_columna(body, tx, cz * CHUNK + 1);
        for (iz, v) in fila.iter_mut().enumerate() {
            let tz = cz * CHUNK + iz as i64 - 1;
            let k = if iz < CHUNK as usize { arriba[iz] } else { abajo[iz - 2] };
            let (_, sombra) = tile_render_color_y_sombra(tiles, body, tx, tz, TileShade::Full);
            *v = sombra * k * oclusion_del_subsuelo(tiles, body, tx, tz);
        }
    }
    // (lx, lz) = línea de esquina local 0..=CHUNK
    let esquina = |lx: usize, lz: usize| -> f32 {
        (sombras[lx][lz] + sombras[lx + 1][lz] + sombras[lx][lz + 1] + sombras[lx + 1][lz + 1]) * 0.25
    };

    for lx in 0..CHUNK {
        let tx = cx * CHUNK + lx;
        for lz in 0..CHUNK {
            let tz = cz * CHUNK + lz;
            // El color sale de `tile_render_color`, la MISMA función que usan el
            // mapa satelital y el disco del planeta: es lo que hace que al subir
            // el terreno no cambie de color ni pierda el relieve.
            let (base, _) = tile_render_color_y_sombra(tiles, body, tx, tz, TileShade::Full);
            let sombra = sombras[lx as usize + 1][lz as usize + 1];
            let rgb = [base[0] * sombra, base[1] * sombra, base[2] * sombra];
            // Render pos: cell (tx,tz) spans [tx,tx+1]×[tz,tz+1] in world; screen
            // y = -z so the quad bottom-left is (tx*TILE, -(tz+1)*TILE).
            let x0 = tx as f32 * TILE_M;
            let y0 = -(tz as f32 + 1.0) * TILE_M;
            // **El pozo se dibuja como AGUJERO, no como celda de otro color.**
            //
            // Con un solo cuadrado del color de abajo atenuado, cavar dejaba una
            // mancha oscura que no se leía como hueco: el jugador miraba el suelo
            // y no entendía qué había pasado. Un agujero en cenital son tres
            // cosas — el corte de tierra alrededor, la pared del pozo y el fondo
            // —, y con eso alcanza aunque sean colores planos.
            //
            // El BORDE va sólo en la orilla del hueco (ver `pit_quads`): en medio
            // de un vacío grande —el que rodea a una azotea— cada celda con su
            // marco pintaba una cuadrícula encima de la calle de abajo.
            if tiles.renders_as_pit(body, tx, tz) {
                let borde = |dx: i64, dz: i64| !tiles.renders_as_pit(body, tx + dx, tz + dz);
                // Sur = z+1 (abajo en el mundo), y en pantalla `y = -z`, así que
                // el lado "de abajo" del quad es el del vecino sur.
                let bordes = [borde(0, 1), borde(0, -1), borde(-1, 0), borde(1, 0)];
                // El pozo se dibuja con colores planos: va a la celda blanca.
                let mut plano = |x: f32, y: f32, w: f32, h: f32, c: [f32; 4]| {
                    quad_esquinas(x, y, w, h, [c; 4], blanca)
                };
                pit_quads(&mut plano, tiles, body, tx, tz, x0, y0, rgb, bordes, orilla_de_pozo);
                continue;
            }
            // **La losa del pack, si este material tiene una.** La variante sale
            // de la celda (determinista) para que la calle y el pasto no repitan
            // el mismo dibujo cada metro. Sin losa → celda blanca → el color de
            // vértice de siempre.
            let mut suelo = tiles.render_tile(body, tx, tz);
            if matches!(suelo, Tile::Cliff | Tile::Slope) {
                suelo = suelo_del_relieve(body, tx, tz);
            }
            // **Bajo tierra el macizo y la galería llevan el carácter de su BIOMA** (`ha_shared::subsuelo`):
            // otra roca según la región, y el piso de la galería con su tinte.
            let bioma = (tiles.z < 0).then(|| ha_shared::subsuelo::bioma_en(body, tx, tz));
            let uv = match bioma {
                Some(b) if es_macizo(suelo) => art
                    .suelo_uv_seeded(b.clave_de_macizo(), celda_semilla(tx, tz))
                    .or_else(|| suelo_de_la_celda(art, body, suelo, tx, tz, vereda))
                    .unwrap_or(blanca),
                _ => suelo_de_la_celda(art, body, suelo, tx, tz, vereda).unwrap_or(blanca),
            };
            let tinte = match bioma {
                Some(b) if suelo == Tile::CaveFloor => b.tinte_del_piso(),
                _ => [1.0, 1.0, 1.0],
            };
            // **Con losa el color de vértice es el SOMBREADO en gris.**
            //
            // No puede ser el color del material —la losa ya lo trae, y
            // multiplicarlos deja el suelo el doble de oscuro—, pero tampoco
            // blanco: eso fue lo que hubo hasta ahora y con blanco se perdía
            // TODO el sombreado. El grano por celda, el aclarado por escalón y la
            // sombra de terraza se calculan en `tile_render_color_y_sombra` y
            // valen igual para un color plano que para una textura; en gris
            // modulan la losa sin virarle el tono.
            //
            // Era el motivo de fondo de que la cumbre se viera como una plancha
            // blanca vacía: la nieve tiene losa, así que su celda salía a
            // 1,0×losa, sin un matiz que dijera dónde sube el terreno.
            // esquinas: abajo-izq (tx, tz+1), abajo-der (tx+1, tz+1), arriba-der
            // (tx+1, tz), arriba-izq (tx, tz) — en líneas de esquina locales
            let (ex, ez) = (lx as usize, lz as usize);
            let ks = [esquina(ex, ez + 1), esquina(ex + 1, ez + 1), esquina(ex + 1, ez), esquina(ex, ez)];
            let color = |k: f32| -> [f32; 4] {
                if uv == blanca {
                    [base[0] * k, base[1] * k, base[2] * k, 1.0]
                } else {
                    [k * tinte[0], k * tinte[1], k * tinte[2], 1.0]
                }
            };
            // **La cara de la pared del subsuelo.** Una celda de roca con la galería al sur se
            // parte en dos: la tapa (arriba) y la CARA que mira a la galería (abajo, más oscura,
            // con un filo claro en el canto). Es lo que hace que el macizo se lea como una pared
            // y que cavar deje una galería con borde, no un recorte en la nada.
            if es_cara_de_pared_del_subsuelo(tiles, body, suelo, tx, tz) && uv != blanca {
                let mid_uv = (uv[1] + uv[3]) * 0.5;
                let alto = TILE_M * CARA_DE_PARED;
                let oscuro = |c: [f32; 4]| [c[0] * 0.58, c[1] * 0.58, c[2] * 0.62, c[3]];
                let (a, b, c, d) = (color(ks[0]), color(ks[1]), color(ks[2]), color(ks[3]));
                // tapa: de arriba hasta (1 - CARA) de la celda
                quad_esquinas(x0, y0 + alto, TILE_M, TILE_M - alto, [a, b, c, d], [uv[0], uv[1], uv[2], mid_uv]);
                // cara: el tramo de abajo, oscurecido
                quad_esquinas(x0, y0, TILE_M, alto, [oscuro(a), oscuro(b), oscuro(a), oscuro(b)], [uv[0], mid_uv, uv[2], uv[3]]);
                // filo claro entre la tapa y la cara
                let filo = TILE_M * 0.07;
                let luz = |c: [f32; 4]| [(c[0] * 1.35).min(1.0), (c[1] * 1.35).min(1.0), (c[2] * 1.3).min(1.0), c[3]];
                quad_esquinas(x0, y0 + alto, TILE_M, filo, [luz(a), luz(b), luz(b), luz(a)], [uv[0], uv[1], uv[2], uv[1] + (uv[3] - uv[1]) * 0.08]);
            } else {
                quad_esquinas(x0, y0, TILE_M, TILE_M, [color(ks[0]), color(ks[1]), color(ks[2]), color(ks[3])], uv);
            }
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
fn spawn_chunk_decor(
    commands: &mut Commands,
    flora: &mut FloraHarvest,
    art: &ArtLibrary,
    tiles: &TileWorld,
    authored: &[ha_shared::world_edits::PlacedDecor],
    decor_free: &DecorFreeCells,
    body: CelestialBody,
    z: i32,
    cx: i64,
    cz: i64,
    detail: DecorDetail,
    small_queue: &mut SmallDecorQueue,
) {
    // ANIDADA dentro de `stream_tile_chunks`: en la tabla de `ha-sys` este número
    // está INCLUIDO en el de aquél. Se separa a propósito — §10 sospecha de las
    // 256 celdas × `light2d::attach` de acá, y sin partirlo el chunk entero es
    // una sola cifra que no acusa a nadie.
    let _probe = crate::debug::probe("spawn_chunk_decor");
    let suffix = body_art_suffix(body);
    let mut trees: Vec<TreeInstance> = Vec::new();

    for lx in 0..CHUNK {
        for lz in 0..CHUNK {
            let tx = cx * CHUNK + lx;
            let tz = cz * CHUNK + lz;
            let id = pack_cell(tx, tz);
            let wx = tx as f32 * TILE_M + 0.5;
            let wz = tz as f32 * TILE_M + 0.5;
            let r = render_xy(wx, wz);
            let sort = sort_z(wz);

            // (Acá se dibujaban la boca de mina y la escalera del generador.
            // Se fueron con las cuevas procedurales: el único agujero del suelo
            // es el que cavás, y lo dibuja la propia malla — ver `pit_quads`.)

            // El decor va donde hay SUELO A LA VISTA, y nunca sobre un tile sólido
            // —natural (roca) o construido (muros)—: así los árboles y los props ya
            // no tapan tu muro (bug de capas). Tampoco sobre celdas AUTORADAS en el
            // editor de mundo: si pintaste un claro donde había bosque, el bosque no
            // vuelve a crecer. `buildable_ground` deja fuera el líquido, las rampas
            // y lo construido.
            //
            // **Qué suelo se ve depende de la planta**, y lo resuelve
            // [`suelo_a_la_vista`]:
            //
            // - En la superficie, el mundo: pasto, árboles, mariposas.
            // - Bajo tierra, NADA: el subsuelo es tierra maciza y ahí no crece un
            //   árbol. (Antes el decor "seguía al mundo" y salía la superficie
            //   entera plantada alrededor de tu túnel — la mitad de por qué el
            //   subsuelo no parecía subsuelo.)
            // - Desde un piso alto, lo de ABAJO, atenuado: por el vacío del borde
            //   de la azotea se ven la calle, los árboles y los coches, apagados
            //   como el color del suelo que ya dibuja `tile_render_color`. Eso es
            //   lo que hace que salir a la azotea sea salir a mirar el barrio y no
            //   asomarse a una losa flotando en gris.
            let (suelo, nivel, atenuar) = suelo_a_la_vista(tiles, body, tx, tz);
            if nivel == 0 && suelo.buildable_ground() && !decor_free.contains(body, 0, tx, tz) {
                if let Some(decor) = surface_decor_at(body, tx, tz) {
                    match decor {
                        DecorKind::Tree => {
                            // Removed set is a flat per-body set at key (0,0);
                            // `id` = packed global cell (see `pack_cell`).
                            if !flora.is_removed(body, 0, 0, id) {
                                let _probe = crate::debug::probe("decor/arbol");
                                trees.push(TreeInstance {
                                    cx: 0,
                                    cz: 0,
                                    id,
                                    world: Vec3::new(r.x, r.y, 0.0),
                                });
                                let species = pick_tree_species_in(body, tx, tz);
                                // SILUETA a distancia. Fuera del anillo cercano el
                                // árbol mide unos píxeles: la textura de su especie no
                                // aporta nada y sí cuesta —cada especie es un material
                                // distinto, o sea un draw call distinto—. Con un quad
                                // de color plano TODOS los árboles lejanos entran en un
                                // solo lote, y la silueta (misma forma, mismo tamaño,
                                // color medio de la copa) se lee igual desde el aire.
                                let sprite = if detail == DecorDetail::Coarse {
                                    let f = species.fallback;
                                    Sprite::from_color(
                                        Color::srgb(f[0], f[1], f[2]),
                                        species.size.into(),
                                    )
                                } else {
                                    art
                                    .sprite_first_seeded(&[
                                        &format!("{}{suffix}", species.key),
                                        species.key,
                                        &format!("decor/tree{suffix}"),
                                        "decor/tree",
                                    ], id as u64, species.size.into())
                                    .unwrap_or_else(|| {
                                        let f = species.fallback;
                                        Sprite::from_color(
                                            Color::srgb(f[0], f[1], f[2]),
                                            species.size.into(),
                                        )
                                    })
                                };
                                let sprite = atenuar_sprite(sprite, atenuar);
                                // Base al suelo: el centro sube medio alto (menos el
                                // hundido que ya tenía el árbol de 3.2 → -0.6).
                                let y = r.y + species.size[1] * 0.5 - 0.6;
                                let mut ent = commands.spawn((
                                    SurfaceDecor { cx, cz, z, small: false },
                                    InteractTarget {
                                        kind: InteractTargetKind::Tree,
                                        id,
                                    },
                                    crate::render::particles::HarvestReactive,
                                    sprite,
                                    Transform::from_xyz(r.x, y, sort),
                                ));
                                // Árboles bioluminiscentes / con farol colgado: si el
                                // usuario le marcó luz a la especie, la lleva puesta.
                                // A distancia NO: cada emisor cuelga tres quads de halo
                                // y compite por los 24 slots del velo de noche
                                // (`light2d::VEIL_LIGHTS`), y un farol a 150 m no
                                // ilumina nada que se vea — sólo le roba el charco al
                                // farol que tenés al lado.
                                if detail == DecorDetail::Full {
                                    crate::light2d::attach(
                                        &mut ent,
                                        &art,
                                        &[
                                            &format!("{}{suffix}", species.key),
                                            species.key,
                                            &format!("decor/tree{suffix}"),
                                            "decor/tree",
                                        ],
                                        species.size.into(),
                                    );
                                }
                            }
                        }
                        DecorKind::Rock => {
                            // Mined rocks are removed just like chopped trees.
                            if !flora.is_removed(body, 0, 0, id) {
                                // Silueta a distancia — mismo criterio que el árbol.
                                let sprite = if detail == DecorDetail::Coarse {
                                    Sprite::from_color(rock_color(body), Vec2::new(1.3, 1.0))
                                } else {
                                    art.sprite_first_seeded(
                                        &[&format!("decor/rock{suffix}"), "decor/rock"],
                                        id as u64,
                                        Vec2::new(1.3, 1.0),
                                    )
                                        .unwrap_or_else(|| {
                                            Sprite::from_color(rock_color(body), Vec2::new(1.2, 0.9))
                                        })
                                };
                                let mut ent = commands.spawn((
                                    SurfaceDecor { cx, cz, z, small: false },
                                    InteractTarget {
                                        kind: InteractTargetKind::Rock,
                                        id,
                                    },
                                    atenuar_sprite(sprite, atenuar),
                                    Transform::from_xyz(r.x, r.y, sort),
                                ));
                                if detail == DecorDetail::Full {
                                    crate::light2d::attach(
                                        &mut ent,
                                        &art,
                                        &[&format!("decor/rock{suffix}"), "decor/rock"],
                                        Vec2::new(1.3, 1.0),
                                    );
                                }
                            }
                        }
                    }
                }
            }

            // **Vetas de mineral y gema**, y sólo las que se PUEDEN VER.
            //
            // Bajo tierra el 1,4 % de las celdas tiene veta, y hasta ahora se
            // dibujaban todas: el macizo entero salía salpicado de cuadraditos de
            // colores hasta el horizonte, como un mapa de puntos. Una veta metida
            // en veinte metros de roca no se ve —hay que llegar cavando—, así que
            // se dibuja la que está EXPUESTA: la que asoma en la pared de una
            // galería, o la que está en el suelo abierto. Es la misma cuenta que
            // hace un minero: la veta se descubre en la cara del túnel.
            let veta_visible = z >= 0 || expuesta(tiles, body, tx, tz);
            if let Some(dep) = deposit_at(body, z, tx, tz)
                .filter(|_| veta_visible && !flora.is_removed(body, 0, 0, id))
            {
                let kind = deposit_target_kind(dep);
                let (color, size) = deposit_style(kind);
                let key = match kind {
                    InteractTargetKind::GemRed => "decor/gem_red",
                    InteractTargetKind::GemBlue => "decor/gem_blue",
                    InteractTargetKind::OreGold => "decor/ore_gold",
                    _ => "decor/ore_iron",
                };
                // Las vetas son UN sprite suelto (art/gen/mundo.png), no poses de tile: `tile_has_art`
                // decía siempre que no y salían las pepitas de color en vez del dibujo.
                let con_arte = art.get(key).is_some();
                let mut e = commands.spawn((
                    SurfaceDecor { cx, cz, z, small: false },
                    InteractTarget { kind, id },
                    if con_arte {
                        art.sprite_or_color(key, color, size)
                    } else {
                        Sprite::from_color(Color::NONE, size)
                    },
                    Transform::from_xyz(r.x, r.y, sort),
                ));
                if !con_arte {
                    // Sin pixel asignado la veta se dibuja POR CÓDIGO: pepitas
                    // sueltas en la roca, no un cuadrado de color. Un cuadrado es
                    // lo que hacía que el subsuelo pareciera un tablero.
                    vetear(&mut e, size, color, id as u64);
                }
                if let Some(a) = art.animation(key) {
                    e.insert(a);
                }
                // Vetas de gema/mineral: brillan en la cueva si el slot lo declara.
                crate::light2d::attach(&mut e, &art, &[key], size);
            }
        }
    }
    // Decor AUTORADO (editor de mundo): pinta los árboles/props colocados a mano cuya celda
    // cae en este chunk. Sólo visual (sin cosecha). Tamaño de la tabla `worldgen` si se
    // conoce; si no, 1.5×1.5.
    for d in authored {
        if d.body != body as u8
            || d.z != z
            || d.x.div_euclid(CHUNK) != cx
            || d.zc.div_euclid(CHUNK) != cz
        {
            continue;
        }
        // Lo autorado también se levanta, y también se queda levantado.
        let aid = pack_cell(d.x, d.zc);
        let (akx, akz) = crate::targeting::PROP_REMOVED_KEY;
        if flora.is_removed(body, akx, akz, aid) {
            continue;
        }
        // Tamaño: el que fijó el editor (deducido del recorte del asset) manda;
        // si no, el de la tabla de especies; si no, un decor chico.
        let size: [f32; 2] = d
            .size
            .filter(|s| s[0] > 0.0 && s[1] > 0.0)
            .or_else(|| {
                ha_shared::worldgen::TREE_SPECIES
                    .iter()
                    .find(|t| t.key == d.key)
                    .map(|t| t.size)
            })
            .or_else(|| {
                ha_shared::worldgen::NATURE_SPECIES
                    .iter()
                    .chain(ha_shared::worldgen::PROP_SPECIES.iter())
                    .find(|e| e.key == d.key)
                    .map(|e| e.size)
            })
            .unwrap_or([1.5, 1.5]);
        // `world_sprite` respeta el recorte/pose del slot: sin esto, un asset
        // recortado de un atlas se plantaba con el atlas entero.
        // Variante por celda: un objeto con varias variantes de arte se ve
        // variado sin guardar nada por instancia (la misma celda elige siempre
        // la misma). Sin variantes, es exactamente `world_sprite` de antes.
        let seed = (d.x as u64) << 32 ^ (d.zc as u64 & 0xFFFF_FFFF);
        let Some((s, anim)) = art.world_sprite_seeded(&d.key, seed, size.into()) else { continue };
        // Colocación ALINEADA AL GRID (misma regla que el editor de mundo, ver
        // `ha_assets::placement_center_tiles`): la celda ancla del asset queda
        // sobre la celda que apuntaste. Antes se apoyaba el pie del sprite en el
        // borde inferior de la celda y se centraba a ojo: con anchos pares el
        // asset quedaba medio tile corrido de su huella de colisión y parecía
        // plantado una celda más arriba de donde lo pusiste.
        let cells = art.slot_cells(&d.key).unwrap_or((
            size[0].round().max(1.0) as u32,
            size[1].round().max(1.0) as u32,
        ));
        let anchor = art.anchor(&d.key, cells);
        let c = ha_assets::placement_center_tiles(d.x, d.zc, cells, anchor, size);
        let (wx, wz) = (c[0] * TILE_M, c[1] * TILE_M);
        let r = render_xy(wx, wz);
        let (min_x, min_z) = ha_assets::placement_min_tile(d.x, d.zc, anchor);
        // El Y-sort sale del PIE del sprite: la fila INFERIOR de su huella.
        //
        // Antes se usaba la fila del ANCLA (`d.zc + 1`), que sólo coincide con el
        // pie cuando el ancla está en la última fila —el default—. En cuanto
        // pintabas una huella de colisión en el editor y el ancla quedaba más
        // arriba (lo normal en una casa: se ancla por la puerta), el asset
        // Y-sorteaba varias filas al norte de donde de verdad se apoya y se
        // dibujaba ENCIMA del jugador que estaba delante. Con la fila real del pie
        // un asset alto nunca tapa lo que tiene por delante.
        let foot_z = (min_z + cells.1.max(1) as i64) as f32 * TILE_M;
        // La CAPA del slot decide la banda: lo que marcaste como piso en el
        // editor de assets (charcos, caminos, alfombras, plataformas) va debajo
        // del jugador; lo que marcaste techo va arriba y se transparenta. Sin
        // capa, Y-sort como siempre. Esto es lo que arregla "lo que dibujo en el
        // editor de mundo me tapa".
        let layer = art.slot_layer(&d.key);
        let mut ent = commands.spawn((
            SurfaceDecor { cx, cz, z, small: false },
            s,
            Transform::from_xyz(r.x, r.y, crate::layers::depth(layer, foot_z)),
        ));
        // Mismo criterio que el decor sembrado: se toca todo menos lo que es piso
        // (un camino o una alfombra pintados en el editor son suelo, no objeto).
        if !crate::layers::es_piso(layer) {
            ent.insert((
                crate::targeting::InteractTarget {
                    kind: InteractTargetKind::Prop,
                    id: aid,
                },
                crate::harvest::PropArt(std::borrow::Cow::Owned(d.key.clone())),
            ));
            if let Some(u) = crate::descanso::MuebleDeUso::nuevo(&d.key, wx, foot_z, size[1]) {
                ent.insert(u);
            }
        }
        if layer == Some(ha_assets::ArtLayer::Roof) {
            // Techo autorado: se desvanece cuando el jugador camina por debajo.
            ent.insert(crate::layers::Overhead::techo(
                (min_x, min_z),
                (cells.0.max(1) as i64, cells.1.max(1) as i64),
            ));
        }
        if let Some(a) = anim {
            ent.insert(a);
        }
        crate::light2d::attach(&mut ent, &art, &[d.key.as_str()], size.into());
    }

    flora.set_chunk_trees(cx as i32, cz as i32, trees);

    if detail == DecorDetail::Full {
        let entrada = (body, z, cx, cz);
        if !small_queue.0.contains(&entrada) {
            small_queue.0.push(entrada);
        }
    }
}

/// Flora y props de ADORNO (F3): matas, hongos, muebles de exterior. Sólo visual
/// y sólo si el slot tiene arte asignada (cero entidades sin textura).
///
/// Vive aparte de [`spawn_chunk_decor`] porque es la categoría más numerosa por
/// chunk y la única que se puebla sólo en el anillo cercano: a 100 m de distancia
/// una mata mide un píxel, y multiplicada por los ~600 chunks que puede tener el
/// anillo de vuelo era el grueso del coste por frame. Los árboles y las piedras
/// —lo que de verdad se lee desde el aire— van en todo el anillo.
#[allow(clippy::too_many_arguments)]
fn spawn_chunk_small_decor(
    commands: &mut Commands,
    flora: &FloraHarvest,
    art: &ArtLibrary,
    tiles: &TileWorld,
    decor_free: &DecorFreeCells,
    body: CelestialBody,
    z: i32,
    cx: i64,
    cz: i64,
) {
    // Bajo tierra no hay vereda, ni flora, ni bancos: hay tierra. Desde un piso
    // alto sí se pueblan —atenuados— porque son la calle que se mira desde la
    // azotea: los coches, los faroles y los bancos son lo que la hace una calle y
    // no una franja gris.
    if z < 0 {
        return;
    }
    for lx in 0..CHUNK {
        for lz in 0..CHUNK {
            let tx = cx * CHUNK + lx;
            let tz = cz * CHUNK + lz;
            if decor_free.contains(body, 0, tx, tz) {
                continue;
            }
            let (tile, nivel, atenuar) = suelo_a_la_vista(tiles, body, tx, tz);
            // Lo que se ve tiene que ser ESTE nivel: planta baja de la calle, o la planta de un interior.
            if nivel != 0 && !ha_shared::interiores::es_interior(nivel) {
                continue;
            }
            // EL AGUA TAMBIÉN SE DECORA. Antes esta puerta era "sólo suelo firme",
            // así que los lagos y el mar salían como una plancha lisa de un solo
            // azul — que a la vista es un agujero en el mapa. Ahora el bajío lleva
            // troncos flotantes, nenúfares y juncos, y su orilla lleva el
            // campamento y el puesto de pesca.
            // Y lo que flota lo elige el SITIO, no un dado: el junco sale donde
            // el agua toca la tierra, el nenúfar sólo en agua quieta, el tronco
            // a la deriva donde hay corriente y el cardumen lejos de la orilla.
            // Ver `ha_shared::agua::sitio_de_agua` — con la tabla única que había
            // antes, el salvavidas del pack aparecía en un charco de la sierra.
            // **ADENTRO DE UNA CASA SÓLO HAY MUEBLES** (29 sep 2026). La flora del
            // jardín y los props del patio se sorteaban también sobre el piso de
            // madera —`WoodFloor` es suelo edificable—, y adentro de las casas
            // aparecían arbustos, fuentes y arcos de seto. El piso de una casa
            // lo decide `ha_shared::amueblado` y nadie más.
            let mueble = (tile == Tile::WoodFloor)
                .then(|| ha_shared::amueblado::mueble_anclado_z(body, z, tx, tz))
                .flatten();
            let e = if tile.swim() {
                ha_shared::agua::decor_at(body, tx, tz)
            } else if tile == Tile::WoodFloor {
                mueble.map(|m| m.decor)
            } else if !tile.buildable_ground() || surface_decor_at(body, tx, tz).is_some() {
                None
            } else {
                // La ESQUINA manda sobre la vereda: el semáforo tiene sitio
                // fijo y no puede perderlo contra un banco sorteado.
                ha_shared::worldgen::esquina_decor_at(body, tx, tz)
                    // Y la composición de los lotes de parque (plaza, natatorio): sitio fijo, simétrica.
                    .or_else(|| ha_shared::worldgen::prefab_de_lote_at(body, tx, tz))
                    // El ALUMBRADO también tiene sitio fijo (el cordón, con
                    // ritmo) y por el mismo motivo va antes del sorteo: una
                    // hilera de faroles con un hueco donde ganó un tacho deja de
                    // ser una hilera.
                    .or_else(|| ha_shared::worldgen::alumbrado_at(body, tx, tz))
                    // Y las guirnaldas, que también tienen sitio fijo (la
                    // retícula de la plaza) — ver `guirnalda_at`.
                    .or_else(|| ha_shared::worldgen::guirnalda_at(body, tx, tz))
                    .or_else(|| ha_shared::worldgen::vereda_decor_at(body, tx, tz))
                    // La calzada tiene su propia tabla —lo tirado en el suelo— y
                    // va acá, antes que la flora: sobre asfalto no crece nada.
                    .or_else(|| ha_shared::worldgen::calzada_decor_at(body, tx, tz))
                    .or_else(|| ha_shared::worldgen::playa_decor_at(body, tx, tz))
                    .or_else(|| ha_shared::worldgen::shore_decor_at(body, tx, tz))
                    // El JARDÍN de las parcelas va antes que la flora suelta: la casa no ocupa toda la parcela y
                    // el resto se decora (macetas, flores, reposeras).
                    .or_else(|| ha_shared::worldgen::jardin_decor_at(body, tx, tz))
                    .or_else(|| nature_decor_at(body, tx, tz))
                    .or_else(|| prop_decor_at(body, tx, tz))
            };
            let Some(e) = e else { continue };
            // **Y lo de afuera no se mete adentro.** Un prop de patio de 3×3 anclado
            // en la celda de al lado de la pared se dibujaba media casa adentro:
            // si su dibujo pisa una celda de edificio, no va.
            if mueble.is_none() && !tile.swim() && pisa_un_edificio(tiles, body, tx, tz, e.size) {
                continue;
            }
            // Levantado = levantado. El prop que sacaste del mundo no vuelve a
            // sembrarse cuando el chunk se re-streamea (mismo set que el árbol
            // talado, misma clave de celda).
            let id = pack_cell(tx, tz);
            let (kx, kz) = crate::targeting::PROP_REMOVED_KEY;
            if flora.is_removed(body, kx, kz, id) {
                continue;
            }
            // `world_sprite` y no `get`: si el slot se recortó de un atlas, con el
            // handle a pelo se plantaba la HOJA ENTERA en el pasto.
            //
            // Y **seeded**: los objetos del pack traen sus colores como variantes
            // (una cama son 56 camas), así que la celda elige cuál. Con la versión
            // sin semilla se dibujaba siempre la primera y un pueblo entero salía
            // del mismo color — que es tener el arte y no usarlo.
            let semilla = ((tx as u64) << 32) ^ (tz as u64 & 0xFFFF_FFFF);
            let Some((mut s, anim)) = art.world_sprite_seeded(e.key, semilla, e.size.into()) else {
                continue;
            };
            // El mueble va donde lo puso el amueblado: centrado en su ancho y con
            // el pie en la línea de su última celda (no en la del ancla).
            let (wx, wz) = match mueble {
                Some(m) => {
                    let (cx_m, pie) = m.centro_pie();
                    (cx_m, pie - 0.5)
                }
                None => (tx as f32 * TILE_M + 0.5, tz as f32 * TILE_M + 0.5),
            };
            let r = render_xy(wx, wz);
            s.custom_size = Some(e.size.into());
            // Pivote al pie (LimeZu): el centro sube medio alto.
            let y = r.y + e.size[1] * 0.5 - 0.5;
            // **La textura del agua va en la banda de piso.** La onda, la
            // hondura, la burbuja, el cardumen y el nenúfar no se levantan del
            // agua: son el agua. Y-sorteados, `light2d` les colgaba su mancha de
            // sombra y el mar abierto salía lleno de manchones oscuros sin nada
            // que los proyecte. Lo que sí tiene volumen —la roca, el tronco— no
            // está en la lista y conserva su sombra. Ver
            // `ha_shared::agua::TEXTURA_DE_AGUA`.
            let capa_slot = if tile.swim() && ha_shared::agua::es_textura_de_agua(e.key) {
                Some(crate::layers::ArtLayer::Floor)
            } else {
                art.slot_layer(e.key)
            };
            let capa = crate::layers::depth(capa_slot, wz);
            let mut ent = commands.spawn((
                SurfaceDecor { cx, cz, z, small: true },
                atenuar_sprite(s, atenuar),
                // Mismo criterio que el decor autorado: una mata marcada piso
                // (musgo, charco, hojas caídas) no debe taparte los pies.
                Transform::from_xyz(r.x, y, capa),
            ));
            // **El semáforo muestra su luz**: una lámpara hija que `vehiculos::lamparas_semaforo`
            // pinta de verde, amarillo o rojo según el reloj del mundo (la misma
            // `luz_en` con la que frena el tránsito). El poste mira contra el tránsito
            // de su brazo, así que gobierna a quien viaja en sentido opuesto.
            if e.key.starts_with("road/semaforo_") {
                if let Some(rumbo) = ha_shared::roads::semaforo_at(body, tx, tz) {
                    crate::vehiculos::agregar_lampara(&mut ent, (rumbo + 4) % 8);
                }
            }
            // **TODO prop es tocable** (15 ago 2026). Hasta acá el dedo sólo
            // encontraba árboles, rocas, vetas y lo que vos hubieras colocado: el
            // banco de la vereda, el farol, el cajón y las 7.821 claves del pack
            // eran pintura de fondo. Un mundo donde la mitad de lo que ves no
            // responde al dedo se siente roto, así que ahora declaran objetivo y
            // se levantan con el mismo gesto que todo lo demás.
            //
            // **Menos lo que es PISO.** El charco, el musgo, las hojas caídas y la
            // textura del agua están marcados `Floor` justamente porque no se
            // levantan del suelo: son suelo. Si fueran objetivo le robarían el
            // toque a la celda —cavar y colocar dejarían de funcionar sobre
            // cualquier baldosa decorada— y encima ofrecerían levantar una mancha.
            if !crate::layers::es_piso(capa_slot) {
                ent.insert((
                    crate::targeting::InteractTarget {
                        kind: InteractTargetKind::Prop,
                        id,
                    },
                    crate::harvest::PropArt(std::borrow::Cow::Borrowed(e.key)),
                ));
            }
            // La cama, la silla, el banco: se usan con un toque (ver `descanso.rs`). `wz` es la fila del pie
            // con la que se Y-sortea, `wx` el centro del sprite.
            if !crate::layers::es_piso(capa_slot) {
                if let Some(u) = crate::descanso::MuebleDeUso::nuevo(e.key, wx, wz, e.size[1]) {
                    ent.insert(u);
                }
            }
            // El ropero, la heladera, la cómoda: se abren (ver `chests.rs`).
            if let (Some(m), Some(tipo)) = (mueble, ha_shared::amueblado::contenedor_de(e.key)) {
                let (x, z) = m.ancla();
                ent.insert(crate::chests::MuebleContenedor { x, z, tipo });
            }
            // **Lo animado se mueve.** `world_sprite_seeded` devolvía la animación
            // y acá se tiraba: el pack trae peceras, fuego, cardúmenes y un cuervo
            // con sus cuadros ya recortados, y el mundo los plantaba congelados.
            // Sólo pagan reloj los slots que declaran animación.
            if let Some(a) = anim {
                // **Un mueble animado se queda quieto hasta que lo tocás**
                // (29 sep 2026, pedido del usuario): la heladera, el televisor, la
                // pecera del living no se mueven solos; se animan al tocarlos y se
                // detienen al volver a tocarlos. El resto (fuego, agua, bichos) sí
                // vive solo.
                if mueble.is_some() {
                    ent.insert((a.quieta(), crate::harvest::AnimaAlTocar));
                } else {
                    ent.insert(a);
                }
            }
            // Faroles, fogatas, hongos brillantes: la luz viene del slot, no de
            // una lista hardcodeada acá.
            crate::light2d::attach(&mut ent, &art, &[e.key], e.size.into());
        }
    }
}

/// Apaga un sprite por un factor multiplicativo. Es cómo se dibuja el mundo que
/// se ve **por el vacío** —la calle desde la azotea, el fondo del pozo—: el mismo
/// [`VOID_DIM`] que ya usa el color del suelo, aplicado al pixel para que el
/// árbol de la vereda no salga a pleno sol sobre un asfalto en penumbra.
///
/// Multiplica el color (no el alpha) a propósito: bajar el alpha lo volvería
/// translúcido y se vería a través, y lo que está abajo no es un fantasma — está
/// lejos y con menos luz.
fn atenuar_sprite(s: Sprite, k: f32) -> Sprite {
    if k < 1.0 {
        tenir_sprite(s, k)
    } else {
        s
    }
}

/// Multiplica el color de un sprite por un factor, **aclare o apague**.
///
/// Es [`atenuar_sprite`] sin el corto por `k < 1.0`, y esa diferencia es toda la
/// gracia: el gris de una celda alta ([`gris_de_celda`]) vale hasta **1,54**, y
/// con el corto el sprite del acantilado se quedaba en el color crudo mientras
/// la malla de al lado subía — el anillo de la meseta.
///
/// Pasarse de 1 no rompe nada: la malla del suelo ya multiplica su losa por el
/// mismo número y el recorte final a blanco lo hace el shader, igual para los
/// dos. El alpha no se toca (ver [`atenuar_sprite`]).
fn tenir_sprite(mut s: Sprite, k: f32) -> Sprite {
    // Sin tinte, ni se toca: el `spawn_chunk_tile_art` pasa por acá con todos los
    // sprites del mundo (agua, puertas, cercas) y sólo el relieve pide gris. Un
    // ida y vuelta por sRGB para multiplicar por uno es trabajo y es una pizca de
    // deriva de coma flotante en cada celda.
    if k == 1.0 {
        return s;
    }
    let c = s.color.to_srgba();
    s.color = Color::srgba(c.red * k, c.green * k, c.blue * k, c.alpha);
    s
}

/// **¿Esta celda del subsuelo da a un hueco?** La propia celda abierta, o maciza
/// pero con una cara contra la galería. Es lo que decide si una veta se ve.
fn expuesta(tiles: &TileWorld, body: CelestialBody, tx: i64, tz: i64) -> bool {
    let z = tiles.z;
    if !tiles.tile_at_level(body, z, tx, tz).solid() {
        return true;
    }
    [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .iter()
        .any(|&(dx, dz)| !tiles.tile_at_level(body, z, tx + dx, tz + dz).solid())
}

/// **Una veta dibujada por código**: pepitas del mineral repartidas en la celda.
///
/// El fallback de una veta no puede ser el cuadrado de color de los demás slots:
/// la veta se mira de cerca y con luz de antorcha, y un cuadrado perfecto de
/// 90 cm de lado no se lee como mineral en la roca — se lee como un bug. Cuatro
/// pepitas sorteadas por la celda (siempre las mismas, es determinista) alcanzan
/// para que se entienda qué es y dónde picar.
fn vetear(padre: &mut bevy::ecs::system::EntityCommands, size: Vec2, color: Color, semilla: u64) {
    let c = color.to_srgba();
    let brillo = Color::srgb(
        (c.red * 1.35).min(1.0),
        (c.green * 1.35).min(1.0),
        (c.blue * 1.35).min(1.0),
    );
    let mut h = semilla | 1;
    let mut az = move || {
        h ^= h << 13;
        h ^= h >> 7;
        h ^= h << 17;
        (h % 1000) as f32 / 1000.0 - 0.5
    };
    let pepitas: Vec<(Vec2, Vec2, Color)> = (0..4)
        .map(|i| {
            let d = Vec2::new(az() * size.x * 0.62, az() * size.y * 0.62);
            let lado = size.x * (0.20 + (i as f32) * 0.045);
            (d, Vec2::splat(lado), if i % 2 == 0 { color } else { brillo })
        })
        .collect();
    padre.with_children(|p| {
        for (d, tam, col) in pepitas {
            p.spawn((
                Sprite::from_color(col, tam),
                Transform::from_xyz(d.x, d.y, 0.001),
            ));
        }
    });
}

fn rock_color(body: CelestialBody) -> Color {
    match body {
        CelestialBody::Moon => Color::srgb(0.55, 0.55, 0.58),
        CelestialBody::Earth => Color::srgb(0.48, 0.48, 0.50),
        _ => Color::srgb(0.52, 0.38, 0.30),
    }
}

fn deposit_style(kind: InteractTargetKind) -> (Color, Vec2) {
    match kind {
        InteractTargetKind::GemRed => (Color::srgb(0.85, 0.20, 0.25), Vec2::new(0.9, 0.9)),
        InteractTargetKind::GemBlue => (Color::srgb(0.25, 0.45, 0.90), Vec2::new(0.9, 0.9)),
        InteractTargetKind::OreGold => (Color::srgb(0.85, 0.70, 0.25), Vec2::new(1.0, 0.9)),
        _ => (Color::srgb(0.62, 0.48, 0.40), Vec2::new(1.0, 0.9)),
    }
}

// ---------------------------------------------------------------------------
// Autotile de tiles con arte (agua, cascadas…) — sprites por tile SÓLO para
// materiales con arte "vivo" (poses o animación). El suelo base sin arte sigue
// siendo el quad vertex-color barato (cero overhead hasta que asignes arte).
// ---------------------------------------------------------------------------

/// Clave de arte del material de un tile, o `None` si ese tile no tiene slot.
/// `pub(crate)`: además del mundo lo usa la mochila, para que la tarjeta de una
/// pieza de construir muestre EL MATERIAL (el piso de madera se ve como piso de
/// madera) en vez de un icono genérico.
///
/// La tabla vive en `ha_shared::topdown::Tile::art_key` — al lado del tile, para
/// que el editor de assets pueda auditar que toda pieza tiene su slot con nombre.
pub(crate) fn tile_art_key(tile: Tile) -> Option<&'static str> {
    tile.art_key()
}

/// **EL SUELO DE UNA CELDA DE RELIEVE ES LA TERRAZA, NO LA ROCA.**
///
/// `Tile::Cliff` y `Tile::Slope` son geometría: dicen "acá se termina la terraza"
/// y "por acá se sube", no de qué está hecho el piso. Pero su color de material
/// es el de la piedra y su slot de suelo (`cliff/roca`) no tiene losa, así que la
/// malla del chunk pintaba **un cuadrado gris pardo por cada celda de borde** —
/// las manchas que se ven en las capturas, salpicadas a lo largo de cada
/// desnivel. Y son celdas por las que se CAMINA: la de un acantilado es el canto
/// de la meseta, la de una rampa es el escalón.
///
/// La pared la dibuja el sprite que cuelga (`terrain_art_key` + el kit de
/// `import-acantilado.py`). El piso de esa celda es el que le tocaría por bioma
/// si el relieve no existiera, que es lo que devuelve esto.
///
/// Es la misma regla que ya manda en el resto del mundo: **la superficie es
/// pasto** (biblia §5), la cumbre incluida.
///
/// La decisión vive en la sim (`geography::superficie_de`): acá sólo se le pide
/// el tile. Lo que era una tabla del cliente pasó a ser LA fuente, para que el
/// borde de la meseta y su relleno no puedan volver a salir de dos sitios.
pub(crate) fn suelo_del_relieve(body: CelestialBody, tx: i64, tz: i64) -> Tile {
    ha_shared::geography::superficie_de(body, tx, tz).tile
}

/// **El material de la cara sale del SUELO que corona, no del bioma.**
///
/// Regla del usuario, sin matices: *"usá donde haya pasto el acantilado de pasto
/// y con arena el de arena"*. La forma en que se rompía era tener DOS tablas
/// paralelas: una de bioma→material para la pared y otra de bioma→tile para el
/// piso, y discrepaban en cuanto un bioma no era del color de su nombre. La
/// sabana es el caso que se vio en la captura —**suelo de pasto, pared de
/// arena**—: `biome_tile(Savanna)` da `Grass` y la tabla vieja daba `"arena"`,
/// así que media pradera salía coronada por un médano.
///
/// Ahora hay UNA fuente: `geography::superficie_de`, el mismo tile que pinta la
/// malla del chunk en esa celda. Si el piso es pasto la pared es de pasto, y no
/// hay forma de que se vuelvan a separar. De paso se fue el barrido en anillos
/// que buscaba tierra seca al lado del agua: la superficie ya devuelve pasto
/// donde el bioma es mojado.
///
/// De qué está hecha la pared que sostiene este suelo. Los cinco materiales son
/// los que corta `scripts/import-acantilado.py`.
fn material_de_suelo(t: Tile) -> &'static str {
    match t {
        Tile::Sand => "arena",
        Tile::Ice => "nieve",
        Tile::Regolith => "regolito",
        Tile::Basalt | Tile::RockRed => "basalto",
        // **La superficie del mundo es pasto, la cumbre incluida** (biblia §5):
        // la sierra, la montaña y la tierra removida beben del mismo verde. Un
        // desnivel de cordillera se lee por su SOMBRA, no porque esté pintado de
        // gris — eso era `cliff/roca`, que no tiene pixel y caía al respaldo por
        // código: los rectángulos grises de las capturas viejas.
        _ => "pasto",
    }
}

/// Los slots de pasto por clima. `tiles/grass` no está: ése es el de
/// referencia, el que se usa cuando el bioma no pide uno propio.
pub(crate) const PASTOS_DE_BIOMA: &[&str] = &[
    "tiles/pasto_bosque",
    "tiles/pasto_seco",
    "tiles/pasto_pantano",
    "tiles/pasto_sierra",
];

/// **La losa de un pasto**, o `None` si le toca el de referencia (`tiles/grass`).
///
/// El mundo entero es pasto por decisión de diseño (§5, "La superficie viva es
/// PASTO"), y eso dejaba a la sabana, al bosque y al pantano pintados del MISMO
/// verde: lo único que los distinguía era cuántos árboles crecían encima. Acá
/// cada clima pide su propia losa, igual que el relieve pide la suya
/// (`cliff_material`): si el slot tiene pixel, se usa; si no, cae al pasto de
/// siempre y no queda una sola celda sin dibujar.
///
/// **De qué pasto es cada celda ya no se decide acá**: lo dice
/// `geography::superficie_de`, la misma respuesta que usa la pared que la corona.
/// Esta tabla sólo traduce esa decisión a una clave de arte.
pub(crate) fn losa_de_pasto(p: ha_shared::geography::Pasto) -> Option<&'static str> {
    use ha_shared::geography::Pasto;
    Some(match p {
        Pasto::Bosque => "tiles/pasto_bosque",
        Pasto::Seco => "tiles/pasto_seco",
        Pasto::Pantano => "tiles/pasto_pantano",
        // La montaña comparte el pastizal de altura de la sierra: desde que la
        // cumbre dejó de ser piedra (`geography::natural_tile`), sin esto salía
        // con el verde de la pradera a 2.000 m.
        Pasto::Sierra => "tiles/pasto_sierra",
        // Pradera y cualquier otro sitio donde crezca pasto: el de referencia.
        Pasto::Referencia => return None,
    })
}

/// El pasto de una celda, ya traducido a clave de arte. Atajo de
/// `losa_de_pasto(superficie_de(...).pasto)`.
fn pasto_de_la_celda(body: CelestialBody, tx: i64, tz: i64) -> Option<&'static str> {
    losa_de_pasto(ha_shared::geography::superficie_de(body, tx, tz).pasto)
}

/// **De qué familia es la vereda de esta ciudad.**
///
/// El pack dibujó seis aceras enteras —crema, crema cálido, gris frío, gris
/// neutro, gris azulado, gris cálido— y el juego usaba una: todas las ciudades
/// del mundo tenían la misma vereda, y la que las distinguía era el trazado y
/// nada más. Acá cada asentamiento se queda con una pareja completa: la baldosa
/// `floor/vereda_tipoN` (malla del chunk) y el escalón `road/cordon_N`
/// (calcomanía sobre el asfalto).
///
/// **La elección es de la CIUDAD, no de la celda.** Sale del centro del
/// asentamiento, que es el único dato estable que tiene: sortearla por celda
/// pondría el cordón de una familia sobre la baldosa de otra cada tres metros, y
/// ése fue siempre el motivo por el que había una sola. Fuera de un casco —una
/// ruta de campo, un cruce suelto— manda la de siempre.
///
/// Devuelve `None` para la familia base, que es la que usa las claves sin
/// sufijo. Sólo se ofrece una familia si el arte de las DOS mitades está
/// cargado: media pareja se ve peor que la vereda de siempre.
fn vereda_de_la_ciudad(art: &ArtLibrary, body: CelestialBody, tx: i64, tz: i64) -> usize {
    // Sin ninguna familia alternativa cargada no hay nada que elegir, y
    // preguntar por el asentamiento costaría nueve consultas de sitio por celda
    // de acera para tirar la respuesta. Ver `ArtLibrary::veredas_de_ciudad`.
    if !art.veredas_de_ciudad() {
        return 0;
    }
    let Some(s) = ha_shared::geography::settlement_covering(body, tx, tz) else {
        return 0;
    };
    // Mezcla barata del centro: dos ciudades vecinas no caen en la misma familia
    // por estar en celdas de sitio contiguas.
    let mut h = (s.center.0 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (s.center.1 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 31;
    let fam = (h % VEREDA_BALDOSA.len() as u64) as usize;
    // Media pareja se ve peor que la vereda de siempre: si a la familia le falta
    // la baldosa o le falta el cordón, no se usa.
    let completa = art.suelo_uv(VEREDA_BALDOSA[fam]).is_some() && art.has_poses(VEREDA_CORDON[fam]);
    if completa {
        fam
    } else {
        0
    }
}

/// Las baldosas de vereda del pack, por familia. La 0 es la de siempre y por eso
/// no lleva sufijo: es la clave que ya existía y a la que cae todo lo que no es
/// una ciudad con familia propia.
pub(crate) const VEREDA_BALDOSA: [&str; 6] = [
    "floor/vereda",
    "floor/vereda_tipo2",
    "floor/vereda_tipo3",
    "floor/vereda_tipo4",
    "floor/vereda_tipo5",
    "floor/vereda_tipo6",
];
/// El escalón que va con cada baldosa. Índices en paralelo a [`VEREDA_BALDOSA`]:
/// la pareja viaja junta o no viaja.
pub(crate) const VEREDA_CORDON: [&str; 6] = [
    "road/cordon",
    "road/cordon_2",
    "road/cordon_3",
    "road/cordon_4",
    "road/cordon_5",
    "road/cordon_6",
];

/// La losa de suelo que le toca a esta celda, ya resuelta a UV de la hoja.
///
/// Es el punto donde el pasto elige clima. Vive aparte de `tile_art_key` porque
/// esa función es del TILE (y vive en `ha-shared`, espejada en TS), mientras que
/// "de qué es el pasto acá" es del SITIO — hace falta el cuerpo y la celda.
///
/// `vereda` es la familia de acera de la ciudad de este chunk, ya resuelta por
/// quien llama: se decide UNA vez por chunk y no por celda, para que la baldosa
/// que dibuja la malla y el escalón que dibuja `spawn_chunk_cordon` no puedan
/// discrepar nunca — si cada uno la resolviera por su cuenta, en el borde del
/// casco caería el cordón de una familia sobre la baldosa de otra.
fn suelo_de_la_celda(
    art: &ArtLibrary,
    body: CelestialBody,
    tile: Tile,
    tx: i64,
    tz: i64,
    vereda: usize,
) -> Option<[f32; 4]> {
    let semilla = celda_semilla(tx, tz);
    // El `pasto_por_bioma` va PRIMERO a propósito: sin losas de clima cargadas,
    // preguntar el bioma sería pagar media docena de fbm por celda para tirar la
    // respuesta a la basura. Ver `ArtLibrary::pasto_por_bioma`.
    if tile == Tile::Grass && art.pasto_por_bioma() {
        // Por `superficie_de_tile`, no por el bioma: lo que el jugador cavó gana
        // sobre lo que dice el clima, y sobre relieve manda la terraza — la misma
        // respuesta que va a usar la pared que corona esta celda.
        let s = ha_shared::geography::superficie_de_tile(body, tile, tx, tz);
        if let Some(uv) = losa_de_pasto(s.pasto).and_then(|k| art.suelo_uv_seeded(k, semilla)) {
            return Some(uv);
        }
    }
    // La acera es de la CIUDAD, no del tile: cada asentamiento tiene la suya.
    if tile == Tile::Pavement {
        if let Some(uv) = art.suelo_uv_seeded(VEREDA_BALDOSA[vereda], semilla) {
            return Some(uv);
        }
    }
    art.suelo_uv_seeded(tile_art_key(tile)?, semilla)
}

/// ¿Esta puerta ya la dibuja la fachada del pack?
///
/// Cierto sólo en la fila del FRENTE (la cara sur) de un lote con fachada: es la
/// única que el pack dibuja de elevación, y por lo tanto la única donde se
/// encimaban dos puertas. Ver el porqué en el llamador.
fn puerta_la_dibuja_la_fachada(body: CelestialBody, z: i32, tx: i64, tz: i64) -> bool {
    // Las fachadas de ciudad sólo se generan en la planta baja
    // (`building::stream_roofs`); en cualquier otra, callar la puerta del tile
    // la dejaría sin nadie que la dibuje.
    if z != 0 {
        return false;
    }
    let Some(s) = ha_shared::geography::settlement_covering(body, tx, tz) else {
        return false;
    };
    let Some((_, z0, _, h, _)) = ha_shared::city::roofed_rect(body, &s, tx, tz) else {
        return false;
    };
    tz == z0 + h - 1
}

/// **La pared de INTERIOR de esta casa, y la cara que se le ve.**
///
/// Devuelve `(clave, pose)`. Es la razón por la que existen las 54 paredes de
/// `scripts/import-paredes-interiores.py`: sin esto, toda casa generada del
/// mundo se dibuja con `wall/wood` —el mismo marrón— y las otras 53 se quedan en
/// el manifest sin que nadie las pida. Era el bug de fondo detrás de "los
/// assets de paredes interiores no se usan": no faltaba pixel, **faltaba quien
/// lo pidiera**.
///
/// ## El estilo es de la CASA, no de la celda
///
/// Se sortea con el origen del rect edificado, así que las cuatro paredes de un
/// cuarto son del mismo empapelado y la casa lo conserva entre partidas sin
/// guardar nada. Sortearlo por celda —que es lo que hace el suelo urbano— acá
/// sería una pared distinta cada 95 cm: un cuarto se lee por su pared, y una
/// pared que cambia de material a cada paso no se lee como cuarto.
///
/// ## La cara sale de la GEOMETRÍA, no de un número escrito
///
/// El pack dibuja el paño de los dos lados y los dos cantos, y cuál se ve lo
/// decide qué borde del rect es esta celda. En cenital 3/4 la pared del NORTE
/// enseña su lado de adentro (mira al sur, hacia la cámara) y la del SUR su lado
/// de afuera. Es la misma regla que `walls::Cara` ya aplicaba a lo que construye
/// el jugador; lo que faltaba era aplicarla a lo que genera la ciudad.
/// La última casa resuelta por [`arte_de_interior`], para no volver a barrer el
/// lote en la celda de al lado.
///
/// `roofed_rect` cuelga de `city::lot_at`, que **corre por tile** y prueba hasta
/// doce anclas. Un chunk de ciudad tiene cientos de celdas de casa y casi todas
/// son de la misma que la anterior, así que con recordar UN rect se saltea el
/// barrido en la enorme mayoría. Es la misma idea que el memo por hilo de
/// `settlement_in` (§10), a la escala de un chunk.
#[derive(Default)]
struct CasaResuelta {
    rect: Option<(i64, i64, i64, i64)>,
    estilo: Option<ha_assets::EstiloInterior>,
}

/// **El arte de interior de ESTA casa para ESTA celda**: `(clave, pose)`.
///
/// Es la razón por la que existen los 54 estilos de
/// `scripts/import-paredes-interiores.py`. Sin esto, toda casa generada del
/// mundo se dibuja con `wall/wood` y `floor/wood` —el mismo marrón— y los otros
/// 53 se quedan en el manifest sin que nadie los pida. Era el bug de fondo
/// detrás de "los assets de paredes interiores no se usan": no faltaba pixel,
/// **faltaba quien lo pidiera**.
///
/// ## El estilo es de la CASA, no de la celda
///
/// Se sortea con el origen del rect edificado, así que la pared, el piso y los
/// arcos de una casa combinan entre sí, y la casa lo conserva entre partidas sin
/// guardar nada. Sortearlo por celda —que es lo que hace el suelo urbano— sería
/// acá un empapelado distinto cada 95 cm.
///
/// ## La cara sale de la GEOMETRÍA, no de un número escrito
///
/// El pack dibuja el paño de adentro y el de afuera, y cuál se ve lo decide qué
/// borde del rect es esta celda. Es la misma regla que `walls::Cara` ya aplicaba
/// a lo que construye el jugador; lo que faltaba era aplicarla a lo que genera
/// la ciudad.
fn arte_de_interior(
    art: &ArtLibrary,
    tiles: &TileWorld,
    body: CelestialBody,
    tile: Tile,
    tx: i64,
    tz: i64,
    memo: &mut CasaResuelta,
) -> Option<(&'static str, &'static str)> {
    if !matches!(tile, Tile::WoodWall | Tile::StoneBrick | Tile::WoodFloor | Tile::Door | Tile::Window) {
        return None;
    }
    let estilos = ha_assets::interior_styles();
    if estilos.is_empty() {
        return None; // sin importar todavía: se sigue viendo como antes
    }
    // ¿Sigue siendo la casa de la celda anterior? El borde CUENTA: las paredes
    // son justamente el perímetro del rect.
    let (x, z0, w, h, estilo) = match (memo.rect, memo.estilo) {
        (Some((mx, mz, mw, mh)), Some(e))
            if tx >= mx && tx < mx + mw && tz >= mz && tz < mz + mh =>
        {
            (mx, mz, mw, mh, e)
        }
        _ => {
            // Un interior (banda de pisos propia) trae su rectángulo del plano; una casa del mapa, de la ciudad.
            let (x, z0, w, h) = if ha_shared::interiores::es_interior(tiles.z) {
                let r = ha_shared::interiores::rect_de_z(tiles.z)?;
                (r.0, r.1, r.2, r.3)
            } else {
                let s = ha_shared::geography::settlement_covering(body, tx, tz)?;
                let (x, z0, w, h, _) = ha_shared::city::roofed_rect(body, &s, tx, tz)?;
                (x, z0, w, h)
            };
            let idx = ha_shared::worldgen::hash_cell(0x1_7ee2, x, z0) as usize % estilos.len();
            memo.rect = Some((x, z0, w, h));
            memo.estilo = Some(estilos[idx]);
            (x, z0, w, h, estilos[idx])
        }
    };
    let perimetro = tz == z0 || tz == z0 + h - 1 || tx == x || tx == x + w - 1;
    let clave = match tile {
        Tile::WoodFloor => estilo.piso,
        // **Un vano de este kit es un HUECO, no un arco.** `Room_Builder_3d_walls`
        // no dibuja "la pared con el agujero": deja pasar el piso y remata el muro
        // a cada lado con su jamba (`end_e`/`end_w`, medidas en `Generic_Home_1`).
        // Así que acá se devuelve la LOSA y el hueco lo dibujan los dos muros que
        // lo flanquean. En el perímetro no, que ahí va la hoja de la fachada.
        Tile::Door if !perimetro => estilo.piso,
        Tile::Door => return None,
        _ => estilo.pared,
    };
    if !art.tile_has_art(clave) {
        return None;
    }
    if !matches!(tile, Tile::WoodWall | Tile::StoneBrick | Tile::Window) {
        return Some((clave, "center"));
    }
    let cara = cara_de_muro(tiles, body, tx, tz)?;
    // **UNA VENTANA EN UN MURO DE COSTADO SE VE DE CANTO** (29 sep 2026). El
    // arte de ventana es un paño DE FRENTE; plantado en un muro norte-sur salía
    // "en vertical pero horizontal", como dijo el usuario: una ventana de frente
    // acostada sobre la tira de la pared. En cenital 3/4 de un muro de costado se
    // ve la tapa, no el paño, así que ahí va la pieza del muro. Sólo el muro que
    // enseña su paño (`edge_n`, el del fondo del cuarto) lleva el vidrio; el que
    // da a la calle lo dibuja la fachada.
    // (En un interior la ventana del muro del fondo SÍ lleva el paño con su borde: el vidrio se le pone encima,
    // `interiores::vidrios_de_adentro`; sin paño quedaba un hueco sin el borde de la pared.)
    if tile == Tile::Window && cara == "edge_n" && !ha_shared::interiores::es_interior(tiles.z) {
        return None;
    }
    Some((clave, cara))
}

/// **Qué pieza del kit de cuarto le toca a este muro**, o `None` si no le toca
/// ninguna.
///
/// El kit es `Room_Builder_3d_walls`, que es LA hoja con la que LimeZu arma un
/// cuarto y estaba sin importar: trae el muro horizontal con su tapa, los
/// verticales vistos de canto y las esquinas en ángulo. Antes se usaba
/// `Room_Builder_Walls`, que dibuja el paño de frente y nada más — con eso un muro
/// norte-sur salía como una hilera de carteles de frente puestos de costado, y la
/// columna que el importador publicaba como "cara de afuera" era el MISMO
/// empapelado (medido: (204,208,211) contra (208,212,215)), así que toda casa
/// generada enseñaba el empapelado del dormitorio mirando a la vereda.
///
/// ## La pieza sale de DÓNDE ESTÁ EL CUARTO, no de qué borde del rect es
///
/// Un tabique y una pared de borde son la misma pared mirada desde el mismo lado;
/// lo que decide qué se ve de ella es para dónde le queda el piso. Con la regla
/// vieja —"si es perímetro, `back`"— un tabique norte-sur y el muro oeste de la
/// casa pedían piezas distintas siendo la misma pared.
///
/// ## Y la pared que da a la calle no tiene arte de interior
///
/// Si el cuarto le queda al NORTE, de esa pared se ve la tapa y nada más: lo que
/// mira al frente es la FACHADA (Modern Exteriors). Dibujarle el paño de interior
/// era plantar el empapelado contra la vereda **y** taparle al cuarto su última
/// fila de piso, porque el muro horizontal mide dos celdas de alto.
fn cara_de_muro(tiles: &TileWorld, body: CelestialBody, tx: i64, tz: i64) -> Option<&'static str> {
    let en = |dx: i64, dz: i64| tiles.render_tile(body, tx + dx, tz + dz);
    let piso = |dx: i64, dz: i64| matches!(en(dx, dz), Tile::WoodFloor | Tile::Door);
    let (sur, este, oeste, norte) = (piso(0, 1), piso(1, 0), piso(-1, 0), piso(0, -1));
    // **La JAMBA de un vano.** El muro que termina contra una puerta remata con su
    // pieza de canto; es lo que dibuja el hueco, porque el hueco en sí es piso.
    if sur || norte {
        if en(1, 0) == Tile::Door {
            return Some("end_e");
        }
        if en(-1, 0) == Tile::Door {
            return Some("end_w");
        }
    }
    if sur {
        return Some("edge_n");
    }
    if este {
        return Some("edge_w");
    }
    if oeste {
        return Some("edge_e");
    }
    // Las esquinas: el cuarto queda en diagonal. **Si el cuarto queda en las DOS diagonales de abajo es una T** (un
    // tabique que muere contra el muro del fondo): el muro sigue de largo, no hay esquina.
    if piso(1, 1) && piso(-1, 1) {
        return Some("edge_n");
    }
    if piso(1, 1) {
        return Some("corner_nw");
    }
    if piso(-1, 1) {
        return Some("corner_ne");
    }
    if norte {
        return Some("edge_s");
    }
    // Las de abajo: el cuarto queda al nordeste / noroeste. (Antes caían en `edge_n` y se dibujaba un muro de fondo
    // con su cara en la esquina inferior.) En T —un tabique que llega al muro de abajo— sigue el remate de abajo.
    if piso(1, -1) && piso(-1, -1) {
        return Some("edge_s");
    }
    if piso(1, -1) {
        return Some("corner_sw");
    }
    if piso(-1, -1) {
        return Some("corner_se");
    }
    Some("edge_n")
}

/// La clave de arte de un tile de RELIEVE con su variante de bioma, si esa
/// variante está curada. `None` = usá la genérica de `tile_art_key`.
fn terrain_art_key(
    art: &ArtLibrary,
    body: CelestialBody,
    tile: Tile,
    tx: i64,
    tz: i64,
) -> Option<&'static str> {
    // **EL AGUA TAMBIÉN ELIGE FAMILIA POR BIOMA.** El pack dibujó tres riberas
    // —pasto, playa de arena y orilla pelada— y el juego usaba la de pasto en
    // todas partes: una laguna del desierto salía con el pastito verde alrededor
    // y el corte contra la arena se veía a un kilómetro. Es la misma regla del
    // acantilado, aplicada al líquido (ver `ha_shared::agua::orilla_de`).
    if tile == Tile::Water {
        let key = ha_shared::agua::orilla_de(body, tx, tz).art_key();
        return art.tile_has_art(key).then_some(key);
    }
    if !matches!(tile, Tile::Cliff | Tile::Slope) {
        return None;
    }
    // **EL RELIEVE SE TIÑE DEL PASTO QUE CORONA.**
    //
    // La pieza del kit trae su meseta dibujada —así es como se ve el asset y así
    // va al mundo—, y el mundo tiene CINCO pastos: el de referencia más los
    // cuatro de clima. Con una sola variante, la franja de arriba de la pared
    // salía de otro verde que la losa de al lado: medido en la captura del campo,
    // (86,165,92) contra un pasto de sabana de (135,147,89), una barra de un tile
    // de ancho recorriendo el mundo.
    //
    // Se arregla eligiendo la variante teñida con SU losa —el importador la corta
    // anclando el tono dominante de la meseta en el color de la losa— y no
    // vaciándole el suelo al dibujo, que fue el intento anterior y dejó el muro
    // hecho bloques de roca pelada. Vale para las tres familias: la pared, el
    // bordillo y la escalinata pisan el mismo pasto.
    //
    // **Y el material y el clima salen de UNA consulta.** Antes el material venía
    // del bioma liso y el clima del difuminado: en la franja del ajedrez eso era
    // una pared de sierra sobre una losa de pradera, y la pared cambiaba de
    // variante celda a celda. `superficie_de` contesta las dos cosas, y es la
    // misma respuesta con la que la malla pinta esta celda.
    //
    // Sin la variante curada cae a la de siempre y no cambia nada.
    let s = ha_shared::geography::superficie_de(body, tx, tz);
    if material_de_suelo(s.tile) == "pasto" {
        let familia = match tile {
            Tile::Cliff if ha_shared::relief::alto_de_pared(body, tx, tz) == 1 => "ledge",
            Tile::Cliff => "cliff",
            _ => "slope",
        };
        use ha_shared::geography::Pasto;
        let clima: Option<&'static str> = match (familia, s.pasto) {
            ("cliff", Pasto::Bosque) => Some("cliff/pasto_bosque"),
            ("cliff", Pasto::Seco) => Some("cliff/pasto_seco"),
            ("cliff", Pasto::Pantano) => Some("cliff/pasto_pantano"),
            ("cliff", Pasto::Sierra) => Some("cliff/pasto_sierra"),
            ("ledge", Pasto::Bosque) => Some("ledge/pasto_bosque"),
            ("ledge", Pasto::Seco) => Some("ledge/pasto_seco"),
            ("ledge", Pasto::Pantano) => Some("ledge/pasto_pantano"),
            ("ledge", Pasto::Sierra) => Some("ledge/pasto_sierra"),
            ("slope", Pasto::Bosque) => Some("slope/pasto_bosque"),
            ("slope", Pasto::Seco) => Some("slope/pasto_seco"),
            ("slope", Pasto::Pantano) => Some("slope/pasto_pantano"),
            ("slope", Pasto::Sierra) => Some("slope/pasto_sierra"),
            _ => None,
        };
        if let Some(k) = clima {
            if art.tile_has_art(k) {
                return Some(k);
            }
        }
    }
    // BORDILLO vs PARED, **y lo decide la altura**. Un desnivel de un solo
    // escalón se trepa (ver `TileWorld::walk_blocked`) y el jugador tiene que
    // poder VERLO antes de caminar hacia él: si se dibujara igual que un farallón
    // de ocho pisos, la única forma de saber cuál es cuál sería chocarse. Su arte
    // vive en `ledge/*` (dos celdas) con el mismo autotile que el resto; mientras
    // no lo cures, cae al `cliff/*` de siempre y todo sigue igual.
    //
    // Antes esto lo decidía una "cota trepable" regional porque TODAS las caras
    // del mundo medían un escalón y la altura no distinguía nada. Con la escalera
    // de `relief::SALTO_DE_TERRAZA` la altura vuelve a ser el dato, y de paso el
    // parpadeo que motivaba aquella regla desaparece por construcción: el alto es
    // el mismo a lo largo de todo el tramo, porque las dos terrazas lo son.
    if tile == Tile::Cliff && ha_shared::relief::alto_de_pared(body, tx, tz) == 1 {
        let ledge: &'static str = match material_de_suelo(s.tile) {
            "arena" => "ledge/arena",
            "nieve" => "ledge/nieve",
            "pasto" => "ledge/pasto",
            "regolito" => "ledge/regolito",
            "basalto" => "ledge/basalto",
            _ => "ledge/roca",
        };
        if art.tile_has_art(ledge) {
            return Some(ledge);
        }
    }
    let key: &'static str = match (tile, material_de_suelo(s.tile)) {
        (Tile::Cliff, "arena") => "cliff/arena",
        (Tile::Cliff, "nieve") => "cliff/nieve",
        (Tile::Cliff, "pasto") => "cliff/pasto",
        (Tile::Cliff, "regolito") => "cliff/regolito",
        (Tile::Cliff, "basalto") => "cliff/basalto",
        (Tile::Slope, "arena") => "slope/arena",
        (Tile::Slope, "nieve") => "slope/nieve",
        (Tile::Slope, "pasto") => "slope/pasto",
        (Tile::Slope, "regolito") => "slope/regolito",
        (Tile::Slope, "basalto") => "slope/basalto",
        // "roca" y cualquier material sin variante propia: la genérica sirve.
        _ => return None,
    };
    art.tile_has_art(key).then_some(key)
}

/// **El costado de un paso.**
///
/// Devuelve la pose de mejilla que le toca a una celda de ACANTILADO pegada a
/// una rampa, o `None` si esta celda no está al lado de ningún paso.
///
/// El kit del pack no dibuja la escalinata suelta: la dibuja con sus dos
/// espolones de roca, uno a cada lado (`Four_Sand_Mountains_1_Bottom`, columnas
/// 0 y 3). Esos espolones son PARED —la celda de acantilado, no la del paso— y
/// por eso viven en `cliff/*` y se piden desde acá. Es la regla 8 del proyecto
/// aplicada al relieve: la escalinata es un kit, no una losa suelta.
///
/// Sólo se pide para las caras que MIRAN AL SUR (la terraza de abajo está al
/// sur): son las únicas que enseñan la pared entera, y el espolón es un dibujo
/// de esa pared. En una cara que se ve de canto no habría dónde ponerlo.
fn mejilla_de_rampa(
    body: CelestialBody,
    tile: Tile,
    tx: i64,
    tz: i64,
) -> Option<&'static str> {
    use ha_shared::relief::{alto_de_pared, relief_cell, ReliefCell};
    if tile != Tile::Cliff {
        return None;
    }
    // **Sólo el bordillo lleva mejilla.** El espolón del pack es una pieza de
    // 1×3 celdas fijas: pegado a una pared de ocho pisos sería un tapón de roca
    // a media altura. Una pared alta muere contra el paso con sus propias filas
    // apiladas (`pared_apilada`), que es lo que hace que el paso se lea como un
    // desfiladero y no como un agujero cuadrado.
    if alto_de_pared(body, tx, tz) != 1 {
        return None;
    }
    // El paso al ESTE ⇒ esta celda es su mejilla OESTE, y viceversa.
    if relief_cell(body, tx + 1, tz) == ReliefCell::Ramp {
        return Some("rampa_o");
    }
    if relief_cell(body, tx - 1, tz) == ReliefCell::Ramp {
        return Some("rampa_e");
    }
    None
}

/// Elige la forma autotile de una celda según si sus 8 vecinos son del MISMO
/// material (`true` = mismo → sin borde de ese lado). El borde "mira" hacia los
/// lados DISTINTOS. Los nombres son los que escribe el editor
/// (`center`/`edge_*`/`corner_*`/`inner_*`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn autotile_shape(n: bool, e: bool, s: bool, w: bool, ne: bool, nw: bool, se: bool, sw: bool) -> &'static str {
    match (!n, !e, !s, !w) {
        // Todos los lados iguales: puede ser esquina cóncava si una diagonal difiere.
        (false, false, false, false) => {
            if !ne { "inner_ne" } else if !nw { "inner_nw" } else if !se { "inner_se" } else if !sw { "inner_sw" } else { "center" }
        }
        // Un solo lado distinto → borde de ese lado.
        (true, false, false, false) => "edge_n",
        (false, true, false, false) => "edge_e",
        (false, false, true, false) => "edge_s",
        (false, false, false, true) => "edge_w",
        // Dos lados adyacentes distintos → esquina convexa.
        (true, true, false, false) => "corner_ne",
        (true, false, false, true) => "corner_nw",
        (false, true, true, false) => "corner_se",
        (false, false, true, true) => "corner_sw",
        // **LADOS OPUESTOS O TRES DISTINTOS: EL PACK NO DIBUJÓ ESA PIEZA.**
        //
        // Acá decía `center`, o sea el INTERIOR del material — y para el agua eso
        // es un cuadrado celeste liso sin orilla, plantado en mitad del pasto. Es
        // el bug que el usuario marcó en círculo rojo por todo el borde de la
        // laguna. Lo que de verdad lo arregla es no generar celdas que no se
        // pueden dibujar (`ha_shared::geography::orilla_dibujable`); esto es la
        // red por si alguna se escapa: se elige la esquina que enseñe orilla por
        // los dos lados que la tienen, empezando por el SUR, que en 3/4 es el que
        // se ve entero.
        _ => match (!n, !e, !s, !w) {
            (_, true, true, _) => "corner_se",
            (_, _, true, true) => "corner_sw",
            (true, true, _, _) => "corner_ne",
            (true, _, _, true) => "corner_nw",
            // Canal de un tile (los dos lados opuestos en tierra): no hay pieza
            // que enseñe las dos orillas, y el interior es lo menos malo.
            _ => "center",
        },
    }
}

/// Semilla estable de una celda del mundo, para elegir su variante de arte.
///
/// Determinista y sin estado: la misma celda saca siempre la misma baldosa, aunque
/// el chunk entre y salga del streaming diez veces. Es lo que permite variar el
/// suelo sin guardar un byte por tile.
pub(crate) fn celda_semilla(tx: i64, tz: i64) -> u64 {
    (tx as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (tz as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
}

/// Spawnea sprites por-tile para los materiales con arte "vivo" del chunk (agua…),
/// eligiendo la variante autotile por vecinos. Cada sprite lleva `SurfaceDecor`
/// para despawnearse con el streaming de chunks. Sólo la superficie (z==0) por
/// ahora; los tiles sin arte no cuestan nada.
fn spawn_chunk_tile_art(
    commands: &mut Commands,
    art: &ArtLibrary,
    tiles: &TileWorld,
    body: CelestialBody,
    z: i32,
    cx: i64,
    cz: i64,
) {
    // Casi todas las paredes de un chunk son de la misma casa (ver `CasaResuelta`).
    let mut casa_memo = CasaResuelta::default();
    for lx in 0..CHUNK {
        let tx = cx * CHUNK + lx;
        // **La franja oscura al pie de los muros de esta columna**, la misma que
        // pinta la malla ([`sombra_de_muro_en_la_columna`]). Hace falta porque el
        // sprite del relieve se tiñe con el gris de su celda y ese gris la
        // incluye; se calcula **perezosa** —una vez, y sólo si esta columna llega
        // a tener relieve— porque barre `MAX_PARED + SOMBRA_FILAS` filas de más
        // por columna y la mayoría de los chunks no tiene un solo acantilado.
        let mut sombra_muro: Option<[f32; CHUNK as usize]> = None;
        for lz in 0..CHUNK {
            let tz = cz * CHUNK + lz;
            // El arte sigue a lo que se DIBUJA, no a la capa en la que estás: bajo
            // tierra la casa de arriba conserva su arte y sólo tu galería se abre.
            let tile = tiles.render_tile(body, tx, tz);
            let Some(generic) = tile_art_key(tile) else { continue };
            // La pileta se dibuja con las piezas de pileta del pack (`spawn_chunk_piletas`), no con la orilla de un lago.
            if tile == Tile::Water && es_agua_de_pileta(tiles, body, tx, tz) {
                continue;
            }
            // El relieve elige su arte por BIOMA: la misma pared es roca desnuda
            // en la sierra, arena en el desierto y nieve en la cumbre. Si esa
            // variante no está curada todavía, cae a la genérica y se ve igual que
            // antes — nunca queda una celda sin dibujar por falta de arte.
            let key = terrain_art_key(art, body, tile, tx, tz).unwrap_or(generic);
            // **El tipo de cerca es de la PARCELA**, no de la celda: reja con base de ladrillo o murete de piedra.
            let key = if tile == Tile::Fence { cerca_de_la_parcela(body, tx, tz).unwrap_or(key) } else { key };
            // **Una casa se dibuja con SU estilo**: su paño, su losa y sus arcos.
            // Se resuelve acá, con la clave, porque de esto depende también qué
            // cara se pide más abajo (`cara_de_pared`).
            let interior = arte_de_interior(art, tiles, body, tile, tx, tz, &mut casa_memo);
            let key = interior.map(|(k, _)| k).unwrap_or(key);
            let cara_de_pared = interior.map(|(_, c)| c);
            let wx = tx as f32 * TILE_M + 0.5;
            let wz = tz as f32 * TILE_M + 0.5;
            let r = render_xy(wx, wz);
            // **La ventana del muro del fondo va SOBRE su pared.** `window/basic` es un recorte de 1×2 con la fila de arriba
            // transparente y marco de ladrillo: sola, en el muro de adentro dejaba un hueco por donde se veía el pasto
            // (las paredes «cortadas en las ventanas»). Se dibuja primero el paño del estilo de la casa y encima el vidrio.
            if tile == Tile::Window && interior.is_none() {
                if let (Some(est), Some(cara)) = (casa_memo.estilo, cara_de_muro(tiles, body, tx, tz)) {
                    if art.tile_has_art(est.pared) {
                        let m = art.medida_de_obra(est.pared, Some(cara), Vec2::splat(TILE_M));
                        if let Some((sprite, _)) = art.tile_sprite(est.pared, cara, m) {
                            commands.spawn((
                                SurfaceDecor { cx, cz, z, small: false },
                                TileArt,
                                sprite,
                                Transform::from_xyz(r.x, r.y + (m.y - TILE_M) * 0.5, crate::layers::ysort(wz) - 0.0004),
                            ));
                        }
                    }
                }
            }
            // Puertas: pose open/closed animada por proximidad (building.rs las
            // conmuta). Con arte de poses, spawn especial; si no, sigue al autotile.
            // **UNA ABERTURA, UNA PUERTA.**
            //
            // La fachada del pack ya trae su puerta DIBUJADA y animada
            // (`building::spawn_puerta_de_fachada`), y el sim además estampa su
            // `Tile::Door` en la misma cara. O sea que la cara a la calle de
            // toda casa generada tenía dos puertas encimadas: la del pixel del
            // artista y la del tile, con pose y posición distintas. Es el mismo
            // bug que `edificio.rs` documenta como razón de existir ("las
            // aberturas son UNA sola lista, las dos caras leen el mismo dato"),
            // sólo que en la ciudad generada nadie lo estaba evitando.
            //
            // Manda la fachada, porque es la que tiene el dibujo bueno: sólo se
            // calla el SPRITE del tile, no el tile. Se sigue cruzando por acá, y
            // las otras tres caras —que no tienen fachada dibujada— conservan la
            // suya, que es por donde se entra cuando la calle se comió el frente.
            if tile == Tile::Door && puerta_la_dibuja_la_fachada(body, z, tx, tz) {
                continue;
            }
            if tile == Tile::Door && art.has_poses(key) {
                if let Some((_, _)) = art.pose(key, "closed").or_else(|| art.pose(key, "open")) {
                    // ALTA Y APOYADA EN SU PIE, igual que las paredes de borde.
                    // Dibujada 1×1 quedaba más baja que el jugador: una puerta
                    // por la que no cabés no se lee como una puerta. Ocupa un
                    // tile de SUELO y `WALL_RENDER_H` tiles de ALTO; el pie se
                    // queda clavado en la celda, así que la colisión y el
                    // Y-sort no cambian — sólo crece hacia arriba en pantalla.
                    let size = crate::building::door_render_size(art, key);
                    let sprite = art.sprite_or_color(key, Color::srgb(0.30, 0.20, 0.10), size);
                    let mut ent = commands.spawn((
                        SurfaceDecor { cx, cz, z, small: false },
                        TileArt,
                        crate::building::DoorArt { tx, tz, open: false, art: key, t: 0.0 },
                        sprite,
                        Transform::from_xyz(
                            r.x,
                            r.y + (size.y - TILE_M) * 0.5,
                            sort_z(wz),
                        ),
                    ));
                    crate::light2d::attach(&mut ent, &art, &[key], size);
                    continue;
                }
            }
            // Los escalones los dibuja `escaleras::dibujar_escalones` orientados al tramo (el arte del
            // pack sube de norte a sur; un tramo que sube de izquierda a derecha necesita girarlo).
            if matches!(tile, Tile::Step | Tile::Stairwell) {
                continue;
            }
            if !art.tile_has_art(key) {
                continue;
            }
            // **EL CUERPO DE UN MURO NO PIDE PIEZA: LA PIDIÓ SU CANTO.**
            //
            // Un farallón de ocho pisos tapa nueve filas de la terraza de abajo,
            // y esas filas son roca (`ReliefCell::Pared`, tile de acantilado). Si
            // cada una pidiera su propia cara, el mismo muro se dibujaría nueve
            // veces solapado y cada copia taparía a la de arriba: en pantalla, la
            // pared entera reducida a su última fila. Lo dibuja el canto, de una
            // pieza, en `pared_apilada`.
            if matches!(
                ha_shared::relief::relief_cell(body, tx, tz),
                ha_shared::relief::ReliefCell::Pared
            ) {
                continue;
            }
            // **DONDE CAE UNA CASCADA, MANDA LA GARGANTA.** El recorte del pack
            // ya trae la roca de los dos costados, y el muro del relieve no le
            // coincide: mide tres celdas fijas contra las que mida el chorro, y
            // las tres columnas del salto pueden arrancar en filas distintas
            // (`agua::salto_vecino` tolera un tile). El labio de roca asomaba por
            // encima del agua — ver `agua::garganta_de_cascada`.
            if tile == Tile::Cliff && ha_shared::agua::garganta_de_cascada(body, tx, tz) {
                continue;
            }
            // **Y EL HOMBRO LO DIBUJA EL MÓDULO DE LA GARGANTA**, no el autotile.
            // La pieza que le tocaría por relieve empieza y termina donde manda
            // el borde de terraza, que no es donde cae el chorro
            // (`agua::hombro_de_garganta`): dejarlas a las dos deja dos paredes
            // solapadas a media fila de distancia.
            if tile == Tile::Cliff && ha_shared::agua::hombro_de_garganta(body, tx, tz).is_some() {
                continue;
            }
            // **Para el bajío, "igual" es LÍQUIDO, no "el mismo tile".**
            //
            // El autotile compara la celda con su vecina; con la comparación
            // literal, el borde entre el bajío y la hondura contaba como tierra
            // y el agua se dibujaba a sí misma una ORILLA CON BARRO en mitad del
            // lago, justo donde el fondo se hunde. Son el mismo líquido: lo que
            // cambia es la profundidad, y eso ya lo dibuja `tiles/water_deep`
            // encima. La hondura sí compara literal — su borde contra el bajío
            // es precisamente lo que tiene que dibujar.
            // **Y para el RELIEVE, "igual" es "no se cae para ese lado".**
            //
            // Un borde de terraza es una banda de UN tile: comparando el tile
            // con el de al lado, sus vecinos de los costados son acantilado
            // (igual) y los de arriba y abajo no (distintos) — o sea lados
            // OPUESTOS distintos, que `autotile_shape` resuelve como "center".
            // Resultado: las 32.000 caras del planeta pedían la misma pieza y
            // daba exactamente lo mismo hacia dónde miraba el precipicio.
            //
            // Lo que define la forma de una cara no es de qué está hecha la
            // celda de al lado, es **por dónde se cae**: el borde mira hacia el
            // terreno más bajo. Con esto `edge_s` es la cara que da a la terraza
            // de abajo, `corner_se` una esquina, y el arte —el que dibuje el
            // usuario en HA Studio o el respaldo de `procedural_art`— por fin
            // puede orientarse.
            let nivel = ha_shared::relief::relief_level(body, tx, tz);
            let same = |dx: i64, dz: i64| {
                if matches!(tile, Tile::Cliff | Tile::Slope) {
                    return ha_shared::relief::relief_level(body, tx + dx, tz + dz) >= nivel;
                }
                let n = tiles.tile_at(body, tx + dx, tz + dz);
                if tile == Tile::Water {
                    // **Y LA BOCA DE UN SALTO CUENTA COMO AGUA.** Es la única
                    // celda del mundo donde la tierra y el agua no ponen su
                    // borde: la transición la hace el propio recorte de la
                    // cascada, que trae su labio dibujado. La boca es acantilado
                    // —tierra— así que sin esto el río de arriba le dibujaba su
                    // banda de barro y pasto CRUZADA POR ENCIMA DEL SALTO, y el
                    // lago terminaba en una ribera con la cascada empezando
                    // debajo, sin que una cosa llevara a la otra.
                    //
                    // **Y LA POZA TAMBIÉN**, que es la mitad que faltaba. Al pie
                    // del salto el agua tiene tierra a los dos costados —los
                    // hombros de la garganta y el pie de la pared—, así que le
                    // pedía `edge_e`/`edge_w`/`corner_*` y pintaba su ribete de
                    // barro DEBAJO del sprite del reventón: el chorro terminaba
                    // contra un borde de orilla en vez de contra su propia agua.
                    // La transición ahí la trae la poza del recorte, con la
                    // espuma dibujada.
                    matches!(n, Tile::Water | Tile::WaterDeep)
                        || ha_shared::agua::boca_de_cascada(body, tx + dx, tz + dz)
                        || ha_shared::agua::poza_de_cascada(body, tx + dx, tz + dz)
                } else {
                    n == tile
                }
            };
            // n=norte(−z) e=este(+x) s=sur(+z) w=oeste(−x) + diagonales.
            let shape = autotile_shape(
                same(0, -1), same(1, 0), same(0, 1), same(-1, 0),
                same(1, -1), same(-1, -1), same(1, 1), same(-1, 1),
            );
            // **La MEJILLA del paso.** Donde la pared muere contra una rampa, la
            // celda de acantilado no es una cara más: es el costado de la
            // escalinata, y el kit le dibuja su espolón de roca
            // (`scripts/import-acantilado.py`). Sin esto el paso se abría como un
            // agujero cuadrado y las dos caras se quedaban mirándose.
            //
            // Sólo vale para las caras que MIRAN AL SUR, que son las que enseñan
            // la pared entera: en una que se ve de canto el espolón no se vería.
            let shape = mejilla_de_rampa(body, tile, tx, tz).unwrap_or(shape);
            // **La baldosa de ESTA celda.** El pack trajo 169 losas de vereda,
            // asfalto y suelo urbano, y el mundo dibujaba siempre la primera:
            // una ciudad entera con la misma baldosa repetida cada metro. La
            // variante se elige por la celda (determinista, sin guardar nada), el
            // mismo camino que ya usaban las rocas y los árboles. Sin variantes
            // cargadas devuelve la clave base y no cambia nada.
            let vk = art.variant_key_seeded(key, celda_semilla(tx, tz));
            // **UNA CERCA ES UNA LÍNEA, NO UNA MANCHA.**
            //
            // `autotile_shape` contesta "¿de qué lado se acaba el material?" y eso
            // describe el borde de un ÁREA. A una cerca hay que preguntarle hacia
            // dónde SIGUE, que es otra pregunta y otra tabla de nombres
            // (`ha_shared::cercas`). Pidiendo `edge_*` —que ninguna cerca tiene—
            // `tile_sprite` caía al recorte base y dibujaba **el mismo tramo
            // horizontal en los cuatro lados de la parcela**: la valla acostada en
            // los tramos verticales que reportó el usuario.
            //
            // El sufijo de lado lo sabe el lote y nadie más (`city::afuera_de_cerca`):
            // una parcela pegada a otra no deja adivinarlo mirando vecinos.
            let cerca = (tile == Tile::Fence).then(|| {
                let v = ha_shared::cercas::Vecinos::new(
                    same(0, -1),
                    same(1, 0),
                    same(0, 1),
                    same(-1, 0),
                );
                let afuera = ha_shared::geography::settlement_covering(body, tx, tz)
                    .and_then(|s| ha_shared::city::afuera_de_cerca(body, &s, tx, tz));
                let cadena = ha_shared::cercas::cadena(v, afuera);
                cadena
                    .iter()
                    .find(|p| art.has_pose(&vk, p) || art.has_pose(key, p))
                    .cloned()
                    // Sin ninguna pose de la cadena curada, la clave base: es lo
                    // que se veía antes de tener el kit orientado, no una regresión.
                    .unwrap_or_default()
            });
            // **Los tramos verticales se alinean con los codos.** En el kit de reja la línea vertical de los codos va pegada al
            // borde de la celda, y la del tramo `ns@e`/`ns@w` simple va en el medio: en la esquina de la parcela el lado quedaba
            // corrido medio tile. Las variantes `#2` (con poste) traen la línea en el borde correcto, pero cruzadas: el lado
            // este usa la `ns@w#2` y el oeste la `ns@e#2` (medido contra las poses `sw`/`es`).
            let mut espejar = false;
            let cerca = cerca.map(|p| {
                // Las dos caras verticales salen de la MISMA pieza (`ns@e#2`, línea pegada al borde izquierdo): el lado este
                // la usa espejada, así la línea cae exactamente donde la dibujan los codos `sw`/`es`.
                let (cruzada, flip) = match p.as_str() {
                    "ns@e" => ("ns@e#2", true),
                    "ns@w" => ("ns@e#2", false),
                    _ => return p,
                };
                if art.has_pose(&vk, cruzada) || art.has_pose(key, cruzada) {
                    espejar = flip;
                    cruzada.to_string()
                } else {
                    p
                }
            });
            let shape = match &cerca {
                Some(p) if !p.is_empty() => p.as_str(),
                _ => shape,
            };
            // **Un muro del mundo se dibuja del alto de su pixel, no aplastado a
            // la celda.**
            //
            // Una superficie (piso, vereda, asfalto) mide la celda por definición
            // — si no, no embaldosa. Pero `wall/*` y `window/*` son recortes de
            // 1×2 celdas y se estaban estirando dentro de un cuadrado de 1×1: la
            // pared de una casa generada salía de la mitad de alto y con el pixel
            // chato, mientras la que construye el jugador sale entera. La misma
            // pieza, dos alturas, según quién la puso.
            //
            // Es la regla de obra (`ArtLibrary::medida_de_obra`): una celda del
            // pack, un tile — así el ancho sigue encajando borde con borde y sólo
            // crece hacia arriba, apoyada en el pie como cualquier pared.
            //
            // **El RELIEVE mide por POSE, no por slot.** Sus formas no miden
            // todas lo mismo: la cara que mira al sur son tres celdas (labio +
            // pared + pie) y la que mira al norte es UNA (el canto de enfrente,
            // que no tiene pared que enseñar). Preguntando por el slot salían
            // todas del alto de la primera y el canto lejano se dibujaba
            // estirado sobre la meseta.
            let relieve = matches!(tile, Tile::Cliff | Tile::Slope);
            // **Y LA CERCA TAMBIÉN MIDE POR POSE.** Sus formas no miden todas lo
            // mismo (el kit de madera son recortes de 1×2 y el murete de piedra de
            // 1×1), y preguntar por el slot devuelve el rect del slot, no el de la
            // pose que se va a dibujar — la mentira dormida de `medida_de_pieza`
            // que ya costó una vuelta con las fachadas (biblia §8).
            let medida = if relieve || tile == Tile::Fence {
                let respaldo = Vec2::splat(TILE_M);
                let m = art.medida_de_obra(&vk, Some(shape), respaldo);
                if m == respaldo { art.medida_de_obra(key, Some(shape), respaldo) } else { m }
            } else if let (true, Some(pose)) = (
                tile.upright() && art.has_pose(key, cara_de_pared.unwrap_or("")),
                cara_de_pared.map(|c| match c {
                    "corner_nw" | "corner_ne" => "edge_n",
                    "corner_sw" | "corner_se" => "edge_s",
                    o => o,
                }),
            ) {
                // El kit de cuarto mide POR POSE: `edge_n` son 1×2 pero `edge_w/e`, `edge_s` y las esquinas son 1×1.
                // Con la medida del slot (1×2) salían estiradas al doble: las esquinas «rotas».
                art.medida_de_obra(key, Some(pose), Vec2::splat(TILE_M))
            } else if tile.upright() {
                let respaldo = Vec2::splat(TILE_M);
                let m = art.medida_de_obra(&vk, None, respaldo);
                if m == respaldo { art.medida_de_obra(key, None, respaldo) } else { m }
            } else {
                Vec2::splat(TILE_M)
            };
            // **Una pared tan alta como su desnivel.** Si la cara baja más de un
            // escalón, la pieza compuesta del kit (labio + cuerpo + pie, que mide
            // UNO) se queda corta y hay que armarla por filas — ver
            // [`pared_apilada`], que devuelve `true` cuando ya la dibujó ella.
            // **Y el PASO se apila igual que la pared.** La escalinata del kit
            // mide lo que mide una pared del pack; un paso que baja diez pisos
            // necesita diez filas de peldaños, o queda una franja de meseta lisa
            // cortando el muro de arriba abajo — que es exactamente lo que había
            // que arreglar. Las dos familias piden `labio`/`cuerpo`/`pie`, así
            // que las apila el mismo código.
            // **EL GRIS DE LA CELDA, PARA QUE EL BORDE NO SEA OTRO VERDE.** La
            // pieza del relieve trae su meseta dibujada y la malla de al lado
            // pinta esa misma meseta multiplicada por el aclarado de su escalón;
            // sin esto son dos colores distintos y se ve el anillo (ver
            // [`gris_de_celda`]). Sólo el relieve: el agua tiene su propio color
            // de cuerpo y su animación, y teñirla es otra medición.
            let gris = if relieve {
                let s = *sombra_muro
                    .get_or_insert_with(|| sombra_de_muro_en_la_columna(body, tx, cz * CHUNK));
                gris_de_celda(tiles, body, tx, tz, s[lz as usize])
            } else {
                1.0
            };
            if matches!(tile, Tile::Cliff | Tile::Slope)
                && CARAS_QUE_CUELGAN.contains(&shape)
                && pared_apilada(commands, &art, tiles, body, key, &vk, shape, tx, tz, cx, cz, z, gris)
            {
                continue;
            }
            // **La CARA manda sobre la forma de autotile.** Una pared de
            // interior no tiene `edge_n` ni `corner_ne` —el pack la dibuja por
            // cara, no por vecindad—, así que sin esto `tile_sprite` caía
            // siempre a `center` y las cuatro paredes de la casa salían con el
            // mismo paño. Ver `arte_de_interior`.
            // Las esquinas del kit de cuarto no traen la tapa horizontal: se arman con el muro de fondo (`edge_n`/`edge_s`)
            // y el canto del muro lateral encima (`spawn_remate_de_esquina`).
            let esquina = cara_de_pared.filter(|c| matches!(*c, "corner_nw" | "corner_ne" | "corner_sw" | "corner_se"));
            let shape = match esquina {
                Some("corner_nw" | "corner_ne") => "edge_n",
                Some(_) => "edge_s",
                None => cara_de_pared.unwrap_or(shape),
            };
            let Some((mut sprite, anim)) = art.tile_sprite(&vk, shape, medida) else { continue };
            sprite.flip_x = espejar;
            // La pieza compuesta de 1×3 (el bordillo y la pared de un escalón)
            // lleva UN color, y va el del CANTO: el anillo se ve arriba, donde la
            // meseta del recorte toca la meseta de la malla. Abajo el recorte es
            // roca contra pasto y ahí no hay dos veces la misma losa.
            let sprite = tenir_sprite(sprite, gris);
            // Banda por ALTO, no por colisión. `Tile::solid()` significa "bloquea
            // el paso" y usarlo acá era el bug del AGUA tapando al jugador: el
            // agua frena pero es plana. `upright()` es el eje correcto — sólo lo
            // que se levanta del suelo (muros, ventanas, roca) Y-sortea; el resto
            // (agua, hielo, pisos) vive en la banda de piso, siempre bajo los pies.
            // Una capa explícita en el slot manda por encima de las dos.
            // La capa la puede declarar la variante; si no, manda la del slot.
            let depth = match art.slot_layer(&vk).or_else(|| art.slot_layer(key)) {
                Some(l) => crate::layers::depth(Some(l), wz),
                // La rampa TAMBIÉN cuelga sobre la terraza de abajo, así que
                // también Y-sortea: es la misma pared del acantilado, abierta.
                None if relieve => crate::layers::relieve(wz),
                None if tile.upright() => crate::layers::ysort(wz),
                None => crate::layers::FLOOR,
            };
            // **Una pared crece hacia arriba; un ACANTILADO cuelga hacia abajo.**
            //
            // Son el mismo truco al revés y la diferencia está en dónde vive la
            // celda dentro del dibujo. En una pared la celda es el PIE: lo que
            // sobra del alto sale por arriba y tapa lo que hay detrás. En un
            // acantilado la celda es el CANTO —el borde de la meseta, que es
            // terreno alto y se camina— y lo que sobra es la cara de roca, que
            // cae sobre la terraza de abajo, hacia la cámara.
            //
            // Dibujado como una pared, el acantilado se tapaba a sí mismo: la
            // roca subía por encima de la meseta que la sostiene.
            //
            // El Y-sort no cambia —sigue siendo el de su celda—, y eso es lo que
            // hace que el que camina por la terraza de abajo pase POR DELANTE de
            // la pared en vez de meterse dentro.
            let alto_extra = (medida.y - TILE_M) * 0.5;
            let y = if relieve { r.y - alto_extra } else { r.y + alto_extra };
            let mut ent = commands.spawn((
                SurfaceDecor { cx, cz, z, small: false },
                TileArt,
                sprite,
                Transform::from_xyz(r.x, y, depth),
            ));
            if let Some(a) = anim {
                ent.insert(a);
            }
            // **UN BORDE QUE NO CUELGA NO TIRA SOMBRA** (28 sep 2026). Las
            // piezas que no son cara (`edge_n`, `edge_e`/`_w`, las esquinas de
            // arriba y las de adentro) son la línea del filo dibujada sobre el
            // suelo: no se levantan. Pero viven en la banda Y-sort y
            // `light2d::attach_ground_shadows` les colgaba la mancha de un objeto
            // parado: en todo borde en escalera salía una sierra de dientes
            // grises (en la nieve, sobre blanco, era lo único que se veía).
            if relieve && !CARAS_QUE_CUELGAN.contains(&shape) {
                crate::light2d::sin_sombra(&mut ent);
            }
            // Tiles que iluminan (lava, cristal, un piso de luz). Sólo pagan quad
            // los materiales que el usuario marcó como emisores.
            crate::light2d::attach(&mut ent, &art, &[&vk, key], medida);
            if let Some(e) = esquina {
                spawn_remate_de_esquina(commands, art, key, e, r, depth, cx, cz, z);
            }
        }
    }
    spawn_chunk_piletas(commands, art, tiles, body, z, cx, cz);
    spawn_chunk_orilla(commands, art, tiles, body, z, cx, cz);
    spawn_chunk_canto_concavo(commands, art, tiles, body, z, cx, cz);
    spawn_chunk_hombros_de_garganta(commands, art, body, z, cx, cz);
    spawn_chunk_cascadas(commands, art, tiles, body, z, cx, cz);
    spawn_chunk_cliff_urbano(commands, art, tiles, body, z, cx, cz);
    spawn_chunk_lane_marks(commands, art, tiles, body, z, cx, cz);
}

/// **El remate de una esquina de cuarto.** El kit (`Room_Builder_3d_walls`) no trae una pieza con la tapa horizontal
/// unida al canto vertical: `corner_*` es un muro lateral solo. La esquina se arma con el muro de fondo (`edge_n` /
/// `edge_s`, que ya dibujó el llamador) y encima el recorte del canto lateral —su franja blanca y su contorno— que
/// prolonga el lateral hasta tocar la tapa. Los recortes salen de medir las piezas (`edge_w`/`edge_e`: contorno 0-2 px,
/// franja blanca 2-12 px; la tapa ocupa las filas 0-12 de su celda).
#[allow(clippy::too_many_arguments)]
fn spawn_remate_de_esquina(
    commands: &mut Commands,
    art: &ArtLibrary,
    key: &str,
    esquina: &str,
    r: Vec2,
    depth: f32,
    cx: i64,
    cz: i64,
    z: i32,
) {
    // (pose del canto lateral, recorte [x0, y0, x1, y1] en px, celdas hacia arriba donde se pega)
    let piezas: &[(&str, [f32; 4], f32)] = match esquina {
        "corner_nw" => &[("edge_w", [0.0, 2.0, 12.0, 32.0], 1.0), ("edge_w", [0.0, 0.0, 12.0, 32.0], 0.0)],
        "corner_ne" => &[("edge_e", [20.0, 2.0, 32.0, 32.0], 1.0), ("edge_e", [20.0, 0.0, 32.0, 32.0], 0.0)],
        "corner_sw" => &[("edge_w", [0.0, 0.0, 12.0, 10.0], 0.0)],
        "corner_se" => &[("edge_e", [20.0, 0.0, 32.0, 10.0], 0.0)],
        _ => return,
    };
    let k = TILE_M / 32.0;
    for &(pose, [x0, y0, x1, y1], arriba) in piezas {
        let Some((mut sprite, _)) = art.tile_sprite(key, pose, Vec2::splat(TILE_M)) else { continue };
        let Some(rect) = sprite.rect else { continue };
        sprite.rect = Some(Rect::from_corners(rect.min + Vec2::new(x0, y0), rect.min + Vec2::new(x1, y1)));
        sprite.custom_size = Some(Vec2::new((x1 - x0) * k, (y1 - y0) * k));
        let pie = r.y - TILE_M * 0.5 + arriba * TILE_M;
        let centro = Vec2::new(r.x + ((x0 + x1) * 0.5 - 16.0) * k, pie + (32.0 - (y0 + y1) * 0.5) * k);
        commands.spawn((
            SurfaceDecor { cx, cz, z, small: false },
            TileArt,
            sprite,
            Transform::from_xyz(centro.x, centro.y, depth + 0.0002),
        ));
    }
}

/// **LA ORILLA DEL SUELO BASE: dónde la tierra, la arena o la nieve muerden el pasto.**
///
/// El suelo lo pinta la malla del chunk con **una losa plana por celda** —medido:
/// `tiles/grass` son 1.024 píxeles de un solo verde—, así que donde dos
/// materiales se tocan había un corte a escuadra entre dos colores. Eso es lo que
/// hacía que la cumbre nevada se leyera como un rectángulo celeste pegado encima
/// del prado y que el pozo que cavás saliera como un cuadrado marrón con el borde
/// a cuchillo.
///
/// El pack ya dibujó esa transición (`Grass_3`, autotile entero de tierra contra
/// pasto) y estaba sin usar. `scripts/import-suelo-bordes.py` la publica como
/// `borde/tierra`, `borde/arena` y `borde/nieve`, con una variante por clima de
/// pasto para que el verde de la orilla sea el MISMO que el del tile de al lado.
///
/// **Sólo las celdas de BORDE pagan sprite.** El relleno lo sigue dibujando la
/// malla —un draw call por chunk— y acá se saltea la forma `center`: el borde de
/// una mancha son decenas de celdas, su interior son miles (regla 2).
///
/// **Y la orilla nace sólo contra el PASTO.** El dibujo del pack trae el verde
/// puesto: usarlo contra el agua pintaría una franja de césped dentro de la
/// laguna, que ya tiene su propia ribera (`tiles/water_arena`). Por eso un lado
/// cuenta como "igual" si es el mismo material **o si no es pasto**.
fn spawn_chunk_orilla(
    commands: &mut Commands,
    art: &ArtLibrary,
    tiles: &TileWorld,
    body: CelestialBody,
    z: i32,
    cx: i64,
    cz: i64,
) {
    // Por COLUMNAS, para poder calcular la sombra de muro de una pasada: la
    // orilla es suelo dibujado encima del suelo y tiene que llevar el mismo gris
    // que la malla debajo (ver [`gris_de_celda`]). El orden de spawn no importa
    // —dos orillas nunca caen en la misma celda— así que invertir los bucles no
    // cambia un pixel.
    for lx in 0..CHUNK {
        let tx = cx * CHUNK + lx;
        let mut sombra_muro: Option<[f32; CHUNK as usize]> = None;
        for lz in 0..CHUNK {
            let tz = cz * CHUNK + lz;
            // **EL POZO QUE CAVÁS TAMBIÉN LLEVA ORILLA**, y es la mitad del pedido:
            // *"cuando se crea ese hueco, que se vaya editando en tiempo real ese
            // contorno de tierra-césped; ahora se colocan unas líneas negras, en
            // vez de eso usaríamos los assets"*. El hueco es tierra abierta contra
            // el pasto de al lado — exactamente lo que dibuja `borde/tierra`. El
            // corte oscuro que hacía ese papel se apaga en `pit_quads`.
            // **Sólo el pozo del SUELO.** En un piso alto, "hueco" es el vacío que
            // rodea a una azotea (`pit_quads` ya tenía escrita esa lección), y ahí
            // una orilla de tierra y pasto sería un ribete de campo alrededor de un
            // tejado. Se cava en la planta baja y hacia abajo.
            let pozo = z <= 0 && tiles.renders_as_pit(body, tx, tz);
            let tile = tiles.render_tile(body, tx, tz);
            // **Y LA COSTURA ENTRE DOS PASTOS TAMBIÉN ES UNA ORILLA.** Era el
            // agujero que dejaba a la meseta cortada a escuadra contra la
            // llanura: `orilla_del_tile` sólo conocía tierra, arena y hielo, y a
            // los dos lados de la frontera sierra/pradera el tile es
            // `Tile::Grass`, así que nadie dibujaba nada — aunque las losas sean
            // (106,148,88) contra (71,151,87), dos verdes claramente distintos.
            let mia = ha_shared::geography::superficie_de_tile(body, tile, tx, tz);
            let mat = if pozo { "tierra" } else {
                let Some(m) = orilla_de_superficie(mia) else { continue };
                m
            };
            // **Igual = ese lado no es pasto que a mí me toque orlar.**
            //
            // Contra lo que no es pasto no hay pieza (la costa la dibuja el agua,
            // la vereda trae su cordón), así que ahí no se orla nunca. Y entre dos
            // pastos manda el RANGO: **una costura, un solo dueño**. Sin un orden
            // total los dos lados dibujarían su fleco y el de arriba taparía al de
            // abajo — dos dibujos distintos para el mismo metro de mundo.
            let same = |dx: i64, dz: i64| {
                if pozo {
                    return tiles.renders_as_pit(body, tx + dx, tz + dz);
                }
                let n = tiles.render_tile(body, tx + dx, tz + dz);
                if n != Tile::Grass {
                    return true;
                }
                match mia.tile {
                    // La tierra, la arena y la nieve orlan contra CUALQUIER pasto.
                    Tile::Grass => {
                        let v = ha_shared::geography::superficie_de(body, tx + dx, tz + dz);
                        rango_de_pasto(v.pasto) >= rango_de_pasto(mia.pasto)
                    }
                    _ => false,
                }
            };
            let shape = autotile_shape(
                same(0, -1), same(1, 0), same(0, 1), same(-1, 0),
                same(1, -1), same(-1, -1), same(1, 1), same(-1, 1),
            );
            if shape == "center" {
                continue;
            }
            // **De qué verde es el fleco.** Es el pasto del OTRO lado, y por eso
            // se pregunta distinto según quién dibuja: la tierra, la arena y la
            // nieve están rodeadas del pasto de su propia celda; una costura entre
            // dos pastos tiene el otro pasto enfrente, así que se toma el vecino
            // de menor rango — el que esta pieza viene a coser.
            //
            // Sin la variante curada cae a la de referencia y no se pierde ninguna
            // celda.
            let clima = if mia.tile == Tile::Grass {
                sufijo_de_pasto(pasto_vecino_mas_bajo(body, tx, tz, mia.pasto))
            } else {
                sufijo_de_pasto(mia.pasto)
            };
            let clave = clima
                .map(|c| format!("borde/{mat}_{c}"))
                .filter(|k| art.has_pose(k, shape))
                .unwrap_or_else(|| format!("borde/{mat}"));
            let Some((sprite, _)) = art.tile_sprite(&clave, shape, Vec2::splat(TILE_M)) else {
                continue;
            };
            let r = render_xy(tx as f32 * TILE_M + 0.5, tz as f32 * TILE_M + 0.5);
            let s = *sombra_muro
                .get_or_insert_with(|| sombra_de_muro_en_la_columna(body, tx, cz * CHUNK));
            let gris = gris_de_celda(tiles, body, tx, tz, s[lz as usize]);
            commands.spawn((
                SurfaceDecor { cx, cz, z, small: true },
                TileArt,
                tenir_sprite(sprite, gris),
                // Es SUELO: plano, en la capa del piso, y nunca Y-sortea. Un
                // borde que tapara al que camina sobre él sería un agujero. La
                // del pozo va un pelo más arriba: se dibuja SOBRE la sombra de
                // la pared, que la malla ya pintó.
                Transform::from_xyz(r.x, r.y, crate::layers::FLOOR + if pozo { 0.006 } else { 0.004 }),
            ));
        }
    }
}

/// Qué orilla le toca a una superficie, o `None` si no lleva.
///
/// Los tres primeros son los que muerden el pasto: la tierra que deja el que
/// cava, la arena de la playa y del desierto, y la nieve de la cumbre. La piedra
/// y el asfalto no entran: los pone la ciudad y ya traen su cordón.
///
/// **Y los cuatro pastos de clima**, que es lo que faltaba: la sierra contra la
/// pradera es un cambio de losa tan visible como la arena contra el pasto, y sin
/// pieza salía a escuadra siguiendo el borde de la terraza. El pasto de
/// referencia no lleva porque es el fondo contra el que orlan todos los demás.
fn orilla_de_superficie(s: ha_shared::geography::Superficie) -> Option<&'static str> {
    use ha_shared::geography::Pasto;
    match s.tile {
        Tile::Dirt => Some("tierra"),
        Tile::Sand => Some("arena"),
        Tile::Ice => Some("nieve"),
        Tile::Grass => match s.pasto {
            Pasto::Bosque => Some("pasto_bosque"),
            Pasto::Seco => Some("pasto_seco"),
            Pasto::Pantano => Some("pasto_pantano"),
            Pasto::Sierra => Some("pasto_sierra"),
            Pasto::Referencia => None,
        },
        _ => None,
    }
}

/// **UNA COSTURA, UN SOLO DUEÑO.** El orden total entre pastos: la dibuja el de
/// mayor rango y su fleco se tiñe del de menor.
///
/// Sin un orden total los dos lados de la frontera dibujarían su propia pieza,
/// una encima de la otra, y el mismo metro de mundo tendría dos dibujos
/// distintos peleándose el z-fighting.
///
/// El orden es de "fondo" a "encima": la pradera es el suelo de referencia del
/// mundo y por eso va última; la sierra gana porque es la que se recorta contra
/// todo lo demás al subir.
fn rango_de_pasto(p: ha_shared::geography::Pasto) -> u8 {
    use ha_shared::geography::Pasto;
    match p {
        Pasto::Referencia => 0,
        Pasto::Bosque => 1,
        Pasto::Seco => 2,
        Pasto::Pantano => 3,
        Pasto::Sierra => 4,
    }
}

/// El sufijo de clave de un pasto (`borde/tierra_sierra`), o `None` para el de
/// referencia, que no lleva.
fn sufijo_de_pasto(p: ha_shared::geography::Pasto) -> Option<&'static str> {
    use ha_shared::geography::Pasto;
    match p {
        Pasto::Bosque => Some("bosque"),
        Pasto::Seco => Some("seco"),
        Pasto::Pantano => Some("pantano"),
        Pasto::Sierra => Some("sierra"),
        Pasto::Referencia => None,
    }
}

/// El pasto de menor rango entre los cuatro vecinos ortogonales, contando sólo
/// los que esta celda orla (los de rango menor al suyo).
///
/// Es de quién es el fleco: si una lengua de sierra baja contra la pradera, el
/// verde del borde tiene que ser el de la pradera. Con todos los vecinos iguales
/// —no debería pasar, la forma ya sería `center`— devuelve el de referencia.
fn pasto_vecino_mas_bajo(
    body: CelestialBody,
    tx: i64,
    tz: i64,
    mio: ha_shared::geography::Pasto,
) -> ha_shared::geography::Pasto {
    use ha_shared::geography::Pasto;
    let mut mejor = Pasto::Referencia;
    let mut rango = u8::MAX;
    for (dx, dz) in [(0i64, -1i64), (1, 0), (0, 1), (-1, 0)] {
        let v = ha_shared::geography::superficie_de(body, tx + dx, tz + dz).pasto;
        let r = rango_de_pasto(v);
        if r < rango_de_pasto(mio) && r < rango {
            rango = r;
            mejor = v;
        }
    }
    mejor
}

/// Las formas que CUELGAN sobre la terraza de abajo: las que miran al sur, que
/// en 3/4 son las únicas que enseñan la pared entera. Sólo ésas pueden necesitar
/// tramos de más (`import-acantilado.py::CARA_SUR`).
const CARAS_QUE_CUELGAN: [&str; 4] = ["edge_s", "corner_se", "corner_sw", "center"];

/// **UNA PARED TAN ALTA COMO SU DESNIVEL.**
///
/// La pieza compuesta del kit —labio + cuerpo + pie, tres celdas— mide UN escalón
/// y punto. Si la cara baja ocho, entre el labio y el pie van ocho cuerpos: con
/// uno solo queda media pared y por debajo asoma el pasto de la terraza de abajo.
///
/// Devuelve `true` si dibujó la pared por filas sueltas (y entonces quien llama
/// no debe dibujar la compuesta).
///
/// **Ésta es la función que el rediseño desbloqueó.** Ya existía y devolvía
/// `false` siempre, porque medido sobre la Tierra las 7.832 caras del planeta
/// bajaban exactamente un escalón: el campo de altura no cambia lo suficiente en
/// un tile y todas las terrazas medían lo mismo. Con la escalera de
/// `relief::SALTO_DE_TERRAZA`, el alto sale de [`ha_shared::relief::alto_de_pared`]
/// y un farallón de meseta apila diez cuerpos de roca — que es lo que hace que
/// la cascada que cae al lado pueda medir diez pisos.
/// **LAS FILAS DE UN MURO NO SON LA MISMA BALDOSA.**
///
/// Un farallón de diez pisos apila nueve `cuerpo`, y con una sola celda eso es la
/// misma piedra repetida nueve veces por el ancho de la corrida: a esa escala el
/// ojo deja de ver cantos rodados y ve una CUADRÍCULA — el "se lee como relleno"
/// de las capturas. La suposición era que el pack no tenía con qué; medido, sí
/// tiene: `Middle_Modular_Single_Small_Sand_Mountain` es la fila que el kit dibujó
/// justamente para repetirse y trae tres celdas distintas, que
/// `import-acantilado.py` publica como `cuerpo_2`/`cuerpo_3`/`cuerpo_4`.
///
/// El reparto es por CELDA (columna y fila), no por fila entera: variando sólo la
/// fila, una corrida larga sigue enseñando una banda horizontal idéntica.
/// Determinista y sin estado, como el resto de las variantes del mundo.
///
/// Sin las poses de variante devuelve `"cuerpo"` y la pared sale como salía: un
/// manifiesto viejo no rompe nada.
///
/// Se pregunta por la clave de VARIANTE (`vk`), que es la que va a dibujar, y no
/// por la base: hoy son la misma —`cliff/*` no tiene variantes— pero el día que
/// las tenga, preguntarle a la base por una pose que la variante no trae dejaría
/// esa fila del muro sin dibujar.
fn cuerpo_de_la_fila(art: &ArtLibrary, vk: &str, tx: i64, fila: i64) -> &'static str {
    const CUERPOS: [&str; 4] = ["cuerpo", "cuerpo_2", "cuerpo_3", "cuerpo_4"];
    let n = CUERPOS.iter().take_while(|p| art.has_pose(vk, p)).count();
    if n <= 1 {
        return "cuerpo";
    }
    let mut h = celda_semilla(tx, fila).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= h >> 29;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 32;
    CUERPOS[(h % n as u64) as usize]
}

/// El gris de la fila `i` de un muro apilado, entre el del canto (`i = 0`) y el
/// del suelo que pisa (`i = filas`).
///
/// Pura y aparte para que un test pueda fijar los dos extremos sin render: son
/// las dos costuras que se ven, y si alguna se despega vuelve el anillo.
fn gris_de_la_fila(gris_canto: f32, gris_pie: f32, i: i64, filas: i64) -> f32 {
    let t = i.clamp(0, filas) as f32 / filas.max(1) as f32;
    gris_canto + (gris_pie - gris_canto) * t
}

#[allow(clippy::too_many_arguments)]
fn pared_apilada(
    commands: &mut Commands,
    art: &ArtLibrary,
    tiles: &TileWorld,
    body: CelestialBody,
    key: &'static str,
    vk: &str,
    shape: &str,
    tx: i64,
    tz: i64,
    cx: i64,
    cz: i64,
    z: i32,
    gris_canto: f32,
) -> bool {
    // El alto de la cara, en escalones. `0` = no cuelga hacia el sur; `1` = es un
    // bordillo y lo dibuja la pieza compuesta de siempre.
    let alto = ha_shared::relief::alto_de_pared(body, tx, tz);
    if alto <= 1 {
        return false;
    }
    if !(art.has_pose(key, "labio") && art.has_pose(key, "cuerpo") && art.has_pose(key, "pie")) {
        return false;
    }
    let medida = Vec2::splat(TILE_M);
    // El Y-sort es el de la celda del CANTO para TODAS las filas: la pared es un
    // solo objeto y tiene que ordenarse como uno, o el que camina por la terraza
    // de abajo se mete entre dos de sus filas.
    let orden = crate::layers::relieve(tz as f32 * TILE_M + 0.5);
    // **Y SE CORTA CONTRA EL CANTO DE MÁS ABAJO.** En una ladera escarpada la
    // terraza siguiente empieza antes de que el muro termine de caer; si el
    // dibujo siguiera, taparía la meseta de abajo con roca por la que se camina.
    // Es la MISMA poda que aplica `relief::pie_de_pared` al decidir qué celdas
    // son roca, y por eso no puede quedar ni pared invisible ni roca fantasma.
    // La roca cubre la TERRAZA DE ABAJO mientras siga siendo la misma: en cuanto
    // el nivel cambia —otro canto que baja, o el suelo que vuelve a subir en el
    // fondo de un valle— el muro deja de tener sobre qué colgar.
    let base = ha_shared::relief::relief_level(body, tx, tz + 1);
    let mut filas = alto + 1;
    for j in 2..=alto + 1 {
        if ha_shared::relief::relief_level(body, tx, tz + j) != base {
            filas = j - 1;
            break;
        }
    }
    // **EL GRIS DEL MURO: EL DEL CANTO ARRIBA, EL DEL SUELO QUE PISA ABAJO.**
    //
    // Son las dos costuras que el ojo lee. Arriba, el `labio` trae dibujada la
    // meseta y toca la meseta que pinta la malla: si no llevan el mismo gris se
    // ve el anillo (ver [`gris_de_celda`]). Abajo, el `pie` se apoya en la
    // terraza baja, que está varios escalones más oscura — con el gris del canto
    // el muro entero flotaba iluminado sobre un suelo en penumbra.
    //
    // El gris del pie va **sin la sombra de columna**: esa franja oscura la tira
    // ESTE muro, y un muro no se sombrea a sí mismo. Dos consultas por pared, no
    // una por fila.
    let gris_pie = gris_de_celda(tiles, body, tx, tz + filas + 1, 1.0);
    let gris = |fila: i64| gris_de_la_fila(gris_canto, gris_pie, fila - tz, filas);
    // **UNA SOMBRA PARA TODA LA PARED, NO UNA POR FILA.**
    //
    // Cada fila (`poner`) es su PROPIO sprite de una celda —`medida = TILE_M ×
    // TILE_M`—, así que `light2d::attach_ground_shadows` (que no sabe nada de
    // paredes, sólo mide el `custom_size` de cada sprite) le calculaba a un
    // farallón de nueve pisos la sombra de una piedra suelta de un metro: nueve
    // sombritas idénticas apiladas, cada una tan corta como la de un guijarro,
    // en vez de UNA sombra que se note del porte real del muro. Es la otra mitad
    // del mismo bug que `attach_ground_shadows::length` (ahí se arregló para el
    // farallón de UNA sola pieza; acá el farallón son VARIAS).
    //
    // La salida es un ancla invisible (`Color::NONE`, misma medida que ya usa
    // `spawn_chunk_tile_art` para las vetas sin arte) del tamaño de la pared
    // ENTERA —`TILE_M` de ancho por `(filas+1) × TILE_M` de alto—, y las filas
    // visibles se marcan `sin_sombra` para que no dupliquen la mancha.
    let centro_fila = tz as f32 + filas as f32 * 0.5;
    let ancla_r = render_xy(tx as f32 * TILE_M + 0.5, centro_fila * TILE_M + 0.5);
    commands.spawn((
        SurfaceDecor { cx, cz, z, small: true },
        TileArt,
        Sprite::from_color(Color::NONE, Vec2::new(TILE_M, (filas + 1) as f32 * TILE_M)),
        Transform::from_xyz(ancla_r.x, ancla_r.y, orden),
    ));
    let mut poner = |pose: &'static str, fila: i64| {
        let r = render_xy(tx as f32 * TILE_M + 0.5, fila as f32 * TILE_M + 0.5);
        if let Some((sprite, anim)) = art.tile_sprite(vk, pose, medida) {
            let mut ent = commands.spawn((
                SurfaceDecor { cx, cz, z, small: false },
                TileArt,
                tenir_sprite(sprite, gris(fila)),
                Transform::from_xyz(r.x, r.y, orden),
            ));
            crate::light2d::sin_sombra(&mut ent);
            if let Some(a) = anim {
                ent.insert(a);
            }
        }
    };
    // **LA ESQUINA DOBLA TAMBIÉN EN LA PARED ALTA.** Una celda `corner_se`/`_sw`
    // se armaba con las filas de la pared RECTA, así que su labio no doblaba y
    // cada esquina se veía rota entre dos tramos sanos (27 sep 2026). Las filas
    // de esquina las deriva `scripts/derivar-esquinas-de-acantilado.py` del rect
    // medido de la pieza 1×3 del kit.
    let esquina = match shape {
        "corner_se" => Some(("labio_se", "cuerpo_se", "pie_se")),
        "corner_sw" => Some(("labio_sw", "cuerpo_sw", "pie_sw")),
        _ => None,
    }
    .filter(|(l, c, p)| art.has_pose(vk, l) && art.has_pose(vk, c) && art.has_pose(vk, p));
    match esquina {
        Some((labio, cuerpo, pie)) => {
            poner(labio, tz);
            for i in 1..filas {
                poner(cuerpo, tz + i);
            }
            poner(pie, tz + filas);
        }
        None => {
            poner("labio", tz);
            for i in 1..filas {
                poner(cuerpo_de_la_fila(art, vk, tx, tz + i), tz + i);
            }
            poner("pie", tz + filas);
        }
    }
    // **EL ESPOLÓN QUE ENMARCA EL PASO.** El kit no dibuja la escalinata suelta:
    // la dibuja entre dos costillas de roca (regla 8 del proyecto — una pieza de
    // kit no se siembra sola). En una pared de un escalón eso lo resolvía la
    // pieza compuesta de 1×3 (`mejilla_de_rampa`); apilada hay que poner el
    // espolón fila a fila, del lado que dé al paso.
    if let Some(lado) = espolon_de_paso(body, tx, tz) {
        let cliff = terrain_art_key(art, body, Tile::Cliff, tx, tz)
            .or(tile_art_key(Tile::Cliff))
            .unwrap_or("cliff/roca");
        if art.has_pose(cliff, &format!("{lado}_cuerpo")) {
            let mut espolon = |pose: String, fila: i64| {
                let r = render_xy(tx as f32 * TILE_M + 0.5, fila as f32 * TILE_M + 0.5);
                if let Some((sprite, anim)) = art.tile_sprite(cliff, &pose, medida) {
                    let mut ent = commands.spawn((
                        SurfaceDecor { cx, cz, z, small: false },
                        TileArt,
                        // Misma pared, misma columna: mismo gris por fila.
                        tenir_sprite(sprite, gris(fila)),
                        Transform::from_xyz(r.x, r.y, orden),
                    ));
                    if let Some(a) = anim {
                        ent.insert(a);
                    }
                }
            };
            espolon(format!("{lado}_labio"), tz);
            for i in 1..filas {
                espolon(format!("{lado}_cuerpo"), tz + i);
            }
            espolon(format!("{lado}_pie"), tz + filas);
        }
    }
    true
}

/// **¿DE QUÉ LADO LE TOCA EL ESPOLÓN A ESTA CELDA DE PASO?**
///
/// Devuelve `rampa_o` si el paso sigue al este (y entonces esta celda es su
/// mejilla oeste) o `rampa_e` si sigue al oeste. `None` si la celda no está en el
/// borde del paso — en el medio de una escalinata ancha no va roca.
fn espolon_de_paso(body: CelestialBody, tx: i64, tz: i64) -> Option<&'static str> {
    use ha_shared::relief::{relief_cell, ReliefCell};
    if relief_cell(body, tx, tz) != ReliefCell::Ramp {
        return None;
    }
    let paso = |dx: i64| relief_cell(body, tx + dx, tz) == ReliefCell::Ramp;
    match (paso(-1), paso(1)) {
        (false, true) => Some("rampa_o"),
        (true, false) => Some("rampa_e"),
        _ => None,
    }
}

/// **EL MURO SE CIERRA EN LAS ESQUINAS CÓNCAVAS.**
///
/// Un borde de terraza es una curva cerrada, pero `relief::relief_cell` sólo
/// marca `Cliff` la celda de la que se puede CAER caminando: la que tiene un
/// vecino ortogonal más abajo. En una esquina cóncava —la celda alta cuyos
/// cuatro vecinos están a su nivel y sólo la DIAGONAL cae— no hay de dónde
/// caerse, así que no era acantilado y nadie le pedía pieza.
///
/// Y son la mitad del perímetro. Medido sobre la Tierra (`medir_acantilados`,
/// 800×800 tiles): **1.522 esquinas cóncavas contra 3.154 caras**. Como los
/// bordes de terraza bajan casi siempre en diagonal, el muro salía alternando
/// pieza y hueco —`M?M?M?`— y en pantalla eso es una línea de puntos de roca
/// bajando por la ladera. Eso es el "acantilado cortado en seco".
///
/// La pieza que va acá **no es una pared**: la celda vecina ya cuelga la suya, y
/// debajo de ésta hay terreno alto por el que se camina —una pared de tres
/// celdas taparía meseta—. Es el CANTO, una celda: la cresta si la diagonal cae
/// al norte, el labio si cae al sur (`import-acantilado.py::CARA_CANTO`).
///
/// **Es dibujo y nada más.** La celda sigue siendo terraza llana: no colisiona,
/// no cambia de tile y `relief_cell` no se entera. Cerrar el muro no puede
/// cerrarle el paso a nadie.
fn spawn_chunk_canto_concavo(
    commands: &mut Commands,
    art: &ArtLibrary,
    tiles: &TileWorld,
    body: CelestialBody,
    z: i32,
    cx: i64,
    cz: i64,
) {
    use ha_shared::relief::relief_level;
    if z != 0 {
        return;
    }
    for lx in 0..CHUNK {
        let tx = cx * CHUNK + lx;
        let mut sombra_muro: Option<[f32; CHUNK as usize]> = None;
        for lz in 0..CHUNK {
            let tz = cz * CHUNK + lz;
            let nivel = relief_level(body, tx, tz);
            // Sólo terreno alto. Bajo el nivel del mar no hay cantos que cerrar:
            // la costa se entra caminando (`relief_cell`).
            if nivel <= 0 {
                continue;
            }
            // Si algún ortogonal cae, esta celda YA es acantilado y su pieza la
            // dibuja el bucle de arriba. El descarte va primero porque es lo que
            // salta la enorme mayoría de las celdas de un chunk.
            if [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)]
                .into_iter()
                .any(|(dx, dz)| relief_level(body, tx + dx, tz + dz) < nivel)
            {
                continue;
            }
            // **La diagonal que cae tiene que ser TIERRA.** Contra el líquido no
            // hay muro —la orilla se camina—, así que un nivel negativo no abre
            // esquina: es la misma regla que ya aplica `relief_cell`.
            let baja = |dx: i64, dz: i64| {
                let n = relief_level(body, tx + dx, tz + dz);
                n >= 0 && n < nivel
            };
            if !(baja(1, 1) || baja(-1, 1) || baja(1, -1) || baja(-1, -1)) {
                continue;
            }
            // Lo que se dibuja encima manda: si acá hay una casa, una calle o un
            // árbol plantado, el canto no se cuela por debajo.
            if tiles.render_tile(body, tx, tz) != suelo_del_relieve(body, tx, tz) {
                continue;
            }
            // **Y la garganta de una cascada también manda**, igual que en el
            // bucle de las caras. Este cierre de esquina no lo miraba, y por eso
            // en el salto de la sierra asomaban dos cuadrados de arena sueltos
            // flotando sobre el agua, a los costados de la boca: eran cantos
            // cóncavos pedidos por el relieve dentro del recorte del pack, que
            // ya trae su propia roca a los dos lados.
            // Y tampoco dentro de un hombro: ahí la pared la arma
            // [`spawn_chunk_hombros_de_garganta`] de una pieza, y un canto suelto
            // encima sería roca sobre roca.
            if ha_shared::agua::hombro_de_garganta(body, tx, tz).is_some() {
                continue;
            }
            if ha_shared::agua::en_la_garganta(body, tx, tz) {
                continue;
            }
            let generic = tile_art_key(Tile::Cliff);
            let key = terrain_art_key(art, body, Tile::Cliff, tx, tz)
                .or(generic)
                .unwrap_or("cliff/roca");
            if !art.tile_has_art(key) {
                continue;
            }
            // El mismo criterio de "igual" que el muro: no es de qué está hecha
            // la celda de al lado, es por dónde se cae.
            let same = |dx: i64, dz: i64| relief_level(body, tx + dx, tz + dz) >= nivel;
            let shape = autotile_shape(
                same(0, -1), same(1, 0), same(0, 1), same(-1, 0),
                same(1, -1), same(-1, -1), same(1, 1), same(-1, 1),
            );
            // Con los cuatro lados al mismo nivel, `autotile_shape` sólo puede
            // devolver `inner_*` o `center`; y `center` es la pared entera, que
            // acá taparía meseta. Si sale eso, no hay esquina que cerrar.
            if !shape.starts_with("inner_") {
                continue;
            }
            let vk = art.variant_key_seeded(key, celda_semilla(tx, tz));
            let medida = Vec2::splat(TILE_M);
            let Some((sprite, anim)) = art.tile_sprite(&vk, shape, medida) else { continue };
            let wz = tz as f32 * TILE_M + 0.5;
            let r = render_xy(tx as f32 * TILE_M + 0.5, wz);
            // Un canto cóncavo es meseta con la esquina mordida: lleva el gris de
            // SU celda, la misma que pinta la malla debajo (ver [`gris_de_celda`]).
            let s = *sombra_muro
                .get_or_insert_with(|| sombra_de_muro_en_la_columna(body, tx, cz * CHUNK));
            let gris = gris_de_celda(tiles, body, tx, tz, s[lz as usize]);
            let mut ent = commands.spawn((
                SurfaceDecor { cx, cz, z, small: false },
                TileArt,
                tenir_sprite(sprite, gris),
                // Mismo orden que el resto del relieve: con `ysort(wz)` el
                // jugador parado en la mitad norte de la celda quedaba DETRÁS
                // de este canto (meseta dibujada encima de sus pies).
                Transform::from_xyz(r.x, r.y, crate::layers::relieve(wz)),
            ));
            crate::light2d::sin_sombra(&mut ent);
            if let Some(a) = anim {
                ent.insert(a);
            }
        }
    }
}

/// **LO QUE LA CIUDAD LE CUELGA AL ACANTILADO.**
///
/// La misma pared cuenta dos cosas distintas según dónde esté: en mitad del
/// bosque es geología y no lleva nada encima; cayendo al borde de un pueblo
/// lleva la escalera por la que se baja, el caño que desagua sobre la terraza y
/// la reja del sumidero. Eso es lo que pidió el usuario —"los tubos y escaleras
/// en un contexto más cercano a la ciudad, para que tengan sentido"— y es
/// también la razón de que no se siembren por bioma: una escalera clavada en un
/// farallón del desierto no la puso nadie.
///
/// **"Cerca de gente" es DENTRO DEL CASCO** (`settlement_covering`), no en la
/// celda de sitio de 2.600 tiles: ésa es la lección que ya costó una vuelta con
/// los semáforos (biblia §8).
///
/// Sólo entra en las caras que MIRAN AL SUR: son las únicas que enseñan la pared
/// entera, y una escalera de canto no se ve. La pieza cuelga sobre las celdas de
/// roca que tapa el muro —las que cuenta `relief::alto_de_pared`—, apoyada en el
/// pie.
fn spawn_chunk_cliff_urbano(
    commands: &mut Commands,
    art: &ArtLibrary,
    tiles: &TileWorld,
    body: CelestialBody,
    z: i32,
    cx: i64,
    cz: i64,
) {
    // Bajo tierra no hay ni ciudad ni ladera. Y sin arte curado no hay nada que
    // sembrar: se pregunta UNA vez, antes de mirar el chunk.
    if z != 0 {
        return;
    }
    let familias: Vec<&'static str> = CLIFF_URBANO
        .iter()
        .copied()
        .filter(|k| art.tile_has_art(k))
        .collect();
    if familias.is_empty() {
        return;
    }
    for lx in 0..CHUNK {
        for lz in 0..CHUNK {
            let tx = cx * CHUNK + lx;
            let tz = cz * CHUNK + lz;
            if tiles.render_tile(body, tx, tz) != Tile::Cliff {
                continue;
            }
            // Sólo la cara que se ve entera: una escalera de canto no se ve.
            let nivel = ha_shared::relief::relief_level(body, tx, tz);
            if ha_shared::relief::relief_level(body, tx, tz + 1) >= nivel {
                continue;
            }
            // **CUÁNTA PARED HAY.** Un bordillo que se trepa
            // (`relief::climbable_edge`) enseña UNA celda de roca y un muro dos,
            // y de eso depende qué se le puede colgar: en el bordillo entra la
            // reja del sumidero y nada más; la escalera y el caño piden el muro.
            //
            // No es un detalle: **las ciudades se fundan en llano**, así que casi
            // todo desnivel urbano es bordillo. Descartándolos —que fue el primer
            // intento— la ladera de ciudad no llevaba nunca nada encima y la
            // regla no se veía en ninguna parte del mundo.
            // La misma medida que dibujó la pared, o la escalera se apoya en un
            // pie que no está donde está la roca. El bordillo enseña UNA celda y
            // un muro de `n` escalones enseña `n + 1`.
            let celdas = match ha_shared::relief::alto_de_pared(body, tx, tz) {
                0 => continue,
                1 => CELDAS_DE_BORDILLO,
                n => n + 1,
            };
            // **Una cada tantas celdas de pared, no una por celda.** Un
            // acantilado con escalera cada metro es una escalera de caracol, no
            // una ladera. El sorteo es por celda y determinista: la misma pared
            // saca siempre las mismas piezas aunque el chunk entre y salga del
            // streaming diez veces.
            let semilla = celda_semilla(tx, tz);
            let mut h = semilla.wrapping_mul(0x9E37_79B9_7F4A_7C15);
            h ^= h >> 31;
            if h % URBANO_UNA_DE_CADA != 0 {
                continue;
            }
            // **"Cerca de gente" es DENTRO DEL CASCO**, no en la celda de sitio
            // de 2.600 tiles: la lección que ya costó una vuelta con los
            // semáforos (biblia §8). Se pregunta por CELDA y no por chunk porque
            // una ladera cae casi siempre en el BORDE del pueblo, y mirando sólo
            // el centro del chunk la mitad de las laderas urbanas quedaban
            // peladas — que es lo que se vio en `relieve-2-urbano`.
            //
            // Sale caro por celda (nueve celdas de sitio) y no importa: para
            // llegar acá una celda tiene que ser cara sur, con pared, y haber
            // ganado un sorteo de 1 entre `URBANO_UNA_DE_CADA`. En un chunk de
            // 1.024 celdas eso es menos de una consulta.
            if ha_shared::geography::settlement_covering(body, tx, tz).is_none() {
                continue;
            }
            let familia = familias[((h >> 8) % familias.len() as u64) as usize];
            let vk = art.variant_key_seeded(familia, semilla);
            // **La pieza se apoya en el pie y ASOMA lo que le sobre.** Se probó
            // recortarla al alto exacto del desnivel y en un bordillo —que es
            // casi todo el relieve urbano, porque las ciudades se fundan en
            // llano— la escalera quedaba en dos peldaños sueltos que no se leían
            // como nada. Una escalera apoyada contra un muro bajo asoma por
            // encima del canto: eso es lo que hace una escalera de verdad, y es
            // lo que la vuelve reconocible de un vistazo.
            let alto = art.medida_de_obra(&vk, Some("alta"), Vec2::splat(TILE_M));
            let Some((sprite, anim)) = art.tile_sprite(&vk, "alta", alto) else { continue };
            // El pie de la pieza se apoya en el PIE DE LA PARED: la celda del
            // acantilado más las celdas de roca que cuelgan. De ahí para arriba
            // crece lo que mida el recorte, que es lo que hace que una escalera
            // corta y un caño largo salgan los dos bien apoyados.
            let pie_wz = (tz + 1 + celdas) as f32 * TILE_M;
            let centro_wz = pie_wz - alto.y * 0.5;
            let r = render_xy(tx as f32 * TILE_M + 0.5, centro_wz);
            // **Y con el gris de la pared en la que se apoya.** Desde que el muro
            // lleva el suyo (ver [`gris_de_celda`]), una escalera sin teñir
            // quedaba a pleno sol contra una roca en penumbra — o al revés en la
            // cumbre, que aclara un 40 %. Va el del CANTO, que es la celda a la
            // que la pieza está colgada. Sin la sombra de columna: esa franja la
            // tira esta misma pared.
            let gris = gris_de_celda(tiles, body, tx, tz, 1.0);
            let mut ent = commands.spawn((
                SurfaceDecor { cx, cz, z, small: true },
                TileArt,
                tenir_sprite(sprite, gris),
                // Y-sort por el pie, como cualquier cosa apoyada: el que camina
                // por la terraza de abajo pasa por delante de la escalera.
                Transform::from_xyz(r.x, r.y, crate::layers::ysort(pie_wz)),
            ));
            if let Some(a) = anim {
                ent.insert(a);
            }
        }
    }
}

/// Las familias que le cuelga la ciudad a un acantilado. Ver
/// [`spawn_chunk_cliff_urbano`]; el pixel lo trae `scripts/import-acantilado.py`.
const CLIFF_URBANO: [&str; 3] = ["cliff/escalera", "cliff/tubo", "cliff/reja"];

/// Celdas de ROCA que cuelgan del canto de un BORDILLO: una sola (es
/// `ALTO_BORDILLO - 1` del importador). Las paredes de verdad no tienen constante
/// porque no tienen alto fijo — se lo pregunta cada una a
/// `relief::alto_de_pared`.
const CELDAS_DE_BORDILLO: i64 = 1;

/// Una de cada cuántas celdas de pared lleva algo colgado. Con 6 sale una pieza
/// cada dos o tres metros de ladera urbana: se nota que la gente usa ese
/// desnivel sin que la pared se convierta en un andamio.
const URBANO_UNA_DE_CADA: u64 = 6;

/// **La línea pintada de la calzada, y el cordón que la bordea.**
///
/// Van aparte del arte de tile porque no son del suelo: el asfalto y la vereda
/// son materiales que cubren celdas y los dibuja la malla del chunk
/// (`suelo_uv_seeded`), mientras que esto son CALCOMANÍAS encima. La línea es una
/// RED que sigue el eje de la calle con sus empalmes en T y en cruz; el cordón es
/// un autotile que sólo existe donde la calzada toca la vereda.
///
/// Sólo cuesta en las celdas que son eje (`roads::lane_mark_at`) y en el borde de
/// la calzada: en un chunk de campo no spawnea nada.
/// **LAS CASCADAS DEL CHUNK.**
///
/// No se siembran: se DERIVAN. `ha_shared::agua::cascada_at` responde sólo en la
/// esquina noroeste de un salto —una pared de terraza de tres tiles de ancho con
/// agua arriba y agua abajo— y devuelve cuántos PISOS de meseta cae. Acá eso se
/// arma con las piezas modulares del pack: la boca, un tramo de caída por piso y
/// la poza reventando sobre el agua de abajo.
///
/// Por eso una cascada nunca está "puesta ahí porque sí": no puede existir donde
/// el agua no baje de verdad un escalón.
///
/// # AL RAS DE LA MESETA, AL RAS DEL SUELO
///
/// Las filas son las MISMAS que las de la pared que el kit cuelga al lado
/// (`pared_apilada`: labio en el canto, un cuerpo por piso, el pie debajo):
///
/// ```text
///   fila tz            boca    ↔  labio   ← al ras de la meseta
///   filas tz+1..tz+n   tramo   ↔  cuerpo     (n = pisos que baja)
///   fila tz+n+1        poza    ↔  pie     ← al ras del suelo de abajo
///   fila tz+n+2        poza                  el reventón, ya sobre el agua
/// ```
///
/// Las dos puntas estaban corridas y las dos se veían. La boca se ponía en
/// `tz-1` —una fila POR ENCIMA del canto—, así que el labio de roca del recorte
/// asomaba flotando en mitad del lago de arriba (los dos cuadrados de arena que
/// se ven en `agua-4-sierra.png` de la corrida vieja). Y el chorro medía lo que
/// `cascada_at` devolvía en TILES sobre el mar: diez filas colgando de un
/// desnivel de un escalón, terminando en mitad del río.
/// **De qué material es la pared que rodea a un salto**, con el mismo criterio
/// con el que [`terrain_art_key`] elige la cara de acantilado: el material del
/// suelo que corona y, si es pasto, el clima que le toca.
///
/// Que sea el MISMO criterio es todo el punto — la garganta y la pared que tiene
/// pegada al lado se dibujan con el mismo color o no se dibujan bien.
fn material_de_cascada(body: CelestialBody, tx: i64, tz: i64) -> &'static str {
    use ha_shared::geography::Pasto;
    let s = ha_shared::geography::superficie_de(body, tx, tz);
    let mat = material_de_suelo(s.tile);
    if mat != "pasto" {
        return mat;
    }
    match s.pasto {
        Pasto::Bosque => "pasto_bosque",
        Pasto::Seco => "pasto_seco",
        Pasto::Pantano => "pasto_pantano",
        Pasto::Sierra => "pasto_sierra",
        Pasto::Referencia => "pasto",
    }
}

/// La clave de la copia teñida de una cascada. **Mismo formato que arma
/// `import-agua.py::clave_de_cascada`**: si allá cambia, acá también.
///
/// Los dos ejes van en el nombre porque son independientes: la variante del pack
/// (qué dibujo le tocó al salto, por posición) y el material (de qué está hecha
/// la pared, por bioma).
fn clave_de_cascada(crudo: &str, material: &str) -> String {
    let n = match crudo {
        "agua/cascada#2" => "2",
        "agua/cascada#3" => "3",
        _ => "1",
    };
    format!("agua/cascada#{n}_{material}")
}

/// **LOS DOS HOMBROS DE CADA GARGANTA — el módulo que cierra la cascada.**
///
/// Un salto es tres piezas y siempre las mismas: el chorro en el medio
/// ([`spawn_chunk_cascadas`]) y una pared de roca a cada lado, de la misma
/// altura, empezando en la misma fila. Eso último no salía del relieve —de los
/// 22 hombros medidos, 9 no eran ni pared y 7 tenían el canto desfasado; ver
/// `agua::hombro_de_garganta`— y en pantalla el chorro colgaba al lado de una
/// banda de acantilado más corta y a otra altura.
///
/// Acá el hombro se ARMA, con las mismas filas sueltas que apila
/// [`pared_apilada`] y con el material de la garganta ([`material_de_cascada`]),
/// que es lo que hace que la roca del costado y la del recorte sean del mismo
/// color:
///
/// ```text
///   fila tz            labio   ← al ras de la meseta, la fila de la boca
///   filas tz+1..tz+n   cuerpo     (n = pisos que baja el salto)
///   fila tz+n+1        pie     ← al ras del suelo de abajo, donde revienta la poza
/// ```
///
/// **Es dibujo y nada más**, como el cierre de esquinas cóncavas: la celda sigue
/// siendo lo que el relieve diga, no colisiona distinto y nadie se queda
/// encerrado por un hombro.
fn spawn_chunk_hombros_de_garganta(
    commands: &mut Commands,
    art: &ArtLibrary,
    body: CelestialBody,
    z: i32,
    cx: i64,
    cz: i64,
) {
    if z != 0 {
        return;
    }
    for lx in 0..CHUNK {
        for lz in 0..CHUNK {
            let tx = cx * CHUNK + lx;
            let tz = cz * CHUNK + lz;
            let Some((fila, pisos)) = ha_shared::agua::hombro_de_garganta(body, tx, tz) else {
                continue;
            };
            // La misma clave de arte que usa la pared de al lado: el material
            // del suelo que corona, con su clima si es pasto.
            let key = match material_de_cascada(body, tx, fila) {
                "arena" => "cliff/arena",
                "nieve" => "cliff/nieve",
                "regolito" => "cliff/regolito",
                "basalto" => "cliff/basalto",
                m => match m {
                    "pasto_bosque" => "cliff/pasto_bosque",
                    "pasto_seco" => "cliff/pasto_seco",
                    "pasto_pantano" => "cliff/pasto_pantano",
                    "pasto_sierra" => "cliff/pasto_sierra",
                    _ => "cliff/pasto",
                },
            };
            if !(art.has_pose(key, "labio") && art.has_pose(key, "cuerpo") && art.has_pose(key, "pie"))
            {
                continue;
            }
            let medida = Vec2::splat(TILE_M);
            // Un hombro es UN objeto: todas sus filas se ordenan por el canto, o
            // el que camina por la terraza de abajo se mete entre dos de ellas.
            let orden = crate::layers::relieve(fila as f32 * TILE_M + 0.5);
            let mut poner = |pose: &'static str, f: i64| {
                let r = render_xy(tx as f32 * TILE_M + 0.5, f as f32 * TILE_M + 0.5);
                if let Some((sprite, anim)) = art.tile_sprite(key, pose, medida) {
                    let mut ent = commands.spawn((
                        SurfaceDecor { cx, cz, z, small: false },
                        TileArt,
                        sprite,
                        Transform::from_xyz(r.x, r.y, orden),
                    ));
                    if let Some(a) = anim {
                        ent.insert(a);
                    }
                }
            };
            poner("labio", fila);
            for i in 0..pisos {
                poner("cuerpo", fila + 1 + i);
            }
            poner("pie", fila + 1 + pisos);
        }
    }
}

fn spawn_chunk_cascadas(
    commands: &mut Commands,
    art: &ArtLibrary,
    tiles: &TileWorld,
    body: CelestialBody,
    z: i32,
    cx: i64,
    cz: i64,
) {
    // Bajo tierra no llueve ni cae ningún río: sólo la superficie.
    if z != 0 {
        return;
    }
    for lx in 0..CHUNK {
        for lz in 0..CHUNK {
            let tx = cx * CHUNK + lx;
            let tz = cz * CHUNK + lz;
            if tiles.render_tile(body, tx, tz) != Tile::Cliff {
                continue;
            }
            let Some(pisos) = ha_shared::agua::cascada_at(body, tx, tz) else { continue };
            let crudo = match ha_shared::agua::variante_cascada(tx, tz) {
                0 => "agua/cascada",
                1 => "agua/cascada#2",
                _ => "agua/cascada#3",
            };
            // **LA GARGANTA SE VISTE DEL MATERIAL DE SU PARED.** El recorte del
            // pack trae su roca terracota dibujada y el kit de acantilado sale
            // teñido con la losa que corona, así que puestos uno al lado del otro
            // —que es literalmente cómo se dibujan— el salto era una cicatriz
            // naranja en un muro verde grisáceo. `import-agua.py` corta una copia
            // teñida por material, con los seis frames de cada pose.
            let tenida = clave_de_cascada(crudo, material_de_cascada(body, tx, tz));
            let base: &str = if art.has_pose(&tenida, "tramo") { &tenida } else { crudo };
            // Sin arte curado no se dibuja nada: el salto sigue siendo la pared
            // de terraza de siempre, que es una lectura correcta del terreno.
            if !art.has_pose(base, "tramo") {
                continue;
            }
            // La poza del pack mide dos tiles de alto (la última caída más el
            // reventón). Se lee del propio recorte, y **lo que falta cae en
            // `POZA_FILAS`, no en 1**: ése es el número que usa la sim para saber
            // hasta dónde llega el dibujo (`filas_de_dibujo`, `bajo_el_chorro`,
            // `poza_de_cascada`), así que un respaldo distinto ponía a las dos
            // mitades a contar filas diferentes. El comentario decía que las
            // variantes derivadas de la hoja entera medían una; medido sobre el
            // manifest, las 30 miden dos, y ahora hay guardián
            // (`art2d::la_poza_del_pack_mide_lo_que_dice_la_sim`).
            let poza_alto = art
                .pose(base, "poza")
                .map(|(r, _)| (r[3] / 32).max(1) as i64)
                .unwrap_or(ha_shared::agua::POZA_FILAS);
            let ancho = ha_shared::agua::ANCHO_CASCADA;
            // Centro en X del chorro: las tres celdas del ancho.
            let wx = (tx as f32 + ancho as f32 * 0.5) * TILE_M;

            let mut poner = |pose: &'static str, fila: i64, alto_celdas: i64| {
                let medida = Vec2::new(ancho as f32 * TILE_M, alto_celdas as f32 * TILE_M);
                let centro = render_xy(wx, (fila as f32 + alto_celdas as f32 * 0.5) * TILE_M);
                let Some((sprite, anim)) = art.tile_sprite(base, pose, medida) else { return };
                // **EL SUELO NUNCA TAPA AL JUGADOR — y un salto es agua.**
                //
                // Las tres piezas Y-sorteaban por el PIE del dibujo entero, así
                // que la poza —que mide dos filas y termina abajo del todo— se
                // ordenaba como si fuese una pared de dos metros: el que se
                // metía a la poza quedaba DETRÁS del reventón, tapado por agua
                // que le llegaba a los tobillos. Lo mismo la boca contra el que
                // vadea el río de arriba.
                //
                // La boca y la poza son líquido acostado en el suelo: van a
                // [`layers::FLOOR`], igual que el agua del río, y por definición
                // no tapan a nadie. El único trozo que se LEVANTA es el chorro,
                // y ése Y-sortea por SU PROPIA fila —no por el pie del conjunto—
                // para que pararse al lado de una fila de caída te ponga
                // delante de ella y no detrás.
                //
                // **Y un pelo por encima del agua** ([`layers::SOBRE_EL_PISO`]):
                // la celda de la boca y las de la poza dibujan además su propio
                // tile de agua, que también pide `FLOOR` exacto. Dos sprites con
                // el mismo z no tienen orden — funcionaba de casualidad, porque
                // esta función corre después del bucle de tiles del chunk, y
                // dejaba de funcionar en cuanto la poza caía del otro lado de una
                // frontera de chunk.
                let capa = match pose {
                    "tramo" => crate::layers::ysort((fila + 1) as f32 * TILE_M),
                    _ => crate::layers::FLOOR + crate::layers::SOBRE_EL_PISO,
                };
                let mut ent = commands.spawn((
                    SurfaceDecor { cx, cz, z, small: false },
                    TileArt,
                    sprite,
                    Transform::from_xyz(centro.x, centro.y, capa),
                ));
                // **Pero el chorro no proyecta sombra**: Y-sortea porque hay que
                // pasarle por delante, no porque se levante del suelo. Es agua.
                crate::light2d::sin_sombra(&mut ent);
                if let Some(a) = anim {
                    ent.insert(a);
                }
            };

            // **La boca EN el canto**, no encima: es la fila que la pared de al
            // lado dibuja como labio, y es lo que pone el principio del chorro al
            // ras de la meseta.
            poner("boca", tz, 1);
            for i in 0..pisos {
                poner("tramo", tz + 1 + i, 1);
            }
            // La poza arranca en la fila del PIE de la pared y desborda una más
            // abajo: el reventón va sobre el agua, un cuadro después de donde la
            // roca toca el suelo.
            poner("poza", tz + 1 + pisos, poza_alto);
        }
    }
}

fn spawn_chunk_lane_marks(
    commands: &mut Commands,
    art: &ArtLibrary,
    tiles: &TileWorld,
    body: CelestialBody,
    z: i32,
    cx: i64,
    cz: i64,
) {
    // La calzada pintada es de la superficie: bajo tierra no hay calle. La senda
    // entra aunque `road/linea` no esté curada — su respaldo lo dibuja
    // `procedural_art`, así que la esquina nunca se queda sin paso de peatones.
    let hay_pintura =
        art.tile_has_art(LANE_MARK_ART) || art.has_art(SENDA_NS_ART) || art.has_poses(CORDON_ART);
    if z != 0 || !hay_pintura {
        return;
    }
    // **Primero: ¿hay calle acá?** Armar el mapa de calzada del chunk cuesta una
    // consulta de geografía por celda, y la enorme mayoría de los chunks del
    // mundo son campo sin una sola calle. El sondeo va de dos en dos y no puede
    // perderse una calzada: la más angosta que se pinta mide tres tiles
    // (`ANCHO_MINIMO_LINEA`), y una retícula de paso 2 siempre pega en una de
    // tres celdas seguidas.
    let hay_calle = (0..CHUNK).step_by(2).any(|lz| {
        (0..CHUNK).step_by(2).any(|lx| {
            matches!(
                ha_shared::geography::civil_kind_at(body, cx * CHUNK + lx, cz * CHUNK + lz),
                Some(ha_shared::geography::Civil::Road)
            )
        })
    });
    if !hay_calle {
        return;
    }
    // De una pasada para el chunk entero: celda por celda son ~240 consultas de
    // geografía cada una (ver `roads::lane_marks_rect`), y eso es un tirón por
    // manzana nueva.
    let lado = CHUNK as usize;
    let marcas = ha_shared::roads::pintura_rect(body, cx * CHUNK, cz * CHUNK, lado, lado);
    for lz in 0..CHUNK {
        for lx in 0..CHUNK {
            let Some(pintura) = marcas[lz as usize * lado + lx as usize] else { continue };
            let tx = cx * CHUNK + lx;
            let tz = cz * CHUNK + lz;
            // La línea es una RED con poses por conexión; la senda es una losa
            // sola que se repite a lo ancho de la calzada. Por eso son dos slots
            // y no dos poses del mismo.
            let sprite = match pintura {
                ha_shared::roads::Pintura::Linea(forma) => {
                    art.tile_sprite(LANE_MARK_ART, forma, Vec2::splat(TILE_M)).map(|(s, _)| s)
                }
                ha_shared::roads::Pintura::Senda { calle_ns } => {
                    let clave = if calle_ns { SENDA_NS_ART } else { SENDA_EW_ART };
                    art.world_sprite(clave, Vec2::splat(TILE_M)).map(|(s, _)| s)
                }
                // **La bahía de bus y el cajón**: un dibujo grande repartido en
                // celdas, así que la pose ES la casilla del recorte. El volteo
                // sirve al cordón de enfrente: la misma bahía espejada, sin que el
                // pack tenga que dibujarla dos veces.
                ha_shared::roads::Pintura::Marca {
                    clase,
                    calle_ns,
                    i,
                    j,
                    voltear_x,
                    voltear_z,
                } => {
                    let eje = if calle_ns { "ns" } else { "ew" };
                    let pose = format!("{eje}_{i}_{j}");
                    art.tile_sprite(clase.slot(), &pose, Vec2::splat(TILE_M)).map(|(mut s, _)| {
                        s.flip_x = voltear_x;
                        s.flip_y = voltear_z;
                        s
                    })
                }
            };
            let Some(sprite) = sprite else { continue };
            let wx = tx as f32 * TILE_M + 0.5;
            let wz = tz as f32 * TILE_M + 0.5;
            let r = render_xy(wx, wz);
            commands.spawn((
                SurfaceDecor { cx, cz, z, small: true },
                TileArt,
                sprite,
                // Justo encima del asfalto y bajo los pies de todo el mundo: una
                // raya pintada no tapa ni al que la pisa ni al auto que pasa.
                Transform::from_xyz(r.x, r.y, crate::layers::FLOOR + 0.01),
            ));
        }
    }
    spawn_chunk_cordon(commands, art, tiles, body, z, cx, cz);
    spawn_chunk_muelle(commands, art, tiles, body, z, cx, cz);
    spawn_chunk_puentes(commands, art, body, z, cx, cz);
}

/// Slot del pack que hace de **pretil** del puente: el muro de piedra de la ribera (`quay/muro`), el mismo de los canales de la
/// ciudad. En las láminas de LimeZu el puente es la calle que sigue sobre el agua, entre dos pretiles de piedra.
const PRETIL_ART: &str = MUELLE_ART;

/// **Los PUENTES**: donde una calzada cruza agua (un enlace interurbano o un canal dentro de la ciudad), `geography::civil_at`
/// devuelve `Civil::Bridge`. Lleva **pretil de piedra a cada lado** que da a algo que NO es calzada; un puente de dos tramos
/// no se llena de pretiles en el medio.
fn spawn_chunk_puentes(commands: &mut Commands, art: &ArtLibrary, body: CelestialBody, z: i32, cx: i64, cz: i64) {
    use ha_shared::geography::{civil_at, Civil};
    if z != 0 || !art.has_poses(PRETIL_ART) {
        return;
    }
    let es_calzada = |x: i64, y: i64| matches!(civil_at(body, x, y), Some(Civil::Road | Civil::Bridge));
    for lz in 0..CHUNK {
        for lx in 0..CHUNK {
            let (tx, tz) = (cx * CHUNK + lx, cz * CHUNK + lz);
            if !matches!(ha_shared::geography::civil_kind_at(body, tx, tz), Some(Civil::Road)) {
                continue;
            }
            if civil_at(body, tx, tz) != Some(Civil::Bridge) {
                continue;
            }
            // (dx, dz, pose): la pieza de ribera que mira hacia ese lado.
            for (dx, dz, pose) in [(0i64, -1i64, "edge_n"), (0, 1, "edge_s"), (-1, 0, "edge_w"), (1, 0, "edge_e")] {
                if es_calzada(tx + dx, tz + dz) {
                    continue;
                }
                let Some((sprite, _)) = art.tile_sprite(PRETIL_ART, pose, Vec2::splat(TILE_M)) else { continue };
                let r = render_xy(tx as f32 * TILE_M + 0.5, tz as f32 * TILE_M + 0.5);
                let mut ent = commands.spawn((
                    SurfaceDecor { cx, cz, z, small: true },
                    TileArt,
                    sprite,
                    Transform::from_xyz(r.x, r.y, crate::layers::FLOOR + 0.04),
                ));
                crate::light2d::sin_sombra(&mut ent);
            }
        }
    }
}

/// **El CORDÓN: por donde la vereda se levanta sobre la calzada.**
///
/// Sin él, el asfalto y la acera se tocaban a filo —dos colores de piso pegados—
/// y una calle se leía como una franja gris entre dos franjas claras. El cordón
/// es lo que dice cuál de las dos está más alta, y es la pieza que hace que la
/// vereda de una ciudad sea una vereda de ciudad y no el sendero de un parque.
///
/// **Va sobre la celda de ASFALTO, no sobre la de vereda.** El pack lo dibuja
/// así y es lo correcto en 3/4: lo que se ve del cordón desde arriba es su CARA,
/// que cae del lado de la calle. La vereda en sí la sigue pintando la malla del
/// chunk con su baldosa.
///
/// El borde sólo nace contra la VEREDA: una ruta de campo con el pasto pegado a
/// la calzada no lleva cordón, y preguntarle a `autotile_shape` por "vecino del
/// mismo material" se lo habría puesto igual. Por eso la vecindad se mide contra
/// `Tile::Pavement` y no contra "es asfalto".
fn spawn_chunk_cordon(
    commands: &mut Commands,
    art: &ArtLibrary,
    tiles: &TileWorld,
    body: CelestialBody,
    z: i32,
    cx: i64,
    cz: i64,
) {
    if !art.has_poses(CORDON_ART) {
        return;
    }
    // La familia se pregunta UNA vez por chunk y no por celda. Un chunk mide 16
    // tiles y un casco urbano cientos, así que todas las celdas de acera de un
    // chunk son de la misma ciudad; preguntarlo por celda serían nueve consultas
    // de sitio por celda de calzada para obtener siempre lo mismo.
    let fam = vereda_de_la_ciudad(
        art,
        body,
        cx * CHUNK + CHUNK / 2,
        cz * CHUNK + CHUNK / 2,
    );
    let cordon_art = VEREDA_CORDON[fam];
    for lz in 0..CHUNK {
        for lx in 0..CHUNK {
            let (tx, tz) = (cx * CHUNK + lx, cz * CHUNK + lz);
            if tiles.tile_at(body, tx, tz) != Tile::Asphalt {
                continue;
            }
            // `same` = "de este lado NO hay vereda", que es la convención de
            // `autotile_shape`: el borde mira hacia los lados distintos.
            let calle = |dx: i64, dz: i64| tiles.tile_at(body, tx + dx, tz + dz) != Tile::Pavement;
            let shape = autotile_shape(
                calle(0, -1), calle(1, 0), calle(0, 1), calle(-1, 0),
                calle(1, -1), calle(-1, -1), calle(1, 1), calle(-1, 1),
            );
            // Sin vereda alrededor no hay cordón que dibujar. `road/cordon` no
            // trae pose `center` a propósito (ver `scripts/import-calles.py`):
            // así una calzada en medio del campo no paga un quad por celda.
            if shape == "center" {
                continue;
            }
            // **LA CUNETA.** Cada tantos metros el cordón norte lleva la rejilla
            // del sumidero, que es lo que una cuneta es: un desagüe cada tanto,
            // no un canal continuo. Antes esta pieza era el cordón NORMAL —la
            // tabla de índices leída a mano le había dado a `edge_n` la losa con
            // rejilla—, así que toda ciudad del mundo tenía una alcantarilla
            // cada 95 cm a lo largo de cada cuadra. Ver `import-calles.py`.
            //
            // El periodo va sobre `tx` y no sobre un hash: un desagüe se pone a
            // intervalos, no al azar, y así la fila entera de la cuadra queda
            // pareja. Nueve celdas son unos 8,5 m.
            let toca_rejilla = shape == "edge_n" && tx.rem_euclid(9) == 0;
            let dibujo = if toca_rejilla {
                // Sin rejilla curada en esta familia, el cordón liso de siempre:
                // el respaldo es sólo para ESTA pose, no para todas — si faltara
                // una esquina, dibujarle un borde recto sería peor que no
                // dibujar nada.
                art.tile_sprite(cordon_art, CORDON_REJILLA, Vec2::splat(TILE_M))
                    .or_else(|| art.tile_sprite(cordon_art, shape, Vec2::splat(TILE_M)))
            } else {
                art.tile_sprite(cordon_art, shape, Vec2::splat(TILE_M))
            };
            let Some((sprite, _)) = dibujo else {
                continue;
            };
            let wx = tx as f32 * TILE_M + 0.5;
            let wz = tz as f32 * TILE_M + 0.5;
            let r = render_xy(wx, wz);
            commands.spawn((
                SurfaceDecor { cx, cz, z, small: true },
                TileArt,
                sprite,
                // Debajo de la línea de carril: si dos calcomanías se solapan en
                // una esquina, manda la pintura, que es lo que se lee de lejos.
                Transform::from_xyz(r.x, r.y, crate::layers::FLOOR + 0.005),
            ));
        }
    }
}

/// Slot del muro de ribera, en poses de autotile. Ver [`spawn_chunk_muelle`].
const MUELLE_ART: &str = "quay/muro";

/// **EL CANAL: el muro que encajona el agua cuando pasa por un pueblo.**
///
/// Un río que cruza una ciudad con orilla de pasto no se lee como una ciudad: se
/// lee como que la ciudad se plantó al lado de un arroyo. Lo que hace un canal es
/// el MURO — la mampostería con su coronamiento de piedra, que es justo lo que el
/// pack dibujó en `1_Terrains_and_Fences` (tres familias `Wall_*`, 58 piezas) y
/// que el juego no usaba en ninguna parte.
///
/// Se dibuja sobre la celda de TIERRA pegada al agua, mirando al agua — la misma
/// convención y el mismo autotile que el cordón de la vereda
/// (`spawn_chunk_cordon`), porque es el mismo problema: un borde que se levanta
/// sobre lo de al lado.
///
/// **Sólo dentro de un casco** (`settlement_covering`). Fuera, el agua sigue
/// teniendo su orilla de pasto, que es lo que corresponde a un río de campo — y
/// además así el muro no cuesta una sola consulta en los millones de celdas de
/// costa del planeta.
fn spawn_chunk_muelle(
    commands: &mut Commands,
    art: &ArtLibrary,
    tiles: &TileWorld,
    body: CelestialBody,
    z: i32,
    cx: i64,
    cz: i64,
) {
    if z != 0 || !art.has_poses(MUELLE_ART) {
        return;
    }
    // El casco se pregunta UNA vez por chunk, igual que la familia de vereda: un
    // chunk mide 16 tiles y un casco cientos, así que todas sus celdas son del
    // mismo pueblo o de ninguno.
    let centro = (cx * CHUNK + CHUNK / 2, cz * CHUNK + CHUNK / 2);
    if ha_shared::geography::settlement_covering(body, centro.0, centro.1).is_none() {
        return;
    }
    for lz in 0..CHUNK {
        for lx in 0..CHUNK {
            let (tx, tz) = (cx * CHUNK + lx, cz * CHUNK + lz);
            let aqui = tiles.tile_at(body, tx, tz);
            // El muro se planta en tierra firme. Sobre la calzada no —ahí el río
            // se cruza por un puente, y un puente ya trae su propio pretil.
            if aqui.swim() || aqui.solid() || matches!(aqui, Tile::Asphalt) {
                continue;
            }
            // `same` = "de este lado NO hay agua": el borde mira al lado distinto,
            // que es por donde el muro cae.
            let seco = |dx: i64, dz: i64| !tiles.tile_at(body, tx + dx, tz + dz).swim();
            let shape = autotile_shape(
                seco(0, -1), seco(1, 0), seco(0, 1), seco(-1, 0),
                seco(1, -1), seco(-1, -1), seco(1, 1), seco(-1, 1),
            );
            // Sin agua alrededor no hay muro que dibujar, y las cóncavas esta
            // familia no las trae (ver `scripts/import-muelle.py`): se cae al
            // borde recto del lado que corresponda en vez de dejar un hueco.
            let shape = match shape {
                "center" => continue,
                "inner_ne" | "inner_nw" => "edge_n",
                "inner_se" | "inner_sw" => "edge_s",
                s => s,
            };
            // **Una pileta no es un canal.** El borde de piedra de la pileta de un patio (4×2 de agua) recibía el
            // muro de ribera y salía como un cerco de bloques con el costado roto. El muro es de ríos, canales y
            // mar: sólo si hay AGUA de verdad alrededor (≥ 70 celdas en 15×15; se corta apenas las junta).
            if aqui == Tile::Pavement && !agua_de_verdad(tiles, body, tx, tz) {
                continue;
            }
            let Some((sprite, _)) = art.tile_sprite(MUELLE_ART, shape, Vec2::splat(TILE_M)) else {
                continue;
            };
            let wx = tx as f32 * TILE_M + 0.5;
            let wz = tz as f32 * TILE_M + 0.5;
            let r = render_xy(wx, wz);
            commands.spawn((
                SurfaceDecor { cx, cz, z, small: true },
                TileArt,
                sprite,
                // Encima del piso y de la pintura: el muro se levanta, no está
                // pintado en el suelo.
                Transform::from_xyz(r.x, r.y, crate::layers::FLOOR + 0.02),
            ));
        }
    }
}

/// ¿Hay un cuerpo de agua DE VERDAD (≥ 70 celdas en el entorno de 15×15) alrededor de `(tx, tz)`? Distingue un río, un
/// canal o el mar de la pileta de un patio. Sale en cuanto junta las 70.
fn agua_de_verdad(tiles: &TileWorld, body: CelestialBody, tx: i64, tz: i64) -> bool {
    let mut n = 0;
    for dz in -7..=7i64 {
        for dx in -7..=7i64 {
            if tiles.tile_at(body, tx + dx, tz + dz).swim() {
                n += 1;
                if n >= 70 {
                    return true;
                }
            }
        }
    }
    false
}


/// Qué familia de cerca lleva la parcela de esta celda (determinista por la esquina del lote).
fn cerca_de_la_parcela(body: CelestialBody, tx: i64, tz: i64) -> Option<&'static str> {
    let s = ha_shared::geography::settlement_covering(body, tx, tz)?;
    let lot = ha_shared::city::lot_at(body, &s, tx, tz)?;
    let h = ha_shared::worldgen::hash_cell(0x0ce2_ca00, lot.min.0, lot.min.1);
    Some(if h % 3 == 0 { "fence/piedra" } else { "fence/madera" })
}

/// ¿Esta celda de agua es de una PILETA de patio (y no de un lago, un río o el mar)? Hay vereda a ≤ 2 celdas y el agua
/// de alrededor no llega a [`agua_de_verdad`]. Sólo dentro de un casco urbano.
fn es_agua_de_pileta(tiles: &TileWorld, body: CelestialBody, tx: i64, tz: i64) -> bool {
    let mut vereda = false;
    'b: for dz in -2..=2i64 {
        for dx in -2..=2i64 {
            if tiles.tile_at(body, tx + dx, tz + dz) == Tile::Pavement {
                vereda = true;
                break 'b;
            }
        }
    }
    vereda && ha_shared::geography::settlement_covering(body, tx, tz).is_some() && !agua_de_verdad(tiles, body, tx, tz)
}

/// **La pileta: UN rectángulo del pack** (`agua/pileta_completa`, armado con las piezas `agua/pileta_N` —borde, esquinas,
/// azulejo de agua— y espejado abajo, para que el marco cierre simétrico). La parcela la genera de 6×4 con la vereda de borde
/// (`city::residential_tile`): se dibuja una vez, desde su esquina noroeste.
fn spawn_chunk_piletas(commands: &mut Commands, art: &ArtLibrary, tiles: &TileWorld, body: CelestialBody, z: i32, cx: i64, cz: i64) {
    if z != 0 || !art.has_art("agua/pileta_completa") {
        return;
    }
    let centro = (cx * CHUNK + CHUNK / 2, cz * CHUNK + CHUNK / 2);
    if ha_shared::geography::settlement_covering(body, centro.0, centro.1).is_none() {
        return;
    }
    for lz in 0..CHUNK {
        for lx in 0..CHUNK {
            let (tx, tz) = (cx * CHUNK + lx, cz * CHUNK + lz);
            let t = |dx: i64, dz: i64| tiles.tile_at(body, tx + dx, tz + dz);
            // Esquina noroeste: vereda con el agua en diagonal y sin agua al este ni al sur, y la esquina opuesta del marco.
            if t(0, 0) != Tile::Pavement || t(1, 1) != Tile::Water || t(1, 0) == Tile::Water || t(0, 1) == Tile::Water {
                continue;
            }
            if t(5, 3) != Tile::Pavement || t(4, 2) != Tile::Water || !es_agua_de_pileta(tiles, body, tx + 1, tz + 1) {
                continue;
            }
            let sprite = art.sprite_or_color("agua/pileta_completa", Color::srgb(0.2, 0.55, 0.65), Vec2::new(6.55 * TILE_M, 4.55 * TILE_M));
            let r = render_xy((tx as f32 + 3.0) * TILE_M, (tz as f32 + 2.0) * TILE_M);
            commands.spawn((
                SurfaceDecor { cx, cz, z, small: true },
                TileArt,
                sprite,
                Transform::from_xyz(r.x, r.y, crate::layers::FLOOR + 0.03),
            ));
        }
    }
}

/// Slot de la línea pintada de la calzada. Ver `spawn_chunk_lane_marks`.
const LANE_MARK_ART: &str = "road/linea";
/// Slot del cordón de la vereda, en poses de autotile. Ver `spawn_chunk_cordon`.
const CORDON_ART: &str = "road/cordon";
/// La pose del cordón que trae la REJILLA de la cuneta. Sólo existe para
/// `edge_n` —es el único borde donde se ve la CARA del cordón— y entra con
/// periodo, no en toda celda: un sumidero cada tanto es una cuneta, uno cada
/// 95 cm es lo que el juego tuvo por error durante meses (ver
/// `scripts/import-calles.py::rejilla_de`).
const CORDON_REJILLA: &str = "edge_n_rejilla";

/// Slots de la senda peatonal (la que cruza una calle norte-sur y la otra). Sin
/// arte asignado los dibuja `procedural_art::zebra`, así que la esquina tiene
/// paso de peatones desde el primer día.
const SENDA_NS_ART: &str = "road/senda_ns";
const SENDA_EW_ART: &str = "road/senda_ew";

/// Regeneración de césped (pedido #2): las celdas de tierra expuesta (cavar,
/// demoler un piso, tierra colocada) sobre césped natural vuelven a pasto tras
/// 45–120 s. Al expirar se BORRA el edit → el terreno natural (Grass) reaparece
/// solo, con rebuild de mesh + autotile vía `edit_log`. Barato: corre cada 2 s
/// y sólo recorre la lista pendiente (vacía casi siempre).
fn tick_grass_regrowth(time: Res<Time>, mut acc: Local<f32>, mut tiles: ResMut<TileWorld>) {
    *acc += time.delta_secs();
    if *acc < 2.0 {
        return;
    }
    let step = *acc;
    *acc = 0.0;
    if tiles.regrow.is_empty() {
        return;
    }
    let mut changed = false;
    let mut i = 0;
    while i < tiles.regrow.len() {
        tiles.regrow[i].timer -= step;
        if tiles.regrow[i].timer > 0.0 {
            i += 1;
            continue;
        }
        let RegrowCell { body, tx, tz, .. } = tiles.regrow.swap_remove(i);
        let key = (body, 0, tx, tz);
        // Sólo si sigue siendo tierra expuesta (el jugador pudo construir ahí).
        if tiles.edits.get(&key) == Some(&Tile::Dirt) {
            tiles.edits.remove(&key);
            tiles.edit_log.push((tx, tz));
            changed = true;
        }
    }
    if changed {
        tiles.version = tiles.version.wrapping_add(1);
        tiles.dirty = true;
    }
}

/// Marca los sprites de arte de tile (autotile/puertas) para poder re-spawnearlos
/// cuando una edición cambia la forma de los vecinos — sin tocar el resto del
/// decor del chunk (árboles, portales, depósitos).
#[derive(Component)]
pub struct TileArt;

/// El pedido explícito del usuario: al colocar/cavar una pieza, los 8 vecinos
/// deben re-elegir su variante autotile (borde/esquina) — si no, quedan
/// desalineados. Drena `TileWorld::edit_log` y re-spawnea el arte de los chunks
/// tocados (una edición en el borde de un chunk afecta al de al lado: se expande
/// la celda ±1 antes de mapear a chunks). Costo cero sin ediciones.
fn rebuild_edited_tile_art(
    mut commands: Commands,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    mut tiles: ResMut<TileWorld>,
    art: Res<ArtLibrary>,
    tile_art: Query<(Entity, &SurfaceDecor), With<TileArt>>,
) {
    if tiles.edit_log.is_empty() {
        return;
    }
    let edits = std::mem::take(&mut tiles.edit_log);
    if surface_region_active(&region).is_none() {
        return;
    }
    let body = active_surface_body(&ctx, &planet);
    let z = tiles.z;
    // Chunks afectados = celdas editadas ±1 (los vecinos cruzan bordes de chunk).
    let mut dirty: HashSet<(i64, i64)> = HashSet::new();
    for (tx, tz) in edits {
        for dx in -1..=1i64 {
            for dz in -1..=1i64 {
                dirty.insert(((tx + dx).div_euclid(CHUNK), (tz + dz).div_euclid(CHUNK)));
            }
        }
    }
    for (e, d) in &tile_art {
        if d.z == z && dirty.contains(&(d.cx, d.cz)) {
            commands.entity(e).despawn_recursive();
        }
    }
    // Re-spawnea el arte de cada chunk tocado (incluye chunks que aún no tenían
    // arte: p.ej. la primera pieza colocada en un chunk sin agua).
    for (cx, cz) in dirty {
        spawn_chunk_tile_art(&mut commands, &art, &tiles, body, z, cx, cz);
    }
}

// ---------------------------------------------------------------------------
// NPCs / colonos — sprites idle en la base lunar, uno por colono del sim
// abstracto (`colony.state.colonists`). Usan el MISMO esquema de poses que el
// jugador (`npc/colonist` → "idle_down"); sin arte caen a una cápsula de color.
// ---------------------------------------------------------------------------

// Reemplazados por `society.rs` (F6): NPCs con cerebro + memoria. Se conservan por si
// hace falta el sprite idle simple; hoy no se schedulean.
#[allow(dead_code)]
#[derive(Component)]
pub struct Colonist;

/// Sprite idle de un colono (o cápsula si no hay arte).
#[allow(dead_code)]
fn colonist_sprite(art: &ArtLibrary) -> (Sprite, Option<crate::art2d::ArtAnimated>) {
    let size = Vec2::new(1.0, 1.6);
    let pose_name = ["idle_down", "idle"]
        .into_iter()
        .find(|n| art.has_pose("npc/colonist", n));
    if let Some((rect_px, anim)) = pose_name.and_then(|n| art.pose("npc/colonist", n)) {
        // La hoja puede ser la del slot o la propia de esa pose (colono armado
        // con un PNG por dirección).
        if let Some(image) = pose_name.and_then(|n| art.pose_image("npc/colonist", n)) {
            let mut s = Sprite::from_image(image);
            s.custom_size = Some(size);
            if anim.is_none() {
                let [x, y, w, h] = rect_px;
                s.rect = Some(Rect::from_corners(
                    Vec2::new(x as f32, y as f32),
                    Vec2::new((x + w) as f32, (y + h) as f32),
                ));
            }
            return (s, anim);
        }
    }
    if let Some((mut s, anim)) = art.world_sprite("npc/colonist", size) {
        s.custom_size = Some(size);
        return (s, anim);
    }
    (Sprite::from_color(Color::srgb(0.85, 0.82, 0.75), Vec2::new(0.7, 1.4)), None)
}

/// Mantiene un sprite por colono junto a los hábitats de la Luna, sólo cuando
/// estás en la superficie lunar. Respawnea cuando cambia la cuenta o el cuerpo.
#[allow(dead_code)]
fn sync_colonists(
    mut commands: Commands,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    colony: Res<crate::colony::ColonyRuntime>,
    machines: Res<crate::machines::MachineWorld>,
    art: Res<ArtLibrary>,
    existing: Query<Entity, With<Colonist>>,
) {
    let on_moon = surface_region_active(&region).is_some()
        && active_surface_body(&ctx, &planet) == CelestialBody::Moon
        && tiles.z == 0;
    let want = if on_moon { colony.state.colonists } else { 0 };
    let have = existing.iter().count() as u32;
    if want == have {
        return;
    }
    for e in &existing {
        commands.entity(e).despawn_recursive();
    }
    if want == 0 {
        return;
    }
    // Anclas = hábitats en la Luna (o el origen si aún no hay).
    let anchors: Vec<(f32, f32)> = machines
        .on_body(CelestialBody::Moon)
        .filter(|m| m.machine_id == ha_shared::HABITAT.id)
        .map(|m| (m.x, m.z))
        .collect();
    let n_anchor = anchors.len().max(1) as u32;
    for i in 0..want {
        let (bx, bz) = if anchors.is_empty() {
            (0.0, 0.0)
        } else {
            anchors[(i % n_anchor) as usize]
        };
        // Cluster en espiral (ángulo áureo) alrededor del hábitat.
        let ring = (i / n_anchor) as f32;
        let ang = i as f32 * 2.399_963_2;
        let rad = 1.5 + ring * 1.2;
        let wx = bx + ang.cos() * rad;
        let wz = bz + 1.0 + ang.sin() * rad;
        let r = render_xy(wx, wz);
        let (sprite, anim) = colonist_sprite(&art);
        let mut ent = commands.spawn((Colonist, sprite, Transform::from_xyz(r.x, r.y, sort_z(wz))));
        if let Some(a) = anim {
            ent.insert(a);
        }
    }
}

// ---------------------------------------------------------------------------
// Persistence (B10)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Default)]
struct TilesSave {
    /// (body, z, tx, tz, kind name)
    edits: Vec<(u8, i32, i64, i64, String)>,
}

fn tiles_path() -> std::path::PathBuf {
    persist_dir().join("tiles.json")
}

fn load_tiles_on_startup(mut tiles: ResMut<TileWorld>) {
    // **Cada partida arranca con lo SUYO.** Esto corría en `Startup`, antes de
    // elegir partida: leía la carpeta base y lo de las pruebas viejas aparecía en
    // cualquier partida nueva. Ahora corre al entrar a la partida (`Loading`) y
    // parte de cero; ver `partida.rs`.
    // Los pozos, la tierra picada y los bloques son de la partida: se vacían.
    // `z` vuelve a la superficie (una partida nueva no arranca en un sótano).
    tiles.edits.clear();
    tiles.removed_floors.clear();
    tiles.edit_log.clear();
    tiles.regrow.clear();
    tiles.dirty = false;
    tiles.z = 0;
    tiles.version = tiles.version.wrapping_add(1);
    let Ok(raw) = std::fs::read_to_string(tiles_path()) else {
        return;
    };
    let Ok(save) = serde_json::from_str::<TilesSave>(&raw) else {
        return;
    };
    for (body, z, tx, tz, name) in save.edits {
        tiles.edits.insert((body, z, tx, tz), tile_from_name(&name));
    }
    // Re-programar el rebrote de césped para la tierra expuesta que quedó
    // guardada de sesiones anteriores (el timer no se persiste — se reinicia).
    let pending: Vec<(u8, i64, i64)> = tiles
        .edits
        .iter()
        .filter(|((_, z, _, _), t)| *z == 0 && **t == Tile::Dirt)
        .map(|((b, _, tx, tz), _)| (*b, *tx, *tz))
        .collect();
    for (b, tx, tz) in pending {
        if let Some(body) = CelestialBody::from_u8(b) {
            tiles.maybe_schedule_regrow(body, tx, tz);
        }
    }
    tiles.version = tiles.version.wrapping_add(1);
}

fn debounced_tiles_save(time: Res<Time>, mut tiles: ResMut<TileWorld>) {
    let paused = is_app_paused();
    let flush_now = paused && !tiles.was_paused && tiles.dirty;
    tiles.was_paused = paused;
    if !tiles.dirty {
        return;
    }
    tiles.debounce += time.delta_secs();
    if !flush_now && tiles.debounce < 20.0 {
        return;
    }
    tiles.debounce = 0.0;
    tiles.dirty = false;
    let save = TilesSave {
        edits: tiles
            .edits
            .iter()
            .map(|((b, z, tx, tz), t)| (*b, *z, *tx, *tz, tile_name(*t).to_string()))
            .collect(),
    };
    if let Ok(json) = serde_json::to_string(&save) {
        let _ = std::fs::write(tiles_path(), json);
    }
}

#[cfg(test)]
mod streaming_tests {
    use super::*;

    /// El mismo cálculo que hace `stream_tile_chunks`, aislado para poder
    /// afirmar la propiedad que importa.
    fn ranges(live_view_h: f32) -> (i64, i64) {
        let need_m = live_view_h * 1.25;
        let visible =
            ((need_m / (CHUNK as f32 * TILE_M)).ceil() as i64).clamp(CHUNK_RANGE, MAX_CHUNK_RANGE);
        (visible, (visible + BUILD_MARGIN).min(MAX_CHUNK_RANGE))
    }

    /// **El "titila verde".** Caminando y en vuelo bajo —que es donde el jugador
    /// mira el suelo de cerca— el anillo que se CONSTRUYE tiene que ser
    /// estrictamente mayor que el que se VE, o se ven nacer los rectángulos de
    /// suelo en el borde de la pantalla.
    ///
    /// Arriba del tope (`MAX_CHUNK_RANGE`, a partir de ~77 m de vista) el anillo
    /// ya es más chico que la pantalla A PROPÓSITO: ahí el borde no se ve porque
    /// el detalle fino se está fundiendo contra el satelital
    /// (`universe::FINE_FADE_VIEW_H` = 95 m). Ese caso lo cubre el test de abajo.
    #[test]
    fn el_suelo_se_arma_fuera_de_cuadro() {
        for view_h in [12.0, 24.0, 30.0, 48.0, 70.0] {
            let (visible, build) = ranges(view_h);
            assert!(
                visible < MAX_CHUNK_RANGE,
                "view_h={view_h} ya toca el tope: no es un caso de caminata"
            );
            assert!(
                build > visible,
                "con view_h={view_h} se construye hasta {build} y se ve hasta {visible}: \
                 el chunk nace a la vista"
            );
        }
    }

    /// El tope duro de memoria manda por encima del margen: volando con el zoom
    /// al máximo no se pasa de `MAX_CHUNK_RANGE` (ahí ya manda el satelital).
    #[test]
    fn el_margen_no_rompe_el_tope() {
        let (_, build) = ranges(10_000.0);
        assert_eq!(build, MAX_CHUNK_RANGE);
    }
}

#[cfg(test)]
mod layer_tests {
    use super::*;

    /// **Por el hueco de un piso alto se ve el suelo de abajo, apagado.**
    ///
    /// En `z > 0` todo lo que no construiste es `Tile::Air`, y el color de `Air`
    /// es negro. Pintado tal cual, el segundo piso quedaba flotando en la nada:
    /// no había forma de saber si el borde de tu losa daba al patio o a la calle,
    /// y el hueco se leía como "acá no hay mundo" en vez de "acá hay una caída".
    #[test]
    fn el_hueco_del_piso_alto_muestra_el_suelo_de_abajo() {
        let body = CelestialBody::Earth;
        let mut tiles = TileWorld::default();
        let (tx, tz) = (120, 120);
        let suelo = tile_render_color(&tiles, body, tx, tz, TileShade::Coarse);
        tiles.z = 1;
        assert_eq!(tiles.tile_at(body, tx, tz), Tile::Air, "arriba tiene que haber vacio");
        let hueco = tile_render_color(&tiles, body, tx, tz, TileShade::Coarse);

        let brillo = |c: [f32; 3]| c[0] + c[1] + c[2];
        assert!(brillo(hueco) > 0.0, "el vacio no puede pintarse negro");
        assert!(
            brillo(hueco) < brillo(suelo),
            "el suelo de abajo tiene que verse ATENUADO: {hueco:?} vs {suelo:?}"
        );
        // Es el MISMO suelo, sólo más oscuro: si cambiara de tinte, el jugador no
        // reconocería la calle sobre la que está construyendo.
        for i in 0..3 {
            let esperado = suelo[i] * VOID_DIM;
            assert!((hueco[i] - esperado).abs() < 1e-4, "canal {i}: {hueco:?}");
        }
    }
}

/// **Voxel 2D: al sacar un bloque queda la cavidad.** Ver `TileWorld::dig_down`
/// y biblia §5.
#[cfg(test)]
mod dig_tests {
    use super::*;

    /// Una celda de superficie de tierra firme con roca maciza debajo: el caso
    /// que obliga a tallar la cavidad, porque caer ahí sin tallarla sería caer
    /// DENTRO de la piedra.
    fn tierra_firme() -> (i64, i64) {
        let body = CelestialBody::Earth;
        (0..4000i64)
            .flat_map(|x| (0..40i64).map(move |z| (x, z)))
            .find(|&(x, z)| {
                ha_shared::topdown::topdown_tile(body, 0, x, z).diggable_down()
                    && ha_shared::topdown::topdown_tile(body, -1, x, z).solid()
            })
            .expect("hay tierra firme con roca debajo cerca del origen")
    }

    /// El corazón del asunto: cavar deja el HUECO donde estabas y la CAVIDAD
    /// abierta abajo. Sin la cavidad, el pozo te dejaría dentro de la roca; sin
    /// el hueco, no habría por dónde bajar y esto sería un mapa plano otra vez.
    #[test]
    fn cavar_deja_el_hueco_y_abre_la_cavidad() {
        let body = CelestialBody::Earth;
        let (tx, tz) = tierra_firme();
        let mut tiles = TileWorld::default();
        let sacado = tiles.dig_down(body, tx, tz).expect("la tierra se cava");

        assert!(sacado.diggable_down(), "se sacó el suelo, no otra cosa: {sacado:?}");
        assert_eq!(tiles.tile_at(body, tx, tz), Tile::Air, "arriba queda el pozo");
        let abajo = tiles.tile_at_level(body, -1, tx, tz);
        assert!(!abajo.solid(), "la cavidad tiene que quedar ABIERTA, no maciza: {abajo:?}");
    }

    /// El pozo se VE: por el hueco se lee el fondo de la cueva, atenuado. Es la
    /// misma regla del vacío del piso alto, aplicada hacia abajo — sin esto el
    /// agujero se pinta negro y no se distingue de "acá no hay mundo".
    #[test]
    fn el_pozo_deja_ver_la_cueva_de_abajo() {
        let body = CelestialBody::Earth;
        let (tx, tz) = tierra_firme();
        let mut tiles = TileWorld::default();
        tiles.dig_down(body, tx, tz);
        let hueco = tile_render_color(&tiles, body, tx, tz, TileShade::Coarse);
        let brillo = |c: [f32; 3]| c[0] + c[1] + c[2];
        assert!(brillo(hueco) > 0.0, "el pozo no puede pintarse negro");

        // Es el fondo del pozo, atenuado: mismo tinte, menos luz. La referencia
        // se pinta a plena luz poniendo ese mismo tile ARRIBA, en superficie: así
        // se compara "lo de abajo" contra "lo mismo pero iluminado".
        let mut fondo = TileWorld::default();
        fondo.author_tile(body, 0, tx, tz, Tile::CaveFloor);
        let claro = tile_render_color(&fondo, body, tx, tz, TileShade::Coarse);
        for i in 0..3 {
            assert!((hueco[i] - claro[i] * VOID_DIM).abs() < 1e-4, "canal {i}: {hueco:?}");
        }
    }

    /// **Bajo tierra se ve el SUBSUELO.**
    ///
    /// Reemplaza a `bajo_tierra_se_sigue_viendo_la_superficie` (7 ago 2026). Aquel
    /// test defendía una idea buena —no cambiar de escena— con el mecanismo
    /// equivocado: pintar la superficie a plena luz alrededor de tu túnel. En la
    /// pantalla eso no se leía como "estoy debajo de mi casa"; se leía como un
    /// mapa roto, con pasto, árboles y mariposas rodeando un agujero negro. El
    /// jugador lo rechazó con todas las letras.
    ///
    /// La continuidad **no la daba el dibujo** y sigue intacta sin él: se baja
    /// andando cuatro escalones (`escaleras.rs`), la luz se apaga en gradiente
    /// (`interior.rs`) y nadie se teletransporta. Lo que cambia es que ahora el
    /// dibujo dice la verdad: tierra maciza hasta el horizonte, y recortado en
    /// ella lo que se excavó.
    #[test]
    fn bajo_tierra_se_ve_el_subsuelo_y_no_el_pasto() {
        let body = CelestialBody::Earth;
        let (tx, tz) = tierra_firme();
        let mut tiles = TileWorld::default();
        let arriba = tiles.tile_at_level(body, 0, tx + 1, tz);
        assert!(arriba.buildable_ground(), "la referencia era superficie pisable");
        tiles.dig_down(body, tx, tz);
        tiles.z = -1; // te caíste a tu propia galería

        let al_lado = tiles.render_tile(body, tx + 1, tz);
        assert_ne!(al_lado, arriba, "al lado seguía dibujándose el pasto de arriba");
        assert!(al_lado.solid(), "el subsuelo es macizo: {al_lado:?}");
        assert_eq!(al_lado, Tile::Subsoil, "y es TIERRA, no roca gris");
        assert!(!tiles.renders_as_pit(body, tx + 1, tz), "el macizo no es un hueco");

        // Tu cavidad NO es un pozo: es el suelo que pisás. Dibujarla como agujero
        // negro es lo que convertía la cueva en una mancha de cuadros negros.
        assert!(!tiles.renders_as_pit(body, tx, tz), "tu galería es SUELO, no un pozo");
        assert!(!tiles.walk_blocked(body, tx, tz), "y se camina");
        // Y se ve más clara que el macizo que la rodea: sin luz ni arte, el
        // contraste solo ya dice dónde se puede andar.
        let brillo = |c: [f32; 3]| c[0] + c[1] + c[2];
        let hueco = tile_render_color(&tiles, body, tx, tz, TileShade::Coarse);
        let macizo = tile_render_color(&tiles, body, tx + 1, tz, TileShade::Coarse);
        assert!(brillo(hueco) > brillo(macizo), "{hueco:?} vs {macizo:?}");
    }

    /// **Desde la azotea se ve la calle.** El otro lado de la misma regla: por el
    /// vacío del borde de un piso alto se ve el mundo de abajo, atenuado — y lo
    /// que se ve es de la SUPERFICIE (nivel 0), que es lo que le permite al decor
    /// plantar ahí los árboles y los coches apagados en vez de nada.
    #[test]
    fn desde_un_piso_alto_lo_que_se_ve_es_la_superficie() {
        let body = CelestialBody::Earth;
        let (tx, tz) = tierra_firme();
        let mut tiles = TileWorld::default();
        let calle = tiles.tile_at_level(body, 0, tx, tz);
        tiles.z = 2;
        let (suelo, nivel, dim) = suelo_a_la_vista(&tiles, body, tx, tz);
        assert_eq!(suelo, calle, "por el vacío se ve el suelo de la calle");
        assert_eq!(nivel, 0, "y sale del nivel de la superficie");
        assert!(dim < 1.0, "atenuado: está abajo, no al lado");
    }

    /// Contra el lecho de roca no se cava, y el líquido se rellena, no se pica:
    /// las dos formas de que un pozo dejara al jugador en un sitio imposible.
    #[test]
    fn ni_el_lecho_de_roca_ni_el_agua_se_cavan() {
        let body = CelestialBody::Earth;
        let (tx, tz) = tierra_firme();
        let mut tiles = TileWorld::default();
        tiles.z = MIN_Z_LEVEL + 1;
        tiles.author_tile(body, tiles.z, tx, tz, Tile::CaveFloor);
        assert!(tiles.dig_down(body, tx, tz).is_none(), "el fondo de la mina no se cava");

        let mut agua = TileWorld::default();
        agua.author_tile(body, 0, tx, tz, Tile::Water);
        assert!(agua.dig_down(body, tx, tz).is_none(), "el bajío no se cava");
        agua.author_tile(body, 0, tx, tz, Tile::WaterDeep);
        assert!(agua.dig_down(body, tx, tz).is_none(), "el fondo hondo tampoco");
    }
}

#[cfg(test)]
mod collision_tests {
    use super::*;
    use ha_assets::CollisionFootprint;
    use ha_shared::world_edits::PlacedDecor;

    /// Una celda de superficie donde el mundo NO genera nada, para probar sólo lo
    /// que colocamos a mano.
    fn empty_cell() -> (i64, i64) {
        (0..2000i64)
            .flat_map(|x| (0..40i64).map(move |z| (x, z)))
            .find(|&(x, z)| surface_decor_at(CelestialBody::Earth, x, z).is_none())
            .expect("hay celdas vacías cerca del origen")
    }

    /// **Los COSTADOS de una terraza se oscurecen.** Las caras que se ven de
    /// canto: una línea de sombra y nada más.
    ///
    /// La cara que MIRA AL SUR ya no está acá — la resuelve
    /// [`sombra_de_muro_en_la_columna`], porque hace falta saber dónde termina el
    /// dibujo del muro y eso no se contesta mirando al vecino. Ver el test de al
    /// lado.
    #[test]
    fn the_foot_of_a_cliff_gets_a_shadow() {
        use ha_shared::relief::relief_level;
        let body = CelestialBody::Earth;
        let mut shadowed = 0;
        let mut plain_ok = 0;
        for tx in -1_500..1_500 {
            for tz in -6..6 {
                let l = relief_level(body, tx, tz);
                if l < 0 {
                    // El agua nunca recibe sombra de terraza.
                    assert_eq!(terrace_shadow(body, tx, tz, l), 1.0, "sombra sobre agua");
                    continue;
                }
                let de_canto = [(-1i64, 0i64), (1, 0), (0, 1)]
                    .into_iter()
                    .any(|(dx, dz)| relief_level(body, tx + dx, tz + dz) > l);
                let s = terrace_shadow(body, tx, tz, l);
                if de_canto {
                    assert!(s < 1.0, "el costado de la terraza en ({tx},{tz}) no tiene sombra");
                    assert!(s >= SOMBRA_PISO, "sombra demasiado negra: {s}");
                    shadowed += 1;
                } else {
                    assert_eq!(s, 1.0, "sombra en campo abierto en ({tx},{tz})");
                    plain_ok += 1;
                }
            }
        }
        assert!(shadowed > 50, "apenas {shadowed} celdas al costado de una terraza");
        assert!(plain_ok > 1_000, "casi todo el mundo salió sombreado: {plain_ok}");
    }

    /// **LA SOMBRA DEL MURO CAE SOBRE EL SUELO QUE SE VE, NO SOBRE LA ROCA.**
    ///
    /// Es el bug que hacía que el mundo no se leyera apilado: la franja oscura se
    /// pintaba en las celdas pegadas al canto, que son justo las que el dibujo del
    /// acantilado TAPA ([`ha_shared::relief::ReliefCell::Pared`]). En pantalla,
    /// dos praderas del mismo verde con una banda de roca en medio.
    ///
    /// Se comprueba lo que de verdad importa: que la primera fila de terraza
    /// visible bajo un muro esté oscurecida, y que ninguna celda de roca cargue con
    /// sombra que nadie va a ver.
    #[test]
    fn la_sombra_del_muro_cae_donde_se_ve() {
        use ha_shared::relief::{alto_de_pared, relief_cell, relief_level, ReliefCell};
        let body = CelestialBody::Earth;
        let mut probados = 0;
        // Ventanas con relieve de verdad: la sierra reparte paredes de 2 a 10.
        for (cx, cz) in [(1_700i64, -5_000i64), (0, 0), (2_400, -1_800)] {
            for lx in 0..64 {
                let tx = cx + lx;
                let z0 = cz - (cz.rem_euclid(CHUNK));
                let sombra = sombra_de_muro_en_la_columna(body, tx, z0);
                for lz in 0..CHUNK {
                    let tz = z0 + lz;
                    let k = sombra[lz as usize];
                    if k >= 1.0 {
                        continue;
                    }
                    probados += 1;
                    assert!(k >= SOMBRA_PISO, "sombra demasiado negra en ({tx},{tz}): {k}");
                    // La celda sombreada se VE: nunca es cuerpo de pared.
                    assert_ne!(
                        relief_cell(body, tx, tz),
                        ReliefCell::Pared,
                        "sombra pintada dentro de la roca en ({tx},{tz})"
                    );
                    // Y hay un muro encima, a menos de SOMBRA_FILAS del pie.
                    let hay_muro = (1..=ha_shared::relief::MAX_PARED + 1 + SOMBRA_FILAS).any(|k| {
                        let alto = alto_de_pared(body, tx, tz - k);
                        alto >= 1 && (alto + 2..alto + 2 + SOMBRA_FILAS).contains(&k)
                    });
                    assert!(hay_muro, "sombra sin muro que la tire en ({tx},{tz})");
                    // Y está sobre terreno, no sobre líquido.
                    assert!(relief_level(body, tx, tz) >= 0, "sombra sobre agua en ({tx},{tz})");
                }
            }
        }
        assert!(probados > 100, "apenas {probados} celdas con sombra de muro");
    }

    fn authored(key: &str, x: i64, z: i64, size: Option<[f32; 2]>) -> AuthoredDecor {
        let mut a = AuthoredDecor::default();
        a.set(vec![PlacedDecor {
            body: CelestialBody::Earth as u8,
            z: 0,
            x,
            zc: z,
            key: key.to_string(),
            size,
            ground: None,
            cells: None,
            anchor: None,
        }]);
        a
    }

    /// Caja del jugador centrada en un punto del mundo.
    fn player_at(x: f32, z: f32) -> [f32; 4] {
        let r = ha_shared::topdown::PLAYER_RADIUS;
        [x - r, z - r, x + r, z + r]
    }

    #[test]
    fn subtile_footprint_only_blocks_where_the_sprite_is() {
        let (tx, tz) = empty_cell();
        // Poste de farol: media celda de ancho, apoyado en el suelo (res 4).
        let fp = CollisionFootprint {
            w: 1,
            h: 2,
            ax: 0,
            az: 1,
            solid: vec![(1, 6), (2, 6), (1, 7), (2, 7)],
            step: Vec::new(), res: 4,
        };
        let art = crate::art2d::ArtLibrary::test_footprint("prop/farol", fp);
        let dec = authored("prop/farol", tx, tz, None);
        let c = (tx as f32 + 0.5, tz as f32 + 0.5);
        // Encima del poste: frena.
        assert!(decor_hits_box(&art, Some(&dec), None, CelestialBody::Earth, 0, player_at(c.0, c.1)));
        // Rozando por el borde de la celda (el jugador entra en ella pero no
        // toca el poste): pasás. Con colisión por TILE esto frenaba y se sentía
        // un bug.
        let roce = player_at(tx as f32 - 0.3, c.1);
        assert!(roce[2] > tx as f32, "el jugador sí invade la celda del farol");
        assert!(!decor_hits_box(&art, Some(&dec), None, CelestialBody::Earth, 0, roce));
        // Una celda más arriba (donde está el farol dibujado, no su base): pasás.
        assert!(!decor_hits_box(
            &art,
            Some(&dec),
            None,
            CelestialBody::Earth,
            0,
            player_at(c.0, tz as f32 - 0.6)
        ));
    }

    #[test]
    fn authored_props_without_a_footprint_do_not_block() {
        let (tx, tz) = empty_cell();
        let art = crate::art2d::ArtLibrary::default();
        let dec = authored("nature/flor", tx, tz, None);
        let c = (tx as f32 + 0.5, tz as f32 + 0.5);
        assert!(!decor_hits_box(&art, Some(&dec), None, CelestialBody::Earth, 0, player_at(c.0, c.1)));
        // Con la casilla "bloquea el paso" marcada sí frena, en su celda.
        let art = crate::art2d::ArtLibrary::test_collides("nature/flor", true);
        assert!(decor_hits_box(&art, Some(&dec), None, CelestialBody::Earth, 0, player_at(c.0, c.1)));
    }

    #[test]
    fn scaling_an_asset_scales_its_collision() {
        let (tx, tz) = empty_cell();
        // Huella de 1 tile completo sobre un asset de 1×1.
        let fp = CollisionFootprint { w: 1, h: 1, ax: 0, az: 0, solid: vec![(0, 0)], step: Vec::new(), res: 1 };
        let art = crate::art2d::ArtLibrary::test_footprint("prop/caja", fp);
        let big = authored("prop/caja", tx, tz, Some([3.0, 3.0]));
        // A 3× la caja alcanza la celda de al lado; a 1× no.
        let side = (tx as f32 + 1.6, tz as f32 + 0.5);
        assert!(decor_hits_box(&art, Some(&big), None, CelestialBody::Earth, 0, player_at(side.0, side.1)));
        let small = authored("prop/caja", tx, tz, Some([1.0, 1.0]));
        assert!(!decor_hits_box(&art, Some(&small), None, CelestialBody::Earth, 0, player_at(side.0, side.1)));
    }
}

#[cfg(test)]
mod pasto_tests {
    use super::*;

    /// Un slot al que el juego le pide pixel y que el editor no lista es un
    /// slot que nadie puede llenar: el pasto de sabana se quedaría vacío para
    /// siempre y nadie sabría por qué. Es la misma regla que ya cuidan
    /// `every_slot_has_a_human_label` y `slot_audit` para el resto del catálogo.
    #[test]
    fn todo_pasto_de_bioma_tiene_slot_con_nombre() {
        let slots = ha_assets::game_slots();
        for clave in PASTOS_DE_BIOMA {
            let s = slots
                .iter()
                .find(|s| s.key == *clave)
                .unwrap_or_else(|| panic!("`{clave}` no está en el catálogo de slots"));
            assert!(!s.label.is_empty(), "`{clave}` sin nombre humano");
        }
    }

    /// **Los cuatro climas que crían pasto piden losa propia, y ninguno más.**
    ///
    /// El resto de los biomas no está por una razón concreta y no por olvido: la
    /// playa y el desierto son arena, el océano es agua. Si alguno de ésos
    /// empezara a pedir pasto sería que `biome_tile` cambió de opinión, y el
    /// mundo se llenaría de pradera sobre la arena.
    ///
    /// **La montaña sí pide, y el de la sierra**: son la misma pastura de altura
    /// (`geography::pasto_de`). Darle una losa por grada era una losa por escalón.
    #[test]
    fn cada_clima_que_cria_pasto_pide_su_losa() {
        use ha_shared::geography::Biome;
        let body = CelestialBody::Earth;
        for (bioma, esperado) in [
            (Biome::Forest, Some("tiles/pasto_bosque")),
            (Biome::Savanna, Some("tiles/pasto_seco")),
            (Biome::Swamp, Some("tiles/pasto_pantano")),
            (Biome::Highland, Some("tiles/pasto_sierra")),
            (Biome::Mountain, Some("tiles/pasto_sierra")),
            (Biome::Grassland, None),
            (Biome::Desert, None),
        ] {
            // Se busca una celda REAL de ese bioma en vez de inyectarlo: así el
            // test también caza que el bioma haya dejado de existir en el mundo.
            // **En llano**: sobre relieve manda la terraza y la respuesta puede no
            // ser la de `biome_at`, que es justamente lo que arregló el anillo.
            let Some((tx, tz)) = (0..3000i64)
                .flat_map(|x| (0..300i64).map(move |z| (x * 7, z * 7)))
                .find(|&(x, z)| {
                    ha_shared::geography::biome_at(body, x, z) == bioma
                        && ha_shared::relief::relief_cell(body, x, z)
                            == ha_shared::relief::ReliefCell::Flat
                })
            else {
                continue; // ese bioma no cae cerca del origen; no es lo que se prueba
            };
            assert_eq!(
                pasto_de_la_celda(body, tx, tz),
                esperado,
                "{bioma:?} en ({tx},{tz})"
            );
        }
    }

    /// **Sin losas de clima cargadas, el suelo cuesta lo que costaba.** Es el
    /// atajo de `ArtLibrary::pasto_por_bioma`: con la biblioteca vacía la malla
    /// no puede preguntar el bioma, así que tampoco puede pagarlo.
    #[test]
    fn sin_arte_de_clima_no_se_pregunta_el_bioma() {
        let art = ArtLibrary::default();
        assert!(!art.pasto_por_bioma());
        assert_eq!(
            suelo_de_la_celda(&art, CelestialBody::Earth, Tile::Grass, 0, 0, 0),
            None,
            "sin hoja de suelo no hay losa: la celda cae al color plano"
        );
    }
}

#[cfg(test)]
mod gris_tests {
    use super::*;

    /// **EL GUARDIÁN DEL ANILLO DE LA MESETA.**
    ///
    /// La pieza del acantilado trae su meseta dibujada y teñida al color CRUDO
    /// de la losa; la malla pinta esa misma losa por `gris_de_celda`, que en el
    /// escalón 9 vale ~1,40. Mientras el sprite salía sin color, borde y relleno
    /// eran dos verdes distintos con la frontera calcada al canto de la terraza
    /// — el bug que reportó el usuario el 16 ago 2026.
    ///
    /// Acá se fijan las dos costuras del muro apilado: arriba tiene que valer
    /// EXACTAMENTE el gris del canto (donde meseta toca meseta) y abajo el del
    /// suelo que pisa.
    #[test]
    fn el_muro_empieza_en_el_gris_del_canto_y_termina_en_el_del_pie() {
        let (canto, pie) = (1.40, 0.72);
        assert_eq!(gris_de_la_fila(canto, pie, 0, 8), canto);
        assert_eq!(gris_de_la_fila(canto, pie, 8, 8), pie);
        // Y en el medio, ni un extremo ni el otro: la cara baja de a poco.
        let medio = gris_de_la_fila(canto, pie, 4, 8);
        assert!(medio < canto && medio > pie, "gris del medio: {medio}");
        // Un muro de una sola fila no divide por cero.
        assert_eq!(gris_de_la_fila(canto, pie, 0, 0), canto);
    }

    /// El tinte de un sprite **puede aclarar**, no sólo apagar: el gris de una
    /// cumbre vale más que 1 y con el corto de `atenuar_sprite` el acantilado se
    /// quedaba en el color crudo mientras la malla de al lado subía.
    #[test]
    fn tenir_aclara_y_atenuar_solo_apaga() {
        let blanco = Sprite::from_color(Color::WHITE, Vec2::ONE);
        let claro = tenir_sprite(blanco.clone(), 1.4).color.to_srgba();
        assert!((claro.red - 1.4).abs() < 1e-5, "no aclaró: {}", claro.red);
        let sin_tocar = atenuar_sprite(blanco, 1.4).color.to_srgba();
        assert!((sin_tocar.red - 1.0).abs() < 1e-5, "atenuar no puede aclarar");
    }
}

#[cfg(test)]
mod orla_tests {
    use super::*;
    use ha_shared::geography::Pasto;

    const PASTOS: [Pasto; 5] =
        [Pasto::Referencia, Pasto::Bosque, Pasto::Seco, Pasto::Pantano, Pasto::Sierra];

    /// **UNA COSTURA, UN SOLO DUEÑO.** El rango tiene que ser un orden TOTAL: con
    /// dos pastos empatados, los dos lados de la frontera dibujarían su fleco, uno
    /// encima del otro, y el mismo metro de mundo tendría dos dibujos peleándose.
    #[test]
    fn una_costura_tiene_un_solo_dueno() {
        for (i, a) in PASTOS.iter().enumerate() {
            for b in &PASTOS[i + 1..] {
                assert_ne!(
                    rango_de_pasto(*a),
                    rango_de_pasto(*b),
                    "{a:?} y {b:?} empatan en rango"
                );
            }
        }
    }

    /// **Toda costura posible tiene su pieza declarada.** Si falta una clave, esa
    /// frontera vuelve a salir a escuadra —el bug de la meseta— y no se entera
    /// nadie: el juego cae al respaldo en silencio.
    #[test]
    fn toda_costura_de_pasto_tiene_pieza() {
        let slots = ha_assets::game_slots();
        for alto in PASTOS {
            let Some(mat) = orilla_de_superficie(ha_shared::geography::Superficie {
                tile: Tile::Grass,
                pasto: alto,
            }) else {
                continue; // el de referencia no dibuja: es el fondo
            };
            for bajo in PASTOS {
                if rango_de_pasto(bajo) >= rango_de_pasto(alto) {
                    continue;
                }
                let clave = match sufijo_de_pasto(bajo) {
                    Some(s) => format!("borde/{mat}_{s}"),
                    None => format!("borde/{mat}"),
                };
                let s = slots
                    .iter()
                    .find(|s| s.key == clave)
                    .unwrap_or_else(|| panic!("falta el slot `{clave}` ({alto:?} contra {bajo:?})"));
                assert!(!s.label.is_empty(), "`{clave}` sin nombre humano");
            }
        }
    }

    /// El pasto de referencia es el FONDO del mundo: nunca dibuja costura, porque
    /// es contra él que orlan todos los demás.
    #[test]
    fn la_pradera_no_orla_a_nadie() {
        assert_eq!(
            orilla_de_superficie(ha_shared::geography::Superficie {
                tile: Tile::Grass,
                pasto: Pasto::Referencia,
            }),
            None
        );
        assert_eq!(rango_de_pasto(Pasto::Referencia), 0);
    }
}

#[cfg(test)]
mod vereda_tests {
    use super::*;

    /// **La pareja viaja junta.** Cada familia de vereda son DOS piezas —la
    /// baldosa que dibuja la malla y el escalón que se pega sobre el asfalto— y
    /// las dos tablas se indexan con el mismo número. Si se desalinearan, una
    /// ciudad tendría el cordón de otra: crema sobre gris azulado, que es
    /// exactamente el motivo por el que durante meses hubo una sola vereda.
    #[test]
    fn cada_baldosa_de_vereda_tiene_su_cordon() {
        assert_eq!(VEREDA_BALDOSA.len(), VEREDA_CORDON.len());
        let slots = ha_assets::game_slots();
        for (i, (baldosa, cordon)) in VEREDA_BALDOSA.iter().zip(VEREDA_CORDON).enumerate() {
            // La familia 6 no llega a publicar cordón (el pack no le dibujó el
            // `edge_n`); su slot igual existe, y el filtro de "pareja completa"
            // de `vereda_de_la_ciudad` la deja fuera en tiempo de ejecución.
            assert!(
                slots.iter().any(|s| s.key == *baldosa),
                "la baldosa {baldosa} (familia {i}) no está en el catálogo"
            );
            assert!(
                slots.iter().any(|s| s.key == cordon) || i == 5,
                "el cordón {cordon} (familia {i}) no está en el catálogo"
            );
        }
    }

    /// Con la biblioteca vacía no hay familias que repartir, así que ni se
    /// pregunta por el asentamiento: se usa la vereda de siempre. Es el atajo
    /// que mantiene el costo del suelo donde estaba.
    #[test]
    fn sin_familias_cargadas_manda_la_vereda_de_siempre() {
        let art = ArtLibrary::default();
        assert!(!art.veredas_de_ciudad());
        assert_eq!(vereda_de_la_ciudad(&art, CelestialBody::Earth, 0, 0), 0);
    }

    /// **Una ciudad, una vereda.** La familia sale del CENTRO del asentamiento,
    /// así que dos celdas cualesquiera del mismo casco tienen que dar la misma —
    /// si dependiera de la celda, la acera cambiaría de material cada tres
    /// metros y el cordón caería sobre la baldosa equivocada.
    #[test]
    fn el_casco_entero_usa_la_misma_familia() {
        use ha_shared::geography::settlement_covering;
        let body = CelestialBody::Earth;
        let Some((tx, tz, s)) = (0..4000i64)
            .flat_map(|x| (0..400i64).map(move |z| (x * 3, z * 3)))
            .find_map(|(x, z)| settlement_covering(body, x, z).map(|s| (x, z, s)))
        else {
            return; // no hay ciudad cerca del origen en esta semilla
        };
        // El mismo cálculo que hace `vereda_de_la_ciudad`, sin depender de que
        // haya arte cargado (en un test no lo hay): lo que se afirma es que la
        // ENTRADA es el centro y no la celda.
        let familia = |cx: i64, cz: i64| {
            settlement_covering(body, cx, cz).map(|s| s.center)
        };
        for (dx, dz) in [(0, 0), (5, 5), (-5, 9), (12, -7)] {
            let (px, pz) = (tx + dx, tz + dz);
            if settlement_covering(body, px, pz).is_none() {
                continue; // esa celda se salió del casco; no es lo que se prueba
            }
            assert_eq!(
                familia(px, pz),
                Some(s.center),
                "({px},{pz}) tendría que seguir siendo la misma ciudad"
            );
        }
    }
}

#[cfg(test)]
mod diagnostico_islas_de_borde {
    use super::*;
    use ha_shared::relief::{alto_de_pared, relief_cell, relief_level, ReliefCell};

    /// **DIAGNÓSTICO, no guardián.** Recorre una ventana real del mundo y para
    /// cada celda de canto (`Cliff` o `Ramp`) imprime la forma que le da
    /// `autotile_shape` —la misma función y los mismos ocho vecinos que usa
    /// `spawn_chunk_tile_art`— junto con si esa forma CUELGA pared
    /// (`CARAS_QUE_CUELGAN`) y cuántos escalones mide (`alto_de_pared`).
    ///
    /// Es la forma de contestar "¿por qué se ven islas en el borde diagonal?"
    /// con datos del mundo de verdad en vez de mirar píxeles: cada fila muestra
    /// la costura entre la celda y la siguiente.
    ///
    /// ```sh
    /// cargo test -p ha-game --lib diagnostico_islas -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn perfil_de_un_canto_diagonal() {
        let body = CelestialBody::Earth;
        // Las mismas ventanas que usa el driver HA_RELIEVETEST: una pared y una
        // rampa reales, no coordenadas inventadas.
        for (nombre, cx, cz) in [("pared", 1763i64, -5087i64), ("rampa", 1796, -5097)] {
            println!("\n=== {nombre} @ ({cx},{cz}) ===");
            for tz in cz - 12..cz + 12 {
                let mut fila = String::new();
                for tx in cx - 12..cx + 12 {
                    let c = relief_cell(body, tx, tz);
                    fila.push(match c {
                        ReliefCell::Flat => '.',
                        ReliefCell::Cliff => 'C',
                        ReliefCell::Pared => '#',
                        ReliefCell::Ramp => 'R',
                    });
                }
                println!("{fila}");
            }
            println!("--- perfil de cada canto (Cliff/Ramp) con su forma ---");
            for tz in cz - 12..cz + 12 {
                for tx in cx - 12..cx + 12 {
                    let c = relief_cell(body, tx, tz);
                    if !matches!(c, ReliefCell::Cliff | ReliefCell::Ramp) {
                        continue;
                    }
                    let l = relief_level(body, tx, tz);
                    let same = |dx: i64, dz: i64| relief_level(body, tx + dx, tz + dz) >= l;
                    let shape = autotile_shape(
                        same(0, -1), same(1, 0), same(0, 1), same(-1, 0),
                        same(1, -1), same(-1, -1), same(1, 1), same(-1, 1),
                    );
                    let cuelga = CARAS_QUE_CUELGAN.contains(&shape);
                    let alto = alto_de_pared(body, tx, tz);
                    println!(
                        "  ({tx},{tz}) nivel={l} {c:?} shape={shape:<10} cuelga={cuelga:5} alto_de_pared={alto}"
                    );
                }
            }
        }
    }
}
