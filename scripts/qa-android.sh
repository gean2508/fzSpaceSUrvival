#!/usr/bin/env bash
# Corre los drivers de QA EN EL CELU y baja la evidencia (log, capturas, bugreport).
#
# Es el gemelo de `scripts/ha-suite.mjs` (que corre en el escritorio). Existe porque la
# regla 11 del CLAUDE.md pide probar en el Redmi antes de publicar, y hasta ahora los
# drivers sólo eran alcanzables por variables de entorno — que una app Android no tiene.
# `crate::flags` los expone como system properties `debug.ha.*`, escribibles por adb sin
# root, y esto es el arnés que las maneja.
#
#   ./scripts/qa-android.sh              # todos los drivers
#   ./scripts/qa-android.sh flight eco   # sólo esos
set -uo pipefail

PKG=com.hungerarena.app
ACT=$PKG/.MainActivity
REMOTE_DIR=/sdcard/Android/data/$PKG/files/ha-qa
OUT="${QA_OUT:-/tmp/ha-qa-android}"

# name:property:segundos.
#
# El timeout va MUY holgado y no es paranoia: `creative_fly` capa el paso de simulación a
# 1/20 s, así que con pocos fps el mundo avanza más lento que el reloj de pared. En el
# Redmi (9-11 fps medidos) un vuelo que en la compu tarda 38 s puede tardar el doble.
# Un timeout corto acá no se lee como "el celu va lento", se lee como "el driver falló".
DRIVERS=(
  "flight:flytest:240"
  "moon_climb:moontest:240"
  "landing_cycle:cycletest:520"
  "round_trip:triptest:640"
  "city:citytest:280"
  "eco:ecotest:280"
  "night:nighttest:280"
  "build:buildtest:280"
  "smoke:autodrive:300"
)
# Copia intacta de la lista completa: la selección de abajo pisa DRIVERS, y `limpiar_props`
# necesita apagar TODAS las palancas, no sólo las que se van a correr.
TODOS=("${DRIVERS[@]}")

if [ $# -gt 0 ]; then
  sel=()
  for want in "$@"; do
    for d in "${DRIVERS[@]}"; do [ "${d%%:*}" = "$want" ] && sel+=("$d"); done
  done
  DRIVERS=("${sel[@]}")
fi

adb get-state >/dev/null 2>&1 || { echo "✗ No hay celu por adb (¿cable? ¿tailnet?)"; exit 2; }
adb shell pm path $PKG >/dev/null 2>&1 || { echo "✗ $PKG no está instalado"; exit 2; }
mkdir -p "$OUT"

# Apaga TODAS las palancas: una property de una corrida anterior se queda pegada y
# arrancaría dos drivers a la vez.
# OJO con el parseo: cada entrada es `nombre:prop:segundos`, así que la property es el campo
# del MEDIO. Un `${d#*:}` a secas deja `flytest:240`, o sea crea una property fantasma con
# ese nombre mientras la de verdad sigue prendida. Y como `autodrive.rs` chequea
# `HA_FLYTEST` primero, TODOS los drivers terminaban corriendo flytest y la suite daba
# verde sin haber probado nada. Pasó de verdad (3 ago 2026).
#
# Se limpia sobre `TODOS`, no sobre la selección: si corrés `qa-android.sh eco` después de
# una corrida entera, las palancas de la anterior siguen puestas en el device.
limpiar_props() {
  for d in "${TODOS[@]}"; do
    rest="${d#*:}"
    adb shell setprop "debug.ha.${rest%%:*}" '""' >/dev/null 2>&1
  done
  adb shell setprop debug.ha.autoplay '""' >/dev/null 2>&1
}

# ¿Hay EXACTAMENTE una palanca de driver prendida, y es la que creemos? Sin esto un error
# de limpieza se ve como una suite en verde. Barato: un `getprop` por driver.
# Sólo mira palancas de DRIVER. La primera versión miraba todo `debug.ha.*` y se colgaba
# con cualquier palanca de diagnóstico prendida (`debug.ha.ramart`, `debug.ha.dayphase`…),
# que son ortogonales: no eligen driver, sólo agregan medición. Rechazaba las 9 corridas.
# Con REINTENTOS: cada `adb shell` es una conexión nueva y el servicio de properties no
# siempre publica el valor antes de que vuelva el `getprop` siguiente. Sin esperar, el
# chequeo daba "sucias" de forma intermitente —8 de 9 drivers saltados, y el noveno
# corriendo bien— que es peor que no tenerlo: parece un problema del juego.
verificar_una_sola() {
  nombres=""
  for d in "${TODOS[@]}"; do rest="${d#*:}"; nombres="$nombres ${rest%%:*}"; done
  for _ in $(seq 1 12); do
    prendidas=""
    volcado=$(adb shell getprop 2>/dev/null | tr -d '\r')
    for p in $nombres; do
      if echo "$volcado" | grep -qE "^\[debug\.ha\.$p\]: \[[^]]+\]$"; then
        prendidas="$prendidas $p"
      fi
    done
    prendidas="${prendidas# }"
    [ "$prendidas" = "$1" ] && return 0
    sleep 0.5
  done
  echo "    (palancas vistas: '${prendidas:-ninguna}')" >&2
  return 1
}

fallos=0
echo "▶ QA en el celu — $(adb shell getprop ro.product.model | tr -d '\r')"
for d in "${DRIVERS[@]}"; do
  name="${d%%:*}"; rest="${d#*:}"; prop="${rest%%:*}"; secs="${rest##*:}"
  printf '  · %-14s ' "$name"

  adb shell am force-stop $PKG
  adb shell rm -rf "$REMOTE_DIR" >/dev/null 2>&1
  limpiar_props
  # El `setprop` se PIERDE de a ratos: el servicio de properties es asíncrono y, con la
  # ráfaga de limpieza justo antes, a veces el borrado pisa al alta. Se veía como
  # "palancas vistas: ninguna" en 5 de 9 drivers. Se reintenta el ALTA, no sólo el
  # chequeo, que era el error de la vuelta anterior.
  puesta=0
  for _ in 1 2 3 4 5 6; do
    adb shell setprop "debug.ha.$prop" 1
    adb shell setprop debug.ha.autoplay 1
    if verificar_una_sola "$prop"; then puesta=1; break; fi
    sleep 1
  done
  if [ "$puesta" != 1 ]; then
    echo "✗ no se pudo prender debug.ha.$prop en el device — se saltea"
    fallos=$((fallos + 1))
    continue
  fi
  adb logcat -c >/dev/null 2>&1

  adb shell am start -n $ACT >/dev/null 2>&1
  # El driver termina con `exit()`; se espera a que el proceso muera o a que se acabe el
  # presupuesto. Sondear `pidof` es más fiable que dormir a ciegas.
  vivo=1
  for _ in $(seq 1 "$secs"); do
    sleep 1
    if ! adb shell pidof $PKG >/dev/null 2>&1; then vivo=0; break; fi
  done

  adb logcat -d > "$OUT/$name.log" 2>&1
  adb pull "$REMOTE_DIR" "$OUT/$name" >/dev/null 2>&1
  adb shell am force-stop $PKG >/dev/null 2>&1

  # El veredicto sale del log: un driver que rompe un invariante deja `ha-bug[...]` y se
  # mata solo. `pidof` por sí solo no distingue "terminó bien" de "se cerró de una".
  if grep -q 'ha-bug\[' "$OUT/$name.log"; then
    echo "✗ invariante roto — $(grep -m1 -o 'ha-bug\[.*' "$OUT/$name.log")"
    fallos=$((fallos + 1))
  # OJO CON EL ORDEN: el `DONE` va ANTES que el chequeo de crash.
  #
  # El juego tiene un crash de CIERRE (mutex ya destruido en el teardown de audio; se ve
  # como `FORTIFY: pthread_mutex_lock called on a destroyed mutex` seguido de SIGSEGV, y es
  # anterior a esta suite). Con el crash chequeado primero, un driver que hizo TODO su
  # trabajo y escribió `DONE` se reportaba como "✗ crash" por reventar al salir. Eso es un
  # falso rojo, y un falso rojo entrena a ignorar el reporte igual que un falso verde.
  #
  # Un crash de verdad —el que interesa— pasa ANTES de terminar, así que no hay `DONE` en
  # el log y cae igual en la rama de abajo. El de cierre se sigue avisando, pero sin
  # invalidar el veredicto del driver.
  elif grep -qE 'DONE|PASS' "$OUT/$name.log"; then
    fps=$(grep -o 'fps=[0-9]*' "$OUT/$name.log" | tail -20 | cut -d= -f2 \
          | awk '{s+=$1; n++} END {if (n) printf "%.0f", s/n; else print "?"}')
    if grep -qE 'FATAL EXCEPTION|signal 11|ha-crash' "$OUT/$name.log"; then
      echo "✓ (fps medio ${fps}) · ojo: reventó AL CERRAR (ver $OUT/$name.log)"
    else
      echo "✓ (fps medio ${fps})"
    fi
  elif grep -qE 'FATAL EXCEPTION|signal 11|ha-crash' "$OUT/$name.log"; then
    echo "✗ crash ANTES de terminar (ver $OUT/$name.log)"
    fallos=$((fallos + 1))
  elif [ "$vivo" = 1 ]; then
    echo "✗ se quedó colgado (sin DONE en ${secs}s)"
    fallos=$((fallos + 1))
  else
    echo "✗ murió sin DONE (ver $OUT/$name.log)"
    fallos=$((fallos + 1))
  fi
done

limpiar_props
echo
echo "Evidencia en $OUT  ·  drivers con falla: $fallos"
exit $((fallos > 0 ? 1 : 0))
