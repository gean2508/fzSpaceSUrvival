pub mod calibration;
mod actions;
mod desktop_input;
mod joystick;
mod layout;
mod look_interact;
mod look_zone_ui;

pub use calibration::{
    apply_stick_curve_vec, adaptive_deadzone_norm, apply_stick_deadzone, InputCalibration,
    DEFAULT_RENDER_SCALE, DEFAULT_STICK_CURVE_EXPONENT, CAMERA_LOOK_SMOOTH_RATE,
};
pub use actions::{layout_action_buttons, CraftIntent, TouchActions};
pub use joystick::{CameraLookIntent, InputPlugin, MovementIntent};
pub use layout::{TouchBlockRects, TouchLayout};
pub use desktop_input::DesktopMouseCapture;
pub use look_interact::{
    AbilityCooldown, AbilityTapPulse, EquippedItem, HoldInteractState, LookInteractPlugin,
};
pub(crate) use look_interact::tick_hold_interact;
