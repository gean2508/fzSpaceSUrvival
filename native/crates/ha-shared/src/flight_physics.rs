//! SFS/KSP-style arcade flight — continuous thrust, gravity, SOI, atmosphere (no cutscenes).

use crate::celestial_bodies::{
    body_atmo_top, body_gravity_surface, body_radius, body_soi, body_walk_radius, CelestialBody,
};
use crate::solar_ephemeris::body_center_at;
use crate::planet_surface::{planet_gameplay_center, remap_planet_gameplay_to_orbital};

pub use crate::celestial_bodies::{
    body_transit_burn, body_terrain_seed, earth_center, moon_center, BodyDef,
};

pub const ROCKET_THRUST_M_S2: f32 = 22.0;
pub const ROCKET_RETRO_THRUST_M_S2: f32 = 38.0;
pub const ROCKET_FUEL_MAX: f32 = 100.0;
// Bajado de 8.0 → 3.5: con tanque lleno (100) da ~28 s de empuje continuo, lo
// suficiente para alcanzar el espacio y maniobrar en el loop SFS (antes ~12 s se
// agotaba a media subida). Native-only (no espejado en TS) — sin paridad.
pub const ROCKET_FUEL_BURN_PER_S: f32 = 3.5;
pub const ROCKET_RETRO_FUEL_BURN_PER_S: f32 = 14.0;
pub const ROCKET_ROT_SPEED: f32 = 1.8;
pub const ROCKET_MAX_V: f32 = 280.0;
/// Higher cap in open vacuum so a manual Earth→Moon crossing (~52 km) takes
/// ~80 s instead of ~3 min. Near a body's gameplay shell the tighter
/// `ROCKET_MAX_V` still applies so descents stay controllable.
pub const ROCKET_CRUISE_MAX_V: f32 = 640.0;
pub const SAFE_LANDING_V: f32 = 12.0;
pub const PARACHUTE_DRAG: f32 = 2.8;
pub const PARACHUTE_DEPLOY_ALT_M: f32 = 65.0;
pub const PARACHUTE_MIN_SPEED: f32 = 9.0;
pub const PARACHUTE_SAFE_V: f32 = 18.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FlightPhase {
    #[default]
    Surface,
    Powered,
    Coast,
    /// RC retro burn — sprint held, thrust opposite velocity.
    Retro,
}

/// Map gameplay Luna shell coords → orbital flight coords (preserves radial distance).
pub fn remap_luna_gameplay_to_orbital(pos: [f32; 3]) -> [f32; 3] {
    remap_planet_gameplay_to_orbital(CelestialBody::Moon, pos)
}

/// Gravity toward the walkable Luna shell (arcade metres).
pub fn gravity_accel_luna_gameplay(pos: [f32; 3]) -> [f32; 3] {
    gravity_accel_planet_gameplay(CelestialBody::Moon, pos)
}

pub fn gravity_accel_planet_gameplay(body: CelestialBody, pos: [f32; 3]) -> [f32; 3] {
    let c = planet_gameplay_center(body);
    let r = body_walk_radius(body);
    let g0 = body_gravity_surface(body);
    let d = dist3(pos, c).max(r + 0.5);
    let dir = normalize3([c[0] - pos[0], c[1] - pos[1], c[2] - pos[2]]);
    let g = g0 * (r / d).powi(2);
    [dir[0] * g, dir[1] * g, dir[2] * g]
}

fn dist3(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    [v[0] / len, v[1] / len, v[2] / len]
}

/// Dominant body by SOI — nearest centre among overlapping spheres.
pub fn dominant_body(pos: [f32; 3]) -> CelestialBody {
    dominant_body_at(pos, 0.0)
}

pub fn dominant_body_at(pos: [f32; 3], t_secs: f32) -> CelestialBody {
    let mut best = CelestialBody::Earth;
    let mut best_d = f32::MAX;
    for &body in CelestialBody::all() {
        let c = body_center_at(body, t_secs);
        let d = dist3(pos, c);
        if d < body_soi(body) && d < best_d {
            best_d = d;
            best = body;
        }
    }
    best
}

/// Gravity acceleration (m/s²) toward body centre.
pub fn gravity_accel_at(pos: [f32; 3]) -> [f32; 3] {
    gravity_accel_at_time(pos, 0.0)
}

pub fn gravity_accel_at_time(pos: [f32; 3], t_secs: f32) -> [f32; 3] {
    let body = dominant_body_at(pos, t_secs);
    let c = body_center_at(body, t_secs);
    let r = body_radius(body);
    let g0 = body_gravity_surface(body);
    let d = dist3(pos, c).max(r + 0.5);
    let dir = normalize3([c[0] - pos[0], c[1] - pos[1], c[2] - pos[2]]);
    let g = g0 * (r / d).powi(2);
    [dir[0] * g, dir[1] * g, dir[2] * g]
}

pub fn altitude_above_body(pos: [f32; 3], body: CelestialBody) -> f32 {
    altitude_above_body_at(pos, body, 0.0)
}

pub fn altitude_above_body_at(pos: [f32; 3], body: CelestialBody, t_secs: f32) -> f32 {
    let c = body_center_at(body, t_secs);
    let r = body_radius(body);
    dist3(pos, c) - r
}

pub fn in_vacuum(pos: [f32; 3]) -> bool {
    for &body in CelestialBody::all() {
        let top = body_atmo_top(body);
        if top > 0.0 && altitude_above_body(pos, body) < top {
            return false;
        }
    }
    true
}

/// Atmosphere drag for any body with an atmosphere shell.
pub fn atmosphere_drag(pos: [f32; 3], vel: [f32; 3]) -> [f32; 3] {
    let mut drag = [0.0f32; 3];
    for &body in CelestialBody::all() {
        let top = body_atmo_top(body);
        if top <= 0.0 {
            continue;
        }
        let alt = altitude_above_body(pos, body).max(0.0);
        if alt >= top {
            continue;
        }
        let rho = (1.0 - alt / top).max(0.0);
        let k = 0.35 * rho;
        drag[0] -= vel[0] * k;
        drag[1] -= vel[1] * k;
        drag[2] -= vel[2] * k;
    }
    drag
}

/// Extra drag when parachute deployed in atmosphere.
pub fn parachute_drag(vel: [f32; 3], strength: f32) -> [f32; 3] {
    if strength <= 0.0 {
        return [0.0; 3];
    }
    let k = PARACHUTE_DRAG * strength;
    [-vel[0] * k, -vel[1] * k, -vel[2] * k]
}

pub fn should_auto_deploy_parachute(body: CelestialBody, alt: f32, speed: f32) -> bool {
    body_atmo_top(body) > 0.0
        && alt < PARACHUTE_DEPLOY_ALT_M
        && alt > 2.0
        && speed > PARACHUTE_MIN_SPEED
}

pub fn effective_safe_landing_v(parachute_open: bool) -> f32 {
    if parachute_open {
        PARACHUTE_SAFE_V
    } else {
        SAFE_LANDING_V
    }
}

/// Unit outward normal from body surface at world position.
pub fn surface_normal(pos: [f32; 3], body: CelestialBody) -> [f32; 3] {
    surface_normal_at(pos, body, 0.0)
}

pub fn surface_normal_at(pos: [f32; 3], body: CelestialBody, t_secs: f32) -> [f32; 3] {
    let c = body_center_at(body, t_secs);
    normalize3([pos[0] - c[0], pos[1] - c[1], pos[2] - c[2]])
}

/// Snap to body surface + terrain displacement for landing check.
pub fn surface_world_pos(pos: [f32; 3], body: CelestialBody, terrain_h: f32) -> [f32; 3] {
    surface_world_pos_at(pos, body, terrain_h, 0.0)
}

pub fn surface_world_pos_at(pos: [f32; 3], body: CelestialBody, terrain_h: f32, t_secs: f32) -> [f32; 3] {
    let c = body_center_at(body, t_secs);
    let n = surface_normal_at(pos, body, t_secs);
    let r = body_radius(body) + terrain_h;
    [
        c[0] + n[0] * r,
        c[1] + n[1] * r,
        c[2] + n[2] * r,
    ]
}

pub fn sun_proximity_damage(pos: [f32; 3]) -> bool {
    let alt = altitude_above_body(pos, CelestialBody::Sun);
    alt < 800.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn earth_surface_near_origin() {
        let alt = altitude_above_body([0.0, 5.0, 0.0], CelestialBody::Earth);
        assert!(alt > 0.0 && alt < 50.0);
    }

    #[test]
    fn vacuum_high_altitude() {
        assert!(in_vacuum([0.0, 10_000.0, 0.0]));
    }

    #[test]
    fn dominant_body_near_moon() {
        let c = moon_center();
        let b = dominant_body([c[0], c[1] + 500.0, c[2]]);
        assert_eq!(b, CelestialBody::Moon);
    }
}
