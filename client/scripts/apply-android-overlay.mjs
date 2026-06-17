#!/usr/bin/env node
/**
 * Apply the Android overlay after `npx cap add android`:
 *  1. copy network_security_config.xml into the generated android project,
 *  2. patch AndroidManifest.xml so cleartext ws:// works on LAN dev builds.
 *
 * Idempotent — safe to run on every `cap sync`.
 *
 * Usage: node client/scripts/apply-android-overlay.mjs
 */
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const androidDir = resolve(root, "android");

if (!existsSync(androidDir)) {
  console.error("android/ not found. Run `npx cap add android` first.");
  process.exit(1);
}

// 1) Copy the network security config.
const srcXml = resolve(root, "android-overlay/res/xml/network_security_config.xml");
const destXmlDir = resolve(androidDir, "app/src/main/res/xml");
mkdirSync(destXmlDir, { recursive: true });
copyFileSync(srcXml, resolve(destXmlDir, "network_security_config.xml"));
console.log("copied network_security_config.xml");

// 2) Patch the manifest's <application> tag (only once).
const manifestPath = resolve(androidDir, "app/src/main/AndroidManifest.xml");
let manifest = readFileSync(manifestPath, "utf8");

if (!manifest.includes("networkSecurityConfig")) {
  manifest = manifest.replace(
    /<application\b/,
    `<application\n        android:networkSecurityConfig="@xml/network_security_config"\n        android:usesCleartextTraffic="true"`
  );
  writeFileSync(manifestPath, manifest);
  console.log("patched AndroidManifest.xml");
} else {
  console.log("AndroidManifest.xml already patched, skipping");
}

console.log("Android overlay applied. Build with VITE_SERVER_HOST set, then `npm run cap:sync`.");
