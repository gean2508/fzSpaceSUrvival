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
  /** Slot fired by quick tap on look zone (dash / cleave / fireball). */
  tapAbilitySlot: number;
}

export const CLASSES: Record<string, ClassDef> = {
  scout: {
    id: "scout",
    name: "Explorador",
    maxHp: 90,
    moveSpeed: 4.0,
    radius: 0.35,
    abilities: ["dagger", "dash"],
    tapAbilitySlot: 1,
  },
  warrior: {
    id: "warrior",
    name: "Guerrero",
    maxHp: 140,
    moveSpeed: 3.8,
    radius: 0.38,
    abilities: ["cleave", "charge"],
    tapAbilitySlot: 0,
  },
  mage: {
    id: "mage",
    name: "Mago",
    maxHp: 80,
    moveSpeed: 4.0,
    radius: 0.35,
    abilities: ["fireball", "frost_nova"],
    tapAbilitySlot: 0,
  },
};

export const DEFAULT_CLASS_ID = "scout";

export function getClass(id: string): ClassDef {
  return CLASSES[id] ?? CLASSES[DEFAULT_CLASS_ID];
}

export function classIds(): string[] {
  return Object.keys(CLASSES);
}
