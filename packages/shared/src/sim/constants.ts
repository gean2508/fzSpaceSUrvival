/** Fixed simulation rate. Authoritative server and offline client share this. */
export const TICK_RATE = 20;
export const TICK_MS = 1000 / TICK_RATE; // 50ms

/** Square arena: world spans [-WORLD_HALF, WORLD_HALF] on X and Z. */
export const WORLD_HALF = 20;

/** Movement tuning shared by sim everywhere. */
export const FRICTION = 12; // velocity damping per second
export const MAX_SNAPSHOT_ENTITIES = 256;

// ---- Parkour / verticality (deterministic) ----
export const SPRINT_MULT = 1.6; // sprint speed multiplier
export const GRAVITY = 22; // units/s^2 applied when airborne
export const CLIMB_SPEED = 3.2; // vertical climb rate (units/s)
export const MAX_CLIMB_HEIGHT = 3.0; // tallest wall a player can climb
export const STEP_HEIGHT = 0.35; // walk over ledges below this without climbing
export const MANTLE_FORWARD = 0.6; // horizontal nudge when mantling onto a ledge

// ---- Stamina ----
export const STAMINA_MAX = 100;
export const STAMINA_REGEN = 14; // per second when not draining
export const STAMINA_SPRINT_DRAIN = 12; // per second while sprinting
export const STAMINA_CLIMB_DRAIN = 26; // per second while climbing/hanging
export const STAMINA_EXHAUSTED_RECOVER = 35; // must reach this to leave exhausted

// ---- Survival decay (per second) ----
export const HUNGER_MAX = 100;
export const THIRST_MAX = 100;
export const HUNGER_DECAY = 0.12;
export const THIRST_DECAY = 0.18;
/** Damage per second when hunger or thirst hit zero. */
export const STARVE_DAMAGE = 1.5;
