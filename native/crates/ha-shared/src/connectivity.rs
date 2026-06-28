//! Line-of-sight connectivity — Earth uplink and orbital repeaters.

use crate::celestial_bodies::{body_radius, CelestialBody};
use crate::solar_ephemeris::body_center_at;
use crate::constants::{LAUNCH_PAD_ZONE_X, LAUNCH_PAD_ZONE_Z};
use crate::vessel::ConnectivitySnapshot;

/// Earth ground station (launch pad) in flight metres.
pub fn earth_uplink_pos(t_secs: f32) -> [f32; 3] {
    let ec = body_center_at(CelestialBody::Earth, t_secs);
    [
        ec[0] + LAUNCH_PAD_ZONE_X,
        ec[1] + 480.0,
        ec[2] + LAUNCH_PAD_ZONE_Z,
    ]
}

/// Orbital repeater at dock altitude.
pub fn orbital_repeater_pos(t_secs: f32) -> [f32; 3] {
    let ec = body_center_at(CelestialBody::Earth, t_secs);
    [ec[0], ec[1] + 600.0, ec[2]]
}

pub fn has_line_of_sight(a: [f32; 3], b: [f32; 3], t_secs: f32) -> bool {
    let dist = dist3(a, b);
    if dist < 1.0 {
        return true;
    }
    let dir = normalize3([b[0] - a[0], b[1] - a[1], b[2] - a[2]]);
    for blocker in [CelestialBody::Earth, CelestialBody::Moon] {
        let c = body_center_at(blocker, t_secs);
        let r = body_radius(blocker);
        if ray_sphere_hit(a, dir, c, r) {
            let hit_dist = ray_sphere_hit_dist(a, dir, c, r).unwrap_or(f32::MAX);
            if hit_dist < dist - 2.0 {
                return false;
            }
        }
    }
    true
}

pub fn sample_connectivity(observer: [f32; 3], body: CelestialBody) -> ConnectivitySnapshot {
    sample_connectivity_at(observer, body, 0.0)
}

pub fn sample_connectivity_at(
    observer: [f32; 3],
    _body: CelestialBody,
    t_secs: f32,
) -> ConnectivitySnapshot {
    let earth = earth_uplink_pos(t_secs);
    let repeater = orbital_repeater_pos(t_secs);
    let earth_los = has_line_of_sight(observer, earth, t_secs);
    let repeater_los = has_line_of_sight(observer, repeater, t_secs);
    let mut strength: f32 = 0.0;
    if earth_los {
        let d = dist3(observer, earth).max(100.0);
        strength = strength.max((800.0 / d).clamp(0.15, 1.0));
    }
    if repeater_los {
        let d = dist3(observer, repeater).max(50.0);
        strength = strength.max((400.0 / d).clamp(0.2, 1.0));
    }
    ConnectivitySnapshot {
        earth_los,
        repeater_los,
        signal_strength: strength,
    }
}

fn ray_sphere_hit(origin: [f32; 3], dir: [f32; 3], center: [f32; 3], radius: f32) -> bool {
    ray_sphere_hit_dist(origin, dir, center, radius).is_some()
}

fn ray_sphere_hit_dist(origin: [f32; 3], dir: [f32; 3], center: [f32; 3], radius: f32) -> Option<f32> {
    let oc = [
        origin[0] - center[0],
        origin[1] - center[1],
        origin[2] - center[2],
    ];
    let b = 2.0 * (oc[0] * dir[0] + oc[1] * dir[1] + oc[2] * dir[2]);
    let c = oc[0] * oc[0] + oc[1] * oc[1] + oc[2] * oc[2] - radius * radius;
    let disc = b * b - 4.0 * c;
    if disc < 0.0 {
        return None;
    }
    let t = (-b - disc.sqrt()) / 2.0;
    if t > 0.01 {
        Some(t)
    } else {
        None
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn earth_surface_sees_uplink() {
        let obs = earth_uplink_pos(0.0);
        assert!(has_line_of_sight(obs, orbital_repeater_pos(0.0), 0.0));
    }
}
