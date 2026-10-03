#!/usr/bin/env python3
"""**Lo último dibujado «en Paint», rehecho con volumen** → `art/gen/restantes.png`. Correr DESPUÉS de los demás dibujar-*."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import lib.restantes as r  # noqa: E402
import lib.ui_v2 as u  # noqa: E402
from lib.iconos import espada, martillo, pico  # noqa: E402
from lib.pixel import CELDA, Pagina, publicar  # noqa: E402

HOJA_REL = "art/gen/restantes.png"


def main():
    seco = "--dry-run" in sys.argv
    lista = [
        ("items/hand", "Mano", r.mano()),
        ("items/jetpack", "Mochila de vuelo", r.jetpack()),
        ("items/probe", "Sonda", r.sonda()),
        ("items/part_engine", "Pieza: motor", r.motor()),
        ("items/part_fin", "Pieza: aleta", r.aleta()),
        ("items/part_lander", "Pieza: módulo", r.modulo()),
        ("items/part_nose", "Pieza: cono", r.nariz()),
        ("items/part_tank", "Pieza: tanque", r.tanque()),
        ("items/gem_blue", "Gema azul", r.gema((70, 120, 220))),
        ("items/gem_red", "Gema roja", r.gema((210, 64, 70))),
        ("items/mat_combustible", "Combustible", r.combustible()),
        ("machines/launch_pad", "Plataforma de lanzamiento", r.plataforma()),
        ("ui/build_chip", "Chip de construir", r.cartel_construir()),
        ("ui/icon_atacar", "Acción: atacar", espada((190, 196, 206))),
        ("ui/icon_cavar", "Acción: cavar", pico((176, 182, 190))),
        ("ui/icon_craft", "Acción: fabricar", martillo()),
        ("ui/icon_usar", "Acción: usar", r.mano()),
        ("ui/menu_ajustes", "Menú: ajustes", r.engranaje()),
        ("ui/menu_creativo", "Menú: creativo", r.varita()),
        ("ui/menu_espacio", "Menú: espacio", r.planeta_anillado()),
        ("prop/cesped_cortado", "Césped cortado", r.cesped_cortado()),
        ("prop/cancha_basket", "Cancha de básquet", r.cancha()),
        ("prop/cancha_tenis", "Cancha de tenis", r.cancha((0.74, 0.95, 0.88), "prop/cancha_tenis")),
        ("ship/rocket", "Cohete", r.cohete()),
        ("ui/menu_luna", "Menú: luna", r.luna_llena()),
    ]
    pag = Pagina()
    sprites, objetos = {}, {}
    for clave, etiqueta, l in lista:
        c, f = pag.poner(l.im)
        sprites[clave] = {"grid": [CELDA, CELDA], "png": HOJA_REL, "rect": {"col": c, "row": f, "w": l.im.width // CELDA, "h": l.im.height // CELDA}}
        objetos[clave] = etiqueta
    publicar(pag, sprites, objetos, HOJA_REL, seco)


if __name__ == "__main__":
    main()
