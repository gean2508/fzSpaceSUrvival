//! Paredes de BORDE (construcción estilo *Last Day on Earth*).
//!
//! El piso vive en la celda (`TileWorld`); la pared vive en la **arista** entre
//! dos celdas. Las reglas puras (snapping, apoyo, colisión) están en
//! `ha_shared::build_grid`; acá vive el estado del juego:
//! - `BuiltWalls`: lo construido, con persistencia en `walls.json`
//!   (mismo debounce/flush-on-pause que `roofs.json` y `structures.json`),
//! - streaming de sprites alrededor del jugador (Y-sorted, anclados al pie del
//!   borde y con la pose que corresponde: `front`/`back`/`left`/`right`),
//! - puertas: se abren/cierran por proximidad (poses `open`/`closed`).
//!
//! Las poses las recorta el usuario en el editor de assets sobre el atlas del
//! slot (`wall/wood`, `wall/stone`, `door/wood`, `window/basic`). Sin arte
//! asignado el sistema igual funciona: quad de color.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use ha_shared::build_grid::{
    edge_center, edge_cells, edge_piece, edge_piece_index, is_build_floor, EdgePiece, WallAxis,
    WallKind, EDGE_PIECES, WALL_RENDER_H,
};
use ha_shared::topdown::TILE_M;
use ha_shared::CelestialBody;

use crate::art2d::ArtLibrary;
use crate::inventory_persist::persist_dir;
use crate::platform::is_app_paused;
use crate::scene::{RegionRuntime, WorldContextRuntime};
use crate::sim::Player;
use crate::world2d::{
    active_surface_body, render_xy, sort_z, surface_region_active, ActivePlanet, TileWorld,
};

/// Radio (tiles) alrededor del jugador donde las paredes existen como entidad.
const WALL_STREAM_RADIUS: i64 = 48;
/// Refresco del streaming (s). Colocar/quitar refresca al instante igual.
const WALL_STREAM_PERIOD: f32 = 0.2;
/// Distancia (tiles) a la que una puerta se abre.
const DOOR_OPEN_DIST: f32 = 1.7;

/// Una pared colocada. `(tx, tz, axis)` es el BORDE (ver `build_grid`), no la
/// celda; `piece` es el índice en `EDGE_PIECES` (se persiste por id, no por
/// índice, para que reordenar el catálogo no rompa partidas guardadas).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PlacedWall {
    pub body: u8,
    pub z: i32,
    pub tx: i64,
    pub tz: i64,
    pub axis: WallAxis,
    pub piece: usize,
}

impl PlacedWall {
    pub fn def(&self) -> &'static EdgePiece {
        edge_piece(self.piece).unwrap_or(&EDGE_PIECES[0])
    }
}

/// Clave de un borde: cuerpo, piso, celda y eje. Es lo que indexa `BuiltWalls`.
type EdgeKey = (u8, i32, i64, i64, u8);

fn edge_key(w: &PlacedWall) -> EdgeKey {
    (w.body, w.z, w.tx, w.tz, w.axis.as_u8())
}

/// Registro persistente de paredes construidas por el jugador.
///
/// **Indexado por borde, no barrido.** `blocks()` es la consulta más caliente del
/// juego: la llama `wall_capsule_hits` para cada arista que toca la cápsula (≈9),
/// dos veces por eje y por frame, para el jugador **y para cada animal y NPC**.
/// Con la búsqueda lineal de antes eso era `paredes × 9 × 2 × entidades` por
/// frame: gratis con la choza de cuatro paredes de la prueba, y decenas de
/// millones de comparaciones en cuanto alguien levanta el refugio que este
/// sistema existe para levantar. Justo el costo que aparece cuando el jugador
/// hace lo que le pedimos.
///
/// El `Vec` se queda porque el streaming y el guardado necesitan recorrer en
/// orden estable; el mapa sólo traduce borde → posición.
#[derive(Resource, Default)]
pub struct BuiltWalls {
    pub list: Vec<PlacedWall>,
    /// Borde → índice en `list`. Se mantiene al día en `add`/`remove`.
    index: std::collections::HashMap<EdgeKey, usize>,
    /// Sube en cada alta/baja: el streaming lo mira para refrescar al instante
    /// (`is_changed()` no sirve — el sistema de guardado toca el recurso cada
    /// frame y lo marcaría siempre).
    rev: u64,
    dirty: bool,
    was_paused: bool,
    debounce: f32,
}

impl BuiltWalls {
    /// La pared que ocupa ese borde, si hay alguna.
    pub fn at(&self, body: CelestialBody, z: i32, tx: i64, tz: i64, axis: WallAxis) -> Option<&PlacedWall> {
        let idx = *self.index.get(&(body as u8, z, tx, tz, axis.as_u8()))?;
        self.list.get(idx)
    }

    /// ¿Hay alguna pieza de borde en las CUATRO aristas de esta celda? Es lo que
    /// hace que el anillo de paredes de una planta aguante la losa de la de
    /// arriba (`build_grid::floor_supported`): techás tu casa apoyándote en sus
    /// propias paredes, aunque el interior no tenga piso construido.
    pub fn any_edge_of_cell(&self, body: CelestialBody, z: i32, tx: i64, tz: i64) -> bool {
        let h = WallAxis::H.as_u8();
        let v = WallAxis::V.as_u8();
        let b = body as u8;
        self.index.contains_key(&(b, z, tx, tz, h))
            || self.index.contains_key(&(b, z, tx, tz + 1, h))
            || self.index.contains_key(&(b, z, tx, tz, v))
            || self.index.contains_key(&(b, z, tx + 1, tz, v))
    }

    /// ¿Ese borde corta el paso? (pared/ventana sí, puerta no).
    pub fn blocks(&self, body: CelestialBody, z: i32, tx: i64, tz: i64, axis: WallAxis) -> bool {
        self.at(body, z, tx, tz, axis)
            .map(|w| w.def().kind.blocks())
            .unwrap_or(false)
    }

    /// Coloca (reemplazando lo que hubiera en ese borde).
    pub fn add(&mut self, w: PlacedWall) {
        let key = edge_key(&w);
        match self.index.get(&key) {
            // Ya había algo en ese borde: se reemplaza en su sitio (el índice de
            // los demás no se mueve).
            Some(&idx) => self.list[idx] = w,
            None => {
                self.index.insert(key, self.list.len());
                self.list.push(w);
            }
        }
        self.dirty = true;
        self.rev += 1;
    }

    /// Quita la pared de un borde; devuelve la que estaba.
    pub fn remove(
        &mut self,
        body: CelestialBody,
        z: i32,
        tx: i64,
        tz: i64,
        axis: WallAxis,
    ) -> Option<PlacedWall> {
        let idx = self.index.remove(&(body as u8, z, tx, tz, axis.as_u8()))?;
        // `swap_remove` es O(1) pero mueve la última al hueco: hay que reapuntar
        // su entrada del índice o la pared movida deja de existir para la
        // colisión (y sigue dibujándose). Es el bug clásico de este patrón.
        let gone = self.list.swap_remove(idx);
        if idx < self.list.len() {
            let moved = edge_key(&self.list[idx]);
            self.index.insert(moved, idx);
        }
        self.dirty = true;
        self.rev += 1;
        Some(gone)
    }
}

/// Sprite vivo de una pared (clave de streaming + pose actual).
#[derive(Component)]
struct WallSprite {
    tx: i64,
    tz: i64,
    axis: WallAxis,
    z: i32,
    body: CelestialBody,
    pose: &'static str,
    piece: usize,
}

/// Puerta de borde: alterna `open`/`closed` por proximidad del jugador.
#[derive(Component)]
struct WallDoor {
    /// Segundos que le quedan a la transición `opening`/`closing` en curso.
    t: f32,
    /// Centro del borde en mundo (x, z) — para la distancia al jugador.
    cx: f32,
    cz: f32,
    art: &'static str,
    open: bool,
}

pub struct WallsPlugin;

impl Plugin for WallsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuiltWalls>()
            .add_systems(Startup, load_walls_on_startup)
            .add_systems(Update, (stream_walls, animate_wall_doors, debounced_walls_save));
    }
}

/// Pose que le toca a la pared según de qué lado está la habitación (dónde hay
/// piso construido). Es lo que hace que el muro del fondo se vea de frente y el
/// del costado de perfil — el usuario recorta cada una en el editor.
fn wall_pose(axis: WallAxis, floor_a: bool, floor_b: bool) -> &'static str {
    match axis {
        // a = celda norte, b = celda sur.
        WallAxis::H => {
            if floor_b && !floor_a {
                "front" // habitación al sur → vemos su pared del fondo
            } else if floor_a && !floor_b {
                "back" // habitación al norte → es su pared de adelante (baja)
            } else {
                "front"
            }
        }
        // a = celda oeste, b = celda este.
        WallAxis::V => {
            if floor_b && !floor_a {
                "left" // habitación al este → pared izquierda
            } else if floor_a && !floor_b {
                "right"
            } else {
                "left"
            }
        }
    }
}

/// Cadena de respaldo de poses: la que toca, la del mismo eje, `center`.
fn pose_chain(axis: WallAxis, pose: &'static str) -> [&'static str; 3] {
    match axis {
        WallAxis::H => [pose, "front", "center"],
        WallAxis::V => [pose, "left", "center"],
    }
}

/// Color de respaldo cuando el slot no tiene arte todavía (el sistema se ve y se
/// prueba igual).
fn fallback_color(p: &EdgePiece) -> Color {
    match p.kind {
        WallKind::Door => Color::srgb(0.30, 0.20, 0.10),
        WallKind::Window => Color::srgb(0.55, 0.65, 0.75),
        WallKind::Wall if p.art.contains("stone") => Color::srgb(0.55, 0.55, 0.58),
        WallKind::Wall => Color::srgb(0.45, 0.31, 0.17),
    }
}

/// Sprite de la pared: primera pose disponible de la cadena, si no el arte plano
/// del slot, si no un quad de color.
fn wall_sprite(art: &ArtLibrary, p: &EdgePiece, poses: [&str; 3], size: Vec2) -> Sprite {
    for name in poses {
        if let Some(s) = art.pose_sprite(p.art, name, size) {
            return s;
        }
    }
    art.sprite_or_color(p.art, fallback_color(p), size)
}

/// Posición y orden de dibujo del sprite de una pared. El pie del sprite se
/// apoya en el borde y crece hacia arriba en pantalla; el Y-sort usa la fila del
/// pie, así el jugador pasa por delante o por detrás como corresponde.
fn wall_transform(tx: i64, tz: i64, axis: WallAxis, size: Vec2) -> Transform {
    let (cx, foot_z) = match axis {
        // Horizontal: pie sobre la propia línea z = tz.
        WallAxis::H => (tx as f32 + 0.5, tz as f32),
        // Vertical: ocupa la celda z ∈ [tz, tz+1]; su pie es el borde sur.
        WallAxis::V => (tx as f32, (tz + 1) as f32),
    };
    let r = render_xy(cx * TILE_M, foot_z * TILE_M);
    Transform::from_xyz(r.x, r.y + size.y * 0.5, sort_z(foot_z * TILE_M))
}

#[allow(clippy::too_many_arguments)]
fn stream_walls(
    mut commands: Commands,
    time: Res<Time>,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    art: Res<ArtLibrary>,
    built: Res<BuiltWalls>,
    player: Query<&Transform, With<Player>>,
    live: Query<(Entity, &WallSprite)>,
    mut acc: Local<f32>,
    mut last_rev: Local<u64>,
) {
    if surface_region_active(&region).is_none() {
        for (e, _) in &live {
            commands.entity(e).despawn_recursive();
        }
        return;
    }
    // Refresco periódico; colocar o quitar una pared se ve al instante.
    *acc += time.delta_secs();
    if *acc < WALL_STREAM_PERIOD && *last_rev == built.rev {
        return;
    }
    *acc = 0.0;
    *last_rev = built.rev;
    let Ok(player_tf) = player.get_single() else {
        return;
    };
    let body = active_surface_body(&ctx, &planet);
    let z = tiles.z;
    let ptx = (player_tf.translation.x / TILE_M).floor() as i64;
    let ptz = (-player_tf.translation.y / TILE_M).floor() as i64;

    // Paredes deseadas (mismo cuerpo/capa, cerca) con la pose que les toca.
    let has_floor = |cx: i64, cz: i64| is_build_floor(tiles.tile_at(body, cx, cz));
    let want: Vec<(&PlacedWall, &'static str)> = built
        .list
        .iter()
        .filter(|w| {
            w.body == body as u8
                && w.z == z
                && (w.tx - ptx).abs() <= WALL_STREAM_RADIUS
                && (w.tz - ptz).abs() <= WALL_STREAM_RADIUS
        })
        .map(|w| {
            let [a, b] = edge_cells(w.tx, w.tz, w.axis);
            (w, wall_pose(w.axis, has_floor(a.0, a.1), has_floor(b.0, b.1)))
        })
        .collect();

    // Despawn de las que ya no aplican (lejos, otra capa, o cambió su pose/pieza).
    let mut alive: Vec<(i64, i64, WallAxis)> = Vec::new();
    for (e, s) in &live {
        let keep = s.body == body
            && s.z == z
            && want.iter().any(|(w, pose)| {
                w.tx == s.tx && w.tz == s.tz && w.axis == s.axis && *pose == s.pose && w.piece == s.piece
            });
        if keep {
            alive.push((s.tx, s.tz, s.axis));
        } else {
            commands.entity(e).despawn_recursive();
        }
    }
    // Spawn de las que faltan.
    for (w, pose) in want {
        if alive.contains(&(w.tx, w.tz, w.axis)) {
            continue;
        }
        let def = w.def();
        let size = Vec2::new(TILE_M, TILE_M * WALL_RENDER_H);
        // La puerta arranca cerrada; su pose la maneja `animate_wall_doors`.
        let poses = match (def.kind, w.axis) {
            (WallKind::Door, WallAxis::H) => ["closed", "front", "center"],
            (WallKind::Door, WallAxis::V) => ["closed", "left", "center"],
            _ => pose_chain(w.axis, pose),
        };
        let mut ent = commands.spawn((
            WallSprite {
                tx: w.tx,
                tz: w.tz,
                axis: w.axis,
                z,
                body,
                pose,
                piece: w.piece,
            },
            wall_sprite(&art, def, poses, size),
            wall_transform(w.tx, w.tz, w.axis, size),
        ));
        if def.kind == WallKind::Door {
            let (cx, cz) = edge_center(w.tx, w.tz, w.axis);
            ent.insert(WallDoor { cx, cz, art: def.art, open: false, t: 0.0 });
        }
        // Un muro con farol/ventana encendida ilumina si el slot lo declara.
        crate::light2d::attach(&mut ent, &art, &[def.art], size);
    }
}

/// Puertas de borde: cerca del jugador → pose `open`, lejos → `closed`. Sólo
/// toca el sprite cuando el estado cambia (cero costo en reposo).
fn animate_wall_doors(
    mut commands: Commands,
    time: Res<Time>,
    art: Res<ArtLibrary>,
    mut sfx: EventWriter<crate::audio::PlaySfxAt>,
    player: Query<&Transform, With<Player>>,
    mut doors: Query<(Entity, &mut WallDoor, &mut Sprite)>,
) {
    let Ok(player_tf) = player.get_single() else {
        return;
    };
    let dt = time.delta_secs();
    let size = Vec2::new(TILE_M, TILE_M * WALL_RENDER_H);
    let ptx = player_tf.translation.x / TILE_M;
    let ptz = -player_tf.translation.y / TILE_M;
    for (ent, mut door, mut sprite) in &mut doors {
        // Misma cadena de estados que las puertas de tile (`building`): reposo →
        // transición → reposo, y la transición corre entera.
        if door.t > 0.0 {
            door.t -= dt;
            if door.t <= 0.0 {
                door.t = 0.0;
                let rest = if door.open { "open" } else { "closed" };
                crate::building::set_door_pose(
                    &mut commands, ent, &art, door.art, rest, &mut sprite, size,
                );
            }
            continue;
        }
        let (dx, dz) = (ptx - door.cx, ptz - door.cz);
        let near = (dx * dx + dz * dz).sqrt() < DOOR_OPEN_DIST;
        if near == door.open {
            continue;
        }
        door.open = near;
        sfx.send(crate::audio::PlaySfxAt(
            if near { crate::audio::Sfx::DoorOpen } else { crate::audio::Sfx::DoorClose },
            render_xy(door.cx * TILE_M, door.cz * TILE_M),
        ));
        let (pose, secs) = match crate::building::door_transition(&art, door.art, near) {
            Some((p, s)) => (p, s),
            None => (if near { "open" } else { "closed" }, 0.0),
        };
        door.t = secs;
        crate::building::set_door_pose(&mut commands, ent, &art, door.art, pose, &mut sprite, size);
    }
}

// --- Persistencia (mirror de structures.json) --------------------------------

/// Formato en disco: la pieza va por **id**, no por índice de catálogo.
#[derive(Serialize, Deserialize, Default)]
struct WallsSave {
    walls: Vec<(u8, i32, i64, i64, u8, String)>,
}

fn walls_path() -> std::path::PathBuf {
    persist_dir().join("walls.json")
}

fn load_walls_on_startup(mut built: ResMut<BuiltWalls>) {
    let Ok(raw) = std::fs::read_to_string(walls_path()) else {
        return;
    };
    let Ok(save) = serde_json::from_str::<WallsSave>(&raw) else {
        return;
    };
    for (body, z, tx, tz, axis, piece) in save.walls {
        // Pieza desconocida (catálogo cambiado) → se descarta esa pared, no se
        // rompe la partida.
        let Some(piece) = edge_piece_index(&piece) else {
            continue;
        };
        // Por `add`, no por `list.push`: es lo único que deja el índice de bordes
        // al día. Una partida cargada por el otro camino se dibujaría entera y no
        // frenaría a nadie.
        built.add(PlacedWall {
            body,
            z,
            tx,
            tz,
            axis: WallAxis::from_u8(axis),
            piece,
        });
    }
    // Cargar no es construir: sin esto la partida recién abierta se reescribiría
    // sola a los 20 s por el `dirty` que deja `add`.
    built.dirty = false;
}

fn debounced_walls_save(time: Res<Time>, mut built: ResMut<BuiltWalls>) {
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
    let save = WallsSave {
        walls: built
            .list
            .iter()
            .map(|w| (w.body, w.z, w.tx, w.tz, w.axis.as_u8(), w.def().id.to_string()))
            .collect(),
    };
    if let Ok(json) = serde_json::to_string(&save) {
        let _ = std::fs::write(walls_path(), json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wall_at(tx: i64, tz: i64, axis: WallAxis) -> PlacedWall {
        PlacedWall { body: 0, z: 0, tx, tz, axis, piece: 0 }
    }

    /// **El índice y la lista no pueden discrepar nunca.**
    ///
    /// `remove` usa `swap_remove`: mueve la ÚLTIMA pared al hueco que dejó la que
    /// se fue. Si no se reapunta su entrada del índice, esa pared queda dibujada
    /// pero invisible para la colisión — o sea, una pared que se atraviesa
    /// caminando, y encima sólo la que estaba al final de la lista. Es la clase de
    /// bug que aparece recién cuando el jugador ya tiene un refugio grande y
    /// demuele una pieza cualquiera.
    #[test]
    fn demoler_una_pared_no_desindexa_a_las_demas() {
        let body = CelestialBody::Earth;
        let mut w = BuiltWalls::default();
        for i in 0..5 {
            w.add(wall_at(i, 0, WallAxis::H));
        }
        // Se quita la PRIMERA: la última pasa a ocupar su lugar en el `Vec`.
        assert!(w.remove(body, 0, 0, 0, WallAxis::H).is_some());
        assert_eq!(w.list.len(), 4);
        for i in 1..5 {
            assert!(w.at(body, 0, i, 0, WallAxis::H).is_some(), "pared {i} perdida");
            assert!(w.blocks(body, 0, i, 0, WallAxis::H), "pared {i} dejo de frenar");
        }
        assert!(w.at(body, 0, 0, 0, WallAxis::H).is_none(), "la demolida sigue ahi");
        // Y todo lo que dice el índice existe de verdad en la lista.
        assert_eq!(w.index.len(), w.list.len());
    }

    /// Colocar sobre un borde ocupado REEMPLAZA (no apila ni duplica).
    #[test]
    fn colocar_sobre_un_borde_ocupado_reemplaza() {
        let body = CelestialBody::Earth;
        let mut w = BuiltWalls::default();
        w.add(wall_at(4, 4, WallAxis::V));
        let mut otra = wall_at(4, 4, WallAxis::V);
        otra.piece = 1;
        w.add(otra);
        assert_eq!(w.list.len(), 1);
        assert_eq!(w.at(body, 0, 4, 4, WallAxis::V).unwrap().piece, 1);
    }

    /// El anillo de paredes de una planta es lo que aguanta la losa de arriba:
    /// `any_edge_of_cell` tiene que ver las CUATRO aristas de la celda.
    #[test]
    fn una_celda_ve_sus_cuatro_aristas() {
        let body = CelestialBody::Earth;
        for (tx, tz, axis) in [
            (7, 7, WallAxis::H), // norte
            (7, 8, WallAxis::H), // sur
            (7, 7, WallAxis::V), // oeste
            (8, 7, WallAxis::V), // este
        ] {
            let mut w = BuiltWalls::default();
            w.add(wall_at(tx, tz, axis));
            assert!(w.any_edge_of_cell(body, 0, 7, 7), "{tx},{tz},{axis:?}");
            // Y no ve la de la celda de al lado.
            assert!(!w.any_edge_of_cell(body, 0, 20, 20));
        }
    }
}
