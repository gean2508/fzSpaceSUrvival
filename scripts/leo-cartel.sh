#!/usr/bin/env bash
# Cartel flotante de "EN PRUEBAS" en el telefono, mientras dure la prueba.
#
#   leo-cartel.sh "instalando APK"     -> muestra/actualiza el renglon de abajo
#   leo-cartel.sh --fin                -> lo saca
#
# El juego lo dibuja solo si el archivo existe (ver `ha-game/src/prueba_overlay.rs`),
# asi que apagarlo es borrarlo: no queda nada encendido "para siempre".
#
# Se llama entre paso y paso de una prueba con `leo-celu`, para que desde el
# telefono se vea que esta pasando sin tener que mirar la terminal.
set -uo pipefail

RUTA="/sdcard/Android/data/com.hungerarena.app/files/ha-prueba"

if [ "${1:-}" = "--fin" ]; then
  leo-celu shell "rm -f $RUTA" >/dev/null 2>&1
  exit 0
fi

TEXTO="${1:-probando}"
# El directorio existe recien despues de la primera instalacion; -p no molesta.
leo-celu shell "mkdir -p $(dirname $RUTA); echo '${TEXTO//\'/}' > $RUTA" >/dev/null 2>&1
