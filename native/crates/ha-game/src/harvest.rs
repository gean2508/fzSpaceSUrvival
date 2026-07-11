//! Real flora harvesting — chopping a tree removes it from the world and
//! persists that removal per body (`harvested.json`, flush-on-pause B10).
//!
//! Flora lives fused inside the streamed 3×3 batches (no per-tree entity), so
//! this owns: (a) the targetable tree instances the world streamer records
//! per chunk, (b) the persistent removed-set the streamer skips on
//! regeneration, and (c) the ray test that turns "look at a tree" into a
//! `Tree` interaction. Harvesting is a NET geometry reduction — it helps perf.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use bevy::prelude::*;
use ha_shared::CelestialBody;
use serde::{Deserialize, Serialize};

use crate::inventory_persist::persist_dir;
use crate::platform::is_app_paused;

/// A harvestable tree the streamer exposed for aiming (mid-trunk world point).
pub struct TreeInstance {
    pub cx: i32,
    pub cz: i32,
    pub id: u32,
    pub world: Vec3,
}

/// Trunk hit radius for the look ray (generous — aiming on mobile is coarse).
const TREE_HIT_RADIUS: f32 = 1.3;

pub struct HarvestPlugin;

impl Plugin for HarvestPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FloraHarvest>()
            .add_systems(Startup, load_harvest_on_startup)
            .add_systems(Update, debounced_harvest_save);
    }
}

#[derive(Resource, Default)]
pub struct FloraHarvest {
    /// Persistent removals: body → chunk → set of prop indices.
    removed: HashMap<CelestialBody, HashMap<(i32, i32), HashSet<u32>>>,
    /// Targetable trees for the active streamed body, keyed by chunk.
    trees: HashMap<(i32, i32), Vec<TreeInstance>>,
    /// Chunks the streamer must regenerate after a harvest (drained by it).
    pub pending_reload: Vec<(i32, i32)>,
    /// Bumped on any change so targeting invalidates its camera cache.
    pub version: u64,
    dirty: bool,
    force: bool,
    was_paused: bool,
    debounce: f32,
}

impl FloraHarvest {
    pub fn is_removed(&self, body: CelestialBody, cx: i32, cz: i32, id: u32) -> bool {
        self.removed
            .get(&body)
            .and_then(|m| m.get(&(cx, cz)))
            .map(|s| s.contains(&id))
            .unwrap_or(false)
    }

    /// Streamer records the harvestable trees it just generated for a chunk.
    pub fn set_chunk_trees(&mut self, cx: i32, cz: i32, trees: Vec<TreeInstance>) {
        if trees.is_empty() {
            self.trees.remove(&(cx, cz));
        } else {
            self.trees.insert((cx, cz), trees);
        }
        self.version = self.version.wrapping_add(1);
    }

    pub fn clear_chunk(&mut self, cx: i32, cz: i32) {
        if self.trees.remove(&(cx, cz)).is_some() {
            self.version = self.version.wrapping_add(1);
        }
    }

    /// Nearest tree along the look ray within reach, or None.
    pub fn ray_hit(&self, origin: Vec3, dir: Vec3, reach: f32) -> Option<(i32, i32, u32)> {
        let mut best: Option<(f32, (i32, i32, u32))> = None;
        for trees in self.trees.values() {
            for t in trees {
                let to = t.world - origin;
                let along = to.dot(dir);
                if along < 0.2 || along > reach {
                    continue;
                }
                let lateral = (t.world - (origin + dir * along)).length();
                if lateral > TREE_HIT_RADIUS {
                    continue;
                }
                if best.map(|(d, _)| along < d).unwrap_or(true) {
                    best = Some((along, (t.cx, t.cz, t.id)));
                }
            }
        }
        best.map(|(_, cell)| cell)
    }

    /// Mark a tree gone: persist, drop it from the targetable set, and queue
    /// the chunk for regeneration so the batch re-merges without it.
    pub fn harvest(&mut self, body: CelestialBody, cx: i32, cz: i32, id: u32) {
        let newly = self
            .removed
            .entry(body)
            .or_default()
            .entry((cx, cz))
            .or_default()
            .insert(id);
        if !newly {
            return;
        }
        if let Some(trees) = self.trees.get_mut(&(cx, cz)) {
            trees.retain(|t| t.id != id);
        }
        if !self.pending_reload.contains(&(cx, cz)) {
            self.pending_reload.push((cx, cz));
        }
        self.dirty = true;
        self.force = true;
        self.version = self.version.wrapping_add(1);
    }
}

// --- Persistence (mirrors blocks.json / inventory.json pattern) ---------------

#[derive(Serialize, Deserialize, Default)]
struct HarvestSave {
    bodies: Vec<BodyHarvestSave>,
}

#[derive(Serialize, Deserialize)]
struct BodyHarvestSave {
    body: String,
    chunks: Vec<ChunkHarvestSave>,
}

#[derive(Serialize, Deserialize)]
struct ChunkHarvestSave {
    cx: i32,
    cz: i32,
    ids: Vec<u32>,
}

fn harvest_path() -> PathBuf {
    persist_dir().join("harvested.json")
}

fn load_harvest_on_startup(mut harvest: ResMut<FloraHarvest>) {
    let Ok(text) = std::fs::read_to_string(harvest_path()) else {
        return;
    };
    let Ok(save) = serde_json::from_str::<HarvestSave>(&text) else {
        eprintln!("ha-harvest: harvested.json unreadable — starting empty");
        return;
    };
    let mut total = 0usize;
    for entry in save.bodies {
        let Some(&body) = CelestialBody::all()
            .iter()
            .find(|b| ha_shared::body_def(**b).label == entry.body)
        else {
            continue;
        };
        let map = harvest.removed.entry(body).or_default();
        for chunk in entry.chunks {
            let set = map.entry((chunk.cx, chunk.cz)).or_default();
            for id in chunk.ids {
                set.insert(id);
                total += 1;
            }
        }
    }
    if total > 0 {
        eprintln!("ha-harvest: loaded {total} harvested props");
    }
}

fn save_harvest_to_disk(harvest: &FloraHarvest) {
    let save = HarvestSave {
        bodies: harvest
            .removed
            .iter()
            .map(|(body, chunks)| BodyHarvestSave {
                body: ha_shared::body_def(*body).label.to_string(),
                chunks: chunks
                    .iter()
                    .map(|(&(cx, cz), ids)| ChunkHarvestSave {
                        cx,
                        cz,
                        ids: ids.iter().copied().collect(),
                    })
                    .collect(),
            })
            .collect(),
    };
    match serde_json::to_string(&save) {
        Ok(json) => {
            if let Err(err) = std::fs::write(harvest_path(), json) {
                eprintln!("ha-harvest: save failed: {err}");
            }
        }
        Err(err) => eprintln!("ha-harvest: serialize failed: {err}"),
    }
}

fn debounced_harvest_save(time: Res<Time>, mut harvest: ResMut<FloraHarvest>) {
    let paused = is_app_paused();
    if paused && !harvest.was_paused {
        save_harvest_to_disk(&harvest);
        harvest.dirty = false;
        harvest.force = false;
    }
    harvest.was_paused = paused;

    if !harvest.dirty {
        return;
    }
    harvest.debounce += time.delta_secs();
    if harvest.force || harvest.debounce >= 3.0 {
        save_harvest_to_disk(&harvest);
        harvest.dirty = false;
        harvest.force = false;
        harvest.debounce = 0.0;
    }
}
