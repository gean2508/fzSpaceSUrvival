/**
 * Generic TS↔Rust catalog parity registry.
 * Add new content modules here — do NOT write bespoke parity tests per catalog.
 */
import { ASSET_CATALOG } from "../content/assets/catalog.js";
import { ITEM_CATALOG } from "../content/items/catalog.js";
import { MACHINE_CATALOG } from "../content/machines/catalog.js";
import { UI_ICON_CATALOG } from "../content/ui/icons.js";

export type ParityEntry = {
  id: string;
  fields: Record<string, string | number | boolean>;
};

export type ParityModuleSpec = {
  /** Stable module id — must match Rust catalog_parity module id. */
  id: string;
  getEntries: () => ParityEntry[];
};

export const CATALOG_PARITY_MODULES: ParityModuleSpec[] = [
  {
    id: "assets",
    getEntries: () =>
      ASSET_CATALOG.map((a) => ({
        id: a.kind,
        fields: { class: a.class, category: a.category, palette: a.palette },
      })).sort((a, b) => a.id.localeCompare(b.id)),
  },
  {
    id: "items",
    getEntries: () =>
      ITEM_CATALOG.map((i) => ({
        id: i.id,
        fields: { assetKind: i.assetKind, category: i.category, maxStack: i.maxStack },
      })).sort((a, b) => a.id.localeCompare(b.id)),
  },
  {
    id: "machines",
    getEntries: () =>
      MACHINE_CATALOG.map((m) => ({
        id: m.id,
        fields: {
          passive: m.passive,
          outputPerSecond: m.outputPerSecond,
          placeDurationMs: m.placeDurationMs,
          assetKind: m.assetKind,
        },
      })).sort((a, b) => a.id.localeCompare(b.id)),
  },
  {
    id: "ui_icons",
    getEntries: () =>
      UI_ICON_CATALOG.map((i) => ({
        id: i.kind,
        fields: {
          shape: i.shape,
          variant: i.variant,
          fillPalette: i.fillPalette,
          bgPalette: i.bgPalette,
          grid: i.grid,
          cellCount: i.cells.length,
        },
      })).sort((a, b) => a.id.localeCompare(b.id)),
  },
];

export type CatalogParitySnapshot = {
  version: 1;
  generatedAt: string;
  modules: { id: string; entries: ParityEntry[] }[];
};

export function buildCatalogParitySnapshot(): CatalogParitySnapshot {
  return {
    version: 1,
    generatedAt: new Date().toISOString(),
    modules: CATALOG_PARITY_MODULES.map((m) => ({
      id: m.id,
      entries: m.getEntries(),
    })),
  };
}
