//! Flight state — the engine-wide "am I flying?" mirror + shared orbital helpers.
//!
//! There are NO ships/rockets anymore (biblia §9 anti-regresión): the CHARACTER
//! flies like a Saiyan — takeoff, free flight through the orbital view, landing —
//! all driven by `creative_fly` (the Ki-flight controller). This module owns the
//! passive `Flight` resource that controller WRITES each frame so the rest of the
//! engine (walk-block, camera-adjacent telemetry, audio, machines, persistence,
//! HUD) reads ONE coherent flight state without knowing about Ki. It also keeps
//! the pure orbital-plane math (angle↔grid-x, launch/insertion points) the
//! universe and nav-computer still need.

use bevy::prelude::*;

use ha_shared::solar_ephemeris::body_center_at;
use ha_shared::{body_atmo_top, body_radius, CelestialBody, FlightPhase};

/// Outward radial direction (in the XZ plane) at a given angle from a body
/// centre. Screen-up (−z) is angle −π/2; takeoff rises straight up.
fn radial_dir(angle: f32) -> Vec3 {
    Vec3::new(angle.cos(), 0.0, angle.sin())
}

/// Default takeoff heading: straight up on screen (−z).
const LAUNCH_ANGLE: f32 = -std::f32::consts::FRAC_PI_2;

/// Altitude above a body where the surface strip hands over to the orbital view
/// (pure render/frame change — physics never notices).
pub fn escape_alt(body: CelestialBody) -> f32 {
    (body_atmo_top(body).max(90.0)) * 1.8
}

/// Orbital insertion point above Earth (nav-computer probes).
pub fn orbital_start_above_pad() -> Vec3 {
    let c = Vec3::from_array(body_center_at(CelestialBody::Earth, 0.0));
    let r = body_radius(CelestialBody::Earth) + escape_alt(CelestialBody::Earth) + 120.0;
    c + radial_dir(LAUNCH_ANGLE) * r
}

// ---------------------------------------------------------------------------
// Ki (replaces rocket fuel) — a soft cost so charging up is worth it.
// ---------------------------------------------------------------------------

/// Full Ki reserve. Cosmetic/gameplay only (client-side): the survival server
/// never sees flight, so this stays in ha-game (not the deterministic sim).
pub const KI_MAX: f32 = 100.0;
/// Ki gastado por segundo de vuelo. Recalibrado a la escala grande
/// (29 jul 2026): con el sistema en millones de metros, una ida y vuelta a la
/// Luna son ~4 minutos de vuelo, y el coste viejo (1/s) la dejaba a medias —
/// el Ki se vaciaba en pleno ascenso, el vuelo caía solo y el jugador quedaba
/// rebotando entre despegue y aterrizaje sin poder salir del planeta.
///
/// A 0.30/s una carga completa da ~5.5 min de vuelo (~3.7 min empujando): la
/// ida y vuelta a la Luna entra holgada, y una expedición a Marte SÍ exige
/// parar a recargar. Eso es lo que se quería: un coste que se nota, no un muro.
pub const KI_DRAIN_PER_S: f32 = 0.30;
/// Ki restored per second while CHARGING (holding the charge/power-up) — a full
/// top-up in ~3 s.
pub const KI_CHARGE_PER_S: f32 = 34.0;
/// Passive Ki regen per second while grounded and not charging (~12 s to refill).
pub const KI_GROUND_REGEN_PER_S: f32 = 8.0;

pub struct FlightPlugin;

impl Plugin for FlightPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Flight::default());
    }
}

/// Passive, engine-wide flight state. Written every frame by `creative_fly`
/// (the Ki-flight controller); read by walk-block, audio, machines, telemetry,
/// HUD and persistence. NOT a physics body — the controller owns the motion.
#[derive(Resource)]
pub struct Flight {
    /// Airborne (any Climb/Orbit/Descend phase). Ground ⇒ false.
    pub active: bool,
    /// Coarse phase for readers that key off `Surface` vs airborne.
    pub phase: FlightPhase,
    /// World-plane position (x, y-flattened, z), coplanar with the ephemerides —
    /// valid while airborne.
    pub pos: Vec3,
    /// Velocidad lateral (m/s) en el marco del cuerpo de referencia.
    pub vel: Vec3,
    /// Velocidad vertical (m/s), + subiendo.
    pub vy: f32,
    /// Centro (x, z) del cuerpo de referencia en el frame anterior — permite
    /// recuperar la posición LOCAL previa y medir la velocidad relativa al
    /// cuerpo aunque él mismo se esté moviendo por su órbita.
    pub last_center: Vec2,
    /// Body we're flying over / toward.
    pub body: CelestialBody,
    /// Flying low over a walkable body's strip (render stays in surface frame).
    pub surface_flight: Option<CelestialBody>,
    /// Solar-clock snapshot from the last update (for frame remaps off-tick).
    pub clock_secs: f32,
    /// Ki reserve (0..=KI_MAX).
    pub ki: f32,
    /// Holding the charge/power-up (aura flares, Ki refills).
    pub charging: bool,
    /// Altura sobre la superficie del cuerpo (m) — la escribe el controlador
    /// (`creative_fly`), que es quien la conoce de verdad.
    ///
    /// NO se re-deriva de `pos`: los cuerpos viven en `FLIGHT_PLANE_Y` (centro
    /// de la Tierra en `y = -radio`, arriba = +Y) pero el vuelo aplana la
    /// posición al plano XZ con `pos.y = 0`. Medir la distancia 3D entre esas
    /// dos convenciones daba un offset fijo de `(√2−1)·radio` (~621 m estando
    /// parado en el suelo), que corrompía el altímetro del HUD y el chequeo de
    /// vacío/atmósfera.
    pub alt_m: f32,
}

impl Default for Flight {
    fn default() -> Self {
        Self {
            active: false,
            phase: FlightPhase::Surface,
            pos: Vec3::ZERO,
            vel: Vec3::ZERO,
            vy: 0.0,
            last_center: Vec2::ZERO,
            body: CelestialBody::Earth,
            surface_flight: None,
            clock_secs: 0.0,
            ki: KI_MAX,
            charging: false,
            alt_m: 0.0,
        }
    }
}

impl Flight {
    /// True while airborne — the walker sim is frozen and interaction is gated.
    pub fn blocks_walking(&self) -> bool {
        self.active && self.phase != FlightPhase::Surface
    }

    pub fn end_surface(&mut self) {
        self.active = false;
        self.phase = FlightPhase::Surface;
        self.vel = Vec3::ZERO;
        self.vy = 0.0;
        self.surface_flight = None;
        self.alt_m = 0.0;
    }

    pub fn speed(&self) -> f32 {
        self.vel.length()
    }

    pub fn altitude(&self) -> f32 {
        self.alt_m
    }

    pub fn altitude_m(&self) -> f32 {
        self.altitude()
    }

    /// Velocidad de subida/bajada (m/s), + subiendo. La escribe el controlador
    /// de vuelo: la altura es un eje propio (perpendicular a la vista cenital),
    /// no se puede sacar proyectando `vel` sobre un radio.
    pub fn vertical_speed(&self) -> f32 {
        self.vy
    }

    /// Velocidad lateral RELATIVA al cuerpo de referencia (m/s). Relativa a
    /// propósito: en absoluto arrastraría los ~565 m/s con los que la Tierra
    /// recorre su órbita, y el HUD marcaría media milla por segundo estando
    /// quieto sobre el suelo.
    pub fn horizontal_speed(&self) -> f32 {
        self.vel.length()
    }

    pub fn in_vacuum(&self) -> bool {
        self.altitude() > body_atmo_top(self.body)
    }

    /// Ki as a 0..100 percentage (HUD + vessel telemetry).
    pub fn ki_pct(&self) -> f32 {
        (self.ki / KI_MAX * 100.0).clamp(0.0, 100.0)
    }

    pub fn ki_frac(&self) -> f32 {
        (self.ki / KI_MAX).clamp(0.0, 1.0)
    }

    /// Out of Ki — flight is forced down / takeoff refused.
    pub fn ki_empty(&self) -> bool {
        self.ki <= 0.0
    }
}
