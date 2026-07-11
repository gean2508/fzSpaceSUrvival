//! Navigation data — probe missions + mapped bodies gating autopilot.
//! Numeric mirror of `packages/shared/src/sim/navData.ts` (rule §2: constants
//! must match; the TS side owns the design values).

use serde::{Deserialize, Serialize};

use crate::celestial_bodies::CelestialBody;

/// Probes fly this factor over the crewed transit burn speed (no mass, no life
/// support aboard).
pub const PROBE_SPEED_FACTOR: f32 = 2.0;
/// Fixed surface-scan phase once the probe arrives.
pub const PROBE_SCAN_SECS: f32 = 20.0;
/// Data transmission back home: base + distance-scaled, capped (arcade comms).
pub const PROBE_TRANSMIT_BASE_SECS: f32 = 5.0;
pub const PROBE_TRANSMIT_SECS_PER_M: f32 = 0.0001;
pub const PROBE_TRANSMIT_CAP_SECS: f32 = 30.0;

pub fn probe_travel_secs(distance_m: f32, crewed_burn_speed: f32) -> f32 {
    distance_m / (crewed_burn_speed * PROBE_SPEED_FACTOR).max(1.0)
}

pub fn probe_transmit_secs(distance_m: f32) -> f32 {
    (PROBE_TRANSMIT_BASE_SECS + distance_m * PROBE_TRANSMIT_SECS_PER_M)
        .min(PROBE_TRANSMIT_CAP_SECS)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProbePhase {
    Outbound,
    Scanning,
    Transmitting,
    Delivered,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ProbeMission {
    pub target: CelestialBody,
    /// SolarSystemClock seconds at launch.
    pub launched_t: f32,
    /// Clock time the probe reaches the target (lead-corrected at launch).
    pub arrive_t: f32,
    /// One-way distance at launch — reused for the transmission delay.
    pub distance_m: f32,
    pub phase: ProbePhase,
}

impl ProbeMission {
    /// Advance phase against the shared clock; returns true when the mission
    /// just delivered its nav data (caller marks the body mapped).
    pub fn advance(&mut self, t: f32) -> bool {
        let scanned_t = self.arrive_t + PROBE_SCAN_SECS;
        let delivered_t = scanned_t + probe_transmit_secs(self.distance_m);
        let next = if t >= delivered_t {
            ProbePhase::Delivered
        } else if t >= scanned_t {
            ProbePhase::Transmitting
        } else if t >= self.arrive_t {
            ProbePhase::Scanning
        } else {
            ProbePhase::Outbound
        };
        let delivered_now = next == ProbePhase::Delivered && self.phase != ProbePhase::Delivered;
        self.phase = next;
        delivered_now
    }
}

/// Persistent per-player navigation network — NOT per ship (losing a ship must
/// not wipe exploration progress) and NOT per base (any console reads the same
/// agency network).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NavData {
    pub mapped: Vec<CelestialBody>,
    pub probes: Vec<ProbeMission>,
}

impl Default for NavData {
    fn default() -> Self {
        // Earth pre-mapped; the Moon is earned by the first probe or manual
        // visit — natural tutorial arc.
        Self {
            mapped: vec![CelestialBody::Earth],
            probes: Vec::new(),
        }
    }
}

impl NavData {
    pub fn is_mapped(&self, body: CelestialBody) -> bool {
        self.mapped.contains(&body)
    }

    /// Returns true if newly mapped (false when already known).
    pub fn mark_mapped(&mut self, body: CelestialBody) -> bool {
        if self.is_mapped(body) {
            return false;
        }
        self.mapped.push(body);
        true
    }

    /// Next autopilot-less landable body in fixed catalog order — the probe
    /// console's default target (v1: no picker UI yet).
    pub fn next_unmapped_target(&self) -> Option<CelestialBody> {
        CelestialBody::visitable_surfaces()
            .iter()
            .copied()
            .find(|b| !self.is_mapped(*b) && !self.probe_en_route(*b))
    }

    pub fn probe_en_route(&self, body: CelestialBody) -> bool {
        self.probes
            .iter()
            .any(|p| p.target == body && p.phase != ProbePhase::Delivered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_maps_earth_only() {
        let nav = NavData::default();
        assert!(nav.is_mapped(CelestialBody::Earth));
        assert!(!nav.is_mapped(CelestialBody::Moon));
    }

    #[test]
    fn transmit_secs_is_capped() {
        assert!(probe_transmit_secs(10_000_000.0) <= PROBE_TRANSMIT_CAP_SECS);
        assert!(probe_transmit_secs(0.0) >= PROBE_TRANSMIT_BASE_SECS - 1e-3);
    }

    #[test]
    fn probe_lifecycle_delivers_once() {
        let mut probe = ProbeMission {
            target: CelestialBody::Moon,
            launched_t: 0.0,
            arrive_t: 100.0,
            distance_m: 52_000.0,
            phase: ProbePhase::Outbound,
        };
        assert!(!probe.advance(50.0));
        assert_eq!(probe.phase, ProbePhase::Outbound);
        assert!(!probe.advance(105.0));
        assert_eq!(probe.phase, ProbePhase::Scanning);
        let done_t = 100.0 + PROBE_SCAN_SECS + probe_transmit_secs(52_000.0) + 1.0;
        assert!(probe.advance(done_t));
        assert_eq!(probe.phase, ProbePhase::Delivered);
        assert!(!probe.advance(done_t + 5.0));
    }

    #[test]
    fn next_unmapped_skips_earth_and_en_route() {
        let mut nav = NavData::default();
        let first = nav.next_unmapped_target().expect("unmapped body");
        assert_ne!(first, CelestialBody::Earth);
        nav.probes.push(ProbeMission {
            target: first,
            launched_t: 0.0,
            arrive_t: 10.0,
            distance_m: 1.0,
            phase: ProbePhase::Outbound,
        });
        let second = nav.next_unmapped_target();
        assert_ne!(second, Some(first));
    }
}
