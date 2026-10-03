/**
 * Generic TS↔Rust catalog parity registry.
 * Add new content modules here — do NOT write bespoke parity tests per catalog.
 */
import { ASSET_CATALOG } from "../content/assets/catalog.js";
import { ITEM_CATALOG } from "../content/items/catalog.js";
import { MACHINE_CATALOG } from "../content/machines/catalog.js";
import { UI_ICON_CATALOG } from "../content/ui/icons.js";
import { PASE_TEMPORADA, TIENDA_CATALOGO, TIENDA_GEMAS, recompensaPase } from "../tienda.js";

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
  {
    // La tienda: precios, paquetes de gemas y las recompensas del pase. Si el
    // juego muestra un precio distinto del que cobra el servidor, esto lo caza.
    id: "tienda",
    getEntries: () => {
      const out: ParityEntry[] = TIENDA_CATALOGO.map((i) => ({
        id: i.id,
        fields: { tipo: i.tipo, gemas: i.gemas, monedas: i.monedas, da: i.da.join(",") },
      }));
      for (const g of TIENDA_GEMAS) {
        out.push({ id: `paquete:${g.id}`, fields: { gemas: g.gemas, extra: g.extra, centavos: g.centavos } });
      }
      for (let n = 1; n <= PASE_TEMPORADA.niveles; n++) {
        const r = recompensaPase(n);
        out.push({ id: `nivel:${String(n).padStart(2, "0")}`, fields: { gratis: r.gratis, premium: r.premium } });
      }
      out.push({
        id: `temporada:${PASE_TEMPORADA.id}`,
        fields: {
          niveles: PASE_TEMPORADA.niveles,
          xpPorNivel: PASE_TEMPORADA.xpPorNivel,
          gemasPremium: PASE_TEMPORADA.gemasPremium,
          desde: PASE_TEMPORADA.desde,
          hasta: PASE_TEMPORADA.hasta,
        },
      });
      return out.sort((a, b) => a.id.localeCompare(b.id));
    },
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
