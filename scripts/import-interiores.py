#!/usr/bin/env python3
"""**Los muebles de adentro, por tema.**

    python3 scripts/import-interiores.py --dry-run
    python3 scripts/import-interiores.py

Genera `ha-shared/src/interior_siembra.rs`: una tabla de muebles por TEMA de
interior, con la misma forma que `pack_siembra` (que amuebla la vereda, el parque
y la playa) para que la consuma el mismo camino de siembra.

## La regla es el tema, y por eso las cosas pegan entre sí

Cada clave del pack lleva su tema adelante (`furniture/cocina_…`,
`furniture/dormitorio_…`, `furniture/aula_…`), y cada parcela de la ciudad
declara de qué tema es. Una casa sólo puede recibir muebles de casa; una escuela,
de aula; un hospital, de hospital. No hace falta revisar objeto por objeto: **una
puerta industrial no puede entrar a un dormitorio porque el tema no la ofrece.**

## Qué entra y qué no, medido

- **Cabe en un cuarto**: hasta `MAX_TILES` de lado. Una cocina entera de 12 tiles
  no es un mueble, es un decorado.
- **Está dibujado**: al menos `OPACIDAD_MIN` de su recorte. Descarta los recortes
  de pared y las capas sueltas que el pack trae para componer.
- **No es un piso ni una pared**: esas claves van por otro lado (la hoja del
  suelo y las piezas de construcción), y sembradas como mueble quedarían tiradas
  en el medio del cuarto.
"""

import argparse
import json
import os
import re
from collections import defaultdict

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ASSETS = os.path.join(ROOT, "native/crates/ha-game/assets")
MANIFEST = os.path.join(ASSETS, "art/manifest.json")
SALIDA = os.path.join(ROOT, "native/crates/ha-shared/src/interior_siembra.rs")

#: **Una celda de 32 px del pack es UN tile del juego**, la misma regla que las
#: paredes (`ArtLibrary::medida_de_obra`: una celda del pack, un tile). Estaba en
#: 1,5 y los muebles salían un 50 % más grandes que el cuarto que los contiene:
#: una cama de 2×3 celdas ocupaba 3×4,5 m de una casa de 11×8, y los cuartos
#: quedaban tapados (reporte del 29 sep 2026, "me quedo atorado").
ESCALA_TILE = 1.0
#: Lado máximo de un mueble, en tiles. Más que esto no entra en un cuarto.
MAX_TILES = 6.0
#: Fracción mínima del recorte dibujada.
OPACIDAD_MIN = 0.35
#: Cuántos muebles por tabla. Con más, la casa se llena; con menos, se repite.
POR_TABLA = 40

#: **Los temas de interior que existen en el pack**, con el nombre de la tabla en
#: Rust. El emparejamiento tema→lote vive en `worldgen::tabla_interior`.
TEMAS = {
    "cocina": "COCINA",
    "dormitorio": "DORMITORIO",
    "bano": "BANO",
    "living": "LIVING",
    "sala": "SALA",
    "aula": "AULA",
    "hospital": "HOSPITAL",
    "carcel": "CARCEL",
    "oficina": "OFICINA",
    "sotano": "SOTANO",
    "museo": "MUSEO",
    "gimnasio": "GIMNASIO",
    "estudio": "ESTUDIO",
}

#: Palabras que delatan una pieza de OBRA, no un mueble: se recortan del pack
#: para componer cuartos, y sembradas sueltas quedan tiradas en el piso.
NO_ES_MUEBLE = ("pared", "piso", "techo", "ventana", "puerta", "capa", "modular", "suelo")

#: **Lo que el nombre no delata y la lámina sí** (mirado el 29 sep 2026, uno por
#: uno): los singles de Modern Interiors están NUMERADOS, así que las losas de
#: piso, los manteles, las tapas de mesada y las cortinas entraban como muebles y
#: se sembraban de pie en mitad del cuarto — el usuario lo vio como "el
#: importador trae cosas que no son". Son planos o cuelgan de una ventana: ninguno
#: es un objeto que se apoya en el piso.
NO_SON_MUEBLES = {
    # cocina: losas de color, manteles, tablas de mesada sueltas
    "furniture/cocina_2", "furniture/cocina_4", "furniture/cocina_13", "furniture/cocina_17",
    "furniture/cocina_20", "furniture/cocina_22", "furniture/cocina_31", "furniture/cocina_40",
    "furniture/cocina_49", "furniture/cocina_58", "furniture/cocina_67", "furniture/cocina_76",
    "furniture/cocina_85", "furniture/cocina_253", "furniture/cocina_256", "furniture/cocina_336",
    # dormitorio: cortinas y ventanas
    "furniture/dormitorio_57", "furniture/dormitorio_119", "furniture/dormitorio_196",
    "furniture/dormitorio_197", "furniture/dormitorio_198", "furniture/dormitorio_199",
    "furniture/dormitorio_200", "furniture/dormitorio_201", "furniture/dormitorio_202",
    "furniture/dormitorio_203", "furniture/dormitorio_204", "furniture/dormitorio_205",
    "furniture/dormitorio_206", "furniture/dormitorio_207", "furniture/dormitorio_208",
    # baño: la alfombra de toalla; living: el marco suelto
    "furniture/bano_76", "furniture/living_4",
}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    with open(MANIFEST) as fh:
        sprites = json.load(fh)["sprites"]
    cache = {}
    por_tema = defaultdict(list)

    for clave, e in sprites.items():
        if "#" in clave or not clave.startswith(("furniture/", "interior/")):
            continue
        familia = clave.split("/", 1)[1]
        tema = next((t for t in TEMAS if familia.startswith(t + "_")), None)
        if tema is None:
            continue
        if any(p in familia for p in NO_ES_MUEBLE) or clave in NO_SON_MUEBLES:
            continue
        r = e.get("rect") or e.get("strip")
        if not r:
            continue
        tw, th = r["w"] * ESCALA_TILE, r["h"] * ESCALA_TILE
        if tw > MAX_TILES or th > MAX_TILES:
            continue
        png = e.get("png")
        ruta = os.path.join(ASSETS, png or "")
        if not png or not os.path.exists(ruta):
            continue
        if png not in cache:
            cache[png] = Image.open(ruta).convert("RGBA")
        gw, gh = e.get("grid", [32, 32])
        im = cache[png].crop(
            (r["col"] * gw, r["row"] * gh, (r["col"] + r["w"]) * gw, (r["row"] + r["h"]) * gh)
        )
        alfa = list(im.getchannel("A").getdata())
        if sum(1 for p in alfa if p > 0) / len(alfa) < OPACIDAD_MIN:
            continue
        # Peso: favorece lo que tiene variantes de color, igual que `pack_siembra`
        # — un objeto con doce colores puede repetirse doce veces sin cansar.
        variantes = sum(1 for k in sprites if k.startswith(clave + "#"))
        por_tema[tema].append((clave, round(tw, 2), round(th, 2), min(24, 6 + 3 * variantes)))

    lineas = [
        "//! **Muebles de interior por TEMA** — generado por",
        "//! `scripts/import-interiores.py`, no se edita a mano.",
        "//!",
        "//! Cada tabla es lo que hay adentro de un edificio de ese tema. Lo consume",
        "//! `worldgen::interior_decor_at`, que elige la tabla por el programa del",
        "//! lote: una casa recibe cocina/dormitorio/baño/living, una escuela recibe",
        "//! aula, un hospital recibe hospital. **Por eso los objetos pegan entre sí**",
        "//! — el tema no ofrece lo que no corresponde.",
        "",
        "use crate::worldgen::ExtraDecor;",
        "",
    ]
    total = 0
    for tema, nombre in TEMAS.items():
        items = sorted(por_tema.get(tema, []), key=lambda x: (-x[3], x[0]))[:POR_TABLA]
        total += len(items)
        print(f"  {nombre:12s} {len(items):3d} muebles")
        lineas.append(f"pub const {nombre}: &[ExtraDecor] = &[")
        for clave, tw, th, peso in items:
            lineas.append(
                f'    ExtraDecor {{ key: "{clave}", size: [{tw:.2f}, {th:.2f}], weight: {peso} }},'
            )
        lineas.append("];")
        lineas.append("")

    print(f"total: {total} muebles en {len(TEMAS)} tablas")
    if args.dry_run:
        return
    open(SALIDA, "w", encoding="utf8").write("\n".join(lineas))
    print(f"escrito {SALIDA}")


if __name__ == "__main__":
    main()
