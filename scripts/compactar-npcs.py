#!/usr/bin/env python3
"""**COMPACTAR LAS HOJAS DE LOS ALDEANOS A SUS FILAS.**

    python3 scripts/compactar-npcs.py [--dry-run]

`art/npc/char_XX.png` sale del hornero (`./editor_assets chargen`) con la hoja COMPLETA del
Character Generator: 1792×1312 (56 columnas × 20 filas de 32×64), 9,4 MB de GPU cada una, ×20.
Los aldeanos sólo usan siete filas (`ha_assets::chargen_index::NPC_ANIMS`: quieto, respirar,
caminar, dormir, sentarse en el piso, juntar, puñetazo) y hasta la columna 48. Este script las
recorta y las apila en ese orden: **1536×448 = 2,7 MB** por hoja (−71 %; ~54 MB en vez de ~188 MB
de VRAM con las 20). `art2d::registrar_poses_de_personaje` ya apunta a esa geometría.

Idempotente: una hoja que ya mide 1536×448 se saltea. **Hay que correrlo cada vez que se
rehornea** (`chargen`), o falla `las_hojas_de_aldeano_estan_compactas`. Las filas se leen de
`chargen_index.rs` (una sola fuente de verdad), no se repiten acá.
"""

import re
import sys
from pathlib import Path

from PIL import Image

RAIZ = Path(__file__).resolve().parent.parent
FUENTE = (RAIZ / "native/crates/ha-assets/src/chargen_index.rs").read_text(encoding="utf8")
CELDA = (32, 64)
COLS_HOJA = 56


def leer_filas():
    anims = {m.group(1): int(m.group(2)) for m in re.finditer(r'a\("(\w+)",\s*"[^"]*",\s*(\d+),', FUENTE)}
    orden = re.search(r"NPC_ANIMS:\s*&\[&str\]\s*=\s*&\[([^\]]*)\]", FUENTE).group(1)
    claves = re.findall(r'"(\w+)"', orden)
    ncols = int(re.search(r"NPC_COLS:\s*u32\s*=\s*(\d+)", FUENTE).group(1))
    return [(k, anims[k]) for k in claves], ncols


def main():
    seco = "--dry-run" in sys.argv
    filas, ncols = leer_filas()
    ancho, alto = ncols * CELDA[0], len(filas) * CELDA[1]
    carpeta = RAIZ / "native/crates/ha-game/assets/art/npc"
    hechas = saltadas = 0
    for p in sorted(carpeta.glob("char_*.png")):
        im = Image.open(p).convert("RGBA")
        if im.size == (ancho, alto):
            saltadas += 1
            continue
        if im.size[0] != COLS_HOJA * CELDA[0] or im.size[1] < 20 * CELDA[1]:
            raise SystemExit(f"{p.name} mide {im.size}: ni compacta ni completa")
        out = Image.new("RGBA", (ancho, alto), (0, 0, 0, 0))
        for i, (k, fila) in enumerate(filas):
            tira = im.crop((0, fila * CELDA[1], ncols * CELDA[0], (fila + 1) * CELDA[1]))
            out.paste(tira, (0, i * CELDA[1]))
        if not seco:
            out.save(p, optimize=True)
        hechas += 1
    print(f"{hechas} hojas compactadas a {ancho}x{alto} · {saltadas} ya lo estaban"
          f"{' (dry-run)' if seco else ''} · filas: {', '.join(k for k, _ in filas)}")


if __name__ == "__main__":
    main()
