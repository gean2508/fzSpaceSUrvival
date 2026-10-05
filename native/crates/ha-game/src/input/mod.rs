use bevy::prelude::*;

pub mod calibration;
pub(crate) mod actions;
pub mod aim_stick;
pub mod desktop_input;
pub(crate) mod gamepad;
pub(crate) mod joystick;
pub(crate) mod layout;
mod look_interact;
mod world_touch;

/// Orden del puntero táctil: **dónde toco → qué toqué → qué significa**.
///
/// Los tres pasos ocurren en el MISMO frame a propósito. Si el gesto leyera un
/// objetivo calculado con el punto del frame anterior, el primer contacto se
/// resolvería contra lo que había antes de apoyar el dedo — y como ahora la
/// acción arranca en el acto (no hay retardo que disimule el desfase), eso se
/// vería como talar el árbol equivocado.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputSet {
    /// El punto del mundo bajo el dedo (`world_touch::track_world_touch`).
    Pointer,
    /// Qué acción significa ese toque (`world_touch::world_touch_gesture`).
    Gesture,
}

pub use calibration::DEFAULT_RENDER_SCALE;
pub use actions::{layout_action_buttons, ActionButton, ActionKind, ActionUiRoot, CraftIntent, TouchActions};
pub use joystick::{CameraLookIntent, InputPlugin, JoystickBase, MovementIntent};
pub(crate) use joystick::set_px;
pub use aim_stick::AimStick;
pub use layout::{touch_blocked, touch_in_move_zone, TouchBlockRects, TouchLayout};
pub use look_interact::{
    AbilityCooldown, AbilityTapPulse, BuildModeActive, EquippedItem, HoldInteractState,
    LookInteractPlugin,
};
pub use world_touch::{place_point, AimCursor, WorldTouchPlugin, AIM_MAX_REACH};
