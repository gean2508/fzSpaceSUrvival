#!/usr/bin/env node
// Regenerate assets/art/manifest.json from the PNGs dropped in assets/art/.
// Key = relative path without extension ("decor/tree"); value = asset path
// Bevy loads ("art/decor/tree.png"). Run after adding/renaming art.

import { readdirSync, statSync, writeFileSync } from "node:fs";
import { join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(fileURLToPath(new URL(".", import.meta.url)), "..");
const ART_DIR = join(ROOT, "native", "crates", "ha-game", "assets", "art");

function walk(dir, out = []) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) walk(full, out);
    else if (/\.png$/i.test(entry)) out.push(full);
  }
  return out;
}

const sprites = {};
for (const file of walk(ART_DIR).sort()) {
  const rel = relative(ART_DIR, file).split(sep).join("/");
  const key = rel.replace(/\.png$/i, "");
  sprites[key] = `art/${rel}`;
}

const manifest = JSON.stringify({ sprites }, null, 2) + "\n";
writeFileSync(join(ART_DIR, "manifest.json"), manifest);
console.log(`sync-art: ${Object.keys(sprites).length} sprites → assets/art/manifest.json`);
