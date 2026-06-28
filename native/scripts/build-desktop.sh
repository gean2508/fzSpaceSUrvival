#!/usr/bin/env bash
# Release Linux binary for Hunger Arena native.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

cargo build -p ha-game --profile release-native
BIN="$ROOT/target/release-native/ha-game"
echo "✓ $BIN"
ls -lh "$BIN"
