#!/usr/bin/env python3
"""Mide DÓNDE están las ventanas de cada fachada del pack, para poder prenderlas de noche.

Las fachadas son una imagen única: no se puede prender una ventana suelta. Este script detecta el
VIDRIO (el celeste claro con rayitas de LimeZu) en cada edificio del manifest y publica los
rectángulos, en píxeles del dibujo, en `native/crates/ha-game/assets/art/ventanas.json`.

Nada a ojo (regla 7): la geometría sale de los píxeles. Idempotente; se corre después de cualquier
importador de edificios.

    python3 scripts/medir-ventanas.py            # todos los edificios
    python3 scripts/medir-ventanas.py building/hospital   # uno solo, con diagnóstico
"""
import json, sys
import numpy as np
from PIL import Image
from scipy import ndimage

BASE = "native/crates/ha-game/assets/"
SALIDA = BASE + "art/ventanas.json"
PREFIJOS = ("building/", "edificio/", "completo/")
MAX_VENTANAS = 48


def mascara_vidrio(a):
    r, g, b, al = (a[..., i].astype(int) for i in range(4))
    # Vidrio de LimeZu: celeste claro, azul dominante sobre rojo, claro. La pared es gris/blanca (azul ~ rojo).
    return (al > 200) & (r >= 120) & (b >= 180) & (b - r >= 22) & (g - r >= 8) & (b <= 252)


def medir(im):
    a = np.array(im.convert("RGBA"))
    m = mascara_vidrio(a)
    # Unir las rayitas del vidrio (separadas por 1 px) en un solo bloque.
    m = ndimage.binary_closing(m, structure=np.ones((3, 3)), iterations=1)
    lab, n = ndimage.label(m)
    out = []
    for i, sl in enumerate(ndimage.find_objects(lab), start=1):
        if sl is None:
            continue
        ys, xs = sl
        h, w = ys.stop - ys.start, xs.stop - xs.start
        if w < 5 or h < 4 or w > 46 or h > 46:
            continue
        relleno = (lab[sl] == i).sum() / float(w * h)
        if relleno < 0.45:
            continue
        out.append([int(xs.start), int(ys.start), int(w), int(h)])
    out.sort(key=lambda r: (r[1], r[0]))
    return out[:MAX_VENTANAS]


def main():
    m = json.load(open(BASE + "art/manifest.json"))["sprites"]
    solo = sys.argv[1:]
    res = {}
    cache = {}
    for k, e in m.items():
        if not k.startswith(PREFIJOS) or (solo and k not in solo):
            continue
        r = e.get("rect") or (e.get("poses", {}).get("center", {}).get("rect"))
        png = e.get("png")
        if not r or not png or r["w"] * r["h"] < 12:
            continue
        if png not in cache:
            cache[png] = Image.open(BASE + png).convert("RGBA")
        im = cache[png]
        tile = im.crop((r["col"] * 32, r["row"] * 32, (r["col"] + r["w"]) * 32, (r["row"] + r["h"]) * 32))
        v = medir(tile)
        if v:
            res[k] = {"tam": [tile.size[0], tile.size[1]], "ventanas": v}
            if solo:
                print(k, tile.size, len(v), v[:8])
    if not solo:
        json.dump(res, open(SALIDA, "w"), indent=0, sort_keys=True)
        print(f"{len(res)} edificios con ventanas medidas -> {SALIDA}")


main()
