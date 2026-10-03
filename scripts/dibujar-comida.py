#!/usr/bin/env python3
"""**COMIDA, MADERA, FRASCOS Y HERRAMIENTAS CON VOLUMEN** (reemplazan los de `dibujar-items.py`).

    python3 scripts/dibujar-comida.py [--dry-run]

Los íconos de la mochila que más se miran —la comida, la madera, los frascos— eran siluetas planas con
tres tonos. Acá salen de `lib/iconos.py` (cúpula iluminada desde arriba-izquierda, contorno de Interiors,
tonos de la paleta del pack). Va a `art/gen/comida.png` y pisa esas claves del manifest: correr DESPUÉS de
`dibujar-items.py`.
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib.iconos import (antorcha, bola_de_nieve, carne, cuero, escudo, frasco, martillo, espada, cofre, hacha, naranja, palos, pan, pico, sopa, tarta, tronco, uvas)  # noqa: E402
from lib.pixel import CELDA, Pagina, publicar  # noqa: E402

HOJA_REL = "art/gen/comida.png"


def main():
    seco = "--dry-run" in sys.argv
    lista = [
        ("orange", "Naranja", naranja()),
        ("grape", "Uvas", uvas()),
        ("bread", "Pan", pan()),
        ("meat", "Carne", carne()),
        ("pie", "Tarta", tarta()),
        ("soup", "Sopa", sopa()),
        ("wood", "Madera", tronco()),
        ("sticks", "Palos", palos()),
        ("snow", "Nieve", bola_de_nieve()),
        ("leather", "Cuero", cuero()),
        ("potion_blue", "Poción azul", frasco((66, 120, 214))),
        ("potion_green", "Poción verde", frasco((84, 184, 96))),
        # herramientas: la cabeza dice el material (madera, piedra, acero)
        ("axe", "Hacha", hacha((176, 182, 190))),
        ("wood_axe", "Hacha de madera", hacha((150, 104, 62))),
        ("stone_axe", "Hacha de piedra", hacha((130, 130, 144))),
        ("pickaxe", "Pico", pico((176, 182, 190))),
        ("wood_pickaxe", "Pico de madera", pico((150, 104, 62))),
        ("stone_pickaxe", "Pico de piedra", pico((130, 130, 144))),
        ("torch", "Antorcha", antorcha()),
        ("shield_wood", "Escudo de madera", escudo()),
        ("hammer", "Martillo", martillo()),
        ("sword_iron", "Espada de hierro", espada((176, 182, 190))),
        ("sword_steel", "Espada de acero", espada((205, 214, 232))),
        ("chest", "Cofre", cofre()),
    ]
    pag = Pagina()
    sprites, objetos = {}, {}
    for clave, etiqueta, l in lista:
        c, r = pag.poner(l.im)
        k = f"items/{clave}"
        sprites[k] = {"grid": [CELDA, CELDA], "png": HOJA_REL, "rect": {"col": c, "row": r, "w": 1, "h": 1}}
        objetos[k] = etiqueta
    publicar(pag, sprites, objetos, HOJA_REL, seco)


if __name__ == "__main__":
    main()
