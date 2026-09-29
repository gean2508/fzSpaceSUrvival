//! Navigation computer — persistent probe network that gates the autopilot.
//!
//! Per-PLAYER state (`nav_data.json`, same persistence pattern as
//! `inventory.json`): losing a ship must not wipe exploration progress, and
//! any console (Earth pad or orbital dock) reads the same agency network.

use std::path::PathBuf;

use bevy::prelude::*;
use ha_shared::{
    body_def, plan_transit, probe_travel_secs, CelestialBody, InteractTargetKind, NavData,
    ProbeMission, ProbePhase,
};

use crate::input::HoldInteractState;
use crate::inventory_persist::{persist_dir, InventoryPersistState, PlayerInventory};
use crate::platform::is_app_paused;
use crate::scene::{RegionId, RegionRuntime};
use crate::solar_system_clock::SolarSystemClock;
use crate::flight::orbital_start_above_pad;
use crate::targeting::LookTarget;

pub struct NavComputerPlugin;

impl Plugin for NavComputerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NavComputer>()
            .init_resource::<NavPersistState>()
            .add_systems(OnEnter(crate::game_phase::GamePhase::Loading), load_nav_on_startup)
            .add_systems(
                Update,
                (
                    try_launch_probe,
                    tick_probe_missions,
                    map_visited_bodies,
                    debounced_nav_save,
                ),
            );
    }
}

#[derive(Resource, Default)]
pub struct NavComputer(pub NavData);

#[derive(Resource)]
pub struct NavPersistState {
    dirty: bool,
    force_next: bool,
    debounce: Timer,
    was_paused: bool,
}

impl Default for NavPersistState {
    fn default() -> Self {
        Self {
            dirty: false,
            force_next: false,
            debounce: Timer::from_seconds(3.0, TimerMode::Repeating),
            was_paused: false,
        }
    }
}

impl NavPersistState {
    pub fn mark_immediate(&mut self) {
        self.dirty = true;
        self.force_next = true;
    }
}

fn nav_path() -> PathBuf {
    persist_dir().join("nav_data.json")
}

fn load_nav_on_startup(mut nav: ResMut<NavComputer>) {
    if let Ok(text) = std::fs::read_to_string(nav_path()) {
        match serde_json::from_str::<NavData>(&text) {
            Ok(loaded) => {
                nav.0 = loaded;
                eprintln!(
                    "ha-nav: loaded — {} mapped, {} probes in flight",
                    nav.0.mapped.len(),
                    nav.0.probes.len()
                );
                return;
            }
            Err(err) => eprintln!("ha-nav: nav_data.json unreadable ({err}) — starting fresh"),
        }
    }
    nav.0 = NavData::default();
}

fn save_nav_to_disk(nav: &NavData) {
    match serde_json::to_string(nav) {
        Ok(json) => {
            if let Err(err) = std::fs::write(nav_path(), json) {
                eprintln!("ha-nav: save failed: {err}");
            }
        }
        Err(err) => eprintln!("ha-nav: serialize failed: {err}"),
    }
}

/// Same contract as inventory persistence — debounced writes + flush on the
/// pause transition (B10: Android may kill the process before the debounce).
fn debounced_nav_save(
    time: Res<Time>,
    nav: Res<NavComputer>,
    mut persist: ResMut<NavPersistState>,
) {
    let paused = is_app_paused();
    if paused && !persist.was_paused {
        save_nav_to_disk(&nav.0);
        persist.dirty = false;
        persist.force_next = false;
    }
    persist.was_paused = paused;

    if !persist.dirty {
        return;
    }
    persist.debounce.tick(time.delta());
    if persist.force_next || persist.debounce.just_finished() {
        save_nav_to_disk(&nav.0);
        persist.dirty = false;
        persist.force_next = false;
    }
}

/// HOLD on a PROBE CONSOLE beacon: consume one crafted `probe` item and send
/// it to the next unmapped landable body (v1: fixed catalog order — a target
/// picker UI can replace `next_unmapped_target` later without touching the
/// mission flow).
fn try_launch_probe(
    mut hold: ResMut<HoldInteractState>,
    target: Res<LookTarget>,
    clock: Res<SolarSystemClock>,
    mut nav: ResMut<NavComputer>,
    mut nav_persist: ResMut<NavPersistState>,
    mut inv: ResMut<PlayerInventory>,
    mut inv_persist: ResMut<InventoryPersistState>,
) {
    if !hold.completed {
        return;
    }
    let Some(kind) = target.kind else {
        return;
    };
    if kind != InteractTargetKind::ProbeConsole {
        return;
    }
    hold.completed = false;

    let Some(target_body) = nav.0.next_unmapped_target() else {
        eprintln!("ha-nav: navigation network complete — no unmapped bodies left");
        return;
    };
    if inv.0.qty("probe") == 0 {
        eprintln!("ha-nav: no probes in inventory — craft a Nav Probe first");
        return;
    }
    if !inv.0.consume_ingredients(&[("probe", 1)]) {
        return;
    }
    inv_persist.mark_immediate();

    let t = clock.t;
    let from = orbital_start_above_pad();
    // Same lead rule + cruise curve as manual burns / autopilot; the probe
    // flies PROBE_SPEED_FACTOR× the crewed cruise speed (no mass, no life).
    let plan = plan_transit(target_body, t, from.to_array());
    let distance_m = plan.distance_m;
    let travel = probe_travel_secs(distance_m, plan.speed);
    nav.0.probes.push(ProbeMission {
        target: target_body,
        launched_t: t,
        arrive_t: t + travel,
        distance_m,
        phase: ProbePhase::Outbound,
    });
    nav_persist.mark_immediate();
    eprintln!(
        "ha-nav: probe launched → {} · travel {:.0}s + scan + transmit",
        body_def(target_body).label,
        travel
    );
}

/// Advance probe phases against the shared clock; deliveries map the body.
fn tick_probe_missions(
    clock: Res<SolarSystemClock>,
    mut nav: ResMut<NavComputer>,
    mut persist: ResMut<NavPersistState>,
) {
    if nav.0.probes.is_empty() {
        return;
    }
    let t = clock.t;
    let mut delivered: Option<CelestialBody> = None;
    for probe in nav.0.probes.iter_mut() {
        if probe.advance(t) {
            delivered = Some(probe.target);
        }
    }
    if let Some(body) = delivered {
        nav.0.mark_mapped(body);
        nav.0.probes.retain(|p| p.phase != ProbePhase::Delivered);
        persist.mark_immediate();
        eprintln!(
            "ha-nav: nav data received from {} — AUTOPILOT available",
            body_def(body).label
        );
    }
}

/// Landing somewhere by hand also maps it — you flew the route yourself.
fn map_visited_bodies(
    region: Res<RegionRuntime>,
    mut nav: ResMut<NavComputer>,
    mut persist: ResMut<NavPersistState>,
    mut last: Local<Option<RegionId>>,
) {
    if *last == Some(region.active) {
        return;
    }
    *last = Some(region.active);
    let body = match region.active {
        RegionId::Overworld { .. } => CelestialBody::Earth,
        RegionId::Luna => CelestialBody::Moon,
        RegionId::Planet(b) => b,
        _ => return,
    };
    if nav.0.mark_mapped(body) {
        persist.mark_immediate();
        eprintln!(
            "ha-nav: {} mapped by manual visit — AUTOPILOT available",
            body_def(body).label
        );
    }
}
