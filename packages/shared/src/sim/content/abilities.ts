import type { AbilityDef } from "../abilities/types.js";

/** Ability registry. Edit numbers here to balance — never in sim code. */
export const ABILITIES: Record<string, AbilityDef> = {
  // Scout
  dagger: {
    id: "dagger",
    name: "Daga arrojadiza",
    kind: "projectile",
    cooldownMs: 320,
    damage: 12,
    range: 14,
    radius: 0.3,
    speed: 20,
    knockback: 1.5,
  },
  dash: {
    id: "dash",
    name: "Impulso",
    kind: "dash",
    cooldownMs: 2200,
    damage: 0,
    range: 5,
    radius: 0.45,
    speed: 22,
  },

  // Warrior
  cleave: {
    id: "cleave",
    name: "Tajo",
    kind: "melee",
    cooldownMs: 650,
    damage: 26,
    range: 1.9,
    radius: 0.6,
    arc: Math.PI / 2,
    knockback: 4,
  },
  charge: {
    id: "charge",
    name: "Embestida",
    kind: "dash",
    cooldownMs: 4000,
    damage: 22,
    range: 7,
    radius: 0.6,
    speed: 26,
    knockback: 6,
  },

  // Mage
  fireball: {
    id: "fireball",
    name: "Bola de fuego",
    kind: "projectile",
    cooldownMs: 900,
    damage: 16,
    range: 16,
    radius: 0.35,
    speed: 14,
    aoeRadius: 2.2,
    aoeDamage: 28,
  },
  frost_nova: {
    id: "frost_nova",
    name: "Nova de escarcha",
    kind: "aoe",
    cooldownMs: 4500,
    damage: 24,
    range: 0,
    radius: 3,
    knockback: 5,
  },
};

export function getAbility(id: string): AbilityDef | undefined {
  return ABILITIES[id];
}
