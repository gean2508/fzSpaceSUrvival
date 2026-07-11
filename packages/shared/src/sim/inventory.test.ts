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
    const woodPick = CRAFT_RECIPES.find((r) => r.id === 2)!;
    const inv = createPlayerInventory({ wood: 3, sticks: 2 });
    expect(hasItems(inv, woodPick.ingredients)).toBe(true);
    inv.sticks = 1;
    expect(hasItems(inv, woodPick.ingredients)).toBe(false);
  });

  it("crafts when materials are sufficient", () => {
    const woodPick = CRAFT_RECIPES.find((r) => r.id === 2)!;
    const inv = createPlayerInventory({ wood: 3, sticks: 2 });
    expect(craftFromRecipe(inv, woodPick)).toBe(true);
    expect(getQty(inv, "wood_pickaxe")).toBe(1);
    expect(getQty(inv, "wood")).toBe(0);
    expect(getQty(inv, "sticks")).toBe(0);
  });

  it("rejects craft when materials are insufficient", () => {
    const woodPick = CRAFT_RECIPES.find((r) => r.id === 2)!;
    const inv = createPlayerInventory({ wood: 1, sticks: 2 });
    expect(craftFromRecipe(inv, woodPick)).toBe(false);
    expect(getQty(inv, "wood_pickaxe")).toBe(0);
    expect(getQty(inv, "wood")).toBe(1);
  });

  it("consumeItems is atomic", () => {
    const woodPick = CRAFT_RECIPES.find((r) => r.id === 2)!;
    const inv = createPlayerInventory({ wood: 3, sticks: 1 });
    expect(consumeItems(inv, woodPick.ingredients)).toBe(false);
    expect(getQty(inv, "wood")).toBe(3);
    expect(getQty(inv, "sticks")).toBe(1);
  });
});
