#!/usr/bin/env node
/**
 * Build native Android APK + publish to server/updates + landing downloads.
 *
 * Usage: node scripts/ship-native-android.mjs [--no-bump] [--host 192.168.1.5]
 */
import { execSync } from "node:child_process";
import { existsSync, readFileSync, statSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { resolveAndroidEnv } from "./lib/android-env.mjs";
import { clearStaleCargoLocks, jniLibsUpToDate } from "./lib/cargo-lock.mjs";
import { freeDisk } from "./lib/free-disk.mjs";
import { copyLibcxxShared } from "./lib/android-ndk-libs.mjs";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const NATIVE = resolve(ROOT, "native");
const ANDROID = resolve(NATIVE, "android");
const JNI_LIBS = resolve(ANDROID, "app/src/main/jniLibs");
const VERSION_PATH = resolve(NATIVE, "version.json");

const args = process.argv.slice(2);
const noBump = args.includes("--no-bump");
const forceRust = args.includes("--rebuild-rust");
// Skip the pre-publish test gates (npm test + `make -C native test`). Those
// recompile the ha-game TEST binary (full Bevy) and OOM on low-RAM machines
// (see biblia.md §12). Use when you've already validated with `cargo check`.
const skipTests = args.includes("--skip-tests");
const hostIdx = args.indexOf("--host");
const lanHost =
  hostIdx >= 0
    ? args[hostIdx + 1]
    : execSync("node scripts/detect-lan-ip.mjs", { cwd: ROOT, encoding: "utf8" }).trim();

function run(cmd, cwd = ROOT, env = {}) {
  console.log(`\n▶ ${cmd}`);
  execSync(cmd, {
    cwd,
    stdio: "inherit",
    env: { ...process.env, ...env },
  });
}

function findApk(modifiedAfterMs = 0) {
  const candidates = [
    resolve(ANDROID, "app/build/outputs/apk/debug/app-debug.apk"),
    resolve(ANDROID, "app/build/outputs/apk/release/app-release.apk"),
    resolve(NATIVE, "target/android-artifacts/hunger-arena-native.apk"),
  ];
  let best = null;
  let bestMtime = 0;
  for (const p of candidates) {
    if (!existsSync(p)) continue;
    const st = statSync(p);
    if (st.size < 5_000_000) continue;
    if (st.mtimeMs < modifiedAfterMs) continue;
    if (st.mtimeMs > bestMtime) {
      best = p;
      bestMtime = st.mtimeMs;
    }
  }
  return best;
}

let androidEnv;
try {
  androidEnv = resolveAndroidEnv();
} catch (e) {
  console.error(e.message);
  process.exit(1);
}

console.log(`LAN host: ${lanHost}`);
console.log(`NDK: ${androidEnv.ANDROID_NDK_ROOT}`);

if (!noBump) {
  run("node scripts/bump-app-version.mjs android");
}

if (skipTests) {
  // Skip the pre-publish test/check gates — they recompile ha-game (full Bevy)
  // twice extra and OOM low-RAM machines. `cargo ndk build` below still compiles
  // it for real, so a broken build fails there anyway.
  console.log("⊘ Skip tests + cargo check (--skip-tests) — validá aparte con cargo check");
} else {
  run("npm run validate:arch", ROOT);
  run("npm test", ROOT);
  run("make -C native test", ROOT);
  run(`cargo check -p ha-game --features android`, NATIVE, androidEnv);
}

const version = JSON.parse(readFileSync(VERSION_PATH, "utf8"));

run("node scripts/setup-android-native.mjs", ROOT, androidEnv);

const buildEnv = {
  ...androidEnv,
  HA_SERVER_HOST: lanHost,
  HA_SERVER_PORT: process.env.HA_SERVER_PORT ?? "2567",
};

const sourceRoots = [
  resolve(NATIVE, "crates/ha-game/src"),
  resolve(NATIVE, "crates/ha-shared/src"),
  resolve(NATIVE, "crates/ha-game/Cargo.toml"),
  resolve(NATIVE, "crates/ha-shared/Cargo.toml"),
  resolve(NATIVE, "Cargo.toml"),
];

const removedLocks = clearStaleCargoLocks(resolve(NATIVE, "target"));
if (removedLocks > 0) {
  console.log(`⊘ Eliminados ${removedLocks} lock(s) stale de Cargo`);
}

const rustUpToDate = !forceRust && jniLibsUpToDate(JNI_LIBS, sourceRoots);

// ABIs a compilar. Por defecto SOLO arm64-v8a (Redmi 10 en adelante y todo
// equipo moderno es 64-bit; la de 32-bit armeabi-v7a duplica build/disco/APK
// sin usarse). Override: ANDROID_ABIS="arm64-v8a,armeabi-v7a".
const ABIS = (process.env.ANDROID_ABIS || "arm64-v8a")
  .split(",")
  .map((a) => a.trim())
  .filter(Boolean);
const abiFlags = ABIS.map((a) => `-t ${a}`).join(" ");

if (rustUpToDate) {
  console.log("⊘ Skip cargo ndk — jniLibs al día (usá --rebuild-rust para forzar)");
} else {
  console.log(`▶ cargo ndk ABIs: ${ABIS.join(", ")}`);
  run(
    `cargo ndk ${abiFlags} -o "${JNI_LIBS}" build -p ha-game --profile release-native --features android`,
    NATIVE,
    buildEnv
  );
}

copyLibcxxShared(androidEnv.ANDROID_NDK_ROOT, JNI_LIBS, ABIS);
console.log("✓ libc++_shared.so copiado a jniLibs");

const gradleBuildStartedAt = Date.now();
run(
  // `assembleRelease`: el APK debug lleva `debuggable=true`, que apaga las
  // optimizaciones de ART y activa los chequeos JNI en cada llamada nativa —
  // justo lo que no sobra en el teléfono. Se firma con la clave de debug (ver
  // native/android/app/build.gradle) porque esto se instala de costado.
  // `findApk` ya elige por fecha entre debug/release, así que sigue funcionando.
  // `-PabiFilters`: gradle tiene que empaquetar EXACTAMENTE los ABIs que compiló
  // cargo ndk. Si le sobra uno, mete el libc++_shared.so del NDK sin su
  // libha_game.so → peso muerto y crash en ese equipo (biblia.md §12).
  `./gradlew assembleRelease -PversionCode=${version.versionCode} -PversionName=${version.version} -PabiFilters=${ABIS.join(",")}`,
  ANDROID,
  buildEnv
);

let apk = findApk(gradleBuildStartedAt);
if (!apk) {
  console.error("APK no encontrado (o está stale) tras el build. Revisá logs de cargo ndk / gradlew.");
  process.exit(1);
}

console.log(`\n✓ APK: ${apk} (${(statSync(apk).size / 1024 / 1024).toFixed(1)} MB)`);

run(
  `node scripts/publish-update.mjs --platform android --file "${apk}" --version ${version.version} --code ${version.versionCode} --base-url http://${lanHost}:2567`,
  ROOT
);

run("node scripts/verify-apk-pipeline.mjs", ROOT);

// El disco de esta máquina se llena y lo que falla después NO parece falta de
// espacio (el linker muere con "Bus error"). Se purga la caché incremental —
// regenerable — al terminar, siempre, sin que nadie tenga que acordarse.
freeDisk(NATIVE);

console.log("\n══ APK listo para instalar ══");
console.log(`Landing:  http://${lanHost}:5173`);
console.log(`Directo:  http://${lanHost}:5173/downloads/hunger-arena.apk`);
console.log(`Servidor: http://${lanHost}:2567/updates/manifest`);
