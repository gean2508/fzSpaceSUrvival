//! Local machine sim slice — mirrors `packages/shared/src/sim/machines.ts`.
//!
//! Machines are per-BODY (Tierra, Luna, planetas): placement resolves the
//! active walkable body, visuals respawn via `sync_machine_visuals` when you
//! return (celestial_cleanup despawns them on leave, B6), and `machines.json`
//! persists the whole world (flush-on-pause, B10). Off-body machines bank
//! their output into `stored_output`; it lands in the inventory when you're
//! back on that body — the antenna's remote monitor shows the stock meanwhile.

use std::collections::{HashMap, HashSet};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use ha_shared::{body_def, require_machine, ANTENNA, LAUNCH_PAD, REGOLITH_DRILL, WOOD_EXTRACTOR};
use serde::{Deserialize, Serialize};

use crate::game_time::fixed_delta_secs;
use crate::launch_pad::{in_launch_pad_zone, spawn_launch_pad, zone_has_launch_pad, LaunchPadWorld};

use crate::assets::AssetFactory;
use crate::camera::look_forward_flat;
use crate::input::{CameraLookIntent, EquippedItem, HoldInteractState};
use crate::inventory_persist::{persist_dir, InventoryPersistState, PlayerInventory};
use crate::materials::ToonMaterial;
use crate::planet_world::ActivePlanet;
use crate::platform::is_app_paused;
use crate::render::particles::spawn_harvest_particles;
use crate::rocket_flight::RocketFlight;
use crate::scene::{RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, SimState};
use crate::surface_patch::active_landable_body;
use crate::surface_placement::{sphere_surface_transform, sphere_world_pos, MACHINE_SURFACE_LIFT};
use ha_shared::CelestialBody;

pub struct MachinesPlugin;

impl Plugin for MachinesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MachineWorld>()
            .init_resource::<PendingMachinePlacement>()
            .add_systems(Startup, load_machines_on_startup)
            .add_systems(FixedUpdate, tick_machines)
            .add_systems(
                Update,
                (try_place_on_hold, sync_machine_visuals, debounced_machines_save),
            );
    }
}

#[derive(Resource)]
pub struct PendingMachinePlacement {
    pub machine_id: &'static str,
}

impl Default for PendingMachinePlacement {
    fn default() -> Self {
        Self {
            machine_id: WOOD_EXTRACTOR.id,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LocalMachine {
    pub uid: u32,
    pub machine_id: &'static str,
    pub body: CelestialBody,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub output_accumulator: f32,
    pub stored_output: u32,
    pub producing_flash: f32,
}

#[derive(Resource, Default)]
pub struct MachineWorld {
    pub next_uid: u32,
    pub placed: Vec<LocalMachine>,
    dirty: bool,
    was_paused: bool,
    debounce: f32,
}

impl MachineWorld {
    /// Machines placed on `body` (camp query for O₂ tether / monitor).
    pub fn on_body(&self, body: CelestialBody) -> impl Iterator<Item = &LocalMachine> {
        self.placed.iter().filter(move |m| m.body == body)
    }

    pub fn has_antenna_on(&self, body: CelestialBody) -> bool {
        self.on_body(body).any(|m| m.machine_id == ANTENNA.id)
    }
}

#[derive(Component)]
pub struct MachineEntity {
    pub uid: u32,
}

/// Active-body context — bundled to stay under Bevy's 16-param limit.
#[derive(SystemParam)]
pub struct PlacementCtx<'w> {
    region: Res<'w, RegionRuntime>,
    world_ctx: Res<'w, WorldContextRuntime>,
    flight: Res<'w, RocketFlight>,
    planet: Res<'w, ActivePlanet>,
}

impl PlacementCtx<'_> {
    fn body(&self) -> Option<CelestialBody> {
        active_landable_body(&self.region, &self.world_ctx, &self.flight, &self.planet)
    }
}

fn ground_y_on(body: CelestialBody, tx: f32, tz: f32) -> f32 {
    if body == CelestialBody::Earth {
        ha_shared::ground_height_at(tx, tz)
    } else {
        ha_shared::planet_ground_y_at(body, tx, tz)
    }
}

/// Machine ↔ body gates: extractors need what they extract to exist there.
fn machine_allowed_on(machine_id: &str, body: CelestialBody) -> bool {
    let airless = body_def(body).atmo_top_m <= 0.0;
    match machine_id {
        // Trees/launch zone are Earth content.
        id if id == WOOD_EXTRACTOR.id || id == LAUNCH_PAD.id => body == CelestialBody::Earth,
        // Regolith + uplink are the off-world camp kit.
        id if id == REGOLITH_DRILL.id || id == ANTENNA.id => airless,
        _ => true,
    }
}

pub(crate) fn try_place_on_hold(
    mut hold: ResMut<HoldInteractState>,
    equipped: Res<EquippedItem>,
    look: Res<CameraLookIntent>,
    pending: Res<PendingMachinePlacement>,
    ctx: PlacementCtx,
    player: Query<(&Transform, &SimState), With<Player>>,
    mut world: ResMut<MachineWorld>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
    mut commands: Commands,
    pool: Res<crate::render::particles::HarvestParticlePool>,
    mut factory: ResMut<AssetFactory>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ToonMaterial>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
    mut launch_pads: ResMut<LaunchPadWorld>,
) {
    // Consume only holds started as machine placement (cf. building_world.rs guard).
    if !hold.completed || !hold.placing_machine || hold.flight_hold {
        return;
    }
    hold.completed = false;
    hold.placing_machine = false;
    hold.placing_stone = false;

    if equipped.item_id != "hammer" {
        return;
    }
    let Some(body) = ctx.body() else {
        return;
    };
    let machine_id = pending.machine_id;
    let def = require_machine(machine_id);
    if !machine_allowed_on(machine_id, body) {
        eprintln!("ha-machines: {} not placeable on {}", def.id, body_def(body).label);
        return;
    }
    if !inv.can_afford_machine(def) {
        return;
    }
    let Ok((_tf, sim)) = player.get_single() else {
        return;
    };
    let fwd = look_forward_flat(&look);
    let tx = sim.body.x + fwd.x * 2.2;
    let tz = sim.body.z + fwd.y * 2.2;
    let y = ground_y_on(body, tx, tz);

    if machine_id == LAUNCH_PAD.id && !in_launch_pad_zone(tx, tz) {
        eprintln!("ha-launch: launch pad must be placed in the reserved zone");
        return;
    }
    if machine_id == LAUNCH_PAD.id && zone_has_launch_pad(&launch_pads) {
        eprintln!("ha-launch: spaceport already has a launch pad");
        return;
    }

    inv.consume_machine_cost(def);
    persist.mark_immediate();
    let uid = world.next_uid;
    world.next_uid += 1;
    world.placed.push(LocalMachine {
        uid,
        machine_id: def.id,
        body,
        x: tx,
        y,
        z: tz,
        output_accumulator: 0.0,
        stored_output: 0,
        producing_flash: 0.0,
    });
    world.dirty = true;

    if machine_id == LAUNCH_PAD.id {
        spawn_launch_pad(
            &mut commands,
            &mut factory,
            &mut meshes,
            &mut materials,
            &mut std_mats,
            uid,
            tx,
            y,
            tz,
            &mut launch_pads,
        );
    } else {
        spawn_machine_visual(
            &mut commands,
            &mut factory,
            &mut meshes,
            &mut materials,
            &mut std_mats,
            def.asset_kind,
            body,
            uid,
            tx,
            tz,
            look.yaw,
        );
    }

    let particle_pos = sphere_world_pos(body, tx, tz, MACHINE_SURFACE_LIFT + 0.15);
    spawn_harvest_particles(
        &mut commands,
        &pool,
        particle_pos,
        uid as usize,
    );
}

#[allow(clippy::too_many_arguments)]
fn spawn_machine_visual(
    commands: &mut Commands,
    factory: &mut AssetFactory,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ToonMaterial>,
    std_mats: &mut Assets<StandardMaterial>,
    asset_kind: &str,
    body: CelestialBody,
    uid: u32,
    tx: f32,
    tz: f32,
    yaw: f32,
) {
    let asset = factory.require(asset_kind, meshes, materials, std_mats);
    let place_tf = sphere_surface_transform(body, tx, tz, MACHINE_SURFACE_LIFT, yaw);
    let mut entity = commands.spawn((
        Mesh3d(asset.mesh.clone()),
        place_tf,
        MachineEntity { uid },
        Name::new(format!("Machine_{uid}")),
    ));
    // Per-body decor marker so celestial_cleanup despawns on leave (B6);
    // `sync_machine_visuals` respawns from `MachineWorld` on return.
    match body {
        CelestialBody::Earth => {
            entity.insert(crate::earth_world::EarthDecor);
        }
        CelestialBody::Moon => {
            entity.insert(crate::moon_world::MoonDecor);
        }
        _ => {
            entity.insert(crate::planet_world::PlanetDecor);
        }
    }
    asset.material.insert_on(&mut entity);
}

/// Respawn visuals for persisted machines on the active body (few machines —
/// the uid scan is cheap; launch pads own their visuals via `LaunchPadWorld`).
fn sync_machine_visuals(
    ctx: PlacementCtx,
    world: Res<MachineWorld>,
    existing: Query<&MachineEntity>,
    mut commands: Commands,
    mut factory: ResMut<AssetFactory>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ToonMaterial>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
) {
    let Some(body) = ctx.body() else {
        return;
    };
    let spawned: HashSet<u32> = existing.iter().map(|e| e.uid).collect();
    for m in world.on_body(body) {
        if m.machine_id == LAUNCH_PAD.id || spawned.contains(&m.uid) {
            continue;
        }
        let def = require_machine(m.machine_id);
        spawn_machine_visual(
            &mut commands,
            &mut factory,
            &mut meshes,
            &mut materials,
            &mut std_mats,
            def.asset_kind,
            body,
            m.uid,
            m.x,
            m.z,
            0.0,
        );
    }
}

fn tick_machines(
    time: Res<Time<Fixed>>,
    ctx: PlacementCtx,
    mut world: ResMut<MachineWorld>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
    mut materials: ResMut<Assets<ToonMaterial>>,
    mut meshes: Query<(&MachineEntity, &MeshMaterial3d<ToonMaterial>)>,
) {
    let dt = fixed_delta_secs(&time);
    let player_body = ctx.body();
    let mut any_flash = false;
    let mut produced = false;
    for m in &mut world.placed {
        let def = require_machine(m.machine_id);
        if !def.passive {
            continue;
        }
        let local = player_body == Some(m.body);
        m.output_accumulator += def.output_per_second * dt;
        let whole = m.output_accumulator.floor() as u32;
        if whole > 0 {
            m.output_accumulator -= whole as f32;
            if local {
                inv.add(def.output_item_id, whole);
            } else {
                // Off-body camp banks its output — collected on your next visit;
                // visible from Earth through the antenna monitor.
                m.stored_output += whole;
            }
            produced = true;
            m.producing_flash = 1.0;
        }
        if local && m.stored_output > 0 {
            // Back at the camp — collect what it banked while you were away.
            inv.add(def.output_item_id, m.stored_output);
            m.stored_output = 0;
            produced = true;
        }
        if m.producing_flash > 0.0 {
            m.producing_flash = (m.producing_flash - dt * 0.8).max(0.0);
            any_flash = true;
        }
    }

    if produced {
        persist.mark_dirty();
        world.dirty = true;
    }

    if !any_flash {
        return;
    }

    let by_uid: HashMap<u32, &LocalMachine> = world.placed.iter().map(|m| (m.uid, m)).collect();
    for (entity, mat_handle) in &mut meshes {
        let Some(m) = by_uid.get(&entity.uid) else {
            continue;
        };
        if m.producing_flash <= 0.0 {
            continue;
        }
        if let Some(mat) = materials.get_mut(&mat_handle.0) {
            mat.emissive_glow = Vec4::new(0.35, 1.1, 0.45, 1.0) * m.producing_flash;
        }
    }
}

// --- Persistence (machines.json, flush-on-pause — B10) --------------------------

#[derive(Serialize, Deserialize)]
struct MachinesSave {
    next_uid: u32,
    placed: Vec<MachineSave>,
}

#[derive(Serialize, Deserialize)]
struct MachineSave {
    uid: u32,
    machine_id: String,
    body: String,
    x: f32,
    y: f32,
    z: f32,
    stored_output: u32,
}

fn machines_path() -> std::path::PathBuf {
    persist_dir().join("machines.json")
}

fn load_machines_on_startup(mut world: ResMut<MachineWorld>) {
    let Ok(text) = std::fs::read_to_string(machines_path()) else {
        return;
    };
    let Ok(save) = serde_json::from_str::<MachinesSave>(&text) else {
        eprintln!("ha-machines: machines.json unreadable — starting empty");
        return;
    };
    world.next_uid = save.next_uid;
    for m in save.placed {
        // Launch pads respawn through their own demo/pad flow — skip here.
        let Some(def) = ha_shared::get_machine(&m.machine_id) else {
            continue;
        };
        if def.id == LAUNCH_PAD.id {
            continue;
        }
        let Some(&body) = CelestialBody::all()
            .iter()
            .find(|b| body_def(**b).label == m.body)
        else {
            continue;
        };
        world.placed.push(LocalMachine {
            uid: m.uid,
            machine_id: def.id,
            body,
            x: m.x,
            y: m.y,
            z: m.z,
            output_accumulator: 0.0,
            stored_output: m.stored_output,
            producing_flash: 0.0,
        });
    }
    if !world.placed.is_empty() {
        eprintln!("ha-machines: loaded {} machines", world.placed.len());
    }
}

fn save_machines_to_disk(world: &MachineWorld) {
    let save = MachinesSave {
        next_uid: world.next_uid,
        placed: world
            .placed
            .iter()
            .filter(|m| m.machine_id != LAUNCH_PAD.id)
            .map(|m| MachineSave {
                uid: m.uid,
                machine_id: m.machine_id.to_string(),
                body: body_def(m.body).label.to_string(),
                x: m.x,
                y: m.y,
                z: m.z,
                stored_output: m.stored_output,
            })
            .collect(),
    };
    match serde_json::to_string(&save) {
        Ok(json) => {
            if let Err(err) = std::fs::write(machines_path(), json) {
                eprintln!("ha-machines: save failed: {err}");
            }
        }
        Err(err) => eprintln!("ha-machines: serialize failed: {err}"),
    }
}

fn debounced_machines_save(time: Res<Time>, mut world: ResMut<MachineWorld>) {
    let paused = is_app_paused();
    if paused && !world.was_paused {
        save_machines_to_disk(&world);
        world.dirty = false;
    }
    world.was_paused = paused;

    if !world.dirty {
        return;
    }
    world.debounce += time.delta_secs();
    if world.debounce >= 5.0 {
        save_machines_to_disk(&world);
        world.dirty = false;
        world.debounce = 0.0;
    }
}
