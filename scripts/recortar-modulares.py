#!/usr/bin/env python3
"""**Recorta el sangrado de las bandas modulares ya importadas (9 celdas → las 7 de verdad).**

    python3 scripts/recortar-modulares.py [--dry-run]

La hoja `5_Floor_Modular_Buildings` dibuja cada banda en un bloque de 9 celdas con UNA CELDA DE SANGRADO por
lado (el pedazo del módulo vecino). El borde verdadero está en los pixeles 32|33 y 255|256: 224 px = 7 celdas.
`import-modulares.py` ya recorta al importar; este script arregla un manifest importado antes (no hace falta la
hoja cruda, sólo mueve el rect una celda y lo achica dos). Idempotente: sólo toca lo que mide 9 de ancho.
También corre `puerta*_en_sprite` (píxeles desde la esquina del recorte) 32 px a la izquierda.
"""
import json, os, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MANIFEST = os.path.join(ROOT, "native/crates/ha-game/assets/art/manifest.json")


def main():
    man = json.load(open(MANIFEST))
    n = 0
    for k, e in man["sprites"].items():
        base = e.get("poses", {}).get("base", e)
        if "modulares_" not in base.get("png", "") or base["rect"]["w"] != 9:
            continue
        for r in {id(x): x for x in (e["rect"], base["rect"])}.values():
            r["col"] += 1
            r["w"] = 7
        for nombre, pose in e.get("poses", {}).items():
            if nombre.startswith("puerta") and nombre.endswith("_en_sprite"):
                pose["rect"]["col"] -= 32
                assert pose["rect"]["col"] >= 0, (k, nombre)
        n += 1
    print(f"{n} bandas recortadas a 7 celdas")
    if "--dry-run" not in sys.argv:
        json.dump(man, open(MANIFEST, "w"), indent=1, ensure_ascii=False)


main()
