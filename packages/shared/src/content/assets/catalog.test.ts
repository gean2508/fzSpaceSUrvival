import { describe, expect, it } from "vitest";
import { ASSET_CATALOG } from "./catalog.js";
import { getAsset, requireAsset } from "./registry.js";
import { ITEM_CATALOG } from "../items/catalog.js";

describe("asset catalog", () => {
  it("has unique kinds", () => {
    const kinds = ASSET_CATALOG.map((a) => a.kind);
    expect(new Set(kinds).size).toBe(kinds.length);
  });

  it("covers sheet categories", () => {
    const categories = new Set(ASSET_CATALOG.map((a) => a.category));
    for (const cat of [
      "terrain",
      "flora",
      "rock",
      "construction",
      "equipment",
      "consumable",
      "material",
      "character",
      "creature",
      "ui",
    ]) {
      expect(categories.has(cat as never)).toBe(true);
    }
  });

  it("items reference valid asset kinds", () => {
    for (const item of ITEM_CATALOG) {
      expect(getAsset(item.assetKind), item.id).toBeDefined();
    }
  });

  it("requireAsset throws on unknown", () => {
    expect(() => requireAsset("nope")).toThrow();
  });
});
