import { readFileSync, writeFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  buildCatalogParitySnapshot,
  CATALOG_PARITY_MODULES,
  type CatalogParitySnapshot,
} from "./catalog-parity.js";

const SNAPSHOT_PATH = resolve(
  dirname(fileURLToPath(import.meta.url)),
  "../../catalog-parity.snapshot.json"
);

function loadSnapshot(): CatalogParitySnapshot {
  return JSON.parse(readFileSync(SNAPSHOT_PATH, "utf8")) as CatalogParitySnapshot;
}

describe("catalog parity (TS snapshot)", () => {
  it("registry covers expected modules", () => {
    const ids = CATALOG_PARITY_MODULES.map((m) => m.id).sort();
    expect(ids).toEqual(["assets", "items", "machines", "tienda", "ui_icons"]);
  });

  it("each module has unique entry ids", () => {
    for (const mod of CATALOG_PARITY_MODULES) {
      const entries = mod.getEntries();
      const ids = entries.map((e) => e.id);
      expect(new Set(ids).size, mod.id).toBe(ids.length);
    }
  });

  it("matches committed snapshot (regenerate: UPDATE_CATALOG_PARITY=1 npm test)", () => {
    const live = buildCatalogParitySnapshot();
    if (process.env.UPDATE_CATALOG_PARITY === "1") {
      writeFileSync(SNAPSHOT_PATH, `${JSON.stringify(live, null, 2)}\n`);
      expect(true).toBe(true);
      return;
    }
    const snap = loadSnapshot();
    expect(live.modules.length).toBe(snap.modules.length);
    for (const liveMod of live.modules) {
      const snapMod = snap.modules.find((m) => m.id === liveMod.id);
      expect(snapMod, `missing module ${liveMod.id} in snapshot`).toBeDefined();
      expect(liveMod.entries).toEqual(snapMod!.entries);
    }
  });
});
