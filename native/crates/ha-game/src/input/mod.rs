pub mod calibration;
mod actions;
pub mod desktop_input;
pub(crate) mod gamepad;
mod joystick;
mod layout;
mod look_interact;

pub use calibration::{
    apply_stick_curve_vec, adaptive_deadzone_norm, apply_stick_deadzone, InputCalibration,
    DEFAULT_RENDER_SCALE, DEFAULT_STICK_CURVE_EXPONENT, CAMERA_LOOK_SMOOTH_RATE,
};
pub use actions::{layout_action_buttons, CraftIntent, TouchActions};
pub use joystick::{AimCursor, CameraLookIntent, InputPlugin, MovementIntent, AIM_MAX_REACH};
pub use layout::{TouchBlockRects, TouchLayout};
pub use desktop_input::DesktopMouseCapture;
pub use look_interact::{
    AbilityCooldown, AbilityTapPulse, BuildModeActive, EquippedItem, HoldInteractState,
    LookInteractPlugin, LookPointer,
};
pub(crate) use look_interact::tick_hold_interact;
