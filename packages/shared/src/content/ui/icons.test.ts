import { describe, expect, it } from "vitest";
import { UI_ICON_CATALOG } from "./icons.js";
import { getUiIcon, requireUiIcon } from "./registry.js";

describe("ui icon catalog", () => {
  it("has unique kinds", () => {
    const kinds = UI_ICON_CATALOG.map((i) => i.kind);
    expect(new Set(kinds).size).toBe(kinds.length);
  });

  it("covers HUD stat variants", () => {
    for (const kind of [
      "heart_full",
      "heart_half",
      "heart_empty",
      "drumstick_full",
      "drumstick_half",
      "drumstick_empty",
    ]) {
      expect(getUiIcon(kind), kind).toBeDefined();
    }
  });

  it("requireUiIcon throws on unknown", () => {
    expect(() => requireUiIcon("nope")).toThrow();
  });
});
