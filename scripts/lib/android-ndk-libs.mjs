/** Copy NDK libc++_shared.so into jniLibs (required by Bevy/Rust on Android). */
import { copyFileSync, existsSync, mkdirSync, statSync } from "node:fs";
import { join } from "node:path";

const ABI_MAP = [
  ["arm64-v8a", "aarch64-linux-android"],
  ["armeabi-v7a", "arm-linux-androideabi"],
];

function ndkPrebuiltSysroot(ndkRoot) {
  const prebuilt = join(ndkRoot, "toolchains/llvm/prebuilt");
  if (!existsSync(prebuilt)) return null;
  for (const host of ["linux-x86_64", "darwin-x86_64", "darwin-arm64"]) {
    const sysroot = join(prebuilt, host, "sysroot/usr/lib");
    if (existsSync(sysroot)) return sysroot;
  }
  return null;
}

export function copyLibcxxShared(ndkRoot, jniLibsDir) {
  const sysrootLib = ndkPrebuiltSysroot(ndkRoot);
  if (!sysrootLib) {
    throw new Error(`No se encontró sysroot del NDK en ${ndkRoot}`);
  }

  for (const [abiFolder, triple] of ABI_MAP) {
    const src = join(sysrootLib, triple, "libc++_shared.so");
    const destDir = join(jniLibsDir, abiFolder);
    const dest = join(destDir, "libc++_shared.so");
    if (!existsSync(src)) {
      throw new Error(`libc++_shared.so no encontrado: ${src}`);
    }
    mkdirSync(destDir, { recursive: true });
    copyFileSync(src, dest);
  }
}

export function jniLibsComplete(jniLibsDir) {
  const required = [
    "arm64-v8a/libha_game.so",
    "armeabi-v7a/libha_game.so",
    "arm64-v8a/libc++_shared.so",
    "armeabi-v7a/libc++_shared.so",
  ];
  return required.every((rel) => {
    const p = join(jniLibsDir, rel);
    return existsSync(p) && statSync(p).size > 100_000;
  });
}
