#!/usr/bin/env bash
# Smoke-check Android startup stability for Hunger Arena.
# Fails if panic/crash/ANR signatures appear in logcat right after launch.
set -euo pipefail

PKG="com.hungerarena.app"
ACT="$PKG/.MainActivity"
WAIT_SECS="${1:-12}"

if ! command -v adb >/dev/null 2>&1; then
  echo "adb no encontrado en PATH."
  exit 1
fi

echo "== Android startup verify =="
adb devices -l

echo "Limpiando logs y relanzando app..."
adb logcat -c
adb shell am force-stop "$PKG" || true
adb shell monkey -p "$PKG" -c android.intent.category.LAUNCHER 1 >/dev/null

echo "Esperando ${WAIT_SECS}s..."
sleep "$WAIT_SECS"

SIG_PATTERN="ha-game panic|error\\[B0001\\]|Encountered a panic|FATAL EXCEPTION|am_crash|am_anr|Process $PKG .* has died|lowmemorykiller: Kill '$PKG'"
if adb logcat -d | rg -i "$SIG_PATTERN" >/tmp/hunger-arena-startup-verify.log; then
  echo "✗ Startup NO estable: se detectaron firmas de crash/panic"
  echo "---- matches ----"
  cat /tmp/hunger-arena-startup-verify.log
  exit 1
fi

PID="$(adb shell pidof "$PKG" | tr -d '\r' || true)"
if [ -z "$PID" ]; then
  echo "✗ Startup dudoso: app no está viva al finalizar la ventana de chequeo."
  exit 1
fi

echo "✓ Startup estable (pid=$PID, sin crash signatures)"
