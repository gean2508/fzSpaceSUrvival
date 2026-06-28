//! Arcade heliocentric ephemeris — one shared universe; bodies orbit Sun, moons orbit planets.

use crate::celestial_bodies::{body_def, body_walk_radius, CelestialBody, FLIGHT_PLANE_Y};

const TAU: f32 = std::f32::consts::TAU;

/// Sun anchor — all planet distances measured from here at t=0.
pub fn sun_position() -> [f32; 3] {
    [-180_000.0, FLIGHT_PLANE_Y, 0.0]
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
    // Farther = slower arcade year (minutes-scale, not real).
    4_200.0 + r * 0.04 + (body_def(body).terrain_seed % 900) as f32
}

/// World position (flight metres) at arcade time `t_secs`.
pub fn body_center_at(body: CelestialBody, t_secs: f32) -> [f32; 3] {
    match body {
        CelestialBody::Sun => sun_position(),
        CelestialBody::Earth => earth_center_at(t_secs),
        CelestialBody::Moon => moon_center_at(t_secs),
        CelestialBody::Phobos => moon_of(CelestialBody::Mars, 1_100.0, 220.0, 0.0, t_secs),
        CelestialBody::Deimos => moon_of(CelestialBody::Mars, 1_400.0, 280.0, 1.7, t_secs),
        _ => heliocentric(body, t_secs),
    }
}

pub fn earth_center_at(t_secs: f32) -> [f32; 3] {
    heliocentric(CelestialBody::Earth, t_secs)
}

pub fn moon_center_at(t_secs: f32) -> [f32; 3] {
    let earth = earth_center_at(t_secs);
    let r = 52_000.0_f32;
    let period = 540.0;
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

/// Walkable shell origin follows the parent body centre.
pub fn gameplay_shell_center_at(body: CelestialBody, t_secs: f32) -> [f32; 3] {
    let c = body_center_at(body, t_secs);
    let r = body_walk_radius(body);
    [c[0], c[1] - r, c[2]]
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
        assert!((d0 - 52_000.0).abs() < 500.0);
        assert!((d1 - 52_000.0).abs() < 500.0);
        assert!(dist(m0, m1) > 1000.0);
    }

    fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
        let dx = a[0] - b[0];
        let dy = a[1] - b[1];
        let dz = a[2] - b[2];
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
}
