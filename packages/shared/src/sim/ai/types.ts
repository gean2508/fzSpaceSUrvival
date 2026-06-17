export type AiState = "idle" | "wander" | "chase" | "attack" | "flee" | "return";

/** Behavioural traits that make each NPC feel distinct and "alive". */
export interface Personality {
  aggression: number; // 0..1 — how far it commits to chasing
  courage: number; // 0..1 — low flees early, high never flees
  visionRange: number;
  attackRange: number;
  attackDamage: number;
  attackCdMs: number;
}

/** Per-NPC AI memory (server-side only, deterministic). */
export interface NpcAi {
  state: AiState;
  targetId: string | null;
  personality: Personality;
  homeX: number;
  homeZ: number;
  wanderAngle: number;
  wanderUntilMs: number;
  nextAttackMs: number;
}
