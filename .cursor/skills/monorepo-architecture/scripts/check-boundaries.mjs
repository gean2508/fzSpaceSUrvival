#!/usr/bin/env node
/**
 * Architecture boundary checker — native client + shared sim + server.
 */
import { readFileSync, existsSync, readdirSync, statSync } from "node:fs";
import { resolve, relative, join } from "node:path";

const ROOT = resolve(import.meta.dirname, "../../../..");
const errors = [];

function walk(dir, acc = []) {
  if (!existsSync(dir)) return acc;
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    const st = statSync(p);
    if (st.isDirectory()) {
      if (name === "node_modules" || name === "dist" || name === ".git" || name === "target") continue;
      walk(p, acc);
    } else if (/\.(ts|tsx|js|jsx|mjs)$/.test(name)) {
      acc.push(p);
    }
  }
  return acc;
}

function rel(p) {
  return relative(ROOT, p);
}

// Forbidden legacy React game clients
for (const legacy of ["apps/mobile", "apps/desktop", "packages/client-core", "packages/client-gfx"]) {
  if (existsSync(resolve(ROOT, legacy))) {
    errors.push(`Legacy path must be removed: ${legacy} (use native/)`);
  }
}

// GameSimulation only in shared
const FORBIDDEN_NAMES = [{ name: "GameSimulation.ts", allowed: ["packages/shared/src/sim"] }];
for (const { name, allowed } of FORBIDDEN_NAMES) {
  for (const base of ["client", "server", "native"]) {
    for (const file of walk(resolve(ROOT, base))) {
      if (!file.endsWith(name)) continue;
      const ok = allowed.some((prefix) => rel(file).startsWith(prefix));
      if (!ok) errors.push(`Forbidden duplicate: ${rel(file)}`);
    }
  }
}

// client/ landing must not import R3F
for (const file of walk(resolve(ROOT, "client/src"))) {
  const src = readFileSync(file, "utf8");
  if (/from\s+["']@react-three\//.test(src) || /from\s+["']three["']/.test(src)) {
    errors.push(`Landing must not use R3F/Three: ${rel(file)}`);
  }
}

// shared sim pure TS
for (const file of walk(resolve(ROOT, "packages/shared/src"))) {
  const src = readFileSync(file, "utf8");
  if (/from\s+["'](react|three|@colyseus\/sdk)/.test(src)) {
    errors.push(`shared must stay pure TS: ${rel(file)}`);
  }
}

if (errors.length) {
  console.error("Architecture boundary violations:\n");
  for (const e of errors) console.error(`  ✗ ${e}`);
  process.exit(1);
}

console.log("✓ Architecture boundaries OK");
