#!/usr/bin/env node
/** Regenerate catalog-parity.snapshot.json from TS catalogs (source of truth). */
import { spawnSync } from "node:child_process";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const res = spawnSync(
  "npm",
  ["test", "-w", "@hunger-arena/shared", "--", "src/test-utils/catalog-parity.test.ts"],
  {
    cwd: ROOT,
    stdio: "inherit",
    env: { ...process.env, UPDATE_CATALOG_PARITY: "1" },
  }
);

process.exit(res.status ?? 1);
