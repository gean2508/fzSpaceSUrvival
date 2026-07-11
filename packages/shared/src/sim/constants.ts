/** Fixed simulation rate. Authoritative server and offline client share this. */
export const TICK_RATE = 20;
export const TICK_MS = 1000 / TICK_RATE; // 50ms

/** Playable island half-extent (square). ~192×192 units — enormous but bounded (GTA-style sea limit). */
export const WORLD_HALF = 96;

/** Visual ocean extent (render only; movement clamped to WORLD_HALF). */
export const OCEAN_HALF = 384;

/** Curved-world vertex shader coefficient (native client). */
export const CURVED_WORLD_K = 0.00032;

/** Movement tuning shared by sim everywhere. */
export const FRICTION = 12; // velocity damping per second
export const MAX_SNAPSHOT_ENTITIES = 256;

// ---- Parkour / verticality (deterministic) ----
export const SPRINT_MULT = 1.6; // sprint speed multiplier
export const GRAVITY = 22; // units/s^2 applied when airborne
export const CLIMB_SPEED = 3.2; // vertical climb rate (units/s)
export const MAX_CLIMB_HEIGHT = 3.0; // tallest wall a player can climb
export const STEP_HEIGHT = 0.35; // walk over ledges below this without climbing
/** Minecraft block size (1 m cube). */
export const BLOCK_SIZE = 1.0;
/** Max vertical step between block columns while walking. */
export const BLOCK_STEP_HEIGHT = 1.05;
/** Max terrain grade the player can walk uphill (~24°). */
export const MAX_WALK_SLOPE = 0.42;
export const MANTLE_FORWARD = 0.6; // horizontal nudge when mantling onto a ledge

// ---- Stamina ----
export const STAMINA_MAX = 100;
export const STAMINA_REGEN = 18; // per second when not draining
export const STAMINA_SPRINT_DRAIN = 7; // per second while sprinting (~14s full bar)
export const STAMINA_CLIMB_DRAIN = 26; // per second while climbing/hanging
export const STAMINA_EXHAUSTED_RECOVER = 35; // must reach this to leave exhausted

/** Jump impulse (USE tap on flat ground; climb takes priority at walls). */
export const JUMP_VELOCITY = 6.8;
export const JUMP_STAMINA_COST = 6;
/** Grace period after leaving ground — jump while falling off ledges (seconds). */
export const COYOTE_TIME_SECS = 0.15;

/** Gravity multiplier per world context — deterministic everywhere. */
export const GRAVITY_SCALE: Record<import("./types.js").WorldContext, number> = {
  overworld: 1,
  mission: 1,
  /** Orbital hub — microgravity (arcade, not sim real). */
  space: 0.05,
  luna: 0.17,
};

/** Reserved flat zone for the launch pad on the island (no procedural props). */
export const LAUNCH_PAD_ZONE = { x: 28, z: -28, radius: 14 } as const;

// ---- Survival decay (per second) ----
export const HUNGER_MAX = 100;
export const THIRST_MAX = 100;
export const HUNGER_DECAY = 0.12;
export const THIRST_DECAY = 0.18;
/** Damage per second when hunger or thirst hit zero. */
export const STARVE_DAMAGE = 1.5;

// ---- Oxygen (vacuum bodies: Luna, space EVA, airless planets) ----
export const OXYGEN_MAX = 100;
/** Drain per second while exposed to vacuum (~83 s full tank). */
export const OXYGEN_DRAIN_VACUUM = 1.2;
/** Recharge per second inside a capsule / near a pad / in atmosphere. */
export const OXYGEN_RECHARGE = 25;
/** Damage per second once oxygen hits zero. */
export const SUFFOCATE_DAMAGE = 4;
