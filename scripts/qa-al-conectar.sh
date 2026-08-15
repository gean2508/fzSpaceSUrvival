#!/usr/bin/env bash
# Espera a que aparezca el celu y, apenas lo detecta, corre TODO el ciclo:
# compila el APK, lo instala y pasa la suite de drivers.
#
# Existe porque la regla 11 pide probar en el Redmi antes de publicar, pero el celu no
# siempre está enchufado: en vez de fallar con "no hay device", esto queda esperando y
# arranca solo. El pedido fue textual — "que esté detectando apenas lo conecte y corrés
# todo".
#
#   ./scripts/qa-al-conectar.sh            # espera para siempre
#   ESPERA_MAX=600 ./scripts/qa-al-conectar.sh
set -uo pipefail

cd "$(dirname "$0")/.."
ESPERA_MAX="${ESPERA_MAX:-0}"        # 0 = sin límite
QA_OUT="${QA_OUT:-/tmp/ha-qa-alconectar}"
PKG=com.hungerarena.app
APK=client/public/downloads/hunger-arena.apk

hay_celu() { adb devices | awk 'NR>1 && $2=="device" {f=1} END{exit !f}'; }

echo "⏳ esperando el celu (adb)…"
i=0
while ! hay_celu; do
  sleep 5
  i=$((i + 5))
  if [ "$ESPERA_MAX" -gt 0 ] && [ "$i" -ge "$ESPERA_MAX" ]; then
    echo "✗ timeout: no apareció el celu en ${ESPERA_MAX}s"
    exit 2
  fi
done
echo "✓ celu detectado: $(adb shell getprop ro.product.model | tr -d '\r')"

# El disco lleno se manifiesta como un "Bus error" del linker, que se lee como un bug del
# código y no lo es. Se chequea ANTES de compilar (regla propia, aprendida a la mala).
libre_gb=$(df -BG --output=avail / | tail -1 | tr -dc '0-9')
echo "· disco libre: ${libre_gb}G"
if [ "$libre_gb" -lt 12 ]; then
  echo "✗ menos de 12G libres — limpiá caché (native/target/debug) antes de compilar"
  exit 3
fi

# APK RECIÉN COMPILADO, SIN PUBLICAR.
#
# `ship-native-android.mjs` compila y ACTO SEGUIDO publica, sin flag para saltear el
# publicado. Acá no queremos publicar: la regla 11 dice que se prueba en el celu ANTES, y
# publicar es el plan B. Con `--no-bump` el publicado además falla siempre ("versionCode
# debe subir"), o sea que su salida no distingue "no compiló" de "compiló y no publicó".
#
# Por eso no se mira el código de salida sino el CONTENIDO del APK: si la `libha_game.so`
# empaquetada es byte a byte la que acaba de compilar `cargo ndk`, el APK está al día y lo
# único que falló fue el publicado, que no nos importa.
#
# Se compara el contenido y no las fechas a propósito. Las fechas mienten en los dos
# sentidos acá: gradle no re-empaqueta cuando nada cambió (y deja un APK viejo con mtime
# nuevo), y el `libc++_shared.so` se copia DESPUÉS de empaquetar (y deja un .so más nuevo
# que el APK que sí lo contiene). Las dos heurísticas de fecha —la de `ship` y una previa
# de este script— daban "stale" sobre un APK perfectamente al día.
APK_BUILD=native/android/app/build/outputs/apk/release/app-release.apk
LIB=native/android/app/src/main/jniLibs/arm64-v8a/libha_game.so

echo "▶ compilando el APK (sin publicar)…"
node scripts/ship-native-android.mjs --no-bump --skip-tests || true

if [ ! -f "$APK_BUILD" ] || [ ! -f "$LIB" ]; then
  echo "✗ no hay APK ni lib — el build falló de verdad"
  exit 1
fi
md5_lib=$(md5sum "$LIB" | cut -d' ' -f1)
md5_apk=$(unzip -p "$APK_BUILD" lib/arm64-v8a/libha_game.so 2>/dev/null | md5sum | cut -d' ' -f1)
if [ "$md5_lib" != "$md5_apk" ]; then
  echo "✗ el APK no contiene la lib recién compilada ($md5_apk ≠ $md5_lib)"
  exit 1
fi
echo "✓ APK al día ($(du -h "$APK_BUILD" | cut -f1), lib $md5_lib)"

echo "▶ instalando…"
if ! adb install -r "$APK_BUILD"; then
  echo "✗ falló la instalación"
  exit 1
fi

echo "▶ suite de drivers (QA_OUT=$QA_OUT)…"
QA_OUT="$QA_OUT" ./scripts/qa-android.sh
estado=$?
echo "· suite terminada con estado $estado — evidencia en $QA_OUT"
exit $estado
