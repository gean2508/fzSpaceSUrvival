#!/usr/bin/env bash
# Captura de perf POR ESCENARIO, para poder comparar antes/después con el mismo
# criterio en vez de "me pareció que iba mejor".
#
# Cada escenario es un archivo aparte en perf-runs/<etiqueta>/<escenario>.log con
# TODAS las líneas de sonda del rato que duró. La lectura la hace `resumen`.
#
#   ./scripts/perf-escenarios.sh listar
#   ./scripts/perf-escenarios.sh antes 3-subir-espacio 40
#   ./scripts/perf-escenarios.sh resumen antes
#
# ANTES DE MEDIR, en el teléfono: MENU -> SONDA (enciende ha-stages/ha-sys). Sin
# eso sólo salen ha-perf/ha-hitch, que dicen cuánto duele pero no quién.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PKG="com.hungerarena.app"
OUT_ROOT="$ROOT/perf-runs"
PATRON="ha-stages|ha-sys|ha-hitch|ha-perf|ha-span|loading:|ha-crash"

# Los seis escenarios del plan. El 3 y el 4 son el reporte del usuario: al salir
# al espacio hay un tirón en cada cambio de escalón de LOD, y al reingresar los
# tiles finos vuelven mal.
escenarios() {
  cat <<'EOF'
1-base            Quieto en el pasto, sin tocar nada
2-caminar         Caminar en linea recta sin parar
3-subir-espacio   Despegar y SUBIR DESPACIO hasta ver el planeta entero
4-reingreso       Bajar desde orbita y volver a pisar el suelo
5-clima           Lluvia/tormenta y noche (HA_WEATHER clava el clima en PC)
6-ui              Abrir y cerrar MENU / PERF / inventario varias veces
EOF
}

cmd_listar() {
  echo "== Escenarios =="
  escenarios | while read -r id desc; do printf '  %-18s %s\n' "$id" "$desc"; done
  echo
  echo "Uso: $0 <etiqueta> <escenario> [segundos]"
  echo "     etiqueta = 'antes' / 'despues' / lo que quieras comparar"
}

cmd_capturar() {
  local etiqueta="$1" escenario="$2" secs="${3:-40}"
  local desc
  desc="$(escenarios | rg "^$escenario " | sed "s/^$escenario *//" || true)"
  if [ -z "$desc" ]; then
    echo "Escenario desconocido: $escenario"
    cmd_listar
    exit 1
  fi

  if ! adb devices | awk 'NR>1 && $2=="device" {f=1} END{exit !f}'; then
    echo "✗ No hay dispositivo. Activá Depuración USB en el celu."
    adb devices -l
    exit 1
  fi

  local dir="$OUT_ROOT/$etiqueta"
  mkdir -p "$dir"
  local log="$dir/$escenario.log"

  echo "== [$etiqueta] $escenario — ${secs}s =="
  echo "   $desc"
  echo
  echo "   Poné el juego en esa situación y mantenela. Empieza en 5s..."
  sleep 5
  adb logcat -c
  echo "   GRABANDO ${secs}s — dale al escenario AHORA."

  timeout "$secs" adb logcat -v time 2>/dev/null | rg --line-buffered "$PATRON" >"$log" || true

  echo "   → $log ($(wc -l <"$log") líneas)"
  resumir_log "$log"
}

# Lo que importa de un log: cuántos frames pasaron el umbral de tirón, cuál fue
# el peor, y el top de sistemas. El promedio solo miente — un tirón de 300 ms se
# le desaparece adentro (§10).
resumir_log() {
  local log="$1"
  echo
  echo "   --- resumen ---"
  local hitches
  hitches="$(rg -c "ha-hitch" "$log" 2>/dev/null || echo 0)"
  echo "   tirones (ha-hitch): $hitches"
  rg "ha-hitch" "$log" 2>/dev/null | rg -o "frame=[0-9.]+" | sort -t= -k2 -rn | head -3 |
    sed 's/^/   peor: /' || true
  echo "   ultimo ha-stages:"
  rg "ha-stages" "$log" 2>/dev/null | tail -2 | sed 's/^/     /' || echo "     (ninguno — ¿encendiste MENU -> SONDA?)"
  echo "   ultimo ha-sys:"
  rg "ha-sys" "$log" 2>/dev/null | tail -2 | sed 's/^/     /' || echo "     (ninguno — ¿encendiste MENU -> SONDA?)"
  echo
}

cmd_resumen() {
  local etiqueta="$1"
  local dir="$OUT_ROOT/$etiqueta"
  [ -d "$dir" ] || { echo "No hay corrida '$etiqueta' en $OUT_ROOT"; exit 1; }
  for log in "$dir"/*.log; do
    echo "=== $(basename "$log" .log)"
    resumir_log "$log"
  done
}

case "${1:-listar}" in
  listar) cmd_listar ;;
  resumen) shift; cmd_resumen "${1:?falta la etiqueta}" ;;
  *) cmd_capturar "$1" "${2:?falta el escenario — probá: $0 listar}" "${3:-40}" ;;
esac
