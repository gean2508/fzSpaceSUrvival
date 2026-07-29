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

use crate::flight::{Flight, KI_CHARGE_PER_S, KI_DRAIN_PER_S, KI_GROUND_REGEN_PER_S, KI_MAX};
use crate::input::{MovementIntent, TouchActions};
use crate::scene::{surface_region_for, RegionId, RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, PlayerVisual, SimState};
use crate::solar_system_clock::SolarSystemClock;
use crate::transition::{mode_for, FlightTransition};
use crate::universe::{
    body_offset, climb_rate, lateral_speed, nearest_landable, space_alt, UniverseFrame, MAX_ALT,
};
use crate::world2d::ActivePlanet;

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
    /// Componente vertical que controla el jugador (m sobre la superficie). El
    /// mando; `alt` es lo que resulta de combinarlo con la distancia al cuerpo.
    pub vert: f32,
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
const TAKEOFF_HOLD: f32 = 0.4;
/// Multiplicador de bajada por defecto al soltar todo en el aire (caída suave).
const IDLE_SINK: f32 = 0.35;

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
            )
                .chain(),
        );
    }
}

/// Suelo → cargar el aura con SALTO; pasado `TAKEOFF_HOLD` (y con Ki) despegás.
fn creative_fly_takeoff(
    time: Res<Time>,
    actions: Res<TouchActions>,
    frame: Res<UniverseFrame>,
    flight: Res<Flight>,
    mut fly: ResMut<CreativeFlight>,
) {
    if fly.flying() {
        return;
    }
    fly.body = frame.ref_body;
    if !actions.jump {
        fly.charge_t = 0.0;
        fly.charging = false;
        return;
    }
    fly.charging = true;
    fly.charge_t += time.delta_secs();
    if fly.charge_t >= TAKEOFF_HOLD && flight.ki > 1.0 {
        fly.alt = 0.0;
        fly.vert = 0.0;
        fly.vy = climb_rate(0.0);
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
    mut player: Query<(&mut SimState, &mut PlayerVisual), With<Player>>,
) {
    if !fly.flying() {
        approach.lead = Vec2::ZERO;
        return;
    }
    // Un frame largo (carga de assets, cambio de app) NO debe catapultarte: la
    // altura es exponencial, así que 0.25 s de hipo valían 7 m de golpe y el
    // encuadre daba un salto visible. Se acota el paso y ya.
    let dt = time.delta_secs().min(1.0 / 20.0);
    let t = clock.t;
    let out_of_ki = flight.ki_empty();

    // ── VERTICAL: la altura abre/cierra el encuadre ─────────────────────────
    let rate = climb_rate(fly.alt);
    let mut vy = -rate * IDLE_SINK; // soltar todo = descenso suave
    if actions.jump && !out_of_ki {
        vy = rate;
    }
    if actions.action || out_of_ki {
        vy = -rate;
    }
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

    // Encuadre de aproximación: la cámara se adelanta hacia el cuerpo al que
    // vas, proporcional a lo lejos que estás de su superficie. Cerca (gap≈0) no
    // se adelanta nada y quedás centrado para aterrizar.
    let to_body = body_offset(near_body, frame.ref_body, t) - here;
    approach.lead = to_body.normalize_or_zero() * gap * 0.35;

    // ── LATERAL: velocidad atada a la ALTURA, no al encuadre ────────────────
    // Atarla al encuadre hacía que el pellizco del usuario cambiara la velocidad
    // de vuelo — hacer zoom no puede acelerarte. La altura sí es física.
    let lat = Vec2::new(intent.move_x, intent.move_z);
    if let Ok((mut sim, mut visual)) = player.get_single_mut() {
        if lat.length() > 0.01 && !out_of_ki {
            let mut spd = lateral_speed(fly.alt);
            if intent.sprint {
                spd *= 1.8;
            }
            let d = lat.normalize_or_zero() * spd * dt;
            sim.body.x += d.x;
            sim.body.z += d.y;
            sim.body.rot = d.y.atan2(d.x);
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
    // Ya en el espacio, si otro cuerpo aterrizable quedó más cerca, pasa a ser
    // el origen. Es un cambio de coordenadas puro: el jugador y todos los discos
    // se mueven el mismo delta, así que en pantalla no se ve NADA.
    if fly.alt >= space_alt(frame.ref_body) {
        fly.land_armed = true;
        let p = Vec2::new(fly.pos.x, -fly.pos.z);
        let (near, _) = nearest_landable(p, frame.ref_body, t);
        if near != frame.ref_body {
            let o = body_offset(near, frame.ref_body, t);
            if let Ok((mut sim, mut visual)) = player.get_single_mut() {
                sim.body.x -= o.x;
                sim.body.z += o.y; // render y = -z
                visual.from_x = sim.body.x;
                visual.to_x = sim.body.x;
                visual.from_z = sim.body.z;
                visual.to_z = sim.body.z;
                fly.pos = Vec3::new(sim.body.x, 0.0, sim.body.z);
            }
            frame.ref_body = near;
            if !matches!(near, CelestialBody::Earth | CelestialBody::Moon) {
                planet.body = Some(near);
            }
            eprintln!("ha-ki: marco rebasado a {near:?}");
        }
    }
    fly.body = frame.ref_body;

    // ── ATERRIZAJE ──────────────────────────────────────────────────────────
    // Tocás suelo cuando la altura EFECTIVA llega a cero: bajaste del todo Y
    // estás sobre el cuerpo. Estar a ras pero lejos no cuenta — ahí seguís en el
    // espacio, que es justo lo que dice la altura efectiva.
    if fly.alt <= 0.5 && vy < 0.0 && fly.land_armed_or_low() {
        fly.phase = Phase::Ground;
        fly.alt = 0.0;
        fly.vert = 0.0;
        fly.vy = 0.0;
        fly.charging = false;
        trans.clear();
        eprintln!("ha-ki: contacto en {:?}", fly.body);
        return;
    }

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
        // Volar SIEMPRE cuesta Ki (en el aire no se recarga); empujar cuesta más.
        let drain = if fly.charging { KI_DRAIN_PER_S * 1.5 } else { KI_DRAIN_PER_S };
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

/// Aura Ki detrás del personaje: destella al cargar, brillo suave volando.
fn sync_player_aura(
    time: Res<Time>,
    fly: Res<CreativeFlight>,
    flight: Res<Flight>,
    mut aura: Query<(&mut Visibility, &mut Sprite, &mut Transform), With<crate::sim::PlayerAura>>,
) {
    let Ok((mut vis, mut sprite, mut tf)) = aura.get_single_mut() else {
        return;
    };
    if !(fly.charging || fly.flying()) {
        *vis = Visibility::Hidden;
        return;
    }
    *vis = Visibility::Visible;
    let t = time.elapsed_secs();
    let (base_a, base_s, flick) = if fly.charging {
        (0.55_f32, 1.28_f32, (t * 22.0).sin() * 0.5 + 0.5)
    } else {
        (0.32_f32, 1.0_f32, (t * 7.0).sin() * 0.5 + 0.5)
    };
    let ki = flight.ki_frac().max(0.2);
    sprite.color = Color::srgba(1.0, 0.82 + 0.12 * flick, 0.25, base_a * ki * (0.8 + 0.2 * flick));
    let s = base_s * (0.85 + 0.15 * flick);
    tf.scale = Vec3::new(s, s * 1.05, 1.0);
}
