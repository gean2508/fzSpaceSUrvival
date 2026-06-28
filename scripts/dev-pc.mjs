#!/usr/bin/env node
/**
 * PC dev loop: optional Colyseus server + native Bevy client with auto-rebuild.
 *
 * Usage:
 *   npm run dev:pc              # server + game (cargo watch)
 *   npm run dev:pc -- --game    # game only, no server
 */
import { spawn, execSync } from "node:child_process";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const NATIVE = resolve(ROOT, "native");
const args = process.argv.slice(2);
const gameOnly = args.includes("--game") || args.includes("--no-server");

function ensureCargoWatch() {
  try {
    execSync("cargo watch --version", { stdio: "ignore" });
    return;
  } catch {
    console.log("▶ Installing cargo-watch (one-time)…");
    execSync("cargo install cargo-watch", { stdio: "inherit" });
  }
}

function lanHost() {
  try {
    return execSync("node scripts/detect-lan-ip.mjs", { cwd: ROOT, encoding: "utf8" }).trim();
  } catch {
    return "127.0.0.1";
  }
}

ensureCargoWatch();

const host = lanHost();
const env = {
  ...process.env,
  RUST_BACKTRACE: "1",
  HA_REPO_ROOT: ROOT,
  HA_SERVER_HOST: process.env.HA_SERVER_HOST ?? host,
  VITE_SERVER_HOST: process.env.VITE_SERVER_HOST ?? host,
};

console.log(`
╔══════════════════════════════════════════════════════════╗
║  HUNGER ARENA — PC dev (auto-rebuild on save)           ║
╠══════════════════════════════════════════════════════════╣
║  Move: WASD · Look: mouse · LMB break · RMB/F place      ║
║  Jump: Space · Sprint: Shift · Debug: F3 · View: F5      ║
║  Server host: ${host.padEnd(39)}║
╚══════════════════════════════════════════════════════════╝
`);

const children = [];

if (!gameOnly) {
  const server = spawn("npm", ["run", "dev:server"], {
    cwd: ROOT,
    env,
    stdio: "inherit",
  });
  children.push(server);
}

const game = spawn(
  "cargo",
  ["watch", "-q", "-w", "crates", "-x", "run -p ha-game"],
  { cwd: NATIVE, env, stdio: "inherit" }
);
children.push(game);

const shutdown = (sig) => {
  for (const c of children) {
    try {
      c.kill(sig);
    } catch {
      /* ignore */
    }
  }
  process.exit(0);
};

process.on("SIGINT", () => shutdown("SIGINT"));
process.on("SIGTERM", () => shutdown("SIGTERM"));
