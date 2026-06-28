//! Vessel registry persistence + off-screen tick.

use std::path::PathBuf;

use bevy::prelude::*;
use ha_shared::{tick_off_screen_vessels, VesselId, VesselRegistry, VESSEL_TICK_HZ};
use ha_shared::FlightPhase;

use crate::inventory_persist::persist_dir;
use crate::platform::is_app_paused;
use crate::rocket_flight::RocketFlight;
use crate::solar_system_clock::SolarSystemClock;

/// Bevy resource wrapper around shared vessel registry.
#[derive(Resource, Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct VesselRegistryRes(pub VesselRegistry);

impl Default for VesselRegistryRes {
    fn default() -> Self {
        Self(VesselRegistry::starter())
    }
}

impl std::ops::Deref for VesselRegistryRes {
    type Target = VesselRegistry;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for VesselRegistryRes {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub struct WorldPersistPlugin;

impl Plugin for WorldPersistPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VesselRegistryRes>()
            .init_resource::<VesselPersistState>()
            .init_resource::<VesselTickTimer>()
            .add_systems(Startup, load_vessels_on_startup)
            .add_systems(FixedUpdate, tick_vessel_registry)
            .add_systems(Update, (debounced_vessel_save, sync_player_vessel_from_flight));
    }
}

#[derive(Resource)]
pub struct VesselPersistState {
    pub dirty: bool,
    pub force_next: bool,
    debounce: Timer,
}

impl Default for VesselPersistState {
    fn default() -> Self {
        Self {
            dirty: false,
            force_next: false,
            debounce: Timer::from_seconds(5.0, TimerMode::Repeating),
        }
    }
}

impl VesselPersistState {
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn mark_immediate(&mut self) {
        self.dirty = true;
        self.force_next = true;
    }
}

#[derive(Resource)]
struct VesselTickTimer(Timer);

impl Default for VesselTickTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(1.0 / VESSEL_TICK_HZ, TimerMode::Repeating))
    }
}

fn load_vessels_on_startup(
    mut reg: ResMut<VesselRegistryRes>,
    mut persist: ResMut<VesselPersistState>,
) {
    match load_vessels_from_disk() {
        Some(loaded) => reg.0 = loaded,
        None => reg.0 = VesselRegistry::starter(),
    }
    reg.park_orbit(
        ha_shared::VesselId::OrbitalStation,
        ha_shared::CelestialBody::Earth,
        600.0,
        0.0,
    );
    persist.mark_immediate();
}

fn tick_vessel_registry(
    time: Res<Time<Fixed>>,
    clock: Res<SolarSystemClock>,
    mut reg: ResMut<VesselRegistryRes>,
    mut timer: ResMut<VesselTickTimer>,
    mut persist: ResMut<VesselPersistState>,
) {
    timer.0.tick(time.delta());
    if !timer.0.just_finished() {
        return;
    }
    tick_off_screen_vessels(&mut reg, timer.0.duration().as_secs_f32(), clock.t);
    persist.mark_dirty();
}

fn debounced_vessel_save(
    time: Res<Time>,
    reg: Res<VesselRegistryRes>,
    mut persist: ResMut<VesselPersistState>,
) {
    if is_app_paused() {
        save_vessels_to_disk(&reg);
        persist.dirty = false;
        persist.force_next = false;
        return;
    }
    if !persist.dirty {
        return;
    }
    persist.debounce.tick(time.delta());
    if persist.force_next || persist.debounce.just_finished() {
        save_vessels_to_disk(&reg);
        persist.dirty = false;
        persist.force_next = false;
    }
}

fn sync_player_vessel_from_flight(
    flight: Res<RocketFlight>,
    clock: Res<SolarSystemClock>,
    mut reg: ResMut<VesselRegistryRes>,
    mut persist: ResMut<VesselPersistState>,
    mut was_flying: Local<bool>,
) {
    let flying = flight.active && flight.phase != FlightPhase::Surface;
    if flying && !*was_flying {
        reg.ensure(VesselId::PlayerShip, flight.body);
        reg.set_active_flight(VesselId::PlayerShip);
        if let Some(v) = reg.get_mut(VesselId::PlayerShip) {
            v.fuel_pct = flight.fuel_pct();
        }
        persist.mark_dirty();
    } else if !flying && *was_flying {
        let pos = flight.pos.to_array();
        let alt = ha_shared::altitude_above_body_at(pos, flight.body, clock.t);
        let phase = (pos[0] * 0.001 + pos[2] * 0.0013).rem_euclid(std::f32::consts::TAU);
        reg.park_orbit(VesselId::PlayerShip, flight.body, alt.max(50.0), phase);
        if let Some(v) = reg.get_mut(VesselId::PlayerShip) {
            v.fuel_pct = flight.fuel_pct();
        }
        persist.mark_dirty();
    } else if flying {
        if let Some(v) = reg.get_mut(VesselId::PlayerShip) {
            v.fuel_pct = flight.fuel_pct();
            v.body = flight.body;
        }
    }
    *was_flying = flying;
}

pub fn vessels_path() -> PathBuf {
    persist_dir().join("vessels.json")
}

pub fn load_vessels_from_disk() -> Option<VesselRegistry> {
    let text = std::fs::read_to_string(vessels_path()).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save_vessels_to_disk(reg: &VesselRegistry) {
    let path = vessels_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(body) = serde_json::to_string_pretty(reg) {
        if let Err(err) = std::fs::write(&path, body) {
            eprintln!("vessels save failed: {err}");
        }
    }
}
