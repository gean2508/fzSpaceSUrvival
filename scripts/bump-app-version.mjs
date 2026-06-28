#!/usr/bin/env node
/**
 * Bump native Android version for manifest + landing.
 */
import { readFileSync, writeFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const VERSION_PATH = resolve(ROOT, "native/version.json");

function bumpPatch(semver) {
  const parts = semver.split("-")[0].split(".").map((n) => parseInt(n, 10) || 0);
  while (parts.length < 3) parts.push(0);
  parts[2] += 1;
  return parts.join(".");
}

const platform = process.argv[2] ?? "android";
const noBump = process.argv.includes("--no-bump");

if (noBump) process.exit(0);

if (platform === "android" || platform === "all") {
  const meta = JSON.parse(readFileSync(VERSION_PATH, "utf8"));
  const prevCode = meta.versionCode ?? 1;
  const prevVer = meta.version ?? "1.0.0";
  meta.versionCode = prevCode + 1;
  meta.version = bumpPatch(prevVer);
  writeFileSync(VERSION_PATH, JSON.stringify(meta, null, 2) + "\n");
  console.log(`✓ Native Android ${prevVer} (code ${prevCode}) → ${meta.version} (code ${meta.versionCode})`);
}
