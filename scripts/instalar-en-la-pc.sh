#!/usr/bin/env bash
#
# instalar-en-la-pc.sh — deja **Ki · Mundo Abierto** instalado en esta PC:
# aparece en la bandeja de aplicaciones, se abre con un clic y **se actualiza
# solo** cada vez que lo abrís, si hay una versión nueva compilada.
#
# Uso:
#   ./scripts/instalar-en-la-pc.sh            # compila si hace falta, instala y deja el acceso
#   ./scripts/instalar-en-la-pc.sh --sin-compilar   # usa el binario que ya esté compilado
#   ./scripts/instalar-en-la-pc.sh --desinstalar    # saca el acceso y lo instalado
#
# Qué NO hace: tocar tus partidas. Viven en ~/.local/share/ki-mundo-abierto/datos
# y sobreviven a cualquier reinstalación (y a `--desinstalar`).
#
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP_ID="ki-mundo-abierto"
NOMBRE="Ki · Mundo Abierto"
DESTINO="$HOME/.local/share/$APP_ID"
BIN_DESTINO="$DESTINO/bin/ha-game"
DATOS="$DESTINO/datos"
LANZADOR="$HOME/.local/bin/$APP_ID"
ESCRITORIO="$HOME/.local/share/applications/$APP_ID.desktop"
ICONO="$HOME/.local/share/icons/hicolor/256x256/apps/$APP_ID.png"
ORIGEN_BIN="$ROOT/native/target/release-native/ha-game"
ORIGEN_ICONO="$ROOT/design/brand/hunger-arena-app-icon.png"

if [ "${1:-}" = "--desinstalar" ]; then
  rm -f "$LANZADOR" "$ESCRITORIO" "$ICONO"
  rm -rf "$DESTINO/bin"
  update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
  echo "✓ Desinstalado. Tus partidas siguen en $DATOS"
  exit 0
fi

# ── 1. El binario ────────────────────────────────────────────────────────────
if [ "${1:-}" != "--sin-compilar" ]; then
  echo "▶ Compilando el juego para esta PC (perfil release-native)…"
  make -C "$ROOT/native" release
fi
if [ ! -x "$ORIGEN_BIN" ]; then
  echo "✗ No encuentro el binario: $ORIGEN_BIN"
  echo "  Compilalo con:  make -C native release"
  exit 1
fi

mkdir -p "$DESTINO/bin" "$DATOS" "$(dirname "$LANZADOR")" \
         "$(dirname "$ESCRITORIO")" "$(dirname "$ICONO")"

install -m755 "$ORIGEN_BIN" "$BIN_DESTINO"
# La huella de lo instalado. El lanzador la compara con la del binario del repo
# para decidir si hay actualización: comparar tamaño+fecha es instantáneo y no
# hace falta ni servidor ni red.
stat -c '%s %Y' "$ORIGEN_BIN" > "$DESTINO/huella"

if [ -f "$ORIGEN_ICONO" ]; then
  install -m644 "$ORIGEN_ICONO" "$ICONO"
fi

# ── 2. El lanzador: mira si hay versión nueva ANTES de abrir ─────────────────
#
# La "versión nueva" es el binario recién compilado en el repo. Es lo correcto
# para esta máquina: acá es donde se compila el juego, así que en cuanto termina
# un build el juego que abrís YA es ése. No hay que publicar ni descargar nada,
# y no depende de que el servidor esté levantado.
cat > "$LANZADOR" <<EOF
#!/usr/bin/env bash
set -euo pipefail
REPO="$ROOT"
DESTINO="$DESTINO"
BIN="$BIN_DESTINO"
DATOS="$DATOS"
ORIGEN="\$REPO/native/target/release-native/ha-game"

# ¿Hay una compilación más nueva en el repo? Se compara tamaño+fecha, que es
# instantáneo — abrir el juego no puede quedarse esperando un hash de 1 GB.
if [ -x "\$ORIGEN" ]; then
  NUEVA="\$(stat -c '%s %Y' "\$ORIGEN")"
  VIEJA="\$(cat "\$DESTINO/huella" 2>/dev/null || echo '')"
  if [ "\$NUEVA" != "\$VIEJA" ]; then
    notify-send "$NOMBRE" "Actualizando…" 2>/dev/null || echo "▶ Actualizando…"
    install -m755 "\$ORIGEN" "\$BIN"
    echo "\$NUEVA" > "\$DESTINO/huella"
  fi
fi

# **Siempre desde la misma carpeta.** El juego guarda la partida en su directorio
# de trabajo (\`inventory_persist::persist_dir\`), así que abrirlo desde sitios
# distintos repartiría las partidas por todo el disco.
mkdir -p "\$DATOS"
cd "\$DATOS"
exec "\$BIN" "\$@"
EOF
chmod +x "$LANZADOR"

# ── 3. El acceso en la bandeja de aplicaciones ───────────────────────────────
cat > "$ESCRITORIO" <<EOF
[Desktop Entry]
Type=Application
Name=$NOMBRE
GenericName=Sandbox de mundo abierto
Comment=Rol 2D cenital con vuelo Ki, cuevas y construcción
Exec=$LANZADOR
Icon=$APP_ID
Categories=Game;AdventureGame;
Terminal=false
StartupNotify=true
EOF
chmod +x "$ESCRITORIO"

update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" 2>/dev/null || true

echo
echo "✓ Instalado: $NOMBRE"
echo "  · Buscalo por su nombre en la bandeja de aplicaciones."
echo "  · Cada vez que lo abras se actualiza solo si compilaste algo nuevo."
echo "  · Partidas:  $DATOS"
echo "  · Desde la terminal también: $APP_ID"
case ":$PATH:" in
  *":$HOME/.local/bin:"*) ;;
  *) echo "  ⚠ ~/.local/bin no está en tu PATH (el acceso de la bandeja funciona igual)." ;;
esac
