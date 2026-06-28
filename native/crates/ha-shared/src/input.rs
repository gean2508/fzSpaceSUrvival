//! Port of `packages/shared/src/sim/types.ts` InputState.

use serde::{Deserialize, Serialize};

pub const ABILITY_SLOTS: usize = 4;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct InputState {
    pub seq: u32,
    pub move_x: f32,
    pub move_z: f32,
    pub aim_x: f32,
    pub aim_z: f32,
    pub abilities: u32,
    pub sprint: u8,
    pub action: u8,
}

impl Default for InputState {
    fn default() -> Self {
        empty_input(0)
    }
}

pub fn empty_input(seq: u32) -> InputState {
    InputState {
        seq,
        move_x: 0.0,
        move_z: 0.0,
        aim_x: 0.0,
        aim_z: 1.0,
        abilities: 0,
        sprint: 0,
        action: 0,
    }
}
