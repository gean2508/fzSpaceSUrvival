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
    json.dump(man, open(MANIFEST, "w"), indent=1, ensure_ascii=False)


main()
