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
use ha_shared::topdown::TILE_M;
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

    /// **¿Hay un techo de prefab sobre esta celda?**
    ///
    /// Un cuarto estampado de una pieza viene con su tapa puesta —para eso es un
    /// prefab—, pero esa tapa vivía en otro registro que el del tumbado por celda,
    /// así que para el resto del juego la habitación estaba **a la intemperie**:
    /// no contaba como techada (`ceilings::covered`), o sea que adentro no se
    /// hacía la penumbra de interior y los muebles no se podían plantar. Que las
    /// dos clases de tapa contesten a la misma pregunta es lo que hace que
    /// "techado" signifique una sola cosa.
    pub fn covers(&self, body: CelestialBody, z: i32, tx: i64, tz: i64) -> bool {
        self.list.iter().any(|r| {
            r.body == body as u8
                && r.z == z
                && tx >= r.min.0
                && tx < r.min.0 + r.w
                && tz >= r.min.1
                && tz < r.min.1 + r.h
        })
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

/// Tamaño con el que se dibuja una puerta de TILE: el de **pieza de obra**
/// (`ArtLibrary::medida_de_obra`), igual que la puerta de borde.
///
/// Que las dos clases de puerta compartan la regla —y no una constante— es lo que
/// evita que la del terreno se vea de otro talle que la que construís. Y la regla
/// es la de obra, no la de objeto: una puerta ocupa una ranura de la grilla, así
/// que su recorte de 1×2 celdas se dibuja de 1×2 tiles. Con la de objeto
/// (× `ESCALA_ARTE`) medía 1,5 de ancho sobre una celda de 1 y se montaba encima
/// de las paredes vecinas.
///
/// El respaldo (`TILE_M × WALL_RENDER_H`) sólo cubre el slot sin pixel asignado.
pub(crate) fn door_render_size(art: &crate::art2d::ArtLibrary, key: &str) -> Vec2 {
    art.medida_de_obra(
        key,
        Some("closed"),
        Vec2::new(TILE_M, TILE_M * ha_shared::build_grid::WALL_RENDER_H),
    )
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
///
/// Recibe una CADENA de candidatas, no un booleano, porque la puerta de borde
/// pide una pose distinta según la cara en la que quedó puesta: la del frente de
/// una casa se abre con la hoja de fachada (`back_opening`) y la de un tabique
/// con la de interior (`opening`). Ver `walls::pose_de_puerta`.
pub(crate) fn door_transition(
    art: &ArtLibrary,
    key: &str,
    poses: &[&'static str],
) -> Option<(&'static str, f32)> {
    let pose = *poses.iter().find(|p| art.has_pose(key, p))?;
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

/// Cuánto se puede agrandar una fachada sobre su tamaño dibujado. Ver
/// [`roof_layout`]: pasado esto, lo que se ve es el pixel ampliado, no el
/// edificio.
pub(crate) const AGRANDE_MAX: f32 = 1.4;

/// **Cuánto mide en pantalla la fachada de un lote, y dónde apoya.**
///
/// Devuelve `(tamaño, alto_extra)`: el tamaño con el que se dibuja el sprite y
/// cuánto sobresale por arriba de la huella edificada.
///
/// Las fachadas de Modern Exteriors están dibujadas en 3/4: el alto del PNG es
/// techo **más** pared de frente, mientras que la huella del lote es sólo el
/// suelo que ocupa la casa. Estiradas al rectángulo del suelo —que es lo que
/// hacía `world_sprite(key, rect)`— salían aplastadas al 50 % de su alto: casas
/// derretidas, con el techo comido y las ventanas ovaladas.
///
/// Así que la fachada se dibuja **a su propia proporción** (celdas del pack ×
/// [`crate::art2d::ESCALA_ARTE`], la escala de todo el resto del pack) y se
/// APOYA en el borde de adelante de la huella, creciendo hacia arriba. Es la
/// misma regla con la que se dibuja cualquier objeto alto del mundo: el pie va
/// donde está la cosa y el dibujo sube. El desvanecido al entrar sigue midiéndose
/// contra la huella del suelo, que es por donde se camina.
///
/// Sin arte (`None`), el respaldo ocupa la huella tal cual: un techo plano.
fn roof_layout(art: &ArtLibrary, key: Option<&str>, huella: Vec2) -> (Vec2, f32) {
    let Some(size) = key.and_then(|k| art.draw_tiles(k, None)) else {
        return (huella, 0.0);
    };
    let size = size * TILE_M;
    // El ancho se ajusta a la huella (la casa mide lo que mide el terreno) y el
    // alto acompaña, para no deformar. Que el dibujo suba por encima del lote es
    // correcto: eso es la altura del edificio, no su suelo.
    // **Y con tope: un dibujo no se agranda más allá de su propio pixel.** El
    // pack no tiene una estación de tren entera (viene en piezas modulares), así
    // que el mejor candidato para una parcela de 50 tiles mide 8; sin tope, ese
    // sprite se estiraba ×6 y la estación quedaba como una foto ampliada. Con
    // tope, el edificio se dibuja de su talle y le sobra terreno alrededor —que
    // es exactamente lo que pasa en un predio de estación.
    let k = (huella.x / size.x.max(0.001)).min(AGRANDE_MAX);
    let size = size * k;
    (size, (size.y - huella.y).max(0.0))
}

/// Sprite del techo/fachada del lote, **y su animación si la tiene**.
///
/// Lo segundo importa desde que hay lotes cuyo arte del pack está ANIMADO: el
/// molino de la central eléctrica son cuatro frames girando
/// (`anim/additional_houses_wind_mill`). Sin devolver la animación, el sprite se
/// dibujaba con la tira entera de cuatro molinos pegados en vez de con uno que
/// gira. Es la regla de "usar siempre el arte animado": si el pack lo dibujó
/// animado, el juego lo pide animado.
fn roof_sprite(
    art: &ArtLibrary,
    key: Option<&str>,
    size: Vec2,
) -> (Sprite, Option<crate::art2d::ArtAnimated>) {
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
                return (s, None);
            }
        }
        // **La FACHADA ENTERA del pack.** Los edificios de Modern Exteriors son
        // un solo sprite —techo, ventanas, toldo y puerta en el mismo dibujo—,
        // sin ninguna pose etiquetada `Roof`. Sin esta rama, apuntar al arte
        // correcto no alcanzaba: se caía igual al cuadrado marrón, y por eso
        // todas las casas del mapa se veían como cajas.
        if let Some((s, anim)) = art.world_sprite(key, size) {
            return (s, anim);
        }
    }
    // Fallback: techo plano oscuro (dos tonos para que se lea como techo).
    (Sprite::from_color(Color::srgb(0.36, 0.26, 0.20), size), None)
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
                    let Some((x, z0, w, h, clave)) =
                        ha_shared::city::roofed_rect(body, &s, tx, tz)
                    else {
                        continue;
                    };
                    if !seen.insert((x, z0)) {
                        continue;
                    }
                    want.push((
                        RoofRect { body: body as u8, z: 0, min: (x, z0), w, h, art: None },
                        Some(clave),
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
        // **Una casa distinta por parcela.** El lote dice QUÉ es (`casa_mediana`)
        // y el arte trae hasta doce fachadas para ese qué; cuál le toca a esta
        // parcela lo decide su esquina, así que siempre es la misma casa en el
        // mismo sitio sin guardar un byte por instancia. Sin esto, el barrio
        // entero —el 60 % de la ciudad— era la misma casa fotocopiada.
        let clave = art_key.map(|k| art.variant_key_seeded(k, crate::world2d::celda_semilla(r.min.0, r.min.1)));
        let huella = Vec2::new(r.w as f32 * TILE_M, r.h as f32 * TILE_M);
        let (size, sube) = roof_layout(&art, clave.as_deref(), huella);
        let cx = r.min.0 as f32 * TILE_M + huella.x * 0.5;
        let cz = r.min.1 as f32 * TILE_M + huella.y * 0.5;
        let pos = render_xy(cx, cz);
        // `render_xy` deja +Y hacia el norte, así que "crecer hacia arriba" es
        // sumarle la mitad de lo que sobresale.
        let pos = Vec2::new(pos.x, pos.y + sube * 0.5);
        let (sprite, anim) = roof_sprite(&art, clave.as_deref(), size);
        let mut ent = commands.spawn((
            BuildingRoof { min: r.min, w: r.w, h: r.h, body, z },
            sprite,
            // Banda de techos: encima de todo el mundo Y-sorted y del ghost de
            // build (5.7). Ver `crate::layers` para la escalera completa.
            Transform::from_xyz(pos.x, pos.y, crate::layers::OVERHEAD),
        ));
        if let Some(a) = anim {
            ent.insert(a);
        }
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
                let rest: &[&str] = if door.open { &["open"] } else { &["closed"] };
                set_door_pose(&mut commands, ent, &art, key, rest, &mut sprite, door_render_size(&art, key));
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
        let transicion: &[&'static str] = if near { &["opening"] } else { &["closing"] };
        let (pose, secs) = match door_transition(&art, key, transicion) {
            Some((p, s)) => (p, s),
            None => (if near { "open" } else { "closed" }, 0.0),
        };
        door.t = secs;
        set_door_pose(&mut commands, ent, &art, key, &[pose], &mut sprite, door_render_size(&art, key));
    }
}

/// Pone una pose en un sprite de puerta y le engancha (o le quita) el animador.
/// Las transiciones corren en modo `once`: llegan al último frame y se clavan
/// ahí, así la puerta no "late" mientras está abierta.
///
/// Recibe una CADENA y usa la primera que exista. Antes recibía una sola pose y
/// **salía sin hacer nada si faltaba**, o sea que la puerta se quedaba congelada
/// en la pose anterior sin decir una palabra. Con las fachadas eso deja de ser
/// teórico: hay temas del pack que traen la hoja cerrada y no la abierta.
pub(crate) fn set_door_pose(
    commands: &mut Commands,
    ent: Entity,
    art: &ArtLibrary,
    key: &str,
    poses: &[&str],
    sprite: &mut Sprite,
    size: Vec2,
) {
    let Some(pose) = poses.iter().copied().find(|p| art.has_pose(key, p)) else { return };
    let Some(s) = art.pose_sprite(key, pose, size) else { return };
    // **El fundido no se pierde al cambiar de pose.** `*sprite = s` reemplaza el
    // sprite entero, alfa incluido, y `layers::fade_overhead` tarda unas décimas en
    // volver a bajarlo: la puerta de una fachada, que se abre justo cuando estás
    // pegado a ella, DESTELLABA opaca de 4 m en la cara de la casa cada vez que se
    // abría o se cerraba. Lo cazó el driver `HA_FACHADATEST` midiendo el alfa desde
    // adentro: 0,14 en dos pasadas y 0,74 en la tercera, la que caía justo después
    // de que la puerta se abriera.
    let alfa = sprite.color.alpha();
    *sprite = s;
    sprite.color.set_alpha(alfa);
    match art.pose(key, pose).and_then(|(_, a)| a) {
        Some(anim) => {
            let looping = matches!(pose, "locked");
            commands.entity(ent).try_insert(if looping { anim } else { anim.once() });
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
