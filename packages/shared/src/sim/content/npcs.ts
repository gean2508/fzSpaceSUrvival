import type { Personality } from "../ai/types.js";

/** Self-contained NPC archetype (does not reuse player classes). */
export interface NpcArchetype {
  id: string;
  name: string;
  maxHp: number;
  moveSpeed: number;
  radius: number;
  personality: Personality;
}

export const NPC_ARCHETYPES: Record<string, NpcArchetype> = {
  grunt: {
    id: "grunt",
    name: "Recluta",
    maxHp: 42,
    moveSpeed: 4.2,
    radius: 0.45,
    personality: {
      aggression: 0.6,
      courage: 0.5,
      visionRange: 9,
      attackRange: 1.3,
      attackDamage: 7,
      attackCdMs: 800,
    },
  },
  skirmisher: {
    id: "skirmisher",
    name: "Hostigador",
    maxHp: 28,
    moveSpeed: 5.2,
    radius: 0.4,
    personality: {
      aggression: 0.5,
      courage: 0.2,
      visionRange: 12,
      attackRange: 1.2,
      attackDamage: 6,
      attackCdMs: 650,
    },
  },
  brute: {
    id: "brute",
    name: "Bruto",
    maxHp: 110,
    moveSpeed: 3.1,
    radius: 0.65,
    personality: {
      aggression: 0.95,
      courage: 0.95,
      visionRange: 8,
      attackRange: 1.7,
      attackDamage: 15,
      attackCdMs: 1200,
    },
  },
};

export function getArchetype(id: string): NpcArchetype {
  return NPC_ARCHETYPES[id] ?? NPC_ARCHETYPES.grunt;
}

export function archetypeIds(): string[] {
  return Object.keys(NPC_ARCHETYPES);
}
