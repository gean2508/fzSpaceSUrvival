#!/usr/bin/env python3
"""**LOS ÁRBOLES SIN SU MATA DE PASTO AL PIE.**

Pedido del usuario, 28 sep 2026: *"en todos los assets de árboles elimina la
parte redonda de la base, que sólo quede el tronco, no ese trozo de césped
debajo del árbol que se ve horrible"*.

Recorre cada recorte `nature/arbol*` del manifest y le borra la elipse de pasto
en su página de `art/pack/` (la medición y la regla viven en
`lib/pasto_al_pie.py`). Idempotente: correrlo dos veces no cambia nada. Lo
llama `import-pack-masivo.py` al terminar, porque ese reescribe las páginas
desde los PNG crudos del pack y la elipse volvería.

```sh
python3 scripts/podar-pasto-de-arboles.py --seco   # informa
python3 scripts/podar-pasto-de-arboles.py          # escribe las páginas
```
"""

import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib.pasto_al_pie import podar  # noqa: E402

RAIZ = Path(__file__).resolve().parent.parent
ASSETS = RAIZ / "native/crates/ha-game/assets"
MANIFEST = ASSETS / "art/manifest.json"


def es_arbol(clave: str) -> bool:
    # Fuera: las PIEZAS del muro de bosque (`arbol_pared_modular_*`, regla 8 —
    # su pasto empalma con la pieza de al lado) y las ramitas sueltas del suelo
    # (`arbol_palo_seco_*`), que no tienen elipse sino hojas.
    if "_modular" in clave or "palo_seco" in clave:
        return False
    return clave.startswith(("nature/arbol", "nature/palmera", "nature/villas_arbol", "decor/tree"))


def main():
    seco = "--seco" in sys.argv
    m = json.loads(MANIFEST.read_text())
    recortes = {}
    for clave, e in m["sprites"].items():
        r = e.get("rect")
        if not es_arbol(clave) or not e.get("png") or e.get("anim") or e.get("strip"):
            continue
        if not r:
            # Un PNG propio (`decor/tree_*`, las especies del bosque): el
            # recorte es la imagen entera.
            w, h = Image.open(ASSETS / e["png"]).size
            recortes.setdefault(e["png"], set()).add((0, 0, w, h))
            continue
        g = e.get("grid") or [32, 32]
        recortes.setdefault(e["png"], set()).add(
            (r["col"] * g[0], r["row"] * g[1], (r["col"] + r["w"]) * g[0], (r["row"] + r["h"]) * g[1])
        )
    # Un recorte de árbol no puede compartir píxeles con nada que no sea árbol:
    # si lo hiciera, borrarle el pasto le comería el borde al vecino.
    for clave, e in m["sprites"].items():
        r = e.get("rect")
        if es_arbol(clave) or not r or e.get("png") not in recortes:
            continue
        g = e.get("grid") or [32, 32]
        x0, y0 = r["col"] * g[0], r["row"] * g[1]
        x1, y1 = x0 + r["w"] * g[0], y0 + r["h"] * g[1]
        for (a0, b0, a1, b1) in recortes[e["png"]]:
            if x0 < a1 and a0 < x1 and y0 < b1 and b0 < y1:
                sys.exit(f"{clave} se pisa con un recorte de árbol en {e['png']}: no se poda")
    total = arboles = 0
    for png, rects in sorted(recortes.items()):
        ruta = ASSETS / png
        a = np.array(Image.open(ruta).convert("RGBA"))
        n_pag = 0
        for (x0, y0, x1, y1) in sorted(rects):
            vista = a[y0:y1, x0:x1]
            n = podar(vista)
            if n:
                arboles += 1
                n_pag += n
        total += n_pag
        if n_pag and not seco:
            Image.fromarray(a).save(ruta, optimize=True)
        print(f"{png}: {n_pag} px de pasto")
    print(f"{arboles} árboles podados, {total} px{' (seco: no se escribió nada)' if seco else ''}")


if __name__ == "__main__":
    main()
