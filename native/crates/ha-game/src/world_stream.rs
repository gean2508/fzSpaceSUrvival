//! Stream flora + deposit chunks around the player (planet-wide exploration).
//!
//! Interact markers spawn per chunk (cheap, no mesh). Visible geometry is
//! re-merged into ONE mesh per asset kind across all loaded chunks whenever
//! the loaded set changes — draw calls stay ~#kinds instead of #chunks×#kinds.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use bevy::render::view::VisibilityRange;
use ha_shared::{
    chunk_key_at, generate_chunk_deposits, generate_chunk_flora, CelestialBody,
    surface_chunk_load_radius,
};

use crate::assets::AssetFactory;
use crate::deposit_spawn::collect_deposit_batches;
use crate::earth_world::{EarthDecor, StreamedChunkMark};
use crate::flora_spawn::collect_flora_batches;
use crate::materials::ToonMaterial;
use crate::render::mesh_batch::{build_merged_instances, ChunkBatchEntry};
use crate::scene::{RegionId, RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, SimState, WorldLayout};
use crate::surface_patch::active_landable_body;

pub struct WorldStreamPlugin;

impl Plugin for WorldStreamPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldStreamState>()
            .add_systems(Update, stream_earth_chunks);
    }
}

/// Chunks per merge group side — 3×3 chunks (144 m) per merged mesh keeps
/// draw calls low while Bevy's per-mesh frustum culling still skips groups
/// behind the camera (a single global mesh defeated culling on vertex-bound
/// GPUs).
const BATCH_GROUP: i32 = 3;

/// Frame budget: crossing a chunk row used to generate the whole new row (5
/// chunks on Android) AND re-merge every dirty 3×3 group in a single frame —
/// the 12→8 fps hitch every 48 m on the Redmi. Chunks come from a near-first
/// queue; groups re-merge one per frame (decor appears a frame late at the
/// stream edge, which is beyond the 40 m horizon of the 480 m sphere anyway).
const MAX_CHUNKS_SPAWN_PER_FRAME: usize = 3;
const MAX_GROUP_REBATCH_PER_FRAME: usize = 1;
/// Measured on the Redmi (v1.0.126): ONE 3×3 group with ~300 GLB tree props
/// merged in a single frame = `chk` span ~30 ms → 4 fps dip. Splitting the
/// group merge per kind under a prop budget spreads it over ~5 frames.
/// 80 (not 120): the hitch moved from CPU merge to the GPU buffer upload of
/// the freshly merged mesh — smaller slices, smaller uploads.
const MAX_MERGE_PROPS_PER_FRAME: usize = 80;
/// Flora LOD render distance (metres). VisibilityRange culls a whole 3×3 group
/// by its AABB-CENTRE distance, so the far bound must clear the group's own
/// ~100 m half-diagonal PLUS the loaded ring — otherwise a group's near trees
/// vanish right in front of you as its centre falls past the bound. At ~290 m
/// nothing culls on the ground (loaded ring is ~120 m); it only kicks in when
/// you fly well above the surface, keeping flight cheap.
const FLORA_LOD_FADE: f32 = 250.0;
const FLORA_LOD_FAR: f32 = 300.0;

fn group_key(cx: i32, cz: i32) -> (i32, i32) {
    (cx.div_euclid(BATCH_GROUP), cz.div_euclid(BATCH_GROUP))
}

/// One kind-level merge waiting its turn — split off a dirty group so a
/// single frame never fuses a whole forest.
struct MergeJob {
    gx: i32,
    gz: i32,
    kind: String,
    material: crate::assets::WorldMaterial,
    mesh: Handle<Mesh>,
    transforms: Vec<Transform>,
}

#[derive(Resource, Default)]
pub struct WorldStreamState {
    pub loaded: HashSet<(i32, i32)>,
    chunk_batches: HashMap<(i32, i32), Vec<ChunkBatchEntry>>,
    dirty_groups: HashSet<(i32, i32)>,
    /// Missing chunks queued far→near; `pop()` streams the nearest first.
    pending_chunks: Vec<(i32, i32)>,
    pending_merges: Vec<MergeJob>,
    last_center: Option<(i32, i32)>,
}

/// One merged mesh per asset kind per 3×3 chunk group.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
struct StreamedKindBatch {
    gx: i32,
    gz: i32,
}

fn stream_earth_chunks(
    region: Res<RegionRuntime>,
    world_ctx: Res<WorldContextRuntime>,
    flight: Res<crate::rocket_flight::RocketFlight>,
    planet: Res<crate::planet_world::ActivePlanet>,
    world: Res<WorldLayout>,
    player: Query<&SimState, With<Player>>,
    mut state: ResMut<WorldStreamState>,
    mut commands: Commands,
    chunks: Query<(Entity, &StreamedChunkMark)>,
    batches: Query<(Entity, &StreamedKindBatch)>,
    mut factory: ResMut<AssetFactory>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut toon_mats: ResMut<Assets<ToonMaterial>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
    mut harvest: ResMut<crate::harvest::FloraHarvest>,
    mut span: ResMut<crate::debug::CpuSpanStats>,
) {
    // `chk=` in ha-perf / `chunk=` in ha-span — this system owns the slot.
    let _guard = crate::debug::span_guard(&mut span, crate::debug::CpuSpanSlot::Chunk);
    let Some(body) = active_landable_body(&region, &world_ctx, &flight, &planet) else {
        return;
    };
    if body != CelestialBody::Earth {
        return;
    }
    if !matches!(region.active, RegionId::Overworld { .. }) {
        return;
    }
    let Ok(sim) = player.get_single() else {
        return;
    };

    // A harvested tree queues its chunk here — regenerate it (skipping the
    // removed prop) through the normal budgeted path. Despawn its old markers
    // first or the deposit markers would double.
    if !harvest.pending_reload.is_empty() {
        let reloads: Vec<(i32, i32)> = harvest.pending_reload.drain(..).collect();
        for (cx, cz) in reloads {
            if !state.loaded.contains(&(cx, cz)) {
                continue;
            }
            state.loaded.remove(&(cx, cz));
            state.chunk_batches.remove(&(cx, cz));
            state.dirty_groups.insert(group_key(cx, cz));
            for (entity, mark) in &chunks {
                if mark.cx == cx && mark.cz == cz {
                    commands.entity(entity).despawn_recursive();
                }
            }
            if !state.pending_chunks.contains(&(cx, cz)) {
                state.pending_chunks.push((cx, cz));
            }
        }
    }

    let (pcx, pcz) = chunk_key_at(sim.body.x, sim.body.z);

    // Recompute needed/unload/pending only when the player crosses a chunk
    // boundary — the per-frame HashSet+Vec churn on the idle path violated B9.
    if state.last_center != Some((pcx, pcz)) {
        state.last_center = Some((pcx, pcz));
        let radius = surface_chunk_load_radius();
        let side = (radius * 2 + 1) as usize;
        let mut needed = HashSet::with_capacity(side * side);
        for dz in -radius..=radius {
            for dx in -radius..=radius {
                needed.insert((pcx + dx, pcz + dz));
            }
        }

        // B6: release what left the ring before anything new spawns.
        let mut to_unload: Vec<(i32, i32)> = state
            .loaded
            .iter()
            .copied()
            .filter(|k| !needed.contains(k))
            .collect();
        to_unload.sort();
        for (cx, cz) in to_unload {
            state.loaded.remove(&(cx, cz));
            state.chunk_batches.remove(&(cx, cz));
            harvest.clear_chunk(cx, cz);
            state.dirty_groups.insert(group_key(cx, cz));
            for (entity, mark) in &chunks {
                if mark.cx == cx && mark.cz == cz {
                    commands.entity(entity).despawn_recursive();
                }
            }
        }

        state.pending_chunks.clear();
        for &(cx, cz) in &needed {
            if !state.loaded.contains(&(cx, cz)) {
                state.pending_chunks.push((cx, cz));
            }
        }
        // Far→near so pop() streams the chunk closest to the player first.
        state.pending_chunks.sort_by_key(|&(cx, cz)| {
            let dx = cx - pcx;
            let dz = cz - pcz;
            std::cmp::Reverse(dx * dx + dz * dz)
        });
    }

    if state.pending_chunks.is_empty()
        && state.dirty_groups.is_empty()
        && state.pending_merges.is_empty()
    {
        return;
    }

    let seed = world.0.seed;
    for _ in 0..MAX_CHUNKS_SPAWN_PER_FRAME {
        let Some((cx, cz)) = state.pending_chunks.pop() else {
            break;
        };
        if state.loaded.contains(&(cx, cz)) {
            continue;
        }
        state.loaded.insert((cx, cz));
        state.dirty_groups.insert(group_key(cx, cz));

        let flora = generate_chunk_flora(body, seed, cx, cz);
        let deposits = generate_chunk_deposits(body, seed, cx, cz);
        let mark = StreamedChunkMark { cx, cz };

        // Skip harvested props (chopped trees) and record the surviving trees
        // as targetable instances so the look ray can aim at them.
        let mut kept = Vec::with_capacity(flora.len());
        let mut trees = Vec::new();
        for (i, prop) in flora.iter().enumerate() {
            let id = i as u32;
            if harvest.is_removed(body, cx, cz, id) {
                continue;
            }
            kept.push(*prop);
            if prop.kind.starts_with("flora_tree") {
                let tf = crate::surface_placement::sphere_surface_transform(
                    body,
                    prop.tangent_x,
                    prop.tangent_z,
                    0.0,
                    prop.yaw,
                );
                let up = (tf.rotation * Vec3::Y).normalize_or_zero();
                trees.push(crate::harvest::TreeInstance {
                    cx,
                    cz,
                    id,
                    world: tf.translation + up * 1.0,
                });
            }
        }
        harvest.set_chunk_trees(cx, cz, trees);

        let mut entries = collect_flora_batches(
            &mut factory,
            &mut meshes,
            &mut toon_mats,
            &mut std_mats,
            body,
            &kept,
        );
        entries.extend(collect_deposit_batches(
            &mut commands,
            &mut factory,
            &mut meshes,
            &mut toon_mats,
            &mut std_mats,
            body,
            &deposits,
            EarthDecor,
            Some(mark),
        ));
        state.chunk_batches.insert((cx, cz), entries);
    }

    // Split at most one dirty 3×3 group per frame into per-kind merge jobs.
    // A group touched by chunks still in the queue just goes dirty again —
    // bounded work either way.
    for _ in 0..MAX_GROUP_REBATCH_PER_FRAME {
        let Some(&(gx, gz)) = state.dirty_groups.iter().min() else {
            break;
        };
        state.dirty_groups.remove(&(gx, gz));

        // Stale jobs from an earlier dirtying of this group would double-spawn.
        state.pending_merges.retain(|j| (j.gx, j.gz) != (gx, gz));
        for (entity, gb) in &batches {
            if (gb.gx, gb.gz) == (gx, gz) {
                commands.entity(entity).despawn_recursive();
            }
        }

        struct Merged {
            material: crate::assets::WorldMaterial,
            mesh: Handle<Mesh>,
            transforms: Vec<Transform>,
        }
        let mut by_kind: HashMap<&str, Merged> = HashMap::new();
        for (&(cx, cz), entries) in &state.chunk_batches {
            if group_key(cx, cz) != (gx, gz) {
                continue;
            }
            for e in entries {
                by_kind
                    .entry(e.kind.as_str())
                    .or_insert_with(|| Merged {
                        material: e.material.clone(),
                        mesh: e.mesh.clone(),
                        transforms: Vec::with_capacity(e.transforms.len() * 4),
                    })
                    .transforms
                    .extend(e.transforms.iter().copied());
            }
        }
        // A single kind can still hold a whole forest (150+ trees) — slice it
        // so no atomic job exceeds the per-frame prop budget. Extra slices
        // cost one draw call each until the group re-merges on next dirty.
        let mut jobs: Vec<MergeJob> = Vec::new();
        for (kind, m) in by_kind {
            if m.transforms.is_empty() {
                continue;
            }
            for slice in m.transforms.chunks(MAX_MERGE_PROPS_PER_FRAME) {
                jobs.push(MergeJob {
                    gx,
                    gz,
                    kind: kind.to_string(),
                    material: m.material.clone(),
                    mesh: m.mesh.clone(),
                    transforms: slice.to_vec(),
                });
            }
        }
        state.pending_merges.extend(jobs);
    }

    // Drain merge jobs under a prop budget (≥1 job/frame so nothing stalls).
    // Merge straight from the borrowed base mesh — a full-mesh clone here
    // used to duplicate every attribute buffer first.
    let mut props_done = 0usize;
    let mut spawned = 0usize;
    while let Some(job) = state.pending_merges.pop() {
        let Some(base_mesh) = meshes.get(&job.mesh) else {
            continue;
        };
        let count = job.transforms.len();
        let mesh = build_merged_instances(base_mesh, &job.transforms);
        let handle = meshes.add(mesh);
        let mut entity = commands.spawn((
            Mesh3d(handle),
            Transform::IDENTITY,
            EarthDecor,
            StreamedKindBatch {
                gx: job.gx,
                gz: job.gz,
            },
            Visibility::Visible,
            // LOD: a flora batch stops rendering its full 3D mesh past this
            // distance (crossfaded into the fog). On the 480 m sphere the eye
            // horizon is ~40 m, so nothing useful is lost on the ground — and
            // when you FLY UP the whole forest culls, keeping flight cheap on
            // any GPU. `use_aabb` measures from the group's world centre.
            VisibilityRange {
                start_margin: 0.0..0.0,
                end_margin: FLORA_LOD_FADE..FLORA_LOD_FAR,
                use_aabb: true,
            },
            Name::new(format!("StreamBatch_{}_{}_{}", job.kind, job.gx, job.gz)),
        ));
        job.material.insert_on(&mut entity);
        props_done += count;
        spawned += 1;
        if props_done >= MAX_MERGE_PROPS_PER_FRAME {
            break;
        }
    }
    if spawned > 0 {
        eprintln!(
            "ha-stream: merge +{spawned} kind batches ({props_done} props), {} jobs / {} groups pending",
            state.pending_merges.len(),
            state.dirty_groups.len()
        );
    }
}
