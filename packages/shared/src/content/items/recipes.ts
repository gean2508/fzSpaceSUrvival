import type { InventoryItem } from "../../sim/types.js";

export interface CraftRecipe {
  id: number;
  key: string;
  displayName: string;
  ingredients: InventoryItem[];
  output: InventoryItem;
}

export const CRAFT_RECIPES: CraftRecipe[] = [
  {
    id: 1,
    key: "stone_pickaxe",
    displayName: "Stone Pickaxe",
    ingredients: [
      { itemId: "wood", qty: 3 },
      { itemId: "stone", qty: 2 },
      { itemId: "sticks", qty: 1 },
    ],
    output: { itemId: "pickaxe", qty: 1 },
  },
  {
    id: 2,
    key: "firefly_torch",
    displayName: "Firefly Torch",
    ingredients: [
      { itemId: "wood", qty: 2 },
      { itemId: "gem_blue", qty: 1 },
      { itemId: "potion_green", qty: 1 },
    ],
    output: { itemId: "potion_blue", qty: 1 },
  },
  {
    id: 4,
    key: "iron_ingot",
    displayName: "Iron Ingot",
    ingredients: [
      { itemId: "stone", qty: 3 },
      { itemId: "wood", qty: 1 },
    ],
    output: { itemId: "iron_ingot", qty: 1 },
  },
  {
    id: 5,
    key: "gold_ingot",
    displayName: "Gold Ingot",
    ingredients: [
      { itemId: "iron_ingot", qty: 2 },
      { itemId: "stone", qty: 2 },
    ],
        output: { itemId: "gold_ingot", qty: 1 },
  },
  {
    id: 6,
    key: "jetpack",
    displayName: "Jetpack",
    ingredients: [
      { itemId: "iron_ingot", qty: 2 },
      { itemId: "gold_ingot", qty: 1 },
      { itemId: "sticks", qty: 4 },
    ],
    output: { itemId: "jetpack", qty: 1 },
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
