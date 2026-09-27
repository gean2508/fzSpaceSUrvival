#!/usr/bin/env python3
"""**UN SUELO SE SORTEA SÓLO ENTRE BALDOSAS DEL MISMO MATERIAL.**

Reporte del 27 sep 2026, con captura: la vereda de la ciudad salía de parches
crema, gris, azul y beige, y el asfalto con cuadrados marrones sueltos. La causa:
`floor/vereda` tenía 39 "variantes" y `floor/asfalto` 24 que el importador masivo
agrupó por nombre, y la malla del chunk sortea UNA por celda
(`ArtLibrary::suelo_uv_seeded`). Entre ellas había baldosas blancas, rojas,
azules, lilas y piezas con bordes dibujados — otro material, o una PIEZA de un
kit (regla 8), no una variación del mismo suelo.

Este script MIDE cada variante contra la baldosa base (regla 7: nada a ojo):

- **color medio**: distancia RGB ≤ `MAX_DIST_COLOR` a la base;
- **el mismo borde que la base**: el salto de luminancia entre la franja de 2 px
  del borde y el centro no puede diferir del de la base en más de
  `MAX_SALTO_BORDE` (una baldosa de relleno es pareja hasta el borde; una pieza
  de esquina o de cordón no);
- **opaca entera**.

Las que no pasan NO se borran: se mudan a `<objeto>_suelta#<variante>`, fuera
del sorteo del suelo, y siguen disponibles para colocarlas a mano.

```sh
python3 scripts/podar-variantes-de-suelo.py --seco   # informa
python3 scripts/podar-variantes-de-suelo.py          # escribe el manifest
```
"""

import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image

RAIZ = Path(__file__).resolve().parent.parent
ASSETS = RAIZ / "native/crates/ha-game/assets"
MANIFEST = ASSETS / "art/manifest.json"

MAX_DIST_COLOR = 22.0
MAX_SALTO_BORDE = 14.0
# Sólo los suelos que la malla SORTEA por celda en la calle: la vereda de cada
# ciudad (`world2d::VEREDA_BALDOSA`) y el asfalto. Otros materiales (`floor/stone`)
# tienen variantes TEÑIDAS a propósito, que se piden por nombre y no se sortean.
BASES = {"floor/vereda", "floor/vereda_tipo2", "floor/vereda_tipo3", "floor/vereda_tipo4",
         "floor/vereda_tipo5", "floor/vereda_tipo6", "floor/asfalto"}

_paginas: dict = {}


def recorte(e):
    png = e.get("png")
    r = e.get("rect")
    if not png or not r:
        return None
    if png not in _paginas:
        _paginas[png] = np.array(Image.open(ASSETS / png).convert("RGBA")).astype(np.float32)
    g = e.get("grid") or [32, 32]
    x0, y0 = r["col"] * g[0], r["row"] * g[1]
    return _paginas[png][y0:y0 + r["h"] * g[1], x0:x0 + r["w"] * g[0]]


def luz(rgb):
    return 0.2126 * rgb[..., 0] + 0.7152 * rgb[..., 1] + 0.0722 * rgb[..., 2]


def medir(a):
    """(color medio, salto borde↔centro, opaca)."""
    rgb, al = a[..., :3], a[..., 3]
    opaca = bool((al == 255).all())
    media = rgb.reshape(-1, 3).mean(0)
    l = luz(rgb)
    h, w = l.shape
    borde = np.concatenate([l[:2].ravel(), l[-2:].ravel(), l[:, :2].ravel(), l[:, -2:].ravel()])
    centro = l[2:-2, 2:-2]
    salto = abs(borde.mean() - centro.mean()) if centro.size else 0.0
    # un borde dibujado también se ve como una franja mucho más oscura en un lado
    for franja in (l[:2], l[-2:], l[:, :2], l[:, -2:]):
        salto = max(salto, abs(franja.mean() - centro.mean()))
    return media, salto, opaca


def main():
    seco = "--seco" in sys.argv
    man = json.loads(MANIFEST.read_text())
    sprites, objetos = man["sprites"], man["objects"]
    grupos: dict = {}
    for k in sprites:
        if "#" not in k:
            continue
        base = k.split("#")[0]
        if base not in BASES:
            continue
        grupos.setdefault(base, []).append(k)

    mover = {}
    for base, variantes in sorted(grupos.items()):
        eb = sprites.get(base)
        rb = recorte(eb) if eb else None
        if rb is None:
            continue
        cb, sb, ob = medir(rb)
        fuera = []
        for k in variantes:
            rv = recorte(sprites[k])
            if rv is None:
                continue
            c, salto, opaca = medir(rv)
            d = float(np.linalg.norm(c - cb))
            # el borde se compara con el de la BASE: una baldosa con junta dibujada
            # la tiene en todas sus variantes
            if d > MAX_DIST_COLOR or abs(salto - sb) > MAX_SALTO_BORDE or not opaca:
                fuera.append((k, d, salto, opaca))
        if fuera:
            quedan = len(variantes) - len(fuera)
            print(f"{base}: {len(variantes)} variantes → quedan {quedan}, se mudan {len(fuera)}")
            for k, d, salto, opaca in fuera[:6]:
                print(f"    {k}: color {d:.0f}  borde {salto:.0f}  {'opaca' if opaca else 'con alfa'}")
            for k, *_ in fuera:
                mover[k] = base + "_suelta#" + k.split("#", 1)[1]

    print(f"TOTAL: {len(mover)} variantes fuera del sorteo del suelo")
    if seco or not mover:
        return
    for viejo, nuevo in mover.items():
        sprites[nuevo] = sprites.pop(viejo)
        base_v, var = viejo.split("#", 1)
        base_n = nuevo.split("#", 1)[0]
        o = objetos.get(base_v)
        if o and "variants" in o:
            o["variants"] = [v for v in o["variants"] if v.get("id") != var]
        on = objetos.setdefault(base_n, {"label": (objetos.get(base_v, {}).get("label") or base_v) + " (piezas sueltas)"})
        on.setdefault("variants", []).append({"id": var, "label": var})
    MANIFEST.write_text(json.dumps(man, ensure_ascii=False, indent=1))
    print("manifest escrito")


if __name__ == "__main__":
    main()
