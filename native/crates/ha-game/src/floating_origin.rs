//! Global floating origin — observer-relative render coords (rocket + remote camera).

use bevy::prelude::*;
use ha_shared::{body_center_at, CelestialBody};

use crate::solar_system_clock::SolarSystemClock;
use crate::view_target::ViewTarget;

pub struct FloatingOriginPlugin;

impl Plugin for FloatingOriginPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FloatingOrigin>()
            .add_systems(Update, update_floating_origin);
    }
}

#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct FloatingOrigin {
    pub shift: Vec3,
}

impl FloatingOrigin {
    pub fn to_render(&self, world: Vec3) -> Vec3 {
        world - self.shift
    }

    pub fn from_render(&self, render: Vec3) -> Vec3 {
        render + self.shift
    }
}

const SHIFT_ON_DIST: f32 = 22_000.0;
const SHIFT_OFF_DIST: f32 = 15_000.0;
/// Settle rate (per second) — converted to a per-frame factor via dt so the
/// origin drifts back identically at 30, 60 or 150 fps (frame-independent).
const LERP_BACK_RATE: f32 = 7.5;

fn update_floating_origin(
    time: Res<Time>,
    clock: Res<SolarSystemClock>,
    view: Res<ViewTarget>,
    mut origin: ResMut<FloatingOrigin>,
) {
    let t = clock.t;
    let observer = view.observer_world;
    let anchor = view.origin_anchor(t);
    let rel = observer - anchor;
    let dist = rel.length();
    if dist > SHIFT_ON_DIST {
        origin.shift = rel;
    } else if dist < SHIFT_OFF_DIST {
        let k = 1.0 - (-LERP_BACK_RATE * time.delta_secs()).exp();
        origin.shift = origin.shift.lerp(Vec3::ZERO, k);
    }
}

/// Default anchor — dominant body centre near observer (Earth fallback).
pub fn earth_relative_anchor(observer: Vec3, t: f32) -> Vec3 {
    Vec3::from_array(body_center_at(CelestialBody::Earth, t))
}
