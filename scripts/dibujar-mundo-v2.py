#!/usr/bin/env python3
"""**OBJETOS DEL MUNDO CON VOLUMEN** (reemplazan los de `dibujar-mundo.py`).

    python3 scripts/dibujar-mundo-v2.py [--dry-run]

Tronco, cactus, rocas lunar y marciana, juncos, caña, pozo, cofre, estantes y taxi, rehechos con
`lib/mundo_v2.py` (cúpula iluminada, textura de la roca del pack, sombra de contacto). Pisan esas claves del
manifest: correr DESPUÉS de `dibujar-mundo.py`.
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib import mundo_v2 as m  # noqa: E402
from lib.pixel import CELDA, Pagina, publicar  # noqa: E402

HOJA_REL = "art/gen/mundo2.png"


def main():
    seco = "--dry-run" in sys.argv
    lista = [
        ("nature/tronco", "Tronco", m.tronco()),
        ("nature/cactus", "Cactus", m.cactus()),
        ("decor/rock_moon", "Roca lunar", m.roca((168, 168, 176), 4)),
        ("decor/rock_mars", "Roca marciana", m.roca((176, 108, 78), 6)),
        ("agua/junco", "Juncos", m.junco()),
        ("fishing/caña", "Caña de pescar", m.cana()),
        ("prop/pozo", "Pozo", m.pozo()),
        ("props/chest", "Cofre", m.cofre()),
        ("furniture/biblioteca", "Biblioteca", m.estante(0)),
        ("furniture/estante", "Estante", m.estante(3)),
        ("vehicle/taxi", "Taxi", m.taxi()),
    ]
    pag = Pagina()
    sprites, objetos = {}, {}
    for clave, etiqueta, l in lista:
        c, r = pag.poner(l.im)
        sprites[clave] = {"grid": [CELDA, CELDA], "png": HOJA_REL, "rect": {"col": c, "row": r, "w": 1, "h": 1}}
        objetos[clave] = etiqueta
    publicar(pag, sprites, objetos, HOJA_REL, seco)


if __name__ == "__main__":
    main()
