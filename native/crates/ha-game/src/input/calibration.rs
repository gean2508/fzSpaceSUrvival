//! Named tuning constants + helpers for stick curve, adaptive deadzone, look smoothing.

use bevy::prelude::Vec2;

/// Default 3D render resolution scale (Camera3d only; HUD stays native).
pub const DEFAULT_RENDER_SCALE: f32 = 1.0;

/// Stick magnitude curve: 1.0 = linear, ~1.7 = fine center + full edge speed.
pub const DEFAULT_STICK_CURVE_EXPONENT: f32 = 1.7;

/// Exponential smoothing rate for look drag (rad/s feel); ~10 = snappy, not laggy.
pub const CAMERA_LOOK_SMOOTH_RATE: f32 = 10.0;

/// Physical dead zone at stick center — fraction of stick radius in pixels.
pub const STICK_DEADZONE_RADIUS_FRAC: f32 = 0.10;

/// Clamp adaptive deadzone when converted to normalized stick space.
pub const STICK_DEADZONE_NORM_MIN: f32 = 0.05;
pub const STICK_DEADZONE_NORM_MAX: f32 = 0.18;

#[derive(Clone, Copy, Debug)]
pub struct InputCalibration {
    pub camera_sensitivity: f32,
    pub movement_sensitivity: f32,
    pub stick_curve_exponent: f32,
    pub render_scale: f32,
}

impl Default for InputCalibration {
    fn default() -> Self {
        Self {
            camera_sensitivity: 1.0,
            movement_sensitivity: 1.0,
            stick_curve_exponent: DEFAULT_STICK_CURVE_EXPONENT,
            // 0.75 regressed on Redmi (blit UI pass); keep 1.0 until blit path is cheaper.
            render_scale: 1.0,
        }
    }
}

impl InputCalibration {
    pub fn clamp(mut self) -> Self {
        self.camera_sensitivity = self.camera_sensitivity.clamp(0.5, 2.0);
        self.movement_sensitivity = self.movement_sensitivity.clamp(0.5, 2.0);
        self.stick_curve_exponent = self.stick_curve_exponent.clamp(1.0, 2.2);
        self.render_scale = self.render_scale.clamp(0.55, 1.0);
        self
    }
}

/// Deadzone derived from the stick's physical radius (pixels), not a fixed normalized constant.
pub fn adaptive_deadzone_norm(stick_radius_px: f32) -> f32 {
    if stick_radius_px <= 1.0 {
        return STICK_DEADZONE_NORM_MIN;
    }
    (STICK_DEADZONE_RADIUS_FRAC).clamp(STICK_DEADZONE_NORM_MIN, STICK_DEADZONE_NORM_MAX)
}

pub fn apply_stick_deadzone(v: Vec2, deadzone_norm: f32) -> Vec2 {
    let len = v.length();
    if len < deadzone_norm {
        return Vec2::ZERO;
    }
    let scaled = ((len - deadzone_norm) / (1.0 - deadzone_norm)).min(1.0);
    v.normalize() * scaled
}

/// Exponential response: fine control near center, full magnitude at the rim.
pub fn apply_stick_curve(magnitude: f32, exponent: f32) -> f32 {
    if magnitude <= 0.0 {
        return 0.0;
    }
    magnitude.clamp(0.0, 1.0).powf(exponent.max(1.0))
}

pub fn apply_stick_curve_vec(v: Vec2, exponent: f32) -> Vec2 {
    let len = v.length();
    if len <= 0.0 {
        return Vec2::ZERO;
    }
    let curved = apply_stick_curve(len, exponent);
    v.normalize() * curved
}
