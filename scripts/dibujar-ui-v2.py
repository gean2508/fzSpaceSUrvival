#!/usr/bin/env python3
"""**ÍCONOS DE INTERFAZ Y MÁQUINAS CON VOLUMEN** (reemplazan los de `dibujar-ui.py` / `dibujar-mundo.py`).

    python3 scripts/dibujar-ui-v2.py [--dry-run]

Pestañas de la mochila, mira, globo del menú y las máquinas del juego (extractores, taladro, hábitat,
plataforma), rehechos con `lib/ui_v2.py`. Pisan esas claves: correr DESPUÉS de `dibujar-ui.py` y `dibujar-mundo.py`.
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib import ui_v2 as u  # noqa: E402
from lib.pixel import CELDA, Pagina, publicar  # noqa: E402

HOJA_REL = "art/gen/ui2.png"


def main():
    seco = "--dry-run" in sys.argv
    lista = [
        ("ui/tab_inventario", "Pestaña: mochila", u.mochila()),
        ("ui/tab_craft", "Pestaña: fabricar", u.martillo_yunque()),
        ("ui/tab_construir", "Pestaña: construir", u.casita()),
        ("ui/crosshair", "Mira", u.mira()),
        ("ui/menu_tierra", "Menú: Tierra", u.tierra()),
        ("machines/wood_extractor", "Extractor de madera", u.extractor((150, 104, 62), (176, 182, 190))),
        ("machines/stone_extractor", "Extractor de piedra", u.extractor((130, 134, 148), (226, 190, 60))),
        ("machines/regolith_drill", "Taladro de regolito", u.taladro()),
        ("machines/habitat", "Hábitat", u.habitat()),
        ("machines/launch_pad", "Plataforma de lanzamiento", u.plataforma()),
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
