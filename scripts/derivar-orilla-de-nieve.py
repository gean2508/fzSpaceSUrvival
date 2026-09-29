#!/usr/bin/env python3
"""**LA ORILLA DEL AGUA EN LA NIEVE ES DE NIEVE.**

Pedido del usuario, 28 sep 2026: *"que la orilla del río no se vea de llano,
sino con el asset del bioma"*. Hasta acá toda agua de tierra adentro llevaba la
ribera de pasto del pack (`tiles/water`), y en la cumbre nevada eso era una
franja verde con barro rojo cortando el blanco. El pack no trae agua con orilla
nevada, así que se DERIVA (skill `pixel-art`, paso 2): se copia el autotile
animado de `tiles/water` y cada tono de la ribera se cambia por el tono de nieve
del mismo escalón de luz, sacado del kit de acantilado de nieve que el juego ya
dibuja (`cliff/nieve`, `edge_n`/`labio`). El agua no se toca.

Nada se escribe a ojo:

- **qué es ribera**: los colores del recorte que no son agua, medidos acá
  (`medir()`); el script se planta si aparece uno que no sabe mapear;
- **a qué se mapea**: los tonos del filo de nieve del kit, ordenados por
  luminancia; el pasto va a la cara de la nieve y el barro al filo gris azulado.

Escribe `art/gen/agua_nieve.png` y el slot `tiles/water_nieve` con las mismas
poses que `tiles/water` (mismas celdas: la hoja es una copia).

```sh
python3 scripts/derivar-orilla-de-nieve.py
```
"""

import copy
import json
from pathlib import Path

import numpy as np
from PIL import Image

RAIZ = Path(__file__).resolve().parent.parent
ASSETS = RAIZ / "native/crates/ha-game/assets"
MANIFEST = ASSETS / "art/manifest.json"
FUENTE = "tiles/water"
DESTINO = "tiles/water_nieve"
PNG_REL = "art/gen/agua_nieve.png"


def lum(c):
    return 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]


def es_agua(c):
    r, g, b = c
    return b > r + 40 and b >= g - 20


def es_pasto(c):
    r, g, b = c
    return g > r + 15 and g > b


def main():
    man = json.loads(MANIFEST.read_text())
    src = man["sprites"][FUENTE]
    hoja = np.array(Image.open(ASSETS / src["png"]).convert("RGBA"))
    grid = src.get("grid") or [32, 32]
    # Las celdas de todas las poses (cada una es una tira animada hacia la derecha).
    celdas = set()
    alto = 0
    for pose in src["poses"].values():
        r = pose["rect"]
        frames = pose.get("anim", {}).get("frames", 1)
        for f in range(frames * r["w"]):
            for h in range(r["h"]):
                celdas.add((r["col"] + f, r["row"] + h))
        alto = max(alto, r["row"] + r["h"])
    ancho = max(c for c, _ in celdas) + 1
    salida = np.zeros((alto * grid[1], ancho * grid[0], 4), dtype=np.uint8)

    # La paleta de destino: el filo de nieve del kit que ya dibuja el juego.
    kit = man["sprites"]["cliff/nieve"]
    acant = np.array(Image.open(ASSETS / kit["png"]).convert("RGBA"))
    nieve = set()
    for p in ("edge_n", "labio"):
        r = kit["poses"][p]["rect"]
        t = acant[r["row"] * 32:(r["row"] + r["h"]) * 32, r["col"] * 32:(r["col"] + r["w"]) * 32]
        nieve |= {tuple(int(x) for x in q[:3]) for q in t.reshape(-1, 4) if q[3]}
    nieve = sorted(nieve, key=lum, reverse=True)  # del más claro al más oscuro
    cara = nieve[:4]      # la nieve y su sombra suave
    filo = nieve[4:]      # el filo gris azulado

    # Los colores de la ribera, medidos en las celdas de las poses.
    ribera = set()
    for (c, r) in celdas:
        t = hoja[r * grid[1]:(r + 1) * grid[1], c * grid[0]:(c + 1) * grid[0]]
        ribera |= {tuple(int(x) for x in q[:3]) for q in t.reshape(-1, 4) if q[3] and not es_agua(tuple(int(x) for x in q[:3]))}
    pasto = sorted((c for c in ribera if es_pasto(c)), key=lum, reverse=True)
    barro = sorted((c for c in ribera if not es_pasto(c)), key=lum, reverse=True)

    def repartir(origen, destino):
        # Mismo orden de luz: el más claro al más claro, el más oscuro al más
        # oscuro, y el medio repartido en proporción.
        if len(origen) == 1:
            return {origen[0]: destino[0]}
        return {c: destino[round(i * (len(destino) - 1) / (len(origen) - 1))] for i, c in enumerate(origen)}

    # **El pasto dominante es el SUELO**, y tiene que salir del blanco exacto de
    # la nieve de al lado o la ribera se lee como otra losa. Él y los más claros
    # (brizna y reflejo) van al blanco; los más oscuros, a la sombra suave.
    frec = {}
    for (c, r) in celdas:
        t = hoja[r * grid[1]:(r + 1) * grid[1], c * grid[0]:(c + 1) * grid[0]]
        for q in t.reshape(-1, 4):
            k = (int(q[0]), int(q[1]), int(q[2]))
            if q[3] and k in pasto:
                frec[k] = frec.get(k, 0) + 1
    suelo = max(frec, key=frec.get)
    mapa = {c: cara[0] for c in pasto if lum(c) >= lum(suelo)}
    mapa.update(repartir([c for c in pasto if lum(c) < lum(suelo)], cara[1:]))
    mapa.update(repartir(barro, filo))
    for c, d in sorted(mapa.items(), key=lambda kv: -lum(kv[0])):
        print(f"  {c} → {d}")

    for (c, r) in celdas:
        ys, xs = slice(r * grid[1], (r + 1) * grid[1]), slice(c * grid[0], (c + 1) * grid[0])
        t = hoja[ys, xs].copy()
        for y in range(t.shape[0]):
            for x in range(t.shape[1]):
                q = t[y, x]
                if q[3] == 0:
                    continue
                k = (int(q[0]), int(q[1]), int(q[2]))
                if k in mapa:
                    t[y, x, :3] = mapa[k]
        salida[ys, xs] = t

    dest = ASSETS / PNG_REL
    dest.parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray(salida).save(dest, optimize=True)
    nuevo = copy.deepcopy(src)
    nuevo["png"] = PNG_REL
    man["sprites"][DESTINO] = nuevo
    MANIFEST.write_text(json.dumps(man, ensure_ascii=False, indent=1))
    print(f"{PNG_REL}: {len(celdas)} celdas · {len(mapa)} tonos de ribera → nieve · slot {DESTINO}")


if __name__ == "__main__":
    main()
