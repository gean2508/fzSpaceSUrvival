#!/usr/bin/env node
/** Copy server/updates/artifacts → client/public/downloads for same-origin landing installs. */
import { createHash } from "node:crypto";
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

const ROOT = resolve(import.meta.dirname, "..");
const SRC = resolve(ROOT, "server/updates/artifacts");
const DEST = resolve(ROOT, "client/public/downloads");

const FILES = ["hunger-arena.apk", "hunger-arena.AppImage", "hunger-arena.ipa"];

function sha256File(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

mkdirSync(DEST, { recursive: true });
let copied = 0;

for (const name of FILES) {
  const src = resolve(SRC, name);
  if (!existsSync(src)) {
    console.log(`⊘ ${name} (no publicado)`);
    continue;
  }
  const dest = resolve(DEST, name);
  copyFileSync(src, dest);
  if (name.endsWith(".apk")) {
    writeFileSync(resolve(DEST, `${name}.sha256`), `${sha256File(dest)}\n`);
  }
  console.log(`✓ ${name}`);
  copied++;
}

if (copied === 0) {
  console.error("Nada que copiar.");
  console.error("  Android: npm run ship:android");
  process.exit(1);
}

console.log(`\n${copied} archivo(s) listos en client/public/downloads/`);
