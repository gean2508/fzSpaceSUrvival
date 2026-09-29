#!/usr/bin/env python3
"""**LOS SUELOS CON TEXTURA: marcas ocasionales sobre el color liso.**

Reclamo del usuario (28 sep 2026): *"la nieve no tiene texturas, es un suelo
liso blanco… así con los suelos en general"*. Así pinta LimeZu sus terrenos —el
interior de un color plano y el detalle en marcas sueltas—, y el juego ya sabe
repartirlas: la losa lisa manda y una con marca aparece salteada
(`art2d::peso_liso`). Pero desde el 27 sep 2026 **ningún suelo natural tenía
marcas**: las variantes que había eran piezas de COSTURA del pack (con la
esquina de tierra naranja metida en el pasto) y `atlas-suelo.py::contamina` las
sacó con razón. Quedó todo liso.

El pack no trae marcas sueltas limpias de suelo, así que se DIBUJAN (skill
`pixel-art`): cada material con los tonos de SU losa en uso, bajados o subidos
por la rampa de la paleta del pack (`lib/pixel.py::rampa`):

- **pastos** (`tiles/grass` y los cuatro de bioma): matas de tres briznas;
- **arena, tierra, regolito, basalto**: granos y piedritas con su sombra;
- **nieve**: montículos, un banco ondulado, piedras heladas y grumos, con la
  rampa del filo de nieve del kit de acantilado (`cliff/nieve`).

El fondo de cada losa es EXACTAMENTE el color de la losa lisa del material y
ninguna marca toca el borde de la celda: empalma con la lisa por los cuatro
lados. Las marcas pasan por el corsé de la librería (`verificar`) antes de
pegarse sobre el fondo. Van a celdas libres de `art/pack/suelo.png` (la hoja que
la malla del suelo lee por UV) como variantes `<material>#m1..`. Lo corre
`atlas-suelo.py` al terminar, porque ese reescribe la hoja.

```sh
python3 scripts/dibujar-marcas-de-suelo.py
```
"""

import json
import sys
from pathlib import Path

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib.pixel import Lienzo, rampa  # noqa: E402

RAIZ = Path(__file__).resolve().parent.parent
ASSETS = RAIZ / "native/crates/ha-game/assets"
MANIFEST = ASSETS / "art/manifest.json"
HOJA = ASSETS / "art/pack/suelo.png"
GEN = ASSETS / "art/gen/suelo"
CELL = 32

# La rampa del filo de nieve del kit (`cliff/nieve`).
N_SOMBRA_1 = (204, 216, 230)
N_SOMBRA_2 = (193, 205, 218)
N_SOMBRA_3 = (172, 183, 194)
N_FILO = (153, 162, 172)
N_BRILLO = (226, 242, 243)


# ── los motivos ──────────────────────────────────────────────────────────────
# Cada motivo dibuja SÓLO la marca sobre un lienzo transparente; el fondo lo pone
# `losa()` con el color exacto del material.

def mata(l, x, y, oscuro, medio, claro):
    """Una mata del pack: cinco briznas en abanico, la del medio más alta y con
    la punta clara, y la base en el tono oscuro. Mide 7×6: a la escala del
    juego (una celda ≈ 20 px en pantalla) una de 5×4 no se veía."""
    l.rect(x + 1, y + 5, 5, 1, oscuro)
    l.rect(x, y + 2, 1, 3, medio)
    l.rect(x + 2, y + 1, 1, 4, medio)
    l.rect(x + 3, y, 1, 5, medio)
    l.rect(x + 4, y + 1, 1, 4, medio)
    l.rect(x + 6, y + 2, 1, 3, medio)
    l.punto(x + 3, y, claro)
    l.punto(x + 2, y + 1, claro)


def matas(posiciones):
    def dibujar(tonos):
        oscuro, medio, _, claro = tonos
        l = Lienzo(CELL, CELL)
        for (x, y) in posiciones:
            mata(l, x, y, oscuro, oscuro if medio == claro else medio, claro)
        return l
    return dibujar


def piedrita(l, x, y, w, sombra, cuerpo, luz):
    """Una piedrita de dos filas con su sombra abajo-derecha y la luz arriba-
    izquierda."""
    l.rect(x + 1, y + 2, w, 1, sombra)
    l.rect(x, y, w, 2, cuerpo)
    l.rect(x, y, max(1, w - 1), 1, luz)


def granos(posiciones):
    def dibujar(tonos):
        oscuro, medio, _, claro = tonos
        l = Lienzo(CELL, CELL)
        for (x, y, w) in posiciones:
            piedrita(l, x, y, w, oscuro, medio, claro)
        return l
    return dibujar


def n_monticulos(_):
    l = Lienzo(CELL, CELL)
    for (cx, cy, rx, ry) in ((12, 13, 8, 4), (22, 22, 6, 3)):
        l.elipse(cx + 1.5, cy + 2, rx, ry, N_SOMBRA_2)
        l.elipse(cx + 0.5, cy + 1, rx, ry, N_SOMBRA_1)
        l.elipse(cx, cy, rx - 0.5, ry - 0.6, (212, 225, 239))
        l.elipse(cx - 2, cy - 1, rx * 0.5, ry * 0.45, N_BRILLO)
    return l


def n_ondas(_):
    l = Lienzo(CELL, CELL)
    for (x0, y0, largo) in ((4, 8, 14), (10, 16, 18), (5, 24, 13)):
        l.rect(x0, y0 + 1, largo, 1, N_SOMBRA_3)
        l.rect(x0 + 1, y0, largo - 1, 1, N_SOMBRA_2)
        l.rect(x0 + 3, y0 - 1, largo - 6, 1, N_BRILLO)
    return l


def n_piedras(_):
    l = Lienzo(CELL, CELL)
    for (cx, cy, r) in ((11, 14, 4), (21, 21, 3)):
        l.elipse(cx + 1, cy + 2.5, r + 1.5, 1.8, N_SOMBRA_2)
        l.elipse(cx, cy + 1, r, r * 0.75, N_SOMBRA_3)
        l.elipse(cx + 1, cy + 1.8, r * 0.7, r * 0.35, N_FILO)
        l.elipse(cx - 0.5, cy - 0.2, r * 0.8, r * 0.35, (212, 225, 239))
    return l


def n_grumos(_):
    l = Lienzo(CELL, CELL)
    for (x, y) in ((6, 6), (19, 5), (11, 12), (24, 13), (5, 19), (16, 21), (25, 25), (9, 26)):
        l.rect(x, y + 1, 3, 1, N_SOMBRA_2)
        l.rect(x, y, 2, 1, N_BRILLO)
    return l


MATAS = [
    matas([(4, 5), (19, 13), (8, 21)]),
    matas([(13, 4), (3, 16), (22, 22)]),
    matas([(21, 6), (11, 15)]),
    matas([(5, 10), (16, 20), (23, 4), (10, 25)]),
]
GRANOS = [
    granos([(6, 7, 3), (21, 12, 4), (11, 22, 3)]),
    granos([(15, 5, 4), (5, 18, 3), (23, 23, 3), (16, 15, 2)]),
    granos([(9, 12, 4), (21, 24, 3)]),
]
NIEVE = [n_monticulos, n_ondas, n_piedras, n_grumos]

#: Qué motivos le tocan a cada material del suelo natural.
MATERIALES = {
    "tiles/grass": MATAS,
    "tiles/pasto_bosque": MATAS,
    "tiles/pasto_seco": MATAS,
    "tiles/pasto_pantano": MATAS,
    "tiles/pasto_sierra": MATAS,
    "tiles/sand": GRANOS,
    "tiles/dirt": GRANOS,
    "tiles/regolith": GRANOS,
    "tiles/basalt": GRANOS,
    "tiles/ice": NIEVE,
}


def color_de_losa(hoja, entrada):
    r = entrada["rect"]
    return hoja.getpixel((r["col"] * CELL + CELL // 2, r["row"] * CELL + CELL // 2))[:3]


def losa(fondo, marca: Lienzo):
    marca.verificar(apoyado=False)
    im = Image.new("RGBA", (CELL, CELL), (*fondo, 255))
    im.alpha_composite(marca.im)
    return im


def celdas_libres(man, hoja, reservadas):
    usadas = set(reservadas)
    for e in man["sprites"].values():
        if e.get("png") == "art/pack/suelo.png" and "rect" in e:
            r = e["rect"]
            for dc in range(r["w"]):
                for dr in range(r["h"]):
                    usadas.add((r["col"] + dc, r["row"] + dr))
    for row in range(hoja.height // CELL):
        for col in range(hoja.width // CELL):
            if (col, row) in usadas:
                continue
            celda = hoja.crop((col * CELL, row * CELL, (col + 1) * CELL, (row + 1) * CELL))
            if celda.getbbox() is None:
                yield col, row


def main():
    man = json.loads(MANIFEST.read_text())
    hoja = Image.open(HOJA).convert("RGBA")
    GEN.mkdir(parents=True, exist_ok=True)
    previas = {k: e["rect"] for k, e in man["sprites"].items() if "#m" in k and e.get("png") == "art/pack/suelo.png"}
    libres = celdas_libres(man, hoja, [])
    total = 0
    for base, motivos in MATERIALES.items():
        entrada = man["sprites"].get(base)
        if not entrada or entrada.get("png") != "art/pack/suelo.png":
            print(f"  (sin losa de {base}: se saltea)")
            continue
        fondo = color_de_losa(hoja, entrada)
        tonos = rampa(fondo, 4)
        obj = man["objects"].setdefault(base, {"label": base, "variants": []})
        ids = {v["id"] for v in obj["variants"]}
        for i, motivo in enumerate(motivos, start=1):
            vid = f"m{i}"
            clave = f"{base}#{vid}"
            im = losa(fondo, motivo(tonos))
            im.save(GEN / f"{base.split('/')[1]}_{vid}.png")
            if clave in previas:
                col, row = previas[clave]["col"], previas[clave]["row"]
            else:
                col, row = next(libres)
            hoja.paste(im, (col * CELL, row * CELL))
            man["sprites"][clave] = {
                "grid": entrada.get("grid", [CELL, CELL]),
                "layer": entrada.get("layer", "Floor"),
                "png": "art/pack/suelo.png",
                "rect": {"col": col, "row": row, "w": 1, "h": 1},
            }
            if vid not in ids:
                obj["variants"].append({"id": vid, "label": f"{base.split('/')[1]} con marca {i}"})
            total += 1
        print(f"{base}: {len(motivos)} marcas sobre {fondo}")
    hoja.save(HOJA, optimize=True)
    MANIFEST.write_text(json.dumps(man, ensure_ascii=False, indent=1))
    print(f"{total} losas con marca en {HOJA.name}")


if __name__ == "__main__":
    main()
