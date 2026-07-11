//! Lazy cache of HUD icon textures — same pattern as `AssetFactory`.

use std::collections::HashMap;

use bevy::prelude::*;
use ha_shared::{get_ui_icon, UI_ICON_CATALOG};

use super::icon_builders::rasterize_icon;

#[derive(Resource, Default)]
pub struct IconFactory {
    cache: HashMap<&'static str, Handle<Image>>,
}

impl IconFactory {
    /// Build icon texture once per kind, reuse everywhere.
    pub fn get(&mut self, kind: &str, images: &mut Assets<Image>) -> Handle<Image> {
        if let Some(hit) = self.cache.get(kind) {
            return hit.clone();
        }
        // Unknown kind must not crash the HUD — fall back to first catalog icon.
        let def = get_ui_icon(kind).unwrap_or(&UI_ICON_CATALOG[0]);
        let handle = images.add(rasterize_icon(def));
        self.cache.insert(def.kind, handle.clone());
        handle
    }

    pub fn preload_hud_atlas(&mut self, images: &mut Assets<Image>) -> HudIconHandles {
        HudIconHandles {
            heart_full: self.get("heart_full", images),
            heart_half: self.get("heart_half", images),
            heart_empty: self.get("heart_empty", images),
            drumstick_full: self.get("drumstick_full", images),
            drumstick_half: self.get("drumstick_half", images),
            drumstick_empty: self.get("drumstick_empty", images),
        }
    }

    pub fn cached_count(&self) -> usize {
        self.cache.len()
    }

    pub fn catalog_len(&self) -> usize {
        UI_ICON_CATALOG.len()
    }
}

#[derive(Clone)]
pub struct HudIconHandles {
    pub heart_full: Handle<Image>,
    pub heart_half: Handle<Image>,
    pub heart_empty: Handle<Image>,
    pub drumstick_full: Handle<Image>,
    pub drumstick_half: Handle<Image>,
    pub drumstick_empty: Handle<Image>,
}

pub struct IconFactoryPlugin;

impl Plugin for IconFactoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<IconFactory>();
    }
}
