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
/// Metros de encuadre por metro de altura cerca del suelo. Bajo se abre
/// despacio: a 1 km de altura ves ~2.6 km de mundo, que es la altura de crucero
/// donde todavía distinguís bosques y costas.
const NEAR_ZOOM_RATE: f32 = 2.4;
/// Lejos del suelo el encuadre es PROPORCIONAL a la distancia al centro del
/// cuerpo, así el planeta se lee como un disco que crece al bajar y se achica
/// al subir (SFS). `view_h = (radio + alt) * esto`.
const FAR_ZOOM_FACTOR: f32 = 2.2;
/// Altura a la que el encuadre ya es 100 % el proporcional. Es MEDIO RADIO del
/// cuerpo, no un número fijo: en un mundo de 30 km de radio, quedarse con los
/// 600 m de la escala vieja te sacaba a vista de planeta entero apenas
/// despegabas.
fn far_blend_alt(body: CelestialBody) -> f32 {
    (body_radius(body) * 0.5).max(400.0)
}
/// Techo de altura (m) — alcanza para encuadrar el sistema solar entero.
pub const MAX_ALT: f32 = 2_000_000.0;

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
pub fn climb_rate(alt: f32) -> f32 {
    (30.0 + alt.max(0.0) * 0.5).min(40_000.0)
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
    (45.0 + alt.max(0.0) * 0.035).min(30_000.0)
}

// ---------------------------------------------------------------------------
// LOD del suelo — de una piedra al sistema solar, sin cortes
// ---------------------------------------------------------------------------

/// Los tiles finos (1 m) empiezan a desvanecerse acá…
pub const FINE_FADE_VIEW_H: f32 = 70.0;
/// …y ya no están acá (el anillo streameado dejaría de llenar la pantalla).
pub const FINE_OFF_VIEW_H: f32 = 130.0;
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
/// resto del juego (HUD, audio, supervivencia): el techo de su atmósfera, con
/// un mínimo para las lunas sin aire.
pub fn space_alt(body: CelestialBody) -> f32 {
    body_atmo_top(body).max(60.0)
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

/// La cara nocturna de un cuerpo: disco oscuro desplazado hacia el lado opuesto
/// al Sol. Lo que queda sin tapar es el creciente iluminado.
#[derive(Component)]
struct BodyNightSide {
    body: CelestialBody,
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
}

/// Mantiene la cara nocturna de cada cuerpo mirando en contra del Sol.
fn sync_terminators(
    frame: Res<UniverseFrame>,
    clock: Res<SolarSystemClock>,
    mut night: Query<(&BodyNightSide, &mut Transform)>,
) {
    let t = clock.t;
    for (n, mut tf) in &mut night {
        let o = body_offset(n.body, frame.ref_body, t);
        // Desplazado 0.62 radios hacia el anti-Sol: deja un creciente iluminado
        // de un tercio del disco, que es como se ve un planeta desde fuera de su
        // órbita. Al orbitar, el creciente gira solo.
        let away = -sun_dir_from(n.body, t) * body_radius(n.body) * 0.78;
        tf.translation = Vec3::new(o.x + away.x, o.y + away.y, -19.5);
    }
}

pub struct UniversePlugin;

impl Plugin for UniversePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UniverseFrame>()
            .init_resource::<TerrainLodState>()
            .add_systems(Startup, (spawn_glow_texture, spawn_universe).chain())
            .add_systems(
                Update,
                (sync_universe, sync_terminators, sync_body_labels, sync_terrain_lod).chain(),
            );
    }
}

/// Un disco por cuerpo + halo de atmósfera + anillo de órbita + etiqueta. Se
/// crean UNA vez y viven siempre: no hay "entrar al espacio", sólo alejarse.
fn spawn_universe(
    mut commands: Commands,
    glow: Res<GlowTexture>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for &body in CelestialBody::all() {
        let r = body_radius(body);
        let base = ha_shared::body_def(body).color;
        let disc_color = if body == CelestialBody::Sun {
            Color::srgb(1.0, 0.88, 0.45)
        } else {
            Color::srgb(base[0], base[1], base[2])
        };
        commands.spawn((
            BodyDisc { body, textured: false },
            Mesh2d(meshes.add(Circle::new(r))),
            MeshMaterial2d(materials.add(ColorMaterial::from_color(disc_color))),
            // Detrás de todo el suelo (tiles finos z≈0, satelital z=-1).
            Transform::from_xyz(0.0, 0.0, -20.0),
        ));

        // LUZ, no aros. La atmósfera y la corona son halos radiales
        // translúcidos (`GlowTexture`): se apagan hacia afuera y no dejan
        // ningún borde duro por más que se agranden. Antes eran dos círculos
        // planos concéntricos y se leían como un dónut gris dibujado encima.
        let atmo = body_atmo_top(body);
        let lum = crate::space_fx::body_luminosity(body);
        if atmo > 0.0 {
            let tint = ha_shared::transition_profile(body).cover_color;
            // Dos capas: una ancha y tenue (la dispersión que se ve desde lejos)
            // y una ceñida al limbo (el arco azul del amanecer visto en órbita).
            for (spread, alpha, z) in
                [(2.1_f32, 0.13_f32, -21.5_f32), (1.28, 0.26, -21.0)]
            {
                commands.spawn((
                    BodyHalo { body },
                    Sprite {
                        image: glow.0.clone(),
                        color: Color::srgba(tint[0], tint[1], tint[2], alpha * lum),
                        custom_size: Some(Vec2::splat(r * 2.0 * spread)),
                        ..default()
                    },
                    Transform::from_xyz(0.0, 0.0, z),
                ));
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
                Transform::from_xyz(0.0, 0.0, -21.0),
            ));
        }

        // El Sol es la fuente: una corona enorme y cálida, en varias capas, que
        // ilumina de verdad el rincón del sistema donde estés.
        if body == CelestialBody::Sun {
            for (spread, alpha, z) in
                [(9.0_f32, 0.10_f32, -22.0_f32), (4.2, 0.20, -21.6), (2.0, 0.38, -21.2)]
            {
                commands.spawn((
                    BodyHalo { body },
                    Sprite {
                        image: glow.0.clone(),
                        color: Color::srgba(1.0, 0.82, 0.42, alpha),
                        custom_size: Some(Vec2::splat(r * 2.0 * spread)),
                        ..default()
                    },
                    Transform::from_xyz(0.0, 0.0, z),
                ));
            }
        } else {
            // TERMINADOR: la mitad que no mira al Sol está a oscuras. Es un
            // disco oscuro del mismo radio, desplazado hacia el anti-Sol — el
            // creciente que queda es la parte iluminada, y gira solo conforme
            // el cuerpo recorre su órbita.
            commands.spawn((
                BodyNightSide { body },
                Sprite {
                    image: glow.0.clone(),
                    // Sombra SUAVE, no un disco recortado: un círculo oscuro
                    // desplazado se salía del planeta y se leía como anillos
                    // negros concéntricos encima del halo. Con el degradado
                    // radial la noche se difumina y el limbo queda limpio.
                    color: Color::srgba(0.01, 0.01, 0.03, 0.58),
                    custom_size: Some(Vec2::splat(r * 2.2)),
                    ..default()
                },
                Transform::from_xyz(0.0, 0.0, -19.5),
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
                Transform::from_xyz(0.0, 0.0, -25.0),
            ));
        }

        // Etiqueta ASCII (regla de fuente del juego), tamaño de pantalla
        // constante — ver `sync_body_labels`.
        commands.spawn((
            BodyLabel { body },
            Text2d::new(ha_shared::body_def(body).label),
            TextFont { font_size: 13.0, ..default() },
            TextColor(Color::srgba(0.85, 0.88, 0.92, 0.0)),
            Transform::from_xyz(0.0, 0.0, -19.0),
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
    tiles: Res<TileWorld>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut discs: Query<(&mut BodyDisc, &mut Transform, &MeshMaterial2d<ColorMaterial>)>,
    mut halos: Query<(&BodyHalo, &mut Transform), (Without<BodyDisc>, Without<OrbitRing>)>,
    mut rings: Query<
        (&OrbitRing, &mut Transform, &Mesh2d, &MeshMaterial2d<ColorMaterial>),
        (Without<BodyDisc>, Without<BodyHalo>),
    >,
) {
    let t = clock.t;
    let r#ref = frame.ref_body;

    // Una textura de terreno por frame como mucho: generarlas es caro y no hay
    // ninguna prisa (el disco ya se ve con su color de catálogo mientras tanto).
    let mut textured_this_frame = false;

    for (mut disc, mut tf, mat) in &mut discs {
        let o = body_offset(disc.body, r#ref, t);
        tf.translation = Vec3::new(o.x, o.y, -20.0);

        if !disc.textured && !textured_this_frame && disc.body != CelestialBody::Sun {
            // Sólo vale la pena cuando el disco ya ocupa algo en pantalla.
            let on_screen = body_radius(disc.body) * 2.0 / zoom.live_view_h.max(1.0);
            if on_screen > 0.02 {
                if let Some(m) = materials.get_mut(&mat.0) {
                    m.texture = Some(images.add(body_surface_texture(&tiles, disc.body)));
                    m.color = Color::WHITE;
                }
                disc.textured = true;
                textured_this_frame = true;
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
        tf.translation = Vec3::new(center.x, center.y, -25.0);
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
        tf.translation = Vec3::new(o.x, o.y + r + view_h * 0.035, -19.0);
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
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    player: Query<&Transform, With<crate::sim::Player>>,
    mut lod: Query<
        (Entity, &Mesh2d, &MeshMaterial2d<ColorMaterial>),
        (With<TerrainLod>, Without<crate::sim::Player>),
    >,
) {
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
        return;
    };
    if view_h < LOD_ON_VIEW_H || !body_landable(body) {
        drop_it(&mut commands, &lod);
        state.body = None;
        return;
    }

    let tile_m = lod_tile_m(view_h);
    let span = LOD_QUADS as f32 * tile_m;
    let p = player_tf.translation.truncate();
    let center = quantize(p, tile_m);
    // Se rehace al cambiar de escalón, de cuerpo, al editar tiles, o cuando el
    // jugador se salió de la cuarta parte central (recentrar antes de que el
    // borde asome).
    let needs_rebuild = state.body != Some(body)
        || state.tile_m != tile_m
        || state.version != tiles.version
        || (p - state.center).length() > span * 0.25;

    if let Ok((_, mesh, mat)) = lod.get_single_mut() {
        if needs_rebuild {
            meshes.insert(mesh.0.id(), build_terrain_lod_mesh(&tiles, body, center, tile_m));
            *state = TerrainLodState {
                body: Some(body),
                center,
                tile_m,
                version: tiles.version,
            };
        }
        if let Some(m) = materials.get_mut(&mat.0) {
            m.color = Color::WHITE;
        }
        return;
    }

    commands.spawn((
        TerrainLod,
        Mesh2d(meshes.add(build_terrain_lod_mesh(&tiles, body, center, tile_m))),
        MeshMaterial2d(materials.add(ColorMaterial::from_color(Color::WHITE))),
        // Entre el disco del planeta (z −20) y los tiles finos (z 0).
        Transform::from_xyz(0.0, 0.0, -1.0),
    ));
    *state = TerrainLodState {
        body: Some(body),
        center,
        tile_m,
        version: tiles.version,
    };
}

fn quantize(p: Vec2, tile_m: f32) -> Vec2 {
    let q = tile_m * 8.0;
    Vec2::new((p.x / q).round() * q, (p.y / q).round() * q)
}

/// Malla de `LOD_QUADS²` quads de `tile_m` metros centrada en `center`
/// (coordenadas de RENDER), recortada al disco del cuerpo. Un color por bloque,
/// promediado del mismo `TileWorld` que los tiles finos → sin costura de color
/// al fundirse, y sin ruido de sal y pimienta al alejarse.
fn build_terrain_lod_mesh(
    tiles: &TileWorld,
    body: CelestialBody,
    center: Vec2,
    tile_m: f32,
) -> Mesh {
    let radius = body_walk_radius(body);
    let n = LOD_QUADS;
    let half = n as f32 * tile_m * 0.5;
    let cap = (n * n) as usize;
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(cap * 4);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(cap * 4);
    let mut indices: Vec<u32> = Vec::with_capacity(cap * 6);

    for iy in 0..n {
        for ix in 0..n {
            let x0 = center.x - half + ix as f32 * tile_m;
            let y0 = center.y - half + iy as f32 * tile_m;
            let cx = x0 + tile_m * 0.5;
            let cy = y0 + tile_m * 0.5;
            // RECORTE AL CUERPO: la malla no puede salirse del disco, o el
            // planeta se queda sin borde y el suelo llena el espacio.
            if Vec2::new(cx, cy).length() > radius {
                continue;
            }
            let rgb = average_tile_color(tiles, body, cx, cy, tile_m, 2);
            let lin = Color::srgb(rgb[0], rgb[1], rgb[2]).to_linear();
            let base = positions.len() as u32;
            positions.push([x0, y0, 0.0]);
            positions.push([x0 + tile_m, y0, 0.0]);
            positions.push([x0 + tile_m, y0 + tile_m, 0.0]);
            positions.push([x0, y0 + tile_m, 0.0]);
            for _ in 0..4 {
                colors.push([lin.red, lin.green, lin.blue, 1.0]);
            }
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
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

/// Color medio del terreno en un cuadrado de `size` metros centrado en un punto
/// de RENDER `(rx, ry)`. Promediar es lo que convierte el ruido de tiles de 1 m
/// en bioma legible cuando un píxel cubre metros de mundo: sin esto, tanto el
/// mapa satelital como el disco del planeta se ven como estática.
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
            let c = tiles.tile_at(body, tx, tz).color();
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

/// El disco del planeta pintado con SU PROPIO terreno (`topdown_tile`), no con
/// ruido inventado: la costa que ves desde el espacio es la misma que pisás. Con
/// oscurecimiento hacia el limbo para que lea como una esfera, y transparente
/// fuera del círculo.
fn body_surface_texture(tiles: &TileWorld, body: CelestialBody) -> Image {
    let n = DISC_TEX_N;
    let r = body_radius(body);
    let seed = ha_shared::body_def(body).terrain_seed;
    let mut data = vec![0u8; n * n * 4];
    for py in 0..n {
        for px in 0..n {
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
            let rgb = average_tile_color(tiles, body, u * 2.0 * r, -v * 2.0 * r, texel_m, 2);
            // Relieve: un sombreado suave de ruido para que un cuerpo de terreno
            // UNIFORME (la Luna es todo regolito) no se lea como un círculo de
            // pintura plana. Es ±8 %, no inventa bioma — sólo da volumen.
            let relief = 0.92 + 0.16 * fbm(u * 9.0 + 4.0, v * 9.0 + 4.0, seed);
            // Limbo oscurecido → lectura esférica.
            let shade = relief * (1.0 - (d / 0.5).powi(2) * 0.42);
            data[idx] = ((rgb[0] * shade).clamp(0.0, 1.0) * 255.0) as u8;
            data[idx + 1] = ((rgb[1] * shade).clamp(0.0, 1.0) * 255.0) as u8;
            data[idx + 2] = ((rgb[2] * shade).clamp(0.0, 1.0) * 255.0) as u8;
            data[idx + 3] = 255;
        }
    }
    // Desenfoque de caja 3×3 sobre el RGB (respetando el alpha del recorte). El
    // promediado por texel ya baja el ruido, pero a la escala del disco un
    // píxel sigue siendo metros de terreno: sin esto el planeta se ve granulado
    // como estática en vez de con biomas. Dos pasadas, muy barato (512²).
    box_blur_rgb(&mut data, n, 2);

    let mut image = Image::new(
        Extent3d { width: n as u32, height: n as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    // Lineal: el disco se ve a todas las escalas, desde llenar la pantalla hasta
    // ser un punto, y con muestreo por vecino más próximo el limbo se escalona.
    image.sampler = bevy::image::ImageSampler::linear();
    image
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
}
