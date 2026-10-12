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
    deposit_at, portal_here, topdown_tile, Deposit, Portal, Tile, MIN_Z_LEVEL, TILE_M,
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
pub struct ChunkBudget(pub usize);

impl Default for ChunkBudget {
    fn default() -> Self {
        Self(CHUNK_BUILD_BUDGET_PER_FRAME)
    }
}

impl ChunkBudget {
    pub fn normal() -> usize {
        CHUNK_BUILD_BUDGET_PER_FRAME
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
}

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
        Tile::Rock => "rock",
        Tile::Regolith => "regolith",
        Tile::Basalt => "basalt",
        Tile::Ice => "ice",
        Tile::RockRed => "rock_red",
        Tile::Stone => "stone",
        Tile::RockDark => "rock_dark",
        Tile::Bedrock => "bedrock",
        Tile::CaveFloor => "cave_floor",
        Tile::WoodFloor => "wood_floor",
        Tile::Wall => "wall",
        Tile::WoodWall => "wood_wall",
        Tile::StoneBrick => "stone_brick",
        Tile::Door => "door",
        Tile::Window => "window",
        Tile::StairsDown => "stairs_down",
        Tile::StairsUp => "stairs_up",
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
        "wall" => Tile::Wall,
        "wood_wall" => Tile::WoodWall,
        "stone_brick" => Tile::StoneBrick,
        "door" => Tile::Door,
        "window" => Tile::Window,
        "stairs_down" => Tile::StairsDown,
        "stairs_up" => Tile::StairsUp,
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
                | Tile::StoneBrick
                | Tile::Pavement
                | Tile::WoodWall
                | Tile::Window
                | Tile::Door
                | Tile::StairsUp
                | Tile::StairsDown
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
    let reach: i64 = art.footprint_reach();
    let x0 = (aabb[0] / TILE_M).floor() as i64 - reach;
    let x1 = (aabb[2] / TILE_M).floor() as i64 + reach;
    let z0 = (aabb[1] / TILE_M).floor() as i64 - reach;
    let z1 = (aabb[3] / TILE_M).floor() as i64 + reach;
    for cz in z0..=z1 {
        for cx in x0..=x1 {
            if decor_cell_hits(art, authored, flora, body, z, cx, cz, aabb) {
                return true;
            }
        }
    }
    false
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
        return art.collides_override(&keys).unwrap_or(false) && cell_hits(cx, cz, aabb);
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
            let rock_key = format!("decor/rock{}", body_art_suffix(body));
            let keys = [rock_key.as_str(), "decor/rock"];
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
    (z == 0 && surface_decor_at(body, tx, tz).is_some()) || deposit_at(body, z, tx, tz).is_some()
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
/// de golpe — un cuarto de segundo de fundido y deja de leerse como un corte.
#[derive(Resource, Default)]
pub struct LayerFade(pub f32);

/// Duración del fundido de capa, en segundos.
const LAYER_FADE_SECS: f32 = 0.28;

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
            .init_resource::<ChunkMaterial>()
            .init_resource::<LayerFade>()
            .init_resource::<AuthoredDecor>()
            .init_resource::<DecorFreeCells>()
            .init_resource::<FineDetailOff>()
            .add_systems(
                Startup,
                (
                    load_tiles_on_startup,
                    init_chunk_material,
                    spawn_cursor_highlight,
                ),
            )
            .add_systems(
                Update,
                (
                    apply_tile_hold.before(crate::hud::update_interact_hud),
                    try_place_tile_on_hold.before(crate::hud::update_interact_hud),
                    layer_transition,
                    fall_through_void.after(layer_transition),
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

/// Corchetes amarillos sobre la celda que estás TOCANDO — el único adorno que
/// quedó del viejo sistema de mira, y sólo mientras el dedo está apoyado: es la
/// confirmación de "acá va a pasar", no un cursor que vive en pantalla.
#[derive(Component)]
struct CursorHighlight;

fn spawn_cursor_highlight(mut commands: Commands) {
    let yellow = Color::srgb(1.0, 0.9, 0.15);
    // Four L-shaped corner brackets on a 1×1 tile (corners at ±0.5).
    commands
        .spawn((
            CursorHighlight,
            Transform::from_xyz(0.0, 0.0, 6.0),
            Visibility::Hidden,
            InheritedVisibility::default(),
            ViewVisibility::default(),
        ))
        .with_children(|p| {
            let arm = 0.28;
            let thick = 0.09;
            for (cx, cz) in [(-1.0, 1.0), (1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
                let corner = Vec2::new(cx * 0.5, cz * 0.5);
                // Horizontal arm (pointing inward).
                p.spawn((
                    Sprite::from_color(yellow, Vec2::new(arm, thick)),
                    Transform::from_xyz(corner.x - cx * arm * 0.5, corner.y, 0.0),
                ));
                // Vertical arm.
                p.spawn((
                    Sprite::from_color(yellow, Vec2::new(thick, arm)),
                    Transform::from_xyz(corner.x, corner.y - cz * arm * 0.5, 0.0),
                ));
            }
        });
}

fn sync_cursor_highlight(
    cursor: Res<crate::input::AimCursor>,
    hold: Res<crate::input::HoldInteractState>,
    region: Res<RegionRuntime>,
    mut hl: Query<(&mut Transform, &mut Visibility), With<CursorHighlight>>,
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
    // Snap to the cell centre under the cursor.
    let tx = cursor.world.x.floor();
    let tz = cursor.world.y.floor();
    let r = render_xy(tx + 0.5, tz + 0.5);
    tf.translation = Vec3::new(r.x, r.y, 6.0);
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
    layer.0 = (layer.0 - time.delta_secs() / LAYER_FADE_SECS).max(0.0);
    let f = crate::universe::fine_tile_alpha(zoom.live_view_h) * (1.0 - layer.0);
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
    if let Some(t) = tiles.break_tile(body, tx as i64, tz as i64) {
        sfx.send(crate::audio::PlaySfxAt(
            crate::audio::Sfx::Dig,
            render_xy((tx as f32 + 0.5) * TILE_M, (tz as f32 + 0.5) * TILE_M),
        ));
        if let Some(item) = t.dug_item() {
            inv.add(item, 1);
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

/// Z-Level transition: standing on a portal cell drops/climbs one floor
/// (a coherent vertical shaft, so `(x,z)` stays — only the layer changes).
/// Latched: re-arms only after the player steps off the portal (so you don't
/// fall through every floor at once). Discrete frame change, never a cutscene.
fn layer_transition(
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    mut tiles: ResMut<TileWorld>,
    mut armed: Local<bool>,
    mut started: Local<bool>,
    mut sfx: EventWriter<crate::audio::PlaySfx>,
    mut fade: ResMut<LayerFade>,
    mut walls: ResMut<crate::walls::BuiltWalls>,
    player: Query<&SimState, With<Player>>,
) {
    if !*started {
        *armed = true;
        *started = true;
    }
    if surface_region_active(&region).is_none() {
        return;
    }
    let Ok(sim) = player.get_single() else {
        return;
    };
    let body = active_surface_body(&ctx, &planet);
    let tx = (sim.body.x / TILE_M).floor() as i64;
    let tz = (sim.body.z / TILE_M).floor() as i64;
    // Portales naturales (mina) O gradas colocadas por el jugador (F6): ambos
    // son el MISMO swap instantáneo de capa, nunca una cutscene.
    let portal = portal_here(body, tiles.z, tx, tz).or_else(|| {
        match tiles.tile_at(body, tx, tz) {
            Tile::StairsDown => Some(Portal::Descend),
            Tile::StairsUp => Some(Portal::Ascend),
            _ => None,
        }
    });
    match portal {
        Some(portal) if *armed => {
            // Verticalidad interior: hasta 3 pisos hacia arriba; hacia abajo,
            // el fondo de la mina.
            let new_z = match portal {
                Portal::Descend => (tiles.z - 1).max(MIN_Z_LEVEL),
                Portal::Ascend => (tiles.z + 1).min(3),
            };
            if new_z != tiles.z {
                let from_stairs = matches!(
                    tiles.tile_at(body, tx, tz),
                    Tile::StairsDown | Tile::StairsUp
                );
                tiles.z = new_z;
                tiles.version = tiles.version.wrapping_add(1);
                // Al llegar por gradas, tallar la sala destino (idempotente) y
                // dejar la grada de vuelta — sensación de subsuelo/piso sin
                // cuevas procedurales ni loading.
                if from_stairs {
                    carve_stair_room(&mut tiles, &mut walls, body, tx, tz, portal);
                }
                fade.0 = 1.0; // el piso nuevo entra fundido, no de golpe
                sfx.send(crate::audio::PlaySfx(crate::audio::Sfx::Stairs));
                *armed = false;
                eprintln!("ha-layer: → z={new_z} ({:?})", portal);
            }
        }
        Some(_) => {}
        None => *armed = true,
    }
}

/// **Del piso alto se puede CAER.** En `z > 0` todo lo que no construiste es
/// vacío (`Tile::Air`), así que salirte de tu losa —o quedarte sin ella porque
/// demoliste el piso donde estabas— te baja un nivel en el acto, en el mismo
/// `(x, z)`. Si abajo también hay vacío seguís bajando hasta pisar algo.
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
    // Volando no se cae: el piso alto es justo por donde entrás volando.
    if tiles.z <= 0 || fly.flying() || surface_region_active(&region).is_none() {
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
    // Bajar hasta pisar algo (o hasta el suelo, que siempre lo hay).
    let mut z = tiles.z;
    while z > 0 && tiles.tile_at_level(body, z, tx, tz) == Tile::Air {
        z -= 1;
    }
    if z == tiles.z {
        return;
    }
    tiles.z = z;
    tiles.version = tiles.version.wrapping_add(1);
    fade.0 = 1.0;
    bob.drop_impact();
    let feet = render_xy(sim.body.x, sim.body.z);
    sfx.send(crate::audio::PlaySfxAt(crate::audio::Sfx::DropLedge, feet));
    shake.send(crate::camera::CameraShakeEvent { trauma: 0.14 });
}

/// Talla una sala cerrada 7×7 alrededor de la celda de llegada de una grada
/// (perímetro = muro, interior = piso, la celda de llegada = grada de vuelta).
/// Idempotente: sólo talla celdas aún naturales (no pisa lo que el jugador
/// construyó); las ediciones persisten en `tiles.json` como cualquier otra.
fn carve_stair_room(
    tiles: &mut TileWorld,
    walls: &mut crate::walls::BuiltWalls,
    body: CelestialBody,
    tx: i64,
    tz: i64,
    took: Portal,
) {
    let z = tiles.z;
    let back = match took {
        Portal::Descend => Tile::StairsUp,
        Portal::Ascend => Tile::StairsDown,
    };
    // Si la grada de vuelta ya está, la sala ya existe.
    if tiles.tile_at(body, tx, tz) == back {
        return;
    }
    // **Subir NO talla una habitación: deja un rellano.**
    //
    // Arriba ya no hay roca que tallar (`topdown_tile` devuelve vacío en `z > 0`),
    // así que la sala 7×7 amurallada de antes no era "abrirse paso": era
    // regalarte un cuarto fijo con paredes-tile —justo lo que la construcción por
    // bordes vino a reemplazar— y encerrarte en él, porque fuera del 7×7 la roca
    // no se podía ni picar ni construir.
    //
    // Ahora la grada te deja una plataforma de 3×3 con la bajada en el centro: lo
    // mínimo para llegar parado y decidir. El segundo piso lo levantás vos, losa a
    // losa, con las mismas piezas de siempre.
    if took == Portal::Ascend {
        for dz in -1..=1 {
            for dx in -1..=1 {
                let (cx, cz) = (tx + dx, tz + dz);
                let key = (body as u8, z, cx, cz);
                if tiles.edits.contains_key(&key) {
                    continue;
                }
                let t = if (cx, cz) == (tx, tz) { back } else { Tile::WoodFloor };
                tiles.edits.insert(key, t);
                tiles.edit_log.push((cx, cz));
            }
        }
        tiles.version = tiles.version.wrapping_add(1);
        tiles.dirty = true;
        return;
    }
    // **El SÓTANO se talla con paredes de BORDE, no con tiles de muro.**
    //
    // Bajar por una grada que pusiste vos abre un sótano, y hasta ahora su
    // perímetro se estampaba como `Tile::WoodWall`: celdas enteras de muro. Eso
    // es exactamente lo que la construcción por bordes vino a reemplazar (§5,
    // anti-regresión) — un cuarto de 5×5 útiles quedaba con 9 pisables — y encima
    // esas paredes no se podían demoler con el gesto de demoler, porque no eran
    // piezas: había que picarlas como si fueran roca.
    //
    // Ahora el interior es piso y el perímetro son piezas de borde de piedra, las
    // mismas que colocás a mano. El sótano nace siendo una habitación normal: se
    // amplía, se demuele y se le pone puerta como cualquier otra.
    const R: i64 = 3;
    for dz in -R..=R {
        for dx in -R..=R {
            let (cx, cz) = (tx + dx, tz + dz);
            let key = (body as u8, z, cx, cz);
            if tiles.edits.contains_key(&key) {
                continue; // respeta lo construido por el jugador
            }
            let t = if (cx, cz) == (tx, tz) { back } else { Tile::WoodFloor };
            tiles.edits.insert(key, t);
            tiles.edit_log.push((cx, cz));
        }
    }
    // Anillo de bordes: la línea que rodea el cuadrado [-R, R] de celdas.
    let piece = ha_shared::build_grid::edge_piece_index("edge_wall_stone").unwrap_or(0);
    for d in -R..=R {
        for (ex, ez, axis) in [
            (tx + d, tz - R, ha_shared::build_grid::WallAxis::H),     // norte
            (tx + d, tz + R + 1, ha_shared::build_grid::WallAxis::H), // sur
            (tx - R, tz + d, ha_shared::build_grid::WallAxis::V),     // oeste
            (tx + R + 1, tz + d, ha_shared::build_grid::WallAxis::V), // este
        ] {
            if walls.at(body, z, ex, ez, axis).is_some() {
                continue; // respeta lo que ya construiste
            }
            walls.add(crate::walls::PlacedWall {
                body: body as u8,
                z,
                tx: ex,
                tz: ez,
                axis,
                piece,
            });
        }
    }
    tiles.version = tiles.version.wrapping_add(1);
    tiles.dirty = true;
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
        for (e, _) in &chunks {
            if cbudget == 0 {
                break;
            }
            commands.entity(e).despawn_recursive();
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

    let mut budget = ctl.budget.0;
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
        if budget == 0 || !decor_room {
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
            want,
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
    // Lo que no entre en el presupuesto queda anotado: la pantalla de carga no
    // levanta la cortina hasta que esto llega a 0 (y `pending_decor` también,
    // porque un anillo con el suelo puesto pero sin árboles se ve a medio hacer).
    ctl.backlog.0 = missing.len().saturating_sub(budget) + deferred_count;
    for (cx, cz) in missing {
        if budget == 0 {
            break;
        }
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
        let mesh = build_chunk_mesh(&tiles, body, cx, cz);
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
            );
            // Tiles con arte "vivo" (agua/autotile/piezas construidas) como
            // sprites — en TODAS las capas (subsuelos/pisos usan floor/wall/*).
            spawn_chunk_tile_art(&mut commands, &art, &tiles, body, z, cx, cz);
        }
    }
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
        let mesh = build_chunk_mesh(&tiles, chunk.body, chunk.cx, chunk.cz);
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
fn terrace_shadow(body: CelestialBody, tx: i64, tz: i64, level: i8) -> f32 {
    // El agua no recibe sombra de terraza: su superficie ya es un plano distinto y
    // oscurecer la orilla la haría parecer más profunda de lo que es.
    if level < 0 {
        return 1.0;
    }
    let mut drop = 0i32;
    // Se pesa más el NORTE porque en 3/4 la luz se lee viniendo de arriba: la
    // pared que tenés al norte es la que te tapa el sol.
    for (dx, dz, w) in [(0i64, -1i64, 3), (-1, 0, 2), (1, 0, 2), (0, 1, 1)] {
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

pub fn tile_render_color(
    tiles: &TileWorld,
    body: CelestialBody,
    tx: i64,
    tz: i64,
    detail: TileShade,
) -> [f32; 3] {
    let t = tiles.tile_at(body, tx, tz);
    // **El hueco de un piso alto deja ver lo de abajo.** En `z > 0` todo lo que no
    // construiste es vacío (`Tile::Air`): pintarlo de negro convertía el segundo
    // piso en una habitación flotando en la nada y no había forma de saber si el
    // borde de tu losa daba al patio o a la calle. Se busca hacia abajo el primer
    // nivel con algo y se dibuja ATENUADO — la lectura de Project Zomboid: tu
    // planta nítida, el mundo de abajo apagado.
    let (t, void_dim) = if t == Tile::Air && tiles.z > 0 {
        let mut below = tiles.z - 1;
        let mut under = tiles.tile_at_level(body, below, tx, tz);
        while under == Tile::Air && below > 0 {
            below -= 1;
            under = tiles.tile_at_level(body, below, tx, tz);
        }
        (under, VOID_DIM)
    } else {
        (t, 1.0)
    };
    // El líquido se pinta con el color del CUERPO: agua en la Tierra, metano
    // ámbar en Titán, lava en Ío. Un solo tile, un solo autotile, una sola
    // animación — y aun así cada mundo se reconoce de un vistazo.
    let rgb = match t {
        Tile::Water => ha_shared::relief::liquid_color(body, false),
        Tile::WaterDeep => ha_shared::relief::liquid_color(body, true),
        _ => t.color(),
    };
    // Ruido por celda para que un relleno grande no se vea plano.
    let n = (hash_cell(0x51ce, tx, tz) % 100) as f32 / 100.0;
    let mut shade = (0.93 + n * 0.09) * void_dim;
    // SOMBREADO POR ALTURA: cada escalón por encima del mar aclara un poco el
    // suelo y cada uno por debajo lo oscurece. Sin esto, una meseta y la llanura
    // de al lado se pintan del mismo verde y el relieve sólo se lee por la línea
    // del acantilado — con esto, se lee de un vistazo desde el aire, que es
    // donde el jugador pasa la mitad del tiempo.
    //
    // 3.5 % por escalón: a 12 escalones la cumbre queda ~40 % más clara que la
    // costa. Más que esto y el mundo parece un mapa de calor.
    let level = ha_shared::relief::relief_level(body, tx, tz);
    shade *= 1.0 + (level as f32).clamp(-4.0, 12.0) * 0.035;
    if detail == TileShade::Full {
        shade *= terrace_shadow(body, tx, tz, level);
    }
    [rgb[0] * shade, rgb[1] * shade, rgb[2] * shade]
}

/// One `Mesh2d` covering a CHUNK×CHUNK grid of tiles, each a colored quad at
/// render pos `(tx*TILE, -tz*TILE)`.
fn build_chunk_mesh(tiles: &TileWorld, body: CelestialBody, cx: i64, cz: i64) -> Mesh {
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(1024);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(1024);
    let mut indices: Vec<u32> = Vec::with_capacity(1536);

    let mut quad = |x0: f32, y0: f32, size: f32, rgba: [f32; 4]| {
        let base = positions.len() as u32;
        positions.push([x0, y0, 0.0]);
        positions.push([x0 + size, y0, 0.0]);
        positions.push([x0 + size, y0 + size, 0.0]);
        positions.push([x0, y0 + size, 0.0]);
        let lin = Color::srgba(rgba[0], rgba[1], rgba[2], rgba[3]).to_linear();
        for _ in 0..4 {
            colors.push([lin.red, lin.green, lin.blue, lin.alpha]);
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    };

    for lx in 0..CHUNK {
        for lz in 0..CHUNK {
            let tx = cx * CHUNK + lx;
            let tz = cz * CHUNK + lz;
            // El color sale de `tile_render_color`, la MISMA función que usan el
            // mapa satelital y el disco del planeta: es lo que hace que al subir
            // el terreno no cambie de color ni pierda el relieve.
            let rgb = tile_render_color(tiles, body, tx, tz, TileShade::Full);
            // Render pos: cell (tx,tz) spans [tx,tx+1]×[tz,tz+1] in world; screen
            // y = -z so the quad bottom-left is (tx*TILE, -(tz+1)*TILE).
            let x0 = tx as f32 * TILE_M;
            let y0 = -(tz as f32 + 1.0) * TILE_M;
            quad(x0, y0, TILE_M, [rgb[0], rgb[1], rgb[2], 1.0]);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
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
) {
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

            // Z-Level portals (mine entrance / ladders).
            if let Some(portal) = portal_here(body, z, tx, tz) {
                let (color, size) = match portal {
                    Portal::Descend => (Color::srgb(0.05, 0.04, 0.06), Vec2::new(1.6, 1.6)),
                    Portal::Ascend => (Color::srgb(0.55, 0.40, 0.20), Vec2::new(1.2, 1.6)),
                };
                let key = match portal {
                    Portal::Descend => "decor/mine_hole",
                    Portal::Ascend => "decor/ladder",
                };
                let mut e = commands.spawn((
                    SurfaceDecor { cx, cz, z, small: false },
                    art.sprite_or_color(key, color, size),
                    // Boca de mina / escalera: son AGUJEROS en el suelo, no
                    // objetos parados. Banda de piso — nunca te tapan al entrar.
                    Transform::from_xyz(r.x, r.y, crate::layers::FLOOR),
                ));
                if let Some(a) = art.animation(key) {
                    e.insert(a);
                }
                crate::light2d::attach(&mut e, &art, &[key], size);
            }

            // Surface decor only on the surface layer, y NUNCA sobre un tile SÓLIDO —
            // natural (roca) o CONSTRUIDO por el jugador (muros): así los árboles/props ya
            // no tapan tu muro (bug de capas). `solid_at` incluye las ediciones.
            // Tampoco sobre celdas AUTORADAS en el editor de mundo: si pintaste
            // un claro de pasto donde había bosque, el bosque no vuelve a crecer.
            // `buildable_ground`: ni sobre líquido (el bajío ya no es "sólido"),
            // ni sobre rampas, ni sobre lo que construyó el jugador.
            if z == 0
                && tiles.tile_at(body, tx, tz).buildable_ground()
                && !decor_free.contains(body, z, tx, tz)
            {
                if let Some(decor) = surface_decor_at(body, tx, tz) {
                    match decor {
                        DecorKind::Tree => {
                            // Removed set is a flat per-body set at key (0,0);
                            // `id` = packed global cell (see `pack_cell`).
                            if !flora.is_removed(body, 0, 0, id) {
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
                                    sprite,
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

            // Ore/gem deposits (surface sparse, underground rich). Mined ones are
            // removed like rocks/trees.
            if let Some(dep) = deposit_at(body, z, tx, tz).filter(|_| !flora.is_removed(body, 0, 0, id)) {
                let kind = deposit_target_kind(dep);
                let (color, size) = deposit_style(kind);
                let key = match kind {
                    InteractTargetKind::GemRed => "decor/gem_red",
                    InteractTargetKind::GemBlue => "decor/gem_blue",
                    InteractTargetKind::OreGold => "decor/ore_gold",
                    _ => "decor/ore_iron",
                };
                let mut e = commands.spawn((
                    SurfaceDecor { cx, cz, z, small: false },
                    InteractTarget { kind, id },
                    art.sprite_or_color(key, color, size),
                    Transform::from_xyz(r.x, r.y, sort),
                ));
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
        if layer == Some(ha_assets::ArtLayer::Roof) {
            // Techo autorado: se desvanece cuando el jugador camina por debajo.
            ent.insert(crate::layers::Overhead {
                min: (min_x, min_z),
                size: (cells.0.max(1) as i64, cells.1.max(1) as i64),
            });
        }
        if let Some(a) = anim {
            ent.insert(a);
        }
        crate::light2d::attach(&mut ent, &art, &[d.key.as_str()], size.into());
    }

    flora.set_chunk_trees(cx as i32, cz as i32, trees);

    if detail == DecorDetail::Full {
        spawn_chunk_small_decor(commands, art, tiles, decor_free, body, z, cx, cz);
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
    art: &ArtLibrary,
    tiles: &TileWorld,
    decor_free: &DecorFreeCells,
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
            if decor_free.contains(body, z, tx, tz) {
                continue;
            }
            let tile = tiles.tile_at(body, tx, tz);
            // EL AGUA TAMBIÉN SE DECORA. Antes esta puerta era "sólo suelo firme",
            // así que los lagos y el mar salían como una plancha lisa de un solo
            // azul — que a la vista es un agujero en el mapa. Ahora el bajío lleva
            // troncos flotantes, nenúfares y juncos, y su orilla lleva el
            // campamento y el puesto de pesca.
            let e = if tile.swim() {
                ha_shared::worldgen::water_decor_at(body, tx, tz)
            } else if !tile.buildable_ground() || surface_decor_at(body, tx, tz).is_some() {
                None
            } else {
                ha_shared::worldgen::shore_decor_at(body, tx, tz)
                    .or_else(|| nature_decor_at(body, tx, tz))
                    .or_else(|| prop_decor_at(body, tx, tz))
            };
            let Some(e) = e else { continue };
            // `world_sprite` y no `get`: si el slot se recortó de un atlas, con el
            // handle a pelo se plantaba la HOJA ENTERA en el pasto.
            let Some((mut s, _)) = art.world_sprite(e.key, e.size.into()) else { continue };
            let wx = tx as f32 * TILE_M + 0.5;
            let wz = tz as f32 * TILE_M + 0.5;
            let r = render_xy(wx, wz);
            s.custom_size = Some(e.size.into());
            // Pivote al pie (LimeZu): el centro sube medio alto.
            let y = r.y + e.size[1] * 0.5 - 0.5;
            let mut ent = commands.spawn((
                SurfaceDecor { cx, cz, z, small: true },
                s,
                // Mismo criterio que el decor autorado: una mata marcada piso
                // (musgo, charco, hojas caídas) no debe taparte los pies.
                Transform::from_xyz(r.x, y, crate::layers::depth(art.slot_layer(e.key), wz)),
            ));
            // Faroles, fogatas, hongos brillantes: la luz viene del slot, no de
            // una lista hardcodeada acá.
            crate::light2d::attach(&mut ent, &art, &[e.key], e.size.into());
        }
    }
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

/// Material de la cara de un acantilado o de una rampa, según el bioma en el que
/// está. Es lo que hace que el relieve no se vea igual en todas partes: la misma
/// geometría, distinto material.
fn cliff_material(body: CelestialBody, tx: i64, tz: i64) -> &'static str {
    use ha_shared::geography::Biome;
    match ha_shared::geography::biome_at(body, tx, tz) {
        Biome::Desert | Biome::Beach | Biome::Savanna => "arena",
        Biome::Snow | Biome::Ice => "nieve",
        Biome::Grassland | Biome::Forest | Biome::Swamp => "pasto",
        Biome::Regolith => "regolito",
        Biome::Basalt => "basalto",
        // Sierra, montaña, río, océano: roca desnuda.
        _ => "roca",
    }
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
    if !matches!(tile, Tile::Cliff | Tile::Slope) {
        return None;
    }
    // BORDILLO vs PARED. Un acantilado de un solo escalón se trepa (ver
    // `TileWorld::walk_blocked`) y el jugador tiene que poder VERLO antes de
    // caminar hacia él: si se dibujara igual que un muro de tres escalones, la
    // única forma de saber cuál es cuál sería chocarse. Su arte vive en
    // `ledge/*` con el mismo autotile que el resto (centro, bordes, esquinas);
    // mientras no lo cures, cae al `cliff/*` de siempre y todo sigue igual.
    if tile == Tile::Cliff && ha_shared::relief::climbable_edge(body, tx, tz) {
        let ledge: &'static str = match cliff_material(body, tx, tz) {
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
    let key: &'static str = match (tile, cliff_material(body, tx, tz)) {
        (Tile::Cliff, "arena") => "cliff/arena",
        (Tile::Cliff, "nieve") => "cliff/nieve",
        (Tile::Cliff, "pasto") => "cliff/pasto",
        (Tile::Cliff, "regolito") => "cliff/regolito",
        (Tile::Cliff, "basalto") => "cliff/basalto",
        (Tile::Slope, "arena") => "slope/arena",
        (Tile::Slope, "nieve") => "slope/nieve",
        (Tile::Slope, "pasto") => "slope/pasto",
        // "roca" y cualquier material sin variante propia: la genérica sirve.
        _ => return None,
    };
    art.tile_has_art(key).then_some(key)
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
        // Lados opuestos / 3-4 lados distintos → center como fallback seguro.
        _ => "center",
    }
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
    for lx in 0..CHUNK {
        for lz in 0..CHUNK {
            let tx = cx * CHUNK + lx;
            let tz = cz * CHUNK + lz;
            let tile = tiles.tile_at(body, tx, tz);
            let Some(generic) = tile_art_key(tile) else { continue };
            // El relieve elige su arte por BIOMA: la misma pared es roca desnuda
            // en la sierra, arena en el desierto y nieve en la cumbre. Si esa
            // variante no está curada todavía, cae a la genérica y se ve igual que
            // antes — nunca queda una celda sin dibujar por falta de arte.
            let key = terrain_art_key(art, body, tile, tx, tz).unwrap_or(generic);
            let wx = tx as f32 * TILE_M + 0.5;
            let wz = tz as f32 * TILE_M + 0.5;
            let r = render_xy(wx, wz);
            // Puertas: pose open/closed animada por proximidad (building.rs las
            // conmuta). Con arte de poses, spawn especial; si no, sigue al autotile.
            if tile == Tile::Door && art.has_poses(key) {
                if let Some((_, _)) = art.pose(key, "closed").or_else(|| art.pose(key, "open")) {
                    // ALTA Y APOYADA EN SU PIE, igual que las paredes de borde.
                    // Dibujada 1×1 quedaba más baja que el jugador: una puerta
                    // por la que no cabés no se lee como una puerta. Ocupa un
                    // tile de SUELO y `WALL_RENDER_H` tiles de ALTO; el pie se
                    // queda clavado en la celda, así que la colisión y el
                    // Y-sort no cambian — sólo crece hacia arriba en pantalla.
                    let size = crate::building::door_render_size();
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
            if !art.tile_has_art(key) {
                continue;
            }
            let same = |dx: i64, dz: i64| tiles.tile_at(body, tx + dx, tz + dz) == tile;
            // n=norte(−z) e=este(+x) s=sur(+z) w=oeste(−x) + diagonales.
            let shape = autotile_shape(
                same(0, -1), same(1, 0), same(0, 1), same(-1, 0),
                same(1, -1), same(-1, -1), same(1, 1), same(-1, 1),
            );
            let Some((sprite, anim)) = art.tile_sprite(key, shape, Vec2::splat(TILE_M)) else { continue };
            // Banda por ALTO, no por colisión. `Tile::solid()` significa "bloquea
            // el paso" y usarlo acá era el bug del AGUA tapando al jugador: el
            // agua frena pero es plana. `upright()` es el eje correcto — sólo lo
            // que se levanta del suelo (muros, ventanas, roca) Y-sortea; el resto
            // (agua, hielo, pisos) vive en la banda de piso, siempre bajo los pies.
            // Una capa explícita en el slot manda por encima de las dos.
            let depth = match art.slot_layer(key) {
                Some(l) => crate::layers::depth(Some(l), wz),
                None if tile.upright() => crate::layers::ysort(wz),
                None => crate::layers::FLOOR,
            };
            let mut ent = commands.spawn((
                SurfaceDecor { cx, cz, z, small: false },
                TileArt,
                sprite,
                Transform::from_xyz(r.x, r.y, depth),
            ));
            if let Some(a) = anim {
                ent.insert(a);
            }
            // Tiles que iluminan (lava, cristal, un piso de luz). Sólo pagan quad
            // los materiales que el usuario marcó como emisores.
            crate::light2d::attach(&mut ent, &art, &[key], Vec2::splat(TILE_M));
        }
    }
}

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

    /// **El pie de un acantilado se oscurece.** Es lo que hace que el relieve se
    /// lea como relieve en cenital: sin la franja de sombra, el ojo no puede
    /// distinguir "la terraza de al lado está más alta" de "es otro material".
    #[test]
    fn the_foot_of_a_cliff_gets_a_shadow() {
        use ha_shared::relief::{relief_cell, relief_level, ReliefCell};
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
                let under_wall = [(0i64, -1i64), (-1, 0), (1, 0), (0, 1)]
                    .into_iter()
                    .any(|(dx, dz)| relief_level(body, tx + dx, tz + dz) > l);
                let s = terrace_shadow(body, tx, tz, l);
                if under_wall {
                    assert!(s < 1.0, "el pie de la pared en ({tx},{tz}) no tiene sombra");
                    assert!(s >= 0.66, "sombra demasiado negra: {s}");
                    shadowed += 1;
                } else {
                    assert_eq!(s, 1.0, "sombra en campo abierto en ({tx},{tz})");
                    plain_ok += 1;
                }
                let _ = relief_cell(body, tx, tz) == ReliefCell::Flat;
            }
        }
        assert!(shadowed > 50, "apenas {shadowed} celdas al pie de una pared");
        assert!(plain_ok > 1_000, "casi todo el mundo salió sombreado: {plain_ok}");
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
