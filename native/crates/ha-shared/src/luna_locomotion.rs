//! Lunar surface locomotion — delegates to unified sphere locomotion.

use crate::celestial_bodies::CelestialBody;
use crate::input::InputState;
use crate::movement::MovementBody;
use crate::surface_locomotion::step_surface_movement;

pub fn step_luna_movement(
    body: &mut MovementBody,
    input: &InputState,
    dt: f32,
    jump_pressed: bool,
    gravity_scale: f32,
) {
    step_surface_movement(
        CelestialBody::Moon,
        body,
        input,
        dt,
        jump_pressed,
        gravity_scale,
    );
}
