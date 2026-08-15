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

# SÓLO arm64. Antes acá decía `-t arm64-v8a -t armeabi-v7a`, o sea que se compilaba el
# `.so` de 32 bits COMPLETO —con LTO, que es lo caro— y después `build.gradle` lo tiraba:
# su `abiFilters` sale de `abiProp`, que por defecto es `arm64-v8a` a secas. Se puede
# comprobar en cualquier APK generado: adentro sólo hay `lib/arm64-v8a/`.
#
# O sea: la mitad de cada build se iba en un binario que nunca se empaquetó. Y no era
# gratis en otro sentido tampoco — un `armeabi-v7a/` viejo quedándose en `jniLibs/` ya
# hizo que se publicara un APK con código rancio adentro (biblia §9).
#
# El Redmi es arm64, como cualquier teléfono de la última década. Si alguna vez hace falta
# 32 bits, se pasa `ANDROID_ABIS=arm64-v8a,armeabi-v7a` y hay que agregar el target ACÁ
# también: las dos listas tienen que decir lo mismo.
ABIS="${ANDROID_ABIS:-arm64-v8a}"
CARGO_NDK_TARGETS=""
for abi in ${ABIS//,/ }; do CARGO_NDK_TARGETS="$CARGO_NDK_TARGETS -t $abi"; done

# Un `.so` de una arquitectura que ya no se compila se queda en `jniLibs/` y se cuela en el
# APK. Se limpia antes de construir.
for d in "$JNI_LIBS"/*/; do
  abi="$(basename "$d")"
  case ",$ABIS," in *",$abi,"*) ;; *) echo "· descarto jniLibs/$abi (no está en ABIS)"; rm -rf "$d" ;; esac
done

# shellcheck disable=SC2086
cargo ndk $CARGO_NDK_TARGETS -o "$JNI_LIBS" build -p ha-game --profile release-native --features android

VERSION_JSON="$ROOT/version.json"
VERSION_CODE=$(node -e "console.log(JSON.parse(require('fs').readFileSync('$VERSION_JSON','utf8')).versionCode)")
VERSION_NAME=$(node -e "console.log(JSON.parse(require('fs').readFileSync('$VERSION_JSON','utf8')).version)")

cd "$ANDROID"
# `assembleRelease`, no `assembleDebug`: un APK debug lleva `debuggable=true`,
# que apaga optimizaciones de ART y activa los chequeos JNI en cada llamada
# nativa — o sea, se pagaba en el dispositivo con menos margen. El Rust ya salía
# optimizado (`release-native`); lo que faltaba era que el envoltorio Java también.
# Se firma con la clave de debug (ver `app/build.gradle`) porque esto se instala
# de costado, no por Play.
./gradlew assembleRelease -PversionCode="$VERSION_CODE" -PversionName="$VERSION_NAME"

APK="$ANDROID/app/build/outputs/apk/release/app-release.apk"
echo "✓ APK → $APK"
