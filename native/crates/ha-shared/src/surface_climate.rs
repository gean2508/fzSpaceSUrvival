//! Whittaker-style climate fields on sphere tangent coords (elevation + temperature + moisture).

use crate::celestial_bodies::CelestialBody;
use crate::luna_terrain::smooth_noise3;
use crate::planet_surface::{planet_dir_from_tangent, planet_tangent_half, surface_relief_at};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClimateSample {
    /// Normalized relief metres (~0 low … 1 peak).
    pub elevation: f32,
    /// 0 cold … 1 hot.
    pub temperature: f32,
    /// 0 arid … 1 wet.
    pub moisture: f32,
}

/// Macro + local climate — deterministic from world seed bias + position.
pub fn sample_climate(body: CelestialBody, tx: f32, tz: f32, world_seed: u32) -> ClimateSample {
    let dir = planet_dir_from_tangent(body, tx, tz);
    let relief = surface_relief_at(body, tx, tz);
    let elev = ((relief + 1.2) / 2.8).clamp(0.0, 1.0);

    let half = planet_tangent_half(body).max(1.0);
    let lat_proxy = (tz / half).clamp(-1.0, 1.0);
    let seed_bias = (world_seed as f32 * 0.0001).sin() * 0.08;

    let macro_temp = 0.52 - lat_proxy * 0.22 + seed_bias;
    let local_temp = smooth_noise3(dir, 2.1 + seed_bias * 10.0) * 0.18
        + smooth_noise3(dir, 6.5) * 0.08;
    let temperature = (macro_temp + local_temp).clamp(0.0, 1.0);

    let coast = (tx * tx + tz * tz).sqrt() / half;
    let macro_moist = 0.58 - coast * 0.22;
    let local_moist = smooth_noise3(dir, 3.8) * 0.28 + smooth_noise3(dir, 11.0) * 0.12;
    let moisture = (macro_moist + local_moist).clamp(0.0, 1.0);

    let _ = body;
    ClimateSample {
        elevation: elev,
        temperature,
        moisture,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn climate_is_deterministic() {
        let a = sample_climate(CelestialBody::Earth, 40.0, -30.0, 42);
        let b = sample_climate(CelestialBody::Earth, 40.0, -30.0, 42);
        assert_eq!(a, b);
    }
}
