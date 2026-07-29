//! Procedural lunar regolith — spherical height field with crater bowls + raised rims.
//! Arcade scale (`MOON_GAMEPLAY_RADIUS_M`); ratios inspired by real Mare/highland geology.

use crate::cosmic_scale::MOON_GAMEPLAY_RADIUS_M;

/// Max tangent coordinate (≈ quarter meridian — wraps for full sphere walk).
pub const LUNA_TANGENT_HALF: f32 = std::f32::consts::PI * MOON_GAMEPLAY_RADIUS_M * 0.5;

const CRATER_SEED: u32 = 0x1A6A_6001;

#[derive(Clone, Copy, Debug)]
struct LunaCrater {
    dir: [f32; 3],
    angular_r: f32,
    depth: f32,
    rim: f32,
}

fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

fn crater_catalog() -> [LunaCrater; 56] {
    let mut out = [LunaCrater {
        dir: [0.0, 1.0, 0.0],
        angular_r: 0.01,
        depth: 0.0,
        rim: 0.0,
    }; 56];
    for i in 0..56u32 {
        let h = hash_u32(CRATER_SEED ^ i.wrapping_mul(0x9e37_79b9));
        let h2 = hash_u32(h);
        let h3 = hash_u32(h2);
        let theta = (h as f32 / u32::MAX as f32) * std::f32::consts::TAU;
        let phi = (h2 as f32 / u32::MAX as f32) * std::f32::consts::PI * 0.92 + 0.04;
        let dir = [
            phi.sin() * theta.cos(),
            phi.cos(),
            phi.sin() * theta.sin(),
        ];
        let scale = match h3 & 7 {
            0 => 2.35, // rare large basins (Mare-scale)
            1 => 0.48, // small impact pits
            2 => 1.75, // medium-large
            3 => 0.72, // small–medium
            _ => 1.0,
        };
        let angular_r = (0.006 + (h2 & 0xff) as f32 / 255.0 * 0.058) * scale;
        let depth = (1.8 + (h3 & 0x7f) as f32 / 127.0 * 14.0) * scale * 1.35;
        let rim = depth * (0.22 + (h & 3) as f32 * 0.05);
        out[i as usize] = LunaCrater {
            dir,
            angular_r,
            depth,
            rim,
        };
    }
    out
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let len = (dot3(v, v)).sqrt().max(1e-6);
    [v[0] / len, v[1] / len, v[2] / len]
}

/// Walkable moon shell centre — matches `moon_world` / `lunar_surface` placement.
pub fn luna_gameplay_center() -> [f32; 3] {
    [0.0, -MOON_GAMEPLAY_RADIUS_M, 0.0]
}

/// World position on the gameplay Luna shell from tangent coords (+ optional lift).
pub fn luna_gameplay_world_pos(tx: f32, tz: f32, lift_m: f32) -> [f32; 3] {
    let r = MOON_GAMEPLAY_RADIUS_M;
    let dir = luna_dir_from_tangent(tx, tz);
    let disp = luna_displacement_at_dir(dir);
    let center = luna_gameplay_center();
    let radial = r + disp + lift_m;
    [
        center[0] + dir[0] * radial,
        center[1] + dir[1] * radial,
        center[2] + dir[2] * radial,
    ]
}

/// Altitude above displaced gameplay Luna surface (metres).
pub fn altitude_above_luna_gameplay(pos: [f32; 3]) -> f32 {
    let c = luna_gameplay_center();
    let dx = pos[0] - c[0];
    let dy = pos[1] - c[1];
    let dz = pos[2] - c[2];
    let dist = (dx * dx + dy * dy + dz * dz).sqrt();
    let dir = normalize3([dx, dy, dz]);
    let disp = luna_displacement_at_dir(dir);
    dist - MOON_GAMEPLAY_RADIUS_M - disp
}

/// Unit direction on moon from tangent coords (north pole = +Y from sphere center).
pub fn luna_dir_from_tangent(x: f32, z: f32) -> [f32; 3] {
    crate::planet_surface::stereographic_dir_from_tangent(MOON_GAMEPLAY_RADIUS_M, x, z)
}

/// Inverse of `luna_dir_from_tangent` — maps surface normal to tangent XZ.
pub fn luna_tangent_from_dir(dir: [f32; 3]) -> (f32, f32) {
    crate::planet_surface::stereographic_tangent_from_dir(MOON_GAMEPLAY_RADIUS_M, dir)
}

/// Slope magnitude from ground relief gradient (metres rise per metre run).
pub fn luna_slope_at(x: f32, z: f32, sample: f32) -> f32 {
    use crate::celestial_bodies::CelestialBody;
    crate::planet_surface::surface_slope_at(CelestialBody::Moon, x, z, sample)
}

/// Block horizontal moves that climb a lunar ridge in one tick.
pub fn block_luna_steep_step(body: &mut crate::movement::MovementBody, from_x: f32, from_z: f32) {
    use crate::celestial_bodies::CelestialBody;
    crate::planet_surface::block_surface_steep_step(body, CelestialBody::Moon, from_x, from_z);
}

/// Block uphill movement into slopes steeper than `MAX_WALK_SLOPE`.
pub fn block_luna_unwalkable_ascent(
    body: &mut crate::movement::MovementBody,
    from_x: f32,
    from_z: f32,
) {
    use crate::celestial_bodies::CelestialBody;
    crate::planet_surface::block_surface_unwalkable_ascent(body, CelestialBody::Moon, from_x, from_z);
}

pub fn luna_clamp_tangent(_body: &mut crate::movement::MovementBody) {
    // Full-sphere stereographic chart — no tangent clamp or wrap.
}

pub fn luna_wrap_tangent(v: f32) -> f32 {
    v
}

/// Height displacement (metres) along outward normal — negative = crater bowl.
pub fn luna_displacement_at_dir(dir: [f32; 3]) -> f32 {
    let dir = normalize3(dir);
    static CRATERS: std::sync::OnceLock<[LunaCrater; 56]> = std::sync::OnceLock::new();
    let craters = CRATERS.get_or_init(crater_catalog);

    let mut h = 0.0f32;
    // Mare basins — broad low regions (dark basalt flats).
    let mare = smooth_noise3(dir, 2.4) * 1.2 - 0.4;
    h += mare * 1.4;

    // Regolith ripples — rolling hills at gameplay scale.
    h += smooth_noise3(dir, 11.0) * 1.5;
    h += smooth_noise3(dir, 23.0) * 0.65;

    for c in craters {
        let d = dot3(dir, c.dir).clamp(-1.0, 1.0);
        let angle = d.acos();
        if angle >= c.angular_r * 1.35 {
            continue;
        }
        let t = 1.0 - (angle / (c.angular_r * 1.35)).min(1.0);
        let bowl = t * t * (3.0 - 2.0 * t);
        h -= c.depth * 0.55 * bowl;
        if t > 0.78 {
            let rim_t = ((t - 0.78) / 0.22).min(1.0);
            h += c.rim * 0.45 * rim_t;
        }
    }

    h.clamp(-14.0, 12.0)
}

pub fn luna_displacement_at(x: f32, z: f32) -> f32 {
    luna_displacement_at_dir(luna_dir_from_tangent(x, z))
}

/// World-space Y for flat sim coords (sphere center at `(0, -R, 0)`).
pub fn luna_ground_y_at(x: f32, z: f32) -> f32 {
    let r = MOON_GAMEPLAY_RADIUS_M;
    let dir = luna_dir_from_tangent(x, z);
    let disp = luna_displacement_at_dir(dir);
    let center_y = -r;
    center_y + dir[1] * (r + disp)
}

pub fn smooth_noise3(dir: [f32; 3], freq: f32) -> f32 {
    let p = [dir[0] * freq, dir[1] * freq, dir[2] * freq];
    let ix = p[0].floor();
    let iy = p[1].floor();
    let iz = p[2].floor();
    let fx = p[0] - ix;
    let fy = p[1] - iy;
    let fz = p[2] - iz;
    let ux = fx * fx * (3.0 - 2.0 * fx);
    let uy = fy * fy * (3.0 - 2.0 * fy);
    let uz = fz * fz * (3.0 - 2.0 * fz);
    let h = |x: f32, y: f32, z: f32| {
        let s = (x * 127.1 + y * 311.7 + z * 74.7).sin() * 43_758.5453;
        s - s.floor()
    };
    let a = h(ix, iy, iz);
    let b = h(ix + 1.0, iy, iz);
    let c = h(ix, iy + 1.0, iz);
    let d = h(ix + 1.0, iy + 1.0, iz);
    let e = h(ix, iy, iz + 1.0);
    let f = h(ix + 1.0, iy, iz + 1.0);
    let g = h(ix, iy + 1.0, iz + 1.0);
    let hh = h(ix + 1.0, iy + 1.0, iz + 1.0);
    let x0 = a + (b - a) * ux;
    let x1 = c + (d - c) * ux;
    let x2 = e + (f - e) * ux;
    let x3 = g + (hh - g) * ux;
    let y0 = x0 + (x1 - x0) * uy;
    let y1 = x2 + (x3 - x2) * uy;
    y0 + (y1 - y0) * uz
}

pub fn luna_albedo_at_dir(dir: [f32; 3]) -> [f32; 4] {
    let mare = smooth_noise3(dir, 1.8);
    if mare < 0.44 {
        [0.32, 0.33, 0.36, 1.0]
    } else if mare < 0.58 {
        [0.42, 0.43, 0.46, 1.0]
    } else {
        [0.52, 0.53, 0.56, 1.0]
    }
}

/// Crater rim markers for client props — `(unit_dir, rim_radius_m)`.
pub fn luna_crater_rim_markers(max: usize) -> Vec<([f32; 3], f32)> {
    static CRATERS: std::sync::OnceLock<[LunaCrater; 56]> = std::sync::OnceLock::new();
    let craters = CRATERS.get_or_init(crater_catalog);
    craters
        .iter()
        .take(max)
        .map(|c| (c.dir, c.angular_r * MOON_GAMEPLAY_RADIUS_M))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luna_pole_near_zero_displacement() {
        let d = luna_displacement_at(0.0, 0.0);
        assert!(d.abs() < 22.0);
    }

    #[test]
    fn luna_tangent_dir_round_trip() {
        for &(x, z) in &[
            (0.0, 0.0),
            (40.0, 0.0),
            (-30.0, 55.0),
            (120.0, -80.0),
            (600.0, 200.0),
            (-900.0, 450.0),
            (1500.0, -1200.0),
        ] {
            let dir = luna_dir_from_tangent(x, z);
            let (rx, rz) = luna_tangent_from_dir(dir);
            assert!((rx - x).abs() < 0.6, "x round-trip at ({x},{z}): {rx}");
            assert!((rz - z).abs() < 0.6, "z round-trip at ({x},{z}): {rz}");
            assert!(dir[1].is_finite());
        }
    }

    #[test]
    fn luna_crater_dents_exist() {
        let mut min = f32::MAX;
        for i in 0..80 {
            let a = i as f32 * 0.31;
            let v = luna_displacement_at(a.cos() * 800.0, a.sin() * 800.0);
            min = min.min(v);
        }
        assert!(min < -4.0, "expected crater bowl min={min}");
    }
}
