//! Retención visual — **luciérnagas del entorno** (FASE H4, revisado).
//!
//! Aparecen desde el ATARDECER (no sólo de noche cerrada) y son **parte del
//! mundo, NO de la cámara**: cada una vive en una posición de MUNDO y se queda
//! ahí. Caminás y las pasás de largo — se corren respecto a vos (paralaje real,
//! que es lo que se siente "mágico"); antes iban pegadas al jugador como un
//! enjambre de mascotas y el efecto se leía plano y falso.
//!
//! Cuando una queda demasiado lejos NO se teletransporta al borde de la vista
//! (eso es lo que delata el truco): se **recicla en una celda de mundo nueva**
//! elegida por hash determinista alrededor tuyo, con su propio ritmo. Como el
//! resto del mundo, es un dato con activación por proximidad.
//!
//! El brillo es un **blur neón**: la misma textura de gradiente radial que usa
//! `light2d` (dos quads: núcleo chiquito + halo difuso) en vez del cuadradito
//! plano de antes. Cuesta lo mismo — es un `Handle<Image>` compartido, batchea.

use bevy::prelude::*;

use crate::light2d::GlowTexture;
use crate::scene::RegionRuntime;
use crate::sim::Player;
use crate::sky2d::DayClock;
use crate::world2d::{render_xy, surface_region_active};

/// Cuántas luciérnagas viven a la vez. Son 2 quads cada una, todos batcheados.
const N: usize = 40;
/// Se encienden cuando la luz del día baja de esto (1 = mediodía). 0.55 agarra
/// el atardecer entero, que es cuando mejor se ven.
const DUSK_ON: f32 = 0.55;
/// Radio (m) alrededor del jugador donde viven. Más allá se reciclan.
const FIELD_R: f32 = 26.0;
/// Z de dibujo: sobre el velo de noche (~140) y sobre las luces (700..703).
const FLY_Z: f32 = 900.0;

#[derive(Component)]
struct Firefly {
    /// Posición de MUNDO (no offset del jugador) — esto es lo que da el paralaje.
    x: f32,
    z: f32,
    /// Deriva propia, lenta.
    vx: f32,
    vz: f32,
    /// Fase y velocidad del latido.
    ph: f32,
    pulse: f32,
    /// Semilla estable para reciclar sin repetir patrones.
    seed: u32,
}

/// El halo difuso; el hijo con el núcleo va aparte para el look "neón".
#[derive(Component)]
struct FireflyCore;

#[derive(Resource, Default)]
struct FirefliesReady(bool);

pub struct AmbientPlugin;

impl Plugin for AmbientPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FirefliesReady>()
            .add_systems(Update, (spawn_fireflies, update_fireflies).chain());
    }
}

/// xorshift determinista → posiciones/ritmos variados sin `rand`.
fn hash01(mut s: u32) -> f32 {
    s ^= s << 13;
    s ^= s >> 17;
    s ^= s << 5;
    (s % 100_000) as f32 / 100_000.0
}

/// Coloca (o recicla) una luciérnaga en una posición de mundo alrededor del
/// jugador, en un anillo: nunca justo encima tuyo, nunca fuera de cuadro.
fn place(f: &mut Firefly, px: f32, pz: f32, tick: u32) {
    f.seed = f.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223 ^ tick);
    let ang = hash01(f.seed) * std::f32::consts::TAU;
    let rad = FIELD_R * (0.35 + 0.6 * hash01(f.seed ^ 0x9E37_79B9));
    f.x = px + ang.cos() * rad;
    f.z = pz + ang.sin() * rad;
    f.vx = (hash01(f.seed ^ 0x51ED_2701) - 0.5) * 0.55;
    f.vz = (hash01(f.seed ^ 0x1B87_3593) - 0.5) * 0.55;
    f.pulse = 0.9 + hash01(f.seed ^ 0x2545_F491) * 2.2;
    f.ph = hash01(f.seed ^ 0x7FEB_352D) * 6.28;
}

fn spawn_fireflies(
    mut commands: Commands,
    glow: Option<Res<GlowTexture>>,
    mut ready: ResMut<FirefliesReady>,
) {
    let Some(glow) = glow else { return }; // la textura se genera en PreStartup
    if ready.0 {
        return;
    }
    ready.0 = true;
    for i in 0..N {
        let mut f = Firefly { x: 0.0, z: 0.0, vx: 0.0, vz: 0.0, ph: 0.0, pulse: 1.5, seed: 0x9E37_79B9 ^ i as u32 };
        place(&mut f, 0.0, 0.0, i as u32);
        commands
            .spawn((
                f,
                // Halo: el "blur neón" alrededor del bicho.
                Sprite {
                    image: glow.0.clone(),
                    custom_size: Some(Vec2::splat(0.9)),
                    color: Color::NONE,
                    ..default()
                },
                Transform::from_xyz(0.0, 0.0, FLY_Z),
                Visibility::Hidden,
                // Micro-luz: agujerea el velo y tiñe el pasto; sin quads extra
                // (ya trae su propio bloom).
                crate::light2d::Light2d {
                    color: Color::srgb(0.55, 1.0, 0.40),
                    // MICRO-pozo: el alcance real es `radius * LIGHT_REACH`, así
                    // que 1.5 daba un charco de 8 m — del tamaño del de un farol,
                    // y con decenas de bichos el bosque quedaba lleno de discos
                    // verdes lechosos. Un bicho ilumina un palmo de pasto.
                    radius: 0.7,
                    intensity: 0.35,
                    offset: Vec2::ZERO,
                    flicker: 0.85,
                    day: false,
                    draw_glow: false,
                },
            ))
            .with_children(|p| {
                // Núcleo: el punto vivo, casi blanco.
                p.spawn((
                    FireflyCore,
                    Sprite {
                        image: glow.0.clone(),
                        custom_size: Some(Vec2::splat(0.22)),
                        color: Color::NONE,
                        ..default()
                    },
                    Transform::from_xyz(0.0, 0.0, 0.1),
                ));
            });
    }
}

#[allow(clippy::type_complexity)]
fn update_fireflies(
    time: Res<Time>,
    clock: Res<DayClock>,
    region: Res<RegionRuntime>,
    player: Query<&Transform, With<Player>>,
    // `Without<Player>` hace este query DISJUNTO del de `player` (ambos tocan
    // Transform) → evita el panic B0001 de Bevy al arrancar.
    mut flies: Query<
        (
            &mut Firefly,
            &mut Transform,
            &mut Sprite,
            &mut Visibility,
            &Children,
            &mut crate::light2d::Light2d,
        ),
        Without<Player>,
    >,
    mut cores: Query<&mut Sprite, (With<FireflyCore>, Without<Firefly>)>,
    mut tick: Local<u32>,
) {
    let day = clock.daylight();
    let on = surface_region_active(&region).is_some() && day < DUSK_ON;
    if !on {
        for (_, _, _, mut vis, _, mut light) in &mut flies {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            light.intensity = 0.0;
        }
        return;
    }
    let Ok(pt) = player.get_single() else {
        return;
    };
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    let dt = time.delta_secs().min(0.1);
    *tick = tick.wrapping_add(1);
    // Sube desde el atardecer y llega a pleno de noche.
    let amt = ((DUSK_ON - day) / DUSK_ON).clamp(0.0, 1.0);
    for (i, (mut f, mut tf, mut sprite, mut vis, kids, mut light)) in flies.iter_mut().enumerate() {
        // Deriva propia en coordenadas de MUNDO (no del jugador).
        f.x += f.vx * dt;
        f.z += f.vz * dt;
        // Se alejó demasiado (caminaste, volaste): renace en otra parte del
        // anillo — nunca "salta" dentro de cuadro delante tuyo.
        let d2 = (f.x - px) * (f.x - px) + (f.z - pz) * (f.z - pz);
        if d2 > (FIELD_R * 1.5) * (FIELD_R * 1.5) {
            let t = *tick;
            place(&mut f, px, pz, t.wrapping_add(i as u32));
        }
        f.ph += f.pulse * dt;
        let r = render_xy(f.x, f.z);
        tf.translation = Vec3::new(r.x, r.y, FLY_Z);
        // Latido: casi se apagan y vuelven — eso es lo que las hace vivas.
        let beat = (0.5 + 0.5 * f.ph.sin()).powf(1.8);
        let a = amt * (0.12 + 0.88 * beat);
        // Verde-lima neón para el halo, blanco cálido para el núcleo.
        sprite.color = Color::srgba(0.72, 1.0, 0.45, a * 0.55);
        *vis = Visibility::Inherited;
        light.intensity = 0.15 + 0.55 * a;
        // El núcleo late con SU luciérnaga (no un promedio global): es lo que
        // hace que el campo parpadee desordenado y no como un semáforo.
        for &k in kids {
            if let Ok(mut core) = cores.get_mut(k) {
                core.color = Color::srgba(1.0, 1.0, 0.86, (a * 1.15).min(1.0));
            }
        }
    }
}
