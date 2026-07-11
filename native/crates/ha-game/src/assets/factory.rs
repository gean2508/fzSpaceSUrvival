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

/// GLB baked texture waiting for `Assets<Image>` — bound one frame after the
/// material is created so `require()` keeps its signature (no `Assets<Image>`
/// threading through every call site).
pub struct PendingGlbTexture {
    pub material: Handle<StandardMaterial>,
    pub image: bevy::image::Image,
}

#[derive(Resource, Default)]
pub struct AssetFactory {
    cache: HashMap<&'static str, ProceduralAsset>,
    pending_textures: Vec<PendingGlbTexture>,
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
            if let (Some(image), WorldMaterial::Standard(handle)) =
                (payload.base_color_image, &material)
            {
                self.pending_textures.push(PendingGlbTexture {
                    material: handle.clone(),
                    image,
                });
            }
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
        app.init_resource::<AssetFactory>()
            .add_systems(PostStartup, preload_catalog_assets)
            .add_systems(Update, apply_pending_glb_textures);
    }
}

/// Warm the whole catalog at boot — lazy `require` mid-walk paid a GLB parse
/// + PNG texture decode inside a gameplay frame the first time each flora
/// kind streamed in (visible hitch on the Redmi).
fn preload_catalog_assets(
    mut factory: ResMut<AssetFactory>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut toon_mats: ResMut<Assets<ToonMaterial>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
) {
    let t0 = std::time::Instant::now();
    for def in ha_shared::ASSET_CATALOG {
        let _ = factory.get(def.kind, &mut meshes, &mut toon_mats, &mut std_mats);
    }
    eprintln!(
        "ha-assets: preloaded {} catalog kinds in {:.0?}",
        factory.cached_count(),
        t0.elapsed()
    );
}

/// Bind decoded GLB bake textures to their (kind-shared) materials.
fn apply_pending_glb_textures(
    mut factory: ResMut<AssetFactory>,
    mut images: ResMut<Assets<Image>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
) {
    if factory.pending_textures.is_empty() {
        return;
    }
    for pending in factory.pending_textures.drain(..) {
        let handle = images.add(pending.image);
        if let Some(mat) = std_mats.get_mut(&pending.material) {
            mat.base_color_texture = Some(handle);
        }
    }
}
