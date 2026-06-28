import { ITEM_CATALOG } from "./catalog.js";
import type { ItemDef } from "../assets/types.js";
import type { InteractableKind } from "../../sim/types.js";

const byId = new Map<string, ItemDef>(ITEM_CATALOG.map((i) => [i.id, i]));

export function getItem(id: string): ItemDef | undefined {
  return byId.get(id);
}

export function requireItem(id: string): ItemDef {
  const def = byId.get(id);
  if (!def) throw new Error(`Unknown item: ${id}`);
  return def;
}

export function itemsByCategory(category: ItemDef["category"]): ItemDef[] {
  return ITEM_CATALOG.filter((i) => i.category === category);
}

export function itemsByCraftTag(tag: string): ItemDef[] {
  return ITEM_CATALOG.filter((i) => i.craftTags.includes(tag));
}

export function itemForGather(kind: InteractableKind): ItemDef | undefined {
  return ITEM_CATALOG.find((i) => i.gatherFrom === kind);
}
