//! Sphere-surface biomes — Whittaker-style classification (elevation × temperature × moisture).

use crate::celestial_bodies::CelestialBody;
use crate::constants::{LAUNCH_PAD_ZONE_RADIUS, LAUNCH_PAD_ZONE_X, LAUNCH_PAD_ZONE_Z};
use crate::planet_surface::planet_tangent_half;
use crate::surface_climate::sample_climate;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceBiome {
    Meadow,
    Forest,
    PineWoods,
    AutumnGrove,
    Wetland,
    Highland,
    Alpine,
    Desert,
    Beach,
    RockyShore,
}

/// Deterministic biome at stereographic tangent `(tx, tz)`.
pub fn surface_biome_at(body: CelestialBody, world_seed: u32, tx: f32, tz: f32) -> SurfaceBiome {
    if body != CelestialBody::Earth {
        return moon_or_planet_biome(body, tx, tz);
    }

    let half = planet_tangent_half(body) * 0.88;
    let dist = (tx * tx + tz * tz).sqrt();
    if dist > half * 0.92 {
        return SurfaceBiome::Beach;
    }
    if dist > half * 0.82 {
        return SurfaceBiome::RockyShore;
    }

    let dx = tx - LAUNCH_PAD_ZONE_X;
    let dz = tz - LAUNCH_PAD_ZONE_Z;
    if dx * dx + dz * dz < LAUNCH_PAD_ZONE_RADIUS * LAUNCH_PAD_ZONE_RADIUS {
        return SurfaceBiome::Meadow;
    }

    classify_whittaker(sample_climate(body, tx, tz, world_seed))
}

fn classify_whittaker(c: crate::surface_climate::ClimateSample) -> SurfaceBiome {
    let e = c.elevation;
    let t = c.temperature;
    let m = c.moisture;

    if e > 0.82 && t < 0.38 {
        return SurfaceBiome::Alpine;
    }
    if e > 0.68 && m < 0.42 {
        return SurfaceBiome::Highland;
    }
    if m < 0.28 && t > 0.55 {
        return SurfaceBiome::Desert;
    }
    if m > 0.62 && e < 0.35 {
        return SurfaceBiome::Wetland;
    }
    if m > 0.58 && t > 0.45 && t < 0.72 {
        return SurfaceBiome::Forest;
    }
    if m > 0.48 && t < 0.48 {
        return SurfaceBiome::PineWoods;
    }
    if t > 0.52 && m > 0.42 && m < 0.62 {
        return SurfaceBiome::AutumnGrove;
    }
    if m > 0.4 && e < 0.55 {
        return SurfaceBiome::Meadow;
    }
    SurfaceBiome::Highland
}

fn moon_or_planet_biome(body: CelestialBody, tx: f32, tz: f32) -> SurfaceBiome {
    let half = planet_tangent_half(body) * 0.85;
    let dist = (tx * tx + tz * tz).sqrt();
    if dist > half * 0.88 {
        return SurfaceBiome::Beach;
    }
    let elev = crate::planet_surface::surface_relief_at(body, tx, tz);
    if elev > 0.7 {
        SurfaceBiome::Highland
    } else {
        SurfaceBiome::RockyShore
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{LAUNCH_PAD_ZONE_X, LAUNCH_PAD_ZONE_Z};

    #[test]
    fn launch_pad_is_meadow() {
        assert_eq!(
            surface_biome_at(CelestialBody::Earth, 42, LAUNCH_PAD_ZONE_X, LAUNCH_PAD_ZONE_Z),
            SurfaceBiome::Meadow
        );
    }

    #[test]
    fn planet_has_biome_variety() {
        // Sample radius scales with the walkable sphere so the angular coverage
        // is stable across radii (0.375·r ≈ the old 180 m on the 480 m sphere).
        let sample_r = crate::celestial_bodies::body_walk_radius(CelestialBody::Earth) * 0.375;
        let mut kinds = std::collections::HashSet::new();
        for i in 0..120 {
            let a = i as f32 * 0.41;
            let tx = a.cos() * sample_r;
            let tz = a.sin() * sample_r;
            kinds.insert(surface_biome_at(CelestialBody::Earth, 42, tx, tz) as u8);
        }
        assert!(kinds.len() >= 2, "expected biome variety across planet, got {kinds:?}");
    }
}
