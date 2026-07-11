import type { InventoryItem } from "../../sim/types.js";

export interface CraftRecipe {
  id: number;
  key: string;
  displayName: string;
  ingredients: InventoryItem[];
  output: InventoryItem;
}

// Minecraft-style tech tree — MUST mirror native/crates/ha-shared/src/recipes.rs.
export const CRAFT_RECIPES: CraftRecipe[] = [
  {
    id: 1,
    key: "sticks",
    displayName: "Sticks",
    ingredients: [{ itemId: "wood", qty: 2 }],
    output: { itemId: "sticks", qty: 4 },
  },
  {
    id: 2,
    key: "wood_pickaxe",
    displayName: "Wood Pickaxe",
    ingredients: [
      { itemId: "wood", qty: 3 },
      { itemId: "sticks", qty: 2 },
    ],
    output: { itemId: "wood_pickaxe", qty: 1 },
  },
  {
    id: 3,
    key: "wood_axe",
    displayName: "Wood Axe",
    ingredients: [
      { itemId: "wood", qty: 3 },
      { itemId: "sticks", qty: 2 },
    ],
    output: { itemId: "wood_axe", qty: 1 },
  },
  {
    id: 4,
    key: "stone_pickaxe",
    displayName: "Stone Pickaxe",
    ingredients: [
      { itemId: "stone", qty: 3 },
      { itemId: "sticks", qty: 2 },
    ],
    output: { itemId: "stone_pickaxe", qty: 1 },
  },
  {
    id: 5,
    key: "stone_axe",
    displayName: "Stone Axe",
    ingredients: [
      { itemId: "stone", qty: 3 },
      { itemId: "sticks", qty: 2 },
    ],
    output: { itemId: "stone_axe", qty: 1 },
  },
  {
    id: 6,
    key: "iron_ingot",
    displayName: "Iron Ingot",
    ingredients: [
      { itemId: "stone", qty: 3 },
      { itemId: "wood", qty: 1 },
    ],
    output: { itemId: "iron_ingot", qty: 1 },
  },
  {
    id: 7,
    key: "gold_ingot",
    displayName: "Gold Ingot",
    ingredients: [
      { itemId: "iron_ingot", qty: 2 },
      { itemId: "stone", qty: 2 },
    ],
    output: { itemId: "gold_ingot", qty: 1 },
  },
  {
    id: 8,
    key: "lantern",
    displayName: "Lantern",
    ingredients: [
      { itemId: "wood", qty: 2 },
      { itemId: "iron_ingot", qty: 1 },
      { itemId: "sticks", qty: 2 },
    ],
    output: { itemId: "lantern", qty: 1 },
  },
  {
    id: 9,
    key: "firefly_torch",
    displayName: "Firefly Torch",
    ingredients: [
      { itemId: "wood", qty: 2 },
      { itemId: "gem_blue", qty: 1 },
      { itemId: "sticks", qty: 2 },
    ],
    output: { itemId: "potion_blue", qty: 1 },
  },
  {
    id: 10,
    key: "wood_shield",
    displayName: "Wood Shield",
    ingredients: [
      { itemId: "wood", qty: 4 },
      { itemId: "iron_ingot", qty: 1 },
    ],
    output: { itemId: "shield_wood", qty: 1 },
  },
  {
    id: 11,
    key: "jetpack",
    displayName: "Jetpack",
    ingredients: [
      { itemId: "iron_ingot", qty: 2 },
      { itemId: "gold_ingot", qty: 1 },
      { itemId: "sticks", qty: 4 },
    ],
    output: { itemId: "jetpack", qty: 1 },
  },
  {
    id: 12,
    key: "nav_probe",
    displayName: "Nav Probe",
    ingredients: [
      { itemId: "iron_ingot", qty: 2 },
      { itemId: "gold_ingot", qty: 1 },
      { itemId: "stone", qty: 2 },
    ],
    output: { itemId: "probe", qty: 1 },
  },
];

const byId = new Map<number, CraftRecipe>(CRAFT_RECIPES.map((r) => [r.id, r]));

export function getCraftRecipe(recipeId: number): CraftRecipe | undefined {
  return byId.get(recipeId);
}

export function canCraft(inventory: InventoryItem[], recipe: CraftRecipe): boolean {
  for (const req of recipe.ingredients) {
    const has = inventory.find((i) => i.itemId === req.itemId)?.qty ?? 0;
    if (has < req.qty) return false;
  }
  return true;
}
