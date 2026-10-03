//! EL UNIVERSO — un solo marco continuo, estilo Space Flight Simulator.
//!
//! Reemplaza al viejo `orbital2d` + el "frame swap" superficie↔órbita. Ya NO hay
//! dos dimensiones ni teletransportes: **hay un solo plano de render** y el
//! jugador nunca cambia de coordenadas al despegar o aterrizar.
//!
//! ## El marco
//!
//! El origen de render es el CENTRO del `ref_body` (el cuerpo cuya superficie
//! estás pisando o sobrevolando). El mundo de tiles de ese cuerpo ya vive en ese
//! origen (`world2d::render_xy(x, z) = (x, -z)`), así que el disco del planeta
//! queda EXACTAMENTE encima del terreno caminable: hacer zoom out revela el
//! planeta del que sos parte, sin cortes.
//!
//! Los demás cuerpos se dibujan en `body_offset(cuerpo, ref, t)` = su posición
//! relativa al `ref_body` en las efemérides vivas. Cuando te acercás a otro
//! cuerpo se REBASA el marco (`ref_body` cambia): todo — jugador, discos, cámara —
//! se desplaza el MISMO delta, así que en pantalla no pasa nada. Ese es el truco
//! que elimina el salto de coordenadas de la versión anterior.
//!
//! ## Altura = zoom
//!
//! La superficie se juega en cenital (top-down), así que la altura es
//! perpendicular a la pantalla: subir NO mueve el sprite, **abre el encuadre**
//! (`view_h_for_alt`). Una sola función continua de la altura gobierna cámara,
//! velocidad lateral y qué nivel de detalle del terreno se ve. Nunca hay un
//! salto que haya que tapar con nubes opacas.
//!
//! ## Los tres niveles de detalle (LOD) del suelo — la continuidad real
//!
//! | capa | quién la dibuja | se ve hasta |
//! |------|-----------------|-------------|
//! | tiles finos (1 m) | `world2d::stream_tile_chunks` | `FINE_OFF_VIEW_H` |
//! | mapa satelital (8 m) | este módulo (`TerrainLod`) | `MED_OFF_VIEW_H` |
//! | disco del planeta | este módulo (`BodyDisc`) | siempre |
//!
//! Cada capa se funde con la de abajo (alpha) y las tres salen del MISMO
//! `topdown_tile`, así que el color y la forma del terreno coinciden: zoom out
//! continuo desde una piedra hasta el sistema solar entero.

use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use ha_shared::topdown::TILE_M;
use ha_shared::{body_atmo_top, body_landable, body_radius, body_walk_radius, CelestialBody};

use crate::camera::CameraZoom;
use crate::solar_system_clock::SolarSystemClock;
use crate::world2d::TileWorld;

// ---------------------------------------------------------------------------
// Escala: altura ↔ encuadre
// ---------------------------------------------------------------------------

/// Encuadre (alto del viewport en metros) pisando el suelo.
pub const GROUND_VIEW_H: f32 = 30.0;
/// Metros de encuadre por metro de altura cerca del suelo: **uno a uno**. O sea, a
/// 200 m de altura ves 200 m de mundo — que es más o menos lo que vería una cámara
/// de verdad mirando hacia abajo desde ahí (~53° de campo vertical). Que la escala
/// sea honesta es la mitad de por qué la subida se lee como una subida y no como un
/// zoom.
///
/// **Era 2.4, y ahí estaba el "se aleja de golpe" del despegue.** El ritmo al que se
/// reencuadra al arrancar es `NEAR_ZOOM_RATE × climb_rate(0) / GROUND_VIEW_H`, o sea
/// el factor por segundo en el exponente: con 2.4 el encuadre se multiplicaba por
/// e^2.4 ≈ **11 en el primer segundo** de vuelo (de ver 30 m a ver 330 m). Eso no es
/// una subida, es un tirón — y era, textual, "al darle a volar la cámara se alejaba
/// de golpe". Con 1.0 el ritmo arranca en ×2.7/s y CAE con la altura, y encima el
/// empuje entra con rampa (`creative_fly::THRUST_RAMP_S`), así que el primer segundo
/// es la mitad de eso. Lo clava el test `el_despegue_no_se_aleja_de_golpe`.
///
/// De paso arregla el vuelo rasante: con 2.4 los tiles finos empezaban a fundirse a
/// **27 m** de altura (`FINE_FADE_VIEW_H`/2.4) y a 58 m ya no quedaba un árbol;
/// ahora eso pasa a 95 y 170 m, que es volar bajo sobre TU mundo y verlo.
const NEAR_ZOOM_RATE: f32 = 1.0;
/// Lejos del suelo el encuadre es PROPORCIONAL a la distancia al centro del
/// cuerpo, así el planeta se lee como un disco que crece al bajar y se achica
/// al subir (SFS). `view_h = (radio + alt) * esto`.
const FAR_ZOOM_FACTOR: f32 = 2.2;
/// Altura a la que el encuadre ya es 100 % el proporcional. Es UN RADIO del cuerpo,
/// no un número fijo: en un mundo de 30 km de radio, quedarse con los 600 m de la
/// escala vieja te sacaba a vista de planeta entero apenas despegabas.
///
/// Era medio radio. Pasarlo a uno reparte el "salto de escala" —de ver tu mundo a
/// verlo como disco hay un ×6 inevitable— sobre casi el doble de tiempo de subida, y
/// con eso el pico del reencuadre baja de ×2.84/s a ×2.63/s. Es el momento más
/// rápido de todo el vuelo (la revelación del planeta) y está bien que se note; lo
/// que no puede es parecer un corte.
fn far_blend_alt(body: CelestialBody) -> f32 {
    body_radius(body).max(400.0)
}
/// Techo de altura (m) — tiene que alcanzar para encuadrar el sistema ENTERO.
///
/// Los 2 000 000 viejos no alcanzaban: el sistema va del Sol (x = −4 000 000) a
/// Neptuno (x = +3 584 000), o sea 7.6 millones de metros de punta a punta, y con
/// ese techo el encuadre topaba en ~4.5 Mm — desde el borde del sistema la mitad
/// del mapa quedaba fuera de pantalla y no había forma de ver hacia dónde ibas.
/// 12 Mm de altura dan un encuadre de ~26 Mm: el sistema entero cabe con aire.
pub const MAX_ALT: f32 = 12_000_000.0;

/// LA función de encuadre: altura sobre la superficie → alto del viewport.
/// Continua y monótona en todo el rango, sin tramos ni saltos — es la que hace
/// que despegar, orbitar y aterrizar sean un solo movimiento de cámara.
pub fn view_h_for_alt(body: CelestialBody, alt: f32) -> f32 {
    let alt = alt.clamp(0.0, MAX_ALT);
    let near = GROUND_VIEW_H + alt * NEAR_ZOOM_RATE;
    let far = (body_radius(body) + alt) * FAR_ZOOM_FACTOR;
    let s = smootherstep(0.0, far_blend_alt(body), alt);
    near + (far - near) * s
}

/// Smootherstep 0→1 (derivadas 1ª y 2ª nulas en los extremos → sin quiebres).
pub fn smootherstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Velocidad de ascenso/descenso (m/s). Crece con la altura para que el zoom se
/// sienta LINEAL: subir de 100 a 200 km cuesta lo mismo que de 100 a 200 m. Sin
/// eso, con un sistema de millones de metros, salir al espacio sería eterno.
///
/// **Los dos números están atados al encuadre, no elegidos sueltos.** Cerca del
/// suelo `view_h = GROUND_VIEW_H + alt·NEAR_ZOOM_RATE`, así que el ritmo al que el
/// suelo se aleja —lo ÚNICO que se ve de una subida en vista cenital— es
///
/// ```text
/// d(ln view)/dt = NEAR_ZOOM_RATE · climb_rate(alt) / view_h(alt)
///               = (CLIMB_BASE + alt/2) / (GROUND_VIEW_H + alt)
/// ```
///
/// Con `CLIMB_BASE = GROUND_VIEW_H / 2` esa fracción vale **exactamente 1/2 a
/// cualquier altura**: el mundo se achica siempre al mismo ritmo percibido (×1.65
/// por segundo) desde que despegás del pasto hasta que el planeta entra entero en
/// pantalla. Ni un tirón al arrancar ni un arrastre después.
///
/// Con los 30 m/s de antes la fracción arrancaba en 1.0 y caía a 0.5, o sea que el
/// primer segundo de vuelo se alejaba al DOBLE del ritmo del resto — el arranque
/// brusco que se reportó. Lo clava `camera::el_despegue_no_se_aleja_de_golpe`.
const CLIMB_BASE: f32 = GROUND_VIEW_H * 0.5;

pub fn climb_rate(alt: f32) -> f32 {
    (CLIMB_BASE + alt.max(0.0) * 0.5).min(40_000.0)
}

/// Velocidad lateral en vuelo (m/s) según la altura EFECTIVA. Calibrada así
/// (29 jul 2026, escala grande):
///
/// | dónde | altura | velocidad | referencia |
/// |-------|--------|-----------|------------|
/// | rasante sobre el bosque | 0 | 45 m/s | 8× caminar, se ve el detalle |
/// | crucero cómodo | 1.4 km | ~95 m/s | **cruzar el planeta ≈ 10 min** |
/// | órbita baja | 30 km | ~1.1 km/s | la Luna en ~7 min |
/// | trayecto a la Luna | ~200 km | ~7 km/s | **Luna en ~50 s** |
/// | interplanetario | > 850 km | 30 km/s (tope) | Marte en ~1 min |
///
/// El tope está por ENCIMA de la velocidad orbital de los planetas (~4 km/s a
/// esta escala): si no, un destino se te escaparía y no habría forma de llegar.
pub fn lateral_speed(alt: f32) -> f32 {
    (45.0 + alt.max(0.0) * 0.035).min(LATERAL_MAX)
}

/// Techo de la velocidad lateral (m/s). Ver [`lateral_speed`].
const LATERAL_MAX: f32 = 30_000.0;

/// Ritmo de crucero en el VACÍO: m/s de más por metro de hueco hasta la superficie
/// más cercana.
///
/// **Por qué existe** (3 oct 2026, "en el espacio tengo que seguirle dando a subir
/// para poder ir hacia la Luna"). Con [`lateral_speed`] sola, irse de lado desde el
/// borde de la Tierra arrancaba a 45 m/s y la velocidad crecía con el hueco a
/// 0,035 por segundo — se duplicaba cada ~20 s. Subir, en cambio, abre la altura a
/// ×1,65 por segundo ([`climb_rate`]). O sea que el stick no te llevaba a ningún
/// lado y el atajo era apretar SUBIR para que la altura arrastrara la velocidad: el
/// "subir" de la Tierra mandaba sobre un viaje que es de lado.
///
/// Con este término el viaje por el vacío tiene su propio ritmo, del orden del de
/// la subida: el hueco crece ×1,6 por segundo al irte y se cierra igual al llegar
/// —la aproximación se frena sola, como en SFS— y la Luna queda a medio minuto de
/// stick sin tocar SUBIR. Sobre el disco el hueco es cero, así que caminar y volar
/// sobre tu mundo (los 10 minutos de cruzarlo) no cambian.
pub const SPACE_CRUISE_RATE: f32 = 0.45;

/// Velocidad lateral en vuelo: la de la altura ([`lateral_speed`]) más la de
/// crucero en el vacío ([`SPACE_CRUISE_RATE`]) por el hueco al cuerpo más cercano.
pub fn cruise_speed(alt: f32, gap: f32) -> f32 {
    (lateral_speed(alt) + gap.max(0.0) * SPACE_CRUISE_RATE).min(LATERAL_MAX)
}

/// El encuadre del vuelo en un punto del marco: el MÍNIMO, entre los cuerpos
/// aterrizables, del encuadre que te daría cada uno con tu altura sobre él.
///
/// [`view_h_for_alt`] depende del cuerpo (la rama lejana es proporcional a su
/// radio), así que evaluarla sólo con el cuerpo de referencia daba un ESCALÓN al
/// rebasar el marco: a mitad de camino a la Luna el encuadre pasaba de golpe del
/// radio de la Tierra al de la Luna (×1,79 medido). La cámara lo suavizaba, pero
/// se veía igual: es el "corte" de ir de la Tierra a la Luna.
///
/// El mínimo de funciones continuas es continuo: cerca de un cuerpo manda él (su
/// hueco es el chico), y en el cruce el relevo es suave, rebase o no.
pub fn flight_view_h(ref_body: CelestialBody, here: Vec2, vert: f32, t: f32) -> f32 {
    let mut best = f32::MAX;
    for &b in CelestialBody::all() {
        if !body_landable(b) {
            continue;
        }
        let gap = ((here - body_offset(b, ref_body, t)).length() - body_walk_radius(b)).max(0.0);
        let alt = (vert * vert + gap * gap).sqrt();
        best = best.min(view_h_for_alt(b, alt));
    }
    if best == f32::MAX {
        view_h_for_alt(ref_body, vert)
    } else {
        best
    }
}

// ---------------------------------------------------------------------------
// LOD del suelo — de una piedra al sistema solar, sin cortes
// ---------------------------------------------------------------------------

/// Los tiles finos (1 m) empiezan a desvanecerse acá…
///
/// 29 jul 2026 — subido de 70 a 95. Con la curva de encuadre (`view_h ≈ 30 +
/// 2.4·alt` cerca del suelo) los 70 m viejos caían a **17 metros de altura**: un
/// vuelo rasante ya empezaba a borrar el bosque, y a 42 m no quedaba ni un árbol.
/// Volar bajo sobre TU mundo tiene que verse como tu mundo.
pub const FINE_FADE_VIEW_H: f32 = 95.0;
/// …y ya no están acá (el anillo streameado dejaría de llenar la pantalla).
///
/// El techo no es libre: es el encuadre que el anillo de chunks todavía puede
/// LLENAR (ver `world2d::stream_tile_chunks`, `range` topa en 12 chunks = 192 m
/// de radio ⇒ cubre pantallas de hasta ~160 m de alto). Se elige de modo que
/// cuando el anillo deja de llenar la vista, la capa fina ya esté al ~5 % de
/// alfa: así el borde del anillo nunca se ve, que es la razón de ser del LOD.
pub const FINE_OFF_VIEW_H: f32 = 170.0;
/// Encuadre a partir del cual existe la malla adaptativa (nunca caminando: sus
/// decenas de miles de quads se reconstruirían con cada bloque que ponés).
const LOD_ON_VIEW_H: f32 = 50.0;

/// Opacidad de los tiles finos (1 = nítidos, 0 = ya los reemplazó la malla).
pub fn fine_tile_alpha(view_h: f32) -> f32 {
    1.0 - smootherstep(FINE_FADE_VIEW_H, FINE_OFF_VIEW_H, view_h)
}

/// Lado del quad de la malla adaptativa para un encuadre dado, en potencias de
/// dos. UNA sola malla que cambia de escalón en vez de tres mallas fijas: cubre
/// desde los 130 m de encuadre hasta el planeta entero con el mismo presupuesto
/// de vértices.
fn lod_tile_m(view_h: f32) -> f32 {
    let want = (view_h / 40.0).max(1.0);
    let level = want.log2().round().clamp(2.0, 10.0); // 4 m … 1024 m
    level.exp2()
}

// ---------------------------------------------------------------------------
// El marco
// ---------------------------------------------------------------------------

/// Cuerpo que está en el ORIGEN del plano de render. Su mundo de tiles y su
/// disco comparten centro, y todo lo demás se dibuja relativo a él.
#[derive(Resource)]
pub struct UniverseFrame {
    pub ref_body: CelestialBody,
}

impl Default for UniverseFrame {
    fn default() -> Self {
        Self { ref_body: CelestialBody::Earth }
    }
}

/// Posición de un punto del sistema (coordenadas absolutas de efemérides) en el
/// plano de render, con `ref_body` en el origen. Es lo que usan los cuerpos
/// menores (`space_fx`), que no son `CelestialBody`.
pub fn body_offset_from_world(p: [f32; 3], ref_body: CelestialBody, t: f32) -> Vec2 {
    let r = ha_shared::solar_ephemeris::body_center_at(ref_body, t);
    Vec2::new(p[0] - r[0], -(p[2] - r[2]))
}

/// Dirección unitaria (render) desde `body` HACIA el Sol, en el instante `t`.
/// Manda la iluminación de todo el sistema.
pub fn sun_dir_from(body: CelestialBody, t: f32) -> Vec2 {
    let sun = ha_shared::solar_ephemeris::sun_position();
    let c = ha_shared::solar_ephemeris::body_center_at(body, t);
    Vec2::new(sun[0] - c[0], -(sun[2] - c[2])).normalize_or_zero()
}

/// Posición de `body` en el plano de render, con `ref_body` en el origen.
pub fn body_offset(body: CelestialBody, ref_body: CelestialBody, t: f32) -> Vec2 {
    let c = ha_shared::solar_ephemeris::body_center_at(body, t);
    let r = ha_shared::solar_ephemeris::body_center_at(ref_body, t);
    Vec2::new(c[0] - r[0], -(c[2] - r[2]))
}

/// El cuerpo aterrizable cuya SUPERFICIE está más cerca de un punto del marco.
/// Devuelve `(cuerpo, distancia al centro)`.
pub fn nearest_landable(p: Vec2, ref_body: CelestialBody, t: f32) -> (CelestialBody, f32) {
    let mut best = (ref_body, f32::MAX);
    for &b in CelestialBody::all() {
        if !body_landable(b) {
            continue;
        }
        let d = (p - body_offset(b, ref_body, t)).length();
        if d - body_walk_radius(b) < best.1 - body_walk_radius(best.0) {
            best = (b, d);
        }
    }
    best
}

/// Altura de la superficie del cuerpo a la que se considera "espacio" para el
/// resto del juego (región, HUD, audio, supervivencia): el techo de su atmósfera
/// y, en los cuerpos sin aire, una fracción de su radio.
///
/// El mínimo viejo eran **60 metros**, y con eso la Luna te echaba al espacio
/// desde la altura de un edificio: a 61 m sobre el regolito la región pasaba a
/// `Space`, el terreno dejaba de streamearse y el marco ya no rebasaba. Era la
/// mitad del "en la Luna me quedaba a 30 metros". Un cuerpo de 8.4 km de radio
/// tiene que tener cielo propio: 5 % del radio (420 m en la Luna), con un piso de
/// 600 m para las rocas chiquitas (Fobos, Deimos).
pub fn space_alt(body: CelestialBody) -> f32 {
    body_atmo_top(body)
        .max(body_radius(body) * 0.05)
        .max(600.0)
}

// ---------------------------------------------------------------------------
// Entidades del cielo
// ---------------------------------------------------------------------------

#[derive(Component)]
struct BodyDisc {
    body: CelestialBody,
    /// La textura del disco se genera del terreno REAL la primera vez que hace
    /// falta (es cara); hasta entonces el disco va con el color de catálogo.
    textured: bool,
}

#[derive(Component)]
struct BodyHalo {
    body: CelestialBody,
}

/// El arco de limbo que se corre hacia el Sol. Es la capa que le saca al halo la lectura
/// de "aro pegado": el aire brilla del lado iluminado y se apaga hacia el terminador.
#[derive(Component)]
struct BodyLimb {
    body: CelestialBody,
}

/// Color del AIRE visto de canto desde el espacio. Ojo: NO es
/// `transition_profile().cover_color`, que es el color de las nubes vistas desde abajo
/// (en la Tierra, casi blanco) y además alimenta las bandas de transición —que no se
/// retunean por estética—. Son dos cosas distintas que estaban compartiendo un valor, y
/// por eso el planeta tenía un borde blanco.
///
/// Acá va la dispersión: la Tierra tiene el limbo AZUL, no blanco.
fn limb_color(body: CelestialBody) -> [f32; 3] {
    match body {
        CelestialBody::Earth => [0.28, 0.55, 0.98],
        CelestialBody::Venus => [0.98, 0.82, 0.48],
        CelestialBody::Mars => [0.92, 0.58, 0.40],
        CelestialBody::Titan => [0.88, 0.70, 0.34],
        CelestialBody::Uranus => [0.55, 0.86, 0.92],
        CelestialBody::Neptune => [0.42, 0.60, 0.98],
        _ => [0.55, 0.70, 0.95],
    }
}

/// La cara nocturna de un cuerpo: una MÁSCARA del mismo círculo que el disco
/// (misma malla, mismo centro), con la sombra pintada según de dónde viene la luz.
///
/// **Era un sprite oscuro desplazado** hacia el anti-Sol, de 2,2 radios, que no
/// estaba recortado al planeta. Dos bugs salían de ahí (3 oct 2026, "al salir de
/// la Tierra y volver a entrar la sombra reemplaza al dibujo de la Tierra"):
///
/// 1. Se salía del disco: una mancha negra sobre el halo y el espacio, del lado
///    de noche.
/// 2. Para el cuerpo que pisás la sombra se centraba en el JUGADOR (por la hora),
///    pero sólo a menos de 1,6 radios del centro; más lejos, por la órbita. Al
///    volver de la Luna, cruzar ese radio la teletransportaba de golpe encima de
///    vos: de noche, el planeta entero se volvía sombra en un frame.
///
/// Ahora la sombra es iluminación de una esfera (`n·L` en bandas de píxel), así
/// que nunca sale del disco, y la luz pasa de "la de la hora" a "la de la órbita"
/// fundida con la distancia ([`night_light_dir`]), sin saltos.
#[derive(Component)]
struct BodyNightSide {
    body: CelestialBody,
    /// Luz cuantizada con la que se pintó la máscara vigente.
    key: Option<[i32; 3]>,
}

/// Lado de la máscara de noche: el mismo que el disco pixelado
/// (`DISC_PIXEL_N`), así los píxeles de la sombra caen sobre los del planeta.
const NIGHT_MASK_N: usize = 128;

/// Pasos de cuantización de la luz por unidad: repintar la máscara (16 k
/// píxeles) sólo cuando la luz se movió algo visible, no cada frame.
const NIGHT_KEY_STEPS: f32 = 48.0;

/// Alto del Sol sobre el plano del sistema visto desde un cuerpo AJENO (seno).
/// Negativo a propósito: deja un creciente algo menor que la mitad, que es como
/// se leía (y se quiere seguir leyendo) un planeta desde fuera de su órbita.
const ORBIT_SUN_ELEV: f32 = -0.25;

/// Dirección de la luz (unitaria; x derecha, y arriba, z hacia la cámara) que
/// ilumina el disco de `body`.
///
/// - Visto de afuera, la de su órbita: el Sol en el plano, algo por detrás.
/// - El cuerpo que pisás obedece al RELOJ (`daylight`, el mismo dato que el velo
///   de la superficie): a pleno día el Sol te queda encima, al atardecer el
///   terminador pasa por tus pies, de noche quedás adentro de la sombra.
/// - Entre una y otra se FUNDE con la distancia del jugador al cuerpo (1 → 1,6
///   radios). Antes era un corte en 1,6 radios: el salto de la sombra.
fn night_light_dir(sun: Vec2, pie: Option<Vec2>, r: f32, daylight: f32) -> Vec3 {
    let (ce, se) = ((1.0 - ORBIT_SUN_ELEV * ORBIT_SUN_ELEV).sqrt(), ORBIT_SUN_ELEV);
    let orbit = Vec3::new(sun.x * ce, sun.y * ce, se);
    let Some(p) = pie else { return orbit };
    let w = 1.0 - smootherstep(r, r * 1.6, p.length());
    if w <= 0.0 {
        return orbit;
    }
    // Normal de la esfera bajo el jugador (si está afuera del disco, en el borde).
    let q = (p / r).clamp_length_max(0.98);
    let n = Vec3::new(q.x, q.y, (1.0 - q.length_squared()).max(0.0).sqrt());
    // Cuán alto está el Sol sobre el jugador: 1 a pleno día, −0,75 a medianoche.
    let c = (-0.75 + 1.75 * daylight).clamp(-1.0, 1.0);
    // Hacia dónde cae: del lado del Sol de verdad, así el terminador queda
    // orientado igual que visto desde afuera.
    let s3 = Vec3::new(sun.x, sun.y, 0.0);
    let mut tang = s3 - n * s3.dot(n);
    if tang.length_squared() < 1e-6 {
        tang = n.any_orthonormal_vector();
    }
    let clock = n * c + tang.normalize() * (1.0 - c * c).max(0.0).sqrt();
    // Giro y no mezcla lineal: interpolar y normalizar pasa cerca de cero cuando
    // las dos luces se oponen, y ahí la dirección pegaría el salto que esto evita.
    let arco = Quat::from_rotation_arc(orbit, clock.normalize());
    (Quat::IDENTITY.slerp(arco, w) * orbit).normalize()
}

/// Pinta la máscara de noche para la luz `l`: tres bandas duras de píxel (como el
/// resto del planeta) y nada fuera del círculo. Fila 0 = arriba.
fn paint_night_mask(data: &mut [u8], l: Vec3) {
    let n = NIGHT_MASK_N;
    for py in 0..n {
        for px in 0..n {
            let u = (px as f32 + 0.5) / n as f32 * 2.0 - 1.0;
            let v = 1.0 - (py as f32 + 0.5) / n as f32 * 2.0;
            let r2 = u * u + v * v;
            let i = (py * n + px) * 4;
            data[i..i + 3].copy_from_slice(&[255, 255, 255]);
            if r2 > 1.0 {
                data[i + 3] = 0;
                continue;
            }
            let ndl = Vec3::new(u, v, (1.0 - r2).sqrt()).dot(l);
            let a = if ndl > 0.10 {
                0.0
            } else if ndl > -0.02 {
                0.33
            } else if ndl > -0.16 {
                0.66
            } else {
                1.0
            };
            data[i + 3] = (a * 255.0) as u8;
        }
    }
}

fn night_key(l: Vec3) -> [i32; 3] {
    [
        (l.x * NIGHT_KEY_STEPS).round() as i32,
        (l.y * NIGHT_KEY_STEPS).round() as i32,
        (l.z * NIGHT_KEY_STEPS).round() as i32,
    ]
}

fn new_night_mask() -> Image {
    let n = NIGHT_MASK_N;
    let mut img = Image::new(
        Extent3d { width: n as u32, height: n as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        vec![0u8; n * n * 4],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    img.sampler = bevy::image::ImageSampler::nearest();
    img
}

#[derive(Component)]
struct OrbitRing {
    body: CelestialBody,
    parent_body: CelestialBody,
}

#[derive(Component)]
struct BodyLabel {
    body: CelestialBody,
}

/// La malla adaptativa de terreno del `ref_body`: UN mesh de quads cuyo lado
/// (`lod_tile_m`) sigue al encuadre. Es el escalón entre los tiles de 1 m y el
/// disco del planeta, y con la escala grande (30 km de radio) es lo que cubre
/// los tres órdenes de magnitud que hay entre uno y otro.
#[derive(Component)]
struct TerrainLod;

/// Estado de la malla adaptativa: de qué cuerpo es, dónde está centrada y con
/// qué lado de quad se construyó.
#[derive(Resource, Default)]
struct TerrainLodState {
    body: Option<CelestialBody>,
    center: Vec2,
    tile_m: f32,
    version: u64,
}

/// Quads por lado de la malla adaptativa. 192² = 37 k quads, un solo draw call;
/// cubre `192 × lado` metros, o sea de 768 m (lado 4) a 196 km (lado 1024).
const LOD_QUADS: i32 = 192;

/// Textura compartida de halo: un disco de alfa que cae suave hacia el borde.
/// Es LA pieza de luz del espacio — se tiñe y se escala, nunca deja un borde.
#[derive(Resource)]
pub struct GlowTexture(pub Handle<Image>);

/// Genera el halo radial una vez. 256², alfa `(1-d)^3`, con muestreo LINEAL.
///
/// El filtrado no es un detalle: sin él, un halo de 128 px estirado a 150 km
/// cuantiza su degradado en escalones de kilómetros y el planeta aparece rodeado
/// de ANILLOS CONCÉNTRICOS grises — el artefacto que hacía que las atmósferas
/// siguieran leyéndose como aros dibujados aunque ya fueran luz.
fn spawn_glow_texture(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    const N: usize = 256;
    let mut data = vec![0u8; N * N * 4];
    for y in 0..N {
        for x in 0..N {
            let u = (x as f32 + 0.5) / N as f32 - 0.5;
            let v = (y as f32 + 0.5) / N as f32 - 0.5;
            let d = ((u * u + v * v).sqrt() * 2.0).min(1.0);
            let i = (y * N + x) * 4;
            data[i] = 255;
            data[i + 1] = 255;
            data[i + 2] = 255;
            // TRAMADO. El alfa tiene 8 bits: estirado a 150 km, cada escalón de
            // 1/255 se convierte en un anillo concéntrico visible alrededor del
            // planeta. Un dither de medio nivel rompe las bandas y el halo se
            // lee como luz continua.
            let dither = (((x * 7 + y * 13) % 16) as f32 / 16.0) - 0.5;
            let a = (1.0 - d).powi(3) * 255.0 + dither;
            data[i + 3] = a.clamp(0.0, 255.0) as u8;
        }
    }
    let mut image = Image::new(
        Extent3d { width: N as u32, height: N as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = bevy::image::ImageSampler::linear();
    commands.insert_resource(GlowTexture(images.add(image)));

    // **El SOL en pixel art**: núcleo claro y bandas hacia el naranja del borde,
    // 64 px, vecino más próximo. Era un círculo plano de un solo color.
    const S: usize = 64;
    let bandas: [(f32, [u8; 3]); 5] = [
        (0.40, [255, 250, 214]),
        (0.62, [255, 232, 150]),
        (0.80, [255, 205, 96]),
        (0.93, [248, 164, 62]),
        (1.00, [226, 118, 44]),
    ];
    let mut sol = vec![0u8; S * S * 4];
    for y in 0..S {
        for x in 0..S {
            let u = (x as f32 + 0.5) / S as f32 - 0.5;
            let v = (y as f32 + 0.5) / S as f32 - 0.5;
            let d = (u * u + v * v).sqrt() * 2.0;
            let i = (y * S + x) * 4;
            if let Some((_, c)) = bandas.iter().find(|(r, _)| d <= *r) {
                sol[i..i + 3].copy_from_slice(c);
                sol[i + 3] = 255;
            }
        }
    }
    let mut img = Image::new(
        Extent3d { width: S as u32, height: S as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        sol,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.sampler = bevy::image::ImageSampler::nearest();
    commands.insert_resource(SunTexture(images.add(img)));
}

/// El disco del Sol en pixel art (ver `spawn_glow_texture`).
#[derive(Resource)]
pub struct SunTexture(pub Handle<Image>);

/// Mantiene la cara nocturna de cada cuerpo mirando en contra del Sol.
fn sync_terminators(
    frame: Res<UniverseFrame>,
    clock: Res<SolarSystemClock>,
    dia: Res<crate::sky2d::DayClock>,
    jugador: Query<&crate::sim::SimState, With<crate::sim::Player>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut night: Query<(&mut BodyNightSide, &mut Transform, &MeshMaterial2d<ColorMaterial>)>,
    mut limb: Query<(&BodyLimb, &mut Transform), Without<BodyNightSide>>,
    mut log_una_vez: Local<u32>,
) {
    let t = clock.t;
    let pie = jugador
        .get_single()
        .ok()
        .map(|s| body_offset_from_world([s.body.x, s.body.y, s.body.z], frame.ref_body, t));
    // El arco de limbo va del lado del Sol. Poco desplazamiento a propósito (0.09 radios):
    // con más, el halo se despega del disco por el lado oscuro y se ve el truco.
    for (l, mut tf) in &mut limb {
        let o = body_offset(l.body, frame.ref_body, t);
        let toward = sun_dir_from(l.body, t) * body_radius(l.body) * 0.09;
        let z = tf.translation.z;
        tf.translation = Vec3::new(o.x + toward.x, o.y + toward.y, z);
    }
    for (mut n, mut tf, mat) in &mut night {
        // La máscara va CENTRADA en el cuerpo, igual que el disco: la sombra se
        // dibuja adentro de la esfera, no como un disco aparte que se corre.
        let o = body_offset(n.body, frame.ref_body, t);
        let z = tf.translation.z; // épsilon por cuerpo — ver `spawn_universe`
        tf.translation = Vec3::new(o.x, o.y, z);
        let r = body_radius(n.body);
        let pie_aca = if n.body == frame.ref_body { pie.map(|p| p - o) } else { None };
        let l = night_light_dir(sun_dir_from(n.body, t), pie_aca, r, dia.daylight());
        let key = night_key(l);
        if n.key == Some(key) {
            continue;
        }
        if *log_una_vez < 1 && crate::flags::var("HA_FLYTEST").is_ok() {
            *log_una_vez += 1;
            eprintln!("ha-terminador: {:?} luz {:?} día {:.2}", n.body, l, dia.daylight());
        }
        // Una imagen NUEVA en vez de mutar la vigente: el material la toma en el
        // mismo frame y no hay un cuadro con la máscara a medio subir. La vieja
        // se libera sola al soltar su handle.
        let mut img = new_night_mask();
        paint_night_mask(&mut img.data, l);
        let h = images.add(img);
        if let Some(m) = materials.get_mut(&mat.0) {
            m.texture = Some(h);
        }
        n.key = Some(key);
    }
}

pub struct UniversePlugin;

impl Plugin for UniversePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(bevy::prelude::OnExit(crate::game_phase::GamePhase::Playing), (crate::salir::reiniciar::<TerrainLodState>,));
        app.init_resource::<UniverseFrame>()
            .init_resource::<TerrainLodState>()
            .init_resource::<TerrainLodJob>()
            .init_resource::<DiscBakeQueue>()
            .init_resource::<DiscBakeRows>()
            .add_systems(Startup, spawn_glow_texture)
            // Los discos planetarios se crean AL EMPEZAR A JUGAR, no en el boot:
            // en splash/menú se dibujaban detrás del panel (con sus halos, anillos
            // y etiquetas) y cualquier parpadeo del panel mostraba el sistema
            // solar. `spawn_glow_texture` sí se queda en `Startup`: es sólo la
            // textura que los discos usan.
            .add_systems(OnEnter(crate::game_phase::GamePhase::Loading), spawn_universe)
            .add_systems(
                Update,
                (
                    sync_universe,
                    bake_body_textures,
                    sync_terminators,
                    sync_body_labels,
                    sync_terrain_lod,
                )
                    .chain()
                    .run_if(crate::game_phase::world_active),
            );
    }
}

/// Un disco por cuerpo + halo de atmósfera + anillo de órbita + etiqueta. Se
/// crean UNA vez y viven siempre: no hay "entrar al espacio", sólo alejarse.
fn spawn_universe(
    mut commands: Commands,
    glow: Res<GlowTexture>,
    sol: Res<SunTexture>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut images: ResMut<Assets<Image>>,
    existing: Query<(), With<BodyDisc>>,
) {
    // Corre al entrar a `Playing`; si algún día se puede volver al menú y jugar
    // otra vez, los discos ya están creados y no se duplican.
    if !existing.is_empty() {
        return;
    }
    for (i, &body) in CelestialBody::all().iter().enumerate() {
        // Desempate de PROFUNDIDAD por cuerpo. La fase transparente de Bevy ordena
        // por z, y con todos los discos exactamente en −20 (y todos los halos en
        // −21) dos cuerpos que se solapan en pantalla —la Luna delante de la
        // Tierra, el cinturón sobre Marte— quedaban empatados: el orden lo decidía
        // el orden de extracción, que puede cambiar de un frame al otro, y el par
        // se INTERCAMBIABA. Eso es parpadeo real, y salta justo donde el jugador lo
        // vio: volando y en el espacio, que es cuando hay varios cuerpos a la vez.
        // Con un épsilon por índice el orden es fijo para siempre.
        let dz = i as f32 * 0.01;
        let r = body_radius(body);
        let base = ha_shared::body_def(body).color;
        let disc_color = if body == CelestialBody::Sun {
            Color::srgb(1.0, 0.88, 0.45)
        } else {
            Color::srgb(base[0], base[1], base[2])
        };
        // La MISMA malla para el disco y su noche: la sombra no puede salirse del
        // planeta porque tiene literalmente su contorno.
        let disc_mesh = meshes.add(Circle::new(r));
        commands.spawn((
            BodyDisc { body, textured: false },
            Mesh2d(disc_mesh.clone()),
            MeshMaterial2d(materials.add(if body == CelestialBody::Sun {
                ColorMaterial { color: Color::WHITE, texture: Some(sol.0.clone()), ..default() }
            } else {
                // El planeta lleva su propia pintura (continentes, nubes, cráteres, bandas…) desde el primer cuadro;
                // el horneado del terreno real lo reemplaza cuando te acercás (`bake_body_textures`).
                ColorMaterial { color: Color::WHITE, texture: Some(images.add(crate::planet_art::textura_de_cuerpo(body))), ..default() }
            })),
            // Detrás de todo el suelo (tiles finos z≈0, satelital z=-1).
            Transform::from_xyz(0.0, 0.0, -20.0 + dz),
        ));

        // LUZ, no aros. La atmósfera y la corona son halos radiales
        // translúcidos (`GlowTexture`): se apagan hacia afuera y no dejan
        // ningún borde duro por más que se agranden. Antes eran dos círculos
        // planos concéntricos y se leían como un dónut gris dibujado encima.
        let atmo = body_atmo_top(body);
        let lum = crate::space_fx::body_luminosity(body);
        if atmo > 0.0 {
            let tint = limb_color(body);
            // Tres capas en vez de dos. La de antes usaba `cover_color` —el color de las
            // NUBES vistas desde abajo, casi blanco en la Tierra— y el resultado era un
            // aro blanco parejo, el mismo grosor y el mismo brillo todo alrededor: se leía
            // como una calcomanía pegada al disco, no como aire.
            //
            // Ahora: dispersión ancha muy tenue, limbo ceñido, y un arco fino que se
            // CORRE hacia el Sol (`BodyLimb`, ver `sync_terminators`). Ese corrimiento es
            // lo que hace la diferencia — en un planeta real el limbo brilla del lado
            // iluminado y se apaga hacia el terminador, y eso es justo lo que rompe la
            // lectura de "aro".
            for (spread, alpha, z, sunward) in [
                (2.35_f32, 0.085_f32, -21.6_f32, false),
                (1.30, 0.20, -21.5, false),
                (1.12, 0.30, -21.4, true),
            ] {
                let mut ent = commands.spawn((
                    BodyHalo { body },
                    Sprite {
                        image: glow.0.clone(),
                        color: Color::srgba(tint[0], tint[1], tint[2], alpha * lum),
                        custom_size: Some(Vec2::splat(r * 2.0 * spread)),
                        ..default()
                    },
                    Transform::from_xyz(0.0, 0.0, z + dz),
                ));
                if sunward {
                    ent.insert(BodyLimb { body });
                }
            }
        } else if body != CelestialBody::Sun {
            // Los cuerpos sin aire igual reflejan: un halo mínimo de su propio
            // color los despega del negro en vez de recortarlos con tijera.
            commands.spawn((
                BodyHalo { body },
                Sprite {
                    image: glow.0.clone(),
                    color: Color::srgba(base[0], base[1], base[2], 0.13 * lum),
                    custom_size: Some(Vec2::splat(r * 2.0 * 1.8)),
                    ..default()
                },
                Transform::from_xyz(0.0, 0.0, -21.4 + dz),
            ));
        }

        // El Sol es la fuente: una corona enorme y cálida, en varias capas, que
        // ilumina de verdad el rincón del sistema donde estés.
        if body == CelestialBody::Sun {
            for (spread, alpha, z) in
                [(9.0_f32, 0.10_f32, -22.4_f32), (4.2, 0.20, -22.0), (2.0, 0.38, -21.6)]
            {
                commands.spawn((
                    BodyHalo { body },
                    Sprite {
                        image: glow.0.clone(),
                        color: Color::srgba(1.0, 0.82, 0.42, alpha),
                        custom_size: Some(Vec2::splat(r * 2.0 * spread)),
                        ..default()
                    },
                    Transform::from_xyz(0.0, 0.0, z + dz),
                ));
            }
        } else {
            // TERMINADOR: la mitad que no mira al Sol está a oscuras. Es una
            // máscara sobre el mismo círculo del disco, repintada cuando la luz
            // se mueve (`sync_terminators` → `night_light_dir`).
            let image = images.add(new_night_mask());
            commands.spawn((
                BodyNightSide { body, key: None },
                Mesh2d(disc_mesh.clone()),
                MeshMaterial2d(materials.add(ColorMaterial {
                    color: Color::srgba(0.01, 0.01, 0.03, 0.62),
                    texture: Some(image),
                    ..default()
                })),
                // Justo encima de SU disco y debajo del disco del cuerpo siguiente
                // (los épsilons van de a 0.01): con −19.5 fijo, la noche de la
                // Tierra oscurecía a la Luna cuando pasaba por delante.
                Transform::from_xyz(0.0, 0.0, -20.0 + dz + 0.005),
            ));
        }

        // Anillo de la órbita real (referencia de mapa, tenue).
        let parent_body = orbit_parent(body);
        if let Some(parent_body) = parent_body {
            commands.spawn((
                OrbitRing { body, parent_body },
                Mesh2d(meshes.add(Annulus::new(1.0, 1.02))),
                MeshMaterial2d(materials.add(ColorMaterial::from_color(Color::srgba(
                    0.55, 0.68, 0.85, 0.07,
                )))),
                Transform::from_xyz(0.0, 0.0, -25.0 + dz),
            ));
        }

        // Etiqueta ASCII (regla de fuente del juego), tamaño de pantalla
        // constante — ver `sync_body_labels`.
        commands.spawn((
            BodyLabel { body },
            Text2d::new(ha_shared::body_def(body).label),
            TextFont { font_size: 13.0, ..default() },
            TextColor(Color::srgba(0.85, 0.88, 0.92, 0.0)),
            Transform::from_xyz(0.0, 0.0, -19.0 + dz),
        ));
    }
}

/// Alrededor de quién orbita cada cuerpo (para el anillo de referencia).
fn orbit_parent(body: CelestialBody) -> Option<CelestialBody> {
    match body {
        CelestialBody::Sun => None,
        CelestialBody::Moon => Some(CelestialBody::Earth),
        CelestialBody::Phobos | CelestialBody::Deimos => Some(CelestialBody::Mars),
        _ => Some(CelestialBody::Sun),
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn sync_universe(
    frame: Res<UniverseFrame>,
    clock: Res<SolarSystemClock>,
    zoom: Res<CameraZoom>,
    mut bake: ResMut<DiscBakeQueue>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut discs: Query<
        (&mut BodyDisc, &mut Transform, &MeshMaterial2d<ColorMaterial>),
        Without<crate::camera::GameCamera>,
    >,
    mut halos: Query<
        (&BodyHalo, &mut Transform),
        (Without<BodyDisc>, Without<OrbitRing>, Without<crate::camera::GameCamera>),
    >,
    mut rings: Query<
        (&OrbitRing, &mut Transform, &Mesh2d, &MeshMaterial2d<ColorMaterial>),
        (Without<BodyDisc>, Without<BodyHalo>, Without<crate::camera::GameCamera>),
    >,
    camera: Query<&Transform, With<crate::camera::GameCamera>>,
) {
    let _probe = crate::debug::probe("sync_universe");
    let t = clock.t;
    let r#ref = frame.ref_body;
    let eye = camera.get_single().map(|c| c.translation.truncate()).unwrap_or(Vec2::ZERO);

    for (mut disc, mut tf, mat) in &mut discs {
        let o = body_offset(disc.body, r#ref, t);
        // La z se CONSERVA (la fijó `spawn_universe` con el épsilon por cuerpo que
        // evita el empate de profundidad): reescribirla a −20.0 los volvía a
        // empatar a todos y devolvía el parpadeo entre discos solapados.
        let z = tf.translation.z;
        tf.translation = Vec3::new(o.x, o.y, z);

        // Encolar la textura de terreno del disco. Antes se generaba ENTERA acá
        // (una por frame): 512² píxeles × 4 muestras de terreno + dos pasadas de
        // desenfoque = más de un millón de evaluaciones de ruido en UN frame, o sea
        // un tirón de cientos de ms cada vez que te acercás a un cuerpo. En el móvil
        // eso se ve como un tranco/parpadeo justo al llegar a un planeta. Ahora la
        // pide y `bake_body_textures` la cocina por franjas — ver ese sistema.
        if !disc.textured && disc.body != CelestialBody::Sun {
            // Sólo vale la pena cuando el disco ya ocupa algo en pantalla…
            let r = body_radius(disc.body);
            let on_screen = r * 2.0 / zoom.live_view_h.max(1.0);
            // …y cuando ESTÁ en pantalla. Este segundo test faltaba, y era el
            // bajón de los primeros dos minutos de partida (§10).
            //
            // `on_screen` sólo compara el TAMAÑO del cuerpo con el encuadre; no
            // mira dónde está. Parado en el pasto, con un encuadre de ~20 m,
            // Neptuno da `on_screen` = 2·24 000 000 / 20 = un millón — o sea que
            // pasaba el test, igual que los otros once cuerpos. Resultado: al
            // entrar a jugar se encolaban **los doce discos de una**, y cada uno
            // son 512² píxeles × 4 muestras de terreno = un millón de
            // evaluaciones de ruido, repartidas de a 32 filas por frame. Doce
            // discos × 16 frames = 192 frames seguidos en los que el horneado se
            // come el frame ENTERO (medido: 125–384 ms cada uno, `ha-sys`), a
            // 3-4 fps. Eso es el "se laggea como dos minutos y después levanta
            // solo": levanta solo cuando la cola se vacía.
            //
            // El disco de un cuerpo que está a 600 000 km no se ve ni como un
            // punto: se textura cuando te acercás, que es justo lo que el diseño
            // original decía ("cuando ya ocupa algo en pantalla") y lo que el
            // test no comprobaba.
            //
            // **Y un TECHO, que es lo que faltaba.** `on_screen` grande no quiere
            // decir "se ve enorme": quiere decir que el encuadre es minúsculo al
            // lado del cuerpo, o sea que estás PARADO EN ÉL. El disco vive en
            // z=−20, detrás de todo el suelo, así que ahí está tapado por el
            // terreno y no se ve ni un píxel — y sin embargo se horneaba, porque
            // para el cuerpo que pisás `dist ≈ r` y el test de arriba se cumple
            // solo. Medido en el Redmi el 24 ago 2026: caminando por el pasto,
            // `bake_body_textures` = 482–568 ms de un frame de 750 ms. Ése era el
            // "el juego es un puto lag".
            //
            // Con el techo, el disco de la Tierra se cocina cuando te despegás lo
            // bastante como para llegar a verlo (`live_view_h` ≥ r/6 ≈ 250 m),
            // que es de sobra antes de que asome, no mientras caminás.
            let dist = (tf.translation.truncate() - eye).length();
            let in_frame = dist < zoom.live_view_h.max(1.0) * 1.5 + r;
            let tapado_por_el_suelo = on_screen > DISCO_TAPADO_SOBRE;
            if on_screen > 0.02 && !tapado_por_el_suelo && in_frame {
                disc.textured = true;
                bake.request(disc.body, mat.0.clone());
            }
        }
    }

    for (halo, mut tf) in &mut halos {
        let o = body_offset(halo.body, r#ref, t);
        let z = tf.translation.z;
        tf.translation = Vec3::new(o.x, o.y, z);
    }

    // Los anillos siguen la órbita VIVA: centro = padre, radio = distancia real
    // al padre en este instante. Coherentes con dónde están los cuerpos.
    let view_h = zoom.live_view_h.max(1.0);
    for (ring, mut tf, mesh, mat) in &mut rings {
        let center = body_offset(ring.parent_body, r#ref, t);
        let here = body_offset(ring.body, r#ref, t);
        let radius = (here - center).length();
        if radius < 1.0 {
            continue;
        }
        // Un anillo sólo se LEE como órbita si entra (más o menos) en pantalla.
        // Muy de cerca su arco cruza la vista como una barra gris que parecía un
        // muro; muy de lejos es un pelo sin información. Fuera de esa ventana se
        // apaga del todo.
        let close = smootherstep(radius * 0.35, radius * 1.1, view_h);
        let far = 1.0 - smootherstep(radius * 12.0, radius * 40.0, view_h);
        if let Some(m) = materials.get_mut(&mat.0) {
            m.color = Color::srgba(0.55, 0.68, 0.85, 0.10 * close * far);
        }
        // z conservada: épsilon por cuerpo (ver `spawn_universe`).
        let z = tf.translation.z;
        tf.translation = Vec3::new(center.x, center.y, z);
        // Grosor ~1 px en pantalla a cualquier zoom: una línea, no una banda.
        let thickness = (view_h * 0.0015).max(radius * 0.0002);
        meshes.insert(
            mesh.0.id(),
            Annulus::new(radius - thickness, radius + thickness).into(),
        );
    }
}

/// Etiquetas de tamaño CONSTANTE en pantalla, que aparecen sólo cuando el
/// cuerpo es chico (a corta distancia estorban) y se apagan del todo pisando
/// la superficie.
fn sync_body_labels(
    frame: Res<UniverseFrame>,
    clock: Res<SolarSystemClock>,
    zoom: Res<CameraZoom>,
    mut labels: Query<(&BodyLabel, &mut Transform, &mut TextColor)>,
) {
    let view_h = zoom.live_view_h.max(1.0);
    let t = clock.t;
    let scale = view_h * 0.0011; // ~13 px a cualquier zoom
    for (label, mut tf, mut color) in &mut labels {
        let r = body_radius(label.body);
        let o = body_offset(label.body, frame.ref_body, t);
        // z conservada: épsilon por cuerpo (ver `spawn_universe`).
        let z = tf.translation.z;
        tf.translation = Vec3::new(o.x, o.y + r + view_h * 0.035, z);
        tf.scale = Vec3::splat(scale);
        // Visible sólo en vista de mapa: aparece cuando el encuadre supera al
        // cuerpo, y desaparece si el disco llena la pantalla.
        let fade = smootherstep(r * 1.4, r * 3.0, view_h);
        let too_far = 1.0 - smootherstep(r * 260.0, r * 700.0, view_h);
        color.0 = Color::srgba(0.85, 0.88, 0.92, 0.6 * fade * too_far);
    }
}

// ---------------------------------------------------------------------------
// Mapa satelital (LOD medio)
// ---------------------------------------------------------------------------

/// Construye/actualiza la malla adaptativa alrededor del jugador. Es lo que
/// evita que al alejarte el terreno se corte en el borde del anillo streameado:
/// los tiles finos se funden en ESTA malla, que tiene el mismo color por venir
/// del mismo `topdown_tile`, y ella misma va bajando de resolución hasta
/// entregarle el relevo al disco del planeta.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn sync_terrain_lod(
    mut commands: Commands,
    frame: Res<UniverseFrame>,
    zoom: Res<CameraZoom>,
    tiles: Res<TileWorld>,
    mut state: ResMut<TerrainLodState>,
    mut job: ResMut<TerrainLodJob>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    player: Query<&Transform, With<crate::sim::Player>>,
    mut lod: Query<
        (Entity, &Mesh2d, &MeshMaterial2d<ColorMaterial>),
        (With<TerrainLod>, Without<crate::sim::Player>),
    >,
) {
    let _probe = crate::debug::probe("sync_terrain_lod");
    let view_h = zoom.live_view_h;
    let body = frame.ref_body;
    let drop_it = |commands: &mut Commands,
                   lod: &Query<
        (Entity, &Mesh2d, &MeshMaterial2d<ColorMaterial>),
        (With<TerrainLod>, Without<crate::sim::Player>),
    >| {
        for (e, _, _) in lod.iter() {
            commands.entity(e).despawn_recursive();
        }
    };

    // Mientras los tiles finos tapan la pantalla entera no se ve, y cada edición
    // de tile obligaría a rehacer sus 37 000 quads: poner un bloque caminando no
    // puede costar eso.
    let Ok(player_tf) = player.get_single() else {
        drop_it(&mut commands, &lod);
        state.body = None;
        job.0 = None;
        return;
    };
    if view_h < LOD_ON_VIEW_H || !body_landable(body) {
        drop_it(&mut commands, &lod);
        state.body = None;
        job.0 = None;
        return;
    }

    // **La malla de OTRO cuerpo no se sigue dibujando** (3 oct 2026). Al rebasar
    // el marco (Tierra → Luna → Tierra) la malla vieja quedaba viva hasta que
    // terminara la nueva —segundos en el celu— con sus vértices en coordenadas del
    // cuerpo anterior: el suelo de la Luna pintado encima del centro de la Tierra.
    // Mejor nada (el disco ya cubre a esta altura) que el planeta equivocado.
    if state.body.is_some_and(|b| b != body) {
        drop_it(&mut commands, &lod);
        state.body = None;
        job.0 = None;
        return;
    }

    let tile_m = lod_tile_m(view_h);
    let span = LOD_QUADS as f32 * tile_m;
    let p = player_tf.translation.truncate();
    let center = quantize(p, tile_m);

    // Cambiar de CUERPO sí cancela lo que haya en curso: la malla a medio cocinar
    // es de un planeta que ya no estás mirando y no sirve para nada (soltar la tarea la cancela).
    if job.0.as_ref().is_some_and(|j| j.body != body) {
        job.0 = None;
    }

    // **UN trabajo a la vez, y se lo deja terminar.** Encolar de nuevo en cuanto el objetivo
    // se movía reiniciaba el horneado sin terminar ninguno. La comparación va contra `state`,
    // que describe la malla que SE VE.
    if job.0.is_none() {
        let needs_rebuild = state.body != Some(body)
            || state.tile_m != tile_m
            || state.version != tiles.version
            || (p - state.center).length() > span * 0.25;
        if needs_rebuild {
            let version = tiles.version;
            let copia = tiles.copia_para_lod();
            let pool = bevy::tasks::AsyncComputeTaskPool::get_or_init(bevy::tasks::TaskPool::default);
            let tarea = pool.spawn(async move {
                let mut bake = LodBake::new(body, center, tile_m, version);
                while !bake.step(&copia, 512) {}
                bake
            });
            job.0 = Some(LodTask { body, center, tile_m, version, tarea });
        }
    }

    let terminado: Option<LodBake> = match job.0.as_mut() {
        Some(j) => bevy::tasks::block_on(bevy::tasks::futures_lite::future::poll_once(&mut j.tarea)),
        None => None,
    };
    if terminado.is_some() {
        job.0 = None;
    }
    let finished = terminado.is_some();

    if let Ok((_, mesh, mat)) = lod.get_single_mut() {
        if let Some(j) = terminado {
            *state = TerrainLodState {
                body: Some(j.body),
                center: j.center,
                tile_m: j.tile_m,
                version: j.version,
            };
            meshes.insert(mesh.0.id(), j.into_mesh());
        }
        if let Some(m) = materials.get_mut(&mat.0) {
            m.color = Color::WHITE;
        }
        return;
    }

    // Todavía no hay malla: se espera a que el primer trabajo termine antes de
    // spawnear. No deja hueco — por debajo de `FINE_FADE_VIEW_H` los tiles finos
    // están opacos y son ellos los que se ven.
    let Some(j) = terminado else {
        return;
    };
    let (jb, jc, jt, jv) = (j.body, j.center, j.tile_m, j.version);
    commands.spawn((
        TerrainLod,
        Mesh2d(meshes.add(j.into_mesh())),
        MeshMaterial2d(materials.add(ColorMaterial::from_color(Color::WHITE))),
        // Entre el disco del planeta (z −20) y los tiles finos (z 0).
        Transform::from_xyz(0.0, 0.0, -1.0),
    ));
    // El estado describe LA MALLA QUE SE VE, así que sale del trabajo que acaba
    // de terminar y no del frame actual: entre que se encoló y ahora, el jugador
    // se movió y `center`/`tile_m` de este frame ya no son los de la malla.
    *state = TerrainLodState {
        body: Some(jb),
        center: jc,
        tile_m: jt,
        version: jv,
    };
}

fn quantize(p: Vec2, tile_m: f32) -> Vec2 {
    let q = tile_m * 8.0;
    Vec2::new((p.x / q).round() * q, (p.y / q).round() * q)
}

/// Malla de `LOD_QUADS²` quads de `tile_m` metros centrada en `center`
/// (coordenadas de RENDER), recortada al disco del cuerpo. El color sale del
/// mismo `TileWorld` que los tiles finos → sin costura al fundirse, y promediado
/// → sin ruido de sal y pimienta al alejarse.
///
/// ## Por qué el color va en los VÉRTICES y no en el quad
///
/// Antes cada quad llevaba su color plano en sus cuatro vértices propios, y eso
/// es exactamente lo que se veía desde el aire: un damero de bloques de borde
/// duro — "el terreno me sale con cuadrados, como cuando no carga la textura".
/// No era arte faltante: es que un color plano por celda ES un cuadrado, y como
/// `lod_tile_m` sigue al encuadre, el damero medía siempre ~40 bloques de alto
/// en pantalla, a cualquier altura.
///
/// Ahora la rejilla comparte vértices y cada vértice lleva SU muestra del
/// terreno: el interpolador de la GPU rellena el resto y el mundo se lee como un
/// mapa pintado —la versión de pocos polígonos del mismo terreno— en vez de como
/// bloques. Y sale más barato que antes: 4 veces menos vértices (se comparten) y
/// las muestras de terreno bajan de `4·n²` a `~n²`, que es el trabajo que costaba
/// el tirón al cambiar de escalón de LOD.
/// ## Por qué se cocina POR FRANJAS
///
/// Muestrear los ~37 000 vértices son 4 llamadas a `topdown_tile` cada uno —
/// ~150 000 evaluaciones de terreno— y hacerlas todas en un frame medía
/// **318 ms** (§10, el pendiente que este trabajo cierra). Como `lod_tile_m`
/// redondea a potencias de dos, subir de la superficie al planeta entero cruza
/// **nueve escalones**: nueve frames de 318 ms, que es exactamente el reporte
/// "al salir al espacio da un tirón en cada cambio".
///
/// Se reparte igual que el disco del planeta (`DiscBakeQueue`): filas de
/// vértices por frame, y **la malla vieja se sigue viendo hasta que la nueva
/// está entera**. El resultado es idéntico al de una pasada —lo clava
/// `la_malla_por_franjas_es_identica_a_la_de_una_pasada`—, sólo que repartido.
struct LodBake {
    body: CelestialBody,
    center: Vec2,
    tile_m: f32,
    version: u64,
    /// Fila de VÉRTICES por la que va el muestreo. `side` = terminado.
    /// Cursor PLANO de vértices (`iy * side + ix`) sobre `side²`. Ver `step`.
    next_v: usize,
    side: usize,
    corner: Vec2,
    radius: f32,
    used: Vec<bool>,
    quads: Vec<(i32, i32)>,
    index_of: Vec<u32>,
    positions: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
}

impl LodBake {
    fn new(body: CelestialBody, center: Vec2, tile_m: f32, version: u64) -> Self {
        // Sonda propia: el armado NO se reparte (ver abajo), así que si algún día
        // pesa hay que verlo separado del muestreo y no adivinarlo.
        let _probe = crate::debug::probe("LodBake::new");
        let radius = body_walk_radius(body);
        let n = LOD_QUADS;
        let side = (n + 1) as usize; // vértices por lado
        let half = n as f32 * tile_m * 0.5;
        let corner = center - Vec2::splat(half);

        // Qué vértices usa alguien: los de los quads que caen DENTRO del disco. Se
        // marca primero para no pagar una muestra de terreno por cada vértice del
        // borde exterior (en un cuerpo chico, la mayoría de la rejilla se descarta).
        //
        // Esta pasada NO se reparte: es geometría pura (una `length()` por quad),
        // sin una sola muestra de terreno, y hay que tenerla entera antes de poder
        // muestrear la primera fila.
        let mut used = vec![false; side * side];
        let mut quads: Vec<(i32, i32)> = Vec::with_capacity((n * n) as usize / 2);
        for iy in 0..n {
            for ix in 0..n {
                let c = corner + Vec2::new((ix as f32 + 0.5) * tile_m, (iy as f32 + 0.5) * tile_m);
                // RECORTE AL CUERPO: la malla no puede salirse del disco, o el
                // planeta se queda sin borde y el suelo llena el espacio.
                if c.length() > radius {
                    continue;
                }
                quads.push((ix, iy));
                for (dx, dy) in [(0, 0), (1, 0), (1, 1), (0, 1)] {
                    used[(iy + dy) as usize * side + (ix + dx) as usize] = true;
                }
            }
        }

        Self {
            body,
            center,
            tile_m,
            version,
            next_v: 0,
            side,
            corner,
            radius,
            used,
            quads,
            index_of: vec![u32::MAX; side * side],
            positions: Vec::with_capacity(side * side / 2),
            colors: Vec::with_capacity(side * side / 2),
        }
    }

    /// Muestrea hasta `verts` VÉRTICES (no filas). Devuelve `true` cuando ya
    /// están todos y la malla se puede armar.
    ///
    /// **La unidad es el vértice y no la fila** por lo mismo que el disco del
    /// planeta cambió a píxeles (§10, 24 ago 2026): una fila son `side` = 193
    /// vértices × 4 muestras de `topdown_tile` + un `tinte_urbano`, y desde que
    /// la sim tiene ciudades eso puede costar cientos de ms. Con la fila como
    /// unidad mínima, cualquier plazo que le pongas arriba es decorativo.
    ///
    /// Quien la llama pone el reloj (ver `sync_terrain_lod`); acá la unidad es
    /// determinista a propósito, que es lo que hace testeable la equivalencia
    /// con el horneado de una sola pasada.
    fn step(&mut self, tiles: &TileWorld, verts: usize) -> bool {
        let total = self.side * self.side;
        let end = self.next_v.saturating_add(verts.max(1)).min(total);
        for i in self.next_v..end {
            let (iy, ix) = (i / self.side, i % self.side);
            {
                if !self.used[iy * self.side + ix] {
                    continue;
                }
                let p = self.corner
                    + Vec2::new(ix as f32 * self.tile_m, iy as f32 * self.tile_m);
                // Los vértices del borde caen FUERA del disco (el quad de adentro los
                // usa igual). Muestrear ahí devolvería el vacío y el planeta quedaría
                // con la orilla lavada: se muestrea el punto de la costa más cercano.
                let s = if p.length() > self.radius {
                    p.normalize_or_zero() * self.radius * 0.999
                } else {
                    p
                };
                // Promediado sobre la celda que rodea al vértice: a 1024 m por quad,
                // UNA muestra es un tile de 1 m elegido al azar — volvería el ruido de
                // sal y pimienta que este promedio existe para matar.
                // **En GRUESO** (`ha_shared::grueso`): sin trazar ríos, que a un
                // vértice cada decenas de metros no se ven y eran el 90 % del costo
                // de subir volando (medido con gdb el 27 sep 2026).
                let rgb = ha_shared::grueso::con(|| {
                    let rgb = average_tile_color(tiles, self.body, s.x, s.y, self.tile_m, 2);
                    // **La ciudad no cabe en un quad de 64 m.** Una consulta por
                    // vértice (memoizada) la pinta igual; ver `world2d::tinte_urbano`.
                    crate::world2d::tinte_urbano(self.body, s.x, s.y, rgb)
                });
                let lin = Color::srgb(rgb[0], rgb[1], rgb[2]).to_linear();
                self.index_of[iy * self.side + ix] = self.positions.len() as u32;
                self.positions.push([p.x, p.y, 0.0]);
                self.colors.push([lin.red, lin.green, lin.blue, 1.0]);
            }
        }
        self.next_v = end;
        self.next_v >= total
    }

    fn into_mesh(self) -> Mesh {
        let side = self.side;
        let mut indices: Vec<u32> = Vec::with_capacity(self.quads.len() * 6);
        for (ix, iy) in self.quads {
            let (ix, iy) = (ix as usize, iy as usize);
            let v = |dx: usize, dy: usize| self.index_of[(iy + dy) * side + ix + dx];
            let (a, b, c, d) = (v(0, 0), v(1, 0), v(1, 1), v(0, 1));
            indices.extend_from_slice(&[a, b, c, a, c, d]);
        }

        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.colors);
        mesh.insert_indices(Indices::U32(indices));
        mesh
    }
}

/// **Plazo por frame del horneado de la malla adaptativa, en milisegundos.**
///
/// Antes esto era `LOD_ROWS_PER_FRAME = 4` — cuatro FILAS de vértices — con el
/// número "medido en el Redmi": una fila = 193 vértices × 4 muestras ≈ 2-4 ms, o
/// sea ~10 ms la tanda y la malla entera (193 filas) en 49 frames.
///
/// Ese número envejeció mal. Cada capa que gana la sim encarece la muestra, y con
/// ciudades la fila dejó de costar 2-4 ms: medido el 24 ago 2026 en el Redmi,
/// `sync_terrain_lod` marcaba **1.950-2.450 ms**. Es la misma lección que ya se
/// aprendió dos veces (`DISC_BAKE_MS_PER_FRAME`, y antes el presupuesto en filas
/// del horneado de discos): **contar unidades de trabajo sólo acota el frame si
/// todas cuestan lo mismo**, y en un generador procedural nunca cuestan lo mismo.
///
/// Lo que se pierde: ya no se puede prometer "la malla sale en N frames" desde una
/// constante, porque depende de la máquina. Se paga que la malla quede un escalón
/// atrasada más tiempo en un teléfono lento — mucho más barato que un frame de dos
/// segundos.
const LOD_MS_PER_FRAME: f32 = 4.0;

/// Vértices de la malla adaptativa entre dos miradas al reloj. Chico a propósito:
/// lo que acota el fotograma es `LOD_MS_PER_FRAME`, y una tanda gorda le vuelve a
/// poner un piso que el plazo no puede bajar.
const LOD_VERTS_POR_TANDA: usize = 48;

/// El horneado de la malla adaptativa en curso, si hay uno.
#[derive(Resource, Default)]
struct TerrainLodJob(Option<LodTask>);

/// El horneado en un HILO DE FONDO, con su propia copia de las ediciones. Cada vértice son
/// ~1,7 ms en el Redmi y la malla tiene decenas de miles: repartirlo por frames en el hilo
/// principal no acota nada (una sola tanda ya eran ~80 ms: `sync_terrain_lod` = 82-88 ms
/// por cuadro volando, medido el 1 oct 2026). La malla vieja sigue en pantalla hasta que la
/// nueva llega entera.
struct LodTask {
    body: CelestialBody,
    center: Vec2,
    tile_m: f32,
    version: u64,
    tarea: bevy::tasks::Task<LodBake>,
}

/// Malla de `LOD_QUADS²` quads de `tile_m` metros centrada en `center`, cocinada
/// **de una sola pasada**. Es la referencia contra la que se valida el horneado
/// por franjas; el juego usa [`LodBake`].
#[cfg(test)]
fn build_terrain_lod_mesh(
    tiles: &TileWorld,
    body: CelestialBody,
    center: Vec2,
    tile_m: f32,
) -> Mesh {
    let mut bake = LodBake::new(body, center, tile_m, tiles.version);
    while !bake.step(tiles, usize::MAX) {}
    bake.into_mesh()
}

/// Color medio del terreno en un cuadrado de `size` metros centrado en un punto
/// de RENDER `(rx, ry)`. Promediar es lo que convierte el ruido de tiles de 1 m
/// en bioma legible cuando un píxel cubre metros de mundo: sin esto, tanto el
/// mapa satelital como el disco del planeta se ven como estática.
/// Color medio del terreno en un cuadrado de `size` metros centrado en un punto
/// de RENDER `(rx, ry)`.
///
/// El color de cada muestra sale de `world2d::tile_render_color`, la MISMA
/// función que pinta los tiles de 1 m: con relieve, con la sombra por altura y
/// con el líquido propio del cuerpo. Antes acá se usaba `Tile::color()` pelado y
/// el resultado era que en cuanto la capa fina se apagaba el mundo perdía todo
/// su modelado y quedaba un manchón café o verde — la "niebla" de la reentrada.
/// La sombra de TERRAZA sí se salta (`TileShade::Coarse`): es de escala de un
/// tile y a esta resolución se promedia sola.
///
/// `taps²` muestras por celda de salida, y el número no es libre: la malla
/// adaptativa son 37 000 quads que se rehacen de golpe en UN frame, así que cada
/// tap de más son 37 000 consultas de relieve más en ese frame. Subirlo fue lo
/// primero que probé y devolvió el tirón al despegue que costó arreglar (ver
/// `docs/RENDIMIENTO.md`). Con 2 alcanza: lo que hacía falta para que el mundo se
/// leyera desde el aire era el SOMBREADO, no más muestras.
fn average_tile_color(
    tiles: &TileWorld,
    body: CelestialBody,
    rx: f32,
    ry: f32,
    size: f32,
    taps: i32,
) -> [f32; 3] {
    let step = size / taps as f32;
    let mut acc = [0.0f32; 3];
    for iy in 0..taps {
        for ix in 0..taps {
            let sx = rx - size * 0.5 + (ix as f32 + 0.5) * step;
            let sy = ry - size * 0.5 + (iy as f32 + 0.5) * step;
            // render y = -z.
            let tx = (sx / TILE_M).floor() as i64;
            let tz = (-sy / TILE_M).floor() as i64;
            let c = crate::world2d::tile_render_color(
                tiles,
                body,
                tx,
                tz,
                crate::world2d::TileShade::Coarse,
            );
            acc[0] += c[0];
            acc[1] += c[1];
            acc[2] += c[2];
        }
    }
    let n = (taps * taps) as f32;
    [acc[0] / n, acc[1] / n, acc[2] / n]
}

/// Lado de la textura del disco. 512² sobre 2·radio (Tierra: 5.9 m/píxel) —
/// suficiente para leer costas y bosques cuando el planeta llena la pantalla.
const DISC_TEX_N: usize = 512;

/// Filas de la textura del disco que se pintan por frame.
///
/// Un disco entero son 1 000 000 de muestras de terreno; repartirlo por franjas
/// evita el tirón de hacerlas de una. El disco se ve con su color de catálogo
/// mientras tanto, así que la espera no se nota.
///
/// **32 filas era demasiado grande para el Redmi.** Con `ha-sys` en el teléfono,
/// subiendo al espacio (que es cuando los discos entran en cuadro y se encolan)
/// `bake_body_textures` marcaba **18,6 ms/frame** — el segundo gasto del juego,
/// más que todo el resto de la simulación junta. A 8 filas la tanda cuesta ~4,5
/// ms y el disco tarda 64 frames en vez de 16: ~1 s de más mostrando el color
/// plano, a cambio de 14 ms por frame.
const DISC_BAKE_ROWS_PER_FRAME: usize = 8;

/// **Y LAS FILAS SON UN PRESUPUESTO FALSO: lo que hay que acotar es el TIEMPO.**
///
/// Una fila cuesta 512 píxeles × 4 muestras de terreno, y **cuánto cuesta una
/// muestra de terreno no es una constante del juego**: cambia con el build
/// (release/debug), con la máquina y con si el sitio tiene relieve, río o
/// ciudad. Ocho filas son 4,5 ms en el Redmi en release y **casi un minuto** en
/// un build de depuración — medido con `HA_STAGES=1` en el driver del relieve:
///
/// ```text
/// ha-stages: frames=1 frame_ms=59441.1 | update=59382.8
/// ha-sys: bake_body_textures=59268.6 stream_tile_chunks=198.2 ...
/// ```
///
/// **Un solo fotograma de 59 segundos**, y el 99,8 % era este horneado. Con eso
/// la cortina de carga se rinde por tiempo ("se acabó el tiempo (20 s) con 45
/// chunks y 1 discos pendientes — entro igual") y el jugador entra a un mundo a
/// medio armar: eso, y no un error de dibujo, es la pantalla vacía de
/// `relieve-3-cumbre.png`. Los drivers de QA directamente no terminaban.
///
/// El arreglo es dejar de contar filas y **mirar el reloj**: se pintan filas
/// hasta agotar el plazo, con un mínimo de una para que el disco siempre avance.
/// Así el mismo código da 8-10 filas en el Redmi, doscientas en un escritorio y
/// una en un build lento — y en los tres casos el fotograma se respeta. Las
/// filas quedan sólo como TECHO por tanda (que no se coma la caché de niveles de
/// un tirón).
/// **Por encima de esto el cuerpo no se ve: lo estás pisando.**
///
/// `on_screen` es `2·radio / alto_del_encuadre`, o sea cuántas pantallas mide el
/// cuerpo. Doce pantallas ya es estar tan cerca que el disco —que vive detrás de
/// todo el suelo, en z=−20— queda tapado por el terreno. Texturarlo ahí es pagar
/// medio segundo de frame por píxeles que nadie ve. Ver el guard de
/// `sync_universe`.
/// **Píxeles del disco entre dos miradas al reloj.** Chico a propósito: lo que
/// acota el fotograma es el plazo, y una tanda gorda le vuelve a poner un piso
/// que el plazo no puede bajar (ver [`bake_body_textures`]). Con la muestra más
/// cara medida (una ciudad sin atajo: ~2 ms por tile) 32 píxeles eran 100-370 ms;
/// hoy los cascos se muestrean en mosaico barato y cuatro píxeles son ~8 ms en el peor
/// caso y ~0 en el mejor; el reloj corta en cuanto se pasa.
const PX_POR_TANDA: usize = 4;

const DISCO_TAPADO_SOBRE: f32 = 12.0;

const DISC_BAKE_MS_PER_FRAME: f32 = 4.0;

/// El plazo **detrás de la cortina de carga**, donde un fotograma largo no es un
/// tirón sino trabajo adelantado. Ver `loading::boost_budget`.
pub const DISC_BAKE_MS_LOADING: f32 = 40.0;

/// Presupuesto vigente del horneado del disco. Es un recurso y no la constante a
/// secas por lo mismo que `world2d::ChunkBudget`: la cortina de carga lo sube y
/// lo devuelve al entrar.
#[derive(Resource)]
pub struct DiscBakeRows {
    /// Techo de filas por tanda.
    pub filas: usize,
    /// Plazo de la tanda, en milisegundos. **Es el que manda.**
    pub ms: f32,
}

impl Default for DiscBakeRows {
    fn default() -> Self {
        Self::normal()
    }
}

impl DiscBakeRows {
    pub const fn normal() -> Self {
        Self { filas: DISC_BAKE_ROWS_PER_FRAME, ms: DISC_BAKE_MS_PER_FRAME }
    }
}

/// Trabajo de textura de disco a medio cocinar.
struct DiscBake {
    body: CelestialBody,
    material: Handle<ColorMaterial>,
    /// RGBA de `DISC_TEX_N`², se llena de arriba hacia abajo.
    data: Vec<u8>,
    /// Cursor en PÍXELES (no en filas) sobre `DISC_TEX_N`². Ver
    /// [`bake_body_textures`]: una fila entera es una unidad demasiado gorda para
    /// que un plazo de 4 ms signifique algo.
    next_px: usize,
    /// El horneado en un hilo de fondo (ver `LodTask`): una muestra cuesta ms en el celu y el
    /// disco son 262 144 píxeles, así que repartirlo por cuadros en el hilo principal nunca
    /// dejó de costar (`bake_body_textures` = 24 ms por cuadro volando, 1 oct 2026).
    tarea: Option<bevy::tasks::Task<Vec<u8>>>,
}

/// Cola de texturas de disco pendientes (una en curso + las que esperan).
///
/// Existe para que generar el mapa de un planeta NUNCA cueste un frame entero.
/// `sync_universe` sólo pide; el horneado real lo reparte `bake_body_textures`.
#[derive(Resource, Default)]
pub struct DiscBakeQueue {
    jobs: Vec<DiscBake>,
}

impl DiscBakeQueue {
    /// Discos que faltan cocinar. Lo mira la pantalla de carga: entrar al juego
    /// con esto sin vaciar es entrar con el tirón puesto.
    pub fn pending(&self) -> usize {
        self.jobs.len()
    }

    fn request(&mut self, body: CelestialBody, material: Handle<ColorMaterial>) {
        if self.jobs.iter().any(|j| j.body == body) {
            return;
        }
        self.jobs.push(DiscBake {
            body,
            material,
            data: Vec::new(),
            next_px: 0,
            tarea: None,
        });
    }
}

/// Cocina la textura del disco del planeta por FRANJAS, un trabajo a la vez.
/// Cuando termina una, aplica el desenfoque, sube la imagen y la enchufa al
/// material del disco (que hasta ese momento mostraba el color de catálogo).
fn bake_body_textures(
    mut queue: ResMut<DiscBakeQueue>,
    rows: Res<DiscBakeRows>,
    tiles: Res<TileWorld>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let _probe = crate::debug::probe("bake_body_textures");
    let Some(job) = queue.jobs.first_mut() else { return };
    let n = DISC_TEX_N;
    // **DE A TANDAS CHICAS, MIRANDO EL RELOJ.**
    //
    // Ver [`DiscBakeRows`]: cuánto cuesta una muestra depende del build, de la
    // máquina y de si ese trozo de planeta tiene relieve, río o ciudad. Por eso
    // se mira el reloj. Pero el reloj sólo acota si la unidad de trabajo entre
    // dos miradas es CHICA, y una fila entera no lo es: son `DISC_TEX_N` píxeles
    // × 4 muestras del resolvedor de celda completo. Medido en el Redmi el 24 ago
    // 2026: **una fila = ~500 ms**. Con "al menos una fila" el plazo de 4 ms era
    // decorativo — el piso del sistema era medio segundo, y ése era el lag.
    //
    // Ahora el cursor va por PÍXEL y la tanda es `PX_POR_TANDA`, así que el peor
    // caso entre dos miradas al reloj son unos pocos píxeles. El disco tarda más
    // en estar listo; el fotograma deja de pagarlo.
    let total = n * n;
    let _ = &rows;
    if job.tarea.is_none() {
        let copia = tiles.copia_para_lod();
        let body = job.body;
        let pool = bevy::tasks::AsyncComputeTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        job.tarea = Some(pool.spawn(async move {
            let mut data = vec![0u8; total * 4];
            fill_disc_span(&mut data, &copia, body, 0, total);
            data
        }));
    }
    let Some(tarea) = job.tarea.as_mut() else { return };
    let Some(data) = bevy::tasks::block_on(bevy::tasks::futures_lite::future::poll_once(tarea)) else {
        return;
    };
    job.data = data;
    job.next_px = total;
    let mut job = queue.jobs.remove(0);
    // Desenfoque de caja sobre el RGB (respetando el alfa del recorte). El
    // promediado por texel ya baja el ruido, pero a la escala del disco un píxel
    // sigue siendo metros de terreno: sin esto el planeta se ve granulado como
    // estática en vez de con biomas.
    box_blur_rgb(&mut job.data, n, 2);
    // **EL PLANETA EN PIXEL ART** (28 sep 2026, referencia del usuario:
    // `design/referencias/luna-pixel-art.png`). El disco suave de 512 px parecía una
    // foto borrosa pegada en un juego de píxeles. Se baja a `DISC_PIXEL_N` por
    // bloques, la luz se posteriza en bandas (tonos planos, el matiz de cada
    // material se conserva) y se dibuja con vecino más próximo.
    let (datos, m) = pixelar_disco(&job.data, n, job.body);
    let mut image = Image::new(
        Extent3d { width: m as u32, height: m as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        datos,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = bevy::image::ImageSampler::nearest();
    if let Some(m) = materials.get_mut(&job.material) {
        m.texture = Some(images.add(image));
        m.color = Color::WHITE;
    }
}

/// Lado del disco pixelado: 128 píxeles de planeta, como la luna de referencia.
const DISC_PIXEL_N: usize = 128;
/// Bandas de luz por material. Cinco tonos planos: luz, medio, sombra, y los dos
/// del limbo.
const DISC_BANDAS: f32 = 5.0;

/// Baja el disco de `n` a [`DISC_PIXEL_N`] por bloques y lo pinta como la luna
/// de referencia: **terminador** (la esfera iluminada de arriba a la derecha,
/// con su lado de noche), **cráteres** en los cuerpos sin aire, y la luz
/// posterizada en bandas. El alfa sale duro (0 o 255): borde de píxeles.
fn pixelar_disco(data: &[u8], n: usize, body: CelestialBody) -> (Vec<u8>, usize) {
    let m = DISC_PIXEL_N.min(n);
    let b = n / m;
    // Luz: de arriba a la derecha y algo de frente (imagen: x→derecha, y→abajo).
    let luz = Vec3::new(0.62, -0.38, 0.69).normalize();
    // Cráteres: sólo donde no hay aire que los borre.
    let crateres: Vec<(f32, f32, f32)> = if ha_shared::body_atmo_top(body) <= 0.0 {
        let seed = ha_shared::body_def(body).terrain_seed;
        (0..46u32)
            .map(|i| {
                let h = |k: u32| {
                    let mut x = seed ^ i.wrapping_mul(0x9E37_79B9) ^ k.wrapping_mul(0x85EB_CA6B);
                    x ^= x >> 16;
                    x = x.wrapping_mul(0x7FEB_352D);
                    x ^= x >> 15;
                    (x & 0xFFFF) as f32 / 65535.0
                };
                // radio en píxeles del disco: muchos chicos, pocos grandes
                let r = 1.5 + h(3).powi(3) * 9.0;
                let ang = h(1) * std::f32::consts::TAU;
                let d = h(2).sqrt() * (m as f32 * 0.5 - r - 1.0);
                (m as f32 * 0.5 + ang.cos() * d, m as f32 * 0.5 + ang.sin() * d, r)
            })
            .collect()
    } else {
        Vec::new()
    };
    let mut out = vec![0u8; m * m * 4];
    for my in 0..m {
        for mx in 0..m {
            let (mut acc, mut op) = ([0.0f32; 3], 0usize);
            for y in my * b..(my + 1) * b {
                for x in mx * b..(mx + 1) * b {
                    let i = (y * n + x) * 4;
                    if data[i + 3] > 127 {
                        for c in 0..3 {
                            acc[c] += data[i + c] as f32;
                        }
                        op += 1;
                    }
                }
            }
            let o = (my * m + mx) * 4;
            if op * 2 < b * b {
                continue; // queda transparente
            }
            let rgb = acc.map(|v| v / op as f32 / 255.0);
            // normal de la esfera en este píxel
            let (u, v) = ((mx as f32 + 0.5) / m as f32 * 2.0 - 1.0, (my as f32 + 0.5) / m as f32 * 2.0 - 1.0);
            let nz = (1.0 - u * u - v * v).max(0.0).sqrt();
            let normal = Vec3::new(u, v, nz);
            // Volumen SIMÉTRICO (limbo oscuro): la cara de noche la pone
            // `BodyNightSide`, que gira con el Sol; una luz horneada fija se
            // pelearía con ella.
            let _ = normal;
            let mut k = 0.45 + 0.55 * nz.powf(0.6);
            // cráteres: piso hundido, borde claro del lado de la luz
            for &(cx, cy, r) in &crateres {
                let (dx, dy) = (mx as f32 + 0.5 - cx, my as f32 + 0.5 - cy);
                let d = (dx * dx + dy * dy).sqrt();
                if d > r {
                    continue;
                }
                if d > r * 0.72 {
                    let de_cara = (dx * luz.x + dy * luz.y) / d.max(1e-3);
                    k *= if de_cara > 0.0 { 1.18 } else { 0.70 };
                } else {
                    k *= 0.84;
                }
                break;
            }
            let lum = (0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]).max(1e-4);
            let objetivo = (lum * k).clamp(0.0, 1.0);
            let banda = ((objetivo * DISC_BANDAS).round() / DISC_BANDAS).max(0.35 / DISC_BANDAS);
            let f = banda / lum;
            for c in 0..3 {
                out[o + c] = ((rgb[c] * f).clamp(0.0, 1.0) * 255.0) as u8;
            }
            out[o + 3] = 255;
        }
    }
    (out, m)
}

/// El disco del planeta pintado con SU PROPIO terreno (`topdown_tile`), no con
/// ruido inventado: la costa que ves desde el espacio es la misma que pisás. Con
/// oscurecimiento hacia el limbo para que lea como una esfera, y transparente
/// fuera del círculo.
///
/// Pinta sólo los píxeles `[px0, px1)` en índice PLANO (fila × `DISC_TEX_N` +
/// columna): es una PORCIÓN del trabajo, para que `bake_body_textures` pueda
/// repartir un disco entre varios frames **sin que la unidad mínima sea una fila
/// entera** — ver el comentario del plazo en ese sistema.
fn fill_disc_span(data: &mut [u8], tiles: &TileWorld, body: CelestialBody, px0: usize, px1: usize) {
    let n = DISC_TEX_N;
    let r = body_radius(body);
    let seed = ha_shared::body_def(body).terrain_seed;
    for i in px0..px1 {
        let (py, px) = (i / n, i % n);
        {
            let u = (px as f32 + 0.5) / n as f32 - 0.5;
            let v = (py as f32 + 0.5) / n as f32 - 0.5;
            let d = (u * u + v * v).sqrt();
            let idx = (py * n + px) * 4;
            if d > 0.5 {
                data[idx + 3] = 0;
                continue;
            }
            // Píxel → punto del mundo. La fila 0 es arriba (render y = +r), y
            // render y = -z, así que z crece hacia abajo de la imagen.
            // Muestreo PROMEDIADO sobre el área que cubre el píxel: un solo tap
            // sobre tiles de 1 m con un píxel de ~6 m daba sal y pimienta (el
            // planeta parecía estática de TV, no un mundo).
            let texel_m = 2.0 * r / n as f32;
            // En grueso: un texel son kilómetros, los ríos no se ven (`ha_shared::grueso`).
            let rgb = ha_shared::grueso::con(|| {
                average_tile_color(tiles, body, u * 2.0 * r, -v * 2.0 * r, texel_m, 2)
            });
            // Un toque de ruido para dar volumen. Bajado de ±8 % a ±4 %: ahora el
            // color de cada texel YA trae el sombreado de relieve real
            // (`average_tile_color` → `tile_render_color`), así que este ruido
            // dejó de ser el que dibujaba el modelado y pasó a ser lo que
            // siempre debió ser — una pizca de grano encima de la geografía de
            // verdad, no un sustituto de ella.
            let relief = 0.96 + 0.08 * fbm(u * 9.0 + 4.0, v * 9.0 + 4.0, seed);
            // Limbo oscurecido → lectura esférica.
            let shade = relief * (1.0 - (d / 0.5).powi(2) * 0.42);
            data[idx] = ((rgb[0] * shade).clamp(0.0, 1.0) * 255.0) as u8;
            data[idx + 1] = ((rgb[1] * shade).clamp(0.0, 1.0) * 255.0) as u8;
            data[idx + 2] = ((rgb[2] * shade).clamp(0.0, 1.0) * 255.0) as u8;
            data[idx + 3] = 255;
        }
    }
}

/// Ruido de valor con interpolación suave — sólo para el sombreado de relieve
/// del disco (el COLOR sale siempre del terreno real, nunca de esto).
fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let hash = |ix: i32, iy: i32| -> f32 {
        let mut h = (ix as u32).wrapping_mul(374_761_393).wrapping_add(seed);
        h = (h ^ (iy as u32).wrapping_mul(668_265_263)).wrapping_mul(2_246_822_519);
        h ^= h >> 15;
        (h & 0xffff) as f32 / 65535.0
    };
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (xf, yf) = (x - xi as f32, y - yi as f32);
    let (sx, sy) = (xf * xf * (3.0 - 2.0 * xf), yf * yf * (3.0 - 2.0 * yf));
    let a = hash(xi, yi) + (hash(xi + 1, yi) - hash(xi, yi)) * sx;
    let b = hash(xi, yi + 1) + (hash(xi + 1, yi + 1) - hash(xi, yi + 1)) * sx;
    a + (b - a) * sy
}

fn fbm(x: f32, y: f32, seed: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 0.5;
    let mut freq = 1.0;
    for o in 0..4 {
        sum += value_noise(x * freq, y * freq, seed.wrapping_add(o * 131)) * amp;
        freq *= 2.0;
        amp *= 0.5;
    }
    sum.clamp(0.0, 1.0)
}

/// Desenfoque de caja 3×3 aplicado `passes` veces sobre los canales RGB de una
/// imagen RGBA cuadrada. Los texels transparentes (fuera del disco) no aportan
/// ni reciben color, así que el limbo no se lava contra el fondo.
fn box_blur_rgb(data: &mut [u8], n: usize, passes: u32) {
    let mut src = data.to_vec();
    for _ in 0..passes {
        for y in 0..n {
            for x in 0..n {
                let idx = (y * n + x) * 4;
                if src[idx + 3] == 0 {
                    continue;
                }
                let mut acc = [0u32; 3];
                let mut count = 0u32;
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let sx = x as i32 + dx;
                        let sy = y as i32 + dy;
                        if sx < 0 || sy < 0 || sx >= n as i32 || sy >= n as i32 {
                            continue;
                        }
                        let s = (sy as usize * n + sx as usize) * 4;
                        if src[s + 3] == 0 {
                            continue;
                        }
                        acc[0] += src[s] as u32;
                        acc[1] += src[s + 1] as u32;
                        acc[2] += src[s + 2] as u32;
                        count += 1;
                    }
                }
                if count > 0 {
                    data[idx] = (acc[0] / count) as u8;
                    data[idx + 1] = (acc[1] / count) as u8;
                    data[idx + 2] = (acc[2] / count) as u8;
                }
            }
        }
        src.copy_from_slice(data);
    }
}

#[cfg(test)]
mod tests {
    /// Hornea el disco de la Tierra y de la Luna y los deja en `HA_DISCO_DIR` para
    /// mirarlos (el estilo pixel art de `pixelar_disco`). `--ignored` porque tarda.
    #[test]
    #[ignore]
    fn hornear_discos_a_png() {
        let dir = std::env::var("HA_DISCO_DIR").unwrap_or_else(|_| "/tmp".into());
        let tiles = crate::world2d::TileWorld::default();
        for &body in CelestialBody::all().iter().filter(|b| **b != CelestialBody::Sun) {
            let n = DISC_TEX_N;
            let mut data = vec![0u8; n * n * 4];
            fill_disc_span(&mut data, &tiles, body, 0, n * n);
            box_blur_rgb(&mut data, n, 2);
            let (px, m) = pixelar_disco(&data, n, body);
            let ruta = format!("{dir}/disco-{body:?}.ppm");
            let mut f = std::fs::File::create(&ruta).unwrap();
            use std::io::Write;
            write!(f, "P6\n{m} {m}\n255\n").unwrap();
            for i in 0..m * m {
                let a = px[i * 4 + 3] as u32;
                for c in 0..3 {
                    f.write_all(&[((px[i * 4 + c] as u32 * a + 12 * (255 - a)) / 255) as u8]).unwrap();
                }
            }
            eprintln!("disco → {ruta}");
        }
    }

    use super::*;

    #[test]
    fn zoom_curve_is_continuous_and_monotonic() {
        let b = CelestialBody::Earth;
        assert!((view_h_for_alt(b, 0.0) - GROUND_VIEW_H).abs() < 1e-3);
        let mut prev = view_h_for_alt(b, 0.0);
        let mut alt = 0.0;
        while alt < MAX_ALT {
            alt += 5.0;
            let v = view_h_for_alt(b, alt);
            assert!(v > prev, "no monotona en alt {alt}");
            // Sin saltos: 5 m de altura nunca deben más que duplicar el encuadre.
            assert!(v < prev * 2.0, "salto de zoom en alt {alt}: {prev} -> {v}");
            prev = v;
        }
    }

    #[test]
    fn planet_fills_a_healthy_slice_when_high() {
        // A un radio de altura el planeta ocupa ~45 % del alto de pantalla.
        let b = CelestialBody::Earth;
        let r = body_radius(b);
        let frac = 2.0 * r / view_h_for_alt(b, r);
        assert!(frac > 0.35 && frac < 0.6, "fraccion {frac}");
    }

    #[test]
    fn frame_offset_is_zero_for_the_reference_body() {
        for &b in CelestialBody::all() {
            let o = body_offset(b, b, 123.0);
            assert!(o.length() < 1e-3, "{b:?}");
        }
    }

    #[test]
    fn rebasing_preserves_relative_geometry() {
        // Rebasar el marco es un cambio de coordenadas puro: la posición de un
        // cuerpo respecto de otro no cambia. Es lo que hace que cambiar de
        // cuerpo de referencia sea invisible en pantalla.
        let t = 77.0;
        let a = body_offset(CelestialBody::Moon, CelestialBody::Earth, t);
        let b = body_offset(CelestialBody::Moon, CelestialBody::Mars, t)
            - body_offset(CelestialBody::Earth, CelestialBody::Mars, t);
        assert!((a - b).length() < 1.0);
    }

    /// Tiempo aproximado de un viaje de ida y vuelta Tierra↔Luna volando:
    /// subir, cruzar, bajar, y lo mismo de vuelta. Sale de las MISMAS funciones
    /// que usa el vuelo, así que se mueve solo si cambian las escalas.
    fn round_trip_secs() -> f32 {
        let earth = CelestialBody::Earth;
        let dist = body_offset(CelestialBody::Moon, earth, 0.0).length();
        // Ascenso: la altura crece exponencialmente, se integra por pasos.
        let mut alt = 0.0f32;
        let mut climb = 0.0f32;
        while alt < dist * 0.5 && climb < 600.0 {
            alt += climb_rate(alt) * 0.1;
            climb += 0.1;
        }
        let cruise = dist / lateral_speed(dist * 0.5);
        2.0 * (2.0 * climb + cruise)
    }

    #[test]
    fn a_full_ki_charge_covers_a_moon_round_trip() {
        // ESTA es la prueba que faltaba cuando el sistema creció a millones de
        // metros: el Ki se vaciaba a mitad del ascenso, el vuelo caía solo y el
        // jugador quedaba rebotando entre despegue y aterrizaje, sin manera de
        // salir del planeta. El presupuesto de Ki y la escala del sistema están
        // acoplados; que lo verifique un test y no la próxima partida.
        use crate::flight::{KI_DRAIN_PER_S, KI_MAX};
        let budget = KI_MAX / KI_DRAIN_PER_S;
        let trip = round_trip_secs();
        assert!(
            budget > trip * 1.3,
            "Ki alcanza para {budget:.0} s de vuelo pero la ida y vuelta a la Luna son {trip:.0} s"
        );
    }

    #[test]
    fn crossing_the_home_world_takes_about_ten_minutes() {
        // El pedido explícito: que cruzar el planeta volando se sienta un viaje.
        // A altura de crucero (donde todavía se distingue el terreno) tiene que
        // rondar los 10 minutos, ni 30 segundos ni media hora.
        let earth = CelestialBody::Earth;
        let across = body_radius(earth) * 2.0;
        let cruise_alt = 1_400.0; // se sigue viendo el suelo
        let secs = across / lateral_speed(cruise_alt);
        assert!(
            (300.0..=1_200.0).contains(&secs),
            "cruzar el mundo tarda {secs:.0} s a altura de crucero"
        );
    }

    #[test]
    fn every_landable_body_has_room_to_fly_before_space() {
        // El bug de "en la Luna me quedaba a 30 metros" empezaba acá: con un
        // mínimo de 60 m, volar la altura de un edificio sobre un cuerpo de 8.4 km
        // de radio ya te ponía en `RegionId::Space` — se dejaba de streamear el
        // terreno y el marco no rebasaba. Un cuerpo tiene que tener cielo propio.
        for &b in CelestialBody::all() {
            if !body_landable(b) {
                continue;
            }
            let sky = space_alt(b);
            assert!(sky >= 600.0, "{b:?} tiene sólo {sky} m de cielo");
            // Y tiene que ser chico respecto del cuerpo: si no, nunca saldrías.
            assert!(
                sky < body_radius(b).max(2_000.0),
                "{b:?}: techo de {sky} m sobre un radio de {}",
                body_radius(b)
            );
        }
    }

    #[test]
    fn the_zoom_ceiling_frames_the_whole_system() {
        // "En teoría podría ir a cualquier rincón del universo": para eso hace
        // falta poder VER el sistema entero desde el borde, o navegar a ciegas.
        // El encuadre máximo tiene que cubrir la distancia Sol↔Neptuno.
        let span = body_offset(CelestialBody::Neptune, CelestialBody::Sun, 0.0).length();
        let widest = view_h_for_alt(CelestialBody::Earth, MAX_ALT);
        assert!(
            widest > span,
            "el encuadre topa en {widest:.0} m y el sistema mide {span:.0} m"
        );
    }

    #[test]
    fn the_farthest_body_is_reachable_on_one_ki_charge() {
        // Con la economía nueva (el Ki cuesta EMPUJAR, no existir) una carga tiene
        // que alcanzar para el trayecto más largo del sistema a velocidad de tope.
        // Si esto falla, explorar los rincones vuelve a depender de no quedarse
        // tirado en el vacío.
        use crate::flight::{KI_DRAIN_PER_S, KI_MAX};
        let span = body_offset(CelestialBody::Neptune, CelestialBody::Sun, 0.0).length();
        let cruise = span / lateral_speed(MAX_ALT);
        let budget = KI_MAX / (KI_DRAIN_PER_S * 1.5); // el peor caso: empujando
        assert!(
            budget > cruise,
            "cruzar el sistema son {cruise:.0} s y el Ki da para {budget:.0} s"
        );
    }

    #[test]
    fn fine_tiles_hand_over_cleanly() {
        assert!(FINE_FADE_VIEW_H < FINE_OFF_VIEW_H);
        assert_eq!(fine_tile_alpha(0.0), 1.0);
        assert_eq!(fine_tile_alpha(FINE_OFF_VIEW_H + 1.0), 0.0);
    }

    #[test]
    fn adaptive_lod_always_covers_the_screen() {
        // La malla adaptativa tiene que abarcar MÁS que la pantalla en todo el
        // rango donde manda: si no, se ve su borde, que es exactamente el corte
        // que este sistema existe para evitar.
        // Sólo hace falta mientras la pantalla sea MENOR que el planeta: pasado
        // ese punto el cuerpo ya no llena la vista, la malla lo cubre entero (va
        // recortada a su radio) y lo que rodea es espacio.
        let world = body_radius(CelestialBody::Earth) * 2.0;
        let mut v = FINE_FADE_VIEW_H;
        while v * 2.4 < world {
            let span = LOD_QUADS as f32 * lod_tile_m(v);
            // Ancho de pantalla en el peor aspecto razonable (móvil apaisado).
            assert!(span > v * 2.4, "encuadre {v}: malla {span} no llena la vista");
            v *= 1.15;
        }
        // Y en el tope, la malla tiene que abarcar el planeta entero.
        assert!(LOD_QUADS as f32 * lod_tile_m(world) >= world);
    }

    #[test]
    fn lod_steps_are_powers_of_two_and_monotonic() {
        let mut prev = 0.0f32;
        let mut v = 40.0;
        while v < 500_000.0 {
            let t = lod_tile_m(v);
            assert!(t >= prev, "el LOD retrocedio en {v}");
            assert_eq!(t, t.log2().round().exp2(), "lado no potencia de dos: {t}");
            prev = t;
            v *= 1.3;
        }
    }

    /// El horneado por franjas existe para MATAR EL TIRÓN, no para cambiar lo que
    /// se ve. Si repartirlo alterara un solo vértice, el mundo desde el aire se
    /// vería distinto según cuántos frames tardó en cocinarse — un bug imposible
    /// de diagnosticar. Por eso la invariante se clava acá: mismas posiciones,
    /// mismos colores, mismos índices que la pasada de una sola vez.
    #[test]
    fn la_malla_por_franjas_es_identica_a_la_de_una_pasada() {
        use bevy::render::mesh::VertexAttributeValues as V;
        let tiles = crate::world2d::TileWorld::default();
        let (body, center, tile_m) = (CelestialBody::Earth, Vec2::new(64.0, -32.0), 16.0);

        let de_una = build_terrain_lod_mesh(&tiles, body, center, tile_m);

        // Tandas chicas a propósito: 7 no divide a 193² = 37 249, así que la
        // última queda corta y se ejercita el recorte contra el total.
        let mut bake = LodBake::new(body, center, tile_m, tiles.version);
        // El tope es generoso porque la unidad es el VÉRTICE: la malla son
        // `side²` ≈ 37 000, así que de a 7 hacen falta unas 5.300 tandas. Lo que
        // el tope caza es que el cursor no avance, no que tarde.
        let mut frames = 0;
        while !bake.step(&tiles, 7) {
            frames += 1;
            assert!(frames < 100_000, "el horneado por franjas no termina nunca");
        }
        assert!(frames > 20, "con 7 vértices por tanda tiene que tardar varias tandas");
        let por_franjas = bake.into_mesh();

        assert_eq!(de_una.count_vertices(), por_franjas.count_vertices());
        // `Indices` no implementa `PartialEq`, así que se comparan los u32.
        let idx = |m: &Mesh| match m.indices() {
            Some(Indices::U32(i)) => i.clone(),
            _ => panic!("la malla perdio sus indices"),
        };
        assert_eq!(idx(&de_una), idx(&por_franjas), "los indices no coinciden");
        for attr in [Mesh::ATTRIBUTE_POSITION, Mesh::ATTRIBUTE_COLOR] {
            match (de_una.attribute(attr.clone()), por_franjas.attribute(attr)) {
                (Some(V::Float32x3(a)), Some(V::Float32x3(b))) => assert_eq!(a, b),
                (Some(V::Float32x4(a)), Some(V::Float32x4(b))) => assert_eq!(a, b),
                _ => panic!("la malla por franjas perdio un atributo"),
            }
        }
    }

    /// **La tanda del LOD tiene que ser CHICA, y tiene que avanzar siempre.**
    ///
    /// Antes este test comprobaba que la malla saliera en menos de 64 frames, con
    /// la cuenta hecha desde `LOD_ROWS_PER_FRAME`. Esa promesa se cayó a propósito
    /// el 24 ago 2026 (ver `LOD_MS_PER_FRAME`): con un presupuesto de reloj,
    /// cuántos frames tarda depende de la máquina, y prometerlo desde una
    /// constante fue justamente lo que dejó pasar el frame de 2 segundos en el
    /// Redmi. Lo que sí se puede afirmar, y es lo que protege al fotograma:
    ///
    /// 1. la tanda entre dos miradas al reloj es chica —una fila entera de
    ///    vértices NO puede entrar—, así que ninguna muestra cara, por cara que
    ///    sea, se cobra más que `LOD_VERTS_POR_TANDA` de una;
    /// 2. y el horneado avanza aunque el plazo ya esté vencido (`verts.max(1)`),
    ///    o sea que nunca se atasca.
    #[test]
    fn la_tanda_del_lod_es_chica_y_siempre_avanza() {
        let fila = LOD_QUADS as usize + 1;
        assert!(
            LOD_VERTS_POR_TANDA < fila,
            "una tanda de {LOD_VERTS_POR_TANDA} vértices se come una fila entera ({fila}): \
             volvimos al presupuesto que no acota"
        );
        // Con plazo cero igual avanza: `step` toma `verts.max(1)`.
        let tiles = crate::world2d::TileWorld::default();
        let mut bake = LodBake::new(CelestialBody::Earth, Vec2::ZERO, 16.0, tiles.version);
        let antes = bake.next_v;
        bake.step(&tiles, 0);
        assert!(bake.next_v > antes, "con tanda 0 el horneado se atasca para siempre");
    }

    #[test]
    fn la_malla_lod_no_es_un_damero() {
        // El terreno visto desde el aire NO puede ser un mosaico de bloques planos
        // ("me sale con cuadrados, como cuando no carga la textura"). La prueba de
        // que dejo de serlo es geometrica: los vertices se COMPARTEN entre quads
        // (un damero necesita 4 propios por quad) y el color varia dentro de un
        // mismo quad, que es lo que la GPU interpola.
        let tiles = crate::world2d::TileWorld::default();
        let mesh = build_terrain_lod_mesh(&tiles, CelestialBody::Earth, Vec2::ZERO, 16.0);
        let verts = mesh.count_vertices();
        let tris = match mesh.indices() {
            Some(Indices::U32(i)) => i.len() / 3,
            _ => panic!("la malla perdio sus indices"),
        };
        let quads = tris / 2;
        assert!(quads > 1_000, "malla sospechosamente vacia: {quads} quads");
        assert!(
            verts < quads * 2,
            "{verts} vertices para {quads} quads: siguen sin compartirse"
        );
        let Some(bevy::render::mesh::VertexAttributeValues::Float32x4(colors)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("la malla perdio su color por vertice");
        };
        assert!(
            colors.windows(2).any(|w| w[0] != w[1]),
            "todos los vertices del mismo color: el terreno seria un manchon plano"
        );
    }

    /// Vuela de lado desde la orilla de la Tierra hacia la Luna, SIN subir
    /// (`vert` = 0), con la misma cuenta que `creative_fly_step`. Devuelve los
    /// segundos hasta pisar el disco de la Luna (o el tope si no llega).
    fn moon_by_stick_secs(speed: impl Fn(f32, f32) -> f32) -> f32 {
        let earth = CelestialBody::Earth;
        let moon = body_offset(CelestialBody::Moon, earth, 0.0);
        let dir = moon.normalize();
        let mut p = dir * body_walk_radius(earth);
        let mut secs = 0.0;
        while secs < 3_600.0 {
            let (b, d) = nearest_landable(p, earth, 0.0);
            let gap = (d - body_walk_radius(b)).max(0.0);
            if b == CelestialBody::Moon && gap <= 0.0 {
                break;
            }
            p += dir * speed(gap, gap) * 0.05;
            secs += 0.05;
        }
        secs
    }

    #[test]
    fn a_la_luna_con_el_stick_sin_tener_que_subir() {
        // "En el espacio tengo que seguirle dando a subir para poder ir hacia la
        // Luna": con la velocidad atada sólo a la altura, irse de lado era eterno y
        // el atajo era subir. El viaje de lado tiene que ser un viaje, no una espera.
        let ahora = moon_by_stick_secs(cruise_speed);
        let antes = moon_by_stick_secs(|alt, _| lateral_speed(alt));
        assert!((8.0..=90.0).contains(&ahora), "a la Luna con el stick: {ahora:.0} s");
        assert!(ahora * 3.0 < antes, "ahora {ahora:.0} s, antes {antes:.0} s");
    }

    #[test]
    fn el_crucero_del_vacio_no_toca_el_vuelo_sobre_el_planeta() {
        // Sobre el disco el hueco es cero: los 10 minutos de cruzar el mundo quedan.
        for alt in [0.0, 300.0, 1_400.0, 30_000.0] {
            assert_eq!(cruise_speed(alt, 0.0), lateral_speed(alt));
        }
    }

    #[test]
    fn el_encuadre_no_salta_al_pasar_de_la_tierra_a_la_luna() {
        // El "corte" de ir a la Luna: el encuadre se evaluaba con el radio del
        // cuerpo de referencia y saltaba ×1,79 al rebasar el marco. Recorriendo la
        // línea Tierra→Luna a pasos de 50 m, ningún paso puede cambiar el encuadre
        // más de un 1 %, y tiene que dar lo mismo visto desde cualquiera de los
        // dos marcos.
        let earth = CelestialBody::Earth;
        let moon_o = body_offset(CelestialBody::Moon, earth, 0.0);
        let dir = moon_o.normalize();
        for vert in [0.0f32, 2_000.0, 60_000.0] {
            let mut prev: Option<f32> = None;
            let mut s = body_walk_radius(earth);
            while s < moon_o.length() - body_walk_radius(CelestialBody::Moon) {
                let p = dir * s;
                let ref_body = nearest_landable(p, earth, 0.0).0;
                let local = p - body_offset(ref_body, earth, 0.0);
                let h = flight_view_h(ref_body, local, vert, 0.0);
                let desde_tierra = flight_view_h(earth, p, vert, 0.0);
                assert!((h / desde_tierra - 1.0).abs() < 1e-3, "el marco cambia el encuadre en {s:.0} m");
                if let Some(q) = prev {
                    let k = (h / q).max(q / h);
                    assert!(k < 1.01, "salto ×{k:.3} en {s:.0} m (vert {vert})");
                }
                prev = Some(h);
                s += 50.0;
            }
        }
    }

    #[test]
    fn el_encuadre_de_vuelo_coincide_con_la_curva_sobre_el_planeta() {
        // Sobre tu mundo manda el cuerpo que tenés debajo: la curva de despegue y
        // aterrizaje (y sus tests de cámara) no cambian.
        let earth = CelestialBody::Earth;
        for vert in [0.0, 50.0, 1_000.0, 20_000.0] {
            let a = flight_view_h(earth, Vec2::new(1_000.0, -2_000.0), vert, 0.0);
            assert!((a - view_h_for_alt(earth, vert)).abs() < 1e-2, "vert {vert}: {a}");
        }
    }

    fn mask_alpha_at(l: Vec3, u: f32, v: f32) -> u8 {
        let n = NIGHT_MASK_N;
        let mut data = vec![0u8; n * n * 4];
        paint_night_mask(&mut data, l);
        let px = (((u + 1.0) * 0.5 * n as f32) as usize).min(n - 1);
        let py = (((1.0 - v) * 0.5 * n as f32) as usize).min(n - 1);
        data[(py * n + px) * 4 + 3]
    }

    #[test]
    fn la_noche_nunca_se_sale_del_disco() {
        // La sombra era un sprite de 2,2 radios corrido 0,78 radios: se salía del
        // planeta como una mancha negra sobre el halo. Ahora, para cualquier luz,
        // fuera del círculo no hay ni un píxel de sombra.
        let n = NIGHT_MASK_N;
        for l in [Vec3::X, -Vec3::X, Vec3::Z, -Vec3::Z, Vec3::new(0.3, -0.8, -0.5).normalize()] {
            let mut data = vec![0u8; n * n * 4];
            paint_night_mask(&mut data, l);
            for py in 0..n {
                for px in 0..n {
                    let u = (px as f32 + 0.5) / n as f32 * 2.0 - 1.0;
                    let v = 1.0 - (py as f32 + 0.5) / n as f32 * 2.0;
                    if u * u + v * v > 1.0 {
                        assert_eq!(data[(py * n + px) * 4 + 3], 0, "sombra fuera del disco con luz {l:?}");
                    }
                }
            }
        }
        // Luz de frente: nada de sombra. Luz de atrás: todo sombra.
        assert_eq!(mask_alpha_at(Vec3::Z, 0.0, 0.0), 0);
        assert_eq!(mask_alpha_at(-Vec3::Z, 0.0, 0.0), 255);
    }

    #[test]
    fn la_sombra_no_salta_al_volver_al_planeta() {
        // "Al salir de la Tierra y volver a entrar, la sombra reemplaza al dibujo de
        // la Tierra": la luz del cuerpo propio pasaba de la órbita a la hora en UN
        // frame al cruzar 1,6 radios. Acercándose a pasos de 0,001 radios, la luz
        // tiene que moverse de a poco, de día y de noche.
        let r = body_radius(CelestialBody::Earth);
        let sun = Vec2::new(-1.0, 0.2).normalize();
        for daylight in [0.0, 0.5, 1.0] {
            let mut prev: Option<Vec3> = None;
            for i in 0..2_500 {
                let d = r * (2.5 - i as f32 * 0.001);
                let pie = Vec2::new(0.6, 0.8) * d;
                let l = night_light_dir(sun, Some(pie), r, daylight);
                assert!((l.length() - 1.0).abs() < 1e-3);
                if let Some(q) = prev {
                    assert!(l.dot(q) > 0.995, "la luz saltó a {:.3} radios (día {daylight})", d / r);
                }
                prev = Some(l);
            }
        }
    }

    #[test]
    fn de_dia_el_jugador_queda_al_sol_y_de_noche_a_la_sombra() {
        // La noche del disco y el velo de la superficie leen el mismo `daylight`:
        // subir de día no puede mostrarte abajo de noche (el titileo de 29 sep).
        let r = body_radius(CelestialBody::Earth);
        let sun = Vec2::new(0.0, 1.0);
        let pie = Vec2::new(0.3, -0.2) * r;
        let dia = night_light_dir(sun, Some(pie), r, 1.0);
        let noche = night_light_dir(sun, Some(pie), r, 0.0);
        assert_eq!(mask_alpha_at(dia, 0.3, -0.2), 0, "de día hay sombra a tus pies");
        assert_eq!(mask_alpha_at(noche, 0.3, -0.2), 255, "de noche estás al sol");
    }
}
