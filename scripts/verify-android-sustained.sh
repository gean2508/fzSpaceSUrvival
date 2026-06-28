#!/usr/bin/env bash
# Sustained-session smoke check: keeps Hunger Arena running and samples perf/lifecycle logs.
# MIUI blocks `adb shell input keyevent` (INJECT_EVENTS) — background cycle uses am start/stop instead.
set -euo pipefail

PKG="com.hungerarena.app"
ACT="$PKG/.MainActivity"
DURATION_SECS="${1:-600}"
SAMPLE_EVERY="${2:-30}"

if ! command -v adb >/dev/null 2>&1; then
  echo "adb no encontrado en PATH."
  exit 1
fi

echo "== Android sustained verify (${DURATION_SECS}s, sample every ${SAMPLE_EVERY}s) =="
adb devices -l

adb logcat -c
adb shell am force-stop "$PKG" || true
adb shell monkey -p "$PKG" -c android.intent.category.LAUNCHER 1 >/dev/null

echo "Esperando arranque (12s)..."
sleep 12

PID="$(adb shell pidof "$PKG" | tr -d '\r' || true)"
if [ -z "$PID" ]; then
  echo "✗ App no arrancó."
  exit 1
fi
echo "✓ App viva pid=$PID"

echo "Simulando background vía force-stop + relaunch (sin INJECT_EVENTS)..."
adb shell am force-stop "$PKG"
sleep 3
adb shell monkey -p "$PKG" -c android.intent.category.LAUNCHER 1 >/dev/null
sleep 4
if adb logcat -d | rg "ha-life paused=true" >/dev/null; then
  echo "✓ Pause detectado en logcat (sesión previa)"
else
  echo "⚠ Pause no visible en log reciente — lifecycle se valida mejor con sesión manual:"
  echo "  1. Con logcat corriendo, tocá HOME en el teléfono 3s"
  echo "  2. Volvé a Hunger Arena desde recientes"
  echo "  3. Buscá: ha-life paused=true / paused=false"
fi

PID="$(adb shell pidof "$PKG" | tr -d '\r' || true)"
if [ -z "$PID" ]; then
  echo "✗ App no relanzó tras force-stop."
  exit 1
fi
echo "✓ App relanzada pid=$PID"

CRASH_PATTERN="ha-game panic|FATAL EXCEPTION|am_crash|am_anr|Process $PKG .* has died"
SAMPLES=0
ELAPSED=0
while [ "$ELAPSED" -lt "$DURATION_SECS" ]; do
  sleep "$SAMPLE_EVERY"
  ELAPSED=$((ELAPSED + SAMPLE_EVERY))
  SAMPLES=$((SAMPLES + 1))

  if adb logcat -d | rg -i "$CRASH_PATTERN" | tail -n 3 | rg -q .; then
    echo "✗ Crash detectado durante sesión sostenida (${ELAPSED}s)"
    adb logcat -d | rg -i "$CRASH_PATTERN" | tail -n 20
    exit 1
  fi

  PID_NOW="$(adb shell pidof "$PKG" | tr -d '\r' || true)"
  if [ -z "$PID_NOW" ]; then
    echo "✗ App murió durante sesión (${ELAPSED}s)"
    exit 1
  fi

  PERF_LINE="$(adb logcat -d | rg "ha-perf " | tail -n 1 || true)"
  echo "[$ELAPSED s] pid=$PID_NOW ${PERF_LINE:-sin ha-perf aún}"
done

echo "✓ Sesión sostenida OK (${SAMPLES} muestras, ${DURATION_SECS}s total, sin crash)"
echo "Tip: overlay F3 en desktop; en Android ha-perf sale cada 5s (draw3d, ui, ocean, chunk…)."
