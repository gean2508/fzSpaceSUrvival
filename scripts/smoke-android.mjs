#!/usr/bin/env node
/**
 * Install + launch Hunger Arena on a connected device and fail on startup crash.
 *
 * Usage:
 *   node scripts/smoke-android.mjs [--device SERIAL] [--wait-ms 15000] [--no-launch]
 *
 * Exit 0 when logcat shows ha-boot without ha-crash / B0001 / FATAL in the window.
 */
import { spawnSync } from "node:child_process";

const args = process.argv.slice(2);
const deviceIdx = args.indexOf("--device");
const waitIdx = args.indexOf("--wait-ms");
const device = deviceIdx >= 0 ? args[deviceIdx + 1] : process.env.ADB_DEVICE;
const waitMs = waitIdx >= 0 ? Number(args[waitIdx + 1]) : 15_000;
const noLaunch = args.includes("--no-launch");

const PKG = "com.hungerarena.app";
const ACTIVITY = `${PKG}/.MainActivity`;

function adb(...cmd) {
  const base = device ? ["adb", "-s", device] : ["adb"];
  const result = spawnSync(base[0], [...base.slice(1), ...cmd], {
    encoding: "utf8",
  });
  if (result.error?.code === "ENOENT") {
    console.error("✗ adb not found — install Android platform-tools");
    process.exit(1);
  }
  return result;
}

function fail(msg) {
  console.error(`✗ smoke-android: ${msg}`);
  process.exit(1);
}

function ok(msg) {
  console.log(`✓ ${msg}`);
}

const devices = adb("devices").stdout ?? "";
const lines = devices
  .split("\n")
  .map((l) => l.trim())
  .filter((l) => l.endsWith("\tdevice"));
if (lines.length === 0) fail("no device connected (adb devices)");
if (!device && lines.length > 1) {
  console.warn(`⚠ multiple devices — using ${lines[0].split("\t")[0]}`);
}
const serial = device ?? lines[0].split("\t")[0];
ok(`device ${serial}`);

if (!noLaunch) {
  adb("logcat", "-c");
  adb("shell", "am", "force-stop", PKG);
  const start = adb("shell", "am", "start", "-n", ACTIVITY);
  if (start.status !== 0) {
    fail(`am start failed\n${start.stderr ?? start.stdout}`);
  }
  ok("launched MainActivity");
}

console.log(`… waiting ${waitMs}ms for boot`);
spawnSync("sleep", [String(Math.ceil(waitMs / 1000))]);

const log = adb("logcat", "-d", "-t", "600").stdout ?? "";
const pkgLines = log.split("\n").filter((l) => l.includes(PKG) || l.includes("RustStdoutStderr"));

const crashPatterns = [
  /ha-crash/,
  /error\[B0001\]/,
  /FATAL EXCEPTION/,
  /JNI DETECTED ERROR/,
  /thread.*panicked/i,
  /AndroidRuntime.*FATAL/,
];

const hits = crashPatterns.flatMap((re) =>
  pkgLines.filter((l) => re.test(l)).slice(0, 3)
);
if (hits.length > 0) {
  console.error("\n--- crash log excerpt ---");
  for (const line of hits) console.error(line);
  fail("startup crash detected — fix before shipping");
}

const booted = pkgLines.some((l) => /ha-boot:/.test(l));
if (!booted) {
  console.warn("⚠ ha-boot not seen — app may still be loading or logging is filtered");
  const running = adb("shell", "pidof", PKG).stdout?.trim();
  if (!running) fail("process not running after wait window");
  ok(`process alive (pid ${running}) — no ha-crash in log`);
} else {
  ok("ha-boot seen, no crash signatures");
}

console.log("\nSmoke test: OK");
