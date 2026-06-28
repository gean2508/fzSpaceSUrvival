#!/usr/bin/env bash
# Launch HA Asset Viewer (local dev UI, not part of game runtime).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DIR="$(cd "$(dirname "$0")" && pwd)"

cd "$DIR"
if [[ ! -d node_modules ]]; then
  echo "ha-asset-viewer: installing deps…"
  npm install --no-fund --no-audit
fi

echo "ha-asset-viewer: repo $ROOT"
echo "ha-asset-viewer: http://localhost:5199"
exec npm run dev
