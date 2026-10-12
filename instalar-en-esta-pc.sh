#!/usr/bin/env bash
#
# instalar-en-esta-pc.sh — deja el juego INSTALADO en esta computadora: aparece
# en la bandeja de aplicaciones con su icono, y se abre como cualquier programa.
#
# No pide root: todo va al home del usuario, que es donde el escritorio
# (GNOME/KDE/XFCE) busca las apps del usuario.
#
#   ~/.local/bin/ki-mundo-abierto          el binario
#   ~/.local/share/ki-mundo-abierto/       los assets (el juego los busca al lado)
#   ~/.local/share/applications/*.desktop  la entrada del menu
#   ~/.local/share/icons/hicolor/...       el icono
#
# Uso:
#   ./instalar-en-esta-pc.sh            compila (si hace falta) e instala
#   ./instalar-en-esta-pc.sh --desinstalar
#
set -euo pipefail
cd "$(dirname "$0")"

APP_ID="ki-mundo-abierto"
APP_NAME="Ki - Mundo Abierto"
BIN_DIR="$HOME/.local/bin"
DATA_DIR="$HOME/.local/share/$APP_ID"
DESKTOP="$HOME/.local/share/applications/$APP_ID.desktop"
ICON_DIR="$HOME/.local/share/icons/hicolor/512x512/apps"

if [ "${1:-}" = "--desinstalar" ]; then
  rm -f "$BIN_DIR/$APP_ID" "$DESKTOP" "$ICON_DIR/$APP_ID.png"
  rm -rf "$DATA_DIR"
  update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
  echo "✓ Desinstalado. (Tu partida guardada NO se toca.)"
  exit 0
fi

# ── 1. Compilar ────────────────────────────────────────────────────────────
# `release-native` es el perfil optimizado de escritorio (ver native/Makefile).
BIN="native/target/release-native/ha-game"
if [ ! -x "$BIN" ] || [ -n "$(find native/crates -newer "$BIN" -name '*.rs' -print -quit 2>/dev/null)" ]; then
  echo "▶ Compilando el juego (perfil release-native)..."
  (cd native && cargo build --profile release-native -p ha-game)
fi

# ── 2. Copiar binario + assets ─────────────────────────────────────────────
# El juego busca `assets/` RELATIVO a donde corre, así que los assets viajan
# junto al binario y el lanzador entra a esa carpeta antes de ejecutarlo.
mkdir -p "$BIN_DIR" "$DATA_DIR" "$ICON_DIR" "$(dirname "$DESKTOP")"
install -m755 "$BIN" "$DATA_DIR/ha-game"
rm -rf "$DATA_DIR/assets"
cp -r native/crates/ha-game/assets "$DATA_DIR/assets"

# Lanzador: entra a la carpeta de datos y ejecuta. Sin esto el juego arranca
# sin arte (busca `assets/` en el directorio actual, que sería el home).
cat > "$BIN_DIR/$APP_ID" <<EOF
#!/usr/bin/env bash
cd "$DATA_DIR"
exec ./ha-game "\$@"
EOF
chmod +x "$BIN_DIR/$APP_ID"

# ── 3. Icono + entrada del menu ────────────────────────────────────────────
ICON_SRC="native/branding/app-icon-poster.png"
[ -f "$ICON_SRC" ] || ICON_SRC="design/brand/hunger-arena-app-icon.png"
[ -f "$ICON_SRC" ] && cp "$ICON_SRC" "$ICON_DIR/$APP_ID.png"

cat > "$DESKTOP" <<EOF
[Desktop Entry]
Type=Application
Name=$APP_NAME
Comment=Sandbox de role-play 2D con vuelo Ki
Exec=$BIN_DIR/$APP_ID
Icon=$APP_ID
Terminal=false
Categories=Game;ActionGame;
StartupWMClass=ha-game
EOF
chmod +x "$DESKTOP"

update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" 2>/dev/null || true

echo
echo "══ Instalado ══"
echo "  Buscalo como \"$APP_NAME\" en la bandeja de aplicaciones."
echo "  Tambien se abre desde la terminal con:  $APP_ID"
echo
if ! echo "$PATH" | tr ':' '\n' | grep -qx "$BIN_DIR"; then
  echo "  (Ojo: $BIN_DIR no esta en tu PATH — desde el menu anda igual.)"
fi
