#!/usr/bin/env bash
# Package Linux release-native binary as hunger-arena.AppImage
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$ROOT/target/release-native/ha-game"
OUT_DIR="$ROOT/target/android-artifacts"
APPIMAGE="$OUT_DIR/hunger-arena.AppImage"
APPDIR="$OUT_DIR/HungerArena.AppDir"
TOOL="${APPIMAGETOOL:-/tmp/appimagetool-x86_64.AppImage}"

if [ ! -f "$BIN" ]; then
  echo "Binary missing — run: make -C native release"
  exit 1
fi

if [ ! -x "$TOOL" ]; then
  echo "appimagetool not found at $TOOL"
  echo "Download: curl -fsSL -o /tmp/appimagetool-x86_64.AppImage \\"
  echo "  https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-x86_64.AppImage"
  exit 1
fi

rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$OUT_DIR"

cp "$BIN" "$APPDIR/usr/bin/hunger-arena"
chmod +x "$APPDIR/usr/bin/hunger-arena"

cat > "$APPDIR/hunger-arena.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=Hunger Arena
Comment=Survival multiplayer — native Bevy client
Exec=hunger-arena
Icon=hunger-arena
Categories=Game;
Terminal=false
EOF

mkdir -p "$APPDIR/usr/share/applications"
cp "$APPDIR/hunger-arena.desktop" "$APPDIR/usr/share/applications/"

# Minimal 256×256 PNG (solid magenta — replace with brand icon later)
python3 - <<'PY' "$APPDIR/hunger-arena.png"
import struct, sys, zlib
path = sys.argv[1]
w = h = 256
rows = []
for y in range(h):
    row = b"\x00"
    for x in range(w):
        row += bytes((255, 0, 120, 255))  # BD pink
    rows.append(row)
raw = b"".join(rows)
comp = zlib.compress(raw, 9)
def chunk(tag, data):
    crc = zlib.crc32(tag + data) & 0xFFFFFFFF
    return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", crc)
png = b"\x89PNG\r\n\x1a\n"
png += chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
png += chunk(b"IDAT", comp)
png += chunk(b"IEND", b"")
open(path, "wb").write(png)
PY

ln -sf hunger-arena.png "$APPDIR/AppIcon.png"

cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
export PATH="${HERE}/usr/bin:${PATH}"
exec "${HERE}/usr/bin/hunger-arena" "$@"
EOF
chmod +x "$APPDIR/AppRun"

ARCH=x86_64 VERSION="$(node -e "console.log(JSON.parse(require('fs').readFileSync('$ROOT/version.json','utf8')).version)")" \
  "$TOOL" "$APPDIR" "$APPIMAGE"

echo "✓ $APPIMAGE ($(du -h "$APPIMAGE" | cut -f1))"
