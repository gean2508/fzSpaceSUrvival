//! Port of `packages/shared/src/sim/types.ts` (subset used by native client).

use serde::{Deserialize, Serialize};

pub type EntityKind = &'static str;
pub const ENTITY_PLAYER: EntityKind = "player";
pub const ENTITY_NPC: EntityKind = "npc";
pub const ENTITY_PROJECTILE: EntityKind = "projectile";

pub type MovementState = &'static str;
pub const MOV_IDLE: MovementState = "idle";
pub const MOV_RUNNING: MovementState = "running";
pub const MOV_SPRINTING: MovementState = "sprinting";
/// Cruzando líquido de poco fondo ([`crate::topdown::Tile::swim`]): más lento, sin
/// esprintar y con la pose `swim_*` del personaje. Si el pack de arte no trae esa
/// pose, el personaje sigue con la de caminar — nunca se queda sin sprite.
pub const MOV_SWIMMING: MovementState = "swimming";
pub const MOV_WALL_TREK: MovementState = "wall_trek";
pub const MOV_CLIMBING: MovementState = "climbing";
pub const MOV_HANGING: MovementState = "hanging";
pub const MOV_EXHAUSTED: MovementState = "exhausted";

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f32,
    pub z: f32,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Obstacle {
    pub x: f32,
    pub z: f32,
    pub half_w: f32,
    pub half_d: f32,
    pub height: f32,
}
