//! Launch pad placement, module assembly, ship prefab visuals, ignition (2D).
//!
//! The pad is a wide platform sprite on the Earth strip; module slots and the
//! ignition point are invisible `InteractTarget`s with small beacon sprites.
//! The config-driven ship stack (BAG→SHIP) is a 2D sprite assembly — what you
//! configure is what stands on the pad.

use bevy::prelude::*;
use ha_shared::{
    assemble_module, create_launch_pad_state, CelestialBody, InteractActionKind,
    InteractTargetKind, LaunchPadState, LAUNCH_PAD, LAUNCH_PAD_MODULE_COUNT,
    LAUNCH_PAD_ZONE_RADIUS, LAUNCH_PAD_ZONE_X,
};

use crate::input::{EquippedItem, HoldInteractState};
use crate::inventory_persist::{InventoryPersistState, PlayerInventory};
use crate::machines::{LocalMachine, MachineEntity, MachineWorld};
use crate::rocket_flight::RocketFlight;
use crate::scene::{RegionId, RegionRuntime};
use crate::sim::{Player, SimState};
use crate::targeting::{InteractTarget, LookTarget};
use crate::world2d::TileWorld;

pub struct LaunchPadPlugin;

impl Plugin for LaunchPadPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LaunchPadWorld>()
            .add_systems(PostStartup, spawn_demo_launch_pad)
            .add_systems(
                Update,
                (
                    try_assemble_on_hold,
                    try_ignite_on_hold,
                    sync_ship_loadout,
                    sync_ship_config_visual,
                    sync_pad_visuals,
                ),
            );
    }
}

#[derive(Resource, Default)]
pub struct LaunchPadWorld {
    pub pads: Vec<LaunchPadState>,
}

#[derive(Component)]
pub struct LaunchPadRoot {
    pub uid: u32,
}

#[derive(Component)]
pub struct LaunchModuleMarker {
    pub pad_uid: u32,
    pub module_index: u8,
}

#[derive(Component)]
pub struct LaunchIgnitionMarker {
    pub pad_uid: u32,
}

#[derive(Component)]
struct LaunchModuleVisual;

/// Every pad-related render entity (platform, slots, ignition, console,
/// ship stack) — B6: despawned off-Earth, respawned by `sync_pad_visuals`.
#[derive(Component)]
pub struct PadDecor;

/// Marker for the config-driven ship visual — respawned when the player
/// edits the ship in BAG→SHIP (`sync_ship_config_visual`).
#[derive(Component)]
pub struct ShipConfigVisual;

/// Pad zone on the 2D strip: `s` within radius of the reserved arc position.
pub fn in_launch_pad_zone(x: f32, _z: f32) -> bool {
    (x - LAUNCH_PAD_ZONE_X).abs() <= LAUNCH_PAD_ZONE_RADIUS
}

pub fn zone_has_launch_pad(pads: &LaunchPadWorld) -> bool {
    pads.pads.iter().any(|p| in_launch_pad_zone(p.x, p.z))
}

/// Pre-placed spaceport pad so the slice is visible without hammer grind.
fn spawn_demo_launch_pad(
    mut commands: Commands,
    mut world: ResMut<MachineWorld>,
    mut pads: ResMut<LaunchPadWorld>,
    tiles: Res<TileWorld>,
    builder: Res<crate::ship_builder::ShipBuilderState>,
) {
    if zone_has_launch_pad(&pads) {
        return;
    }
    let _ = &tiles;
    let x = LAUNCH_PAD_ZONE_X;
    // Top-down: place the spaceport a few tiles south of the player spawn band.
    let z = 6.0;
    let uid = world.next_uid;
    world.next_uid += 1;
    world.placed.push(LocalMachine {
        uid,
        machine_id: LAUNCH_PAD.id,
        body: CelestialBody::Earth,
        x,
        y: 0.0,
        z,
        output_accumulator: 0.0,
        stored_output: 0,
        producing_flash: 0.0,
    });
    spawn_launch_pad(&mut commands, uid, x, z, &mut pads);
    // Demo ship pre-assembled so the Tierra → orbit loop is always testable.
    if let Some(pad) = pads.pads.iter_mut().find(|p| p.uid == uid) {
        pad.assembled_modules = LAUNCH_PAD_MODULE_COUNT;
        pad.ready = true;
    }
    spawn_config_ship_visual(&mut commands, uid, x, z, &builder.config);
    eprintln!("ha-launch: demo pad at ({x},{z}) — ship ready for launch");
}

/// Pad platform + interact markers (top-down). `x,z` = world ground coords.
pub fn spawn_launch_pad(
    commands: &mut Commands,
    uid: u32,
    x: f32,
    z: f32,
    pads: &mut LaunchPadWorld,
) {
    use crate::world2d::{render_xy, sort_z};
    pads.pads.push(create_launch_pad_state(uid, x, 0.0, z));
    let base = render_xy(x, z);

    // Wide top-down pad platform.
    commands.spawn((
        LaunchPadRoot { uid },
        PadDecor,
        MachineEntity { uid },
        Sprite::from_color(Color::srgb(0.42, 0.44, 0.48), Vec2::new(7.0, 6.0)),
        Transform::from_xyz(base.x, base.y, sort_z(z) - 1.0),
        Name::new(format!("LaunchPad_{uid}")),
    ));

    // Module slots (hull / tank / engine) around the pad centre.
    for (dx, dz, i) in [(-2.0, 1.0, 0u8), (2.0, 1.0, 1u8), (0.0, 2.0, 2u8)] {
        let r = render_xy(x + dx, z + dz);
        commands.spawn((
            PadDecor,
            Transform::from_xyz(r.x, r.y, sort_z(z + dz)),
            InteractTarget {
                kind: InteractTargetKind::LaunchModule,
                id: i as u32,
            },
            LaunchModuleMarker {
                pad_uid: uid,
                module_index: i,
            },
            Sprite::from_color(Color::srgba(0.9, 0.8, 0.3, 0.35), Vec2::new(1.2, 1.2)),
            Name::new(format!("LaunchModule_{uid}_{i}")),
        ));
    }

    // Ignition point at the pad centre.
    commands.spawn((
        PadDecor,
        Transform::from_xyz(base.x, base.y, sort_z(z) + 0.5),
        InteractTarget {
            kind: InteractTargetKind::LaunchIgnition,
            id: uid,
        },
        LaunchIgnitionMarker { pad_uid: uid },
        Sprite::from_color(Color::srgba(1.0, 0.55, 0.12, 0.55), Vec2::new(1.2, 1.2)),
        Name::new(format!("LaunchIgnition_{uid}")),
    ));

    // PROBE CONSOLE — send nav probes without leaving Earth.
    let pr = render_xy(x + 4.5, z);
    commands.spawn((
        PadDecor,
        Transform::from_xyz(pr.x, pr.y, sort_z(z)),
        InteractTarget {
            kind: InteractTargetKind::ProbeConsole,
            id: uid,
        },
        Sprite::from_color(Color::srgb(1.0, 0.62, 0.18), Vec2::new(0.9, 1.2)),
        Name::new(format!("ProbeConsolePad_{uid}")),
    ));
}

/// Top-down ship on the pad: a rocket seen from above (nose up = north).
/// Size hints at the config (more tanks/engines → bigger).
fn spawn_config_ship_visual(
    commands: &mut Commands,
    uid: u32,
    x: f32,
    z: f32,
    config: &ha_shared::ShipConfig,
) {
    use crate::world2d::{render_xy, sort_z};
    let r = render_xy(x, z);
    let len = 2.6 + config.tanks.max(1) as f32 * 0.5;
    let wid = 1.1 + config.fins.min(4) as f32 * 0.15;
    commands
        .spawn((
            ShipConfigVisual,
            PadDecor,
            Sprite::from_color(Color::srgb(0.80, 0.82, 0.86), Vec2::new(wid, len)),
            Transform::from_xyz(r.x, r.y, sort_z(z) + 0.6),
            Name::new(format!("ShipPrefab_{uid}")),
        ))
        .with_children(|p| {
            // Nose (north / up).
            if config.nose > 0 {
                p.spawn((
                    Sprite::from_color(Color::srgb(0.92, 0.48, 0.12), Vec2::new(wid * 0.7, 0.7)),
                    Transform::from_xyz(0.0, len * 0.5, 0.05),
                ));
            }
            // Engine bells (south / down).
            p.spawn((
                Sprite::from_color(Color::srgb(0.88, 0.35, 0.2), Vec2::new(wid * 0.8, 0.6)),
                Transform::from_xyz(0.0, -len * 0.5, 0.05),
            ));
        });
}

/// BAG→SHIP edits rebuild the pad ship on the spot — the SFS payoff.
pub(crate) fn sync_ship_config_visual(
    builder: Res<crate::ship_builder::ShipBuilderState>,
    pads: Res<LaunchPadWorld>,
    _tiles: Res<TileWorld>,
    old: Query<Entity, With<ShipConfigVisual>>,
    mut commands: Commands,
) {
    if !builder.is_changed() || builder.is_added() {
        return;
    }
    let Some(pad) = pads.pads.iter().find(|p| p.ready) else {
        return;
    };
    for e in &old {
        commands.entity(e).despawn_recursive();
    }
    spawn_config_ship_visual(&mut commands, pad.uid, pad.x, pad.z, &builder.config);
}

/// Assembled module visual — small colored crate beside the slot.
fn spawn_starship_module_visual(
    commands: &mut Commands,
    module_index: u8,
    pos: Vec3,
) {
    let rgb = match module_index {
        0 => Color::srgb(0.55, 0.58, 0.62),
        1 => Color::srgb(0.92, 0.48, 0.12),
        _ => Color::srgb(0.88, 0.22, 0.14),
    };
    commands.spawn((
        Sprite::from_color(rgb, Vec2::new(1.2, 1.2)),
        Transform::from_translation(pos.truncate().extend(1.1)),
        LaunchModuleVisual,
        PadDecor,
        Name::new(format!("ShipModule_{module_index}")),
    ));
}

pub(crate) fn try_assemble_on_hold(
    mut commands: Commands,
    mut hold: ResMut<HoldInteractState>,
    target: Res<LookTarget>,
    equipped: Res<EquippedItem>,
    region: Res<RegionRuntime>,
    mut pads: ResMut<LaunchPadWorld>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
    modules: Query<(&LaunchModuleMarker, &GlobalTransform)>,
) {
    if !hold.completed || hold.placing_machine {
        return;
    }
    if !matches!(region.active, RegionId::Overworld { .. }) {
        return;
    }
    let Some(kind) = target.kind else {
        return;
    };
    if kind != InteractTargetKind::LaunchModule {
        return;
    }
    let Some(entity) = target.entity else {
        return;
    };
    let Ok((marker, gtf)) = modules.get(entity) else {
        return;
    };
    let Some(rule) = ha_shared::resolve_interaction(Some(equipped.item_id.as_str()), kind) else {
        return;
    };
    if rule.kind != InteractActionKind::Assemble {
        return;
    }
    hold.completed = false;

    let pad_uid = marker.pad_uid;
    let module_index = marker.module_index;
    let pos = gtf.translation();

    let Some(pad) = pads.pads.iter_mut().find(|p| p.uid == pad_uid) else {
        return;
    };
    if assemble_module(pad, module_index, &mut inv) {
        persist.mark_immediate();
        spawn_starship_module_visual(&mut commands, module_index, pos);
        eprintln!(
            "ha-launch: starship module {} on pad {} ({}/3)",
            module_index, pad_uid, pad.assembled_modules
        );
    }
}

pub(crate) fn try_ignite_on_hold(
    mut hold: ResMut<HoldInteractState>,
    target: Res<LookTarget>,
    equipped: Res<EquippedItem>,
    region: Res<RegionRuntime>,
    pads: Res<LaunchPadWorld>,
    mut flight: ResMut<RocketFlight>,
    clock: Res<crate::solar_system_clock::SolarSystemClock>,
    ignition: Query<&LaunchIgnitionMarker>,
    player: Query<&SimState, With<Player>>,
) {
    if !hold.completed || hold.placing_machine || flight.active {
        if hold.completed && flight.active {
            hold.completed = false;
        }
        return;
    }
    if !matches!(region.active, RegionId::Overworld { .. }) {
        return;
    }
    let Some(kind) = target.kind else {
        return;
    };
    if kind != InteractTargetKind::LaunchIgnition {
        return;
    }
    let Some(entity) = target.entity else {
        return;
    };
    let Ok(marker) = ignition.get(entity) else {
        return;
    };
    let Some(rule) = ha_shared::resolve_interaction(Some(equipped.item_id.as_str()), kind) else {
        return;
    };
    if rule.kind != InteractActionKind::Launch {
        return;
    }
    let Some(pad) = pads.pads.iter().find(|p| p.uid == marker.pad_uid) else {
        return;
    };
    if !pad.ready {
        hold.completed = false;
        return;
    }
    hold.completed = false;
    if player.get_single().is_err() {
        return;
    }

    // SFS ascent in 2D top-down: ignite into the orbital plane straight up off
    // Earth; the camera zooms out — continuous, no cutscene (B8).
    let t = clock.t;
    flight.clock_secs = t;
    let (pos, vel) = crate::rocket_flight::launch_pos_vel(CelestialBody::Earth, t, 18.0);
    flight.surface_flight = Some(CelestialBody::Earth);
    flight.begin_at(pos);
    // The Earth pad is the ONLY refuel point — the agency tops the tank here.
    flight.fuel = flight.capacity();
    flight.vel = vel;
    eprintln!("ha-launch: rocket ignition from pad — tank full");
}

/// B6 for the spaceport: pad visuals exist only on the Earth strip.
/// Leaving despawns them; returning respawns pad + ready ship from state.
fn sync_pad_visuals(
    region: Res<RegionRuntime>,
    flight: Res<RocketFlight>,
    pads: Res<LaunchPadWorld>,
    tiles: Res<TileWorld>,
    builder: Res<crate::ship_builder::ShipBuilderState>,
    decor: Query<Entity, With<PadDecor>>,
    roots: Query<(), With<LaunchPadRoot>>,
    ship_vis: Query<Entity, With<ShipConfigVisual>>,
    mut cooldown: Local<u32>,
    mut commands: Commands,
) {
    // Pad lives on Earth's surface layer only (z=0); hidden off-Earth or
    // underground (Z-Level < 0).
    let on_earth = matches!(region.active, RegionId::Overworld { .. }) && tiles.z == 0;
    if !on_earth {
        for e in &decor {
            commands.entity(e).despawn_recursive();
        }
        *cooldown = 0;
        return;
    }
    if *cooldown > 0 {
        *cooldown -= 1;
        return;
    }
    if flight.blocks_walking() {
        // The parked stack IS the rocket now — hide it while it flies (the pad
        // itself stays: the ascent surface frame shows it receding below).
        for e in &ship_vis {
            commands.entity(e).despawn_recursive();
        }
        return;
    }
    if !roots.is_empty() {
        // Pad survived the whole flight (ascent aborted back onto Earth):
        // restore the parked stack the ignition hid.
        if ship_vis.is_empty() {
            if let Some(pad) = pads.pads.iter().find(|p| p.ready) {
                spawn_config_ship_visual(&mut commands, pad.uid, pad.x, pad.z, &builder.config);
            }
        }
        return;
    }
    *cooldown = 3;
    // Back on Earth with pad state but no visuals → rebuild them.
    let mut pads_rebuilt = LaunchPadWorld::default();
    for pad in &pads.pads {
        spawn_launch_pad(&mut commands, pad.uid, pad.x, pad.z, &mut pads_rebuilt);
        if pad.ready {
            spawn_config_ship_visual(&mut commands, pad.uid, pad.x, pad.z, &builder.config);
        }
    }
}

/// Derive mass/fuel stats from the assembled pad modules + BAG→SHIP extras.
pub(crate) fn sync_ship_loadout(
    pads: Res<LaunchPadWorld>,
    builder: Res<crate::ship_builder::ShipBuilderState>,
    mut loadout: ResMut<crate::rocket_flight::ShipLoadout>,
    mut flight: ResMut<RocketFlight>,
) {
    if !pads.is_changed() && !builder.is_changed() {
        return;
    }
    const PAD_MODULE_IDS: [&str; 3] = ["hull", "fuel_tank", "engine"];
    let assembled = pads
        .pads
        .iter()
        .map(|p| p.assembled_modules as usize)
        .max()
        .unwrap_or(0)
        .min(PAD_MODULE_IDS.len());
    if assembled == 0 {
        return;
    }
    if assembled < PAD_MODULE_IDS.len() {
        let modules = &PAD_MODULE_IDS[..assembled];
        let mass = ha_shared::ship_mass_t(modules);
        let tanks = modules.iter().filter(|m| **m == "fuel_tank").count() as u32;
        loadout.mass_t = mass;
        loadout.burn_factor = ha_shared::burn_factor(mass);
        loadout.fuel_max = ha_shared::fuel_max_for_tanks(tanks);
        loadout.thrust_factor = 1.0;
        loadout.drag_factor = 1.0;
        loadout.fin_align_rate = 0.0;
        loadout.safe_landing_bonus = 0.0;
    } else {
        let stats = ha_shared::ship_config_stats(builder.config);
        loadout.mass_t = stats.mass_t;
        loadout.burn_factor = stats.burn_factor;
        loadout.fuel_max = stats.fuel_max;
        loadout.thrust_factor = stats.thrust_factor;
        loadout.drag_factor = stats.drag_factor;
        loadout.fin_align_rate = stats.fin_align_rate;
        loadout.safe_landing_bonus = stats.safe_landing_bonus;
    }
    flight.fuel_max = loadout.fuel_max;
    flight.burn_factor = loadout.burn_factor;
    flight.thrust_factor = loadout.thrust_factor;
    flight.drag_factor = loadout.drag_factor;
    flight.fin_align_rate = loadout.fin_align_rate;
    flight.safe_v_bonus = loadout.safe_landing_bonus;
}
