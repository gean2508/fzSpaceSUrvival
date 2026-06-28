#!/usr/bin/env node
/**
 * Publish a new APK or AppImage to server/updates and refresh manifest.json.
 *
 * Examples:
 *   node scripts/publish-update.mjs --platform android --file native/target/.../ha-game.apk --version 1.0.1 --code 2
 *   node scripts/publish-update.mjs --platform desktop --file "..." --version 0.1.1
 *   node scripts/publish-update.mjs ... --optional   # allow skip on splash (dev only)
 *
 * Optional env:
 *   UPDATE_BASE_URL=http://192.168.1.42:2567  (artifact URLs in manifest)
 */
import { createHash } from "node:crypto";
import { copyFileSync, mkdirSync, readFileSync, writeFileSync, statSync } from "node:fs";
import { resolve, basename } from "node:path";

const ROOT = resolve(import.meta.dirname, "..");
const MANIFEST_PATH = resolve(ROOT, "server/updates/manifest.json");
const ARTIFACTS_DIR = resolve(ROOT, "server/updates/artifacts");
const LANDING_DOWNLOADS = resolve(ROOT, "client/public/downloads");

function parseArgs(argv) {
  const out = {};
  for (let i = 2; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--platform") out.platform = argv[++i];
    else if (a === "--file") out.file = argv[++i];
    else if (a === "--version") out.version = argv[++i];
    else if (a === "--code") out.code = Number(argv[++i]);
    else if (a === "--mandatory") out.mandatory = true;
    else if (a === "--optional") out.mandatory = false;
    else if (a === "--notes") out.notes = argv[++i];
    else if (a === "--base-url") out.baseUrl = argv[++i];
  }
  return out;
}

function sha256File(path) {
  const buf = readFileSync(path);
  return createHash("sha256").update(buf).digest("hex");
}

const args = parseArgs(process.argv);
if (!args.platform || !args.file || !args.version) {
  console.error(
    "Usage: node scripts/publish-update.mjs --platform android|desktop --file <path> --version <semver> [--code N] [--optional] [--notes text] [--base-url http://host:2567]"
  );
  process.exit(1);
}

const platform = args.platform;
if (platform !== "android" && platform !== "desktop") {
  console.error("platform must be android or desktop");
  process.exit(1);
}

if (platform === "android" && !Number.isInteger(args.code)) {
  console.error("android publish requires --code <versionCode entero>");
  process.exit(1);
}

const src = resolve(args.file);
const artifactName =
  platform === "android" ? "hunger-arena.apk" : "hunger-arena.AppImage";
const dest = resolve(ARTIFACTS_DIR, artifactName);

const manifest = JSON.parse(readFileSync(MANIFEST_PATH, "utf8"));
if (platform === "android") {
  const prevCode = Number(manifest.android?.versionCode ?? 0);
  if (Number.isFinite(prevCode) && args.code <= prevCode) {
    console.error(
      `versionCode debe subir (actual=${prevCode}, nuevo=${args.code}). Corré: node scripts/bump-app-version.mjs android`
    );
    process.exit(1);
  }
}

copyFileSync(src, dest);
mkdirSync(LANDING_DOWNLOADS, { recursive: true });
const landingDest = resolve(LANDING_DOWNLOADS, artifactName);
copyFileSync(src, landingDest);
const sizeBytes = statSync(dest).size;
const sha256 = sha256File(dest);
writeFileSync(resolve(LANDING_DOWNLOADS, `${artifactName}.sha256`), `${sha256}\n`);

const baseUrl = (args.baseUrl ?? process.env.UPDATE_BASE_URL ?? "http://localhost:2567").replace(
  /\/$/,
  ""
);
const url = `${baseUrl}/updates/artifacts/${artifactName}`;
manifest.publishedAt = new Date().toISOString();

const mandatory = args.mandatory !== false;

const entry = {
  version: args.version,
  url,
  sha256,
  sizeBytes,
  mandatory,
  notes: args.notes ?? `Publicado desde ${basename(src)}`,
};
if (platform === "android" && args.code) {
  entry.versionCode = args.code;
}

manifest[platform] = entry;
writeFileSync(MANIFEST_PATH, JSON.stringify(manifest, null, 2) + "\n");

console.log(`✓ Copied ${src} → ${dest}`);
console.log(`✓ Landing static → ${resolve(LANDING_DOWNLOADS, artifactName)}`);
console.log(`✓ SHA-256 ${sha256}`);
console.log(`✓ Manifest updated (${platform} ${args.version}, mandatory=${mandatory})`);
console.log(`  URL: ${url}`);
console.log("\nClients with an older version will auto-update on next launch.");
