/** Resolve Android SDK/NDK paths for cargo-apk and cargo-ndk. */
import { execSync } from "node:child_process";
import { existsSync, readdirSync } from "node:fs";
import { resolve, join } from "node:path";

function latestNdkVersion(sdk) {
  const ndkDir = join(sdk, "ndk");
  if (!existsSync(ndkDir)) return null;
  const versions = readdirSync(ndkDir)
    .filter((name) => existsSync(join(ndkDir, name)))
    .sort((a, b) => a.localeCompare(b, undefined, { numeric: true }));
  return versions.at(-1) ?? null;
}

function ndkToolchainBin(ndk) {
  const prebuilt = join(ndk, "toolchains/llvm/prebuilt");
  if (!existsSync(prebuilt)) return null;
  const host = readdirSync(prebuilt).find((name) => existsSync(join(prebuilt, name, "bin")));
  return host ? join(prebuilt, host, "bin") : null;
}

export function resolveAndroidEnv(base = process.env) {
  const sdk = base.ANDROID_HOME ?? base.ANDROID_SDK_ROOT;
  if (!sdk) {
    throw new Error("ANDROID_HOME no configurado — instalá Android SDK y exportá ANDROID_HOME.");
  }

  let ndk = base.ANDROID_NDK_ROOT ?? base.NDK_HOME;
  if (!ndk) {
    const version = latestNdkVersion(sdk);
    if (!version) throw new Error(`No hay NDK en ${join(sdk, "ndk")}`);
    ndk = join(sdk, "ndk", version);
  }

  if (!existsSync(ndk)) {
    throw new Error(`Android NDK no encontrado: ${ndk}`);
  }

  const env = {
    ANDROID_HOME: sdk,
    ANDROID_NDK_ROOT: ndk,
    NDK_HOME: ndk,
  };

  const toolchainBin = ndkToolchainBin(ndk);
  if (toolchainBin) {
    env.PATH = `${toolchainBin}:${base.PATH ?? ""}`;
  }

  return env;
}
