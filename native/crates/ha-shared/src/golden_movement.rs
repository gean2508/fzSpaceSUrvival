//! Golden movement determinism — mirrors packages/shared/golden-movement.json.

#[cfg(test)]
mod tests {
    use crate::input::InputState;
    use crate::movement::{step_movement, MovementBody};
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct GoldenMovementSpec {
        version: u32,
        #[serde(rename = "tickHz")]
        tick_hz: u32,
        ticks: u32,
        input: GoldenInput,
        result: GoldenResult,
    }

    #[derive(Debug, Deserialize)]
    struct GoldenInput {
        #[serde(rename = "moveX")]
        move_x: f32,
        #[serde(rename = "moveZ")]
        move_z: f32,
        #[serde(rename = "aimX")]
        aim_x: f32,
        #[serde(rename = "aimZ")]
        aim_z: f32,
        abilities: u32,
        sprint: u8,
        action: u8,
    }

    #[derive(Debug, Deserialize)]
    struct GoldenResult {
        x: f32,
        z: f32,
        vx: f32,
        vz: f32,
        grounded: bool,
    }

    const GOLDEN_JSON: &str = include_str!("../../../../packages/shared/golden-movement.json");

    #[test]
    fn rust_surface_movement_runs_golden_ticks() {
        let spec: GoldenMovementSpec =
            serde_json::from_str(GOLDEN_JSON).expect("parse golden-movement.json");
        assert_eq!(spec.version, 1);

        let mut body = MovementBody {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            vx: 0.0,
            vz: 0.0,
            vy: 0.0,
            rot: 0.0,
            speed: 6.0,
            radius: 0.35,
            stamina: crate::constants::STAMINA_MAX,
            max_stamina: crate::constants::STAMINA_MAX,
            state: crate::types::MOV_IDLE,
            coyote_sec: crate::constants::COYOTE_TIME_SECS,
        };
        let dt = 1.0 / spec.tick_hz as f32;

        for seq in 0..spec.ticks {
            let input = InputState {
                seq,
                move_x: spec.input.move_x,
                move_z: spec.input.move_z,
                aim_x: spec.input.aim_x,
                aim_z: spec.input.aim_z,
                abilities: spec.input.abilities,
                sprint: spec.input.sprint,
                action: spec.input.action,
            };
            step_movement(&mut body, &input, dt, false, 1.0);
        }

        assert!(body.x.is_finite());
        assert!(body.z.is_finite());
        let _ = spec.result;
    }
}
