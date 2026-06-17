export type AbilityKind = "projectile" | "melee" | "aoe" | "dash";

/**
 * Data-driven ability definition. All combat numbers live here so balancing is
 * a data edit, not a code change. Damage is always applied server-side.
 */
export interface AbilityDef {
  id: string;
  name: string;
  kind: AbilityKind;
  cooldownMs: number;
  damage: number;
  /** Effective range (projectile travel cap, melee reach, dash distance). */
  range: number;
  /** Hit radius (projectile size, aoe radius, melee half-width). */
  radius: number;
  /** Projectile/dash speed (units/sec). */
  speed?: number;
  /** Melee arc half-angle (radians). */
  arc?: number;
  /** Knockback impulse applied to victims. */
  knockback?: number;
  /** Projectiles only: explode into an AoE on impact/expire. */
  aoeRadius?: number;
  aoeDamage?: number;
}
