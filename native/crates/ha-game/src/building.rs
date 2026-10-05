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
/// Cuántas puertas animadas puede traer un mismo dibujo de fachada
/// (`puerta_en_sprite`, `puerta2_en_sprite`, …). Las publica
/// `scripts/import-puertas-edificios.py`; la base militar trae dos.
const PUERTAS_POR_FACHADA: usize = 4;

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

/// Sprite de puerta que se abre y se cierra al interactuar (lo spawnea
/// `spawn_chunk_tile_art`). Ver `crate::puertas`.
#[derive(Component)]
#[require(crate::puertas::Abrible)]
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
pub(crate) fn door_transition<'a>(
    art: &ArtLibrary,
    key: &str,
    poses: &[&'a str],
) -> Option<(&'a str, f32)> {
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
            .init_resource::<CasaActual>()
            .add_systems(OnEnter(crate::game_phase::GamePhase::Loading), load_roofs_on_startup)
            .add_systems(
                Update,
                (
                    stream_roofs,
                    fade_roofs,
                    animate_doors,
                    animar_puertas_de_fachada,
                    debounced_roofs_save,
                    medir_solidos_de_fachada,
                ),
            );
    }
}

/// **Lo que la fachada dibuja FUERA de la casa, frena** (29 sep 2026).
///
/// La huella de la casa ya tiene sus paredes de verdad (`city::residential_tile`):
/// el frente con su puerta, el fondo y los costados. Pero la fachada del pack es
/// techo y pared en 3/4 y se dibuja más alta (y a veces más ancha) que esa huella:
/// el techo cae sobre las celdas de atrás de la casa. Ahí se caminaba "dentro del
/// dibujo" de la casa, que es lo que el usuario reportó como atravesar la fachada.
/// Ahora esa franja —dibujo menos huella— es sólida. Las puertas están en la
/// huella, así que por ellas se sigue entrando.
#[derive(Component, Clone, Default)]
pub struct SolidoDeFachada(pub Vec<[f32; 4]>);

/// Rectángulos `[x0, z0, x1, z1]` de `dibujo` que quedan fuera de `huella`.
pub fn resta_rect(dibujo: [f32; 4], huella: [f32; 4]) -> Vec<[f32; 4]> {
    let [dx0, dz0, dx1, dz1] = dibujo;
    let [hx0, hz0, hx1, hz1] = huella;
    // Sin solape: todo el dibujo sobra.
    if dx1 <= hx0 || hx1 <= dx0 || dz1 <= hz0 || hz1 <= dz0 {
        return vec![dibujo];
    }
    let mut v = Vec::new();
    if dz0 < hz0 {
        v.push([dx0, dz0, dx1, hz0]); // atrás (norte)
    }
    if dz1 > hz1 {
        v.push([dx0, hz1, dx1, dz1]); // adelante (sur)
    }
    let (z0, z1) = (dz0.max(hz0), dz1.min(hz1));
    if dx0 < hx0 {
        v.push([dx0, z0, hx0, z1]); // oeste
    }
    if dx1 > hx1 {
        v.push([hx1, z0, dx1, z1]); // este
    }
    v.retain(|r| r[2] - r[0] > 0.05 && r[3] - r[1] > 0.05);
    v
}

#[allow(clippy::type_complexity)]
fn medir_solidos_de_fachada(
    mut commands: Commands,
    q: Query<
        (Entity, &BuildingRoof, &Sprite, &Transform),
        (Without<SolidoDeFachada>, Without<PuertaDeFachada>, Without<DoorArt>),
    >,
) {
    for (e, roof, sprite, tf) in &q {
        let Some(size) = sprite.custom_size else { continue };
        let (x, y) = (tf.translation.x, tf.translation.y);
        // Pantalla → mundo: x igual, z = −y.
        let dibujo = [x - size.x * 0.5, -(y + size.y * 0.5), x + size.x * 0.5, -(y - size.y * 0.5)];
        let huella = [
            roof.min.0 as f32 * TILE_M,
            roof.min.1 as f32 * TILE_M,
            (roof.min.0 + roof.w) as f32 * TILE_M,
            (roof.min.1 + roof.h) as f32 * TILE_M,
        ];
        // Sólo lo generado en superficie: un techo construido a mano no frena.
        let v = if roof.z == 0 { resta_rect(dibujo, huella) } else { Vec::new() };
        if crate::flags::var("HA_VER_COLISION").is_ok() {
            eprintln!("ha-fachada: huella {huella:?} dibujo {dibujo:?} → sólido {v:?}");
        }
        commands.entity(e).try_insert(SolidoDeFachada(v));
    }
}

/// ¿La caja toca lo sólido de alguna fachada? (ver [`SolidoDeFachada`]).
pub fn fachada_frena(fachadas: &Query<&SolidoDeFachada>, caja: [f32; 4]) -> bool {
    fachadas.iter().any(|s| {
        s.0.iter().any(|r| r[0] < caja[2] && caja[0] < r[2] && r[1] < caja[3] && caja[1] < r[3])
    })
}

#[cfg(test)]
mod tests_fachada {
    use super::*;

    #[test]
    fn lo_que_sobra_de_la_fachada_es_el_techo_de_atras() {
        // Casa 10×8 con el pie en z=8; fachada 10 de ancho × 12 de alto.
        let v = resta_rect([0.0, -4.0, 10.0, 8.0], [0.0, 0.0, 10.0, 8.0]);
        assert_eq!(v, vec![[0.0, -4.0, 10.0, 0.0]]);
        // Más ancha: también los costados.
        let v = resta_rect([-1.0, -4.0, 11.0, 8.0], [0.0, 0.0, 10.0, 8.0]);
        assert_eq!(v.len(), 3);
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
///
/// **`cubrir`: la fachada TAPA la huella entera**, sin tope. Es para las casas:
/// debajo del sprite están las paredes y el piso de verdad (`city::residential_tile`),
/// y todo lo que el dibujo no alcanza a tapar se ve como interior asomando por
/// los costados o por el fondo —"el interior se sale del exterior"—. Una casa
/// levemente agrandada se lee como casa; una con el piso de adentro a la vista,
/// no. [`elegir_fachada`] ya eligió la variante que menos hay que agrandar.
fn roof_layout(art: &ArtLibrary, key: Option<&str>, huella: Vec2, cubrir: bool) -> (Vec2, f32) {
    let Some(size) = key.and_then(|k| art.draw_tiles(k, None)) else {
        return (huella, 0.0);
    };
    let size = size * TILE_M;
    if cubrir {
        // **La casa que mide su dibujo no se escala**: la sim edificó `huella_de_dibujo(celdas)` y el dibujo va
        // a su tamaño de pack. (Sólo el redondeo a tiles enteros los separa: ≤ medio tile.)
        let exacta = key.and_then(|k| art.celdas_de(k, None)).is_some_and(|c| {
            ha_shared::city::huella_de_dibujo(c.x.round() as i64, c.y.round() as i64)
                == ((huella.x / TILE_M).round() as i64, (huella.y / TILE_M).round() as i64)
        });
        if exacta {
            return (size, (size.y - huella.y).max(0.0));
        }
        let k = (huella.x / size.x.max(0.001)).max(huella.y / size.y.max(0.001));
        let size = size * k;
        return (size, (size.y - huella.y).max(0.0));
    }
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

/// ¿Es vivienda este programa de lote? Las casas llevan interior pisable bajo
/// la fachada, así que su dibujo tiene que taparlo entero (ver [`roof_layout`]).
fn es_vivienda(clave: &str) -> bool {
    ha_shared::city::LOT_PROGRAMS
        .iter()
        .any(|p| p.key == clave && p.district == ha_shared::city::District::Residential)
}

/// **Cuántos tiles libres hay detrás de una casa** antes de la calzada de atrás.
///
/// La fachada crece hacia arriba desde el frente (es techo y pared en 3/4), y un
/// dibujo alto sobre una huella chata terminaba con el tejado encima de la calle
/// de atrás: "casas sobre las calles". Se mide en tres columnas —las dos puntas
/// y el medio— porque la calle puede entrar por un costado.
fn fondo_libre(body: CelestialBody, r: &RoofRect) -> f32 {
    const TOPE: i64 = 40;
    let columnas = [r.min.0, r.min.0 + r.w / 2, r.min.0 + r.w - 1];
    let mut libre = TOPE;
    for x in columnas {
        for d in 1..=libre {
            if ha_shared::geography::civil_kind_at(body, x, r.min.1 - d)
                == Some(ha_shared::geography::Civil::Road)
            {
                libre = d - 1;
                break;
            }
        }
    }
    libre as f32
}

/// **La variante de fachada que le queda a ESTA huella.**
///
/// El pack trae casas de 9 a 23 celdas de ancho bajo una misma clave, y el
/// sorteo por celda no miraba tamaños. Dos resultados, los dos a la vista:
///
/// 1. un dibujo angosto sobre una huella ancha no alcanzaba a taparla ni
///    agrandado al tope, y **el interior asomaba por los costados**;
/// 2. un dibujo alto sobre una huella chata subía tanto que **el tejado quedaba
///    encima de la calle de atrás**.
///
/// Así que se sortea entre las variantes que tapan la huella sin agrandarse más
/// de [`AGRANDE_MAX`] y sin que el tejado pase de la vereda de atrás. Si ninguna
/// cumple las dos, se relaja primero el agrandado (lo cubre `roof_layout` con
/// `cubrir`) y después el fondo: nunca se deja de dibujar la casa.
fn elegir_fachada(art: &ArtLibrary, base: &str, r: &RoofRect, body: CelestialBody, semilla: u64) -> String {
    let variantes = art.variantes_con_arte(base);
    // **La casa se dibuja SIN ESTIRAR**: la sim edificó el rectángulo que mide un dibujo del pack
    // (`city::huella_de_dibujo`), y acá se elige una variante que mida exactamente eso. Si hay varias del mismo
    // tamaño, decide la semilla de la parcela.
    if es_vivienda(base) {
        let iguales: Vec<&String> = variantes
            .iter()
            .filter(|k| {
                art.celdas_de(k, None).is_some_and(|c| {
                    ha_shared::city::huella_de_dibujo(c.x.round() as i64, c.y.round() as i64) == (r.w, r.h)
                })
            })
            .collect();
        if !iguales.is_empty() {
            let mut h = semilla.wrapping_mul(0x9E37_79B9_7F4A_7C15);
            h ^= h >> 29;
            return iguales[(h % iguales.len() as u64) as usize].clone();
        }
    }
    if variantes.len() <= 1 {
        return art.variant_key_seeded(base, semilla);
    }
    let huella = Vec2::new(r.w as f32, r.h as f32);
    let fondo = fondo_libre(body, r);
    // (cuánto hay que agrandarla para tapar, cuánto sube por detrás), en tiles.
    let medir = |k: &str| {
        let t = art.draw_tiles(k, None)?;
        let escala = (huella.x / t.x.max(0.001)).max(huella.y / t.y.max(0.001));
        Some((escala, t.y * escala - huella.y))
    };
    let medidas: Vec<(&String, f32, f32)> =
        variantes.iter().filter_map(|k| medir(k).map(|(e, s)| (k, e, s))).collect();
    let cabe_atras = |s: f32| s <= fondo;
    let pasadas: [&dyn Fn(f32, f32) -> bool; 3] = [
        &|e, s| e <= AGRANDE_MAX && cabe_atras(s),
        &|_, s| cabe_atras(s),
        &|_, _| true,
    ];
    for ok in pasadas {
        let buenas: Vec<&String> = medidas.iter().filter(|(_, e, s)| ok(*e, *s)).map(|m| m.0).collect();
        if buenas.is_empty() {
            continue;
        }
        // Misma mezcla que `variant_key_seeded`: la misma parcela elige siempre
        // la misma casa, sin guardar nada.
        let mut h = semilla.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        h ^= h >> 29;
        h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h ^= h >> 32;
        return buenas[(h % buenas.len() as u64) as usize].clone();
    }
    art.variant_key_seeded(base, semilla)
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

/// **Los locales que el pack dibujó para la planta baja de un edificio.**
///
/// Es lo que hace que una cuadra comercial se lea como una cuadra: cada local es
/// un rubro distinto, con su vidriera y su cartel, y salen del mismo kit modular
/// que el resto del edificio, así que encastran sin un pixel de corrimiento.
// Sólo bandas de MODULO (7) celdas: las de 6 (discos, galletas, juguetes, cerrada) son el vano sin pilares
// y dejaban un hueco de pared al costado.
pub(crate) const LOCALES: &[&str] = &[
    "edificio/local",
    "edificio/tienda_panaderia",
    "edificio/tienda_carniceria",
    "edificio/tienda_mascotas",
    "edificio/tienda_pesca",
    "edificio/tienda_heladeria",
    "edificio/tienda_vidriera",
    "edificio/tienda_persiana",
];

/// Los tres temas de pared del kit, en el orden de la hoja del pack.
const TEMAS_DE_EDIFICIO: &[&str] = &["ladrillo", "azul", "hormigon"];

/// **Ningún edificio deja ver su interior desde afuera.**
///
/// Lo que el dibujo de la fachada no alcanza a tapar de la huella —el fondo de
/// un lote más hondo que el edificio dibujado, o los costados si el sprite no da
/// el ancho— se cubre con una azotea lisa por debajo (un pelo detrás de la
/// fachada, en su misma banda). El interior se ve al ENTRAR: el techo se
/// desvanece (`fade_roofs`) igual que el resto porque lleva el mismo
/// [`BuildingRoof`].
fn spawn_azotea_de_respaldo(commands: &mut Commands, r: &RoofRect, body: CelestialBody, z: i32, color: Color) {
    let huella = Vec2::new(r.w as f32 * TILE_M, r.h as f32 * TILE_M);
    let centro = render_xy(
        r.min.0 as f32 * TILE_M + huella.x * 0.5,
        r.min.1 as f32 * TILE_M + huella.y * 0.5,
    );
    let z_frente_mundo = (r.min.1 + r.h) as f32 * TILE_M;
    commands.spawn((
        BuildingRoof { min: r.min, w: r.w, h: r.h, body, z },
        Sprite::from_color(color, huella),
        Transform::from_xyz(centro.x, centro.y, crate::layers::fachada(z_frente_mundo) - crate::layers::HAIR),
    ));
}

/// **La ciudad levanta un edificio del kit modular, del ancho de la parcela.**
///
/// El pack dibujó `5_Floor_Modular_Building` para apilarse: bandas de
/// [`MODULO`](ha_shared::edificio::MODULO) celdas de frente —planta baja, piso,
/// azotea— que se encadenan a lo ancho y se apilan a lo alto. Hasta hoy ninguna
/// de esas 343 piezas llegaba al mundo generado, porque `import-edificios.py` le
/// busca a cada lote UNA fachada entera y en esa carpeta no hay ninguna: lo que
/// hay son las piezas para hacerlas. La oficina y la torre del centro terminaban
/// con **el mismo** condominio de `4_Generic_Building`, y la tienda y el shopping
/// con **el mismo** dibujo de mercado.
///
/// Componiendo, el edificio mide lo que mide la parcela y tiene los pisos que le
/// tocan (`city::pisos_modulares`, sembrado en la esquina del lote), así que una
/// manzana del centro sale con torres de alturas distintas sin guardar un byte
/// por edificio. Es el mismo dibujo que ya hace `crate::edificios` para lo que
/// construye el jugador — se comparten las tablas de piezas y el sorteo.
///
/// Devuelve `false` si la parcela no es de las que se componen, para que el
/// llamador siga por el camino de siempre (una fachada, un sprite).
#[allow(clippy::too_many_arguments)]
fn spawn_fachada_apilada(
    commands: &mut Commands,
    art: &ArtLibrary,
    art_key: &str,
    r: &RoofRect,
    body: CelestialBody,
    z: i32,
) -> bool {
    use ha_shared::edificio::{ALTO_AZOTEA, ALTO_PISO, ALTO_PLANTA_BAJA, MODULO};
    let Some(pisos) = ha_shared::city::pisos_modulares_en(body, art_key, r.min.0, r.min.1) else {
        return false;
    };
    // **Cuántos módulos entran, redondeando para ABAJO.** Las parcelas modulares
    // están declaradas para que la huella dé un múltiplo exacto (ver
    // `city::LOTES_MODULARES`), pero `lot_at` recorta el lote a lo que da la
    // manzana: si la parcela quedó corta, el edificio pierde un módulo antes que
    // salirse a la vereda.
    let modulos = (r.w / MODULO).max(1);
    // Centrado en la huella: lo que sobra queda de vereda a los dos lados.
    let x0 = r.min.0 + (r.w - modulos * MODULO) / 2;
    // El tema de pared es del EDIFICIO entero, no de cada banda: un edificio
    // mitad ladrillo y mitad hormigón no es variedad, es un error de armado.
    let tema = crate::edificios::variante(TEMAS_DE_EDIFICIO, r.min.0, r.min.1, 0);
    // La planta baja de un comercio es un LOCAL; la de una torre, una entrada.
    let plantas_bajas =
        if art_key.contains("tienda") || art_key.contains("shopping") || art_key.contains("mercado")
        {
            LOCALES
        } else {
            crate::edificios::PLANTAS_BAJAS
        };
    let pie = (r.min.1 + r.h) as f32 * TILE_M;
    let base = render_xy(0.0, pie);
    let z_base = crate::layers::fachada(pie);
    let alto_dibujado = ALTO_PLANTA_BAJA + pisos as i64 * ALTO_PISO + ALTO_AZOTEA;
    if alto_dibujado < r.h || modulos * MODULO < r.w {
        spawn_azotea_de_respaldo(commands, r, body, z, Color::srgb(0.46, 0.48, 0.58));
    }
    for m in 0..modulos {
        let mx = x0 + m * MODULO;
        let mut alto = 0i64;
        let pila = std::iter::once((plantas_bajas, ALTO_PLANTA_BAJA))
            .chain((0..pisos as i64).map(|_| (crate::edificios::PISOS, ALTO_PISO)))
            .chain(std::iter::once((crate::edificios::AZOTEAS, ALTO_AZOTEA)));
        for (piso, (tabla, celdas)) in pila.enumerate() {
            // **Un edificio es UN edificio**: la planta baja cambia de local por módulo, pero
            // los pisos y la azotea son los mismos en todo el ancho y en toda la altura.
            // Sorteados por módulo Y por piso, una torre salía con un ventanal, sobre una
            // rejilla, sobre un balcón: tres fachadas de tres casas distintas apiladas.
            let pieza = if piso == 0 {
                crate::edificios::variante(tabla, mx, r.min.1, m as usize)
            } else {
                crate::edificios::variante(tabla, r.min.0, r.min.1, 0)
            };
            let clave = art.resolved_key(pieza, Some(tema));
            // **Una celda del pack por TILE.** Es una banda que embaldosa a lo
            // largo del frente, igual que una pared: con la escala de objeto
            // (0,95 m/celda) un módulo de 7 mediría 6,65 m sobre 7 tiles y cada
            // edificio se montaría medio metro sobre su vecino.
            let size = art.medida_de_obra(
                &clave,
                None,
                Vec2::new(MODULO as f32 * TILE_M, celdas as f32 * TILE_M),
            );
            commands.spawn((
                BuildingRoof { min: r.min, w: r.w, h: r.h, body, z },
                art.sprite_or_color(&clave, crate::edificios::color_de_respaldo(piso), size),
                Transform::from_xyz(
                    base.x + (mx as f32 + MODULO as f32 * 0.5) * TILE_M,
                    base.y + alto as f32 * TILE_M + size.y * 0.5,
                    // **El de arriba, un pelo por detrás.** Las bandas se dibujan
                    // apiladas sobre el mismo pie, así que su orden entre hermanas
                    // no lo puede decidir el Y-sort: lo decide este épsilon, mucho
                    // más chico que lo que separa a dos edificios vecinos.
                    z_base + (piso as f32) * 1.0e-6,
                ),
            ));
            alto += celdas;
        }
    }
    true
}

/// **La casa que crece de a módulos.**
///
/// El pack dibuja las casas en hilera como un KIT: un `tramo` de 10 celdas que se
/// repite y un `remate` de 12 que cierra la fila por la derecha con su pared de
/// costado (`scripts/import-casas.py` los mide y los publica como poses del
/// slot). Componer con eso es lo que hace que una fachada mida **exactamente** lo
/// que mide la casa que generó la ciudad, en vez de estirarse hasta 1,4× o
/// quedarse corta y dejar el piso de adentro asomando por los costados.
///
/// Devuelve `(tramo, remate)` en CELDAS del pack, o `None` si este slot no es un
/// kit y se dibuja como siempre (una fachada entera, un sprite).
fn kit_de_hilera(art: &ArtLibrary, key: &str) -> Option<Kit> {
    // **Se pregunta si la pose EXISTE, no cuánto mide.** `celdas_de` cae al
    // tamaño del slot cuando la pose falta —es lo correcto para quien sólo quiere
    // una medida—, así que preguntando por la medida TODA fachada contestaba que
    // era un kit, entraba por este camino, no encontraba `remate` que dibujar y
    // el barrio entero se quedaba sin techos: se veía el piso y los muebles de
    // cada casa desde la vereda.
    if !art.has_pose(key, "tramo") || !art.has_pose(key, "remate") {
        return None;
    }
    let tramo = art.celdas_de(key, Some("tramo"))?;
    let remate = art.celdas_de(key, Some("remate"))?;
    if tramo.x <= 0.0 || remate.x <= 0.0 {
        return None;
    }
    // **El PASO no es el ancho.** Los módulos del pack traen medio sangrado por
    // lado: pegados al ras dejan una ranura de una celda por la que se ve el
    // pasto. Cuánto se pisan lo mide `import-casas.py` y lo publica como una pose
    // en píxeles; sin ella se avanza el ancho entero, que es el comportamiento
    // viejo y se nota.
    let paso = |n| art.has_pose(key, n).then(|| art.celdas_de(key, Some(n))).flatten().map(|v| v.x);
    Some(Kit {
        tramo,
        remate,
        paso_tramo: paso("paso_tramo").unwrap_or(tramo.x),
        paso_remate: paso("paso_remate").unwrap_or(remate.x),
    })
}

/// El kit ya resuelto: medidas del módulo y del remate, y cuánto avanza cada uno.
#[derive(Clone, Copy)]
struct Kit {
    /// Celdas del pack que mide el módulo repetible.
    tramo: Vec2,
    /// Celdas que mide el que cierra la fila.
    remate: Vec2,
    /// Celdas que hay que avanzar tras poner un módulo (ancho menos la solapa).
    paso_tramo: f32,
    paso_remate: f32,
}

/// Una pieza ya ubicada de una fachada compuesta.
struct Pieza {
    /// Pose del slot que se dibuja (`tramo` / `remate`).
    pose: &'static str,
    /// Borde izquierdo y derecho en el mundo, en metros.
    x0: f32,
    x1: f32,
    /// Alto dibujado, en metros.
    alto: f32,
    /// Franja horizontal del recorte que entra (`[u0, u1]`).
    u0: f32,
    u1: f32,
    /// Cuánto se estiró el dibujo para que su paso mida una vivienda. Lo necesita
    /// la puerta: su sitio está medido en píxeles del recorte, así que hay que
    /// llevarlo a la misma escala o cae donde estaría sin estirar.
    escala: f32,
}

/// **Un módulo por VIVIENDA**, de derecha a izquierda.
///
/// La hilera no se reparte libre a lo ancho de la casa: el sim ya la partió en
/// viviendas de `city::ANCHO_DE_HILERA` tiles, cada una con su puerta y su
/// interior, y acá se le dibuja **una fachada a cada una**. Antes se repartía por
/// el ancho del dibujo y salían tres fachadas apoyadas sobre un único caserón —
/// "estás colocando dos casas como si fuera una", y era literal.
///
/// De derecha a izquierda porque el remate es el que trae la pared de costado: la
/// hilera termina donde termina la casa.
///
/// Cada módulo se dibuja de modo que su **paso** (el ancho útil, sin el sangrado
/// que se pisa con el vecino) mida exactamente una vivienda; lo que sobra se
/// monta sobre el módulo de al lado, que es como el pack los encadena.
fn repartir_hilera(kit: Kit, x0: f32, x1: f32, unidad: f32) -> Vec<Pieza> {
    let mut out = Vec::new();
    let n = ((x1 - x0) / unidad).round().max(1.0) as usize;
    for k in 0..n.min(24) {
        let (pose, celdas, paso) = if k == 0 {
            ("remate", kit.remate, kit.paso_remate)
        } else {
            ("tramo", kit.tramo, kit.paso_tramo)
        };
        if paso <= 0.01 || celdas.x <= 0.01 {
            break;
        }
        // La vivienda k-ésima contando desde la derecha.
        let der = x1 - k as f32 * unidad;
        let izq = der - unidad;
        // El dibujo se estira de `paso` a una vivienda; el resto de la pieza
        // (el sangrado) sale por la derecha y se monta sobre su vecina.
        let escala = unidad / (paso * crate::art2d::ESCALA_ARTE * TILE_M);
        let ancho = celdas.x * crate::art2d::ESCALA_ARTE * TILE_M * escala;
        let alto = celdas.y * crate::art2d::ESCALA_ARTE * TILE_M * escala;
        out.push(Pieza { pose, x0: izq, x1: izq + ancho, alto, u0: 0.0, u1: 1.0, escala });
    }
    out
}

/// **La puerta ANIMADA de una fachada**, dibujada encima del paño en el pixel
/// exacto donde el pack la dibujó cerrada.
///
/// No reusa `DoorArt` por una razón concreta: aquélla es la puerta de una CELDA
/// del mundo y se dibuja alta y apoyada (`door_render_size`), que es lo correcto
/// para un vano de obra y lo que arruinaría el calce acá — el batiente tiene que
/// medir lo que mide en la fachada o se ve montado. Y la clave es dinámica
/// (`building/casa_hilera#3`), no un `&'static str`.
#[derive(Component)]
#[require(crate::puertas::Abrible)]
pub struct PuertaDeFachada {
    /// Slot de la fachada: de ahí salen `closed`/`opening`/`open`/`closing`.
    pub art: String,
    /// Prefijo de las poses de ESTA puerta: `""` para la primera, `"puerta2_"`
    /// para la segunda del mismo dibujo, etc.
    pub prefijo: String,
    /// Celda del mundo bajo la puerta, para medirle la cercanía al jugador.
    pub tx: i64,
    pub tz: i64,
    pub open: bool,
    /// Segundos que le quedan a la transición en curso.
    pub t: f32,
    /// Tamaño con el que se dibuja, en metros.
    pub size: Vec2,
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
        // **La casa MODULAR se compone; la entera se dibuja de una.** Ver
        // `kit_de_hilera`: cuando el slot trae `tramo` y `remate`, la fachada se
        // arma del ancho exacto de la huella y no se estira ni un pixel.
        if let Some(kit) = clave.as_deref().and_then(|k| kit_de_hilera(&art, k)) {
            spawn_fachada_modular(&mut commands, &art, clave.as_deref().unwrap(), &r, body, z, kit);
            continue;
        }
        // **Y el edificio que se APILA.** El centro y el comercio no tienen una
        // fachada dibujada entera: tienen el kit de bandas de 9 celdas con el que
        // se arma una torre del ancho y la altura que pida la parcela. Se prueba
        // con la clave del LOTE (no con la variante ya resuelta) porque la tabla
        // que decide esto es la de `city`, que habla de lotes.
        if let Some(k) = art_key {
            if spawn_fachada_apilada(&mut commands, &art, k, &r, body, z) {
                continue;
            }
        }
        // **La fachada que CABE, no una cualquiera.** El sorteo de arriba no
        // mira tamaños: a una huella de 17 le podía tocar un dibujo de 9, y ni
        // agrandado al tope tapaba las paredes. Ver `elegir_fachada`.
        let residencial = art_key.is_some_and(es_vivienda);
        let clave = art_key.map(|k| {
            elegir_fachada(&art, k, &r, body, crate::world2d::celda_semilla(r.min.0, r.min.1))
        });
        let (size, _) = roof_layout(&art, clave.as_deref(), huella, residencial);
        let z_frente_mundo = (r.min.1 + r.h) as f32 * TILE_M;
        let cx = r.min.0 as f32 * TILE_M + huella.x * 0.5;
        let cz = r.min.1 as f32 * TILE_M + huella.y * 0.5;
        let pos = render_xy(cx, cz);
        // `render_xy` deja +Y hacia el norte, así que "crecer hacia arriba" es
        // sumarle la mitad de lo que sobresale.
        // **El dibujo se apoya en el FRENTE de la huella**, no en su centro. Si
        // era más chato que el lote (`sube` = 0), quedaba centrado y dejaba una
        // franja de interior a la vista arriba y otra abajo.
        let sube_real = size.y - huella.y;
        // **La puerta dibujada cae SOBRE la celda `Door`.** El dibujo se centraba
        // en la huella, pero su puerta puede estar corrida dentro de la fachada:
        // el jugador caminaba hacia la puerta pintada y entraba por otra celda.
        // Se corre el paño lo que haga falta (hasta 1,5 tiles; lo que quede
        // destapado lo cubre la azotea de respaldo).
        let corrimiento = clave
            .as_deref()
            .zip(art_key)
            .and_then(|(k, lote)| {
                let celdas = art.celdas_de(k, None)?;
                let (puerta, _) = art.pose(k, "puerta_en_sprite")?;
                if celdas.x <= 0.0 {
                    return None;
                }
                let escala = size.x / (celdas.x * 32.0);
                let puerta_x = (puerta[0] as f32 + puerta[2] as f32 * 0.5) * escala;
                let deseada = (ha_shared::city::puerta_del_frente(lote, r.min.0, r.w) as f32 + 0.5) * TILE_M;
                let actual = pos.x - size.x * 0.5 + puerta_x;
                Some((deseada - actual).clamp(-1.5 * TILE_M, 1.5 * TILE_M))
            })
            .filter(|d| d.abs() > 0.25 * TILE_M)
            .unwrap_or(0.0);
        let pos = Vec2::new(pos.x + corrimiento, pos.y + sube_real * 0.5);
        let (sprite, anim) = roof_sprite(&art, clave.as_deref(), size);
        if clave.is_some() {
            spawn_azotea_de_respaldo(&mut commands, &r, body, z, Color::srgb(0.60, 0.58, 0.54));
        }
        let mut ent = commands.spawn((
            BuildingRoof { min: r.min, w: r.w, h: r.h, body, z },
            sprite,
            // Banda de caras exteriores, Y-sorteada por el pie de la casa: la de
            // más al sur tapa a la de atrás. Ver `crate::layers`.
            //
            // **Lo abierto es SUELO, no cara.** La gasolinera y el estacionamiento no tienen paredes ni
            // interior: se caminan por dentro (surtidores, playa de asfalto). En la banda de fachadas
            // (siempre por encima de los actores) el dibujo TAPABA al jugador en cuanto pisaba la playa.
            // Y-sorteados por el borde de ATRÁS de la huella, el jugador que está dentro queda encima y el
            // que pasa por detrás queda tapado, como con cualquier objeto.
            Transform::from_xyz(
                pos.x,
                pos.y,
                if es_a_cielo_abierto(art_key) {
                    crate::layers::ysort(r.min.1 as f32 * TILE_M)
                } else {
                    crate::layers::fachada(z_frente_mundo)
                },
            ),
        ));
        if let Some(a) = anim {
            ent.insert(a);
        }
        // **Y su puerta, si el pack la dibujó animada.** Vale para las casas
        // ENTERAS igual que para la hilera: `import-casas.py` mide dónde está el
        // batiente dentro del recorte y lo publica en `puerta_en_sprite`. Sin
        // esto, media docena de tiras de puerta compradas se quedaban sin usar y
        // las casas del barrio tenían la puerta pintada y nada más.
        if let Some(clave) = clave.as_deref() {
            spawn_puerta_de_fachada(&mut commands, &art, clave, &r, body, z, pos, size);
        }
    }
}

/// Lotes sin paredes ni interior: se recorren por dentro.
fn es_a_cielo_abierto(art_key: Option<&str>) -> bool {
    matches!(art_key, Some("building/gasolinera" | "lot/estacionamiento"))
}

/// **La puerta animada de una fachada que se dibuja de UN sprite.**
///
/// La hilera compone su fachada de a módulos y ubica la puerta dentro de cada
/// uno; una casa entera no tiene módulos, así que el sitio lo dice el propio slot:
/// la pose `puerta_en_sprite`, con `grid: [1, 1]`, trae en píxeles el
/// desplazamiento del batiente desde la esquina del recorte y su tamaño.
///
/// El único cuidado es la ESCALA: `roof_layout` dibuja la fachada al ancho de la
/// huella (hasta [`AGRANDE_MAX`]), así que el offset hay que llevarlo a la misma
/// escala con la que se dibujó el paño — si no, la puerta cae donde estaría si la
/// casa midiera lo que mide su pixel.
#[allow(clippy::too_many_arguments)]
fn spawn_puerta_de_fachada(
    commands: &mut Commands,
    art: &ArtLibrary,
    clave: &str,
    r: &RoofRect,
    body: CelestialBody,
    z: i32,
    centro: Vec2,
    size: Vec2,
) {
    let z_frente = (r.min.1 + r.h) as f32 * TILE_M;
    let Some(celdas) = art.celdas_de(clave, None) else { return };
    if celdas.x <= 0.0 {
        return;
    }
    // Metros de mundo por pixel del dibujo, tal como quedó dibujado este paño.
    let escala = size.x / (celdas.x * 32.0);
    let izq = centro.x - size.x * 0.5;
    let arriba = centro.y + size.y * 0.5;
    // **Todas las puertas del dibujo**, no sólo la primera: la base militar y el
    // hospital traen dos. Cada una lleva sus poses con su prefijo.
    for n in 1..=PUERTAS_POR_FACHADA {
        let prefijo = if n == 1 { String::new() } else { format!("puerta{n}_") };
        let sitio = if n == 1 { "puerta_en_sprite".to_string() } else { format!("puerta{n}_en_sprite") };
        let Some((puerta, _)) = art.pose(clave, &sitio) else { break };
        let (dx, dy) = (puerta[0] as f32 * escala, puerta[1] as f32 * escala);
        let (pw, ph) = (puerta[2] as f32 * escala, puerta[3] as f32 * escala);
        let (cx, cy) = (izq + dx + pw * 0.5, arriba - dy - ph * 0.5);
        let tam = Vec2::new(pw, ph);
        let Some(sp) = art.pose_sprite(clave, &format!("{prefijo}closed"), tam) else { continue };
        commands.spawn((
            BuildingRoof { min: r.min, w: r.w, h: r.h, body, z },
            PuertaDeFachada {
                art: clave.to_string(),
                prefijo,
                tx: (cx / TILE_M).floor() as i64,
                tz: r.min.1 + r.h - 1,
                open: false,
                t: 0.0,
                size: tam,
            },
            sp,
            Transform::from_xyz(cx, cy, crate::layers::fachada(z_frente) + 1.0e-5 * n as f32),
        ));
    }
}

/// **Planta la fachada de una casa en hilera**: un sprite por módulo, más su
/// puerta animada encima.
///
/// Cada módulo lleva su propio [`BuildingRoof`] con el rect de la casa entera:
/// así `fade_roofs` los desvanece a todos juntos al entrar —es una sola casa— y
/// el streaming los da de baja de una, porque comparten la esquina con la que se
/// deduplican.
#[allow(clippy::too_many_arguments)]
fn spawn_fachada_modular(
    commands: &mut Commands,
    art: &ArtLibrary,
    clave: &str,
    r: &RoofRect,
    body: CelestialBody,
    z: i32,
    kit: Kit,
) {
    let x0 = r.min.0 as f32 * TILE_M;
    let x1 = (r.min.0 + r.w) as f32 * TILE_M;
    // El pie de la fachada va en el borde de ADELANTE de la huella y el dibujo
    // crece hacia arriba: es la misma regla que `roof_layout` (el pie va donde
    // está la cosa; lo que sube es la altura del edificio, no su suelo).
    let z_frente = (r.min.1 + r.h) as f32 * TILE_M;
    let y_pie = render_xy(0.0, z_frente).y;
    // **La cara exterior Y-SORTEA por su pie.** Una fachada mide catorce tiles de
    // alto sobre una huella de diez, así que SIEMPRE se mete sobre el terreno del
    // vecino de atrás: si todas comparten una z plana, cuál tapa a cuál lo decide
    // el orden en que se spawnearon, y aparece media pared ajena flotando encima
    // de la casa. La banda `layers::fachada` existe justo para eso — la de más al
    // sur va adelante, como cualquier cosa del mundo.
    let z_base = crate::layers::fachada(z_frente);
    // **Y el de la derecha va encima.** Los módulos se pisan una celda (la solapa
    // medida) y el pack los dibujó para encadenarse de izquierda a derecha: cada
    // uno tapa el sangrado del anterior. Como se reparten de derecha a izquierda,
    // el z baja un pelo por módulo — un pelo de verdad, mucho más chico que lo que
    // separa a dos casas vecinas, para no saltar por encima de ninguna.
    let unidad = ha_shared::city::ANCHO_DE_HILERA as f32 * TILE_M;
    for (idx, pieza) in repartir_hilera(kit, x0, x1, unidad).into_iter().enumerate() {
        let z_pieza = z_base - idx as f32 * 1.0e-6;
        let ancho = pieza.x1 - pieza.x0;
        let size = Vec2::new(ancho, pieza.alto);
        let Some(sprite) =
            art.pose_sprite_slice(clave, pieza.pose, size, [pieza.u0, 0.0, pieza.u1, 1.0])
        else {
            continue;
        };
        let cx = (pieza.x0 + pieza.x1) * 0.5;
        commands.spawn((
            BuildingRoof { min: r.min, w: r.w, h: r.h, body, z },
            sprite,
            Transform::from_xyz(cx, y_pie + pieza.alto * 0.5, z_pieza),
        ));
        // La puerta del módulo, si el pack la dibujó y el importador la midió.
        // El sitio viaja en una pose con `grid: [1,1]`, o sea en PÍXELES de la
        // página: restándole el origen de la pieza sale el offset exacto.
        let sitio = if pieza.pose == "remate" { "puerta_en_remate" } else { "puerta_en_tramo" };
        let Some((rect_puerta, _)) = art.pose(clave, sitio) else { continue };
        // El rect de esa pose viene con `grid: [1, 1]`, o sea EN PÍXELES, y su
        // esquina es un desplazamiento desde la esquina de la pieza.
        let celda_m = crate::art2d::ESCALA_ARTE * TILE_M * pieza.escala;
        let dx = rect_puerta[0] as f32 / 32.0 * celda_m;
        let dy = rect_puerta[1] as f32 / 32.0 * celda_m;
        let pw = rect_puerta[2] as f32 / 32.0 * celda_m;
        let ph = rect_puerta[3] as f32 / 32.0 * celda_m;
        // Recortada por la izquierda, la puerta del módulo del borde puede quedar
        // fuera de lo que se ve: entonces no se dibuja.
        let px = pieza.x0 + dx;
        let size_p = Vec2::new(pw, ph);
        let Some(sp) = art.pose_sprite(clave, "closed", size_p) else { continue };
        // El pie de la puerta: `dy` se mide desde ARRIBA del recorte de la pieza.
        let y_puerta = y_pie + pieza.alto - dy - ph * 0.5;
        let tx = (px + pw * 0.5) / TILE_M;
        commands.spawn((
            BuildingRoof { min: r.min, w: r.w, h: r.h, body, z },
            PuertaDeFachada {
                art: clave.to_string(),
                prefijo: String::new(),
                tx: tx.floor() as i64,
                tz: r.min.1 + r.h - 1,
                open: false,
                t: 0.0,
                size: size_p,
            },
            sp,
            // Un pelo por delante de SU paño: es la misma banda de techos, así
            // que el orden entre hermanos lo decide este épsilon y nada más.
            Transform::from_xyz(px + pw * 0.5, y_puerta, z_pieza + 0.001),
        ));
    }
}

/// La puerta dibujada en una fachada se abre y se cierra al interactuar con
/// ella (`crate::puertas`), y queda como la dejaste.
///
/// Es el gemelo de [`animate_doors`] para las puertas que viven DENTRO de un
/// sprite de fachada: mismo ciclo de estados y misma fuente de poses, pero
/// conservando el tamaño con el que se plantó (ver [`PuertaDeFachada`]).
fn animar_puertas_de_fachada(
    mut commands: Commands,
    time: Res<Time>,
    art: Res<ArtLibrary>,
    mut sfx: EventWriter<crate::audio::PlaySfxAt>,
    mut puertas: Query<(Entity, &mut PuertaDeFachada, &mut crate::puertas::Abrible, &mut Sprite)>,
) {
    let dt = time.delta_secs();
    for (ent, mut p, mut abrible, mut sprite) in &mut puertas {
        let size = p.size;
        let key = p.art.clone();
        let pre = p.prefijo.clone();
        if p.t > 0.0 {
            abrible.pedido = false;
            p.t -= dt;
            if p.t <= 0.0 {
                p.t = 0.0;
                let rest = format!("{pre}{}", if p.open { "open" } else { "closed" });
                set_door_pose(&mut commands, ent, &art, &key, &[rest.as_str()], &mut sprite, size);
            }
            continue;
        }
        if !std::mem::take(&mut abrible.pedido) {
            continue;
        }
        let abrir = !p.open;
        p.open = abrir;
        sfx.send(crate::audio::PlaySfxAt(
            if abrir { crate::audio::Sfx::DoorOpen } else { crate::audio::Sfx::DoorClose },
            render_xy((p.tx as f32 + 0.5) * TILE_M, (p.tz as f32 + 0.5) * TILE_M),
        ));
        let transicion = format!("{pre}{}", if abrir { "opening" } else { "closing" });
        let reposo = format!("{pre}{}", if abrir { "open" } else { "closed" });
        let (pose, secs) = door_transition(&art, &key, &[transicion.as_str()]).unwrap_or((reposo.as_str(), 0.0));
        p.t = secs;
        set_door_pose(&mut commands, ent, &art, &key, &[pose], &mut sprite, size);
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
/// El rectángulo `[x0, z0, x1, z1]` (tiles) de la casa en la que está el jugador, si está adentro. Lo lee `light2d`: adentro
/// no entra el sol, así que las sombras de lo que hay en esa casa son sólo de contacto (nada de rombos alargados).
#[derive(Resource, Default)]
pub struct CasaActual(pub Option<[f32; 4]>);

fn fade_roofs(
    time: Res<Time>,
    player: Query<&Transform, With<Player>>,
    mut roofs: Query<(&BuildingRoof, &mut Sprite)>,
    mut actual: ResMut<CasaActual>,
) {
    let Ok(player_tf) = player.get_single() else {
        return;
    };
    let mut adentro = None;
    let ptx = player_tf.translation.x / TILE_M;
    let ptz = -player_tf.translation.y / TILE_M;
    let k = 1.0 - (-ROOF_FADE_RATE * time.delta_secs()).exp();
    for (roof, mut sprite) in &mut roofs {
        let inside = ptx >= (roof.min.0 - 1) as f32
            && ptx <= (roof.min.0 + roof.w + 1) as f32
            && ptz >= (roof.min.1 - 1) as f32
            && ptz <= (roof.min.1 + roof.h + 1) as f32;
        if inside {
            adentro = Some([roof.min.0 as f32, roof.min.1 as f32, (roof.min.0 + roof.w) as f32, (roof.min.1 + roof.h) as f32]);
        }
        let target = if inside { ROOF_ALPHA_INSIDE } else { 1.0 };
        let a = sprite.color.alpha();
        if (a - target).abs() > 0.002 {
            sprite.color.set_alpha(a + (target - a) * k);
        }
    }
    if actual.0 != adentro {
        actual.0 = adentro;
    }
}

/// Puertas animadas: cada pedido del jugador (`crate::puertas`) las cambia de
/// estado, y quedan así hasta el próximo. Sólo toca el sprite cuando el estado
/// cambia (cero costo en reposo).
///
/// El estado (abierto/cerrado) lo decide el juego; la pose es sólo cómo se ve.
/// Si la pose es una tira de frames se reproduce en modo `once`: la transición
/// corre entera y se clava en el último frame.
fn animate_doors(
    mut commands: Commands,
    time: Res<Time>,
    art: Res<ArtLibrary>,
    mut sfx: EventWriter<crate::audio::PlaySfxAt>,
    mut doors: Query<(Entity, &mut DoorArt, &mut crate::puertas::Abrible, &mut Sprite)>,
) {
    let dt = time.delta_secs();
    for (ent, mut door, mut abrible, mut sprite) in &mut doors {
        let key = door.art;
        // ¿Terminó la transición que estaba corriendo? → pose de reposo.
        if door.t > 0.0 {
            abrible.pedido = false; // no se cambia de idea a mitad de camino
            door.t -= dt;
            if door.t <= 0.0 {
                door.t = 0.0;
                let rest: &[&str] = if door.open { &["open"] } else { &["closed"] };
                set_door_pose(&mut commands, ent, &art, key, rest, &mut sprite, door_render_size(&art, key));
            }
            continue;
        }
        if !std::mem::take(&mut abrible.pedido) {
            continue;
        }
        let abrir = !door.open;
        door.open = abrir;
        sfx.send(crate::audio::PlaySfxAt(
            if abrir { crate::audio::Sfx::DoorOpen } else { crate::audio::Sfx::DoorClose },
            render_xy((door.tx as f32 + 0.5) * TILE_M, (door.tz as f32 + 0.5) * TILE_M),
        ));
        // Con transición autorada se reproduce entera; sin ella, salto directo.
        let transicion: &[&'static str] = if abrir { &["opening"] } else { &["closing"] };
        let (pose, secs) = match door_transition(&art, key, transicion) {
            Some((p, s)) => (p, s),
            None => (if abrir { "open" } else { "closed" }, 0.0),
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
    // **Cada partida arranca con lo SUYO.** Esto corría en `Startup`, antes de
    // elegir partida: leía la carpeta base y lo de las pruebas viejas aparecía en
    // cualquier partida nueva. Ahora corre al entrar a la partida (`Loading`) y
    // parte de cero; ver `partida.rs`.
    *built = BuiltRoofs::default();
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
