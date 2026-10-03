//! Vuelo Ki — el PERSONAJE vuela (biblia §9: sin naves ni cohetes), y vuela en
//! un mundo CONTINUO estilo Space Flight Simulator.
//!
//! ## Qué cambió respecto de la versión anterior
//!
//! Antes había dos dimensiones (la superficie y "el espacio") y despegar
//! teletransportaba al jugador de una a la otra bajo una pantalla opaca de
//! nubes. Eso es lo que se rompía: al salir la cámara miraba a un sitio donde el
//! personaje no estaba, no se veía ningún planeta y no había forma de volver.
//!
//! Ahora **no hay transición**: el jugador conserva SIEMPRE sus coordenadas
//! (`SimState::body.x/z`), que son a la vez sus coordenadas de tile y su
//! posición en el plano del sistema solar (ver `universe.rs`). Volar es:
//!
//! - **altura** (`alt`): perpendicular a la pantalla en una vista cenital, así
//!   que abre el encuadre (`universe::view_h_for_alt`) en vez de mover el sprite.
//!   Subir = alejarse; el terreno pasa de tiles a mapa satelital a disco del
//!   planeta, todo fundido, sin cortes.
//! - **lateral**: mover x/z a una velocidad atada al encuadre, así cruzar la
//!   pantalla cuesta lo mismo rasante sobre un bosque que entre planetas.
//! - **rebase**: al acercarte a otro cuerpo cambia el origen del marco. Jugador,
//!   discos y cámara se desplazan el MISMO delta ⇒ en pantalla no pasa nada, y
//!   tus coordenadas pasan a ser las de la superficie del cuerpo nuevo.
//!
//! Consecuencia: aterrizar es bajar hasta `alt = 0` donde estés, y despegar es
//! subir. No hay estados cruzados posibles — región y contexto se DERIVAN de la
//! altura cada frame, así que el soft-lock de "quedarse pegado en el espacio" ya
//! no se puede representar.
//!
//! Controles: mantener SALTO en el suelo carga el aura (un toque sigue siendo un
//! salto normal); en el aire SALTO sube, USAR baja, el stick mueve. El Ki es el
//! costo: drena volando, se recarga cargando o en tierra; sin Ki bajás solo.

use bevy::prelude::*;
use ha_shared::{body_walk_radius, CelestialBody, FlightPhase, WorldContext};

use crate::flight::{
    Flight, KI_CHARGE_PER_S, KI_DRAIN_PER_S, KI_GLIDE_REGEN_PER_S, KI_GROUND_REGEN_PER_S, KI_MAX,
};
use crate::input::{MovementIntent, TouchActions};
use crate::scene::{surface_region_for, RegionId, RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, PlayerVisual, SimState};
use crate::solar_system_clock::SolarSystemClock;
use crate::transition::{mode_for, FlightTransition};
use crate::universe::{
    body_offset, climb_rate, cruise_speed, flight_view_h, nearest_landable, space_alt,
    UniverseFrame, MAX_ALT,
};
use crate::world2d::{ActivePlanet, TileWorld};

/// Fases del vuelo. Se DERIVAN de la altura y de la velocidad vertical — no son
/// estados que se puedan cruzar entre sí (esa era la fuente de los soft-locks).
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Phase {
    #[default]
    Ground,
    Climb,
    Orbit,
    Descend,
}

#[derive(Resource, Default)]
pub struct CreativeFlight {
    pub phase: Phase,
    /// Posición en el plano del marco (x, 0, z) — SIEMPRE igual a la del
    /// caminante. Se conserva como campo porque medio motor la lee.
    pub pos: Vec3,
    /// Altura sobre la superficie del cuerpo de referencia (m).
    pub alt: f32,
    /// Cuerpo de referencia (el que estás pisando o sobrevolando).
    pub body: CelestialBody,
    /// Sólo se puede aterrizar después de haber salido de la atmósfera: evita
    /// el rebote "despego y aterrizo en el mismo frame".
    pub land_armed: bool,
    /// Segundos que SALTO lleva pulsado en tierra — carga el despegue.
    pub charge_t: f32,
    /// El aura está encendida (cargando en tierra, o empujando hacia arriba).
    pub charging: bool,
    /// Velocidad vertical del último frame (m/s) — para derivar la fase.
    pub vy: f32,
    /// Empuje vertical actual, −1..1 (−1 = bajando a fondo, 1 = subiendo a fondo).
    /// Persigue a lo que pide el botón con una rampa (`THRUST_RAMP_S`) en vez de
    /// saltar: es lo que hace que el despegue ARRANQUE, en vez de encenderse a
    /// fondo en un frame.
    pub thrust: f32,
    /// Componente vertical que controla el jugador (m sobre la superficie). El
    /// mando; `alt` es lo que resulta de combinarlo con la distancia al cuerpo.
    pub vert: f32,
    /// Encuadre automático del vuelo en este frame (`universe::flight_view_h`):
    /// continuo al rebasar el marco. 0 = todavía no se calculó (la cámara cae a
    /// `view_h_for_alt`).
    pub view_h: f32,
}

impl CreativeFlight {
    /// Por encima del techo atmosférico del cuerpo de referencia.
    pub fn in_orbit(&self) -> bool {
        self.phase == Phase::Orbit
    }
    pub fn flying(&self) -> bool {
        self.phase != Phase::Ground
    }
}

/// Mantener SALTO este tiempo en el suelo antes de despegar. Por debajo, un
/// SALTO es un salto normal.
pub const TAKEOFF_HOLD: f32 = 0.4;
/// Bajada al quedarse SIN Ki. No es "gravedad": es el aterrizaje forzoso, y es
/// suave a propósito para que se pueda elegir dónde caer.
///
/// Soltar los botones **no** hunde: volar ANULA la gravedad (pedido explícito,
/// 29 jul 2026). Antes había un `IDLE_SINK` que te bajaba solo en cuanto dejabas
/// de empujar — en el aire se sentía como que el vuelo "se apagaba", y en el
/// espacio, donde no hay ni suelo ni gravedad que justifiquen nada, era
/// directamente un bug. Ahora: sin input, la altura se queda donde está.
const NO_KI_SINK: f32 = 0.45;
/// Cuánto tarda el empuje vertical en llegar a fondo (s). El despegue no puede ser
/// un escalón: pasar de quieto a `climb_rate` en UN frame es una discontinuidad en
/// la velocidad, y como la altura encuadra la cámara (`universe::view_h_for_alt`),
/// esa discontinuidad se ve — es la mitad del "al darle a volar la cámara se alejaba
/// de golpe" (la otra mitad era `NEAR_ZOOM_RATE`, ver ahí).
///
/// Corto a propósito: 0.35 s es una arrancada, no un carraspeo. Al soltar, el mismo
/// tiempo de bajada deja una inercia de ~9 % de la altura — se lee como que el
/// cuerpo sigue un instante y frena, que es lo que hace un Saiyan, y es despreciable
/// frente al encuadre.
const THRUST_RAMP_S: f32 = 0.35;
/// A cuántos metros del borde de un cuerpo se puede tocar suelo.
///
/// Bajar del todo cerca de la orilla te posa EN la orilla (el aterrizaje te mete
/// dentro del disco). Sin este margen, quedarse a un puñado de metros del borde no
/// era aterrizable: la altura efectiva no bajaba de ahí y el jugador se quedaba
/// suspendido sin ninguna manera de tocar el suelo — el bug reportado.
const LAND_REACH_M: f32 = 400.0;

/// **Bajo tierra no se vuela.** Con roca arriba no hay cielo al que subir: el
/// vuelo Ki te sacaba atravesando el techo del subsuelo (pedido del usuario,
/// 3 oct 2026). Se mira la capa que pisás (`TileWorld::z`): negativa = subsuelo.
pub fn se_puede_volar_en_capa(z: i32) -> bool {
    z >= 0
}

pub struct CreativeFlyPlugin;

impl Plugin for CreativeFlyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CreativeFlight>().add_systems(
            Update,
            (
                creative_fly_takeoff,
                creative_fly_step,
                sync_frame_region,
                sync_flight_mirror,
                sync_player_shadow,
                sync_player_aura,
                spawn_ki_trail,
                tick_ki_trail,
            )
                .chain(),
        );
    }
}

/// Suelo → cargar el aura con SALTO; pasado `TAKEOFF_HOLD` (y con Ki) despegás.
fn creative_fly_takeoff(
    time: Res<Time>,
    actions: Res<TouchActions>,
    trepada: Res<crate::escaleras::Trepada>,
    frame: Res<UniverseFrame>,
    flight: Res<Flight>,
    tiles: Res<TileWorld>,
    mut fly: ResMut<CreativeFlight>,
) {
    if fly.flying() {
        return;
    }
    fly.body = frame.ref_body;
    // **El botón hace lo que dice.** Cuando hay borde delante pasa a decir TREPAR
    // (`input::actions::update_jump_button_label`), y entonces trepa: no carga el
    // aura. Sin esto el mismo dedo hacía las dos cosas, y en el fondo de un pozo
    // —donde SIEMPRE hay pared delante— pulsar para salir te despegaba en vuelo Ki
    // y te ibas atravesando el subsuelo. Lo cazó el driver del pozo, que empezó a
    // recorrer 148 celdas de roca en línea recta.
    // En el subsuelo SALTO es sólo un salto: ni carga el aura (que promete un
    // despegue) ni despega.
    if !actions.jump || trepada.hacia.is_some() || !se_puede_volar_en_capa(tiles.z) {
        fly.charge_t = 0.0;
        fly.charging = false;
        return;
    }
    fly.charging = true;
    fly.charge_t += time.delta_secs();
    if fly.charge_t >= TAKEOFF_HOLD && flight.ki > 1.0 {
        fly.alt = 0.0;
        fly.vert = 0.0;
        // Se despega con el empuje en CERO y `creative_fly_step` lo sube por la
        // rampa. Arrancar con `climb_rate(0)` de un saque era un escalón de 30 m/s
        // en un frame — el tirón de cámara del despegue.
        fly.vy = 0.0;
        fly.thrust = 0.0;
        fly.phase = Phase::Climb;
        fly.land_armed = false;
        fly.charge_t = 0.0;
        eprintln!("ha-ki: despegue desde {:?}", fly.body);
    }
}

/// EL vuelo: una sola función continua para subir, cruzar el sistema y bajar.
/// No hay cambio de fase que mueva al jugador ni que cambie de marco de golpe.
#[allow(clippy::too_many_arguments)]
fn creative_fly_step(
    time: Res<Time>,
    actions: Res<TouchActions>,
    intent: Res<MovementIntent>,
    clock: Res<SolarSystemClock>,
    flight: Res<Flight>,
    mut fly: ResMut<CreativeFlight>,
    mut frame: ResMut<UniverseFrame>,
    mut trans: ResMut<FlightTransition>,
    mut planet: ResMut<ActivePlanet>,
    mut approach: ResMut<crate::camera::ApproachFraming>,
    tiles: Res<TileWorld>,
    mut player: Query<(&mut SimState, &mut PlayerVisual), With<Player>>,
) {
    if !fly.flying() {
        approach.lead = Vec2::ZERO;
        approach.dir = Vec2::ZERO;
        fly.view_h = 0.0;
        return;
    }
    // Si por cualquier camino quedaste en vuelo con el subsuelo cargado (cargar
    // partida, un driver, una escalera), se aterriza ahí mismo: bajo tierra el
    // vuelo no existe, ni un frame.
    if !se_puede_volar_en_capa(tiles.z) {
        fly.phase = Phase::Ground;
        fly.alt = 0.0;
        fly.vert = 0.0;
        fly.vy = 0.0;
        fly.thrust = 0.0;
        fly.charging = false;
        fly.charge_t = 0.0;
        approach.lead = Vec2::ZERO;
        trans.clear();
        eprintln!("ha-ki: bajo tierra no se vuela — aterrizaje en la capa {}", tiles.z);
        return;
    }
    // Un frame largo (carga de assets, cambio de app) NO debe catapultarte: la
    // altura es exponencial, así que 0.25 s de hipo valían 7 m de golpe y el
    // encuadre daba un salto visible. Se acota el paso y ya.
    //
    // **Pero el tope es 1/10 s, no 1/20** (27 sep 2026): con 1/20, cualquier
    // equipo por debajo de 20 fps volaba en CÁMARA LENTA —a 6 fps el vuelo iba al
    // 30 % de su velocidad— y salir del planeta se sentía como no poder moverse
    // (`HA_TRIPTEST` se quedaba corto al volver de la Luna). 1/10 sigue frenando
    // el hipo de un frame de 0,25 s y deja volar a tiempo real a un celu lento.
    let dt = time.delta_secs().min(1.0 / 10.0);
    let t = clock.t;
    let out_of_ki = flight.ki_empty();

    // ── VERTICAL: la altura abre/cierra el encuadre ─────────────────────────
    // FLOTAS. Sin botones no hay movimiento vertical: volar anula la gravedad, y
    // eso vale igual sobre un bosque que entre planetas. Subís con SALTO, bajás
    // con USAR, y si no tocás nada te quedás exactamente donde estás.
    let rate = climb_rate(fly.alt);
    // El BOTÓN pide un empuje; el empuje REAL lo persigue por la rampa. Sin esto la
    // velocidad vertical es un escalón, y como la altura encuadra la cámara, el
    // escalón se ve (ver `THRUST_RAMP_S`).
    let mut want = 0.0;
    if actions.jump && !out_of_ki {
        want = 1.0;
    }
    if actions.action && !out_of_ki {
        want = -1.0;
    }
    // Sin Ki sí bajás: es el aterrizaje forzoso, la única "gravedad" que queda.
    if out_of_ki {
        want = -NO_KI_SINK;
    }
    let paso = (dt / THRUST_RAMP_S).min(1.0);
    fly.thrust += (want - fly.thrust).clamp(-paso, paso);
    // Sin botón el empuje se apaga del todo: FLOTÁS. El umbral evita que quede un
    // residuo de 1e-9 empujando para siempre (y marcando "maniobrando" en el Ki).
    if want == 0.0 && fly.thrust.abs() < 1e-3 {
        fly.thrust = 0.0;
    }
    let vy = rate * fly.thrust;
    fly.charging = actions.jump && !out_of_ki;
    fly.vert = (fly.vert + vy * dt).clamp(0.0, MAX_ALT);
    fly.vy = vy;

    // ── ALTURA EFECTIVA: vertical + hueco lateral ───────────────────────────
    // La altura que ve todo el juego NO es sólo el mando vertical: es la
    // distancia a la superficie MÁS CERCANA, o sea la hipotenusa entre lo que
    // subiste y lo lejos que estás del borde del cuerpo.
    //
    // Sin esto, bajar del todo a 30 km de la Luna te dejaba "a ras de suelo"
    // sobre la nada: cámara pegada, velocidad de peatón y ninguna manera de
    // llegar al cuerpo. Con esto la aproximación se regula sola — cuanto más te
    // acercás, menor es la altura efectiva, más cierra el encuadre y más despacio
    // vas, hasta posarte. Es la curva de aterrizaje de SFS, y sale gratis.
    let here = player
        .get_single()
        .map(|(sim, _)| Vec2::new(sim.body.x, -sim.body.z))
        .unwrap_or(Vec2::new(fly.pos.x, -fly.pos.z));
    let (near_body, dist) = nearest_landable(here, frame.ref_body, t);
    let gap = (dist - body_walk_radius(near_body)).max(0.0);
    fly.alt = (fly.vert * fly.vert + gap * gap).sqrt().min(MAX_ALT);

    // Encuadre de aproximación: la cámara se adelanta HACIA DONDE VAS,
    // proporcional a lo lejos que estás de una superficie. Cerca (gap≈0) no se
    // adelanta nada y quedás centrado para aterrizar.
    //
    // Antes apuntaba siempre al cuerpo MÁS CERCANO. Yendo a la Luna, la primera
    // mitad del viaje el más cercano es la Tierra —la que dejás atrás—, así que la
    // cámara se te corría para atrás y, al cruzar la mitad, giraba de golpe 180°
    // hacia la Luna: la otra mitad del "corte" de ir de un cuerpo al otro. Ahora
    // manda el rumbo del stick (y se recuerda al soltarlo); sólo si todavía no te
    // moviste en este vuelo se mira al cuerpo más cercano.
    let lat = Vec2::new(intent.move_x, intent.move_z);
    if lat.length() > 0.01 {
        // Mundo (x, z) → render (x, −z).
        approach.dir = Vec2::new(lat.x, -lat.y).normalize_or_zero();
    }
    let toward = if approach.dir != Vec2::ZERO {
        approach.dir
    } else {
        (body_offset(near_body, frame.ref_body, t) - here).normalize_or_zero()
    };
    approach.lead = toward * gap * 0.35;

    // ── LATERAL: velocidad atada a la ALTURA, no al encuadre ────────────────
    // Atarla al encuadre hacía que el pellizco del usuario cambiara la velocidad
    // de vuelo — hacer zoom no puede acelerarte. La altura sí es física, y en el
    // vacío también el hueco hasta la superficie más cercana (`cruise_speed`): así
    // se viaja de un cuerpo a otro con el stick, sin tener que apretar SUBIR.
    if let Ok((mut sim, mut visual)) = player.get_single_mut() {
        if lat.length() > 0.01 {
            let mut spd = cruise_speed(fly.alt, gap);
            if intent.sprint {
                spd *= 1.8;
            }
            // Sin Ki te arrastrás, pero te MOVÉS. Bloquear el lateral del todo era
            // un soft-lock de exploración: quedarse sin Ki en medio del vacío (donde
            // no hay suelo al que caer ni recarga en tierra) te dejaba congelado sin
            // ninguna acción posible salvo reiniciar el juego.
            if out_of_ki {
                spd *= 0.22;
            }
            let d = lat.normalize_or_zero() * spd * dt;
            sim.body.x += d.x;
            sim.body.z += d.y;
            sim.body.rot = d.y.atan2(d.x);
        }
        // **En el vacío, BAJAR es bajar HACIA el cuerpo más cercano** (3 oct 2026).
        // La altura efectiva es la hipotenusa entre `vert` y el hueco lateral, así
        // que entre la Tierra y la Luna el hueco manda y bajar `vert` no hacía nada
        // que se viera: el botón quedaba muerto, y "abajo" seguía queriendo decir
        // "hacia la Tierra" aunque estuvieras al lado de la Luna. Ahora el mismo
        // empuje que baja `vert` cierra también el hueco, al ritmo del crucero: de
        // un toque sostenido te posás en el cuerpo más cercano, como en SFS. Sobre
        // el disco el hueco es cero y esto no hace nada. Sin Ki pasa lo mismo, más
        // lento: el aterrizaje forzoso te lleva a tierra en vez de dejarte varado.
        let down = (-fly.thrust).max(0.0);
        if down > 0.0 && gap > 0.0 {
            let to_body = body_offset(near_body, frame.ref_body, t) - here;
            let step = (cruise_speed(fly.alt, gap) * down * dt).min(gap);
            let d = to_body.normalize_or_zero() * step;
            // Render (x, y) → mundo (x, −z).
            sim.body.x += d.x;
            sim.body.z -= d.y;
            if lat.length() <= 0.01 {
                sim.body.rot = (-d.y).atan2(d.x);
            }
        }
        sim.body.vx = 0.0;
        sim.body.vz = 0.0;
        sim.body.y = 0.0;
        // El espejo de render va clavado a la posición volada (sin interpolar).
        visual.from_x = sim.body.x;
        visual.to_x = sim.body.x;
        visual.from_z = sim.body.z;
        visual.to_z = sim.body.z;
        visual.from_rot = sim.body.rot;
        visual.to_rot = sim.body.rot;
        fly.pos = Vec3::new(sim.body.x, 0.0, sim.body.z);
    }

    // ── REBASE DEL MARCO ────────────────────────────────────────────────────
    // Si otro cuerpo aterrizable quedó más cerca, pasa a ser el origen. Es un
    // cambio de coordenadas puro: el jugador y todos los discos se mueven el
    // MISMO delta, así que en pantalla no se ve NADA.
    //
    // Antes esto sólo pasaba por encima del techo atmosférico del cuerpo de
    // referencia (`alt >= space_alt(ref)`), y ahí estaba el bug de "en la Luna me
    // quedaba a 30 metros": acercarse a un cuerpo hace caer la altura EFECTIVA
    // (es la distancia a la superficie más cercana), así que en cuanto bajabas del
    // techo del cuerpo VIEJO el rebase dejaba de dispararse — te quedabas
    // sobrevolando la Luna con el marco, la región y el terreno de la Tierra, sin
    // ninguna manera de posarte. Ahora se rebasa siempre que se vuele: es
    // invisible por construcción, así que no hay motivo para condicionarlo.
    if fly.alt >= space_alt(frame.ref_body) {
        fly.land_armed = true;
    }
    if near_body != frame.ref_body {
        let o = body_offset(near_body, frame.ref_body, t);
        if let Ok((mut sim, mut visual)) = player.get_single_mut() {
            sim.body.x -= o.x;
            sim.body.z += o.y; // render y = -z
            visual.from_x = sim.body.x;
            visual.to_x = sim.body.x;
            visual.from_z = sim.body.z;
            visual.to_z = sim.body.z;
            fly.pos = Vec3::new(sim.body.x, 0.0, sim.body.z);
        }
        frame.ref_body = near_body;
        if !matches!(near_body, CelestialBody::Earth | CelestialBody::Moon) {
            planet.body = Some(near_body);
        }
        eprintln!("ha-ki: marco rebasado a {near_body:?}");
    }
    fly.body = frame.ref_body;

    // ── ATERRIZAJE ──────────────────────────────────────────────────────────
    // Tocás suelo cuando la altura EFECTIVA llega a cero: bajaste del todo Y
    // estás sobre el cuerpo. Estar a ras pero lejos no cuenta — ahí seguís en el
    // espacio, que es justo lo que dice la altura efectiva.
    // La condición es sobre `vert` (lo que BAJASTE), no sobre la altura efectiva:
    // ésta incluye el hueco lateral al cuerpo, así que exigir `alt <= 0.6` te
    // obligaba a estar ya dentro del disco caminable. Posarse **justo en el borde**
    // era imposible: a 15 m de la orilla, `vert` llegaba a 0, `alt` se quedaba
    // clavada en esos 15 m y el contacto no ocurría nunca. Eso es literalmente el
    // "me quedaba a 30 metros" que se reportó, y por eso el margen se resuelve
    // ACERCÁNDOTE (abajo) en vez de rechazando el aterrizaje.
    //
    // `vy <= 0` (no `< 0`): ahora que soltar todo deja FLOTANDO, exigir velocidad
    // de bajada estricta dejaba al jugador suspendido a 20 cm del suelo sin poder
    // tocarlo salvo manteniendo USAR — el mismo problema, pero al ras.
    if fly.vert <= 0.6 && gap <= LAND_REACH_M && vy <= 0.0 && fly.land_armed_or_low() {
        // Aterrizás en la orilla: te posás en el borde del disco, no a 15 m sobre
        // el vacío. El marco ya rebasó a este cuerpo (ver arriba), así que las
        // coordenadas del jugador son las de SU superficie y basta con meterlas
        // dentro del radio caminable.
        if gap > 0.0 {
            if let Ok((mut sim, mut visual)) = player.get_single_mut() {
                let r = body_walk_radius(near_body) * 0.995;
                let p = Vec2::new(sim.body.x, sim.body.z);
                let inside = p.normalize_or_zero() * r;
                sim.body.x = inside.x;
                sim.body.z = inside.y;
                visual.from_x = inside.x;
                visual.to_x = inside.x;
                visual.from_z = inside.y;
                visual.to_z = inside.y;
                fly.pos = Vec3::new(inside.x, 0.0, inside.y);
            }
        }
        fly.phase = Phase::Ground;
        fly.alt = 0.0;
        fly.vert = 0.0;
        fly.vy = 0.0;
        fly.thrust = 0.0;
        fly.charging = false;
        fly.view_h = 0.0;
        trans.clear();
        eprintln!("ha-ki: contacto en {:?}", fly.body);
        return;
    }

    // Encuadre continuo: se evalúa DESPUÉS del rebase, en las coordenadas que van
    // a usar la cámara y el render este frame.
    let here_now = Vec2::new(fly.pos.x, -fly.pos.z);
    fly.view_h = flight_view_h(frame.ref_body, here_now, fly.vert, t);

    fly.phase = if fly.alt >= space_alt(frame.ref_body) {
        Phase::Orbit
    } else if vy < 0.0 {
        Phase::Descend
    } else {
        Phase::Climb
    };
    trans.drive(mode_for(frame.ref_body, fly.alt, vy), frame.ref_body, fly.alt);
}

impl CreativeFlight {
    /// Aterrizar exige haber salido de la atmósfera (`land_armed`) O no haberse
    /// alejado nunca del suelo — así un vuelo rasante puede posarse cuando
    /// quiera, pero una reentrada no rebota.
    fn land_armed_or_low(&self) -> bool {
        self.land_armed || self.alt < space_alt(self.body)
    }
}

/// Región y contexto se DERIVAN de la altura y del cuerpo de referencia, cada
/// frame. Por construcción no puede haber un estado cruzado (la clase entera de
/// bugs "aterricé pero sigo en el espacio" desaparece: no hay estado que
/// sincronizar, hay una función).
fn sync_frame_region(
    fly: Res<CreativeFlight>,
    frame: Res<UniverseFrame>,
    mut region: ResMut<RegionRuntime>,
    mut ctx: ResMut<WorldContextRuntime>,
    mut planet: ResMut<ActivePlanet>,
) {
    // Las regiones interiores (cueva, nave) son ortogonales al vuelo: no las toca.
    if matches!(region.active, RegionId::Cave(_) | RegionId::ShipInterior) {
        return;
    }
    let body = frame.ref_body;
    if fly.alt >= space_alt(body) && fly.flying() {
        if region.active != RegionId::Space {
            region.active = RegionId::Space;
        }
        ctx.active = WorldContext::Space;
        return;
    }
    let (r, c) = surface_region_for(body);
    if region.active != r {
        region.active = r;
    }
    ctx.active = c;
    if !matches!(body, CelestialBody::Earth | CelestialBody::Moon) && planet.body != Some(body) {
        planet.body = Some(body);
    }
}

/// Espejo del vuelo hacia el resto del motor (bloqueo de caminar, audio,
/// máquinas, telemetría, HUD, persistencia) + economía del Ki.
pub(crate) fn sync_flight_mirror(
    time: Res<Time>,
    fly: Res<CreativeFlight>,
    clock: Res<SolarSystemClock>,
    frame: Res<UniverseFrame>,
    mut flight: ResMut<Flight>,
) {
    let dt = time.delta_secs();
    flight.clock_secs = clock.t;
    flight.charging = fly.charging;
    flight.body = frame.ref_body;
    flight.alt_m = fly.alt;

    // Posición ABSOLUTA en el sistema solar (para nav/telemetría): centro del
    // cuerpo de referencia + el desplazamiento local del jugador.
    let c = ha_shared::solar_ephemeris::body_center_at(frame.ref_body, clock.t);
    let prev_local = Vec3::new(flight.pos.x - flight.last_center.x, 0.0, flight.pos.z - flight.last_center.y);
    flight.pos = Vec3::new(c[0] + fly.pos.x, 0.0, c[2] + fly.pos.z);
    flight.last_center = Vec2::new(c[0], c[2]);

    if fly.flying() {
        flight.active = true;
        flight.phase = FlightPhase::Powered;
        // Velocidad en el marco LOCAL (relativa al cuerpo): la absoluta incluye
        // la traslación orbital del propio cuerpo, y el HUD marcaría cientos de
        // m/s estando quieto.
        flight.vel = if dt > 0.0 { (fly.pos - prev_local) / dt } else { Vec3::ZERO };
        flight.vy = fly.vy;
        // EL KI CUESTA EMPUJAR, NO EXISTIR.
        //
        // Antes drenaba siempre, y en un sistema de millones de metros eso hacía
        // que el vuelo fuera una cuenta atrás: quedarse sin Ki lejos de todo
        // cuerpo era un callejón sin salida (no se recarga en el aire, no hay
        // suelo al que caer). Ahora el coste está donde está el trabajo —
        // maniobrar— y PLANEAR recupera despacio: siempre existe un camino de
        // vuelta, aunque sea lento. Es la misma economía de un Saiyan aguantando
        // el vuelo: mantenerse cuesta poco, acelerar cuesta.
        let thrusting = fly.charging || fly.vy.abs() > 0.01;
        let maneuvering = flight.vel.length() > 0.5;
        let drain = if fly.charging {
            KI_DRAIN_PER_S * 1.5
        } else if thrusting || maneuvering {
            KI_DRAIN_PER_S
        } else {
            -KI_GLIDE_REGEN_PER_S
        };
        flight.ki = (flight.ki - drain * dt).clamp(0.0, KI_MAX);
    } else {
        flight.end_surface();
        let rate = if fly.charging { KI_CHARGE_PER_S } else { KI_GROUND_REGEN_PER_S };
        flight.ki = (flight.ki + rate * dt).min(KI_MAX);
    }
}

/// Sombra en el suelo mientras volás bajo (refuerza la altura). Se apaga cuando
/// el suelo ya no se dibuja.
fn sync_player_shadow(
    fly: Res<CreativeFlight>,
    day: Res<crate::sky2d::DayClock>,
    zoom: Res<crate::camera::CameraZoom>,
    mut shadow: Query<
        (&mut Visibility, &mut Sprite, &mut Transform),
        With<crate::sim::PlayerFlightShadow>,
    >,
) {
    let Ok((mut vis, mut sprite, mut tf)) = shadow.get_single_mut() else {
        return;
    };
    let f = crate::universe::fine_tile_alpha(zoom.live_view_h);
    if !fly.flying() || f <= 0.01 {
        *vis = Visibility::Hidden;
        return;
    }
    *vis = Visibility::Visible;
    let near = 1.0 - (fly.alt / space_alt(fly.body).max(1.0)).clamp(0.0, 1.0);
    let light = if fly.body == CelestialBody::Earth { day.daylight().max(0.15) } else { 1.0 };
    sprite.color = Color::srgba(0.0, 0.0, 0.0, 0.5 * near * light * f);
    sprite.custom_size = Some(Vec2::new(1.2, 0.5) * (0.4 + 0.6 * near));
    tf.translation.y = 0.0;
}

/// Aura de Ki: tres halos concéntricos que respiran juntos.
///
/// Cada capa tiene su tono, su alfa y su ritmo de latido. Que los ritmos NO sean
/// el mismo es la mitad del efecto: si las tres pulsan en fase el aura se lee como
/// un sprite al que le suben y bajan la opacidad; desfasadas, el borde ondula
/// contra el núcleo y parece que arde. Ver `crate::sim::PlayerAura`.
#[allow(clippy::type_complexity)]
fn sync_player_aura(
    time: Res<Time>,
    fly: Res<CreativeFlight>,
    flight: Res<Flight>,
    // Los tres `Without` cruzados no son adorno: Bevy exige que las queries
    // mutables sean DEMOSTRABLEMENTE disjuntas, y sin ellos aborta en caliente con
    // B0001 (se probó: panic al primer frame de vuelo).
    mut outer: Query<
        (&mut Visibility, &mut Sprite, &mut Transform),
        (
            With<crate::sim::PlayerAura>,
            Without<crate::sim::PlayerAuraMid>,
            Without<crate::sim::PlayerAuraCore>,
        ),
    >,
    mut mid: Query<
        (&mut Visibility, &mut Sprite, &mut Transform),
        (
            With<crate::sim::PlayerAuraMid>,
            Without<crate::sim::PlayerAura>,
            Without<crate::sim::PlayerAuraCore>,
        ),
    >,
    mut core: Query<
        (&mut Visibility, &mut Sprite, &mut Transform),
        (
            With<crate::sim::PlayerAuraCore>,
            Without<crate::sim::PlayerAura>,
            Without<crate::sim::PlayerAuraMid>,
        ),
    >,
) {
    let on = fly.charging || fly.flying();
    let t = time.elapsed_secs();
    // Cargando el aura estalla; volando queda un fuego sostenido.
    let (gain, grow, beat_hz) = if fly.charging {
        (1.0_f32, 1.24_f32, 19.0_f32)
    } else {
        (0.58, 1.0, 6.5)
    };
    let ki = flight.ki_frac().max(0.18);

    // (color, alfa base, escala relativa, desfase del latido)
    // Naranja profundo afuera → dorado casi blanco en el núcleo. El alfa de la capa
    // ancha es MUY bajo a propósito: es un sprite normal (pinta encima), y subirlo
    // lava al personaje en vez de rodearlo.
    let layers: [(Color, f32, f32, f32); 3] = [
        (Color::srgb(1.0, 0.42, 0.06), 0.16, 1.0, 0.0),
        (Color::srgb(1.0, 0.66, 0.14), 0.28, 1.0, 1.9),
        (Color::srgb(1.0, 0.92, 0.62), 0.50, 1.0, 3.7),
    ];

    let paint = |vis: &mut Visibility, sprite: &mut Sprite, tf: &mut Transform, i: usize| {
        if !on {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            return;
        }
        if *vis != Visibility::Visible {
            *vis = Visibility::Visible;
        }
        let (color, base_a, rel, phase) = layers[i];
        // Latido propio: dos senos incoherentes (como las llamas de `light2d`), no
        // un seno único, para que no se sienta un metrónomo.
        let w = ((t * beat_hz + phase).sin() * 0.62
            + (t * beat_hz * 0.43 + phase * 1.7).sin() * 0.38)
            * 0.5
            + 0.5;
        let c = color.to_srgba();
        sprite.color = Color::srgba(c.red, c.green, c.blue, base_a * gain * ki * (0.72 + 0.28 * w));
        // La capa ancha respira MÁS que el núcleo: el aura se infla y se deshincha
        // por fuera mientras el punto caliente se queda quieto, que es lo que la
        // hace parecer una llama y no un sprite escalándose.
        let breathe = 1.0 + (0.16 - 0.04 * i as f32) * (w - 0.5) * 2.0;
        let s = rel * grow * breathe;
        tf.scale = Vec3::new(s, s * 1.06, 1.0);
    };

    if let Ok((mut v, mut s, mut tf)) = outer.get_single_mut() {
        paint(&mut v, &mut s, &mut tf, 0);
    }
    if let Ok((mut v, mut s, mut tf)) = mid.get_single_mut() {
        paint(&mut v, &mut s, &mut tf, 1);
    }
    if let Ok((mut v, mut s, mut tf)) = core.get_single_mut() {
        paint(&mut v, &mut s, &mut tf, 2);
    }
}

// ---------------------------------------------------------------------------
// Estela de Ki — la "cola de cometa" del jugador
// ---------------------------------------------------------------------------

/// Cuánto vive un segmento de estela. Corto: la cola tiene que desvanecerse
/// DESDE vos hasta la punta, no quedarse pintada en el aire.
const TRAIL_LIFE: f32 = 0.62;
/// Cada cuánto (en fracción del encuadre) se suelta un segmento. Al estar atado al
/// encuadre y no a los metros, la cola mide lo mismo EN PANTALLA rasando un bosque
/// a 45 m/s que cruzando el sistema a 30 km/s — sin esto, en el espacio los
/// segmentos caerían a kilómetros uno de otro y no habría cola.
const TRAIL_STEP_VIEW_FRAC: f32 = 0.020;
/// Lado del segmento al nacer, también en fracción del encuadre.
const TRAIL_SIZE_VIEW_FRAC: f32 = 0.055;
/// Z de la estela: por encima del velo de noche (~140) y del suelo, por debajo de
/// los halos de luz (700). Volando estás sobre todo lo demás, así que corresponde.
const TRAIL_Z: f32 = 260.0;

/// Suelta segmentos de estela detrás del jugador mientras vuela.
///
/// Por DISTANCIA recorrida, no por tiempo: así la cola es densa cuando te movés
/// rápido y no se acumula un montón de quads encima cuando estás quieto.
fn spawn_ki_trail(
    mut commands: Commands,
    fly: Res<CreativeFlight>,
    flight: Res<Flight>,
    zoom: Res<crate::camera::CameraZoom>,
    glow: Option<Res<crate::light2d::GlowTexture>>,
    player: Query<&Transform, With<Player>>,
    mut last: Local<Option<Vec2>>,
) {
    let Some(glow) = glow else { return };
    if !fly.flying() {
        *last = None;
        return;
    }
    let Ok(tf) = player.get_single() else { return };
    let p = tf.translation.truncate();
    let view_h = zoom.live_view_h.max(1.0);
    let step = view_h * TRAIL_STEP_VIEW_FRAC;
    if let Some(prev) = *last {
        if prev.distance(p) < step {
            return;
        }
    }
    *last = Some(p);
    let size = view_h * TRAIL_SIZE_VIEW_FRAC;
    let ki = flight.ki_frac().max(0.15);
    commands.spawn((
        crate::sim::KiTrail { life: TRAIL_LIFE, max_life: TRAIL_LIFE, size },
        crate::sim::UnlitSprite,
        Sprite {
            image: glow.0.clone(),
            custom_size: Some(Vec2::splat(size)),
            // Dorado al nacer; `tick_ki_trail` lo lleva a naranja al enfriarse.
            color: Color::srgba(1.0, 0.86, 0.42, 0.42 * ki),
            ..default()
        },
        Transform::from_xyz(p.x, p.y, TRAIL_Z),
        Name::new("KiTrail"),
    ));
}

/// Enfría la estela: cada segmento se encoge, pasa de dorado a naranja y se apaga.
/// El degradado a lo largo de la cola sale solo de que los segmentos de atrás son
/// más viejos — no hace falta coordinar nada entre ellos.
fn tick_ki_trail(
    time: Res<Time>,
    mut commands: Commands,
    mut trail: Query<(Entity, &mut crate::sim::KiTrail, &mut Sprite)>,
) {
    let dt = time.delta_secs();
    for (e, mut seg, mut sprite) in &mut trail {
        seg.life -= dt;
        if seg.life <= 0.0 {
            commands.entity(e).despawn_recursive();
            continue;
        }
        let k = (seg.life / seg.max_life).clamp(0.0, 1.0);
        // Alfa al cuadrado: el tramo pegado al jugador se ve bien y la punta se
        // desvanece rápido, que es la forma de cola de cometa (y no una salchicha
        // de opacidad uniforme cortada de golpe al final).
        sprite.color = Color::srgba(1.0, 0.52 + 0.34 * k, 0.10 + 0.32 * k, 0.42 * k * k);
        sprite.custom_size = Some(Vec2::splat(seg.size * (0.35 + 0.65 * k)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bajo_tierra_no_se_vuela() {
        assert!(se_puede_volar_en_capa(0), "en la superficie se vuela");
        assert!(se_puede_volar_en_capa(1), "desde un piso alto también");
        for z in [-1, -2, -8] {
            assert!(!se_puede_volar_en_capa(z), "en el subsuelo (capa {z}) no");
        }
    }
}
