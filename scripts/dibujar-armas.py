#!/usr/bin/env python3
"""**LAS ARMAS: 9 armas × 7 aspectos (base + 6 camuflajes) → `art/gen/armas.png` + claves `items/gun_<arma>[__<camo>]`.**

    python3 scripts/dibujar-armas.py [--vista]     # --vista: sólo deja la lámina de control en /tmp/claude-1000/armas.png

Cada ícono ocupa 2×2 celdas (64×64) con el arma de perfil, mirando a la derecha. El mismo sprite es el ícono de la mochila y
el arma que sostiene el personaje.
"""
import json
import sys
from pathlib import Path

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib.armas import CAMOS, dibujar  # noqa: E402
from lib.armas_diseno import ARMAS, MUNICION  # noqa: E402
from lib.pixel import CELDA, Pagina, publicar  # noqa: E402

HOJA_REL = "art/gen/armas.png"
RAIZ = Path(__file__).resolve().parent.parent


def municiones():
    for iid, (nombre, piezas) in MUNICION.items():
        yield iid, nombre, dibujar(piezas, None)


def todas():
    for aid, (nombre, _cal, piezas) in ARMAS.items():
        yield aid, None, nombre, dibujar(piezas, None)
        for cid, (cnom, _f) in CAMOS.items():
            yield aid, cid, f"{nombre} · {cnom}", dibujar(piezas, cid)


def main():
    lista = list(todas())
    Path("/tmp/claude-1000").mkdir(exist_ok=True)
    cols = 7
    lam = Image.new("RGBA", (cols * 128, ((len(lista) + cols - 1) // cols) * 128), (60, 78, 70, 255))
    for i, (_a, _c, _n, im) in enumerate(lista):
        lam.alpha_composite(im.resize((128, 128), Image.NEAREST), ((i % cols) * 128, (i // cols) * 128))
    mun = list(municiones())
    lam2 = Image.new("RGBA", (len(mun) * 128, 128), (60, 78, 70, 255))
    for i, (_i, _n, im) in enumerate(mun):
        lam2.alpha_composite(im.resize((128, 128), Image.NEAREST), (i * 128, 0))
    lam2.convert("RGB").save("/tmp/claude-1000/municion.png")
    lam.convert("RGB").save("/tmp/claude-1000/armas.png")
    if "--vista" in sys.argv:
        return
    pag = Pagina()
    sprites, objetos = {}, {}
    for iid, nombre, im in municiones():
        c, r = pag.poner(im)
        sprites[f"items/{iid}"] = {"grid": [CELDA, CELDA], "png": HOJA_REL, "rect": {"col": c, "row": r, "w": 2, "h": 2}}
        objetos[f"items/{iid}"] = nombre
    for aid, cid, nombre, im in lista:
        c, r = pag.poner(im)
        clave = f"items/gun_{aid}" + (f"__{cid}" if cid else "")
        sprites[clave] = {"grid": [CELDA, CELDA], "png": HOJA_REL, "rect": {"col": c, "row": r, "w": 2, "h": 2}}
        objetos[clave] = nombre
    publicar(pag, sprites, objetos, HOJA_REL, False)


if __name__ == "__main__":
    main()
