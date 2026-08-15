//! **Techos y tumbados por CELDA** — la tercera capa de la construcción.
//!
//! El modelo entero, en una línea: **piso = celda · pared = borde · techo =
//! celda, pero por encima**. Antes el único techo era el rectángulo que estampaba
//! un prefab (`building::BuiltRoofs`), así que una casa que crece losa a losa —la
//! que el jugador construye de verdad— **no tenía techo**, y el del prefab no
//! sabía nada de la planta de arriba.
//!
//! Dos clases, un solo sistema (`build_grid::CeilingKind`):
//! - **Tumbado** (`Ceiling`): la tapa lisa de adentro. Es "poner el piso como
//!   techo", que es exactamente como lo pide uno cuando termina el cuarto.
//! - **Techo modular** (`Roof`): la cubierta de afuera, con autotile por vecinos
//!   (`world2d::autotile_shape`) — pintás la superficie y el juego resuelve
//!   faldones, cumbrera y esquinas. Nueve piezas sueltas para encajar a mano en
//!   un celular no es un sistema de construcción, es un rompecabezas.
//!
//! **El suelo del 2º ES el techo del 1º.** Además de lo que coloques, cada celda
//! cuya planta de arriba tiene losa se tapa sola (`derived_cover`). Así techar es
//! el resultado natural de construir el piso siguiente y no una tarea aparte que
//! hay que acordarse de hacer.
//!
//! **Se transparenta al entrar**, con la misma regla que los techos de prefab: si
//! el jugador está debajo de una tapa, todas las de su vecindad bajan el alpha.
//! No hay cambio de escena ni cámara nueva (§4).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use ha_shared::build_grid::{ceiling_piece, ceiling_piece_index, CeilingKind, CeilingPiece, CEILING_PIECES};
use ha_shared::topdown::TILE_M;
use ha_shared::CelestialBody;

use crate::art2d::ArtLibrary;
use crate::inventory_persist::persist_dir;
use crate::platform::is_app_paused;
use crate::scene::{RegionRuntime, WorldContextRuntime};
use crate::sim::Player;
use crate::world2d::{
    active_surface_body, autotile_shape, render_xy, surface_region_active, ActivePlanet, TileWorld,
};

/// Radio (tiles) alrededor del jugador donde las tapas existen como entidad. Lo
/// que se ve en pantalla a zoom normal, no más: una tapa fuera de cuadro no se
/// mira y cuesta lo mismo que una que sí.
const CEIL_STREAM_RADIUS: i64 = 26;
/// Tope duro de sprites de tapa vivos. Una base grande —que es lo que este
/// sistema existe para permitir— puede tapar miles de celdas, y un sprite por
/// celda las convertiría en miles de entidades: justo el costo que hundió el
/// frame cuando el decor no tenía tope (§10). Pasado el tope se quedan las MÁS
/// CERCANAS, así lo que se recorta es siempre el borde de la pantalla.
const MAX_CEIL_SPRITES: usize = 512;
/// Refresco del streaming (s). Colocar/quitar refresca al instante igual.
const CEIL_STREAM_PERIOD: f32 = 0.25;
/// Alpha de la tapa con el jugador debajo (se ve el interior).
const INSIDE_ALPHA: f32 = 0.15;
/// Velocidad del fundido (lerp exponencial por dt).
const FADE_RATE: f32 = 6.0;
/// Radio (tiles) de tapas que se transparentan al meterte debajo — el tamaño de
/// "tu edificio", no del barrio.
const FADE_RADIUS: i64 = 12;

/// Una tapa colocada. `piece` es el índice en `CEILING_PIECES` (se persiste por
/// id, no por índice, para que reordenar el catálogo no rompa partidas).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PlacedCeiling {
    pub body: u8,
    pub z: i32,
    pub tx: i64,
    pub tz: i64,
    pub piece: usize,
}

type CellKey = (u8, i32, i64, i64);

fn cell_key(c: &PlacedCeiling) -> CellKey {
    (c.body, c.z, c.tx, c.tz)
}

/// Registro persistente de tapas. Indexado por celda desde el principio: lo
/// consultan el streaming (por vecino, para el autotile) y el fundido, y la
/// lección del índice de paredes ya está aprendida (`walls.rs`).
#[derive(Resource, Default)]
pub struct BuiltCeilings {
    pub list: Vec<PlacedCeiling>,
    index: std::collections::HashMap<CellKey, usize>,
    rev: u64,
    dirty: bool,
    was_paused: bool,
    debounce: f32,
}

impl BuiltCeilings {
    pub fn at(&self, body: CelestialBody, z: i32, tx: i64, tz: i64) -> Option<&PlacedCeiling> {
        let idx = *self.index.get(&(body as u8, z, tx, tz))?;
        self.list.get(idx)
    }

    pub fn add(&mut self, c: PlacedCeiling) {
        let key = cell_key(&c);
        match self.index.get(&key) {
            Some(&idx) => self.list[idx] = c,
            None => {
                self.index.insert(key, self.list.len());
                self.list.push(c);
            }
        }
        self.dirty = true;
        self.rev += 1;
    }

    /// Quita la tapa de una celda; devuelve la que estaba. Mismo `swap_remove`
    /// con reapunte del índice que `BuiltWalls::remove`.
    pub fn remove(&mut self, body: CelestialBody, z: i32, tx: i64, tz: i64) -> Option<PlacedCeiling> {
        let idx = self.index.remove(&(body as u8, z, tx, tz))?;
        let gone = self.list.swap_remove(idx);
        if idx < self.list.len() {
            let moved = cell_key(&self.list[idx]);
            self.index.insert(moved, idx);
        }
        self.dirty = true;
        self.rev += 1;
        Some(gone)
    }
}

/// Sprite vivo de una tapa (clave de streaming + forma autotile actual).
#[derive(Component)]
pub(crate) struct CeilingSprite {
    tx: i64,
    tz: i64,
    z: i32,
    body: CelestialBody,
    shape: &'static str,
    piece: usize,
    /// `true` = derivada de la losa de arriba, no colocada a mano.
    derived: bool,
}

pub struct CeilingsPlugin;

impl Plugin for CeilingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuiltCeilings>()
            .add_systems(Startup, load_ceilings_on_startup)
            .add_systems(
                Update,
                (stream_ceilings, fade_ceilings, debounced_ceilings_save),
            );
    }
}

/// **¿Esta celda está tapada?** Las tres formas que tiene una casa de tener
/// techo, y las tres cuentan igual:
///
/// - el tumbado que colocaste celda a celda,
/// - la losa de la planta de arriba (el suelo del 2º es el techo del 1º),
/// - el techo del cuarto que estampaste de una pieza (`building::BuiltRoofs`).
///
/// La tercera faltaba, y por eso una habitación prefabricada —que se ve techada—
/// no se comportaba como techada: adentro daba el sol y no se podían poner
/// muebles.
pub fn covered(
    ceilings: &BuiltCeilings,
    roofs: &crate::building::BuiltRoofs,
    tiles: &TileWorld,
    body: CelestialBody,
    z: i32,
    tx: i64,
    tz: i64,
) -> bool {
    ceilings.at(body, z, tx, tz).is_some()
        || ha_shared::build_grid::is_build_floor(tiles.tile_at_level(body, z + 1, tx, tz))
        || roofs.covers(body, z, tx, tz)
}

/// Color de respaldo cuando el slot todavía no tiene arte: el sistema se ve y se
/// prueba igual, como el resto de la construcción.
fn fallback_color(p: &CeilingPiece) -> Color {
    match p.kind {
        // El tumbado de piedra se distingue del de madera sin arte asignado: si
        // los dos fueran el mismo café, tener dos piezas no significaría nada.
        CeilingKind::Ceiling if p.art.contains("piedra") => Color::srgb(0.46, 0.45, 0.43),
        CeilingKind::Ceiling => Color::srgb(0.42, 0.33, 0.24),
        CeilingKind::Roof if p.art.contains("chapa") => Color::srgb(0.55, 0.57, 0.60),
        CeilingKind::Roof => Color::srgb(0.52, 0.26, 0.20),
    }
}

/// Sprite de la tapa: la pose autotile si el arte la trae, si no la plana.
fn ceiling_sprite(art: &ArtLibrary, p: &CeilingPiece, shape: &str, size: Vec2) -> Sprite {
    if let Some(s) = art.pose_sprite(p.art, shape, size) {
        return s;
    }
    art.sprite_or_color(p.art, fallback_color(p), size)
}

#[allow(clippy::too_many_arguments)]
fn stream_ceilings(
    mut commands: Commands,
    time: Res<Time>,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    art: Res<ArtLibrary>,
    built: Res<BuiltCeilings>,
    player: Query<&Transform, With<Player>>,
    live: Query<(Entity, &CeilingSprite)>,
    mut acc: Local<f32>,
    mut last_rev: Local<u64>,
    mut last_version: Local<u64>,
) {
    if surface_region_active(&region).is_none() {
        for (e, _) in &live {
            commands.entity(e).despawn_recursive();
        }
        return;
    }
    // Refresco periódico; colocar/quitar una tapa —o la losa de arriba, que es
    // una edición de tiles— se ve al instante.
    *acc += time.delta_secs();
    if *acc < CEIL_STREAM_PERIOD && *last_rev == built.rev && *last_version == tiles.version {
        return;
    }
    *acc = 0.0;
    *last_rev = built.rev;
    *last_version = tiles.version;
    let Ok(player_tf) = player.get_single() else {
        return;
    };
    let body = active_surface_body(&ctx, &planet);
    let z = tiles.z;
    let ptx = (player_tf.translation.x / TILE_M).floor() as i64;
    let ptz = (-player_tf.translation.y / TILE_M).floor() as i64;

    // Qué tapa le toca a una celda: la colocada gana sobre la derivada, porque es
    // una decisión explícita del jugador (le pusiste teja a un alero que además
    // tiene losa arriba: querías teja).
    let piece_at = |cx: i64, cz: i64| -> Option<(usize, bool)> {
        if let Some(c) = built.at(body, z, cx, cz) {
            return Some((c.piece, false));
        }
        if ha_shared::build_grid::is_build_floor(tiles.tile_at_level(body, z + 1, cx, cz)) {
            // La losa de arriba se ve desde abajo como tumbado.
            return Some((0, true));
        }
        None
    };

    // **Se recorre lo que HAY, no el cuadrado por si acaso.** Barrer 81×81 celdas
    // cuatro veces por segundo cuesta lo mismo tengas una casa o ninguna, y el
    // 99 % de la partida no tiene ni una tapa cerca. Las candidatas son las tapas
    // colocadas y las celdas EDITADAS de la planta de arriba (las losas): sin
    // segundo piso, la segunda lista está vacía y esto no cuesta nada.
    let r = CEIL_STREAM_RADIUS;
    let cerca = |cx: i64, cz: i64| (cx - ptx).abs() <= r && (cz - ptz).abs() <= r;
    let mut cand: Vec<(i64, i64)> = built
        .list
        .iter()
        .filter(|c| c.body == body as u8 && c.z == z && cerca(c.tx, c.tz))
        .map(|c| (c.tx, c.tz))
        .collect();
    cand.extend(
        tiles
            .edited_cells_at_level(body, z + 1)
            .filter(|&(cx, cz, t)| ha_shared::build_grid::is_build_floor(t) && cerca(cx, cz))
            .map(|(cx, cz, _)| (cx, cz)),
    );
    cand.sort_unstable();
    cand.dedup();
    if cand.len() > MAX_CEIL_SPRITES {
        cand.sort_by_key(|&(cx, cz)| (cx - ptx).abs() + (cz - ptz).abs());
        cand.truncate(MAX_CEIL_SPRITES);
    }

    let mut want: Vec<(i64, i64, usize, bool, &'static str)> = Vec::new();
    for (cx, cz) in cand {
        {
            let Some((piece, derived)) = piece_at(cx, cz) else {
                continue;
            };
            let def = ceiling_piece(piece).unwrap_or(&CEILING_PIECES[0]);
            // El tumbado no tiene bordes que resolver: es una tapa lisa. El techo
            // sí — su forma sale de qué vecinas están tapadas.
            let shape = if def.kind == CeilingKind::Roof {
                let same = |dx: i64, dz: i64| piece_at(cx + dx, cz + dz).is_some();
                autotile_shape(
                    same(0, -1), same(1, 0), same(0, 1), same(-1, 0),
                    same(1, -1), same(-1, -1), same(1, 1), same(-1, 1),
                )
            } else {
                "center"
            };
            want.push((cx, cz, piece, derived, shape));
        }
    }

    // Despawn de las que ya no aplican (lejos, otra capa, o cambió su forma).
    let mut alive: Vec<(i64, i64)> = Vec::new();
    for (e, s) in &live {
        let keep = s.body == body
            && s.z == z
            && want.iter().any(|&(cx, cz, piece, derived, shape)| {
                cx == s.tx && cz == s.tz && piece == s.piece && derived == s.derived && shape == s.shape
            });
        if keep {
            alive.push((s.tx, s.tz));
        } else {
            commands.entity(e).despawn_recursive();
        }
    }
    // Spawn de las que faltan.
    let size = Vec2::splat(TILE_M);
    for (cx, cz, piece, derived, shape) in want {
        if alive.contains(&(cx, cz)) {
            continue;
        }
        let def = ceiling_piece(piece).unwrap_or(&CEILING_PIECES[0]);
        let pos = render_xy((cx as f32 + 0.5) * TILE_M, (cz as f32 + 0.5) * TILE_M);
        commands.spawn((
            CeilingSprite { tx: cx, tz: cz, z, body, shape, piece, derived },
            ceiling_sprite(&art, def, shape, size),
            // Encima de todo lo Y-sorted, igual que los techos de prefab: una tapa
            // tapa, y lo que la vuelve transitable visualmente es el fundido.
            Transform::from_xyz(pos.x, pos.y, crate::layers::OVERHEAD),
        ));
    }
}

/// El jugador entra debajo → las tapas de alrededor se transparentan; sale →
/// vuelven. Se decide por la celda del jugador (¿está tapada?), no por
/// rectángulos: una casa que creció losa a losa no tiene rectángulo.
fn fade_ceilings(
    time: Res<Time>,
    tiles: Res<TileWorld>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    built: Res<BuiltCeilings>,
    roofs: Res<crate::building::BuiltRoofs>,
    player: Query<&Transform, With<Player>>,
    mut sprites: Query<(&CeilingSprite, &mut Sprite)>,
) {
    let Ok(player_tf) = player.get_single() else {
        return;
    };
    let body = active_surface_body(&ctx, &planet);
    let ptx = (player_tf.translation.x / TILE_M).floor() as i64;
    let ptz = (-player_tf.translation.y / TILE_M).floor() as i64;
    let inside = covered(&built, &roofs, &tiles, body, tiles.z, ptx, ptz);
    let k = 1.0 - (-FADE_RATE * time.delta_secs()).exp();
    for (s, mut sprite) in &mut sprites {
        // Se transparenta la tapa DE TU EDIFICIO, no la del barrio. Sin el radio,
        // meterse en una casa apagaba también el techo del vecino de enfrente y
        // media manzana quedaba destapada.
        //
        // Es un radio y no la región conectada de verdad a propósito: calcular la
        // mancha exacta pediría un flood-fill por frame, y a esta escala —una casa
        // entra de sobra en el radio, la de enfrente no— no se nota la diferencia.
        let cerca = (s.tx - ptx).abs() <= FADE_RADIUS && (s.tz - ptz).abs() <= FADE_RADIUS;
        let target = if inside && cerca { INSIDE_ALPHA } else { 1.0 };
        let a = sprite.color.alpha();
        if (a - target).abs() > 0.002 {
            sprite.color.set_alpha(a + (target - a) * k);
        }
    }
}

// --- Persistencia (mirror de walls.json) -------------------------------------

#[derive(Serialize, Deserialize, Default)]
struct CeilingsSave {
    ceilings: Vec<(u8, i32, i64, i64, String)>,
}

fn ceilings_path() -> std::path::PathBuf {
    persist_dir().join("ceilings.json")
}

fn load_ceilings_on_startup(mut built: ResMut<BuiltCeilings>) {
    let Ok(raw) = std::fs::read_to_string(ceilings_path()) else {
        return;
    };
    let Ok(save) = serde_json::from_str::<CeilingsSave>(&raw) else {
        return;
    };
    for (body, z, tx, tz, piece) in save.ceilings {
        // Pieza desconocida (catálogo cambiado) → se descarta esa tapa, no se
        // rompe la partida. Por `add`, que es lo único que indexa.
        let Some(piece) = ceiling_piece_index(&piece) else {
            continue;
        };
        built.add(PlacedCeiling { body, z, tx, tz, piece });
    }
    built.dirty = false; // cargar no es construir
}

fn debounced_ceilings_save(time: Res<Time>, mut built: ResMut<BuiltCeilings>) {
    let paused = is_app_paused();
    let flush_now = paused && !built.was_paused && built.dirty;
    built.was_paused = paused;
    if !built.dirty {
        return;
    }
    built.debounce += time.delta_secs();
    if !flush_now && built.debounce < 20.0 {
        return;
    }
    built.debounce = 0.0;
    built.dirty = false;
    let save = CeilingsSave {
        ceilings: built
            .list
            .iter()
            .map(|c| {
                let id = ceiling_piece(c.piece).unwrap_or(&CEILING_PIECES[0]).id;
                (c.body, c.z, c.tx, c.tz, id.to_string())
            })
            .collect(),
    };
    if let Ok(json) = serde_json::to_string(&save) {
        let _ = std::fs::write(ceilings_path(), json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ceil_at(tx: i64, tz: i64) -> PlacedCeiling {
        PlacedCeiling { body: 0, z: 0, tx, tz, piece: 0 }
    }

    /// Mismo contrato que las paredes: el índice y la lista no discrepan nunca.
    #[test]
    fn quitar_una_tapa_no_desindexa_a_las_demas() {
        let body = CelestialBody::Earth;
        let mut c = BuiltCeilings::default();
        for i in 0..5 {
            c.add(ceil_at(i, 0));
        }
        assert!(c.remove(body, 0, 0, 0).is_some());
        assert_eq!(c.list.len(), 4);
        for i in 1..5 {
            assert!(c.at(body, 0, i, 0).is_some(), "tapa {i} perdida");
        }
        assert!(c.at(body, 0, 0, 0).is_none());
        assert_eq!(c.index.len(), c.list.len());
    }

    /// **El suelo del 2º es el techo del 1º.** Una celda sin tapa colocada pero
    /// con losa arriba tiene que contar como tapada, o techar sería una tarea
    /// aparte de construir el piso siguiente.
    #[test]
    fn la_losa_de_arriba_tapa_la_planta_de_abajo() {
        let body = CelestialBody::Earth;
        let ceilings = BuiltCeilings::default();
        let mut roofs = crate::building::BuiltRoofs::default();
        let mut tiles = TileWorld::default();
        let (tx, tz) = (300, 300);
        assert!(!covered(&ceilings, &roofs, &tiles, body, 0, tx, tz), "a cielo abierto");
        tiles.z = 1;
        assert!(tiles.place_tile(body, tx, tz, ha_shared::topdown::Tile::WoodFloor));
        tiles.z = 0;
        assert!(covered(&ceilings, &roofs, &tiles, body, 0, tx, tz), "la losa de arriba tapa");
        // Y el cuarto estampado de una pieza también está techado: su tapa vive en
        // otro registro, pero techado es techado.
        let mut tiles = TileWorld::default();
        let (px, pz) = (500, 500);
        assert!(!covered(&ceilings, &roofs, &tiles, body, 0, px, pz));
        roofs.add(crate::building::RoofRect {
            body: body as u8,
            z: 0,
            min: (px - 1, pz - 1),
            w: 3,
            h: 3,
            art: None,
        });
        assert!(
            covered(&ceilings, &roofs, &tiles, body, 0, px, pz),
            "el techo del prefab no tapa"
        );
        assert!(!covered(&ceilings, &roofs, &tiles, body, 0, px + 5, pz));
        tiles.z = 0;
    }
}
