#!/usr/bin/env node
/**
 * One command: ship APK + start server + landing. User only opens URL on phone.
 *
 * Usage: npm run play
 *        npm run play -- --no-ship     (skip rebuild if APK already published)
 *        npm run play -- --skip-tests  (build APK but skip test gates — evita OOM en PCs de poca RAM)
 *        npm run play -- --host 100.x.y.z  (bake this server host into the APK —
 *            e.g. a Tailscale IP so the phone can DOWNLOAD + PLAY from OUTSIDE the LAN)
 */
import { execSync, spawn } from "node:child_process";
import { existsSync, statSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const noShip = args.includes("--no-ship");
const skipTests = args.includes("--skip-tests");
// Optional explicit server host baked into the APK (e.g. a Tailscale IP so the
// phone downloads + plays from OUTSIDE the LAN): `npm run play -- --host 100.x.y.z`.
// The dev server + landing bind ALL interfaces (vite host:true, colyseus 0.0.0.0),
// so they stay reachable on the LAN IP too — only the APK's baked host is this one.
const hostIdx = args.indexOf("--host");
const explicitHost = hostIdx >= 0 ? args[hostIdx + 1] : null;

const lanHost =
  explicitHost ||
  execSync("node scripts/detect-lan-ip.mjs", {
    cwd: ROOT,
    encoding: "utf8",
  }).trim();

const apkPath = resolve(ROOT, "client/public/downloads/hunger-arena.apk");
const apkOk = existsSync(apkPath) && statSync(apkPath).size > 5_000_000;

if (!noShip || !apkOk) {
  try {
    const shipFlags = skipTests ? " --skip-tests" : "";
    execSync(`node scripts/ship-native-android.mjs --host ${lanHost}${shipFlags}`, {
      cwd: ROOT,
      stdio: "inherit",
    });
  } catch (e) {
    if (apkOk) {
      console.error("\n⚠ Build APK falló — siguiendo con el APK ya publicado en client/public/downloads/");
    } else {
      console.error("\n⚠ Build APK falló. Si ya tenés un APK publicado, probá: npm run play -- --no-ship");
      throw e;
    }
  }
} else {
  console.log("⊘ Skip ship — APK en client/public/downloads/ OK");
}

console.log("\n▶ Levantando servidor + landing…");

const env = {
  ...process.env,
  VITE_SERVER_HOST: lanHost,
  UPDATE_BASE_URL: `http://${lanHost}:2567`,
};

const server = spawn("npm", ["run", "dev:server"], {
  cwd: ROOT,
  env,
  stdio: "inherit",
  detached: false,
});

const client = spawn("npm", ["run", "dev:client"], {
  cwd: ROOT,
  env,
  stdio: "inherit",
  detached: false,
});

const shutdown = () => {
  server.kill("SIGTERM");
  client.kill("SIGTERM");
  process.exit(0);
};
process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);

setTimeout(() => {
  console.log(`
╔══════════════════════════════════════════════════════════╗
║  HUNGER ARENA — listo para jugar                         ║
╠══════════════════════════════════════════════════════════╣
║  1. Celular en la misma WiFi que esta PC                 ║
║  2. Abrí en Chrome:                                      ║
║     → http://${lanHost}:5173
║  3. Tocá DESCARGAR APK → instalá → abrí                  ║
║                                                          ║
║  APK directo: http://${lanHost}:5173/downloads/hunger-arena.apk
╚══════════════════════════════════════════════════════════╝
`);
}, 4000);

Promise.all([
  new Promise((res) => server.on("exit", res)),
  new Promise((res) => client.on("exit", res)),
]).then(shutdown);
