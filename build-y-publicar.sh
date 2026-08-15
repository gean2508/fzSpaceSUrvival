#!/usr/bin/env bash
#
# build-y-publicar.sh — UN solo comando para: compilar el APK del juego,
# publicarlo en la landing, y levantar el servidor + la web de descarga.
# Después abrís la URL en el celu (misma WiFi) y descargás el APK.
#
# Pensado para correrlo VOS, sin depender de la IA (cuando te quedás sin tokens).
#
# Uso:
#   ./build-y-publicar.sh              # build completo + tests + publica + sirve
#   ./build-y-publicar.sh sin-tests    # igual pero SALTA los tests (evita OOM en PC de poca RAM)
#   ./build-y-publicar.sh remoto       # como completo pero hornea la IP de Tailscale en el
#                                      #   APK → descargás/jugás FUERA de la LAN (celu en el tailnet)
#   ./build-y-publicar.sh remoto sin-tests   # remoto, saltando los tests
#   ./build-y-publicar.sh rapido       # NO recompila: sirve el APK ya publicado
#   ./build-y-publicar.sh editor       # abre SOLO el editor de assets (no compila el juego)
#   ./build-y-publicar.sh sonido       # abre SOLO el editor de sonido / mini-DAW
#
# Requisitos (una vez): ver biblia.md §9 — Node + Rust + Android NDK/SDK + `cargo ndk`.
#
set -euo pipefail

# Ir a la raíz del repo (donde vive este script), sin importar desde dónde lo llames.
cd "$(dirname "$0")"

MODO="${1:-completo}"

case "$MODO" in
  editor)
    echo "▶ Abriendo el editor de assets (dev). Cerralo con la X de la ventana."
    echo "  (esto NO compila el juego; sólo edita el manifest.json de arte)"
    exec ./editor_assets gui
    ;;
  sonido)
    echo "▶ Abriendo el editor de sonido (dev). Cerralo con la X de la ventana."
    echo "  (edita native/assets/audio.json; guardá y recompilá para oírlo en el juego)"
    exec ./editor_assets sonido
    ;;
  mundo|mapa)
    echo "▶ Abriendo el editor de MUNDO (dev, WYSIWYG). Cerralo con la X de la ventana."
    echo "  (H1: vista del mundo procedural; los pinceles de overrides llegan en H2)"
    exec ./editor_assets mundo
    ;;
  herramientas|tools)
    echo "▶ Hub de HERRAMIENTAS REMOTAS — editá assets/mundo/música desde el celu por Tailscale."
    echo "  (corre en esta PC; abrí la URL Tailscale que imprime, en el navegador del celu)"
    exec node scripts/tools-server.mjs
    ;;
  rapido)
    echo "▶ Modo RÁPIDO — sin recompilar, sirviendo el APK ya publicado."
    exec npm run play -- --no-ship
    ;;
  sin-tests)
    echo "▶ Build del APK SIN gates de test (más liviano en RAM) + publica + sirve."
    exec npm run play -- --skip-tests
    ;;
  remoto|tailscale)
    TS_IP="$(tailscale ip -4 2>/dev/null | head -1)"
    if [ -z "$TS_IP" ]; then
      echo "✗ Tailscale no responde. Encendé/instalá Tailscale (probá: tailscale status) y reintentá."
      exit 1
    fi
    EXTRA=""
    if [ "${2:-}" = "sin-tests" ]; then EXTRA="--skip-tests"; fi
    echo "▶ Build del APK apuntando a Tailscale ($TS_IP) + publica + sirve."
    echo "  Descargás/jugás desde el celu estando en el tailnet, DENTRO o FUERA de casa."
    echo "  (el celu tiene que estar ONLINE en Tailscale — verificá con: tailscale status)"
    exec npm run play -- --host "$TS_IP" $EXTRA
    ;;
  completo)
    echo "▶ Build completo del APK + tests + publica + sirve."
    echo "  (si tu PC tiene poca RAM y se cuelga en los tests, usá:  ./build-y-publicar.sh sin-tests)"
    exec npm run play
    ;;
  *)
    echo "Modo desconocido: '$MODO'"
    echo "Usá: ./build-y-publicar.sh [completo|sin-tests|remoto|herramientas|rapido|editor|sonido|mundo]"
    exit 1
    ;;
esac
