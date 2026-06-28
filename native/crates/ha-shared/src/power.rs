//! Vessel power grid — solar generation, load, battery storage (1 Hz tick).

use crate::solar_light::{sample_solar, sun_center_world};
use crate::vessel::{PowerSnapshot, VesselState};

pub const SOLAR_PANEL_PEAK_W: f32 = 120.0;
pub const STATION_BASE_LOAD_W: f32 = 18.0;
pub const BATTERY_CAPACITY_WH: f32 = 500.0;
pub const ANTENNA_LOAD_W: f32 = 8.0;
pub const CAMERA_LOAD_W: f32 = 12.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleKind {
    SolarPanel = 0,
    Antenna = 1,
    Battery = 2,
}

impl ModuleKind {
    pub fn from_index(i: u8) -> Option<Self> {
        match i {
            0 => Some(Self::SolarPanel),
            1 => Some(Self::Antenna),
            2 => Some(Self::Battery),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::SolarPanel => "SOLAR PANEL",
            Self::Antenna => "ANTENNA",
            Self::Battery => "BATTERY",
        }
    }
}

/// Which station modules are assembled (3-slot layout).
#[derive(Clone, Debug, Default)]
pub struct ModuleAssembly {
    pub assembled: u8,
}

impl ModuleAssembly {
    pub fn has(&self, kind: ModuleKind) -> bool {
        (kind as u8) < self.assembled
    }
}

pub fn tick_vessel_power(v: &mut VesselState, observer_flight: [f32; 3], dt: f32) {
    let modules = ModuleAssembly {
        assembled: if v.id == crate::vessel::VesselId::OrbitalStation {
            3
        } else {
            0
        },
    };
    let mut snap = PowerSnapshot {
        storage_max_wh: BATTERY_CAPACITY_WH,
        ..v.power.clone()
    };

    let solar = if modules.has(ModuleKind::SolarPanel) {
        let sample = sample_solar(
            observer_flight,
            Some([0.0, 1.0, 0.0]),
            v.body,
            0.25,
            false,
        );
        snap.solar_efficiency = sample.disk_visibility;
        SOLAR_PANEL_PEAK_W * sample.disk_visibility
    } else {
        0.0
    };
    snap.generation_w = solar;

    let mut load = STATION_BASE_LOAD_W;
    if modules.has(ModuleKind::Antenna) {
        load += ANTENNA_LOAD_W;
    }
    if modules.has(ModuleKind::Battery) {
        load += CAMERA_LOAD_W;
    }
    snap.load_w = load;

    let net = snap.net_w();
    if net >= 0.0 {
        snap.storage_wh = (snap.storage_wh + net * dt / 3600.0).min(snap.storage_max_wh);
    } else {
        snap.storage_wh = (snap.storage_wh + net * dt / 3600.0).max(0.0);
    }

    v.power = snap;
}

/// Ray-sphere occlusion: is the Sun blocked by a planet between observer and sun?
pub fn sun_occluded_by_body(
    observer: [f32; 3],
    blocker_center: [f32; 3],
    blocker_radius: f32,
) -> bool {
    let sun = sun_center_world();
    let to_sun = [sun[0] - observer[0], sun[1] - observer[1], sun[2] - observer[2]];
    let len = (to_sun[0] * to_sun[0] + to_sun[1] * to_sun[1] + to_sun[2] * to_sun[2]).sqrt();
    if len < 1.0 {
        return false;
    }
    let dir = [to_sun[0] / len, to_sun[1] / len, to_sun[2] / len];
    let oc = ray_sphere_hit(observer, dir, blocker_center, blocker_radius * 1.05);
    oc.map(|t| t < len).unwrap_or(false)
}

fn ray_sphere_hit(origin: [f32; 3], dir: [f32; 3], center: [f32; 3], radius: f32) -> Option<f32> {
    let oc = [
        origin[0] - center[0],
        origin[1] - center[1],
        origin[2] - center[2],
    ];
    let b = 2.0 * (oc[0] * dir[0] + oc[1] * dir[1] + oc[2] * dir[2]);
    let c = oc[0] * oc[0] + oc[1] * oc[1] + oc[2] * oc[2] - radius * radius;
    let disc = b * b - 4.0 * c;
    if disc < 0.0 {
        return None;
    }
    let t = (-b - disc.sqrt()) / 2.0;
    if t > 0.01 {
        Some(t)
    } else {
        None
    }
}
