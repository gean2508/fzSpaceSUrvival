import { describe, expect, it } from "vitest";
import {
  INTERACTION_RULES,
  interactionDurationMs,
  resolveInteraction,
} from "./interactions.js";

describe("interaction matrix", () => {
  it("has unique equipped×target keys", () => {
    const keys = INTERACTION_RULES.map((r) => `${r.equippedItemId}:${r.targetKind}`);
    expect(new Set(keys).size).toBe(keys.length);
  });

  it("pickaxe mines rock into stone", () => {
    const rule = resolveInteraction("pickaxe", "rock");
    expect(rule?.kind).toBe("mine");
    expect(rule?.durationMs).toBe(800);
    expect(rule?.itemId).toBe("stone");
  });

  it("axe gathers wood from tree", () => {
    const rule = resolveInteraction("axe", "tree");
    expect(rule?.kind).toBe("gather");
    expect(rule?.itemId).toBe("wood");
  });

  it("sword attacks entity", () => {
    const rule = resolveInteraction("sword_iron", "entity");
    expect(rule?.kind).toBe("attack");
    expect(rule?.damage).toBeGreaterThan(0);
  });

  it("watering can collects water", () => {
    const rule = resolveInteraction("watering_can", "water");
    expect(rule?.kind).toBe("collect");
    expect(rule?.itemId).toBe("supply");
  });

  it("empty hand on rock does nothing", () => {
    const rule = resolveInteraction("hand", "rock");
    expect(rule?.kind).toBe("none");
    expect(rule?.durationMs).toBe(0);
  });

  it("empty hand falls back when equipped unknown", () => {
    expect(resolveInteraction(null, "tree")?.durationMs).toBe(2200);
  });

  it("duration is deterministic for fixed pairs", () => {
    expect(interactionDurationMs("pickaxe", "rock")).toBe(800);
    expect(interactionDurationMs("pickaxe", "rock")).toBe(800);
  });

  it("launch pad interactions exist", () => {
    expect(INTERACTION_RULES.length).toBe(14);
    expect(resolveInteraction("hand", "launch_module")?.kind).toBe("assemble");
    expect(resolveInteraction("hand", "launch_ignition")?.kind).toBe("launch");
    expect(resolveInteraction("hand", "lunar_return")?.kind).toBe("return_earth");
  });
});
