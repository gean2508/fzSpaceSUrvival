//! **Fuegos artificiales** — el cohete, el estallido, y el destello que ilumina
//! el pueblo entero durante medio segundo.
//!
//! ## Por qué existe
//!
//! Es la pieza que le faltaba a la noche. Con los faroles encendidos
//! (`scripts/import-emisiones.py`) el pueblo ya se lee de noche, pero una noche
//! con luz fija es una postal: **lo que la vuelve una escena es que algo cambie
//! la luz**. Un estallido de pirotecnia hace exactamente eso — durante un
//! instante la fuente de luz dominante deja de ser el farol y pasa a ser el
//! cielo, todas las sombras cambian de rumbo, y el pueblo se ve entero.
//!
//! ## Cómo está hecho (y por qué así)
//!
//! Tres piezas, ninguna cara:
//!
//! 1. **El cohete** — una mota que sube perdiendo velocidad, dejando un rastro
//!    de chispas cortas. Lleva su propia [`Light2d`] chica: mientras sube, se
//!    ve subir su reflejo por los techos.
//! 2. **El estallido** — N chispas con velocidad radial, gravedad y ARRASTRE. El
//!    arrastre es lo que distingue un fuego artificial de una explosión: las
//!    chispas frenan casi enseguida y después caen despacio, y ese "abrirse de
//!    golpe y quedar colgando" es la silueta que uno reconoce.
//! 3. **El destello** — una [`Light2d`] efímera, grande y sin bulbo, cuya
//!    intensidad decae con la misma curva que las chispas. Es lo que ilumina el
//!    pueblo, y es UNA luz por estallido: el sistema de luz ya sabe repartir su
//!    presupuesto por importancia (`light2d::light_priority`), y un destello de
//!    radio 26 le gana a cualquier farol, que es justo lo que uno quiere.
//!
//! Las chispas son sprites del **mismo degradé radial** que usa todo el sistema
//! de luz (`GlowTexture`): sobre una noche oscura, un sprite muy brillante con
//! borde difuso ya se lee como bloom, sin pase de bloom. La misma decisión que
//! toma la llama de `torches.rs`.
//!
//! ## El presupuesto (regla 2: manda el Redmi)
//!
//! Todo está acotado por arriba y nada corre de día:
//!
//! - como máximo [`MAX_ESTALLIDOS`] estallidos vivos a la vez,
//! - como máximo [`CHISPAS`] chispas por estallido,
//! - sólo de noche, sólo en superficie, y sólo si el jugador está **dentro del
//!   casco** de un poblado (`settlement_covering` — la regla de "cerca de gente"
//!   del proyecto): en el medio del campo no hay quién los tire.
//!
//! O sea: el peor caso son ~200 sprites de vida corta, que es menos de lo que
//! cuesta una hilera de árboles, y el caso normal —de día, o caminando por el
//! bosque— es **cero entidades y un `if` por frame**.

use bevy::prelude::*;

use crate::light2d::{Ambiente, GlowTexture, Light2d};
use crate::scene::{RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, UnlitSprite};
use crate::world2d::{active_surface_body, surface_region_active, ActivePlanet};
use ha_shared::topdown::TILE_M;

/// Estallidos vivos a la vez. Tres se solapan lindo (las capturas de referencia
/// tienen dos o tres en el aire); con más, el cielo se lee como ruido y el
/// destello deja de significar algo porque nunca se apaga.
const MAX_ESTALLIDOS: usize = 3;

/// Chispas por estallido. Medido contra el costo: 44 sprites de vida corta es
/// del orden de un chunk de decoración, y por debajo de ~30 la corona se ve
/// pinchada en vez de redonda.
const CHISPAS: usize = 44;

/// Segundos entre lanzamientos (mínimo y máximo). Irregular a propósito: un
/// intervalo fijo suena a metrónomo y se nota enseguida.
const ESPERA: (f32, f32) = (2.6, 7.5);

/// `HA_PIROTECNIA=1` acorta la espera para que un driver de QA vea estallidos en
/// los pocos segundos que dura su corrida. Sin esto, `HA_NIGHTTEST` saca sus tres
/// capturas y el cohete puede no haber salido todavía — o sea que el sistema se
/// verificaría por el log y nunca por la imagen, que es justo lo que no queremos
/// (ver la nota "mirar la captura del driver" de biblia §11).
fn espera_de_prueba() -> Option<(f32, f32)> {
    crate::flags::var("HA_PIROTECNIA").ok().filter(|v| v != "0").map(|_| (0.5, 1.4))
}

/// A qué distancia del jugador se lanzan, en metros. Lejos suficiente para que
/// el estallido entre en cuadro entero, cerca suficiente para que su luz llegue.
const DISTANCIA: (f32, f32) = (14.0, 34.0);

/// Altura del estallido sobre el suelo, en metros de pantalla. El juego es
/// cenital: "alto" es simplemente más arriba en Y, y eso alcanza — un cohete que
/// sube por la pantalla se lee como un cohete que sube.
const ALTURA: (f32, f32) = (11.0, 19.0);

/// Oscuridad mínima para que alguien tire pirotecnia. Un poco más alta que el
/// umbral con que se encienden los faroles (`light2d::DARK_ON`): los faroles se
/// prenden al atardecer, los fuegos se tiran de noche.
const NOCHE_MINIMA: f32 = 0.45;

/// Z de las chispas: por encima del velo de noche (~140, hijo de la cámara) y a
/// la altura de las luciérnagas, que es "en el aire, delante de todo".
const CHISPA_Z: f32 = 900.0;

/// Velocidad de subida del cohete (m/s), **mínima y máxima**.
///
/// Estuvo mal escrito una vez y el driver lo cazó: la tupla era
/// `(velocidad, frenado)` y el sorteo la leía como `(min, max)`, o sea que
/// sorteaba velocidades entre 3,4 y 13 m/s. Con las lentas, la mecha calculada
/// (`altura / velocidad`) daba casi cuatro segundos y chocaba contra su tope, y
/// el cohete estallaba mucho después de haber dejado de subir — colgado en el
/// aire. `HA_NIGHTTEST` lo mostró como `mecha 2.40s` en todos los lanzamientos,
/// que es exactamente el valor del tope.
const SUBIDA: (f32, f32) = (11.0, 16.0);

/// Cuánto frena el cohete por segundo (m/s²). Al llegar arriba está casi quieto,
/// que es cuando el ojo espera el estallido.
const FRENO: f32 = 3.4;

/// Gravedad de las chispas (m/s²). Bastante menos que la real: la ceniza
/// encendida es liviana y cuelga, y eso es lo que se ve en el cielo.
const GRAVEDAD: f32 = 3.1;

/// Cuánto frena una chispa por segundo (fracción que conserva). Es LA constante
/// de la silueta: sin arrastre la corona sigue creciendo y parece una onda
/// expansiva; con arrastre se abre de golpe y queda colgando.
const ARRASTRE: f32 = 0.16;

/// Vida de una chispa, en segundos (mínimo y máximo dentro del mismo estallido).
/// Que no mueran todas juntas es la mitad de que el estallido se "deshilache"
/// en vez de apagarse de un corte.
const VIDA_CHISPA: (f32, f32) = (1.15, 2.05);

/// Radio del destello, en metros. Grande: la gracia es que ilumine el pueblo,
/// no el pedazo de cielo donde estalló.
const DESTELLO_R: f32 = 26.0;

/// Las paletas de un estallido: el pack no trae pirotecnia dibujada, así que el
/// color se declara. Salen de las capturas de referencia — oro, verde, rojo,
/// blanco frío y magenta — y cada estallido usa UNA, que es lo que hace que se
/// lea como una carcasa y no como confeti.
const PALETA: [[f32; 3]; 5] = [
    [1.00, 0.84, 0.42], // oro
    [0.55, 1.00, 0.58], // verde
    [1.00, 0.46, 0.34], // rojo
    [0.82, 0.93, 1.00], // blanco frío
    [1.00, 0.58, 0.92], // magenta
];

/// El cohete que sube. Al agotarse la mecha, estalla.
#[derive(Component)]
struct Cohete {
    vel: f32,
    /// Cuánto le falta para estallar.
    mecha: Timer,
    color: Color,
    /// Semilla propia: decide la paleta, el rumbo de cada chispa y su vida. Que
    /// viva en el cohete es lo que hace que el estallido sea reproducible a
    /// partir de él, sin ningún RNG global compartido.
    semilla: u32,
}

/// Una chispa del estallido (o del rastro del cohete).
#[derive(Component)]
struct Chispa {
    vel: Vec2,
    vida: Timer,
    /// Tamaño inicial en metros: la chispa se encoge mientras se apaga.
    tam: f32,
}

/// El destello del estallido: una luz que decae. Sin sprite — el brillo del
/// cielo lo ponen las chispas; esto es lo que ilumina el SUELO.
#[derive(Component)]
struct Destello {
    vida: Timer,
    base: f32,
}

/// Estado del lanzador. Un solo recurso, un solo temporizador.
#[derive(Resource)]
struct Pirotecnia {
    espera: f32,
    semilla: u32,
}

impl Default for Pirotecnia {
    fn default() -> Self {
        Self { espera: ESPERA.0, semilla: 0x9E37_79B9 }
    }
}

impl Pirotecnia {
    /// XorShift32: barato, determinista y sin dependencias. El mismo generador
    /// que ya usan las estrellas de `sky2d` y las luciérnagas de `ambient`.
    fn sortear(&mut self) -> u32 {
        self.semilla ^= self.semilla << 13;
        self.semilla ^= self.semilla >> 17;
        self.semilla ^= self.semilla << 5;
        self.semilla
    }

    fn rango(&mut self, a: f32, b: f32) -> f32 {
        a + (self.sortear() & 0xffff) as f32 / 65535.0 * (b - a)
    }
}

pub struct FireworksPlugin;

impl Plugin for FireworksPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Pirotecnia>().add_systems(
            Update,
            (lanzar, subir_cohetes, mover_chispas, apagar_destellos)
                .chain()
                .run_if(crate::game_phase::world_active),
        );
    }
}

/// Ruido determinista a partir de dos enteros. Se usa para repartir las chispas
/// de un estallido sin tocar el estado del lanzador.
fn hash(a: u32, b: u32) -> u32 {
    let mut h = a.wrapping_mul(0x85eb_ca6b) ^ b.wrapping_mul(0xc2b2_ae35);
    h ^= h >> 13;
    h = h.wrapping_mul(0x27d4_eb2f);
    h ^ (h >> 15)
}

fn frac(h: u32) -> f32 {
    (h & 0x00ff_ffff) as f32 / 16_777_215.0
}

/// ¿Hay alguien acá que tire fuegos artificiales?
///
/// Dentro del casco de un poblado y de noche. **Dentro del casco**, no "en la
/// misma celda de sitio": la celda mide 2.600 tiles y eso incluye el campo
/// vacío de al lado — la regla de "cerca de gente" del proyecto.
fn hay_fiesta(pos: Vec2, ctx: &WorldContextRuntime, planet: &ActivePlanet) -> bool {
    let body = active_surface_body(ctx, planet);
    let tx = (pos.x / TILE_M).floor() as i64;
    let tz = (-pos.y / TILE_M).floor() as i64;
    ha_shared::geography::settlement_covering(body, tx, tz).is_some()
}

fn lanzar(
    time: Res<Time>,
    ambiente: Ambiente,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    glow: Option<Res<GlowTexture>>,
    mut pyro: ResMut<Pirotecnia>,
    mut commands: Commands,
    player: Query<&Transform, With<Player>>,
    vivos: Query<(), Or<(With<Cohete>, With<Destello>)>>,
) {
    let Some(glow) = glow else { return };
    pyro.espera -= time.delta_secs();
    if pyro.espera > 0.0 {
        return;
    }
    // Se reprograma SIEMPRE, aunque no se lance: si sólo se reprogramara al
    // lanzar, al entrar al pueblo saldría un cohete en el primer frame (el
    // temporizador vendría en deuda desde el atardecer) y después el ritmo
    // quedaría enganchado al momento exacto en que cruzaste el borde.
    let (min, max) = espera_de_prueba().unwrap_or(ESPERA);
    pyro.espera = pyro.rango(min, max);

    if surface_region_active(&region).is_none() || ambiente.darkness() < NOCHE_MINIMA {
        return;
    }
    if vivos.iter().count() >= MAX_ESTALLIDOS {
        return;
    }
    let Ok(p) = player.get_single() else { return };
    let base = p.translation.truncate();
    if !hay_fiesta(base, &ctx, &planet) {
        return;
    }

    let ang = pyro.rango(0.0, std::f32::consts::TAU);
    let dist = pyro.rango(DISTANCIA.0, DISTANCIA.1);
    let desde = base + Vec2::new(ang.cos(), ang.sin()) * dist;
    let alto = pyro.rango(ALTURA.0, ALTURA.1);
    let vel = pyro.rango(SUBIDA.0, SUBIDA.1);
    let semilla = pyro.sortear();
    let [r, g, b] = PALETA[(semilla >> 8) as usize % PALETA.len()];
    let color = Color::srgb(r, g, b);
    // La mecha sale de la cinemática, no de un número suelto: es el tiempo que
    // la mota tarda en llegar a la altura pedida frenándose. Así el cohete
    // estalla DONDE se ve que dejó de subir, y no antes ni después.
    let mecha = (alto / vel.max(0.1)).clamp(0.45, 2.4);

    commands.spawn((
        Cohete { vel, mecha: Timer::from_seconds(mecha, TimerMode::Once), color, semilla },
        UnlitSprite,
        Sprite {
            image: glow.0.clone(),
            custom_size: Some(Vec2::splat(0.55)),
            color: color.with_alpha(0.95),
            ..default()
        },
        Transform::from_translation(desde.extend(CHISPA_Z)),
        // La luz del cohete mientras sube: chica, pero suficiente para que se
        // vea correr un reflejo por los techos. Sin ella el cohete es un sprite
        // pegado sobre la escena, no algo que está EN la escena.
        Light2d {
            color,
            radius: 3.2,
            intensity: 0.75,
            offset: Vec2::ZERO,
            flicker: 0.35,
            day: false,
            draw_glow: true,
        },
        Name::new("CoheteDeArtificio"),
    ));
    // Una línea por cohete, y no es ruido: el lanzador tiene cuatro guardas
    // (noche, superficie, casco, presupuesto) y, si alguna falla, desde afuera
    // se ve exactamente igual que si el sistema no existiera. Con esto, el
    // driver de QA (`HA_NIGHTTEST`) deja constancia de que sí se tiraron.
    eprintln!(
        "ha-pirotecnia: cohete en ({:.0},{:.0}) a {:.0} m del jugador, mecha {mecha:.2}s",
        desde.x, desde.y, dist
    );
}

fn subir_cohetes(
    time: Res<Time>,
    glow: Option<Res<GlowTexture>>,
    mut commands: Commands,
    mut cohetes: Query<(Entity, &mut Transform, &mut Cohete)>,
) {
    let Some(glow) = glow else { return };
    let dt = time.delta_secs();
    for (e, mut tf, mut c) in &mut cohetes {
        c.mecha.tick(time.delta());
        // Sube frenándose: al llegar arriba está casi quieto, que es cuando el
        // ojo espera el estallido.
        c.vel = (c.vel - FRENO * dt).max(0.0);
        tf.translation.y += c.vel * dt;

        // Rastro: una chispita corta cada frame mientras hay velocidad. No hace
        // falta contarlas — se apagan en 0.3 s, así que el rastro tiene siempre
        // el mismo largo sin importar los fps.
        if c.vel > 1.0 {
            let h = hash(e.index(), c.mecha.elapsed().subsec_millis());
            commands.spawn((
                Chispa {
                    vel: Vec2::new((frac(h) - 0.5) * 1.4, -c.vel * 0.18),
                    vida: Timer::from_seconds(0.3, TimerMode::Once),
                    tam: 0.3,
                },
                UnlitSprite,
                Sprite {
                    image: glow.0.clone(),
                    custom_size: Some(Vec2::splat(0.3)),
                    color: c.color.with_alpha(0.7),
                    ..default()
                },
                Transform::from_translation(tf.translation),
            ));
        }

        if !c.mecha.finished() {
            continue;
        }
        estallar(&mut commands, &glow, tf.translation, c.color, c.semilla);
        commands.entity(e).despawn_recursive();
    }
}

/// **El estallido**: la corona de chispas más el destello que ilumina el suelo.
fn estallar(commands: &mut Commands, glow: &GlowTexture, en: Vec3, color: Color, semilla: u32) {
    for i in 0..CHISPAS {
        let h = hash(semilla, i as u32);
        // Reparto angular en espiral áurea + un empujón al azar: repartir sólo
        // al azar deja huecos y grumos, y repartir sólo parejo deja una rueda de
        // carro. La mezcla es lo que se ve redondo y natural.
        let ang = i as f32 * 2.399_963_2 + frac(h) * 0.9;
        // Velocidades distintas por chispa → la corona tiene GROSOR. Con una
        // sola velocidad todas caen en un aro de un píxel de ancho.
        let rapidez = 4.5 + frac(h >> 8) * 7.5;
        let vida = VIDA_CHISPA.0 + frac(h >> 16) * (VIDA_CHISPA.1 - VIDA_CHISPA.0);
        let tam = 0.26 + frac(h >> 4) * 0.20;
        // Unas pocas salen casi blancas: son las que "queman", y son las que
        // hacen que el resto se lea como color y no como pintura plana.
        let c = if h % 7 == 0 { Color::srgb(1.0, 0.97, 0.90) } else { color };
        commands.spawn((
            Chispa {
                vel: Vec2::new(ang.cos(), ang.sin()) * rapidez,
                vida: Timer::from_seconds(vida, TimerMode::Once),
                tam,
            },
            UnlitSprite,
            Sprite {
                image: glow.0.clone(),
                custom_size: Some(Vec2::splat(tam)),
                color: c,
                ..default()
            },
            Transform::from_translation(en),
            Name::new("ChispaDeArtificio"),
        ));
    }

    // El destello. `draw_glow: false` a propósito: el brillo del cielo ya lo
    // ponen las 44 chispas, y un bulbo encima de ellas quemaría el centro a
    // blanco. Esto es sólo la luz que le llega al pueblo.
    commands.spawn((
        Destello { vida: Timer::from_seconds(0.85, TimerMode::Once), base: 1.5 },
        Transform::from_translation(en),
        Visibility::Hidden,
        Light2d {
            color,
            radius: DESTELLO_R,
            intensity: 1.5,
            offset: Vec2::ZERO,
            flicker: 0.0,
            day: false,
            draw_glow: false,
        },
        Name::new("DestelloDeArtificio"),
    ));
}

fn mover_chispas(
    time: Res<Time>,
    mut commands: Commands,
    mut chispas: Query<(Entity, &mut Transform, &mut Sprite, &mut Chispa)>,
) {
    let dt = time.delta_secs();
    for (e, mut tf, mut sprite, mut c) in &mut chispas {
        c.vida.tick(time.delta());
        if c.vida.finished() {
            commands.entity(e).despawn_recursive();
            continue;
        }
        // Arrastre exponencial: `v *= ARRASTRE^dt`. Independiente de los fps —
        // con `v *= k` por frame, la corona del Redmi (13 fps medidos en §10)
        // saldría cuatro veces más grande que la de la compu.
        c.vel *= ARRASTRE.powf(dt);
        c.vel.y -= GRAVEDAD * dt;
        tf.translation += (c.vel * dt).extend(0.0);

        let k = 1.0 - c.vida.fraction();
        // Se apaga al cuadrado (vive casi entera y se va rápido al final) y se
        // encoge: una brasa que se enfría es más chica y más tenue, las dos
        // cosas juntas. Mismo criterio que `particles::tick_particles`.
        sprite.color.set_alpha(k * k);
        sprite.custom_size = Some(Vec2::splat(c.tam * (0.45 + 0.55 * k)));
    }
}

fn apagar_destellos(
    time: Res<Time>,
    mut commands: Commands,
    mut destellos: Query<(Entity, &mut Light2d, &mut Destello)>,
) {
    for (e, mut luz, mut d) in &mut destellos {
        d.vida.tick(time.delta());
        if d.vida.finished() {
            commands.entity(e).despawn_recursive();
            continue;
        }
        // Curva del fogonazo: sube en un suspiro y baja largo. `1-t` al cubo es
        // lo que hace que se lea como un golpe de luz y no como un fundido.
        let t = d.vida.fraction();
        let subida = (t / 0.08).min(1.0);
        let caida = (1.0 - t).powi(3);
        luz.intensity = d.base * subida * caida;
    }
}
