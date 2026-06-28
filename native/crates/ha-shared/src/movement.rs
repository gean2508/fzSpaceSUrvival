//! Player locomotion on SFS-style sphere surfaces.

use crate::celestial_bodies::CelestialBody;
use crate::input::InputState;
use crate::surface_locomotion::step_surface_movement;

#[derive(Clone, Debug)]
pub struct MovementBody {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub vx: f32,
    pub vz: f32,
    pub vy: f32,
    pub rot: f32,
    pub speed: f32,
    pub radius: f32,
    pub stamina: f32,
    pub max_stamina: f32,
    pub state: &'static str,
    pub coyote_sec: f32,
}

impl MovementBody {
    pub fn new(speed: f32, radius: f32) -> Self {
        use crate::constants::STAMINA_MAX;
        use crate::types::MOV_IDLE;
        Self {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            vx: 0.0,
            vz: 0.0,
            vy: 0.0,
            rot: 0.0,
            speed,
            radius,
            stamina: STAMINA_MAX,
            max_stamina: STAMINA_MAX,
            state: MOV_IDLE,
            coyote_sec: crate::constants::COYOTE_TIME_SECS,
        }
    }
}

pub fn step_movement(
    body: &mut MovementBody,
    input: &InputState,
    dt: f32,
    jump_pressed: bool,
    gravity_scale: f32,
) {
    step_surface_movement(
        CelestialBody::Earth,
        body,
        input,
        dt,
        jump_pressed,
        gravity_scale,
    );
}

pub fn clamp_body(_e: &mut MovementBody) {
    // Stereographic locomotion covers the full sphere — no tangent box clamp.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::empty_input;

    #[test]
    fn friction_moves_toward_target() {
        let mut b = MovementBody::new(5.0, 0.5);
        let mut input = empty_input(1);
        input.move_x = 1.0;
        step_movement(&mut b, &input, 0.05, false, 1.0);
        assert!(b.vx > 0.0);
        assert!(b.x.hypot(b.z) > 0.0, "expected geodesic displacement");
    }
}
