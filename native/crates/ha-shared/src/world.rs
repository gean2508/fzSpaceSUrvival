//! SFS-style world — seed + streamed chunks (flora/deposits generated on demand).

use crate::celestial_bodies::CelestialBody;
use crate::planet_surface::planet_ground_y_at;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractableKind {
    Tree,
    Rock,
    Container,
    OreIron,
    OreGold,
    GemBlue,
    GemRed,
}

impl InteractableKind {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Tree => "tree",
            Self::Rock => "rock",
            Self::Container => "container",
            Self::OreIron => "ore_iron",
            Self::OreGold => "ore_gold",
            Self::GemBlue => "gem_blue",
            Self::GemRed => "gem_red",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct InteractableNode {
    pub id: u32,
    pub kind: InteractableKind,
    pub x: f32,
    pub z: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedWorld {
    pub seed: u32,
}

/// Earth home-body — flora and deposits stream per chunk at runtime.
pub fn generate_world(seed: u32) -> GeneratedWorld {
    GeneratedWorld { seed }
}

pub fn ground_height_at(tx: f32, tz: f32) -> f32 {
    planet_ground_y_at(CelestialBody::Earth, tx, tz)
}

pub fn ground_height_at_footprint(tx: f32, tz: f32, footprint_radius: f32) -> f32 {
    if footprint_radius <= 0.001 {
        return ground_height_at(tx, tz);
    }
    let mut h = ground_height_at(tx, tz);
    let r = footprint_radius;
    for (ox, oz) in [
        (r, 0.0),
        (-r, 0.0),
        (0.0, r),
        (0.0, -r),
        (r * 0.7, r * 0.7),
        (-r * 0.7, r * 0.7),
    ] {
        h = h.max(ground_height_at(tx + ox, tz + oz));
    }
    h
}

pub fn ground_height_for_locomotion(tx: f32, tz: f32, footprint_radius: f32) -> f32 {
    ground_height_at_footprint(tx, tz, footprint_radius)
}

pub fn collision_obstacles(_world: &GeneratedWorld) -> Vec<crate::types::Obstacle> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_deterministic_for_seed() {
        let a = generate_world(42);
        let b = generate_world(42);
        assert_eq!(a, b);
    }
}
