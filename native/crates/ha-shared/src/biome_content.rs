//! Biome → asset pools — uses the full flora / rock catalog with weighted picks.

use crate::rng::Rng;
use crate::surface_biome::SurfaceBiome;

#[derive(Clone, Copy)]
struct WeightedKind {
    kind: &'static str,
    weight: f32,
}

fn pick_weighted(rng: &mut Rng, table: &[WeightedKind]) -> &'static str {
    let total: f32 = table.iter().map(|e| e.weight).sum();
    let mut roll = rng.next() * total;
    for entry in table {
        roll -= entry.weight;
        if roll <= 0.0 {
            return entry.kind;
        }
    }
    table.last().map(|e| e.kind).unwrap_or("flora_grass_tuft")
}

/// Decorative flora kind for a biome (uses Blender / procedural catalog kinds).
pub fn pick_flora_asset(rng: &mut Rng, biome: SurfaceBiome) -> &'static str {
    let table = match biome {
        SurfaceBiome::Forest => &[
            WeightedKind { kind: "flora_tree_oak", weight: 2.2 },
            WeightedKind { kind: "flora_tree_pine", weight: 1.8 },
            WeightedKind { kind: "flora_bush", weight: 1.4 },
            WeightedKind { kind: "flora_shrub", weight: 1.0 },
            WeightedKind { kind: "flora_mushroom_brown", weight: 0.5 },
            WeightedKind { kind: "flora_grass_tuft", weight: 0.6 },
        ][..],
        SurfaceBiome::PineWoods => &[
            WeightedKind { kind: "flora_tree_pine", weight: 3.5 },
            WeightedKind { kind: "flora_tree_birch", weight: 0.8 },
            WeightedKind { kind: "flora_shrub", weight: 1.0 },
            WeightedKind { kind: "rock_small", weight: 0.6 },
        ][..],
        SurfaceBiome::AutumnGrove => &[
            WeightedKind { kind: "flora_tree_oak_autumn", weight: 2.5 },
            WeightedKind { kind: "flora_tree_red", weight: 1.8 },
            WeightedKind { kind: "flora_tree_cherry", weight: 1.2 },
            WeightedKind { kind: "flora_bush", weight: 1.0 },
            WeightedKind { kind: "flora_flower_red", weight: 0.7 },
        ][..],
        SurfaceBiome::Meadow => &[
            WeightedKind { kind: "flora_grass_tuft", weight: 2.5 },
            WeightedKind { kind: "flora_flower_yellow", weight: 1.2 },
            WeightedKind { kind: "flora_flower_white", weight: 0.8 },
            WeightedKind { kind: "flora_shrub", weight: 1.0 },
            WeightedKind { kind: "flora_tree_oak", weight: 0.5 },
            WeightedKind { kind: "flora_flower_blue", weight: 0.4 },
        ][..],
        SurfaceBiome::Wetland => &[
            WeightedKind { kind: "flora_bush", weight: 2.0 },
            WeightedKind { kind: "flora_mushroom_red", weight: 1.2 },
            WeightedKind { kind: "flora_mushroom_brown", weight: 0.8 },
            WeightedKind { kind: "flora_grass_tuft", weight: 1.0 },
        ][..],
        SurfaceBiome::Highland => &[
            WeightedKind { kind: "rock_medium", weight: 2.0 },
            WeightedKind { kind: "rock_small", weight: 1.5 },
            WeightedKind { kind: "flora_tree_pine", weight: 1.2 },
            WeightedKind { kind: "rock_boulder", weight: 0.6 },
            WeightedKind { kind: "crystal_blue", weight: 0.3 },
        ][..],
        SurfaceBiome::Alpine => &[
            WeightedKind { kind: "rock_boulder", weight: 2.0 },
            WeightedKind { kind: "rock_medium", weight: 1.5 },
            WeightedKind { kind: "flora_tree_pine", weight: 0.8 },
            WeightedKind { kind: "rock_cave_entrance", weight: 0.15 },
            WeightedKind { kind: "crystal_yellow", weight: 0.25 },
        ][..],
        SurfaceBiome::Desert => &[
            WeightedKind { kind: "rock_small", weight: 2.0 },
            WeightedKind { kind: "rock_medium", weight: 1.2 },
            WeightedKind { kind: "crystal_yellow", weight: 0.5 },
            WeightedKind { kind: "flora_grass_tuft", weight: 0.2 },
        ][..],
        SurfaceBiome::Beach | SurfaceBiome::RockyShore => &[
            WeightedKind { kind: "rock_small", weight: 2.0 },
            WeightedKind { kind: "flora_grass_tuft", weight: 1.0 },
            WeightedKind { kind: "prop_barrel", weight: 0.15 },
            WeightedKind { kind: "prop_crate", weight: 0.12 },
        ][..],
    };
    pick_weighted(rng, table)
}

/// Spawn density 0..1 for decorative flora in this biome.
pub fn flora_density(biome: SurfaceBiome) -> f32 {
    match biome {
        SurfaceBiome::Forest | SurfaceBiome::PineWoods | SurfaceBiome::AutumnGrove => 0.88,
        SurfaceBiome::Wetland => 0.72,
        SurfaceBiome::Meadow => 0.52,
        SurfaceBiome::Highland => 0.45,
        SurfaceBiome::Alpine => 0.35,
        SurfaceBiome::Desert => 0.28,
        SurfaceBiome::Beach => 0.22,
        SurfaceBiome::RockyShore => 0.38,
    }
}
