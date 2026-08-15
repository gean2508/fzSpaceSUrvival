#!/usr/bin/env bash
# ESPERAR-EL-CELU — se queda mirando hasta que el teléfono aparece, y ahí
# arranca solo: instala el APK, lanza el juego, espera, captura y vuelca el log.
#
# Es la regla 12 del proyecto (probar en el celu ANTES de publicar) sin tener que
# estar delante: se deja corriendo al terminar los cambios y el ciclo se dispara
# en cuanto el teléfono entra al tailnet.
#
#   ./scripts/esperar-el-celu.sh                 # espera indefinidamente
#   MINUTOS=30 ./scripts/esperar-el-celu.sh      # se rinde a la media hora
#
# Deja todo en /tmp/ha-celu-espera.log y la captura donde la ponga `leo-celu
# probar` (/tmp/leo-celu-prueba.png).
set -uo pipefail

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOG=/tmp/ha-celu-espera.log
MINUTOS="${MINUTOS:-0}"          # 0 = sin límite
CADA="${CADA:-20}"               # segundos entre intentos

di() { echo "[$(date +%H:%M:%S)] $*" | tee -a "$LOG"; }

: > "$LOG"
di "esperando al celu (reintento cada ${CADA}s)…"

inicio=$(date +%s)
until leo-celu conectar >>"$LOG" 2>&1 && adb devices | grep -qE '\sdevice$'; do
  if [ "$MINUTOS" -gt 0 ]; then
    ahora=$(date +%s)
    if [ $(( (ahora - inicio) / 60 )) -ge "$MINUTOS" ]; then
      di "el celu no apareció en $MINUTOS min — probá a mano con 'leo-celu conectar'"
      exit 2
    fi
  fi
  sleep "$CADA"
done

di "celu conectado:"
leo-celu estado 2>&1 | tee -a "$LOG"

# El APK tiene que estar compilado. Si el build todavía corre, se espera; si no
# existe, se compila acá — instalar el de ayer no probaría nada.
APK="$RAIZ/native/android/app/build/outputs/apk/release/app-release.apk"
[ -f "$APK" ] || APK=""
if [ -z "$APK" ]; then
  di "no hay APK compilado: compilando (esto tarda)…"
  ( cd "$RAIZ/native" && ./scripts/build-android.sh ) >>"$LOG" 2>&1 || {
    di "el build del APK falló — mirá $LOG"
    exit 1
  }
fi

di "instalando y probando en el teléfono…"
if leo-celu probar 2>&1 | tee -a "$LOG"; then
  di "la app quedó VIVA. Mirá igual la captura: un proceso vivo con la pantalla"
  di "en negro sigue siendo un fallo → /tmp/leo-celu-prueba.png"
else
  di "la prueba en el celu FALLÓ — el log de arriba tiene el motivo"
  exit 1
fi
