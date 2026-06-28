//! Generic walkable planet shell — tangent coords + procedural displacement (Luna pattern).

use crate::celestial_bodies::{body_center, body_terrain_seed, body_walk_radius, CelestialBody};
use crate::constants::{LAUNCH_PAD_ZONE_RADIUS, LAUNCH_PAD_ZONE_X, LAUNCH_PAD_ZONE_Z, MAX_WALK_SLOPE, SLOPE_SAMPLE, STEP_HEIGHT};
use crate::cosmic_scale::{EARTH_GAMEPLAY_RADIUS_M, MOON_GAMEPLAY_RADIUS_M};
use crate::luna_terrain::{
    altitude_above_luna_gameplay, luna_displacement_at_dir, luna_gameplay_center,
    luna_gameplay_world_pos, smooth_noise3,
};
use crate::movement::MovementBody;

/// Tangent half-extent for a walkable shell.
pub fn planet_tangent_half(body: CelestialBody) -> f32 {
    std::f32::consts::PI * body_walk_radius(body) * 0.5
}

pub fn earth_gameplay_center() -> [f32; 3] {
    [0.0, -EARTH_GAMEPLAY_RADIUS_M, 0.0]
}

pub fn planet_gameplay_center(body: CelestialBody) -> [f32; 3] {
    if body == CelestialBody::Earth {
        return earth_gameplay_center();
    }
    if body == CelestialBody::Moon {
        return luna_gameplay_center();
    }
    let r = body_walk_radius(body);
    let c = body_center(body);
    [c[0], c[1] - r, c[2]]
}

pub fn planet_dir_from_tangent(body: CelestialBody, x: f32, z: f32) -> [f32; 3] {
    stereographic_dir_from_tangent(walk_radius(body), x, z)
}

pub fn planet_tangent_from_dir(body: CelestialBody, dir: [f32; 3]) -> (f32, f32) {
    stereographic_tangent_from_dir(walk_radius(body), dir)
}

/// North-pole stereographic chart — covers the full sphere (south pole at infinity).
pub fn stereographic_dir_from_tangent(r: f32, x: f32, z: f32) -> [f32; 3] {
    let sx = x / r;
    let sz = z / r;
    let len2 = sx * sx + sz * sz;
    let s = 1.0 / (len2 + 1.0);
    normalize3([2.0 * sx * s, (1.0 - len2) * s, 2.0 * sz * s])
}

/// Inverse stereographic; south-pole singularity uses south-hemisphere chart fallback.
pub fn stereographic_tangent_from_dir(r: f32, dir: [f32; 3]) -> (f32, f32) {
    let d = normalize3(dir);
    if d[1] <= -0.9995 {
        let denom = (1.0 - d[1]).max(1e-4);
        return (r * d[0] / denom, r * -d[2] / denom);
    }
    let denom = (1.0 + d[1]).max(1e-4);
    (r * d[0] / denom, r * d[2] / denom)
}

fn walk_radius(body: CelestialBody) -> f32 {
    if body == CelestialBody::Earth {
        EARTH_GAMEPLAY_RADIUS_M
    } else {
        body_walk_radius(body)
    }
}

/// Advance `(tx, tz)` by `dx`/`dz` metres along stereographic +X / +Z on the shell.
pub fn sphere_tangent_step(body: CelestialBody, tx: f32, tz: f32, dx: f32, dz: f32) -> (f32, f32) {
    if dx.abs() + dz.abs() < 1e-8 {
        return (tx, tz);
    }
    let r = walk_radius(body);
    let center = planet_gameplay_center(body);
    let dir = planet_dir_from_tangent(body, tx, tz);
    let disp = surface_relief_at(body, tx, tz);
    let shell = r + disp;
    let pos = [
        center[0] + dir[0] * shell,
        center[1] + dir[1] * shell,
        center[2] + dir[2] * shell,
    ];
    // Jacobian of stereographic map — +dx / +dz align with tangent XZ (not world ENU).
    let eps = 0.05_f32;
    let dir_x = planet_dir_from_tangent(body, tx + eps, tz);
    let dir_z = planet_dir_from_tangent(body, tx, tz + eps);
    let east = normalize3([
        (dir_x[0] - dir[0]) / eps * shell,
        (dir_x[1] - dir[1]) / eps * shell,
        (dir_x[2] - dir[2]) / eps * shell,
    ]);
    let north = normalize3([
        (dir_z[0] - dir[0]) / eps * shell,
        (dir_z[1] - dir[1]) / eps * shell,
        (dir_z[2] - dir[2]) / eps * shell,
    ]);
    let trial = [
        pos[0] + east[0] * dx + north[0] * dz,
        pos[1] + east[1] * dx + north[1] * dz,
        pos[2] + east[2] * dx + north[2] * dz,
    ];
    let off = [
        trial[0] - center[0],
        trial[1] - center[1],
        trial[2] - center[2],
    ];
    let new_dir = normalize3(off);
    planet_tangent_from_dir(body, new_dir)
}

pub fn planet_displacement_at_dir(body: CelestialBody, dir: [f32; 3]) -> f32 {
    if body == CelestialBody::Moon {
        return luna_displacement_at_dir(dir);
    }
    let n = normalize3(dir);
    let seed = body_terrain_seed(body);
    let freq_bias = (seed as f32 * 0.0001).fract() * 0.5;
    if body == CelestialBody::Earth {
        let mut h = 0.0f32;
        h += smooth_noise3(n, 2.8 + freq_bias) * 0.85;
        h += smooth_noise3(n, 6.5 + freq_bias) * 0.35;
        return h.clamp(-0.8, 1.6);
    }
    let mut h = 0.0f32;
    h += smooth_noise3(n, 3.0 + freq_bias) * 1.0;
    h += smooth_noise3(n, 7.0 + freq_bias) * 0.4;
    (h - 0.25).clamp(-1.2, 2.0)
}

/// Radial relief (metres above ideal sphere) — slope + Earth locomotion height.
pub fn surface_relief_at(body: CelestialBody, tx: f32, tz: f32) -> f32 {
    let dir = planet_dir_from_tangent(body, tx, tz);
    let mut disp = planet_displacement_at_dir(body, dir);
    if body == CelestialBody::Earth {
        disp = flatten_launch_pad(tx, tz, disp);
    }
    disp
}

fn flatten_launch_pad(tx: f32, tz: f32, disp: f32) -> f32 {
    let dx = tx - LAUNCH_PAD_ZONE_X;
    let dz = tz - LAUNCH_PAD_ZONE_Z;
    let d = (dx * dx + dz * dz).sqrt();
    if d >= LAUNCH_PAD_ZONE_RADIUS {
        return disp;
    }
    let t = (d / LAUNCH_PAD_ZONE_RADIUS).powi(2);
    disp * t
}

pub fn planet_ground_y_at(body: CelestialBody, tx: f32, tz: f32) -> f32 {
    surface_relief_at(body, tx, tz)
}

pub fn planet_gameplay_world_pos(body: CelestialBody, tx: f32, tz: f32, lift_m: f32) -> [f32; 3] {
    if body == CelestialBody::Moon {
        return luna_gameplay_world_pos(tx, tz, lift_m);
    }
    let r = if body == CelestialBody::Earth {
        EARTH_GAMEPLAY_RADIUS_M
    } else {
        body_walk_radius(body)
    };
    let dir = planet_dir_from_tangent(body, tx, tz);
    let disp = surface_relief_at(body, tx, tz);
    let center = planet_gameplay_center(body);
    let radial = r + disp + lift_m;
    [
        center[0] + dir[0] * radial,
        center[1] + dir[1] * radial,
        center[2] + dir[2] * radial,
    ]
}

pub fn altitude_above_planet_gameplay(body: CelestialBody, pos: [f32; 3]) -> f32 {
    if body == CelestialBody::Moon {
        return altitude_above_luna_gameplay(pos);
    }
    let c = planet_gameplay_center(body);
    let dx = pos[0] - c[0];
    let dy = pos[1] - c[1];
    let dz = pos[2] - c[2];
    let dist = (dx * dx + dy * dy + dz * dz).sqrt();
    let dir = normalize3([dx, dy, dz]);
    let disp = planet_displacement_at_dir(body, dir);
    dist - body_walk_radius(body) - disp
}

pub fn remap_planet_gameplay_to_orbital(body: CelestialBody, pos: [f32; 3]) -> [f32; 3] {
    if body == CelestialBody::Moon {
        let gp = luna_gameplay_center();
        let oc = body_center(CelestialBody::Moon);
        let offset = [pos[0] - gp[0], pos[1] - gp[1], pos[2] - gp[2]];
        let dist = dist3(pos, gp).max(body_walk_radius(body) + 0.5);
        let n = normalize3(offset);
        return [
            oc[0] + n[0] * dist,
            oc[1] + n[1] * dist,
            oc[2] + n[2] * dist,
        ];
    }
    let gp = planet_gameplay_center(body);
    let oc = body_center(body);
    let offset = [pos[0] - gp[0], pos[1] - gp[1], pos[2] - gp[2]];
    let dist = dist3(pos, gp).max(body_walk_radius(body) + 0.5);
    let n = normalize3(offset);
    [
        oc[0] + n[0] * dist,
        oc[1] + n[1] * dist,
        oc[2] + n[2] * dist,
    ]
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

/// Re-export Moon gameplay radius for compatibility.
pub fn moon_walk_radius() -> f32 {
    MOON_GAMEPLAY_RADIUS_M
}

// --- Walkable slope limits (Earth, Luna, all planets) ---

/// Rise/run gradient magnitude (1.0 = 45°).
pub fn surface_slope_at(body: CelestialBody, tx: f32, tz: f32, sample: f32) -> f32 {
    let h0 = surface_relief_at(body, tx, tz);
    let hx = surface_relief_at(body, tx + sample, tz) - h0;
    let hz = surface_relief_at(body, tx, tz + sample) - h0;
    (hx * hx + hz * hz).sqrt() / sample.max(0.1)
}

pub fn block_surface_steep_step(
    body: &mut MovementBody,
    surface: CelestialBody,
    from_x: f32,
    from_z: f32,
) {
    let total_dx = body.x - from_x;
    let total_dz = body.z - from_z;
    if total_dx * total_dx + total_dz * total_dz < 1e-8 {
        return;
    }
    let h0 = surface_relief_at(surface, from_x, from_z);
    const SEGMENTS: i32 = 8;
    let mut cx = from_x;
    let mut cz = from_z;
    let micro_dx = total_dx / SEGMENTS as f32;
    let micro_dz = total_dz / SEGMENTS as f32;
    for _ in 1..=SEGMENTS {
        let (nx, nz) = sphere_tangent_step(surface, cx, cz, micro_dx, micro_dz);
        let rise = surface_relief_at(surface, nx, nz) - h0;
        if rise > STEP_HEIGHT + 0.02 {
            let slope = surface_slope_at(surface, nx, nz, SLOPE_SAMPLE);
            if slope > MAX_WALK_SLOPE || rise > STEP_HEIGHT * 1.25 {
                body.x = from_x;
                body.z = from_z;
                body.vx = 0.0;
                body.vz = 0.0;
                return;
            }
        }
        cx = nx;
        cz = nz;
    }
}

pub fn block_surface_unwalkable_ascent(
    body: &mut MovementBody,
    surface: CelestialBody,
    from_x: f32,
    from_z: f32,
) {
    let dx = body.x - from_x;
    let dz = body.z - from_z;
    if dx * dx + dz * dz < 1e-6 {
        return;
    }
    let sample = SLOPE_SAMPLE;
    let h0 = surface_relief_at(surface, from_x, from_z);
    let gx = (surface_relief_at(surface, from_x + sample, from_z) - h0) / sample;
    let gz = (surface_relief_at(surface, from_x, from_z + sample) - h0) / sample;
    if gx * dx + gz * dz <= 0.0 {
        return;
    }
    if surface_slope_at(surface, from_x, from_z, sample) > MAX_WALK_SLOPE {
        body.x = from_x;
        body.z = from_z;
        body.vx = 0.0;
        body.vz = 0.0;
    }
}

pub fn follow_surface_ground(
    surface: CelestialBody,
    body: &mut MovementBody,
    from_x: f32,
    from_z: f32,
) {
    if body.vy > 0.08 {
        return;
    }
    let floor = planet_ground_y_at(surface, body.x, body.z);
    // Airborne — do not snap at jump apex (vy≈0) or during lunar hops.
    if body.y > floor + 0.14 {
        return;
    }
    let prev = planet_ground_y_at(surface, from_x, from_z);
    let drop = prev - floor;
    if drop > 0.0 && drop < STEP_HEIGHT * 2.5 {
        body.y = floor;
    } else if body.y >= floor - 0.08 {
        body.y = floor;
    }
}

#[cfg(test)]
mod slope_tests {
    use super::*;
    use crate::celestial_bodies::CelestialBody;

    #[test]
    fn earth_slopes_stay_under_45_degrees_on_grid() {
        let sample = SLOPE_SAMPLE;
        let mut max_slope = 0.0f32;
        for ix in -12..=12 {
            for iz in -12..=12 {
                let tx = ix as f32 * 8.0;
                let tz = iz as f32 * 8.0;
                let s = surface_slope_at(CelestialBody::Earth, tx, tz, sample);
                max_slope = max_slope.max(s);
            }
        }
        assert!(
            max_slope <= MAX_WALK_SLOPE + 0.08,
            "earth max slope {max_slope} exceeds cap"
        );
    }

    #[test]
    fn launch_pad_is_flat() {
        let c = surface_relief_at(CelestialBody::Earth, LAUNCH_PAD_ZONE_X, LAUNCH_PAD_ZONE_Z);
        assert!(c.abs() < 0.05, "pad center relief {c}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mars_ground_is_finite() {
        let y = planet_ground_y_at(CelestialBody::Mars, 10.0, -5.0);
        assert!(y.is_finite());
    }

    #[test]
    fn geodesic_no_flip_mid_moon_meridian() {
        for z in [85.757_f32, 1439.118, 2000.0] {
            let (_, nz) = sphere_tangent_step(CelestialBody::Moon, 0.0, z, 0.0, 0.3);
            assert!(nz > z, "backward step at z={z} -> {nz}");
        }
    }

    #[test]
    fn geodesic_step_monotonic_in_tangent_z() {
        let mut z = -18.0_f32;
        for step in 0..4400 {
            let (_, nz) = sphere_tangent_step(CelestialBody::Moon, 0.0, z, 0.0, 0.3);
            assert!(
                nz > z + 1e-4,
                "tangent z decreased at step {step} z={z} -> {nz}"
            );
            z = nz;
        }
    }

    #[test]
    fn stereographic_round_trip_far_from_pole() {
        let r = walk_radius(CelestialBody::Moon);
        for &(x, z) in &[
            (0.0, 0.0),
            (120.0, -80.0),
            (600.0, 200.0),
            (-900.0, 450.0),
            (1500.0, -1200.0),
        ] {
            let dir = stereographic_dir_from_tangent(r, x, z);
            let (rx, rz) = stereographic_tangent_from_dir(r, dir);
            assert!((rx - x).abs() < 0.6, "x round-trip ({x},{z}): {rx}");
            assert!((rz - z).abs() < 0.6, "z round-trip ({x},{z}): {rz}");
            assert!(dir[1].is_finite());
        }
    }

    #[test]
    fn sphere_tangent_step_loops_moon_without_falling() {
        use crate::movement::MovementBody;
        use crate::surface_locomotion::step_surface_movement;
        use crate::input::InputState;

        let mut b = MovementBody::new(6.0, 0.35);
        b.x = 0.0;
        b.z = -18.0;
        b.y = surface_relief_at(CelestialBody::Moon, b.x, b.z);
        let dt = 1.0 / 20.0;
        let mut input = InputState {
            seq: 0,
            move_x: 0.0,
            move_z: 1.0,
            aim_x: 0.0,
            aim_z: 1.0,
            abilities: 0,
            sprint: 0,
            action: 0,
        };
        let start_x = b.x;
        let start_z = b.z;
        for tick in 0..20000 {
            input.seq = tick;
            step_surface_movement(CelestialBody::Moon, &mut b, &input, dt, false, 0.17);
            assert!(b.y.is_finite(), "y NaN at tick {tick}");
            assert!(
                b.y < 20.0,
                "fell through shell at tick {tick} y={} x={} z={}",
                b.y,
                b.x,
                b.z
            );
        }
        let moved = (b.x - start_x).hypot(b.z - start_z);
        assert!(
            moved > 1200.0,
            "expected long moon walk, only moved {moved}m to ({}, {})",
            b.x,
            b.z
        );
    }
}
