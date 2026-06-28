/** Clear stale Cargo artifact locks when no cargo/rustc process is running. */
import { execSync } from "node:child_process";
import { existsSync, readdirSync, rmSync, statSync } from "node:fs";
import { join } from "node:path";

function cargoRunning() {
  try {
    const out = execSync("pgrep -x cargo 2>/dev/null || true", { encoding: "utf8" }).trim();
    if (out) return true;
    const rustc = execSync("pgrep -x rustc 2>/dev/null || true", { encoding: "utf8" }).trim();
    return Boolean(rustc);
  } catch {
    return false;
  }
}

function findCargoLocks(dir, found = []) {
  if (!existsSync(dir)) return found;
  for (const ent of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, ent.name);
    if (ent.isDirectory()) findCargoLocks(p, found);
    else if (ent.name === ".cargo-lock") found.push(p);
  }
  return found;
}

export function clearStaleCargoLocks(targetDir) {
  if (cargoRunning()) return 0;
  const locks = findCargoLocks(targetDir);
  let removed = 0;
  for (const lock of locks) {
    try {
      rmSync(lock);
      removed += 1;
    } catch {
      // Another process acquired the lock between checks.
    }
  }
  return removed;
}

export function newestMtime(root) {
  if (!existsSync(root)) return 0;
  const st = statSync(root);
  if (!st.isDirectory()) return st.mtimeMs;
  let max = st.mtimeMs;
  for (const ent of readdirSync(root, { withFileTypes: true })) {
    max = Math.max(max, newestMtime(join(root, ent.name)));
  }
  return max;
}

export function jniLibsUpToDate(jniLibsDir, sourceRoots) {
  const libs = [
    join(jniLibsDir, "arm64-v8a/libha_game.so"),
    join(jniLibsDir, "armeabi-v7a/libha_game.so"),
  ];
  if (!libs.every((p) => existsSync(p) && statSync(p).size > 1_000_000)) return false;
  const libMtime = Math.min(...libs.map((p) => statSync(p).mtimeMs));
  const srcMtime = Math.max(...sourceRoots.map((p) => newestMtime(p)));
  return libMtime >= srcMtime;
}
