//! Lazy mesh cache + procedural builders — optional `.glb` drop-in overrides.

mod builders;
mod factory;
mod glb;

pub use factory::{AssetFactory, AssetFactoryPlugin, AssetSource, ProceduralAsset};
pub use glb::WorldMaterial;
pub use builders::build_demo_ship_mesh;
