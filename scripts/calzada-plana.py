#!/usr/bin/env python3
"""**En la calzada no va un cajero automático.**

    python3 scripts/calzada-plana.py --dry-run
    python3 scripts/calzada-plana.py

`pack_siembra::CALLE` se había generado con la misma regla que la vereda, así que
el asfalto recibía bancos, tachos, bebederos y cajeros: los objetos de la acera,
sembrados en medio de la calle. Se lee como lo que es, un error de colocación.

En una calzada sólo hay cosas **pintadas o tiradas en el suelo**: tapas de
alcantarilla, marcas de frenada, cartones, latas, conos. La regla es medible y no
depende del nombre —que en este pack no es de fiar—: **entra lo que mide una
celda de alto**. Lo que se levanta del suelo vuelve a la vereda, que es su sitio.

La regla quedó también en `import-pack-masivo.py` (`max_alto`), así que una
reimportación la respeta. Este script sólo arregla la tabla que ya está escrita,
para no tener que rehacer las 11.700 piezas.
"""

import argparse
import json
import os
import re

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SIEMBRA = os.path.join(ROOT, "native/crates/ha-shared/src/pack_siembra.rs")
MANIFEST = os.path.join(ROOT, "native/crates/ha-game/assets/art/manifest.json")

#: Alto máximo, en TILES, para que algo pueda estar en la calzada. Una celda del
#: pack son 1,5 tiles: con 1,6 entra lo de una celda y nada de dos.
MAX_ALTO = 1.6


def alto_declarado(linea):
    m = re.search(r"size: \[[\d.]+, ([\d.]+)\]", linea)
    return float(m.group(1)) if m else None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    src = open(SIEMBRA, encoding="utf8").read()
    m = re.search(r"(pub const CALLE: &\[ExtraDecor\] = &\[)(.*?)(\n\];)", src, re.S)
    if not m:
        raise SystemExit("no encontré la tabla CALLE")
    lineas = [l for l in m.group(2).split("\n") if "ExtraDecor" in l]

    quedan, se_van = [], []
    for l in lineas:
        h = alto_declarado(l)
        (quedan if h is not None and h <= MAX_ALTO else se_van).append(l)

    print(f"CALZADA: {len(lineas)} objetos → quedan {len(quedan)}, se van {len(se_van)}")
    for l in se_van[:12]:
        clave = re.search(r'key: "([^"]+)"', l)
        print(f"  fuera (se levanta del suelo): {clave.group(1) if clave else l.strip()[:60]}")
    if len(se_van) > 12:
        print(f"  … y {len(se_van) - 12} más")
    if not quedan:
        raise SystemExit("no quedaría nada en la calzada: revisá MAX_ALTO")
    if args.dry_run:
        return

    cuerpo = "\n" + "\n".join(quedan)
    src = src[: m.start(2)] + cuerpo + src[m.end(2) :]
    open(SIEMBRA, "w", encoding="utf8").write(src)
    print(f"pack_siembra.rs: CALLE queda con {len(quedan)} objetos, todos a ras del suelo")


if __name__ == "__main__":
    main()
