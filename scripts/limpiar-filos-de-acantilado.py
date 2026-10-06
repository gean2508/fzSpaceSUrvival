#!/usr/bin/env python3
"""**UN SOLO FILO: la meseta va sólida hasta el borde y de ahí viene la piedra.**

Queja del usuario (6 oct 2026, captura con las esquinas del mesa tachadas): en el
borde de una terraza se veía «verde, verde, verde, se pone un filo y después otro
filo». Cada pieza del kit (`labio`, `labio_se/sw`, `inner_*`, `edge_e/w` y la fila
de arriba de las caras) trae dibujados, DENTRO de la meseta, arcos y sombras
verde oscuro del contorno redondeado del montículo. Puestas una al lado de la otra
en una escalera de bordes, cada celda repite su propio arco: una cadena de
cuadrados con filos que no empalman.

El filo que importa es el que TOCA la piedra (o el que baja al cuerpo de la pared
que sigue debajo). Todo lo demás es meseta y se pinta del color plano de la meseta.

Reglas (medidas sobre la hoja, no a ojo):

- **piedra** = colores del `cuerpo` de ese material que NO están en el `labio`
  (el labio es meseta pura más su filo).
- se conserva un píxel no-piedra sólo si está a ≤ `RADIO` píxeles de una piedra, o
  si la pieza sigue en una pared debajo (`labio`/primera fila de una cara: sus
  últimas `FILAS_DE_FILO` filas son el filo que empalma con el cuerpo).
- el resto se manda al color dominante de la pieza.

Idempotente: correrlo dos veces deja la hoja igual. Correr después de
`import-acantilado.py` y `derivar-esquinas-de-acantilado.py`.
"""

import json
from collections import Counter
from pathlib import Path

from PIL import Image

RAIZ = Path(__file__).resolve().parent.parent
ASSETS = RAIZ / "native/crates/ha-game/assets"
MANIFEST = ASSETS / "art/manifest.json"
CELDA = 32
RADIO = 3
FILAS_DE_FILO = 8

#: Piezas de UNA celda que son meseta con filo.
SUELTAS = ("labio", "labio_se", "labio_sw", "inner_ne", "inner_nw", "inner_se", "inner_sw", "edge_e", "edge_w")
#: Piezas de 1×3 cuya fila de arriba es meseta con filo.
CARAS = ("edge_s", "center", "corner_se", "corner_sw")
#: Las que arrancan una pared hacia abajo: arriba de ellas hay meseta lisa, así
#: que su borde superior no puede terminar en una púa de roca suelta.
SIN_PUA_ARRIBA = ("labio_se", "labio_sw", "inner_se", "inner_sw")
FILAS_SIN_PUA = 12
#: Las que siguen en una pared debajo de la celda.
CON_PARED_ABAJO = {"labio", "labio_se", "labio_sw"}


def rect_px(r, fila=0, filas=None):
    h = r["h"] if filas is None else filas
    return (r["col"] * CELDA, (r["row"] + fila) * CELDA, (r["col"] + r["w"]) * CELDA, (r["row"] + fila + h) * CELDA)


def paleta(im, box):
    return {p[:3] for p in im.crop(box).getdata() if p[3] > 0}


def limpiar(im, box, piedra, pared_abajo, sin_pua=False):
    c = im.crop(box)
    w, h = c.size
    px = c.load()
    cuenta = Counter(px[x, y][:3] for y in range(h) for x in range(w) if px[x, y][3] > 0)
    if not cuenta:
        return 0
    plano = cuenta.most_common(1)[0][0]
    es_piedra = [[px[x, y][3] > 0 and px[x, y][:3] in piedra for x in range(w)] for y in range(h)]
    cambios = 0
    for y in range(h):
        for x in range(w):
            p = px[x, y]
            if sin_pua and y < FILAS_SIN_PUA and p[3] > 0 and p[:3] != plano:
                px[x, y] = (*plano, p[3])
                cambios += 1
                continue
            if p[3] == 0 or p[:3] == plano or es_piedra[y][x]:
                continue
            if pared_abajo and y >= h - FILAS_DE_FILO:
                continue
            cerca = any(
                es_piedra[yy][xx]
                for yy in range(max(0, y - RADIO), min(h, y + RADIO + 1))
                for xx in range(max(0, x - RADIO), min(w, x + RADIO + 1))
            )
            if cerca:
                continue
            px[x, y] = (*plano, p[3])
            cambios += 1
    if cambios:
        im.paste(c, box[:2])
    return cambios


def main():
    man = json.loads(MANIFEST.read_text())
    hojas = {}
    total = 0
    hechos = set()
    for clave, e in sorted(man["sprites"].items()):
        familia, _, material = clave.partition("/")
        if familia not in ("cliff", "ledge") or not e.get("poses") or "labio" not in e["poses"] and "edge_e" not in e["poses"]:
            continue
        ruta = e["png"]
        if ruta not in hojas:
            hojas[ruta] = Image.open(ASSETS / ruta).convert("RGBA")
        im = hojas[ruta]
        poses = e["poses"]
        # La piedra sale del cuerpo del acantilado del MISMO material.
        ref = man["sprites"].get(f"cliff/{material}", e)["poses"]
        if "cuerpo" not in ref or "labio" not in ref:
            continue
        pi = Image.open(ASSETS / man["sprites"].get(f"cliff/{material}", e)["png"]).convert("RGBA")
        piedra = paleta(pi, rect_px(ref["cuerpo"]["rect"])) - paleta(pi, rect_px(ref["labio"]["rect"]))
        for nombre in SUELTAS:
            if nombre in poses:
                box = rect_px(poses[nombre]["rect"], 0, 1)
                if (ruta, box) not in hechos:
                    hechos.add((ruta, box))
                    total += limpiar(im, box, piedra, nombre in CON_PARED_ABAJO, nombre in SIN_PUA_ARRIBA)
        for nombre in CARAS:
            r = poses.get(nombre, {}).get("rect")
            if r and r["h"] >= 1:
                box = rect_px(r, 0, 1)
                if (ruta, box) not in hechos:
                    hechos.add((ruta, box))
                    total += limpiar(im, box, piedra, True, nombre in ('corner_se', 'corner_sw'))
    for ruta, im in hojas.items():
        im.save(ASSETS / ruta, optimize=True)
    print(f"{total} píxeles de filo interior aplanados en {len(hojas)} hoja(s)")


if __name__ == "__main__":
    main()
