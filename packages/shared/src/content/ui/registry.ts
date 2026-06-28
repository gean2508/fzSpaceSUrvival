import { UI_ICON_CATALOG } from "./icons.js";
import type { UiIconDef } from "./types.js";

export function getUiIcon(kind: string): UiIconDef | undefined {
  return UI_ICON_CATALOG.find((i) => i.kind === kind);
}

export function requireUiIcon(kind: string): UiIconDef {
  const icon = getUiIcon(kind);
  if (!icon) throw new Error(`unknown ui icon: ${kind}`);
  return icon;
}
