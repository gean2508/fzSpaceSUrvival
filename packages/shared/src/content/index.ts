import { ASSET_CATALOG } from "./assets/catalog.js";
import { getAsset, requireAsset } from "./assets/registry.js";
import { ITEM_CATALOG } from "./items/catalog.js";
import type {
  AssetCategory,
  AssetClass,
  AssetDef,
  AssetRenderHints,
  ItemCategory,
  ItemDef,
  PaletteKey,
} from "./assets/types.js";

export type {
  AssetCategory,
  AssetClass,
  AssetDef,
  AssetRenderHints,
  ItemCategory,
  ItemDef,
  PaletteKey,
};

export {
  ASSET_CATALOG,
  ITEM_CATALOG,
  getAsset,
  requireAsset,
};

export {
  assetsByCategory,
  assetsByClass,
  assetsByCraftTag,
  assetKindForInteractable,
  assetKindForWorldProp,
  assetKindForObstacle,
  instancedAssetKinds,
  billboardAssetKinds,
} from "./assets/registry.js";

export {
  getItem,
  requireItem,
  itemForGather,
  itemsByCategory,
  itemsByCraftTag,
} from "./items/registry.js";

export {
  CRAFT_RECIPES,
  getCraftRecipe,
  canCraft,
  type CraftRecipe,
} from "./items/recipes.js";

export {
  MACHINE_CATALOG,
} from "./machines/catalog.js";

export {
  getMachine,
  requireMachine,
  machinesByOutputItem,
} from "./machines/registry.js";

export type { MachineDef, PlacedMachine, MachineSimState } from "./machines/types.js";

export {
  INTERACTION_RULES,
  INTERACT_REACH,
  GESTURE_HOLD_START_MS,
  GESTURE_TAP_MAX_MS,
  GESTURE_MOVE_CANCEL_PX,
  resolveInteraction,
  interactionDurationMs,
  type InteractTargetKind,
  type InteractActionKind,
  type InteractionRule,
} from "./interactions.js";
