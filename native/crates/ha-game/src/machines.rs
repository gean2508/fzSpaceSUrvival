//! Local machine sim slice — mirrors `packages/shared/src/sim/machines.ts`.

use std::collections::HashMap;

use bevy::prelude::*;
use ha_shared::{require_machine, WOOD_EXTRACTOR, LAUNCH_PAD};

use crate::game_time::fixed_delta_secs;
use crate::launch_pad::{in_launch_pad_zone, spawn_launch_pad, zone_has_launch_pad, LaunchPadWorld};

use crate::assets::AssetFactory;
use crate::camera::look_forward_flat;
use crate::earth_world::EarthDecor;
use crate::input::{EquippedItem, HoldInteractState, CameraLookIntent};
use crate::inventory_persist::{InventoryPersistState, PlayerInventory};
use crate::materials::ToonMaterial;
use crate::render::particles::spawn_harvest_particles;
use crate::sim::{Player, SimState};
use crate::surface_placement::{sphere_surface_transform, sphere_world_pos, MACHINE_SURFACE_LIFT};
use ha_shared::CelestialBody;

pub struct MachinesPlugin;

impl Plugin for MachinesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MachineWorld>()
            .init_resource::<PendingMachinePlacement>()
            .add_systems(FixedUpdate, tick_machines)
            .add_systems(Update, try_place_on_hold);
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
}

#[derive(Component)]
pub struct MachineEntity {
    pub uid: u32,
}

pub(crate) fn try_place_on_hold(
    mut hold: ResMut<HoldInteractState>,
    equipped: Res<EquippedItem>,
    look: Res<CameraLookIntent>,
    pending: Res<PendingMachinePlacement>,
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
    layout: Res<crate::sim::WorldLayout>,
) {
    if !hold.completed || hold.placing_machine || hold.flight_hold {
        return;
    }
    hold.completed = false;
    hold.placing_machine = false;
    hold.placing_stone = false;

    if equipped.item_id != "hammer" {
        return;
    }
    let machine_id = pending.machine_id;
    let def = require_machine(machine_id);
    if !inv.can_afford_machine(def) {
        return;
    }
    let Ok((_tf, sim)) = player.get_single() else {
        return;
    };
    let fwd = look_forward_flat(&look);
    let tx = sim.body.x + fwd.x * 2.2;
    let tz = sim.body.z + fwd.y * 2.2;
    let y = ha_shared::ground_height_at(tx, tz);

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
        x: tx,
        y,
        z: tz,
        output_accumulator: 0.0,
        stored_output: 0,
        producing_flash: 0.0,
    });

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
        let asset = factory.require(def.asset_kind, &mut meshes, &mut materials, &mut std_mats);
        let place_tf = sphere_surface_transform(CelestialBody::Earth, tx, tz, MACHINE_SURFACE_LIFT, look.yaw);
        let entity = commands.spawn((
            Mesh3d(asset.mesh.clone()),
            place_tf,
            EarthDecor,
            MachineEntity { uid },
            Name::new(format!("Machine_{uid}")),
        ));
        let mut entity = entity;
        asset.material.insert_on(&mut entity);
    }

    let particle_pos = sphere_world_pos(CelestialBody::Earth, tx, tz, MACHINE_SURFACE_LIFT + 0.15);
    spawn_harvest_particles(
        &mut commands,
        &pool,
        particle_pos,
        uid as usize,
    );
}

fn tick_machines(
    time: Res<Time<Fixed>>,
    mut world: ResMut<MachineWorld>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
    mut materials: ResMut<Assets<ToonMaterial>>,
    mut meshes: Query<(&MachineEntity, &MeshMaterial3d<ToonMaterial>)>,
) {
    let dt = fixed_delta_secs(&time);
    let mut any_flash = false;
    let mut produced = false;
    for m in &mut world.placed {
        let def = require_machine(m.machine_id);
        if !def.passive {
            continue;
        }
        m.output_accumulator += def.output_per_second * dt;
        let whole = m.output_accumulator.floor() as u32;
        if whole > 0 {
            m.output_accumulator -= whole as f32;
            m.stored_output += whole;
            inv.add(def.output_item_id, whole);
            produced = true;
            m.producing_flash = 1.0;
        }
        if m.producing_flash > 0.0 {
            m.producing_flash = (m.producing_flash - dt * 0.8).max(0.0);
            any_flash = true;
        }
    }

    if produced {
        persist.mark_dirty();
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
