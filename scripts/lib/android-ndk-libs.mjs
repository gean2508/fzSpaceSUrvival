/** Copy NDK libc++_shared.so into jniLibs (required by Bevy/Rust on Android). */
import { copyFileSync, existsSync, mkdirSync, readdirSync, rmSync } from "node:fs";
import { join } from "node:path";

const ABI_TRIPLE = {
  "arm64-v8a": "aarch64-linux-android",
  "armeabi-v7a": "arm-linux-androideabi",
  "x86_64": "x86_64-linux-android",
};

function ndkPrebuiltSysroot(ndkRoot) {
  const prebuilt = join(ndkRoot, "toolchains/llvm/prebuilt");
  if (!existsSync(prebuilt)) return null;
  for (const host of ["linux-x86_64", "darwin-x86_64", "darwin-arm64"]) {
    const sysroot = join(prebuilt, host, "sysroot/usr/lib");
    if (existsSync(sysroot)) return sysroot;
  }
  return null;
}

/**
 * Copia `libc++_shared.so` SÓLO para los ABIs pedidos y borra los directorios de
 * los ABIs que no se compilan.
 *
 * Antes copiaba siempre los dos ABIs sin mirar cuáles había compilado cargo ndk:
 * un equipo de 32 bits recibía un APK con `libc++_shared.so` pero SIN
 * `libha_game.so` y crasheaba al cargar el motor, y cada APK arrastraba 1.3 MB
 * muertos. Ver biblia.md §12.
 */
export function copyLibcxxShared(ndkRoot, jniLibsDir, abis = ["arm64-v8a"]) {
  const sysrootLib = ndkPrebuiltSysroot(ndkRoot);
  if (!sysrootLib) {
    throw new Error(`No se encontró sysroot del NDK en ${ndkRoot}`);
  }

  for (const abi of abis) {
    const triple = ABI_TRIPLE[abi];
    if (!triple) throw new Error(`ABI desconocido: ${abi}`);
    const src = join(sysrootLib, triple, "libc++_shared.so");
    if (!existsSync(src)) {
      throw new Error(`libc++_shared.so no encontrado: ${src}`);
    }
    const destDir = join(jniLibsDir, abi);
    mkdirSync(destDir, { recursive: true });
    copyFileSync(src, join(destDir, "libc++_shared.so"));
  }

  // Poda: un ABI que quedó de un build anterior se cuela igual en el APK, porque
  // gradle empaqueta lo que encuentre en jniLibs/.
  if (!existsSync(jniLibsDir)) return;
  for (const entry of readdirSync(jniLibsDir, { withFileTypes: true })) {
    if (entry.isDirectory() && !abis.includes(entry.name)) {
      rmSync(join(jniLibsDir, entry.name), { recursive: true, force: true });
      console.log(`  ⌫ jniLibs/${entry.name} (ABI que no se compila) eliminado`);
    }
  }
}
