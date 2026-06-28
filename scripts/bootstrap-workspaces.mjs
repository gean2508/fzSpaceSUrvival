#!/usr/bin/env node
/**
 * After `npm install`, build shared package if dist/ is missing.
 */
import { existsSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { execSync } from "node:child_process";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");

if (!existsSync(resolve(ROOT, "packages/shared/dist/index.js"))) {
  console.log("bootstrap: building @hunger-arena/shared");
  execSync("npm run build -w @hunger-arena/shared", { cwd: ROOT, stdio: "inherit" });
} else {
  console.log("bootstrap: shared dist OK");
}
