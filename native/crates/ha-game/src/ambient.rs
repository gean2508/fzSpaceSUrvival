//! Retención visual — **las partículas vivas del entorno** (FASE H4, revisado):
//! luciérnagas de noche y **mariposas de día**. Son el mismo truco con dos
//! turnos, y ninguna de las dos se solapa con la otra (ver [`DUSK_ON`] /
//! [`DAY_ON`]): el campo nunca está muerto, pero tampoco tiene bichos de más.
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

use crate::art2d::ArtLibrary;
use crate::light2d::GlowTexture;
use crate::scene::RegionRuntime;
use crate::sim::Player;
use crate::sky2d::DayClock;
use crate::weather::Weather;
use crate::world2d::{render_xy, surface_region_active, TileWorld};
use ha_shared::topdown::TILE_M;

/// **Un bicho no atraviesa una pared.** Es la condición que le faltaba a las
/// partículas: viven en el mundo, así que el mundo las tiene que frenar. Una
/// celda SÓLIDA (muro, puerta cerrada, roca — lo que ya frena al jugador, con las
/// ediciones del editor incluidas) es un no-lugar para una luciérnaga: no entra,
/// y si al reciclarse cayó dentro de tu casa, se vuelve a sortear.
///
/// Sólo aplica en la superficie (`z == 0`): bajo tierra la grilla es la cueva y
/// ahí todo es sólido menos el túnel.
fn blocked(tiles: &TileWorld, body: ha_shared::CelestialBody, x: f32, z: f32) -> bool {
    if tiles.z != 0 {
        return false;
    }
    tiles.solid_at(body, (x / TILE_M).floor() as i64, (z / TILE_M).floor() as i64)
}

/// Cuántas motas viven a la vez (el tope; cada cuerpo usa las que declara en
/// [`MoteSpec::count`]). Son 2 quads cada una, todos batcheados.
///
/// **40, como estaba.** Se había bajado a 26 por el presupuesto de luces del velo
/// (`VEIL_LIGHTS` = 24), pero el enjambre denso es el efecto: un bosque con 26
/// bichos se lee vacío. El presupuesto ya se reparte por prioridad —las lámparas
/// cerca del jugador ganan—, así que el costo real es que en medio de un enjambre
/// alguna luciérnaga lejana no agujerea el velo, y eso no se ve.
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
            .init_resource::<ButterfliesReady>()
            // Sólo en partida: las luciérnagas y mariposas se creaban en el
            // arranque (cientos de sprites pegados a la cámara) y quedaban
            // volando detrás del menú.
            .add_systems(
                Update,
                (
                    spawn_fireflies,
                    update_fireflies,
                    spawn_butterflies,
                    update_butterflies,
                )
                    .chain()
                    .run_if(crate::game_phase::world_active),
            );
    }
}

// ── Las motas de cada mundo ──────────────────────────────────────────────────

/// Cómo son las partículas de ambiente de un cuerpo. Es la MISMA maquinaria en
/// todos lados (posición de mundo, paralaje, reciclado por hash); lo único que
/// cambia son estos números — por eso agregar un planeta cuesta cinco líneas y
/// no un sistema nuevo.
#[derive(Clone, Copy)]
struct MoteSpec {
    /// Cuántas hay activas. 0 = este cuerpo no tiene nada flotando.
    count: usize,
    /// Color del halo (el borde difuso, que es lo que más se ve).
    halo: Color,
    /// Color del punto vivo del centro.
    core: Color,
    /// Color de la LUZ que emite, que no es el mismo que el del halo: el halo es
    /// lo que ves del bicho y `emit` es lo que el bicho le hace al pasto de abajo.
    /// En la Tierra el halo es lima claro y la luz un verde más hondo — es esa
    /// diferencia la que hace que el charquito se lea como luz y no como calco.
    emit: Color,
    /// Diámetro del halo en metros.
    size: f32,
    /// Radio del `Light2d`. **0 = no ilumina**: el polvo no emite, sólo refleja.
    light: f32,
    /// Velocidad de deriva (m/s). El polvo con viento corre; un bicho pasea.
    drift: f32,
    /// `true` = late como bicho (casi se apaga y vuelve). `false` = brillo
    /// parejo, que es como se lee una partícula de polvo.
    beats: bool,
}

impl MoteSpec {
    const NONE: Self = Self {
        count: 0,
        halo: Color::NONE,
        core: Color::NONE,
        emit: Color::NONE,
        size: 0.0,
        light: 0.0,
        drift: 0.0,
        beats: false,
    };
}

/// El catálogo. Cada mundo tiene lo suyo flotando, y lo que emite luz **emite
/// luz de verdad** (`light > 0` → `Light2d` con su color, que agujerea el velo y
/// tiñe el suelo alrededor como cualquier antorcha).
fn body_motes(body: ha_shared::CelestialBody) -> MoteSpec {
    use ha_shared::CelestialBody as B;
    match body {
        // Luciérnagas. **Los valores de siempre**: halo verde-lima claro, núcleo
        // casi blanco cálido y una luz verde más honda de 0.7 de radio. Se habían
        // retocado (lima ácido, radio 0.62) mientras se buscaba otra cosa; la luz
        // de las luciérnagas nunca estuvo en discusión, así que vuelve a esto.
        B::Earth => MoteSpec {
            count: 40,
            halo: Color::srgb(0.72, 1.0, 0.45),
            core: Color::srgb(1.0, 1.0, 0.86),
            emit: Color::srgb(0.55, 1.0, 0.40),
            size: 0.9,
            light: 0.7,
            drift: 0.55,
            beats: true,
        },
        // Polvo de regolito en suspensión: no hay aire, pero cada paso levanta
        // partículas que tardan una eternidad en caer con 1/6 g. No emiten.
        B::Moon => MoteSpec {
            count: 20,
            halo: Color::srgb(0.78, 0.80, 0.86),
            core: Color::srgb(0.92, 0.94, 1.0),
            emit: Color::srgb(0.78, 0.80, 0.86),
            size: 0.42,
            light: 0.0,
            drift: 0.18,
            beats: false,
        },
        // Polvo marciano arrastrado por el viento: más rápido que ninguno, y
        // ocre. Es el planeta donde el aire SE VE.
        B::Mars => MoteSpec {
            count: 34,
            halo: Color::srgb(0.86, 0.46, 0.24),
            core: Color::srgb(1.0, 0.70, 0.42),
            emit: Color::srgb(0.86, 0.46, 0.24),
            size: 0.5,
            light: 0.0,
            drift: 1.6,
            beats: false,
        },
        // Pavesas: ceniza al rojo subiendo del suelo. SÍ iluminan — son fuego.
        B::Venus => MoteSpec {
            count: 22,
            halo: Color::srgb(1.0, 0.42, 0.10),
            core: Color::srgb(1.0, 0.80, 0.45),
            emit: Color::srgb(1.0, 0.42, 0.10),
            size: 0.7,
            light: 0.5,
            drift: 0.5,
            beats: true,
        },
        // Chispas de azufre sobre los volcanes de Io: amarillo verdoso, y arden.
        B::Io => MoteSpec {
            count: 24,
            halo: Color::srgb(0.95, 0.95, 0.25),
            core: Color::srgb(1.0, 1.0, 0.70),
            emit: Color::srgb(0.95, 0.95, 0.25),
            size: 0.62,
            light: 0.45,
            drift: 0.8,
            beats: true,
        },
        // Cristales de hielo: destellan al girar (por eso late) pero no emiten
        // nada — el brillo es reflejo.
        B::Europa => MoteSpec {
            count: 30,
            halo: Color::srgb(0.72, 0.92, 1.0),
            core: Color::srgb(0.95, 1.0, 1.0),
            emit: Color::srgb(0.72, 0.92, 1.0),
            size: 0.38,
            light: 0.0,
            drift: 0.25,
            beats: true,
        },
        // Llovizna de metano en la niebla de Titán: ámbar, densa, lenta.
        B::Titan => MoteSpec {
            count: 30,
            halo: Color::srgb(0.92, 0.66, 0.30),
            core: Color::srgb(1.0, 0.85, 0.55),
            emit: Color::srgb(0.92, 0.66, 0.30),
            size: 0.55,
            light: 0.0,
            drift: 0.35,
            beats: false,
        },
        // Roca muerta sin atmósfera: no flota nada, y ese vacío también dice algo.
        B::Mercury | B::Phobos | B::Deimos | B::Uranus | B::Neptune | B::Sun => MoteSpec::NONE,
    }
}

// ── Mariposas (el turno de día) ──────────────────────────────────────────────

/// Cuántas mariposas viven a la vez. Menos que luciérnagas a propósito: de día
/// el fondo es claro y una mariposa es un sprite opaco de 4 frames, no un punto
/// de luz — a 40 se lee como plaga, a 22 como "el campo está vivo".
const N_BUTTERFLIES: usize = 22;

/// Se despiertan cuando la luz del día sube de esto. El hueco contra
/// [`DUSK_ON`] (0.55) es a propósito: al atardecer no hay ni unas ni otras
/// durante un momento, y ese vacío es lo que hace que encenderse las
/// luciérnagas se note como un cambio de turno.
const DAY_ON: f32 = 0.68;

/// Radio (m) alrededor del jugador donde viven. Más ancho que el de las
/// luciérnagas: de día se ve mucho más lejos.
const FIELD_R_DAY: f32 = 26.0;

/// Z de dibujo: por encima del mundo, por debajo de las luciérnagas.
const FLY_Z_DAY: f32 = 880.0;

/// Por debajo de esta distancia (m) dos mariposas se apartan.
const SEPARATE_R: f32 = 1.6;
/// Entre [`SEPARATE_R`] y esto se buscan: es el radio del "juego".
const COMPANY_R: f32 = 7.0;

/// Tamaño de la CELDA en metros. La mariposa ocupa como la mitad del PNG de
/// 32×32, así que el bicho visible mide ~0.6 m: a 0.62 (lo primero que probé)
/// quedaban motas de 8 px que no se leían como mariposas ni de cerca. Es una
/// exageración a propósito, la misma que hace que un corazón de vida se vea.
const BUTTERFLY_SIZE: f32 = 1.75;

/// Los cinco slots que hay en el pack (editor de assets → `particles/`). Se
/// reparten por hash, así una pradera tiene mariposas de varios colores.
const BUTTERFLY_SLOTS: [&str; 5] = [
    "particles/mariposa",
    "particles/mariposa_amarilla",
    "particles/mariposa_azul",
    "particles/mariposa_roja",
    "particles/mariposa_verde",
];

#[derive(Component)]
struct Butterfly {
    /// Posición de MUNDO (igual que la luciérnaga: el paralaje es el efecto).
    x: f32,
    z: f32,
    vx: f32,
    vz: f32,
    /// Fase del aleteo/serpenteo y su velocidad.
    ph: f32,
    pulse: f32,
    seed: u32,
}

#[derive(Resource, Default)]
struct ButterfliesReady(bool);

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
                    // Valores de arranque; `update_fireflies` los pisa cada frame
                    // con los del cuerpo donde estés (ver `body_motes`).
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
    weather: Res<Weather>,
    tiles: Res<TileWorld>,
    region: Res<RegionRuntime>,
    world_ctx: Res<crate::scene::WorldContextRuntime>,
    planet: Res<crate::world2d::ActivePlanet>,
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
    let body = crate::world2d::active_surface_body(&world_ctx, &planet);
    let spec = body_motes(body);
    // En un cuerpo SIN atmósfera no hay ciclo día/noche que valga (el cielo es
    // negro igual): el polvo está siempre. En la Tierra sale con el atardecer.
    let airless = ha_shared::transition_profile(body).sky_fade_end <= 0.0;
    // Con lluvia los bichos se guardan. Es la condición más barata que existe y
    // la que más se nota: un campo lleno de luciérnagas bajo una tormenta rompe
    // todo el clima que acaba de armar `weather`.
    let on = surface_region_active(&region).is_some()
        && spec.count > 0
        && (airless || day < DUSK_ON)
        && !weather.wet();
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
    // Sube desde el atardecer y llega a pleno de noche. En un cuerpo sin aire no
    // hay atardecer: están a pleno siempre.
    let amt = if airless { 1.0 } else { ((DUSK_ON - day) / DUSK_ON).clamp(0.0, 1.0) };
    // Un bicho no es una hoja: el viento lo empuja, no se lo lleva.
    let wind = weather.wind_ms() * 0.35;
    let halo = spec.halo.to_srgba();
    let core_rgb = spec.core.to_srgba();
    for (i, (mut f, mut tf, mut sprite, mut vis, kids, mut light)) in flies.iter_mut().enumerate() {
        // Las de más allá del cupo de ESTE cuerpo se guardan (las entidades se
        // reciclan entre planetas: nunca se despawnean, sólo se apagan).
        if i >= spec.count {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            light.intensity = 0.0;
            continue;
        }
        // Deriva propia en coordenadas de MUNDO (no del jugador). `drift` viene
        // del cuerpo: el polvo marciano corre con el viento, el lunar flota; y
        // encima va el viento del CLIMA, que es lo que hace que en una tormenta
        // el polvo se vaya de lado en vez de pasear.
        let sp = spec.drift / 0.55;
        let (nx, nz) = (
            f.x + (f.vx * sp + wind) * dt,
            f.z + f.vz * sp * dt,
        );
        // Contra una pared rebota (y de paso deja de empujar hacia adentro): eso
        // es lo que hace que la luciérnaga bordee tu casa en vez de cruzarla.
        if blocked(&tiles, body, nx, f.z) {
            f.vx = -f.vx;
        } else {
            f.x = nx;
        }
        if blocked(&tiles, body, f.x, nz) {
            f.vz = -f.vz;
        } else {
            f.z = nz;
        }
        // Se alejó demasiado (caminaste, volaste): renace en otra parte del
        // anillo — nunca "salta" dentro de cuadro delante tuyo.
        let d2 = (f.x - px) * (f.x - px) + (f.z - pz) * (f.z - pz);
        // Se alejó, o quedó dentro de una pared (construiste encima de ella):
        // renace en el anillo, y si el sorteo cae en otro muro se vuelve a tirar.
        if d2 > (FIELD_R * 1.5) * (FIELD_R * 1.5) || blocked(&tiles, body, f.x, f.z) {
            let t = *tick;
            for attempt in 0..4u32 {
                place(&mut f, px, pz, t.wrapping_add((i as u32) << 3).wrapping_add(attempt));
                if !blocked(&tiles, body, f.x, f.z) {
                    break;
                }
            }
        }
        f.ph += f.pulse * dt;
        let r = render_xy(f.x, f.z);
        tf.translation = Vec3::new(r.x, r.y, FLY_Z);
        // Latido: casi se apagan y vuelven — eso es lo que hace vivo a un bicho.
        // El polvo NO late (`beats: false`): una mota que parpadea se lee como
        // luciérnaga gris, no como polvo.
        let beat = if spec.beats { (0.5 + 0.5 * f.ph.sin()).powf(1.8) } else { 0.55 };
        let a = amt * (0.12 + 0.88 * beat);
        sprite.custom_size = Some(Vec2::splat(spec.size));
        sprite.color = Color::srgba(halo.red, halo.green, halo.blue, a * 0.55);
        *vis = Visibility::Inherited;
        // **Lo que dice que emite luz, emite luz.** `spec.light == 0` (polvo,
        // hielo) apaga el `Light2d` del todo en vez de dejar un charco fantasma.
        light.color = spec.emit;
        light.radius = spec.light.max(0.05);
        light.intensity = if spec.light > 0.0 { 0.15 + 0.55 * a } else { 0.0 };
        // El núcleo late con SU luciérnaga (no un promedio global): es lo que
        // hace que el campo parpadee desordenado y no como un semáforo.
        for &k in kids {
            if let Ok(mut core) = cores.get_mut(k) {
                // El núcleo va del MISMO matiz que su halo (nunca blanco fijo):
                // es lo que hace que el conjunto lea "lima" en la Tierra y
                // "ceniza al rojo" en Venus, con el mismo par de quads.
                core.custom_size = Some(Vec2::splat(spec.size * 0.24));
                core.color =
                    Color::srgba(core_rgb.red, core_rgb.green, core_rgb.blue, (a * 1.15).min(1.0));
            }
        }
    }
}

/// Coloca (o recicla) una mariposa en el anillo alrededor del jugador. Misma
/// idea que [`place`]: nunca encima tuyo, nunca fuera de cuadro, y al reciclar
/// renace en una celda nueva en vez de teletransportarse al borde (eso es lo que
/// delata el truco).
fn place_butterfly(b: &mut Butterfly, px: f32, pz: f32, tick: u32) {
    b.seed = b.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223 ^ tick);
    let ang = hash01(b.seed) * std::f32::consts::TAU;
    let rad = FIELD_R_DAY * (0.30 + 0.65 * hash01(b.seed ^ 0x9E37_79B9));
    b.x = px + ang.cos() * rad;
    b.z = pz + ang.sin() * rad;
    // Vuelan más rápido que la deriva de una luciérnaga, pero no en línea recta:
    // el serpenteo se lo pone `update_butterflies` encima de esta velocidad base.
    b.vx = (hash01(b.seed ^ 0x51ED_2701) - 0.5) * 1.9;
    b.vz = (hash01(b.seed ^ 0x1B87_3593) - 0.5) * 1.9;
    b.pulse = 1.6 + hash01(b.seed ^ 0x2545_F491) * 2.4;
    b.ph = hash01(b.seed ^ 0x7FEB_352D) * 6.28;
}

/// Spawnea la bandada una sola vez, cuando el pack de arte ya está cargado. Si
/// el slot `particles/mariposa*` no existe en el manifest no pasa nada: se
/// marca hecho y el día simplemente no tiene mariposas (cero regresión en un
/// pack viejo).
fn spawn_butterflies(
    mut commands: Commands,
    art: Option<Res<ArtLibrary>>,
    mut ready: ResMut<ButterfliesReady>,
) {
    let Some(art) = art else { return };
    if ready.0 {
        return;
    }
    let size = Vec2::splat(BUTTERFLY_SIZE);
    // Qué slots EXISTEN de verdad en el manifest. Si todavía no hay ninguno el
    // flag no se quema y se reintenta el frame siguiente (el pack se carga
    // asincrónico); si el pack simplemente no trae mariposas, el día se queda
    // sin ellas y no pasa nada más.
    let slots: Vec<&str> = BUTTERFLY_SLOTS
        .iter()
        .copied()
        .filter(|slot| art.world_sprite(slot, size).is_some())
        .collect();
    if slots.is_empty() {
        return;
    }
    ready.0 = true;
    for i in 0..N_BUTTERFLIES {
        let mut b = Butterfly {
            x: 0.0,
            z: 0.0,
            vx: 0.0,
            vz: 0.0,
            ph: 0.0,
            pulse: 2.0,
            seed: 0x85EB_CA6B ^ i as u32,
        };
        place_butterfly(&mut b, 0.0, 0.0, i as u32);
        let Some((mut sprite, anim)) = art.world_sprite(slots[i % slots.len()], size) else {
            continue;
        };
        // Arrancan invisibles: `update_butterflies` les pone el alpha del día.
        sprite.color = Color::srgba(1.0, 1.0, 1.0, 0.0);
        let mut e = commands.spawn((
            b,
            sprite,
            Transform::from_xyz(0.0, 0.0, FLY_Z_DAY),
            Visibility::Hidden,
        ));
        if let Some(anim) = anim {
            e.insert(anim);
        }
    }
}

fn update_butterflies(
    time: Res<Time>,
    clock: Res<DayClock>,
    weather: Res<Weather>,
    tiles: Res<TileWorld>,
    region: Res<RegionRuntime>,
    world_ctx: Res<crate::scene::WorldContextRuntime>,
    planet: Res<crate::world2d::ActivePlanet>,
    player: Query<&Transform, With<Player>>,
    // `Without<Player>` mantiene los dos queries de `Transform` disjuntos (panic
    // B0001 de Bevy), igual que en las luciérnagas.
    mut flies: Query<(&mut Butterfly, &mut Transform, &mut Sprite, &mut Visibility), Without<Player>>,
    mut tick: Local<u32>,
) {
    let day = clock.daylight();
    // Sólo en la Tierra: es el único cuerpo con aire respirable y flores. En los
    // demás el turno de día lo cubre el polvo de `body_motes`.
    let body = crate::world2d::active_surface_body(&world_ctx, &planet);
    let earth = body == ha_shared::CelestialBody::Earth;
    // Con lluvia (o con el cielo muy tapado) no vuela ninguna: una mariposa no
    // sale con mal tiempo, y ese vacío es parte de lo que hace que el clima
    // signifique algo.
    let on = earth
        && surface_region_active(&region).is_some()
        && day > DAY_ON
        && !weather.wet()
        && weather.cover < 0.75;
    if !on {
        for (_, _, _, mut vis) in &mut flies {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
        }
        return;
    }
    let Ok(pt) = player.get_single() else {
        return;
    };
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    let dt = time.delta_secs().min(0.1);
    *tick = tick.wrapping_add(1);
    // Entran con el día ya alto y llegan a pleno al mediodía — igual que las
    // luciérnagas salen con el atardecer, pero al revés.
    let amt = ((day - DAY_ON) / (1.0 - DAY_ON)).clamp(0.0, 1.0);
    // **Juegan entre ellas.** Una pasada previa junta las posiciones para que el
    // paso siguiente pueda mirar a las vecinas: con cohesión suave las mariposas
    // se buscan y se persiguen en pareja, y con separación no se funden en un
    // punto. Es un boids de dos reglas — 22 bichos son 484 comparaciones, nada—,
    // y es la diferencia entre "partículas a la deriva" y "bichos jugando".
    let pos: Vec<Vec2> = flies.iter().map(|(b, ..)| Vec2::new(b.x, b.z)).collect();
    let wind = weather.wind_ms() * 0.45;
    for (i, (mut b, mut tf, mut sprite, mut vis)) in flies.iter_mut().enumerate() {
        b.ph += b.pulse * dt;
        let me = Vec2::new(b.x, b.z);
        let mut social = Vec2::ZERO;
        for (j, other) in pos.iter().enumerate() {
            if j == i {
                continue;
            }
            let to = *other - me;
            let d = to.length();
            if d < 0.001 {
                continue;
            }
            if d < SEPARATE_R {
                // Muy pegadas: se apartan fuerte. Sin esto la bandada colapsa a un
                // solo punto y parece un bicho gordo, no un baile.
                social -= to / d * (SEPARATE_R - d) * 2.4;
            } else if d < COMPANY_R {
                // A media distancia se atraen flojo: eso es la "persecución".
                social += to / d * 0.55;
            }
        }
        // Serpenteo propio: la velocidad base gira despacio con la fase, así
        // ninguna vuela en línea recta (que es lo que delata una partícula).
        let wob = b.ph.sin();
        let (vx, vz) = (
            b.vx + wob * 0.55 + social.x,
            b.vz + (b.ph * 0.73).cos() * 0.55 + social.y,
        );
        // Igual que las luciérnagas: la pared existe, el viento empuja.
        let (nx, nz) = (b.x + (vx + wind) * dt, b.z + vz * dt);
        if blocked(&tiles, body, nx, b.z) {
            b.vx = -b.vx;
        } else {
            b.x = nx;
        }
        if blocked(&tiles, body, b.x, nz) {
            b.vz = -b.vz;
        } else {
            b.z = nz;
        }
        let d2 = (b.x - px) * (b.x - px) + (b.z - pz) * (b.z - pz);
        if d2 > (FIELD_R_DAY * 1.5) * (FIELD_R_DAY * 1.5) || blocked(&tiles, body, b.x, b.z) {
            let t = *tick;
            for attempt in 0..4u32 {
                place_butterfly(&mut b, px, pz, t.wrapping_add((i as u32) << 3).wrapping_add(attempt));
                if !blocked(&tiles, body, b.x, b.z) {
                    break;
                }
            }
        }
        let r = render_xy(b.x, b.z);
        // Cabeceo vertical: sube y baja mientras avanza. Sin esto se leen como
        // calcomanías deslizándose por el suelo en vez de bichos volando.
        let bob = (b.ph * 1.7).sin() * 0.22;
        tf.translation = Vec3::new(r.x, r.y + bob, FLY_Z_DAY);
        // El arte mira a la derecha (`strip_dir: Right`): se espeja al ir hacia
        // la izquierda para que el aleteo acompañe al rumbo.
        sprite.flip_x = vx < 0.0;
        sprite.color = Color::srgba(1.0, 1.0, 1.0, amt);
        if *vis != Visibility::Inherited {
            *vis = Visibility::Inherited;
        }
    }
}
