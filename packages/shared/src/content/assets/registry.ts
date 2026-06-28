import type { InteractableKind, WorldPropKind } from "../../sim/types.js";
import { ASSET_CATALOG } from "./catalog.js";
import type { AssetCategory, AssetClass, AssetDef } from "./types.js";

const byKind = new Map<string, AssetDef>(
  ASSET_CATALOG.map((def) => [def.kind, def])
);

export function getAsset(kind: string): AssetDef | undefined {
  return byKind.get(kind);
}

export function requireAsset(kind: string): AssetDef {
  const def = byKind.get(kind);
  if (!def) throw new Error(`Unknown asset kind: ${kind}`);
  return def;
}

export function assetsByCategory(category: AssetCategory): AssetDef[] {
  return ASSET_CATALOG.filter((a) => a.category === category);
}

export function assetsByClass(assetClass: AssetClass): AssetDef[] {
  return ASSET_CATALOG.filter((a) => a.class === assetClass);
}

export function assetsByCraftTag(tag: string): AssetDef[] {
  return ASSET_CATALOG.filter((a) => a.craftTags.includes(tag));
}

/** Sim interactable → default procedural visual (variant alternates by seed). */
export function assetKindForInteractable(
  kind: InteractableKind,
  variant = 0
): string {
  switch (kind) {
    case "tree":
      return variant % 2 === 0 ? "flora_tree_oak" : "flora_tree_pine";
    case "rock":
      return variant % 2 === 0 ? "rock_medium" : "rock_boulder";
    case "container":
      return variant % 2 === 0 ? "prop_crate" : "prop_barrel";
    default:
      return "prop_crate";
  }
}

/** World decor props from path placement. */
export function assetKindForWorldProp(kind: WorldPropKind): string {
  switch (kind) {
    case "house":
      return "building_cottage";
    case "bridge":
      return "prop_bridge";
    default:
      return "prop_crate";
  }
}

/** Obstacle climb blocks — organic terrain mesh. */
export function assetKindForObstacle(): string {
  return "terrain_grass";
}

export function instancedAssetKinds(): string[] {
  return ASSET_CATALOG.filter((a) => a.hints.instanced).map((a) => a.kind);
}

export function billboardAssetKinds(): string[] {
  return ASSET_CATALOG.filter((a) => a.hints.billboard).map((a) => a.kind);
}
