//! Surface locomotion hints — block parkour removed (SFS sphere worlds).

use crate::movement::MovementBody;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParkourButtonHint {
    Jump,
    Climb,
    Mantle,
}

pub fn parkour_button_hint(_body: &MovementBody) -> ParkourButtonHint {
    ParkourButtonHint::Jump
}
