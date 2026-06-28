#!/usr/bin/env bash
# Build Android APK (arm64 + armeabi-v7a) for Hunger Arena native.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ANDROID="$ROOT/android"
JNI_LIBS="$ANDROID/app/src/main/jniLibs"
cd "$ROOT"

: "${ANDROID_HOME:?Set ANDROID_HOME}"
export ANDROID_NDK_ROOT="${ANDROID_NDK_ROOT:-${NDK_HOME:-$ANDROID_HOME/ndk/$(ls "$ANDROID_HOME/ndk" | sort -V | tail -1)}}"
export NDK_HOME="$ANDROID_NDK_ROOT"

mkdir -p "$JNI_LIBS"

cargo ndk -t arm64-v8a -t armeabi-v7a -o "$JNI_LIBS" build -p ha-game --profile release-native --features android

VERSION_JSON="$ROOT/version.json"
VERSION_CODE=$(node -e "console.log(JSON.parse(require('fs').readFileSync('$VERSION_JSON','utf8')).versionCode)")
VERSION_NAME=$(node -e "console.log(JSON.parse(require('fs').readFileSync('$VERSION_JSON','utf8')).version)")

cd "$ANDROID"
./gradlew assembleDebug -PversionCode="$VERSION_CODE" -PversionName="$VERSION_NAME"

APK="$ANDROID/app/build/outputs/apk/debug/app-debug.apk"
echo "✓ APK → $APK"
