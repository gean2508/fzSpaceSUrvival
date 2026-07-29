#!/usr/bin/env node
/**
 * HA_SUITE (B1, PLAN_MAESTRO §3.B1) — corre TODOS los drivers de QA del juego
 * (autodrive.rs), cada uno en su propio proceso, y AGREGA los bugs que emiten a
 * `bugreport.jsonl` (una línea JSON por invariante roto). Al final imprime un
 * resumen para que la IA (o vos) lea y resuelva uno por uno.
 *
 * Uso:
 *   node scripts/ha-suite.mjs           # corre la suite, escribe bugreport.jsonl
 *   node scripts/ha-suite.mjs --headless  # (futuro) modo sin ventana / swarm
 *
 * Requiere el binario ya compilado: `cd native && cargo build -q -p ha-game`.
 */
import { spawnSync } from "node:child_process";
import { existsSync, rmSync, readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const NATIVE = resolve(ROOT, "native");
const BIN = resolve(NATIVE, "target/debug/ha-game");
const REPORT = resolve(NATIVE, "bugreport.jsonl");

// Un driver por mecánica (env que lo activa + segundos de timeout). Ampliá acá
// a medida que nazcan drivers nuevos (cavar/craft/cuevas/puertas/construcción…).
const DRIVERS = [
  { name: "flight", env: "HA_FLYTEST", secs: 80 },
  { name: "moon_climb", env: "HA_MOONTEST", secs: 80 },
  { name: "rocket", env: "HA_ROCKETTEST", secs: 80 },
  { name: "landing_cycle", env: "HA_CYCLETEST", secs: 100 },
  { name: "smoke", env: "HA_AUTODRIVE", secs: 120 },
];

if (!existsSync(BIN)) {
  console.error(`✗ Falta el binario: ${BIN}\n  Compilá:  cd native && cargo build -q -p ha-game`);
  process.exit(2);
}
// Empezar limpio: un reporte por corrida de suite.
if (existsSync(REPORT)) rmSync(REPORT);

console.log("▶ HA_SUITE — corriendo drivers de QA…\n");
let failed = 0;
for (const d of DRIVERS) {
  process.stdout.write(`  · ${d.name.padEnd(16)} `);
  const r = spawnSync("timeout", [String(d.secs), BIN], {
    cwd: NATIVE,
    env: { ...process.env, [d.env]: "1", HA_AUTOPLAY: "1", WGPU_BACKEND: "gl", HA_BUGREPORT: REPORT },
    stdio: ["ignore", "ignore", "ignore"],
  });
  // exit 0 = invariantes OK; ≠0 = algún invariante roto (ya quedó en el reporte).
  const ok = r.status === 0;
  if (!ok) failed++;
  console.log(ok ? "OK" : `FAIL (exit ${r.status ?? "timeout"})`);
}

// Agregar y resumir el reporte.
const bugs = existsSync(REPORT)
  ? readFileSync(REPORT, "utf8").trim().split("\n").filter(Boolean).map((l) => {
      try { return JSON.parse(l); } catch { return { mechanic: "?", invariant: "parse_error", detail: l }; }
    })
  : [];

console.log(`\n══ Resumen ══  drivers: ${DRIVERS.length}  ·  con fallo: ${failed}  ·  bugs: ${bugs.length}`);
if (bugs.length) {
  console.log(`\nbugreport.jsonl (${REPORT}):`);
  for (const b of bugs) console.log(`  ✗ [${b.mechanic}] ${b.invariant} — ${b.detail}`);
  console.log("\nLa IA puede leer bugreport.jsonl y resolver cada bug re-corriendo la suite hasta verde.");
} else {
  console.log("\n✓ Sin bugs: todos los invariantes pasaron.");
}
process.exit(failed ? 1 : 0);
