/**
 * Character class definitions (data-driven). Abilities are added in Phase 3;
 * Phase 1 uses the movement/stat fields. Keep this as the single source of
 * truth shared by server and client.
 */
export interface ClassDef {
  id: string;
  name: string;
  maxHp: number;
  moveSpeed: number; // units per second
  radius: number; // collision radius
  /** Ability ids per slot (filled in Phase 3). */
  abilities: string[];
}

export const CLASSES: Record<string, ClassDef> = {
  scout: {
    id: "scout",
    name: "Explorador",
    maxHp: 90,
    moveSpeed: 6.5,
    radius: 0.45,
    abilities: ["dagger", "dash"],
  },
  warrior: {
    id: "warrior",
    name: "Guerrero",
    maxHp: 140,
    moveSpeed: 5,
    radius: 0.5,
    abilities: ["cleave", "charge"],
  },
  mage: {
    id: "mage",
    name: "Mago",
    maxHp: 80,
    moveSpeed: 5.5,
    radius: 0.45,
    abilities: ["fireball", "frost_nova"],
  },
};

export const DEFAULT_CLASS_ID = "scout";

export function getClass(id: string): ClassDef {
  return CLASSES[id] ?? CLASSES[DEFAULT_CLASS_ID];
}

export function classIds(): string[] {
  return Object.keys(CLASSES);
}
