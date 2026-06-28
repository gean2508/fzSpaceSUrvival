#!/usr/bin/env bash
# One-time Android toolchain for Hunger Arena native (Bevy).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if [[ -z "${ANDROID_HOME:-}" && -z "${ANDROID_SDK_ROOT:-}" ]]; then
  echo "Set ANDROID_HOME or ANDROID_SDK_ROOT to your Android SDK."
  exit 1
fi

SDK="${ANDROID_HOME:-$ANDROID_SDK_ROOT}"
export ANDROID_HOME="$SDK"
export NDK_HOME="${NDK_HOME:-$SDK/ndk/$(ls "$SDK/ndk" 2>/dev/null | sort -V | tail -1)}"

if [[ ! -d "$NDK_HOME" ]]; then
  echo "NDK not found. Install via Android Studio SDK Manager."
  exit 1
fi

echo "Using NDK: $NDK_HOME"

rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android

if ! command -v cargo-ndk >/dev/null; then
  cargo install cargo-ndk
fi

echo "✓ Android rust targets + cargo-ndk ready"
echo "  Build: ./scripts/build-android.sh"
