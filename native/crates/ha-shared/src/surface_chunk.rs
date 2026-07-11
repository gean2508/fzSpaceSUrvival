//! Chunked world generation — flora + sparse deposits stream as the player explores.

use crate::celestial_bodies::CelestialBody;
use crate::constants::{LAUNCH_PAD_ZONE_RADIUS, LAUNCH_PAD_ZONE_X, LAUNCH_PAD_ZONE_Z};
use crate::rng::Rng;
use crate::surface_biome::{surface_biome_at, SurfaceBiome};
use crate::surface_deposits::SurfaceDeposit;
use crate::surface_flora::FloraProp;
use crate::world::InteractableKind;
use crate::biome_content::{flora_density, pick_flora_asset};

/// Tangent-space chunk size (metres) — industry-standard ~32–64 m cells.
pub const SURFACE_CHUNK_SIZE: f32 = 48.0;
/// How many chunks to keep loaded around the player (radius).
/// Prefer [`surface_chunk_load_radius()`] at runtime — Android uses a smaller ring.
pub const SURFACE_CHUNK_LOAD_RADIUS: i32 = 4;

/// Runtime chunk ring — smaller on Android for fill-rate / entity budget.
/// Android radius 1 (3×3 chunks, 144 m box): on the 480 m walkable sphere the
/// eye-level horizon sits at ~40 m and even tall trees vanish behind the
/// curvature past ~130 m — radius 2 rendered a 240 m ring of provably
/// invisible geometry on a vertex-weak GPU (sky-look test: 4→15 fps).
pub fn surface_chunk_load_radius() -> i32 {
    #[cfg(target_os = "android")]
    {
        1
    }
    #[cfg(not(target_os = "android"))]
    {
        // Desktop is DEV only (weak dev GPU: 940M/NVK). Radius 4 = 81 chunks
        // → ~185 draw calls, GPU-bound to ~4-10 fps. Radius 2 = 25 chunks keeps
        // the dev machine playable; the shipping target (Android) is radius 1.
        2
    }
}

fn flora_grid_step() -> f32 {
    #[cfg(target_os = "android")]
    {
        7.0
    }
    #[cfg(not(target_os = "android"))]
    {
        5.0
    }
}
const LAUNCH_PAD_MARGIN: f32 = LAUNCH_PAD_ZONE_RADIUS + 2.0;

pub fn chunk_key_at(tx: f32, tz: f32) -> (i32, i32) {
    (
        (tx / SURFACE_CHUNK_SIZE).floor() as i32,
        (tz / SURFACE_CHUNK_SIZE).floor() as i32,
    )
}

pub fn chunk_center(cx: i32, cz: i32) -> (f32, f32) {
    (
        cx as f32 * SURFACE_CHUNK_SIZE + SURFACE_CHUNK_SIZE * 0.5,
        cz as f32 * SURFACE_CHUNK_SIZE + SURFACE_CHUNK_SIZE * 0.5,
    )
}

fn chunk_seed(world_seed: u32, body: CelestialBody, cx: i32, cz: i32) -> u32 {
    let b = body as u32;
    world_seed
        ^ b.wrapping_mul(0x9e37_79b9)
        ^ (cx as u32).wrapping_mul(0x85eb_ca6b)
        ^ (cz as u32).wrapping_mul(0xc2b2_ae35)
}

/// Decorative props for one chunk — deterministic from seed + chunk coords.
pub fn generate_chunk_flora(
    body: CelestialBody,
    world_seed: u32,
    cx: i32,
    cz: i32,
) -> Vec<FloraProp> {
    let mut rng = Rng::new(chunk_seed(world_seed, body, cx, cz) ^ 0xf10a_a);
    let x0 = cx as f32 * SURFACE_CHUNK_SIZE;
    let z0 = cz as f32 * SURFACE_CHUNK_SIZE;
    let grid = flora_grid_step();
    let steps = (SURFACE_CHUNK_SIZE / grid).ceil() as i32;
    let mut out = Vec::new();

    for iz in 0..steps {
        for ix in 0..steps {
            let jx = (rng.next() - 0.5) * grid * 0.5;
            let jz = (rng.next() - 0.5) * grid * 0.5;
            let tx = x0 + ix as f32 * grid + jx;
            let tz = z0 + iz as f32 * grid + jz;

            if body == CelestialBody::Earth {
                let pdx = tx - LAUNCH_PAD_ZONE_X;
                let pdz = tz - LAUNCH_PAD_ZONE_Z;
                if pdx * pdx + pdz * pdz < LAUNCH_PAD_MARGIN * LAUNCH_PAD_MARGIN {
                    continue;
                }
            }

            let biome = surface_biome_at(body, world_seed, tx, tz);
            if biome == SurfaceBiome::Beach && rng.next() > 0.35 {
                continue;
            }
            if rng.next() > flora_density(biome) {
                continue;
            }

            let kind = pick_flora_asset(&mut rng, biome);
            out.push(FloraProp {
                tangent_x: tx,
                tangent_z: tz,
                kind,
                scale: 0.7 + rng.next() * 0.6,
                yaw: rng.next() * std::f32::consts::TAU,
            });
        }
    }
    out
}

/// 0–2 harvestable nodes per chunk — sparse, biome-weighted.
pub fn generate_chunk_deposits(
    body: CelestialBody,
    world_seed: u32,
    cx: i32,
    cz: i32,
) -> Vec<SurfaceDeposit> {
    if body != CelestialBody::Earth {
        return Vec::new();
    }
    let mut rng = Rng::new(chunk_seed(world_seed, body, cx, cz) ^ 0xde_05170);
    let count = if rng.next() < 0.55 { 1 } else if rng.next() < 0.22 { 2 } else { 0 };
    if count == 0 {
        return Vec::new();
    }

    let x0 = cx as f32 * SURFACE_CHUNK_SIZE;
    let z0 = cz as f32 * SURFACE_CHUNK_SIZE;
    let mut out = Vec::new();
    for i in 0..count {
        let tx = rng.range(x0 + 4.0, x0 + SURFACE_CHUNK_SIZE - 4.0);
        let tz = rng.range(z0 + 4.0, z0 + SURFACE_CHUNK_SIZE - 4.0);
        let pdx = tx - LAUNCH_PAD_ZONE_X;
        let pdz = tz - LAUNCH_PAD_ZONE_Z;
        if pdx * pdx + pdz * pdz < LAUNCH_PAD_MARGIN * LAUNCH_PAD_MARGIN {
            continue;
        }
        let biome = surface_biome_at(body, world_seed, tx, tz);
        let kind = pick_deposit_kind(&mut rng, biome);
        let id = deposit_id(world_seed, cx, cz, i);
        out.push(SurfaceDeposit {
            id,
            kind,
            tangent_x: tx,
            tangent_z: tz,
        });
    }
    out
}

fn deposit_id(world_seed: u32, cx: i32, cz: i32, local: u32) -> u32 {
    world_seed
        ^ (cx as u32).wrapping_mul(0x1e24_4a47)
        ^ (cz as u32).wrapping_mul(0x517c_c1b1)
        ^ local.wrapping_mul(0x6c07_8965)
}

fn pick_deposit_kind(rng: &mut Rng, biome: SurfaceBiome) -> InteractableKind {
    let roll = rng.next();
    match biome {
        SurfaceBiome::Forest | SurfaceBiome::PineWoods | SurfaceBiome::AutumnGrove | SurfaceBiome::Meadow => {
            if roll < 0.38 {
                InteractableKind::Tree
            } else if roll < 0.62 {
                InteractableKind::Rock
            } else if roll < 0.78 {
                InteractableKind::Container
            } else if roll < 0.9 {
                InteractableKind::OreIron
            } else {
                InteractableKind::GemBlue
            }
        }
        SurfaceBiome::Highland | SurfaceBiome::Alpine | SurfaceBiome::RockyShore => {
            if roll < 0.45 {
                InteractableKind::Rock
            } else if roll < 0.75 {
                InteractableKind::OreIron
            } else if roll < 0.9 {
                InteractableKind::OreGold
            } else {
                InteractableKind::GemRed
            }
        }
        SurfaceBiome::Desert | SurfaceBiome::Beach => {
            if roll < 0.5 {
                InteractableKind::Rock
            } else if roll < 0.75 {
                InteractableKind::OreGold
            } else {
                InteractableKind::Container
            }
        }
        SurfaceBiome::Wetland => {
            if roll < 0.35 {
                InteractableKind::Tree
            } else if roll < 0.6 {
                InteractableKind::Container
            } else if roll < 0.8 {
                InteractableKind::GemBlue
            } else {
                InteractableKind::Rock
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_vary_by_position() {
        let a = generate_chunk_flora(CelestialBody::Earth, 42, 0, 0);
        let b = generate_chunk_flora(CelestialBody::Earth, 42, 3, -2);
        assert!(!a.is_empty());
        assert!(!b.is_empty());
        assert_ne!(a, b);
    }
}
