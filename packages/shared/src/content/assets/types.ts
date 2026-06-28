/** Procedural asset taxonomy — code-generated meshes, no texture files. */

export type AssetClass =
  | "terrain"
  | "flora360"
  | "floraFlat"
  | "prop"
  | "item"
  | "character"
  | "creature"
  | "ui";

export type AssetCategory =
  | "terrain"
  | "flora"
  | "rock"
  | "construction"
  | "equipment"
  | "consumable"
  | "material"
  | "character"
  | "creature"
  | "ui";

/** Palette key into shared Ghibli/neon palette (TS sim + Rust ha-shared). */
export type PaletteKey =
  | "meadow"
  | "earth"
  | "bark"
  | "hero"
  | "foe"
  | "neonPink"
  | "neonCyan"
  | "night"
  | "fog"
  | "gold"
  | "crystal"
  | "potionRed"
  | "potionBlue"
  | "potionGreen"
  | "wood"
  | "stone"
  | "iron"
  | "leather";

export interface AssetRenderHints {
  /** Draw many instances with one InstancedMesh (terrain, small flora). */
  instanced: boolean;
  /** Y-axis billboard (items, small flora, UI drops). */
  billboard: boolean;
  /** Cast shadows (large 360° props). */
  shadows: boolean;
  /** Uniform scale applied at spawn. */
  scale: number;
}

export interface AssetDef {
  kind: string;
  class: AssetClass;
  category: AssetCategory;
  displayName: string;
  palette: PaletteKey;
  hints: AssetRenderHints;
  /** Tags for crafting / construction filters. */
  craftTags: string[];
}

export type ItemCategory =
  | "material"
  | "consumable"
  | "equipment"
  | "tool"
  | "weapon"
  | "key"
  | "misc";

export interface ItemDef {
  id: string;
  assetKind: string;
  displayName: string;
  category: ItemCategory;
  maxStack: number;
  craftTags: string[];
  /** Gather node that yields this item (sim). */
  gatherFrom?: "tree" | "rock" | "container";
}
