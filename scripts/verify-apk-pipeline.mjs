#!/usr/bin/env node
/**
 * Audit APK build → publish → landing sync integrity (CI / pre-ship).
 * Exit 1 on any mismatch.
 */
import { createHash } from "node:crypto";
import { existsSync, readFileSync, statSync } from "node:fs";
import { resolve } from "node:path";
import { spawnSync } from "node:child_process";

const ROOT = resolve(import.meta.dirname, "..");
const SERVER_APK = resolve(ROOT, "server/updates/artifacts/hunger-arena.apk");
const LANDING_APK = resolve(ROOT, "client/public/downloads/hunger-arena.apk");
const MANIFEST = resolve(ROOT, "server/updates/manifest.json");
const REQUIRE_RELEASE_SIGN =
  process.env.APK_REQUIRE_RELEASE_SIGN === "1" ||
  process.env.APK_REQUIRE_RELEASE_SIGN === "true";

function sha256File(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function fail(msg) {
  console.error(`✗ ${msg}`);
  process.exit(1);
}

function ok(msg) {
  console.log(`✓ ${msg}`);
}

function commandMissing(result) {
  return result.error && typeof result.error === "object" && "code" in result.error && result.error.code === "ENOENT";
}

if (!existsSync(SERVER_APK)) fail(`missing ${SERVER_APK}`);
if (!existsSync(LANDING_APK)) fail(`missing ${LANDING_APK}`);

const serverSha = sha256File(SERVER_APK);
const landingSha = sha256File(LANDING_APK);
const serverSize = statSync(SERVER_APK).size;
const landingSize = statSync(LANDING_APK).size;

if (serverSha !== landingSha) {
  fail(`SHA256 mismatch server vs landing\n  server:  ${serverSha}\n  landing: ${landingSha}`);
}
ok(`SHA256 match (${serverSha.slice(0, 16)}…)`);

if (serverSize !== landingSize) {
  fail(`size mismatch server=${serverSize} landing=${landingSize}`);
}
ok(`size match (${serverSize} bytes)`);

const manifest = JSON.parse(readFileSync(MANIFEST, "utf8"));
const android = manifest.android;
if (!android) fail("manifest.json missing android entry");
if (android.sha256 !== serverSha) {
  fail(`manifest sha256 mismatch\n  manifest: ${android.sha256}\n  file:     ${serverSha}`);
}
if (android.sizeBytes !== serverSize) {
  fail(`manifest sizeBytes mismatch (${android.sizeBytes} vs ${serverSize})`);
}
ok("manifest.json sha256 + sizeBytes match artifact");

const zipalign = spawnSync("zipalign", ["-c", "-v", "4", SERVER_APK], {
  encoding: "utf8",
});
if (commandMissing(zipalign)) {
  console.warn("⚠ zipalign not available — skip alignment check");
} else if (zipalign.status !== 0) {
  fail(`zipalign check failed${zipalign.stderr ? `\n${zipalign.stderr.trim()}` : ""}`);
} else {
  ok("zipalign verified");
}

const apksigner = spawnSync(
  "apksigner",
  ["verify", "--print-certs", SERVER_APK],
  { encoding: "utf8" }
);
if (commandMissing(apksigner)) {
  console.warn("⚠ apksigner not available — skip signature check");
} else if (apksigner.status !== 0) {
  fail(`apksigner verify failed${apksigner.stderr ? `\n${apksigner.stderr.trim()}` : ""}`);
} else {
  const out = apksigner.stdout + apksigner.stderr;
  ok("apksigner verify OK");
  if (/CN=Android Debug/i.test(out)) {
    if (REQUIRE_RELEASE_SIGN) {
      fail("APK firmado con certificado Debug y APK_REQUIRE_RELEASE_SIGN está activo");
    }
    console.warn("⚠ APK signed with Android Debug certificate (not release keystore)");
  }
}

console.log("\nPipeline integrity: OK");
