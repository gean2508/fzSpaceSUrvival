//! Batched decorative flora on sphere surfaces (trees, bushes, rocks).

use std::collections::HashMap;

use bevy::prelude::*;
use ha_shared::{CelestialBody, FloraProp};

use crate::assets::AssetFactory;
use crate::materials::ToonMaterial;
use crate::surface_placement::sphere_surface_transform;

fn flora_lift(kind: &str, scale: f32) -> f32 {
    let base = if kind.starts_with("flora_tree") {
        0.05
    } else if kind.starts_with("rock_") {
        0.12
    } else if kind.starts_with("flora_grass") || kind.starts_with("flora_flower") {
        0.04
    } else {
        0.06
    };
    base * scale
}

/// Collect per-kind instance data for streamed flora — the world streamer
/// merges all loaded chunks into ONE mesh per kind (draw calls stay flat no
/// matter how many chunks are loaded).
pub fn collect_flora_batches(
    factory: &mut AssetFactory,
    meshes: &mut Assets<Mesh>,
    toon_mats: &mut Assets<ToonMaterial>,
    std_mats: &mut Assets<StandardMaterial>,
    body: CelestialBody,
    flora: &[FloraProp],
) -> Vec<crate::render::mesh_batch::ChunkBatchEntry> {
    let mut by_kind: HashMap<&'static str, crate::render::mesh_batch::ChunkBatchEntry> =
        HashMap::new();

    for prop in flora {
        let asset = factory.require(prop.kind, meshes, toon_mats, std_mats);
        let scale_hint = asset.def.hints.scale;
        let lift = flora_lift(prop.kind, prop.scale);
        let mut tf = sphere_surface_transform(body, prop.tangent_x, prop.tangent_z, lift, prop.yaw);
        tf.scale = Vec3::splat(scale_hint * prop.scale);

        by_kind
            .entry(prop.kind)
            .or_insert_with(|| crate::render::mesh_batch::ChunkBatchEntry {
                kind: prop.kind.to_string(),
                material: asset.material.clone(),
                mesh: asset.mesh.clone(),
                transforms: Vec::new(),
            })
            .transforms
            .push(tf);
    }

    by_kind.into_values().collect()
}
