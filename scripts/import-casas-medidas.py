#!/usr/bin/env python3
"""**Cuánto mide cada casa del pack, para que la sim edifique ESE rectángulo y no otro.**

    python3 scripts/import-casas-medidas.py            # escribe ha-shared/src/casas_medidas.rs
    python3 scripts/import-casas-medidas.py --check    # falla si el .rs no coincide con el manifest

## Por qué (3 oct 2026, pedido del usuario)

> *«Las casas no deben ocupar toda la parcela. Que la casa ocupe una parte, que el interior ocupe esa misma
> parte que ocupa por fuera, y el resto de la parcela se decore: jardín, piscina. Está prohibido que en la
> parcela vaya sólo la casa, y que la casa ocupe toda la parcela.»*

Hasta hoy la huella de la casa era un 55 % x 50 % del jardín (`city::CASA_DEL_LOTE`), sin mirar el dibujo: la
fachada se estiraba para taparla (`building::roof_layout(cubrir)`) y salía deformada, o quedaba corta y asomaba el
interior. Ahora **la huella es el tamaño del dibujo elegido**, con la escala del pack (`ESCALA_ARTE`), y todo lo
que sobra de la parcela es jardín. Este script MIDE los dibujos (celdas del manifest) y los publica como una
tabla Rust, igual que `import-casas.py` publica los kits: nunca a mano.

El cliente elige, entre las variantes del slot, la que mide lo que midió la sim (`city::huella_de_dibujo`); el
test `las_casas_medidas_coinciden_con_el_manifest` (ha-game) cierra el círculo.
"""
import json
import re
import sys
from pathlib import Path

RAIZ = Path(__file__).resolve().parent.parent
MANIFEST = RAIZ / "native/crates/ha-game/assets/art/manifest.json"
CITY = RAIZ / "native/crates/ha-shared/src/city.rs"
SALIDA = RAIZ / "native/crates/ha-shared/src/casas_medidas.rs"


def programas() -> list[str]:
    src = CITY.read_text()
    pat = re.compile(r'LotProgram \{ key: "([^"]+)", cells: \(\d+, \d+\), district: District::Residential')
    return pat.findall(src)


def generar() -> str:
    man = json.loads(MANIFEST.read_text())
    sp, ob = man["sprites"], man.get("objects", {})
    filas = []
    for base in programas():
        if base == "building/casa_hilera":
            continue  # se arma con el kit tramo/remate, del ancho exacto de la huella
        ids = [v["id"] for v in ob.get(base, {}).get("variants", [])]
        claves = [f"{base}#{i}" for i in ids if f"{base}#{i}" in sp] or ([base] if base in sp else [])
        medidas = []
        for k in claves:
            r = sp[k].get("rect")
            if r and (r["w"], r["h"]) not in medidas:
                medidas.append((r["w"], r["h"]))
        if medidas:
            filas.append((base, sorted(medidas)))
    out = [
        "//! **GENERADO por `scripts/import-casas-medidas.py` — no se edita a mano.**",
        "//!",
        "//! Celdas (ancho, alto) de cada dibujo de casa del pack, por programa de lote. La sim edifica el rectángulo",
        "//! que mide el dibujo ([`crate::city::huella_de_dibujo`]) y el cliente dibuja ESA variante, sin estirarla.",
        "",
        "pub const CASAS_DEL_PACK: &[(&str, &[(i64, i64)])] = &[",
    ]
    for base, ms in filas:
        out.append(f'    ("{base}", &[{", ".join(f"({w}, {h})" for w, h in ms)}]),')
    out.append("];")
    out.append("")
    return "\n".join(out)


def main():
    nuevo = generar()
    if "--check" in sys.argv:
        if not SALIDA.exists() or SALIDA.read_text() != nuevo:
            raise SystemExit("casas_medidas.rs no coincide con el manifest: corré scripts/import-casas-medidas.py")
        print("casas_medidas.rs al día")
        return
    SALIDA.write_text(nuevo)
    print(nuevo)


if __name__ == "__main__":
    main()
