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

use crate::camera::facing_sign;
use crate::hud::item_icons::ItemIconCache;
use crate::input::{EquippedItem, HoldInteractState};
use crate::inventory_persist::{persist_dir, InventoryPersistState, PlayerInventory};
use crate::platform::is_app_paused;
use crate::render::particles::spawn_harvest_particles;
use crate::rocket_flight::RocketFlight;
use crate::scene::{RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, SimState};
use crate::world2d::{active_landable_body, ActivePlanet, TileWorld};
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
                (
                    try_place_on_hold.before(crate::hud::update_interact_hud),
                    sync_machine_visuals,
                    debounced_machines_save,
                ),
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

fn ground_y_on(tiles: &TileWorld, body: CelestialBody, s: f32) -> f32 {
    tiles.ground_height_at(body, s)
}

/// Machine ↔ body gates: extractors need what they extract to exist there.
fn machine_allowed_on(machine_id: &str, body: CelestialBody) -> bool {
    let airless = body_def(body).atmo_top_m <= 0.0;
    match machine_id {
        // Trees/launch zone are Earth content.
        id if id == WOOD_EXTRACTOR.id || id == LAUNCH_PAD.id => body == CelestialBody::Earth,
        // Regolith + uplink + sealed habitats are the off-world camp kit.
        id if id == REGOLITH_DRILL.id || id == ANTENNA.id || id == ha_shared::HABITAT.id => {
            airless
        }
        _ => true,
    }
}

pub(crate) fn try_place_on_hold(
    mut hold: ResMut<HoldInteractState>,
    equipped: Res<EquippedItem>,
    pending: Res<PendingMachinePlacement>,
    ctx: PlacementCtx,
    tiles: Res<TileWorld>,
    player: Query<(&Transform, &SimState), With<Player>>,
    mut world: ResMut<MachineWorld>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
    mut commands: Commands,
    pool: Res<crate::render::particles::HarvestParticlePool>,
    mut icons: ResMut<ItemIconCache>,
    mut images: ResMut<Assets<Image>>,
    art: Res<crate::art2d::ArtLibrary>,
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
    let tx = sim.body.x + facing_sign(sim.body.rot) * 2.4;
    let tz = 0.0;
    let y = ground_y_on(&tiles, body, tx);

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
        spawn_launch_pad(&mut commands, uid, tx, y, &mut launch_pads);
    } else {
        spawn_machine_visual(
            &mut commands,
            &mut icons,
            &mut images,
            &art,
            def.id,
            body,
            uid,
            tx,
            y,
        );
    }

    spawn_harvest_particles(
        &mut commands,
        &pool,
        Vec3::new(tx, y + 0.6, 0.0),
        uid as usize,
    );
}

#[allow(clippy::too_many_arguments)]
fn spawn_machine_visual(
    commands: &mut Commands,
    icons: &mut ItemIconCache,
    images: &mut Assets<Image>,
    art: &crate::art2d::ArtLibrary,
    machine_id: &str,
    body: CelestialBody,
    uid: u32,
    tx: f32,
    ground_y: f32,
) {
    // Drop-in pack art first (machines/<id>.png), then the item icon (which
    // itself may be pack art via items/<id>.png), then procedural raster.
    let image = art
        .get(&format!("machines/{machine_id}"))
        .unwrap_or_else(|| icons.get(machine_id, images));
    let mut sprite = Sprite::from_image(image);
    sprite.custom_size = Some(Vec2::splat(1.8));
    let _ = body;
    commands
        .spawn((
            sprite,
            Transform::from_xyz(tx, ground_y + 0.95, 1.0),
            MachineEntity { uid },
            Name::new(format!("Machine_{uid}")),
        ))
        .with_children(|p| {
            p.spawn((
                Sprite::from_color(Color::srgb(0.30, 0.32, 0.36), Vec2::new(1.9, 0.25)),
                Transform::from_xyz(0.0, -1.0, -0.05),
            ));
        });
}

/// Respawn visuals for persisted machines on the active body (few machines —
/// the uid scan is cheap; launch pads own their visuals via `LaunchPadWorld`).
fn sync_machine_visuals(
    ctx: PlacementCtx,
    world: Res<MachineWorld>,
    tiles: Res<TileWorld>,
    existing: Query<(Entity, &MachineEntity)>,
    mut commands: Commands,
    mut icons: ResMut<ItemIconCache>,
    mut images: ResMut<Assets<Image>>,
    art: Res<crate::art2d::ArtLibrary>,
) {
    let body = ctx.body();
    let by_uid: HashMap<u32, &LocalMachine> =
        world.placed.iter().map(|m| (m.uid, m)).collect();
    let mut spawned: HashSet<u32> = HashSet::new();
    // B6: despawn machine sprites that belong to another body (or are gone).
    for (e, me) in &existing {
        let keep = body.is_some() && by_uid.get(&me.uid).map(|m| Some(m.body) == body).unwrap_or(false);
        if keep {
            spawned.insert(me.uid);
        } else {
            commands.entity(e).despawn_recursive();
        }
    }
    let Some(body) = body else {
        return;
    };
    for m in world.on_body(body) {
        if m.machine_id == LAUNCH_PAD.id || spawned.contains(&m.uid) {
            continue;
        }
        spawn_machine_visual(
            &mut commands,
            &mut icons,
            &mut images,
            &art,
            m.machine_id,
            body,
            m.uid,
            m.x,
            ground_y_on(&tiles, body, m.x),
        );
    }
}

fn tick_machines(
    time: Res<Time<Fixed>>,
    ctx: PlacementCtx,
    colony: Res<crate::colony::ColonyRuntime>,
    mut world: ResMut<MachineWorld>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
    mut sprites: Query<(&MachineEntity, &mut Sprite)>,
) {
    let dt = fixed_delta_secs(&time);
    let player_body = ctx.body();
    let mut any_flash = false;
    let mut produced = false;
    // Fed colonists boost the first N lunar drills (stable placement order).
    let mut moon_drill_idx = 0u32;
    for m in &mut world.placed {
        let def = require_machine(m.machine_id);
        if !def.passive {
            continue;
        }
        let local = player_body == Some(m.body);
        let boost = if m.body == CelestialBody::Moon && m.machine_id == REGOLITH_DRILL.id {
            let b = ha_shared::machine_boost(&colony.state, moon_drill_idx);
            moon_drill_idx += 1;
            b
        } else {
            1.0
        };
        m.output_accumulator += def.output_per_second * boost * dt;
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
    for (entity, mut sprite) in &mut sprites {
        let Some(m) = by_uid.get(&entity.uid) else {
            continue;
        };
        // Green production flash tint on the machine sprite.
        let f = m.producing_flash.clamp(0.0, 1.0);
        sprite.color = Color::srgb(1.0 - 0.2 * f, 1.0 + 0.0 * f, 1.0 - 0.2 * f);
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
