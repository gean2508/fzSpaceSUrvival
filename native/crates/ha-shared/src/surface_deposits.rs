//! Procedural mineral deposits on sphere surfaces (Luna + visitable planets).

use crate::celestial_bodies::CelestialBody;
use crate::constants::{LAUNCH_PAD_ZONE_X, LAUNCH_PAD_ZONE_Z};
use crate::luna_terrain::LUNA_TANGENT_HALF;
use crate::planet_surface::planet_tangent_half;
use crate::rng::Rng;
use crate::world::InteractableKind;

#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceDeposit {
    pub id: u32,
    pub kind: InteractableKind,
    pub tangent_x: f32,
    pub tangent_z: f32,
}

const LUNA_TARGET: usize = 28;
const PLANET_TARGET: usize = 22;
const LUNA_SPAWN_CLEAR: f32 = 6.0;
const PLANET_SPAWN_CLEAR: f32 = 5.0;
const MIN_DEPOSIT_SPACING: f32 = 4.5;

pub fn generate_earth_deposits(seed: u32) -> Vec<SurfaceDeposit> {
    generate_sphere_deposits(CelestialBody::Earth, seed, 52, 22.0)
}

pub fn generate_luna_deposits(seed: u32) -> Vec<SurfaceDeposit> {
    generate_sphere_deposits(CelestialBody::Moon, seed, LUNA_TARGET, LUNA_SPAWN_CLEAR)
}

pub fn generate_planet_deposits(body: CelestialBody, seed: u32) -> Vec<SurfaceDeposit> {
    generate_sphere_deposits(body, seed, PLANET_TARGET, PLANET_SPAWN_CLEAR)
}

fn generate_sphere_deposits(
    body: CelestialBody,
    seed: u32,
    target: usize,
    spawn_clear: f32,
) -> Vec<SurfaceDeposit> {
    let tangent_half = match body {
        CelestialBody::Moon => LUNA_TANGENT_HALF,
        _ => planet_tangent_half(body),
    };
    let weights = deposit_weights(body);
    let mut rng = Rng::new(seed ^ 0xde_05170);
    let mut out: Vec<SurfaceDeposit> = Vec::new();
    let mut attempts = 0;
    while out.len() < target && attempts < 900 {
        attempts += 1;
        let tx = rng.range(-tangent_half * 0.85, tangent_half * 0.85);
        let tz = rng.range(-tangent_half * 0.85, tangent_half * 0.85);
        if too_close_to_spawn(body, tx, tz, spawn_clear) {
            continue;
        }
        if out.iter().any(|d| {
            let dx = d.tangent_x - tx;
            let dz = d.tangent_z - tz;
            dx * dx + dz * dz < MIN_DEPOSIT_SPACING * MIN_DEPOSIT_SPACING
        }) {
            continue;
        }
        let kind = pick_kind(&mut rng, &weights);
        out.push(SurfaceDeposit {
            id: out.len() as u32,
            kind,
            tangent_x: tx,
            tangent_z: tz,
        });
    }
    out
}

fn too_close_to_spawn(body: CelestialBody, tx: f32, tz: f32, clear: f32) -> bool {
    if body == CelestialBody::Earth {
        let dx = tx - LAUNCH_PAD_ZONE_X;
        let dz = tz - LAUNCH_PAD_ZONE_Z;
        return dx * dx + dz * dz < clear * clear;
    }
    tx * tx + tz * tz < clear * clear
}

struct DepositWeights {
    tree: f32,
    rock: f32,
    container: f32,
    ore_iron: f32,
    ore_gold: f32,
    gem_blue: f32,
    gem_red: f32,
}

fn deposit_weights(body: CelestialBody) -> DepositWeights {
    match body {
        CelestialBody::Earth => DepositWeights {
            tree: 0.28,
            rock: 0.22,
            container: 0.14,
            ore_iron: 0.16,
            ore_gold: 0.08,
            gem_blue: 0.07,
            gem_red: 0.05,
        },
        CelestialBody::Moon => DepositWeights {
            tree: 0.0,
            rock: 0.32,
            container: 0.14,
            ore_iron: 0.42,
            ore_gold: 0.06,
            gem_blue: 0.04,
            gem_red: 0.02,
        },
        CelestialBody::Mars => DepositWeights {
            tree: 0.0,
            rock: 0.28,
            container: 0.10,
            ore_iron: 0.34,
            ore_gold: 0.16,
            gem_blue: 0.06,
            gem_red: 0.06,
        },
        CelestialBody::Mercury => DepositWeights {
            tree: 0.0,
            rock: 0.22,
            container: 0.08,
            ore_iron: 0.18,
            ore_gold: 0.28,
            gem_blue: 0.08,
            gem_red: 0.16,
        },
        CelestialBody::Venus => DepositWeights {
            tree: 0.0,
            rock: 0.38,
            container: 0.28,
            ore_iron: 0.14,
            ore_gold: 0.10,
            gem_blue: 0.06,
            gem_red: 0.04,
        },
        CelestialBody::Phobos | CelestialBody::Deimos => DepositWeights {
            tree: 0.0,
            rock: 0.40,
            container: 0.12,
            ore_iron: 0.36,
            ore_gold: 0.08,
            gem_blue: 0.02,
            gem_red: 0.02,
        },
        CelestialBody::Io => DepositWeights {
            tree: 0.0,
            rock: 0.30,
            container: 0.10,
            ore_iron: 0.20,
            ore_gold: 0.10,
            gem_blue: 0.08,
            gem_red: 0.22,
        },
        CelestialBody::Europa => DepositWeights {
            tree: 0.0,
            rock: 0.18,
            container: 0.28,
            ore_iron: 0.12,
            ore_gold: 0.08,
            gem_blue: 0.26,
            gem_red: 0.08,
        },
        CelestialBody::Titan => DepositWeights {
            tree: 0.0,
            rock: 0.32,
            container: 0.34,
            ore_iron: 0.14,
            ore_gold: 0.08,
            gem_blue: 0.08,
            gem_red: 0.04,
        },
        CelestialBody::Uranus | CelestialBody::Neptune => DepositWeights {
            tree: 0.0,
            rock: 0.20,
            container: 0.26,
            ore_iron: 0.10,
            ore_gold: 0.10,
            gem_blue: 0.22,
            gem_red: 0.12,
        },
        _ => DepositWeights {
            tree: 0.0,
            rock: 0.35,
            container: 0.20,
            ore_iron: 0.25,
            ore_gold: 0.10,
            gem_blue: 0.06,
            gem_red: 0.04,
        },
    }
}

fn pick_kind(rng: &mut Rng, w: &DepositWeights) -> InteractableKind {
    let roll = rng.next();
    let mut acc = w.tree;
    if roll < acc {
        return InteractableKind::Tree;
    }
    acc += w.rock;
    if roll < acc {
        return InteractableKind::Rock;
    }
    acc += w.container;
    if roll < acc {
        return InteractableKind::Container;
    }
    acc += w.ore_iron;
    if roll < acc {
        return InteractableKind::OreIron;
    }
    acc += w.ore_gold;
    if roll < acc {
        return InteractableKind::OreGold;
    }
    acc += w.gem_blue;
    if roll < acc {
        return InteractableKind::GemBlue;
    }
    InteractableKind::GemRed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luna_deposits_deterministic() {
        let a = generate_luna_deposits(7);
        let b = generate_luna_deposits(7);
        assert_eq!(a, b);
        assert!(a.len() >= 20);
    }

    #[test]
    fn mars_has_ore_nodes() {
        let d = generate_planet_deposits(CelestialBody::Mars, 99);
        assert!(d.iter().any(|n| matches!(n.kind, InteractableKind::OreIron | InteractableKind::OreGold)));
    }
}
