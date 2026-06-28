#!/usr/bin/env bash
# Instala Hunger Arena en el celular conectado + smoke tests.
# Uso: ./scripts/install-android-ready.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APK="$ROOT/client/public/downloads/hunger-arena.apk"
PKG="com.hungerarena.app"

if ! command -v adb >/dev/null 2>&1; then
  echo "✗ adb no está en PATH"
  exit 1
fi

echo "== Starship — install + verify =="
adb devices -l

DEVICE="$(adb devices | awk 'NR>1 && $2=="device" {print $1; exit}')"
if [ -z "$DEVICE" ]; then
  echo "✗ No hay dispositivo conectado (USB debugging + autorizar PC)"
  exit 1
fi
echo "✓ Dispositivo: $DEVICE"

if [ ! -f "$APK" ]; then
  echo "✗ APK no encontrado: $APK"
  echo "  Corré: npm run ship:android"
  exit 1
fi

echo "Instalando $(basename "$APK") ($(du -h "$APK" | cut -f1))..."
if adb install -r --no-streaming "$APK"; then
  echo "✓ Instalado"
elif adb push "$APK" /data/local/tmp/hunger-arena.apk && adb shell pm install -r -t /data/local/tmp/hunger-arena.apk | rg -q "Success"; then
  echo "✓ Instalado vía pm install (workaround MIUI — adb install bloqueado)"
else
  echo ""
  echo "⚠ Si falla con INSTALL_FAILED_USER_RESTRICTED (MIUI):"
  echo "  Ajustes → Opciones desarrollador → Instalar vía USB → ON"
  echo "  Aceptá el popup en el teléfono e intentá de nuevo."
  exit 1
fi

echo ""
"$ROOT/scripts/verify-android-startup.sh"

echo ""
echo "✓ Listo — abrí Starship en el celular."
echo "  Logs perf/lifecycle: adb logcat | rg 'ha-perf|ha-span|ha-life|ha-game panic'"
