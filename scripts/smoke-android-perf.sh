#!/usr/bin/env bash
# Quick perf smoke — launch app, sample ha-perf for DURATION_SECS, append perf-history.jsonl.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PKG="com.hungerarena.app"
DURATION="${1:-45}"
INTERVAL="${2:-5}"

adb devices -l
adb logcat -c
adb shell am force-stop "$PKG" 2>/dev/null || true
adb shell monkey -p "$PKG" -c android.intent.category.LAUNCHER 1 >/dev/null 2>&1
echo "Esperando arranque 15s..."
sleep 15

echo "=== ha-perf (${DURATION}s) ==="
END=$((SECONDS + DURATION))
while [ "$SECONDS" -lt "$END" ]; do
  sleep "$INTERVAL"
  adb logcat -d | rg "ha-perf " | tail -n 1 || true
done

echo "=== resumen ==="
adb logcat -d | rg "ha-perf " | tail -n 20

# Read version from native Cargo.toml if not set
export APP_VERSION="${APP_VERSION:-$(rg '^version' "$ROOT/native/Cargo.toml" -m1 | cut -d'"' -f2)}"
chmod +x "$ROOT/scripts/append-perf-history.sh"
"$ROOT/scripts/append-perf-history.sh"
