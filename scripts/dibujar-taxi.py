#!/usr/bin/env python3
"""**El taxi, derivado de un auto del pack** (skill `pixel-art` §0: derivar del arte en uso antes que dibujar).

    python3 scripts/dibujar-taxi.py [--dry-run]

El taxi dibujado a mano el 2 oct (1 celda, `art/gen/mundo2.png`) quedó POR DEBAJO en el juez (3 oct): un cuarto
del tamaño de un auto del pack y un salto de luz entre tonos de la mitad (11,8 contra 24,9). El pack no trae
taxi, pero sí un auto amarillo visto desde arriba (`vehicle/auto_abajo_14`, la variante de carrocería más
amarilla, que se ELIGE midiendo, no a ojo). Se le agrega el cartel del techo —una caja con tapa clara y lado en
sombra, colores de la paleta del pack— y se publica en `art/gen/taxi.png` como `vehicle/taxi` (2x4 celdas,
la escala de los autos). Se juzga contra las otras variantes del mismo auto antes de escribir nada.
"""
import colorsys
import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib.juez import informe, juzgar  # noqa: E402
from lib.pixel import ajustar, rampa  # noqa: E402

RAIZ = Path(__file__).resolve().parent.parent
ASSETS = RAIZ / "native/crates/ha-game/assets"
MANIFEST = ASSETS / "art/manifest.json"
HOJA = "art/gen/taxi.png"
FAMILIA = "vehicle/auto_abajo_14"


def recorte(e):
    g = e.get("grid", [32, 32])
    r = e["rect"]
    return Image.open(ASSETS / e["png"]).convert("RGBA").crop((r["col"] * g[0], r["row"] * g[1], (r["col"] + r["w"]) * g[0], (r["row"] + r["h"]) * g[1]))


def amarillez(im) -> float:
    a = np.asarray(im).astype(np.float32) / 255
    op = a[..., 3] > 0.8
    pts = 0.0
    for r, g, b in a[..., :3][op]:
        h, s, v = colorsys.rgb_to_hsv(r, g, b)
        if 0.10 <= h <= 0.18 and s > 0.45 and v > 0.6:
            pts += 1
    return pts / max(op.sum(), 1)


def cartel(a: np.ndarray) -> np.ndarray:
    """La caja del cartel, centrada sobre el techo (la franja oscura más ancha de la mitad de arriba)."""
    out = a.copy()
    H, W = a.shape[:2]
    lum = (a[..., :3] * [0.299, 0.587, 0.114]).sum(-1)
    op = a[..., 3] > 200
    ys, xs = np.nonzero(op)
    cx = int(round((xs.min() + xs.max()) / 2))
    # techo: el tramo oscuro CONTINUO más largo de la columna central, en los 2/3 de arriba (luneta + techo +
    # parabrisas son oscuros pero los separan las líneas claras de los marcos; el techo es el tramo del medio)
    banda = (lum[:, cx - 3 : cx + 4] < 90).mean(1) > 0.8
    tope = ys.min() + 2 * (ys.max() - ys.min()) // 3
    mejor, ini, k = 0, ys.min(), None
    for y in range(ys.min(), tope):
        if banda[y]:
            k = y if k is None else k
            if y - k + 1 > mejor:
                mejor, ini = y - k + 1, k
        else:
            k = None
    fila = ini + mejor // 2
    w, h = 14, 6
    x0, y0 = cx - w // 2, fila - h // 2
    amarillo = rampa((240, 196, 70), 4)
    sombra = rampa((70, 70, 94), 3)
    for y in range(h):
        for x in range(w):
            if (x in (0, w - 1)) and (y in (0, h - 1)):
                continue  # esquinas romas, como las del pack
            c = amarillo[2]
            if y == 0:
                c = amarillo[3]  # tapa iluminada (luz de arriba)
            elif y == h - 1 or x == w - 1:
                c = amarillo[0]  # lado en sombra: abajo y derecha
            out[y0 + y, x0 + x] = (*ajustar(c), 255)
    # la leyenda: tres trazos oscuros en la cara del cartel (a 12 px no se lee «TAXI»; el pack tampoco escribe)
    for x in (4, 6, 8, 10):
        out[y0 + 2, x0 + x] = (*ajustar(sombra[0]), 255)
    # brillo puntual del lado de la luz
    out[y0 + 1, x0 + 1] = (*ajustar((250, 244, 220)), 255)
    # sombra de contacto del cartel sobre el techo (fila de abajo, tono del techo un escalón más oscuro)
    for x in range(1, w):
        y = y0 + h
        if op[y, x0 + x]:
            out[y, x0 + x] = (*ajustar(sombra[0]), 255)
    return out


def main():
    seco = "--dry-run" in sys.argv
    man = json.loads(MANIFEST.read_text())
    sp = man["sprites"]
    fam = [k for k in sp if k == FAMILIA or k.startswith(FAMILIA + "#")]
    ims = {k: recorte(sp[k]) for k in fam}
    base = max(fam, key=lambda k: amarillez(ims[k]))
    print(f"auto de partida: {base} (amarillo {amarillez(ims[base]):.0%} de la carrocería)")
    taxi = cartel(np.asarray(ims[base]))
    refs = [np.asarray(ims[k]) for k in fam if k != base][:4]
    r = juzgar(taxi, refs)
    print(informe(r, "vehicle/taxi"))
    if r["veredicto"] == "POR DEBAJO":
        raise SystemExit("el taxi no llega al pack: no se publica")
    if seco:
        return
    (ASSETS / HOJA).parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray(taxi).save(ASSETS / HOJA)
    h, w = taxi.shape[0] // 32, taxi.shape[1] // 32
    sp["vehicle/taxi"] = {"grid": [32, 32], "png": HOJA, "rect": {"col": 0, "row": 0, "w": w, "h": h}}
    MANIFEST.write_text(json.dumps(man, ensure_ascii=False, indent=1))
    print(f"{HOJA}: {w}x{h} celdas → vehicle/taxi")


if __name__ == "__main__":
    main()
