#!/usr/bin/env python3
"""**Devuelve las poses a las cercas que las perdieron** (5 oct 2026).

    python3 scripts/cercas-reparar.py

`Tile::Fence` pide `fence/madera`, y en el manifest esa clave (y `alambre`) quedó SIN poses —sólo `fence/reja` conservó las
suyas, las de `art/pack/cercas.png`—. `tile_has_art` daba `false`, el mundo se saltaba el sprite y el perímetro de cada
parcela se veía como una franja de TIERRA marrón (el color plano del tile). Sin la materia prima cruda (`PRUEBAS-CODIGOS`)
no se puede volver a correr `import-cercas.py`; este script reparte las poses medidas de `reja` —tramos, codos y las dos
caras de cada lado— a `madera` y `alambre`, más `solo` (el poste) y `ew@n`. Idempotente. Cuando haya hoja cruda,
`import-cercas.py` manda y este script sobra.
"""
import json, os

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MANIFEST = os.path.join(ROOT, "native/crates/ha-game/assets/art/manifest.json")


def main():
    man = json.load(open(MANIFEST))
    sp = man["sprites"]
    reja = sp["fence/reja"]
    for clave in ("fence/madera", "fence/alambre"):
        e = sp[clave]
        poses = json.loads(json.dumps(reja["poses"]))
        poses["solo"] = dict(poses["s"])
        poses["ew@n"] = dict(poses["ew"])
        e["png"] = reja["png"]
        e["poses"] = poses
        e["rect"] = dict(poses["ew"]["rect"])
        print(f"{clave}: {len(poses)} poses")
    # El murete de piedra (Wall_1): una pose por celda de la hoja, leídas de `cercas.png` fila 5 (y la 15 de la fila 4).
    pg = reja["png"]
    R = lambda c, r: {"rect": {"col": c, "row": r, "w": 1, "h": 1}}
    poses = {"ew": R(0, 5), "ew@s": R(0, 5), "ew@s#2": R(1, 5), "ew@n": R(7, 5), "ns": R(4, 5), "ns@e": R(9, 5),
             "ns@w": R(6, 5), "sw": R(2, 5), "es": R(15, 4), "ne": R(3, 5), "solo": R(5, 5)}
    e = sp["fence/piedra"]
    e["png"], e["poses"], e["rect"] = pg, poses, dict(poses["ew"]["rect"])
    json.dump(man, open(MANIFEST, "w"), indent=1, ensure_ascii=False)


main()
