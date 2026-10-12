#!/usr/bin/env python3
"""Empaqueta el arte de MUNDO en una sola hoja para que los sprites batcheen.

## Por qué

Medido en el Redmi con el cable (2026-07-31): a pie el frame se clavaba en 74 ms con
un jitter de 0,2 ms y sólo 1 ms de CPU en los spans. Coste perfectamente PAREJO = no
es un tirón, son draw calls. La causa está en el manifest: 118 slots repartidos en 93
PNG distintos, y los 8 árboles son 8 imágenes separadas. Con Y-sort las texturas se
alternan en el orden de dibujo, así que el batcher de Bevy rompe el lote casi en cada
sprite — un bosque son miles de draw calls.

Los atlas "completos" que traen los packs NO sirven para esto:
`Modern_Exteriors_Complete_Tileset_32x32.png` mide 5632x16448 y `Interiors_32x32.png`
512x34048; las dos pasan del límite de textura de una GPU (16384, y 8192 en muchos
Adreno). Por eso nunca se cargaron. Pero los PNG que el juego ya usa son recortes de
esos packs, todos múltiplos de 32, y juntos caben en 1024x1024 de sobra.

## Qué hace

Toma los slots de MUNDO (los que se dibujan por miles: decoración, flora, props,
tiles), los pega en una rejilla de celdas de 32 px y reescribe el manifest para que
apunten a la hoja con su `rect` en celdas. **No cambia ni un píxel de arte**: es el
mismo dibujo, en el mismo orden, con una textura en vez de noventa.

El formato ya lo soportaba (`grid` + `rect`/`strip`/`poses` en CELDAS, ver
`ha-assets/src/art_manifest.rs`) y 23 slots ya lo usaban — esto sólo lo generaliza.

## Uso

    python3 scripts/pack-world-atlas.py            # empaqueta y reescribe el manifest
    python3 scripts/pack-world-atlas.py --dry-run  # sólo informa, no toca nada

Es idempotente: los slots que ya apuntan a la hoja se saltan.
"""

import json
import shutil
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "native/crates/ha-game/assets"
MANIFEST = ASSETS / "art/manifest.json"
ATLAS_REL = "art/atlas/world_atlas.png"
CELL = 32
# Ancho de la hoja en celdas. 32 celdas = 1024 px: minúsculo para cualquier GPU y de
# sobra para lo que entra. Si algún día no cupiera, subir a 64 (2048 px) antes que
# partir en dos hojas — dos hojas vuelven a romper el lote.
SHEET_COLS = 32

# Prefijos que se dibujan POR MILES en superficie. Son los que importan; la UI, los
# NPCs y los interiores salen unas pocas veces por pantalla y no mueven la aguja.
WORLD_PREFIXES = ("decor/", "nature/", "prop/", "tiles/")


def cells(px: int) -> int:
    return (px + CELL - 1) // CELL


def shift_rect(rect: dict, dc: int, dr: int) -> dict:
    """Desplaza un rect en CELDAS al sitio que ocupa la imagen dentro de la hoja."""
    out = dict(rect)
    out["col"] = rect.get("col", 0) + dc
    out["row"] = rect.get("row", 0) + dr
    return out


def main() -> int:
    dry = "--dry-run" in sys.argv
    manifest = json.loads(MANIFEST.read_text())
    sprites = manifest["sprites"]

    # 1) Elegir candidatos: slots de mundo, con PNG propio y medidas múltiplo de celda.
    jobs = []
    skipped = []
    for key, val in sorted(sprites.items()):
        if not isinstance(val, dict) or "png" not in val:
            continue
        if not key.startswith(WORLD_PREFIXES):
            continue
        if val["png"] == ATLAS_REL:
            continue  # ya empaquetado
        src = ASSETS / val["png"]
        if not src.exists():
            skipped.append((key, "falta el png"))
            continue
        with Image.open(src) as im:
            w, h = im.size
        if w % CELL or h % CELL:
            # No se redondea a celda: abarcaría píxeles vacíos y el sprite saldría
            # con margen. Ese slot se queda con su PNG suelto, que es correcto.
            skipped.append((key, f"{w}x{h} no es múltiplo de {CELL}"))
            continue
        jobs.append((key, src, cells(w), cells(h)))

    if not jobs:
        print("nada que empaquetar (¿ya está hecho?)")
        return 0

    # 2) Empaquetado por estanterías: filas de altura fija, la más alta manda. Con
    #    ~40 imágenes y celdas de 32 no hace falta nada más listo que esto.
    jobs.sort(key=lambda j: (-j[3], -j[2]))
    placed = {}
    cur_col = cur_row = row_h = 0
    for key, src, cw, ch in jobs:
        if cur_col + cw > SHEET_COLS:
            cur_row += row_h
            cur_col = row_h = 0
        placed[key] = (cur_col, cur_row, cw, ch, src)
        cur_col += cw
        row_h = max(row_h, ch)
    total_rows = cur_row + row_h
    print(f"{len(placed)} slots -> hoja de {SHEET_COLS * CELL}x{total_rows * CELL} px")
    for key, why in skipped:
        print(f"  (fuera) {key}: {why}")
    if dry:
        return 0

    # 3) Pintar la hoja.
    sheet = Image.new("RGBA", (SHEET_COLS * CELL, total_rows * CELL), (0, 0, 0, 0))
    for key, (c, r, _cw, _ch, src) in placed.items():
        with Image.open(src) as im:
            sheet.paste(im.convert("RGBA"), (c * CELL, r * CELL))
    out = ASSETS / ATLAS_REL
    out.parent.mkdir(parents=True, exist_ok=True)
    sheet.save(out)
    print(f"escrita {out.relative_to(ROOT)}")

    # 4) Reescribir el manifest. Todo lo que ya venía en celdas (rect, strip, poses)
    #    sólo necesita desplazarse; el arte no se toca.
    backup = MANIFEST.with_suffix(".json.bak")
    if not backup.exists():
        shutil.copy(MANIFEST, backup)
    for key, (c, r, cw, ch, _src) in placed.items():
        val = sprites[key]
        val["png"] = ATLAS_REL
        val["grid"] = [CELL, CELL]
        for field in ("rect", "strip"):
            if field in val:
                val[field] = shift_rect(val[field], c, r)
        if "poses" in val:
            for pose in val["poses"].values():
                if isinstance(pose, dict) and "rect" in pose:
                    pose["rect"] = shift_rect(pose["rect"], c, r)
        if "rect" not in val and "strip" not in val:
            # Sprite entero: su rect es su hueco en la hoja.
            val["rect"] = {"col": c, "row": r, "w": cw, "h": ch}
    MANIFEST.write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"manifest reescrito ({backup.name} guardado como respaldo)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
