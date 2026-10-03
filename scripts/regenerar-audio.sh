#!/usr/bin/env bash
# Regenera TODO el audio sintetizado del juego desde `ha-audio/src/defaults.rs` (el diseño de sonido):
#   native/assets/audio.json                      (lo embebe ha-game; los SFX y la música se sintetizan en boot)
#   native/crates/ha-game/assets/audio/splash.wav  (la marca de FZ STUDIOS; es un WAV porque suena antes del banco)
# y corre el laboratorio (`scripts/sfx-lab.py`) para auditar niveles, clipping y silencios.
set -euo pipefail
cd "$(dirname "$0")/../native"
timeout 900 cargo run -q -p ha-audio --profile qa --bin export_default > assets/audio.json
timeout 900 cargo run -q -p ha-audio --profile qa --bin render_all -- /tmp/audio-lab
cp /tmp/audio-lab/sfx_splash_logo.wav crates/ha-game/assets/audio/splash.wav
echo "audio.json y splash.wav regenerados"
python3 ../scripts/sfx-lab.py "$@"
