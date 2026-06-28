import { describe, expect, it } from "vitest";
import { CRAFT_RECIPES } from "../content/items/recipes.js";
import {
  addItem,
  consumeItems,
  craftFromRecipe,
  createPlayerInventory,
  getQty,
  hasItems,
  removeItem,
} from "./inventory.js";

describe("PlayerInventory", () => {
  it("adds and reads quantities", () => {
    const inv = createPlayerInventory({});
    addItem(inv, "wood", 3);
    addItem(inv, "wood", 2);
    expect(getQty(inv, "wood")).toBe(5);
    expect(getQty(inv, "stone")).toBe(0);
  });

  it("removes items when sufficient stock", () => {
    const inv = createPlayerInventory({ wood: 5, stone: 2 });
    expect(removeItem(inv, "wood", 3)).toBe(true);
    expect(getQty(inv, "wood")).toBe(2);
    expect(removeItem(inv, "stone", 3)).toBe(false);
    expect(getQty(inv, "stone")).toBe(2);
  });

  it("clears slot when quantity reaches zero", () => {
    const inv = createPlayerInventory({ sticks: 1 });
    expect(removeItem(inv, "sticks", 1)).toBe(true);
    expect(inv.sticks).toBeUndefined();
  });

  it("checks recipe affordability", () => {
    const pickaxe = CRAFT_RECIPES.find((r) => r.id === 1)!;
    const inv = createPlayerInventory({ wood: 3, stone: 2, sticks: 1 });
    expect(hasItems(inv, pickaxe.ingredients)).toBe(true);
    inv.stone = 1;
    expect(hasItems(inv, pickaxe.ingredients)).toBe(false);
  });

  it("crafts when materials are sufficient", () => {
    const pickaxe = CRAFT_RECIPES.find((r) => r.id === 1)!;
    const inv = createPlayerInventory({ wood: 3, stone: 2, sticks: 1 });
    expect(craftFromRecipe(inv, pickaxe)).toBe(true);
    expect(getQty(inv, "pickaxe")).toBe(1);
    expect(getQty(inv, "wood")).toBe(0);
    expect(getQty(inv, "stone")).toBe(0);
    expect(getQty(inv, "sticks")).toBe(0);
  });

  it("rejects craft when materials are insufficient", () => {
    const pickaxe = CRAFT_RECIPES.find((r) => r.id === 1)!;
    const inv = createPlayerInventory({ wood: 1, stone: 2, sticks: 1 });
    expect(craftFromRecipe(inv, pickaxe)).toBe(false);
    expect(getQty(inv, "pickaxe")).toBe(0);
    expect(getQty(inv, "wood")).toBe(1);
  });

  it("consumeItems is atomic", () => {
    const inv = createPlayerInventory({ wood: 3, stone: 1, sticks: 1 });
    const reqs = CRAFT_RECIPES[0]!.ingredients;
    expect(consumeItems(inv, reqs)).toBe(false);
    expect(getQty(inv, "wood")).toBe(3);
    expect(getQty(inv, "stone")).toBe(1);
  });
});
