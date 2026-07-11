#!/usr/bin/env bash
# Batch-generate GLB assets into the game models directory (headless Blender).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BLENDER="${BLENDER:-blender}"
SCRIPT="$ROOT/tools/blender/generate_assets.py"

if ! command -v "$BLENDER" >/dev/null 2>&1; then
  echo "error: blender not found. Install Blender 5.x or set BLENDER=/path/to/blender" >&2
  exit 1
fi

echo "ha-blender: using $("$BLENDER" --version | head -1)"
echo "ha-blender: script $SCRIPT"
"$BLENDER" --background --python "$SCRIPT" -- "$@"
echo "ha-blender: done. Game models: $ROOT/native/crates/ha-game/assets/models/"
