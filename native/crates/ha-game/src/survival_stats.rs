//! Survival bars runtime — hunger / thirst / oxygen on the local player.
//!
//! Sim rules live in `ha-shared/survival.rs` (mirror of TS `survivalStats.ts`);
//! this plugin owns: the 20 Hz tick (vacuum detection + damage), the eat-hold
//! consumer, gentle death/respawn, and `survival.json` persistence
//! (flush-on-pause, same pattern as `harvested.json`).

use bevy::prelude::*;
use ha_shared::{
    apply_food, body_def, step_survival_stats, InteractTargetKind, SurvivalEnv, SurvivalStats,
    LAUNCH_PAD_ZONE_X, LAUNCH_PAD_ZONE_Z, WorldContext,
};
use serde::{Deserialize, Serialize};

use crate::game_time::fixed_delta_secs;
use crate::hud::inventory_ui::{
    consume_from_ui_slots, flush_slots_to_inventory, InventoryUiState,
};
use crate::input::{EquippedItem, HoldInteractState};
use crate::inventory_persist::{persist_dir, InventoryPersistState, PlayerInventory};
use crate::planet_world::ActivePlanet;
use crate::platform::is_app_paused;
use crate::rocket_flight::RocketFlight;
use crate::scene::WorldContextRuntime;
use crate::sim::{Player, SimState};
use crate::targeting::InteractTarget;

/// O₂ tether radius around pads / landers / docks (metres).
const SUPPLY_RADIUS: f32 = 14.0;

pub struct SurvivalStatsPlugin;

impl Plugin for SurvivalStatsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SurvivalPersist>()
            .add_systems(Startup, load_survival_on_startup)
            .add_systems(FixedUpdate, tick_survival)
            .add_systems(
                Update,
                (
                    apply_loaded_survival,
                    consume_eat_hold.before(crate::hud::update_interact_hud),
                    debounced_survival_save,
                ),
            );
    }
}

/// Walkable body for the active context (None while in space dock/mission).
fn current_body(ctx: WorldContext, planet: &ActivePlanet) -> Option<ha_shared::CelestialBody> {
    match ctx {
        WorldContext::Overworld => Some(ha_shared::CelestialBody::Earth),
        WorldContext::Luna => Some(ha_shared::CelestialBody::Moon),
        WorldContext::Planet => planet.body,
        WorldContext::Space | WorldContext::Mission => None,
    }
}

/// True when this world context exposes the suit to vacuum.
fn context_vacuum(ctx: WorldContext, planet: &ActivePlanet) -> bool {
    match ctx {
        WorldContext::Overworld | WorldContext::Mission => false,
        WorldContext::Space | WorldContext::Luna => true,
        WorldContext::Planet => planet
            .body
            .map(|b| body_def(b).atmo_top_m <= 0.0)
            // Unknown body — assume vacuum, suits stay on.
            .unwrap_or(true),
    }
}

/// Pads, landers, docks and return beacons all count as O₂ supply points.
fn is_supply_beacon(kind: InteractTargetKind) -> bool {
    matches!(
        kind,
        InteractTargetKind::LaunchModule
            | InteractTargetKind::LaunchIgnition
            | InteractTargetKind::SpaceReentry
            | InteractTargetKind::SpaceTransitLuna
            | InteractTargetKind::SpaceTransitBody
            | InteractTargetKind::SpaceStationModule
            | InteractTargetKind::LunarReturn
            | InteractTargetKind::PlanetReturn
    )
}

fn tick_survival(
    time: Res<Time<Fixed>>,
    world_ctx: Res<WorldContextRuntime>,
    planet_ctx: Res<ActivePlanet>,
    flight: Res<RocketFlight>,
    machines: Res<crate::machines::MachineWorld>,
    beacons: Query<(&GlobalTransform, &InteractTarget)>,
    mut player: Query<(&mut SimState, &Transform), With<Player>>,
    mut persist: ResMut<SurvivalPersist>,
) {
    if is_app_paused() {
        return;
    }
    let Ok((mut sim, tf)) = player.get_single_mut() else {
        return;
    };
    let dt = fixed_delta_secs(&time);

    // Inside the capsule during flight = pressurized, not exposed.
    let exposed_vacuum = context_vacuum(world_ctx.active, &planet_ctx) && !flight.active;
    let near_beacon = exposed_vacuum
        && beacons.iter().any(|(gtf, t)| {
            is_supply_beacon(t.kind)
                && gtf.translation().distance(tf.translation) < SUPPLY_RADIUS
        });
    // The camp keeps you breathing: any machine on this body within the
    // tether counts as base life support (tangent-space distance).
    let near_camp = exposed_vacuum
        && !near_beacon
        && current_body(world_ctx.active, &planet_ctx).is_some_and(|body| {
            machines.on_body(body).any(|m| {
                let dx = m.x - sim.body.x;
                let dz = m.z - sim.body.z;
                (dx * dx + dz * dz).sqrt() < SUPPLY_RADIUS
            })
        });
    let env = SurvivalEnv {
        vacuum: exposed_vacuum,
        oxygen_supply: !exposed_vacuum || near_beacon || near_camp,
    };

    let damage = step_survival_stats(&mut sim.survival, dt, env);
    if damage > 0.0 {
        sim.hp = (sim.hp - damage).max(0.0);
        if sim.hp <= 0.0 {
            gentle_respawn(&mut sim, world_ctx.active);
        }
    }
    persist.dirty = true;
}

/// Gentle failure (vision doc): no hard game over. Bars refill, inventory is
/// kept; on Tierra you wake up at the agency pad, off-world you get back up
/// where you fell with a fresh tank (the beacon tether keeps it survivable).
fn gentle_respawn(sim: &mut SimState, ctx: WorldContext) {
    sim.hp = sim.max_hp;
    sim.survival = SurvivalStats::default();
    if matches!(ctx, WorldContext::Overworld) {
        sim.body.x = LAUNCH_PAD_ZONE_X;
        sim.body.z = LAUNCH_PAD_ZONE_Z;
        sim.body.y = crate::sim::earth_surface_y(LAUNCH_PAD_ZONE_X, LAUNCH_PAD_ZONE_Z);
        sim.body.vx = 0.0;
        sim.body.vy = 0.0;
        sim.body.vz = 0.0;
    }
}

/// Consumes a completed eat-hold: one food item out of the equipped stack,
/// bars up. Runs before `update_interact_hud` so the generic no-target branch
/// doesn't swallow the completion.
fn consume_eat_hold(
    mut hold: ResMut<HoldInteractState>,
    mut equipped: ResMut<EquippedItem>,
    mut inv: ResMut<PlayerInventory>,
    mut inv_ui: ResMut<InventoryUiState>,
    mut inv_persist: ResMut<InventoryPersistState>,
    mut player: Query<&mut SimState, With<Player>>,
) {
    if !hold.completed || !hold.eating {
        return;
    }
    hold.completed = false;
    hold.eating = false;

    let Ok(mut sim) = player.get_single_mut() else {
        return;
    };
    let item_id = equipped.item_id.clone();
    if inv.qty(&item_id) == 0 {
        return;
    }
    if !apply_food(&mut sim.survival, &item_id) {
        return;
    }
    consume_from_ui_slots(&mut inv_ui, &[(item_id.as_str(), 1)]);
    flush_slots_to_inventory(&inv_ui, &mut inv);
    inv_persist.mark_immediate();
    if inv.qty(&item_id) == 0 {
        equipped.item_id = "hand".to_string();
    }
}

// --- Persistence (survival.json, flush-on-pause) -------------------------------

#[derive(Resource, Default)]
pub struct SurvivalPersist {
    loaded: Option<SurvivalSave>,
    dirty: bool,
    was_paused: bool,
    debounce: f32,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
struct SurvivalSave {
    hp: f32,
    hunger: f32,
    thirst: f32,
    oxygen: f32,
}

fn survival_path() -> std::path::PathBuf {
    persist_dir().join("survival.json")
}

fn load_survival_on_startup(mut persist: ResMut<SurvivalPersist>) {
    let Ok(text) = std::fs::read_to_string(survival_path()) else {
        return;
    };
    match serde_json::from_str::<SurvivalSave>(&text) {
        Ok(save) => persist.loaded = Some(save),
        Err(_) => eprintln!("ha-survival: survival.json unreadable — starting full"),
    }
}

/// The player spawns in `scene::spawn_world`; apply the loaded bars once it exists.
fn apply_loaded_survival(
    mut persist: ResMut<SurvivalPersist>,
    mut player: Query<&mut SimState, With<Player>>,
) {
    let Some(save) = persist.loaded else {
        return;
    };
    let Ok(mut sim) = player.get_single_mut() else {
        return;
    };
    sim.hp = save.hp.clamp(1.0, sim.max_hp);
    sim.survival.hunger = save.hunger.clamp(0.0, ha_shared::HUNGER_MAX);
    sim.survival.thirst = save.thirst.clamp(0.0, ha_shared::THIRST_MAX);
    sim.survival.oxygen = save.oxygen.clamp(0.0, ha_shared::OXYGEN_MAX);
    persist.loaded = None;
}

fn debounced_survival_save(
    time: Res<Time>,
    mut persist: ResMut<SurvivalPersist>,
    player: Query<&SimState, With<Player>>,
) {
    let paused = is_app_paused();
    let pause_edge = paused && !persist.was_paused;
    persist.was_paused = paused;

    if !persist.dirty {
        return;
    }
    persist.debounce += time.delta_secs();
    // Bars drift every tick — a slow cadence keeps writes negligible.
    if !pause_edge && persist.debounce < 20.0 {
        return;
    }
    let Ok(sim) = player.get_single() else {
        return;
    };
    let save = SurvivalSave {
        hp: sim.hp,
        hunger: sim.survival.hunger,
        thirst: sim.survival.thirst,
        oxygen: sim.survival.oxygen,
    };
    match serde_json::to_string(&save) {
        Ok(json) => {
            if let Err(err) = std::fs::write(survival_path(), json) {
                eprintln!("ha-survival: save failed: {err}");
            }
        }
        Err(err) => eprintln!("ha-survival: serialize failed: {err}"),
    }
    persist.dirty = false;
    persist.debounce = 0.0;
}
