#!/usr/bin/env python3
"""**LAS ESQUINAS DEL ACANTILADO, POR FILAS.**

El kit dibuja cada esquina (`corner_se`, `corner_sw`) como UNA pieza de 1×3:
labio de esquina arriba, pared de esquina al medio, pie de esquina abajo. Una
pared de UN escalón usa esa pieza entera. Pero una pared más alta se arma por
filas (`world2d::pared_apilada`: labio + N cuerpos + pie), y como sólo existían
las filas de la pared RECTA, cada esquina alta se dibujaba con el labio recto:
el borde de arriba no doblaba y la esquina se veía rota entre dos tramos sanos
(reporte del usuario, 27 sep 2026).

Este script NO mide a ojo: parte el rect de la esquina que ya publicó
`import-acantilado.py` en sus tres filas y las publica como
`labio_se`/`cuerpo_se`/`pie_se` (y `_sw`). Correrlo después de ese importador.
"""

import copy
import json
from pathlib import Path

MANIFEST = Path(__file__).resolve().parent.parent / "native/crates/ha-game/assets/art/manifest.json"
FILAS = ("labio", "cuerpo", "pie")


def main():
    man = json.loads(MANIFEST.read_text())
    n = 0
    for clave, e in man["sprites"].items():
        if not clave.startswith(("cliff/", "slope/")):
            continue
        poses = e.get("poses") or {}
        for lado in ("se", "sw"):
            esquina = poses.get(f"corner_{lado}")
            if not esquina or esquina["rect"]["h"] != 3 or esquina["rect"]["w"] != 1:
                continue
            for i, fila in enumerate(FILAS):
                p = copy.deepcopy(esquina)
                p["rect"]["row"] += i
                p["rect"]["h"] = 1
                poses[f"{fila}_{lado}"] = p
                n += 1
    MANIFEST.write_text(json.dumps(man, ensure_ascii=False, indent=1))
    print(f"{n} poses de esquina por fila publicadas")


if __name__ == "__main__":
    main()
