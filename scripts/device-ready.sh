#!/usr/bin/env bash
# Espera celular USB → instala APK Starship → verify startup → muestra ha-perf.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APK="$ROOT/client/public/downloads/hunger-arena.apk"
PKG="com.hungerarena.app"
WAIT_MAX="${1:-120}"

echo "== Starship device-ready =="
echo "Conectá el celu (USB debugging ON). Esperando hasta ${WAIT_MAX}s..."

for ((i=1; i<=WAIT_MAX; i++)); do
  if adb devices | awk 'NR>1 && $2=="device" {found=1} END{exit !found}'; then
    echo "✓ Dispositivo detectado"
    break
  fi
  sleep 1
  if [ "$i" -eq "$WAIT_MAX" ]; then
    echo "✗ Timeout — no hay dispositivo"
    exit 1
  fi
done

adb devices -l

if [ ! -f "$APK" ]; then
  echo "✗ APK no encontrado. Corré: npm run ship:android"
  exit 1
fi

VER=$(node -p "JSON.parse(require('fs').readFileSync('$ROOT/native/version.json')).version")
echo "Instalando v${VER} ($(du -h "$APK" | cut -f1))..."

if adb install -r --no-streaming "$APK"; then
  echo "✓ Instalado"
elif adb push "$APK" /data/local/tmp/starship.apk && adb shell pm install -r -t /data/local/tmp/starship.apk | rg -q "Success"; then
  echo "✓ Instalado vía pm install (MIUI workaround)"
else
  echo "✗ Install falló — activá Instalar vía USB en MIUI"
  exit 1
fi

"$ROOT/scripts/verify-android-startup.sh" 15

echo ""
echo "=== ha-perf (20s post-arranque) ==="
sleep 5
adb logcat -d | rg "ha-perf " | tail -n 3 || true

echo ""
echo "=== ha-span CPU (últimos 60f avg) ==="
adb logcat -d | rg "ha-span " | tail -n 5 || true

if [ -x "$ROOT/scripts/append-perf-history.sh" ]; then
  export APP_VERSION="$VER"
  "$ROOT/scripts/append-perf-history.sh" || true
fi

echo ""
echo "✓ Listo — Starship v${VER} en el celu. Android: MENU → PERF = overlay; ha-perf en logcat cada 5s."
