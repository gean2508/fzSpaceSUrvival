//! Edificios con TECHO TRANSLÚCIDO (F4) — entrar por la puerta, sin cortes.
//!
//! Un edificio se dibuja en CAPAS: piso (bajo el jugador, tile art), paredes
//! (tiles Y-sorted), y un TECHO como sprite encima de todo. Cuando el jugador
//! está dentro del footprint, el alpha del techo baja a ~0.15 con fade suave —
//! misma escena, misma cámara, cero loading. Aplica igual a los asentamientos
//! generados (`ha_shared::topdown::building_site_in`, Tierra por tabla) y a lo
//! construido por el jugador (prefabs F5, registrados en `BuiltRoofs`).
//!
//! Puertas: la celda `Tile::Door` es transitable; si el slot `door/wood` tiene
//! poses `open`/`closed`, el sprite (spawneado por `spawn_chunk_tile_art`)
//! alterna por proximidad del jugador.
//!
//! Perf: los techos sólo existen para edificios con chunk activo cerca; el fade
//! es un chequeo de rectángulo por techo. Cero costo sin edificios a la vista.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use ha_assets::ArtLayer;
use ha_shared::topdown::{BuildingSite, TILE_M};
use ha_shared::CelestialBody;

use crate::art2d::ArtLibrary;
use crate::inventory_persist::persist_dir;
use crate::platform::is_app_paused;
use crate::scene::{RegionRuntime, WorldContextRuntime};
use crate::sim::Player;
use crate::world2d::{
    active_surface_body, render_xy, surface_region_active, ActivePlanet, TileWorld,
};

/// Alpha del techo con el jugador adentro (translúcido, se ve el interior).
const ROOF_ALPHA_INSIDE: f32 = 0.15;
/// Velocidad del fade (lerp exponencial por dt).
const ROOF_FADE_RATE: f32 = 6.0;
/// Radio (tiles) alrededor del jugador donde los techos existen como entidades.
const ROOF_STREAM_RADIUS: i64 = 56;
/// Distancia (tiles) a la que una puerta animada se abre.
const DOOR_OPEN_DIST: f32 = 1.7;

/// Un techo construido por el jugador (prefab F5) — persiste en `roofs.json`.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq)]
pub struct RoofRect {
    pub body: u8,
    pub z: i32,
    pub min: (i64, i64),
    pub w: i64,
    pub h: i64,
    /// Slot de arte del prefab (`room/casa_1`) para la pose de capa Techo.
    pub art: Option<&'static str>,
}

/// Techos de estructuras construidas por el jugador. Los generados NO van acá
/// (se derivan del hash del sitio, no se persisten).
#[derive(Resource, Default)]
pub struct BuiltRoofs {
    pub list: Vec<RoofRect>,
    dirty: bool,
    was_paused: bool,
    debounce: f32,
}

impl BuiltRoofs {
    pub fn add(&mut self, roof: RoofRect) {
        if !self.list.contains(&roof) {
            self.list.push(roof);
            self.dirty = true;
        }
    }
}

/// Entidad de techo viva, con su rect en tiles para el fade por posición.
#[derive(Component)]
pub struct BuildingRoof {
    min: (i64, i64),
    w: i64,
    h: i64,
    body: CelestialBody,
    z: i32,
}

/// Sprite de puerta animado por proximidad (lo spawnea `spawn_chunk_tile_art`).
#[derive(Component)]
pub struct DoorArt {
    pub tx: i64,
    pub tz: i64,
    pub open: bool,
    /// Clave de arte de ESTA puerta. Antes se asumía `door/wood` a fuerza bruta,
    /// así que una puerta de metal o de piedra se animaba con el sprite de la de
    /// madera (o no se animaba). La clave sale del tile, como todo lo demás.
    pub art: &'static str,
    /// Segundos que le quedan a la transición en curso (`opening`/`closing`).
    /// 0 = ya está en su pose de reposo.
    pub t: f32,
}

/// Tamaño con el que se dibuja una puerta de TILE: un tile de suelo de ancho y
/// `WALL_RENDER_H` de alto, igual que una pared de borde. Que las dos clases de
/// puerta compartan medida es lo que evita que la del terreno se vea enana al
/// lado de la que construís.
pub(crate) fn door_render_size() -> Vec2 {
    Vec2::new(TILE_M, TILE_M * ha_shared::build_grid::WALL_RENDER_H)
}

/// **La cadena de estados de una puerta**: reposo → transición → reposo.
///
/// `closed → opening → open → closing → closed`. Las dos transiciones son
/// OPCIONALES: si el asset no las trae, la puerta salta directo entre reposos y
/// se comporta como siempre. Devuelve `(pose, duración)`; duración 0 = no hay
/// transición que esperar.
///
/// Vive acá y la usan las dos clases de puerta —la de tile (este módulo) y la de
/// borde (`walls`)— para que una puerta se vea igual la hayas puesto como la
/// hayas puesto.
pub(crate) fn door_transition(
    art: &ArtLibrary,
    key: &str,
    opening: bool,
) -> Option<(&'static str, f32)> {
    let pose = if opening { "opening" } else { "closing" };
    if !art.has_pose(key, pose) {
        return None;
    }
    // Cuánto dura: los frames de la propia animación. Así el estado del juego y
    // lo que se ve terminan a la vez, sin un número mágico que se desincronice
    // al recortar la animación de otra manera en el editor.
    let secs = art
        .pose_frames(key, pose)
        .map(|(frames, fps)| frames as f32 / fps.max(0.1))
        .unwrap_or(0.35);
    Some((pose, secs))
}

pub struct BuildingPlugin;

impl Plugin for BuildingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuiltRoofs>()
            .add_systems(Startup, load_roofs_on_startup)
            .add_systems(
                Update,
                (stream_roofs, fade_roofs, animate_doors, debounced_roofs_save),
            );
    }
}

/// Clave de arte del edificio generado según su tamaño (el usuario recorta las
/// poses `base`/`roof` de estos slots en el editor).
fn generated_building_art(site: &BuildingSite) -> &'static str {
    match (site.w, site.h) {
        (5, 4) => "building/casa_chica",
        (7, 6) => "building/casa_mediana",
        _ => "building/tienda",
    }
}

/// Sprite del techo: pose etiquetada Techo del slot, o un quad oscuro de
/// fallback (así el sistema se ve/prueba sin arte asignado).
fn roof_sprite(art: &ArtLibrary, key: Option<&str>, size: Vec2) -> Sprite {
    if let Some(key) = key {
        // El techo puede estar en su propia hoja (edificio armado con varios PNG).
        if let Some((pose, rect_px)) = art.pose_with_layer_named(key, ArtLayer::Roof) {
            if let Some(image) = art.pose_image(key, &pose.to_string()) {
                let mut s = Sprite::from_image(image);
                s.custom_size = Some(size);
                let [x, y, w, h] = rect_px;
                s.rect = Some(Rect::from_corners(
                    Vec2::new(x as f32, y as f32),
                    Vec2::new((x + w) as f32, (y + h) as f32),
                ));
                return s;
            }
        }
    }
    // Fallback: techo plano oscuro (dos tonos para que se lea como techo).
    Sprite::from_color(Color::srgb(0.36, 0.26, 0.20), size)
}

/// Spawn/despawn de techos cerca del jugador: sitios generados (deterministas,
/// z=0) + techos construidos (`BuiltRoofs`, cualquier cuerpo/z).
fn stream_roofs(
    mut commands: Commands,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    art: Res<ArtLibrary>,
    built: Res<BuiltRoofs>,
    player: Query<&Transform, With<Player>>,
    roofs: Query<(Entity, &BuildingRoof)>,
) {
    if surface_region_active(&region).is_none() {
        for (e, _) in &roofs {
            commands.entity(e).despawn_recursive();
        }
        return;
    }
    let Ok(player_tf) = player.get_single() else {
        return;
    };
    let body = active_surface_body(&ctx, &planet);
    let z = tiles.z;
    let ptx = (player_tf.translation.x / TILE_M).floor() as i64;
    let ptz = (-player_tf.translation.y / TILE_M).floor() as i64;

    // Techos deseados alrededor del jugador.
    let mut want: Vec<(RoofRect, Option<&'static str>)> = Vec::new();
    if z == 0 {
        // TECHOS POR PARCELA. Se barre la vecindad del jugador preguntando por el
        // lote de cada celda (`city::roofed_rect`) y se deduplica por esquina.
        //
        // Antes se iteraban MANZANAS con una retícula fija de 34/26/22 tiles
        // copiada a mano de `geography`. Con parcelas de tamaño variable —una
        // universidad ocupa cuatro manzanas, una plaza ninguna— esa retícula ya no
        // describe nada, y peor: era una tercera copia del tamaño de manzana que
        // se desincronizaba en silencio. Ahora el dueño del dato es `city` y acá
        // sólo se pregunta.
        if let Some(s) = ha_shared::geography::settlement_covering(body, ptx, ptz) {
            let mut seen: std::collections::HashSet<(i64, i64)> = std::collections::HashSet::new();
            // Paso de 4 tiles: el edificio más chico mide 5 de lado, así que
            // ninguno se cuela entre dos muestras.
            let step = 4i64;
            let r = ROOF_STREAM_RADIUS;
            for dz in (-r..=r).step_by(step as usize) {
                for dx in (-r..=r).step_by(step as usize) {
                    let (tx, tz) = (ptx + dx, ptz + dz);
                    let Some((x, z0, w, h)) =
                        ha_shared::city::roofed_rect(body, &s, tx, tz)
                    else {
                        continue;
                    };
                    if !seen.insert((x, z0)) {
                        continue;
                    }
                    let site = ha_shared::geography::BuildingSite {
                        min: (x, z0),
                        w,
                        h,
                        door: (x + w / 2, z0 + h - 1),
                    };
                    want.push((
                        RoofRect { body: body as u8, z: 0, min: site.min, w, h, art: None },
                        Some(generated_building_art(&site)),
                    ));
                }
            }
        }
    }
    for r in &built {
        let r = *r;
        if r.body == body as u8
            && r.z == z
            && (r.min.0 + r.w / 2 - ptx).abs() <= ROOF_STREAM_RADIUS
            && (r.min.1 + r.h / 2 - ptz).abs() <= ROOF_STREAM_RADIUS
        {
            want.push((r, r.art));
        }
    }

    // Despawn de techos que ya no aplican (lejos / otro cuerpo / otra capa).
    let mut live: Vec<(i64, i64)> = Vec::new();
    for (e, roof) in &roofs {
        let still = roof.body == body
            && roof.z == z
            && want.iter().any(|(r, _)| r.min == roof.min && (r.w, r.h) == (roof.w, roof.h));
        if still {
            live.push(roof.min);
        } else {
            commands.entity(e).despawn_recursive();
        }
    }
    // Spawn de los que faltan.
    for (r, art_key) in want {
        if live.contains(&r.min) {
            continue;
        }
        let size = Vec2::new(r.w as f32 * TILE_M, r.h as f32 * TILE_M);
        let cx = r.min.0 as f32 * TILE_M + size.x * 0.5;
        let cz = r.min.1 as f32 * TILE_M + size.y * 0.5;
        let pos = render_xy(cx, cz);
        commands.spawn((
            BuildingRoof { min: r.min, w: r.w, h: r.h, body, z },
            roof_sprite(&art, art_key, size),
            // Banda de techos: encima de todo el mundo Y-sorted y del ghost de
            // build (5.7). Ver `crate::layers` para la escalera completa.
            Transform::from_xyz(pos.x, pos.y, crate::layers::OVERHEAD),
        ));
    }
}

impl<'a> IntoIterator for &'a BuiltRoofs {
    type Item = &'a RoofRect;
    type IntoIter = std::slice::Iter<'a, RoofRect>;
    fn into_iter(self) -> Self::IntoIter {
        self.list.iter()
    }
}

/// El corazón de F4: jugador dentro del footprint (+1 de margen) → el techo se
/// desvanece a `ROOF_ALPHA_INSIDE`; afuera → vuelve a opaco. Lerp suave por dt.
fn fade_roofs(
    time: Res<Time>,
    player: Query<&Transform, With<Player>>,
    mut roofs: Query<(&BuildingRoof, &mut Sprite)>,
) {
    let Ok(player_tf) = player.get_single() else {
        return;
    };
    let ptx = player_tf.translation.x / TILE_M;
    let ptz = -player_tf.translation.y / TILE_M;
    let k = 1.0 - (-ROOF_FADE_RATE * time.delta_secs()).exp();
    for (roof, mut sprite) in &mut roofs {
        let inside = ptx >= (roof.min.0 - 1) as f32
            && ptx <= (roof.min.0 + roof.w + 1) as f32
            && ptz >= (roof.min.1 - 1) as f32
            && ptz <= (roof.min.1 + roof.h + 1) as f32;
        let target = if inside { ROOF_ALPHA_INSIDE } else { 1.0 };
        let a = sprite.color.alpha();
        if (a - target).abs() > 0.002 {
            sprite.color.set_alpha(a + (target - a) * k);
        }
    }
}

/// Puertas animadas: cerca del jugador → pose `open`; lejos → `closed`.
/// Sólo toca el sprite cuando el estado cambia (cero costo en reposo).
///
/// El estado (abierto/cerrado) lo decide el juego; la pose es sólo cómo se ve.
/// Si la pose es una tira de frames se reproduce en modo `once`: la transición
/// corre entera y se clava en el último frame, así queda abierta mientras la
/// usás y al alejarte se ve cerrarse completa en vez de saltar de golpe.
fn animate_doors(
    mut commands: Commands,
    time: Res<Time>,
    art: Res<ArtLibrary>,
    mut sfx: EventWriter<crate::audio::PlaySfxAt>,
    player: Query<&Transform, With<Player>>,
    mut doors: Query<(Entity, &mut DoorArt, &mut Sprite)>,
) {
    let Ok(player_tf) = player.get_single() else {
        return;
    };
    let dt = time.delta_secs();
    let ptx = player_tf.translation.x / TILE_M;
    let ptz = -player_tf.translation.y / TILE_M;
    for (ent, mut door, mut sprite) in &mut doors {
        let key = door.art;
        // ¿Terminó la transición que estaba corriendo? → pose de reposo.
        if door.t > 0.0 {
            door.t -= dt;
            if door.t <= 0.0 {
                door.t = 0.0;
                let rest = if door.open { "open" } else { "closed" };
                set_door_pose(&mut commands, ent, &art, key, rest, &mut sprite, door_render_size());
            }
            continue; // mientras se abre/cierra no se cambia de idea a mitad
        }
        let dx = ptx - (door.tx as f32 + 0.5);
        let dz = ptz - (door.tz as f32 + 0.5);
        let near = (dx * dx + dz * dz).sqrt() < DOOR_OPEN_DIST;
        if near == door.open {
            continue;
        }
        door.open = near;
        sfx.send(crate::audio::PlaySfxAt(
            if near { crate::audio::Sfx::DoorOpen } else { crate::audio::Sfx::DoorClose },
            render_xy((door.tx as f32 + 0.5) * TILE_M, (door.tz as f32 + 0.5) * TILE_M),
        ));
        // Con transición autorada se reproduce entera; sin ella, salto directo.
        let (pose, secs) = match door_transition(&art, key, near) {
            Some((p, s)) => (p, s),
            None => (if near { "open" } else { "closed" }, 0.0),
        };
        door.t = secs;
        set_door_pose(&mut commands, ent, &art, key, pose, &mut sprite, door_render_size());
    }
}

/// Pone una pose en un sprite de puerta y le engancha (o le quita) el animador.
/// Las transiciones corren en modo `once`: llegan al último frame y se clavan
/// ahí, así la puerta no "late" mientras está abierta.
pub(crate) fn set_door_pose(
    commands: &mut Commands,
    ent: Entity,
    art: &ArtLibrary,
    key: &str,
    pose: &str,
    sprite: &mut Sprite,
    size: Vec2,
) {
    let Some(s) = art.pose_sprite(key, pose, size) else { return };
    *sprite = s;
    match art.pose(key, pose).and_then(|(_, a)| a) {
        Some(anim) => {
            let looping = matches!(pose, "locked");
            commands.entity(ent).insert(if looping { anim } else { anim.once() });
        }
        None => {
            commands.entity(ent).remove::<crate::art2d::ArtAnimated>();
        }
    }
}

// ---------------------------------------------------------------------------
// Persistencia de techos construidos (junto a tiles.json, mismo debounce)
// ---------------------------------------------------------------------------

/// Formato en disco sin el `&'static str` (clave de arte como String).
#[derive(Serialize, Deserialize, Default)]
struct RoofsSave {
    roofs: Vec<(u8, i32, i64, i64, i64, i64, String)>,
}

fn roofs_path() -> std::path::PathBuf {
    persist_dir().join("roofs.json")
}

/// Resuelve la clave String del guardado a la &'static de `ROOM_ART_KEYS`.
fn intern_room_art(name: &str) -> Option<&'static str> {
    const ROOM_ART_KEYS: &[&str] =
        &["room/room_small", "room/room_medium", "room/room_large", "room/casa_1"];
    ROOM_ART_KEYS.iter().find(|k| **k == name).copied()
}

fn load_roofs_on_startup(mut built: ResMut<BuiltRoofs>) {
    let Ok(raw) = std::fs::read_to_string(roofs_path()) else {
        return;
    };
    let Ok(save) = serde_json::from_str::<RoofsSave>(&raw) else {
        return;
    };
    for (body, z, tx, tz, w, h, art) in save.roofs {
        built.list.push(RoofRect {
            body,
            z,
            min: (tx, tz),
            w,
            h,
            art: intern_room_art(&art),
        });
    }
}

fn debounced_roofs_save(time: Res<Time>, mut built: ResMut<BuiltRoofs>) {
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
    let save = RoofsSave {
        roofs: built
            .list
            .iter()
            .map(|r| {
                (
                    r.body,
                    r.z,
                    r.min.0,
                    r.min.1,
                    r.w,
                    r.h,
                    r.art.unwrap_or("").to_string(),
                )
            })
            .collect(),
    };
    if let Ok(json) = serde_json::to_string(&save) {
        let _ = std::fs::write(roofs_path(), json);
    }
}
