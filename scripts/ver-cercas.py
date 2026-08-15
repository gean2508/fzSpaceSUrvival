#!/usr/bin/env python3
"""**Componer una parcela con los recortes de cerca de verdad, sin levantar el juego.**

    cd native && HA_DUMP=/tmp/cercas.tsv cargo test -p ha-shared --test citydump \
        -- --ignored volcar_cercas
    python3 scripts/ver-cercas.py /tmp/cercas.tsv /tmp/cercas.png

## Por qué existe

Un test verde con una captura fea es un fallo, y la valla acostada en los tramos
verticales no la cazaba ningún invariante: había que MIRAR. Levantar el cliente
para eso son quince minutos de build de Bevy y un teléfono; esto pega los mismos
recortes que va a pegar el juego, leyendo el mismo `manifest.json` y las mismas
poses que eligió `ha_shared::cercas::cadena`, y tarda un segundo.

Es la misma idea que el volcado ASCII de `citydump` —mirar la trama con los ojos—
pero con el pixel puesto.

## Lo que NO prueba

El Y-sort, la medida en metros y la banda de render son del cliente; acá cada
celda se pega en su casilla de 32 px. Si el dibujo se ve bien acá y mal en el
juego, el problema está en `world2d`, no en el arte ni en la forma elegida.
"""

import argparse
import json
import os
import sys

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ASSETS = os.path.join(ROOT, "native/crates/ha-game/assets")
MANIFEST = os.path.join(ASSETS, "art/manifest.json")
CELL = 32


def recorte(man, clave, cadena):
    """El PNG de la primera pose curada de la cadena, o `None`.

    **Se recorre la cadena entera**, igual que el juego: la primera versión de
    esto se quedaba con la primera pose y reportaba 96 celdas sin dibujo en el
    lado norte de las parcelas, cuando el juego ahí cae a `ew` y dibuja bien.
    Una herramienta de inspección que miente en el sentido "está roto" es peor
    que no tenerla.
    """
    sp = man.get("sprites", {}).get(clave)
    if not sp:
        return None
    poses = sp.get("poses") or {}
    r = next(((poses[p] or {}).get("rect") for p in cadena.split("|") if p in poses), None)
    # Agotada la cadena, la clave base: es lo que hace el juego
    # (`ArtLibrary::tile_sprite` cae al rect del slot).
    r = r or sp.get("rect")
    if not r:
        return None
    png = os.path.join(ASSETS, sp["png"])
    if not os.path.exists(png):
        return None
    gw, gh = sp.get("grid", [CELL, CELL])
    im = Image.open(png).convert("RGBA")
    return im.crop(
        (r["col"] * gw, r["row"] * gh, (r["col"] + r["w"]) * gw, (r["row"] + r["h"]) * gh)
    )


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("tsv", help="volcado de `citydump::volcar_cercas`")
    ap.add_argument("salida")
    ap.add_argument("--clave", default="fence/madera", help="qué cerca dibujar")
    ap.add_argument("--zoom", type=int, default=2)
    args = ap.parse_args()

    man = json.load(open(MANIFEST))
    filas = [l.rstrip("\n").split("\t") for l in open(args.tsv)][1:]
    celdas = [(int(a), int(b), c, d) for a, b, c, d in filas if a]
    if not celdas:
        sys.exit("el volcado no tiene ni una celda de cerca")

    x0 = min(c[0] for c in celdas)
    z0 = min(c[1] for c in celdas)
    w = max(c[0] for c in celdas) - x0 + 1
    h = max(c[1] for c in celdas) - z0 + 1
    # Fondo verde pasto: una cerca sobre transparente no se lee, y lo que hay que
    # juzgar es justo cómo se recorta contra el jardín.
    lienzo = Image.new("RGBA", (w * CELL, (h + 3) * CELL), (86, 132, 74, 255))

    faltan = {}
    for tx, tz, forma, pose in celdas:
        im = recorte(man, args.clave, pose)
        if im is None:
            faltan[pose] = faltan.get(pose, 0) + 1
            continue
        # El pie de la pieza se apoya en SU celda y lo que sobra crece hacia
        # arriba, igual que en el juego (`world2d`: las cercas son `upright`).
        px = (tx - x0) * CELL
        py = (tz - z0 + 1) * CELL - (im.height - CELL)
        lienzo.alpha_composite(im, (px, max(0, py)))

    if faltan:
        print("poses sin recorte:", faltan)
    z = args.zoom
    lienzo = lienzo.resize((lienzo.width * z, lienzo.height * z), Image.NEAREST)
    lienzo.save(args.salida)
    print(f"{args.salida}: {lienzo.width}x{lienzo.height} px · {len(celdas)} celdas de cerca")


if __name__ == "__main__":
    main()
