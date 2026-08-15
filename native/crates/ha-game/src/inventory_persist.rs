//! Disk persistence for player inventory (`inventory.json` beside `ui-layout.cfg`).

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ha_shared::PlayerInventory as PlayerInventoryInner;
use serde::{Deserialize, Serialize};

use crate::platform::is_app_paused;

pub struct InventoryPersistPlugin;

impl Plugin for InventoryPersistPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerInventory>()
            .init_resource::<InventoryPersistState>()
            .add_systems(Startup, load_inventory_on_startup)
            .add_systems(Update, debounced_inventory_save);
    }
}

/// Bevy resource wrapper around shared deterministic inventory state.
#[derive(Resource, Clone, Debug, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerInventory(pub PlayerInventoryInner);

impl Deref for PlayerInventory {
    type Target = PlayerInventoryInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for PlayerInventory {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Resource)]
pub struct InventoryPersistState {
    pub dirty: bool,
    pub force_next: bool,
    debounce: Timer,
    was_paused: bool,
}

impl Default for InventoryPersistState {
    fn default() -> Self {
        Self {
            dirty: false,
            force_next: false,
            debounce: Timer::from_seconds(3.0, TimerMode::Repeating),
            was_paused: false,
        }
    }
}

impl InventoryPersistState {
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn mark_immediate(&mut self) {
        self.dirty = true;
        self.force_next = true;
    }
}

fn load_inventory_on_startup(
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
) {
    match load_inventory_from_disk() {
        Some(loaded) => inv.0 = loaded,
        None => inv.0 = PlayerInventoryInner::starter(),
    }
    inv.0.ensure_agency_demo_kit();
    persist.dirty = true;
    persist.force_next = true;
}

fn debounced_inventory_save(
    time: Res<Time>,
    inv: Res<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
) {
    let paused = is_app_paused();
    // Always flush on pause transition — Android may kill the process before debounce fires.
    if paused && !persist.was_paused {
        save_inventory_to_disk(&inv.0);
        persist.dirty = false;
        persist.force_next = false;
    }
    persist.was_paused = paused;

    if !persist.dirty {
        return;
    }
    persist.debounce.tick(time.delta());
    if persist.force_next || persist.debounce.just_finished() {
        save_inventory_to_disk(&inv.0);
        persist.dirty = false;
        persist.force_next = false;
    }
}

pub fn persist_dir() -> PathBuf {
    if let Ok(path) = crate::flags::var("HA_UI_LAYOUT_FILE") {
        let p = PathBuf::from(path);
        if let Some(parent) = p.parent() {
            if parent != Path::new(".") && parent != Path::new("") {
                return parent.to_path_buf();
            }
        }
    }
    #[cfg(target_os = "android")]
    {
        return PathBuf::from("/data/data/com.hungerarena.app/files");
    }
    #[cfg(not(target_os = "android"))]
    {
        PathBuf::from(".")
    }
}

pub fn inventory_path() -> PathBuf {
    persist_dir().join("inventory.json")
}

pub fn load_inventory_from_disk() -> Option<PlayerInventoryInner> {
    let path = inventory_path();
    let text = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save_inventory_to_disk(inv: &PlayerInventoryInner) {
    let path = inventory_path();
    if let Some(parent) = path.parent() {
        if parent != Path::new(".") {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    match serde_json::to_string_pretty(inv) {
        Ok(body) => {
            if let Err(err) = std::fs::write(&path, body) {
                eprintln!("inventory save failed ({}): {err}", path.display());
            }
        }
        Err(err) => eprintln!("inventory serialize failed: {err}"),
    }
}
