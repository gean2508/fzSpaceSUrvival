//! Port of `packages/shared/src/sim/constants.ts`

pub const TICK_RATE: u32 = 20;
pub const TICK_MS: f32 = 1000.0 / TICK_RATE as f32;
/// Cap virtual frame delta — at 20 Hz fixed sim this allows at most 5 ticks per rendered frame.
pub const FIXED_MAX_FRAME_DELTA_MS: u64 = 250;

pub const WORLD_HALF: f32 = 96.0;

/// Visual ocean half-extent (render only; player clamped to `WORLD_HALF`).
pub const OCEAN_HALF: f32 = 384.0;

/// Animal Crossing–style horizon bend (metres drop per metre² horizontal distance).
pub const CURVED_WORLD_K: f32 = 0.00032;

pub const FRICTION: f32 = 12.0;
pub const MAX_SNAPSHOT_ENTITIES: usize = 256;

pub const SPRINT_MULT: f32 = 1.6;
pub const GRAVITY: f32 = 22.0;
pub const CLIMB_SPEED: f32 = 3.2;
pub const MAX_CLIMB_HEIGHT: f32 = 3.0;
pub const STEP_HEIGHT: f32 = 0.35;
/// Minecraft block size (1 m cube).
pub const BLOCK_SIZE: f32 = 1.0;
/// Max vertical step between block columns while walking.
pub const BLOCK_STEP_HEIGHT: f32 = 1.05;
/// While jumping / in coyote time, allow stepping onto ledges up to ~3 blocks.
pub const BLOCK_JUMP_STEP_HEIGHT: f32 = 3.05;
/// Heightmap lattice — legacy mesh; block terrain uses `BLOCK_SIZE` columns.
#[cfg(target_os = "android")]
pub const TERRAIN_MESH_GRID_STEP: f32 = 2.0;
#[cfg(not(target_os = "android"))]
pub const TERRAIN_MESH_GRID_STEP: f32 = 1.25;
/// Max terrain grade the player can walk uphill (~45°): tan(45°) = 1.0 rise/run.
pub const MAX_WALK_SLOPE: f32 = 1.0;
/// Horizontal sample distance for slope estimates (metres, tangent space).
pub const SLOPE_SAMPLE: f32 = 0.75;
pub const MANTLE_FORWARD: f32 = 0.6;

pub const STAMINA_MAX: f32 = 100.0;
pub const STAMINA_REGEN: f32 = 18.0;
pub const STAMINA_SPRINT_DRAIN: f32 = 7.0;
pub const STAMINA_CLIMB_DRAIN: f32 = 26.0;
pub const STAMINA_EXHAUSTED_RECOVER: f32 = 35.0;

/// Vertical impulse for jump (USE tap on flat ground).
pub const JUMP_VELOCITY: f32 = 6.8;
pub const JUMP_STAMINA_COST: f32 = 6.0;
/// Grace period after leaving ground — jump while falling off ledges (seconds).
pub const COYOTE_TIME_SECS: f32 = 0.15;

/// Gravity multiplier in orbital hub — mirror of TS `GRAVITY_SCALE.space`.
pub const GRAVITY_SPACE_SCALE: f32 = 0.05;

/// Gravity multiplier on Luna — mirror of TS `GRAVITY_SCALE.luna`.
pub const GRAVITY_LUNA_SCALE: f32 = 0.17;

/// Reserved flat zone for launch pad — mirror of TS `LAUNCH_PAD_ZONE`.
pub const LAUNCH_PAD_ZONE_X: f32 = 28.0;
pub const LAUNCH_PAD_ZONE_Z: f32 = -28.0;
/// Flat plateau + pad mesh footprint (~36 m diameter).
pub const LAUNCH_PAD_ZONE_RADIUS: f32 = 18.0;

/// Flat toroidal island today; Luna uses `MOON_GAMEPLAY_RADIUS_M` sphere shell.
/// Earth sphere migration: tangent coords + column height on shell (see `luna_terrain.rs`).
pub const EARTH_WALKABLE_SPHERE_RADIUS_M: f32 = 6400.0;

pub const HUNGER_MAX: f32 = 100.0;
pub const THIRST_MAX: f32 = 100.0;
pub const HUNGER_DECAY: f32 = 0.12;
pub const THIRST_DECAY: f32 = 0.18;
pub const STARVE_DAMAGE: f32 = 1.5;
