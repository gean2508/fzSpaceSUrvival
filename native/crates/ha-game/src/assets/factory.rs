use std::collections::HashMap;

use bevy::prelude::*;
use ha_shared::{get_asset, AssetDef};

use super::builders;
use super::glb::{try_load_kind, world_material_from_glb, world_material_procedural, WorldMaterial};
use crate::materials::ToonMaterial;

/// How a catalog kind was resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetSource {
    Procedural,
    GlbFile,
}

/// Cached mesh + material for one asset kind (procedural or `.glb` override).
#[derive(Clone)]
pub struct ProceduralAsset {
    pub def: &'static AssetDef,
    pub mesh: Handle<Mesh>,
    pub material: WorldMaterial,
    pub source: AssetSource,
}

#[derive(Resource, Default)]
pub struct AssetFactory {
    cache: HashMap<&'static str, ProceduralAsset>,
}

impl AssetFactory {
    const FALLBACK_KIND: &'static str = "prop_crate";

    /// Lazy-instantiate mesh + material (built once per kind, reused everywhere).
    pub fn get(
        &mut self,
        kind: &str,
        meshes: &mut Assets<Mesh>,
        toon_mats: &mut Assets<ToonMaterial>,
        std_mats: &mut Assets<StandardMaterial>,
    ) -> Option<ProceduralAsset> {
        if let Some(hit) = self.cache.get(kind) {
            return Some(hit.clone());
        }
        let def = get_asset(kind)?;

        if let Some(payload) = try_load_kind(kind) {
            let material = world_material_from_glb(&payload, def.palette, toon_mats, std_mats);
            let mesh = meshes.add(payload.mesh);
            let entry = ProceduralAsset {
                def,
                mesh,
                material,
                source: AssetSource::GlbFile,
            };
            self.cache.insert(def.kind, entry.clone());
            return Some(entry);
        }

        let mesh = meshes.add(builders::build_mesh(def));
        let material = world_material_procedural(def.palette, toon_mats, std_mats);
        let entry = ProceduralAsset {
            def,
            mesh,
            material,
            source: AssetSource::Procedural,
        };
        self.cache.insert(def.kind, entry.clone());
        Some(entry)
    }

    pub fn require(
        &mut self,
        kind: &str,
        meshes: &mut Assets<Mesh>,
        toon_mats: &mut Assets<ToonMaterial>,
        std_mats: &mut Assets<StandardMaterial>,
    ) -> ProceduralAsset {
        if let Some(asset) = self.get(kind, meshes, toon_mats, std_mats) {
            return asset;
        }
        eprintln!(
            "ha-game: unknown asset kind '{kind}', using fallback '{}'",
            Self::FALLBACK_KIND
        );
        self.get(Self::FALLBACK_KIND, meshes, toon_mats, std_mats)
            .expect("fallback asset kind must exist")
    }

    pub fn cached_count(&self) -> usize {
        self.cache.len()
    }

    /// Drop one kind from cache (dev hot-reload after override change).
    pub fn invalidate(&mut self, kind: &str) {
        self.cache.remove(kind);
    }

    /// Dev bridge: clear cached kinds affected by override file changes.
    pub fn invalidate_all_dev(&mut self, paths: &std::collections::HashMap<String, std::path::PathBuf>) {
        for kind in paths.keys() {
            self.cache.remove(kind.as_str());
        }
    }

    #[cfg(debug_assertions)]
    pub fn invalidate_all(&mut self) {
        self.cache.clear();
    }
}

pub struct AssetFactoryPlugin;

impl Plugin for AssetFactoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AssetFactory>();
    }
}
