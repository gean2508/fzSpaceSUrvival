//! Off-screen vessel registry — abstract orbital state (no full physics on Mali).

use crate::celestial_bodies::CelestialBody;
use serde::{Deserialize, Serialize};

pub const VESSEL_TICK_HZ: f32 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum VesselId {
    #[default]
    PlayerShip = 0,
    OrbitalStation = 1,
    LunaOutpost = 2,
}

impl VesselId {
    pub fn label(self) -> &'static str {
        match self {
            Self::PlayerShip => "PLAYER SHIP",
            Self::OrbitalStation => "ORBITAL STATION",
            Self::LunaOutpost => "LUNA OUTPOST",
        }
    }

    pub fn all() -> &'static [VesselId] {
        &[Self::PlayerShip, Self::OrbitalStation, Self::LunaOutpost]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VesselSimMode {
    #[default]
    Inactive,
    /// Full arcade flight — client `RocketFlight` owns motion.
    ActiveFlight,
    /// Cheap analytic orbit around a body centre.
    ParkedOrbit,
    /// On a walkable shell — tangent coords only.
    ParkedSurface,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VesselState {
    pub id: VesselId,
    pub label: String,
    pub mode: VesselSimMode,
    pub body: CelestialBody,
    /// Orbital phase radians (parked orbit) or tangent X (surface).
    pub orbit_phase: f32,
    /// Tangent Z when parked on surface.
    pub surface_tz: f32,
    /// Altitude above body centre (flight metres) when orbiting.
    pub orbit_alt_m: f32,
    pub fuel_pct: f32,
    pub power: PowerSnapshot,
    pub connectivity: ConnectivitySnapshot,
}

impl VesselState {
    pub fn new(id: VesselId, body: CelestialBody) -> Self {
        Self {
            id,
            label: id.label().into(),
            mode: VesselSimMode::Inactive,
            body,
            orbit_phase: 0.0,
            surface_tz: 0.0,
            orbit_alt_m: 420.0,
            fuel_pct: 100.0,
            power: PowerSnapshot::default(),
            connectivity: ConnectivitySnapshot::default(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PowerSnapshot {
    pub generation_w: f32,
    pub load_w: f32,
    pub storage_wh: f32,
    pub storage_max_wh: f32,
    pub solar_efficiency: f32,
}

impl PowerSnapshot {
    pub fn net_w(&self) -> f32 {
        self.generation_w - self.load_w
    }

    pub fn storage_pct(&self) -> f32 {
        if self.storage_max_wh <= 0.0 {
            0.0
        } else {
            (self.storage_wh / self.storage_max_wh * 100.0).clamp(0.0, 100.0)
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ConnectivitySnapshot {
    pub earth_los: bool,
    pub repeater_los: bool,
    pub signal_strength: f32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VesselRegistry {
    pub vessels: Vec<VesselState>,
    pub active_vessel: VesselId,
}

impl VesselRegistry {
    pub fn starter() -> Self {
        let mut reg = Self {
            vessels: Vec::new(),
            active_vessel: VesselId::PlayerShip,
        };
        reg.ensure(VesselId::PlayerShip, CelestialBody::Earth);
        reg.ensure(VesselId::OrbitalStation, CelestialBody::Earth);
        reg
    }

    pub fn ensure(&mut self, id: VesselId, body: CelestialBody) {
        if self.get(id).is_none() {
            self.vessels.push(VesselState::new(id, body));
        }
    }

    pub fn get(&self, id: VesselId) -> Option<&VesselState> {
        self.vessels.iter().find(|v| v.id == id)
    }

    pub fn get_mut(&mut self, id: VesselId) -> Option<&mut VesselState> {
        self.vessels.iter_mut().find(|v| v.id == id)
    }

    pub fn park_orbit(&mut self, id: VesselId, body: CelestialBody, alt_m: f32, phase: f32) {
        if let Some(v) = self.get_mut(id) {
            v.mode = VesselSimMode::ParkedOrbit;
            v.body = body;
            v.orbit_alt_m = alt_m;
            v.orbit_phase = phase;
        }
    }

    pub fn park_surface(&mut self, id: VesselId, body: CelestialBody, tx: f32, tz: f32) {
        if let Some(v) = self.get_mut(id) {
            v.mode = VesselSimMode::ParkedSurface;
            v.body = body;
            v.orbit_phase = tx;
            v.surface_tz = tz;
        }
    }

    pub fn set_active_flight(&mut self, id: VesselId) {
        self.active_vessel = id;
        if let Some(v) = self.get_mut(id) {
            v.mode = VesselSimMode::ActiveFlight;
        }
    }

    pub fn set_inactive(&mut self, id: VesselId) {
        if let Some(v) = self.get_mut(id) {
            if v.mode == VesselSimMode::ActiveFlight {
                v.mode = VesselSimMode::Inactive;
            }
        }
    }
}

/// Advance parked vessels — Kepler-ish circular drift + power/connectivity.
pub fn tick_off_screen_vessels(reg: &mut VesselRegistry, dt: f32, t_secs: f32) {
    use crate::connectivity::sample_connectivity;
    use crate::power::tick_vessel_power;

    for v in &mut reg.vessels {
        if v.mode != VesselSimMode::ParkedOrbit && v.mode != VesselSimMode::ParkedSurface {
            continue;
        }
        if v.mode == VesselSimMode::ParkedOrbit {
            let period = orbit_period_for(v.body);
            v.orbit_phase = (v.orbit_phase + dt / period * std::f32::consts::TAU) % std::f32::consts::TAU;
        }
        let observer = vessel_flight_pos(v, t_secs);
        tick_vessel_power(v, observer, dt);
        v.connectivity = sample_connectivity(observer, v.body);
    }
}

fn orbit_period_for(body: CelestialBody) -> f32 {
    match body {
        CelestialBody::Earth => 540.0,
        CelestialBody::Moon => 420.0,
        _ => 600.0,
    }
}

/// Approximate flight-space position for telemetry / LoS.
pub fn vessel_flight_pos(v: &VesselState, t_secs: f32) -> [f32; 3] {
    use crate::solar_ephemeris::body_center_at;
    let c = body_center_at(v.body, t_secs);
    match v.mode {
        VesselSimMode::ParkedOrbit => {
            let r = v.orbit_alt_m + crate::celestial_bodies::body_radius(v.body);
            [
                c[0] + v.orbit_phase.cos() * r,
                c[1] + 40.0,
                c[2] + v.orbit_phase.sin() * r,
            ]
        }
        VesselSimMode::ParkedSurface => {
            use crate::planet_surface::planet_gameplay_world_pos;
            planet_gameplay_world_pos(v.body, v.orbit_phase, v.surface_tz, 2.0)
        }
        _ => c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parked_orbit_advances_phase() {
        let mut reg = VesselRegistry::starter();
        reg.park_orbit(VesselId::OrbitalStation, CelestialBody::Earth, 800.0, 0.0);
        let p0 = reg.get(VesselId::OrbitalStation).unwrap().orbit_phase;
        tick_off_screen_vessels(&mut reg, 1.0, 0.0);
        let p1 = reg.get(VesselId::OrbitalStation).unwrap().orbit_phase;
        assert!(p1 > p0);
    }
}
