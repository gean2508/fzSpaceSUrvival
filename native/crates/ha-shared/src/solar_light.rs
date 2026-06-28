//! Physical solar lighting — Sun at catalog position; all direct light from that vector.
//! Crafted spotlights / panels are separate systems (see `CraftedLight` in ha-game).

use crate::celestial_bodies::{body_atmo_top, body_def, CelestialBody};
use crate::solar_ephemeris::{body_center_at, sun_position};
use crate::constants::{LAUNCH_PAD_ZONE_X, LAUNCH_PAD_ZONE_Z};
use crate::luna_terrain::luna_gameplay_center;
use crate::planet_surface::planet_gameplay_center;

/// Direction from observer toward the Sun (unit, world flight metres).
pub fn sun_direction_to(observer_flight: [f32; 3]) -> [f32; 3] {
    let sun = sun_position();
    let dx = sun[0] - observer_flight[0];
    let dy = sun[1] - observer_flight[1];
    let dz = sun[2] - observer_flight[2];
    normalize3([dx, dy, dz])
}

/// Light travel direction (from Sun toward scene) — Bevy `DirectionalLight` forward is -Z.
pub fn sun_light_direction(observer_flight: [f32; 3]) -> [f32; 3] {
    let to = sun_direction_to(observer_flight);
    [-to[0], -to[1], -to[2]]
}

pub fn sun_center_world() -> [f32; 3] {
    sun_position()
}

/// Map a gameplay-shell world position into flight-scale coordinates.
pub fn gameplay_world_to_flight(body: CelestialBody, world_pos: [f32; 3]) -> [f32; 3] {
    let gp = if body == CelestialBody::Moon {
        luna_gameplay_center()
    } else {
        planet_gameplay_center(body)
    };
    let fc = body_center_at(body, 0.0);
    [
        fc[0] + world_pos[0] - gp[0],
        fc[1] + world_pos[1] - gp[1],
        fc[2] + world_pos[2] - gp[2],
    ]
}

/// Observer above the launch pad at the atmosphere edge (flight metres).
pub fn orbital_dock_flight_pos() -> [f32; 3] {
    let ec = body_center_at(CelestialBody::Earth, 0.0);
    [
        ec[0] + LAUNCH_PAD_ZONE_X,
        ec[1] + body_atmo_top(CelestialBody::Earth) + 8.0,
        ec[2] + LAUNCH_PAD_ZONE_Z,
    ]
}

#[derive(Clone, Copy, Debug)]
pub struct SolarSample {
    /// True direction toward Sun (for solar panels / physics).
    pub physical_to_sun: [f32; 3],
    /// Direction used for rendering on spinning bodies (Earth day/night).
    pub lit_to_sun: [f32; 3],
    pub illuminance: f32,
    pub color: [f32; 3],
    /// 0..1 direct flux on a horizontal surface facing up at the observer.
    pub disk_visibility: f32,
}

/// `day_fraction`: 0..1 Earth spin phase (only affects `lit_to_sun` on Earth).
pub fn sample_solar(
    observer_flight: [f32; 3],
    surface_normal: Option<[f32; 3]>,
    body: CelestialBody,
    day_fraction: f32,
    in_cave: bool,
) -> SolarSample {
    if in_cave {
        return SolarSample {
            physical_to_sun: [0.0, 1.0, 0.0],
            lit_to_sun: [0.0, 1.0, 0.0],
            illuminance: 0.0,
            color: [0.0, 0.0, 0.0],
            disk_visibility: 0.0,
        };
    }

    let physical = sun_direction_to(observer_flight);
    let lit = if body == CelestialBody::Earth {
        spin_sun_for_earth_day(physical, day_fraction)
    } else {
        physical
    };

    let sun_rgb = body_def(CelestialBody::Sun).color;
    let dist = dist3(observer_flight, sun_center_world()).max(1.0);
    const REF_DIST: f32 = 150_000.0;
    const REF_LUX: f32 = 110_000.0;
    let mut illuminance = REF_LUX * (REF_DIST / dist).powi(2);
    illuminance = illuminance.clamp(800.0, 130_000.0);

    let normal = surface_normal.unwrap_or([0.0, 1.0, 0.0]);
    let mut disk = dot3(normal, lit).max(0.0);
    if body == CelestialBody::Moon {
        // Vacuum — crisp terminator, no air glow fill.
        disk = if disk > 0.02 { disk } else { 0.0 };
        illuminance *= 1.15;
    }

    SolarSample {
        physical_to_sun: physical,
        lit_to_sun: lit,
        illuminance: illuminance * disk.max(0.05),
        color: sun_rgb,
        disk_visibility: disk,
    }
}

/// Earth spin: rotate apparent sun direction for day/night while catalog Sun stays fixed.
pub fn spin_sun_for_earth_day(physical_to_sun: [f32; 3], day_fraction: f32) -> [f32; 3] {
    let angle = -day_fraction * std::f32::consts::TAU;
    let (s, c) = angle.sin_cos();
    let x = physical_to_sun[0];
    let z = physical_to_sun[2];
    normalize3([x * c - z * s, physical_to_sun[1], x * s + z * c])
}

fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    [v[0] / len, v[1] / len, v[2] / len]
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn dist3(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn earth_surface_sun_is_westward() {
        let ec = body_center_at(CelestialBody::Earth, 0.0);
        let observer = [ec[0] + 20.0, ec[1] + 480.0, ec[2] - 10.0];
        let dir = sun_direction_to(observer);
        assert!(dir[0] < -0.9, "sun should be -X from Earth, got {dir:?}");
    }

    #[test]
    fn moon_sees_same_sun_hemisphere() {
        let mc = body_center_at(CelestialBody::Moon, 0.0);
        let observer = [mc[0], mc[1] + 400.0, mc[2]];
        let dir = sun_direction_to(observer);
        assert!(dir[0] < -0.95);
    }

    #[test]
    fn night_side_zero_disk() {
        let ec = body_center_at(CelestialBody::Earth, 0.0);
        let sample = sample_solar(
            [ec[0], ec[1] + 400.0, ec[2]],
            Some([0.0, 1.0, 0.0]),
            CelestialBody::Earth,
            0.75,
            false,
        );
        assert!(sample.disk_visibility < 0.1 || sample.illuminance < 15_000.0);
    }
}
