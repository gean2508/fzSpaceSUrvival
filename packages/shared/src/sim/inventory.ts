import type { CraftRecipe } from "../content/items/recipes.js";
import type { InventoryItem } from "./types.js";

/** Deterministic item map — itemId → quantity. */
export type PlayerInventoryMap = Record<string, number>;

/** Minecraft-style cold start: empty hands. Punch trees to begin. */
export const DEFAULT_STARTER_INVENTORY: PlayerInventoryMap = {};

export function createPlayerInventory(initial?: PlayerInventoryMap): PlayerInventoryMap {
  return { ...(initial ?? DEFAULT_STARTER_INVENTORY) };
}

export function getQty(inv: PlayerInventoryMap, itemId: string): number {
  return inv[itemId] ?? 0;
}

export function addItem(inv: PlayerInventoryMap, itemId: string, qty: number): void {
  if (qty <= 0) return;
  inv[itemId] = getQty(inv, itemId) + qty;
}

export function removeItem(inv: PlayerInventoryMap, itemId: string, qty: number): boolean {
  if (qty <= 0) return true;
  const has = getQty(inv, itemId);
  if (has < qty) return false;
  const next = has - qty;
  if (next === 0) delete inv[itemId];
  else inv[itemId] = next;
  return true;
}

export function hasItems(inv: PlayerInventoryMap, requirements: InventoryItem[]): boolean {
  for (const req of requirements) {
    if (getQty(inv, req.itemId) < req.qty) return false;
  }
  return true;
}

export function consumeItems(inv: PlayerInventoryMap, requirements: InventoryItem[]): boolean {
  if (!hasItems(inv, requirements)) return false;
  for (const req of requirements) {
    removeItem(inv, req.itemId, req.qty);
  }
  return true;
}

export function craftFromRecipe(inv: PlayerInventoryMap, recipe: CraftRecipe): boolean {
  if (!consumeItems(inv, recipe.ingredients)) return false;
  addItem(inv, recipe.output.itemId, recipe.output.qty);
  return true;
}

export function toInventoryList(inv: PlayerInventoryMap): InventoryItem[] {
  return Object.entries(inv).map(([itemId, qty]) => ({ itemId, qty }));
}
