#!/usr/bin/env bash
# Append last ha-perf line from logcat to perf-history.jsonl (append-only trend log).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
HISTORY="${PERF_HISTORY:-$ROOT/perf-history.jsonl}"
VERSION="${APP_VERSION:-unknown}"

LAST=$(adb logcat -d 2>/dev/null | rg "ha-perf " | tail -n 1 || true)
if [ -z "$LAST" ]; then
  echo "No ha-perf line found — run smoke-android-perf.sh first" >&2
  exit 1
fi

# Parse: ha-perf fps=16 frame_ms=62.1 jitter=... res=2297x1080 draw3d~=62 ui=77 ents=332 ocean=... ground=... chunk=... obs=... nodes=... outline=... other3d=...
parse_field() {
  echo "$LAST" | rg -o "$1=[^ ]+" | head -1 | cut -d= -f2
}

FPS=$(parse_field "fps")
FRAME_MS=$(parse_field "frame_ms")
JITTER=$(parse_field "jitter")
RES=$(parse_field "res")
DRAW3D=$(parse_field "draw3d~")
UI=$(parse_field "ui")
ENTS=$(parse_field "ents")
OCEAN=$(parse_field "ocean")
GROUND=$(parse_field "ground")
CHUNK=$(parse_field "chunk")
OBS=$(parse_field "obs")
NODES=$(parse_field "nodes")
OUTLINE=$(parse_field "outline")
OTHER3D=$(parse_field "other3d")

TS=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

ENTRY=$(cat <<EOF
{"ts":"$TS","version":"$VERSION","fps":$FPS,"frame_ms":$FRAME_MS,"jitter_ms":$JITTER,"res":"$RES","draw3d":$DRAW3D,"ui":$UI,"ents":$ENTS,"ocean":$OCEAN,"ground":$GROUND,"chunk":$CHUNK,"obs":$OBS,"nodes":$NODES,"outline":$OUTLINE,"other3d":$OTHER3D}
EOF
)

echo "$ENTRY" >> "$HISTORY"
echo "✓ Appended to $HISTORY"
echo "$ENTRY"

# Compare to previous entry if present
PREV=$(tail -n 2 "$HISTORY" 2>/dev/null | head -n 1 || true)
if [ -n "$PREV" ] && [ "$(wc -l < "$HISTORY")" -ge 2 ]; then
  PREV_FPS=$(echo "$PREV" | rg -o '"fps":[0-9.]+' | cut -d: -f2)
  if [ -n "$PREV_FPS" ] && [ -n "$FPS" ]; then
    DELTA=$(awk "BEGIN {printf \"%.1f\", $FPS - $PREV_FPS}")
    echo "Δ fps vs previous: ${DELTA}"
  fi
fi
