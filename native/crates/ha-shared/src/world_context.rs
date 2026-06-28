//! World context + gravity scale — mirror of `packages/shared/src/sim/world_context.ts`.

use crate::constants::{GRAVITY, GRAVITY_LUNA_SCALE, GRAVITY_SPACE_SCALE};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WorldContext {
    #[default]
    Overworld,
    Mission,
    Space,
    Luna,
    /// Walkable shell on Mercury, Mars, Titan, etc. — sim uses per-body gravity.
    Planet,
}

impl WorldContext {
    pub fn gravity_scale(self) -> f32 {
        match self {
            WorldContext::Luna => GRAVITY_LUNA_SCALE,
            WorldContext::Space => GRAVITY_SPACE_SCALE,
            WorldContext::Planet => GRAVITY_LUNA_SCALE,
            WorldContext::Overworld | WorldContext::Mission => 1.0,
        }
    }

    pub fn gravity(self) -> f32 {
        GRAVITY * self.gravity_scale()
    }
}

pub fn gravity_scale_for_context(ctx: WorldContext) -> f32 {
    ctx.gravity_scale()
}

pub fn gravity_for_context(ctx: WorldContext) -> f32 {
    ctx.gravity()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luna_gravity_scale_matches_ts() {
        assert_eq!(GRAVITY_LUNA_SCALE, 0.17);
        assert_eq!(GRAVITY_SPACE_SCALE, 0.05);
        assert!((gravity_for_context(WorldContext::Luna) - GRAVITY * 0.17).abs() < 0.001);
        assert!((gravity_for_context(WorldContext::Space) - GRAVITY * 0.05).abs() < 0.001);
        assert_eq!(gravity_for_context(WorldContext::Overworld), GRAVITY);
    }
}
