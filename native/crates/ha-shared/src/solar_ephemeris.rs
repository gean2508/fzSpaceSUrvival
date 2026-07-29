//! Arcade heliocentric ephemeris — one shared universe; bodies orbit Sun, moons orbit planets.

use crate::celestial_bodies::{body_def, CelestialBody, FLIGHT_PLANE_Y};

const TAU: f32 = std::f32::consts::TAU;

/// Sun anchor — all planet distances measured from here at t=0.
pub fn sun_position() -> [f32; 3] {
    [-4_000_000.0, FLIGHT_PLANE_Y, 0.0]
}

fn helio_radius_from_catalog(body: CelestialBody) -> f32 {
    let sun = sun_position();
    let c = body_def(body).center;
    let dx = c[0] - sun[0];
    let dz = c[2] - sun[2];
    (dx * dx + dz * dz).sqrt().max(1.0)
}

fn helio_base_angle(body: CelestialBody) -> f32 {
    let sun = sun_position();
    let c = body_def(body).center;
    (c[2] - sun[2]).atan2(c[0] - sun[0])
}

fn helio_orbit_period(body: CelestialBody) -> f32 {
    let r = helio_radius_from_catalog(body);
    // Más lejos = año más lento. Calibrado a la escala grande (29 jul 2026):
    // Tierra ~50 min, Marte ~62 min, Neptuno ~1 h 45. Se ven VIVAS en una
    // sesión, y —clave— la velocidad tangencial resultante (~4 km/s en la
    // Tierra) queda por DEBAJO del techo de crucero del jugador (30 km/s), así
    // que un planeta destino nunca se te escapa. Con la fórmula vieja aplicada a
    // esta escala las órbitas iban a 10 km/s y viajar era imposible.
    1_800.0 + r * 0.0003 + (body_def(body).terrain_seed % 600) as f32
}

/// World position (flight metres) at arcade time `t_secs`.
pub fn body_center_at(body: CelestialBody, t_secs: f32) -> [f32; 3] {
    match body {
        CelestialBody::Sun => sun_position(),
        CelestialBody::Earth => earth_center_at(t_secs),
        CelestialBody::Moon => moon_center_at(t_secs),
        CelestialBody::Phobos => moon_of(CelestialBody::Mars, 22_000.0, 1_200.0, 0.0, t_secs),
        CelestialBody::Deimos => moon_of(CelestialBody::Mars, 28_000.0, 1_600.0, 1.7, t_secs),
        _ => heliocentric(body, t_secs),
    }
}

/// La Tierra ORBITA el Sol como cualquier otro planeta (pedido del usuario
/// 2026-07-26: "haz que la Tierra también orbite al Sol, no que se quede quieta").
/// En `t=0` `heliocentric` devuelve EXACTAMENTE el centro de catálogo (el ángulo y
/// el radio se derivan de ese centro), así que el spawn/despegue sigue arrancando
/// en la posición de siempre; a partir de ahí la Tierra revoluciona. El hand-off
/// superficie→órbita ya usa `body_center_at(body, t)` en vivo y ocurre bajo nubes
/// opacas, igual que en Marte (que ya orbitaba), así que el despegue sigue siendo
/// continuo. La Luna orbita este centro móvil (la sigue por el sistema).
pub fn earth_center_at(t_secs: f32) -> [f32; 3] {
    heliocentric(CelestialBody::Earth, t_secs)
}

pub fn moon_center_at(t_secs: f32) -> [f32; 3] {
    let earth = earth_center_at(t_secs);
    let r = 416_000.0_f32;
    // 1 h por vuelta: sigue leyéndose como una luna en movimiento, pero su
    // velocidad tangencial (~730 m/s) es una fracción de la del jugador, así que
    // se puede interceptar y aterrizar sin perseguirla.
    let period = 3_600.0;
    let a = t_secs / period * TAU;
    [
        earth[0] + a.cos() * r,
        earth[1],
        earth[2] + a.sin() * r,
    ]
}

fn heliocentric(body: CelestialBody, t_secs: f32) -> [f32; 3] {
    let sun = sun_position();
    let r = helio_radius_from_catalog(body);
    let period = helio_orbit_period(body);
    let a = helio_base_angle(body) + t_secs / period * TAU;
    [sun[0] + a.cos() * r, FLIGHT_PLANE_Y, sun[2] + a.sin() * r]
}

fn moon_of(
    parent: CelestialBody,
    orbit_r: f32,
    period: f32,
    phase: f32,
    t_secs: f32,
) -> [f32; 3] {
    let p = body_center_at(parent, t_secs);
    let a = phase + t_secs / period * TAU;
    [p[0] + a.cos() * orbit_r, p[1], p[2] + a.sin() * orbit_r]
}

/// Centro del cuerpo para direcciones de cielo (Sol/Luna vistos desde una
/// superficie). CONTINUIDAD: con el marco co-localizado el centro caminable ==
/// centro orbital, así que es directamente `body_center_at` (antes restaba el
/// radio → para la Tierra daba [0,-3000,0], desviando la dirección del SkyMoon).
pub fn gameplay_shell_center_at(body: CelestialBody, t_secs: f32) -> [f32; 3] {
    body_center_at(body, t_secs)
}

/// Aim point for a cruise toward `target`: where the body will BE when you
/// arrive, not where it is when you leave. With compressed orbital periods
/// (~30 min) and cruises up to ~180 s the target moves tens of degrees in
/// transit — aiming at the departure-time position misses outright.
///
/// Shared lead rule for manual transit burns AND the (future) autopilot —
/// do not duplicate this computation elsewhere. Two fixed-point passes are
/// plenty at arcade speeds (eta error < 1% after the second).
pub fn body_intercept_point(
    target: CelestialBody,
    t_secs: f32,
    from: [f32; 3],
    cruise_speed: f32,
) -> [f32; 3] {
    let speed = cruise_speed.max(1.0);
    let mut aim = body_center_at(target, t_secs);
    for _ in 0..2 {
        let dx = aim[0] - from[0];
        let dy = aim[1] - from[1];
        let dz = aim[2] - from[2];
        let eta = (dx * dx + dy * dy + dz * dz).sqrt() / speed;
        aim = body_center_at(target, t_secs + eta);
    }
    aim
}

/// Cruise duration for a transit over `distance_m` — non-linear ramp with a
/// hard ceiling: Moon ≈ 28 s, Mars ≈ 62 s, Neptune ≈ 180 s.
/// TODO(device): validate the pacing feels right playing on the Redmi.
pub fn transit_cruise_secs(distance_m: f32) -> f32 {
    use crate::ship_mass::{
        TRANSIT_CRUISE_EXPONENT, TRANSIT_CRUISE_MAX_SECS, TRANSIT_CRUISE_MIN_SECS,
        TRANSIT_FAR_REF_M,
    };
    let norm = (distance_m / TRANSIT_FAR_REF_M).max(0.0);
    (TRANSIT_CRUISE_MAX_SECS * norm.powf(TRANSIT_CRUISE_EXPONENT))
        .clamp(TRANSIT_CRUISE_MIN_SECS, TRANSIT_CRUISE_MAX_SECS)
}

/// Everything a transit needs, from one place: led aim point, cruise speed
/// derived from the time curve, and the actual cruise duration. Shared by
/// manual burns, the autopilot and probe launches — do not duplicate.
#[derive(Clone, Copy, Debug)]
pub struct TransitPlan {
    pub aim: [f32; 3],
    pub speed: f32,
    pub cruise_secs: f32,
    pub distance_m: f32,
}

pub fn plan_transit(target: CelestialBody, t_secs: f32, from: [f32; 3]) -> TransitPlan {
    let d3 = |a: [f32; 3], b: [f32; 3]| -> f32 {
        let dx = a[0] - b[0];
        let dy = a[1] - b[1];
        let dz = a[2] - b[2];
        (dx * dx + dy * dy + dz * dz).sqrt()
    };
    // Speed from the departure-time distance, then lead the moving target.
    let d0 = d3(from, body_center_at(target, t_secs)).max(1.0);
    let speed = (d0 / transit_cruise_secs(d0)).max(1.0);
    let aim = body_intercept_point(target, t_secs, from, speed);
    let distance_m = d3(from, aim).max(1.0);
    TransitPlan {
        aim,
        speed,
        cruise_secs: distance_m / speed,
        distance_m,
    }
}

/// Unit vector from observer toward another body (flight coords).
pub fn direction_to(observer: [f32; 3], target: [f32; 3]) -> [f32; 3] {
    let dx = target[0] - observer[0];
    let dy = target[1] - observer[1];
    let dz = target[2] - observer[2];
    let len = (dx * dx + dy * dy + dz * dz).sqrt().max(1e-6);
    [dx / len, dy / len, dz / len]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cruise_curve_moon_short_far_capped() {
        let moon = transit_cruise_secs(416_000.0);
        assert!(moon >= 18.0 && moon < 60.0, "moon cruise {moon}");
        assert!((transit_cruise_secs(3_600_000.0) - 180.0).abs() < 1.0);
        assert!((transit_cruise_secs(80_000_000.0) - 180.0).abs() < 1e-3);
    }

    #[test]
    fn plan_transit_speed_matches_curve() {
        let plan = plan_transit(CelestialBody::Moon, 0.0, [0.0, 0.0, 0.0]);
        assert!(plan.cruise_secs > 10.0 && plan.cruise_secs < 60.0);
        assert!((plan.speed * plan.cruise_secs - plan.distance_m).abs() / plan.distance_m < 0.01);
    }

    #[test]
    fn earth_starts_at_catalog_origin() {
        let c0 = body_center_at(CelestialBody::Earth, 0.0);
        let cat = body_def(CelestialBody::Earth).center;
        assert!((c0[0] - cat[0]).abs() < 1.0);
        assert!((c0[2] - cat[2]).abs() < 1.0);
    }

    #[test]
    fn moon_orbits_earth() {
        let t = 42.0;
        let e = earth_center_at(t);
        let m0 = moon_center_at(t);
        let m1 = moon_center_at(t + 120.0);
        let d0 = dist(e, m0);
        let d1 = dist(earth_center_at(t + 120.0), m1);
        assert!((d0 - 416_000.0).abs() < 4_000.0);
        assert!((d1 - 416_000.0).abs() < 4_000.0);
        assert!(dist(m0, m1) > 1000.0);
    }

    fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
        let dx = a[0] - b[0];
        let dy = a[1] - b[1];
        let dz = a[2] - b[2];
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
}
