//! EL ESPACIO VIVO — luz, cuerpos menores, polvo y reentrada.
//!
//! `universe.rs` pone la geometría (dónde está cada cosa y a qué escala se ve).
//! Este módulo pone lo que hace que el espacio se sienta un LUGAR y no un fondo
//! negro con círculos:
//!
//! - **Luz.** Todo lo que brilla lo hace con un halo radial translúcido — la
//!   misma pieza para la atmósfera de un planeta, la corona del Sol, la cabeza
//!   de un cometa. Antes las atmósferas eran dos aros 2D planos con un borde
//!   duro; ahora son luz que se apaga hacia afuera.
//! - **Cuerpos menores.** El cinturón de asteroides y los cometas de
//!   `ha_shared::minor_bodies`, orbitando el Sol en el mismo plano y con el mismo
//!   reloj que los planetas. Los cometas encienden su cola al acercarse al Sol y
//!   siempre la llevan en contra de él.
//! - **Polvo.** Motas que se mueven con parallax respecto de la cámara: dan
//!   sensación de velocidad y de profundidad en un vacío que si no es estático.
//! - **Reentrada.** Bajar rápido dentro de una atmósfera calienta: destello
//!   naranja creciente y sacudida. Es la recompensa de que altura y velocidad
//!   ahora sean magnitudes de verdad.

use bevy::prelude::*;
use ha_shared::minor_bodies::{asteroid_at, comet_at, ASTEROID_COUNT, COMET_COUNT};
use ha_shared::{body_atmo_top, CelestialBody};

use crate::camera::{CameraShakeEvent, CameraZoom, GameCamera};
use crate::creative_fly::CreativeFlight;
use crate::solar_system_clock::SolarSystemClock;
use crate::universe::{body_offset_from_world, GlowTexture, UniverseFrame};

#[derive(Component)]
struct BeltRock {
    index: usize,
}

#[derive(Component)]
struct CometHead {
    index: usize,
}

#[derive(Component)]
struct CometTail {
    index: usize,
}

/// Mota de polvo espacial, con su profundidad de parallax (0 = pegada a la
/// cámara, 1 = infinitamente lejos).
#[derive(Component)]
struct SpaceMote {
    unit: Vec2,
    depth: f32,
    size: f32,
}

/// Velo naranja de reentrada (nodo de UI, encima del mundo y debajo del HUD).
#[derive(Component)]
struct ReentryGlow;

pub struct SpaceFxPlugin;

impl Plugin for SpaceFxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_reentry_overlay)
            .add_systems(
                Update,
                (
                    spawn_minor_bodies,
                    sync_minor_bodies,
                    spawn_space_motes,
                    sync_space_motes,
                    sync_reentry_heat,
                ),
            );
    }
}

// ---------------------------------------------------------------------------
// Cinturón de asteroides + cometas
// ---------------------------------------------------------------------------

fn spawn_minor_bodies(
    mut commands: Commands,
    glow: Option<Res<GlowTexture>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    existing: Query<Entity, With<BeltRock>>,
) {
    let Some(glow) = glow else { return };
    if !existing.is_empty() {
        return;
    }
    // Una sola malla unitaria para las 220 rocas: la escala de cada Transform le
    // da su radio. Cuatro tonos de gris compartidos para que no sean clones.
    let unit = meshes.add(Circle::new(1.0));
    let shades: Vec<Handle<ColorMaterial>> = [0.34, 0.42, 0.5, 0.58]
        .iter()
        .map(|&g| materials.add(ColorMaterial::from_color(Color::srgb(g, g * 0.96, g * 0.9))))
        .collect();
    for i in 0..ASTEROID_COUNT {
        commands.spawn((
            BeltRock { index: i },
            Mesh2d(unit.clone()),
            MeshMaterial2d(shades[i % shades.len()].clone()),
            Transform::from_xyz(0.0, 0.0, -18.0),
        ));
    }
    for i in 0..COMET_COUNT {
        // La cola va DETRÁS de la cabeza y se dibuja estirada hacia el
        // anti-Sol; ambas son el mismo halo, sólo cambia la forma.
        commands.spawn((
            CometTail { index: i },
            Sprite {
                image: glow.0.clone(),
                color: Color::srgba(0.65, 0.85, 1.0, 0.0),
                ..default()
            },
            Transform::from_xyz(0.0, 0.0, -17.5),
        ));
        commands.spawn((
            CometHead { index: i },
            Sprite {
                image: glow.0.clone(),
                color: Color::srgba(0.85, 0.95, 1.0, 0.0),
                ..default()
            },
            Transform::from_xyz(0.0, 0.0, -17.0),
        ));
    }
}

#[allow(clippy::type_complexity)]
fn sync_minor_bodies(
    frame: Res<UniverseFrame>,
    clock: Res<SolarSystemClock>,
    zoom: Res<CameraZoom>,
    mut rocks: Query<(&BeltRock, &mut Transform, &mut Visibility), Without<CometHead>>,
    mut heads: Query<
        (&CometHead, &mut Transform, &mut Sprite),
        (Without<BeltRock>, Without<CometTail>),
    >,
    mut tails: Query<
        (&CometTail, &mut Transform, &mut Sprite),
        (Without<BeltRock>, Without<CometHead>),
    >,
) {
    let t = clock.t;
    let r#ref = frame.ref_body;
    let view_h = zoom.live_view_h.max(1.0);

    for (rock, mut tf, mut vis) in &mut rocks {
        let a = asteroid_at(rock.index, t);
        let p = body_offset_from_world(a.pos, r#ref, t);
        // Por debajo de ~medio píxel no vale la pena dibujarla; un cinturón de
        // 220 puntos sub-píxel sólo produce parpadeo.
        let on_screen = a.radius_m * 2.0 / view_h;
        *vis = if on_screen < 0.0015 { Visibility::Hidden } else { Visibility::Visible };
        tf.translation = Vec3::new(p.x, p.y, -18.0);
        tf.rotation = Quat::from_rotation_z(a.spin);
        // Escala mínima: cuando el cinturón se ve de lejos las rocas siguen
        // leyéndose como polvo en vez de desaparecer.
        let r = a.radius_m.max(view_h * 0.0012);
        tf.scale = Vec3::new(r, r * 0.82, 1.0);
    }

    for (comet, mut tf, mut sprite) in &mut heads {
        let c = comet_at(comet.index, t);
        let p = body_offset_from_world(c.pos, r#ref, t);
        tf.translation = Vec3::new(p.x, p.y, -17.0);
        let r = (c.radius_m * 6.0).max(view_h * 0.01);
        sprite.custom_size = Some(Vec2::splat(r * 2.0));
        sprite.color = Color::srgba(0.85, 0.95, 1.0, 0.35 + 0.5 * c.activity);
    }

    for (comet, mut tf, mut sprite) in &mut tails {
        let c = comet_at(comet.index, t);
        let p = body_offset_from_world(c.pos, r#ref, t);
        // Dirección de la cola en coordenadas de render (y = −z).
        let dir = Vec2::new(c.tail_dir[0], -c.tail_dir[2]).normalize_or_zero();
        // Larga y encendida cerca del Sol, corta y apagada lejos.
        let len = (c.radius_m * 90.0 * (0.25 + c.activity)).max(view_h * 0.02);
        let wide = (c.radius_m * 8.0).max(view_h * 0.006);
        let mid = p + dir * len * 0.5;
        tf.translation = Vec3::new(mid.x, mid.y, -17.5);
        tf.rotation = Quat::from_rotation_z(dir.y.atan2(dir.x));
        sprite.custom_size = Some(Vec2::new(len, wide));
        sprite.color = Color::srgba(0.65, 0.85, 1.0, 0.10 + 0.35 * c.activity);
    }
}

// ---------------------------------------------------------------------------
// Polvo espacial con parallax
// ---------------------------------------------------------------------------

/// Cuántas motas. Son hijas de la cámara y muy pequeñas: coste despreciable.
const MOTE_COUNT: usize = 120;

fn spawn_space_motes(
    mut commands: Commands,
    camera: Query<Entity, With<GameCamera>>,
    existing: Query<Entity, With<SpaceMote>>,
) {
    if !existing.is_empty() {
        return;
    }
    let Ok(cam) = camera.get_single() else { return };
    let mut seed = 0x51ed_2b39u32;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (seed & 0xffff) as f32 / 65535.0
    };
    commands.entity(cam).with_children(|p| {
        for _ in 0..MOTE_COUNT {
            let unit = Vec2::new((rnd() - 0.5) * 2.6, (rnd() - 0.5) * 1.7);
            // Profundidad: las cercanas se mueven mucho con la cámara, las
            // lejanas casi nada. Eso es lo que da sensación de velocidad.
            let depth = 0.15 + rnd() * 0.8;
            p.spawn((
                SpaceMote { unit, depth, size: 0.0016 + rnd() * 0.0035 },
                Sprite::from_color(Color::srgba(0.75, 0.8, 0.95, 0.0), Vec2::ONE),
                Transform::from_xyz(0.0, 0.0, -350.0),
                Visibility::Hidden,
            ));
        }
    });
}

#[allow(clippy::type_complexity)]
fn sync_space_motes(
    fly: Res<CreativeFlight>,
    zoom: Res<CameraZoom>,
    camera: Query<&Transform, (With<GameCamera>, Without<SpaceMote>)>,
    mut motes: Query<(&SpaceMote, &mut Transform, &mut Sprite, &mut Visibility)>,
) {
    let Ok(cam) = camera.get_single() else { return };
    // Sólo en el espacio: en la superficie el polvo sería ruido sobre el mundo.
    let show = fly.in_orbit();
    let view_h = zoom.live_view_h.max(1.0);
    let cam_p = cam.translation.truncate();
    for (mote, mut tf, mut sprite, mut vis) in &mut motes {
        if !show {
            *vis = Visibility::Hidden;
            continue;
        }
        *vis = Visibility::Visible;
        // Deriva por parallax: la posición de la cámara arrastra a la mota una
        // fracción de su recorrido, envuelta en la pantalla para que nunca se
        // acabe el campo.
        let span = view_h * 2.6;
        let drift = -cam_p * (1.0 - mote.depth) / span;
        let wrap = |v: f32| (v + 0.5).rem_euclid(1.0) - 0.5;
        let x = wrap(mote.unit.x / 2.6 + drift.x) * 2.6;
        let y = wrap(mote.unit.y / 1.7 + drift.y) * 1.7;
        tf.translation.x = x * view_h;
        tf.translation.y = y * view_h;
        sprite.custom_size = Some(Vec2::splat(mote.size * view_h));
        sprite.color = Color::srgba(0.75, 0.8, 0.95, 0.10 + 0.35 * (1.0 - mote.depth));
    }
}

// ---------------------------------------------------------------------------
// Reentrada
// ---------------------------------------------------------------------------

/// Velocidad de bajada (m/s) a partir de la cual la atmósfera empieza a
/// calentar. Por debajo se puede descender tranquilo sin efectos.
const REENTRY_MIN_SPEED: f32 = 900.0;
/// Velocidad a la que el efecto está al máximo.
const REENTRY_FULL_SPEED: f32 = 6_000.0;

fn spawn_reentry_overlay(mut commands: Commands) {
    commands.spawn((
        ReentryGlow,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(Color::srgba(1.0, 0.45, 0.12, 0.0)),
        // Encima del mundo, debajo del HUD (igual que la bruma atmosférica).
        GlobalZIndex(-40),
        bevy::ui::FocusPolicy::Pass,
        Name::new("ReentryGlow"),
    ));
}

/// Entrar rápido en una atmósfera calienta: destello naranja creciente y
/// sacudida. Sólo donde hay aire — caer sobre la Luna no quema nada.
fn sync_reentry_heat(
    fly: Res<CreativeFlight>,
    mut shake: EventWriter<CameraShakeEvent>,
    mut glow: Query<&mut BackgroundColor, With<ReentryGlow>>,
    time: Res<Time>,
) {
    let atmo = body_atmo_top(fly.body);
    let heat = if !fly.flying() || atmo <= 0.0 || fly.vy >= -REENTRY_MIN_SPEED {
        0.0
    } else {
        // Profundidad dentro de la atmósfera × exceso de velocidad de caída.
        let depth = 1.0 - (fly.alt / atmo).clamp(0.0, 1.0);
        let speed = ((-fly.vy - REENTRY_MIN_SPEED) / (REENTRY_FULL_SPEED - REENTRY_MIN_SPEED))
            .clamp(0.0, 1.0);
        depth * speed
    };
    for mut bg in &mut glow {
        // Parpadeo rápido: es fuego, no un filtro.
        let flicker = 0.85 + 0.15 * (time.elapsed_secs() * 27.0).sin();
        *bg = BackgroundColor(Color::srgba(1.0, 0.45, 0.12, heat * 0.5 * flicker));
    }
    if heat > 0.05 {
        shake.send(CameraShakeEvent { trauma: heat * 0.06 });
    }
}

/// Luz que un cuerpo emite o refleja, 0..1 — la usa `universe.rs` para decidir
/// cuánto halo dibujarle. El Sol brilla por sí mismo; los demás reflejan, y los
/// que tienen aire dispersan bastante más.
pub fn body_luminosity(body: CelestialBody) -> f32 {
    match body {
        CelestialBody::Sun => 1.0,
        _ if body_atmo_top(body) > 0.0 => 0.55,
        _ => 0.22,
    }
}
