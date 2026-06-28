#!/usr/bin/env node
/** Setup Rust Android targets + cargo-ndk + cargo-apk for native client. */
import { execSync, spawnSync } from "node:child_process";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { resolveAndroidEnv } from "./lib/android-env.mjs";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");

let androidEnv;
try {
  androidEnv = resolveAndroidEnv();
} catch (e) {
  console.error(e.message);
  process.exit(1);
}

Object.assign(process.env, androidEnv);
console.log(`NDK: ${process.env.ANDROID_NDK_ROOT}`);

execSync(
  "rustup target add aarch64-linux-android armv7-linux-androideabi",
  { stdio: "inherit" }
);

const cargoPlugins = [{ pkg: "cargo-ndk", args: ["ndk", "--version"] }];

for (const { pkg, args } of cargoPlugins) {
  if (spawnSync("cargo", args, { env: process.env }).status !== 0) {
    console.log(`Instalando ${pkg}…`);
    execSync(`cargo install ${pkg}`, { stdio: "inherit", env: process.env });
  }
}

console.log("✓ Android native toolchain OK");
