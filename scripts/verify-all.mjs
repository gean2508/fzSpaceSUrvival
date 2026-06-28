#!/usr/bin/env node
/**
 * Release health check — native client + server + landing.
 */
import { execSync } from "node:child_process";
import { existsSync, statSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");

function run(cmd, label) {
  console.log(`\n▶ ${label}`);
  execSync(cmd, { cwd: ROOT, stdio: "inherit" });
}

function checkFile(path, minBytes, label) {
  if (!existsSync(path)) {
    console.log(`✗ ${label}: missing (${path})`);
    return false;
  }
  const size = statSync(path).size;
  if (size < minBytes) {
    console.log(`✗ ${label}: too small (${(size / 1024).toFixed(0)} KB)`);
    return false;
  }
  console.log(`✓ ${label}: ${(size / 1_000_000).toFixed(1)} MB`);
  return true;
}

console.log("══ Hunger Arena verify ══");

run("npm run validate:arch", "Architecture boundaries");
run("npm run build:shared", "Shared sim package");
run("npm test", "Unit tests (shared sim)");
run("npm run test:native", "Native ha-shared tests");
run("npm run build -w @hunger-arena/client", "Landing build");
run("npm run build -w @hunger-arena/server", "Server build");

const artifacts = [
  [resolve(ROOT, "client/public/downloads/hunger-arena.apk"), 5_000_000, "Landing APK"],
  [resolve(ROOT, "server/updates/artifacts/hunger-arena.apk"), 5_000_000, "Server APK"],
];

let okArtifacts = 0;
for (const [p, min, label] of artifacts) {
  if (checkFile(p, min, label)) okArtifacts++;
}

if (existsSync(resolve(ROOT, "server/updates/artifacts/hunger-arena.apk"))) {
  try {
    run("npm run verify:apk", "APK pipeline integrity");
  } catch {
    process.exit(1);
  }
}

console.log(`\n══ Done: ${okArtifacts}/${artifacts.length} APK artifacts OK ══`);
if (okArtifacts < 2) {
  console.log("Tip: npm run ship:android");
  process.exit(1);
}
