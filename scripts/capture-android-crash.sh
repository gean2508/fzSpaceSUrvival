#!/usr/bin/env bash
# Capture Android crash log for Hunger Arena APK diagnosis.
# Usage: ./scripts/capture-android-crash.sh
# Requires: USB debugging, device authorized (adb devices shows device)
set -euo pipefail

if ! command -v adb >/dev/null; then
  echo "adb no encontrado. Instalá Android platform-tools."
  exit 1
fi

DEVICES=$(adb devices | grep -v "List" | grep "device$" | wc -l)
if [ "$DEVICES" -eq 0 ]; then
  echo "No hay dispositivo USB autorizado. Conectá el celu y activá depuración USB."
  adb devices
  exit 1
fi

OUT="${1:-/tmp/hunger-arena-crash.log}"
echo "Limpiando logcat…"
adb logcat -c
echo "Abrí Hunger Arena en el celular AHORA (tienes 15s)…"
timeout 15 adb logcat -v brief \
  AndroidRuntime:E \
  libc:F \
  DEBUG:F \
  *:S \
  | tee "$OUT" || true

# Also capture anything tagged with our package / native lib
timeout 15 adb logcat -d 2>/dev/null | grep -iE 'hungerarena|ha_game|FATAL EXCEPTION|UnsatisfiedLinkError|signal [0-9]+' >> "$OUT" || true

if grep -qiE 'FATAL EXCEPTION|UnsatisfiedLinkError|signal [0-9]+|hungerarena' "$OUT"; then
  echo ""
  echo "══ CRASH / ERROR (filtrado) ══"
  grep -iE 'FATAL EXCEPTION|UnsatisfiedLinkError|signal [0-9]+|hungerarena|ha_game|AndroidRuntime' "$OUT" | head -80
  echo ""
  echo "Log completo: $OUT"
else
  echo "No se encontró crash de Hunger Arena en 15s."
  echo "Probá: adb logcat -c && adb shell am start -n com.hungerarena.app/.MainActivity && adb logcat -d | grep -iE 'hungerarena|FATAL|UnsatisfiedLink'"
  echo "Log parcial: $OUT"
fi
