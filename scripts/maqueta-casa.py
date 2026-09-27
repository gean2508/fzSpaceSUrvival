#!/usr/bin/env python3
"""**Dibuja una casa generada usando el manifest, sin compilar el juego.**

    python3 scripts/maqueta-casa.py --estilo 7 --out /tmp/casa.png

Es el banco de pruebas de `world2d::arte_de_interior`: arma la planta de una casa
igual que `ha_shared::city` (perímetro + tabiques + vano) y la pinta con los
MISMOS rects del manifest y la MISMA regla de apoyo que usa el chunk —una celda
del pack es un tile, el sprite se apoya en el pie y crece hacia arriba.

No reemplaza a la captura del juego: reemplaza a *adivinar*. Un recorte mal
elegido se ve acá en un segundo y no en veinte minutos de `cargo`.
"""

import argparse
import json
import os

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ASSETS = os.path.join(ROOT, "native/crates/ha-game/assets")
MANIFEST = os.path.join(ASSETS, "art/manifest.json")
CELL = 32

_hojas = {}


def hoja(rel):
    if rel not in _hojas:
        _hojas[rel] = Image.open(os.path.join(ASSETS, rel)).convert("RGBA")
    return _hojas[rel]


def recorte(sprites, key, pose=None):
    """El PNG de `key` en la pose pedida, con la cadena de respaldo del juego."""
    e = sprites.get(key)
    if e is None:
        return None
    d = e
    if pose:
        p = (e.get("poses") or {}).get(pose)
        if p is not None:
            d = {**e, **p}
    r = d.get("rect")
    if not r:
        return None
    png = d.get("png", e.get("png"))
    return hoja(png).crop((r["col"] * CELL, r["row"] * CELL,
                           (r["col"] + r["w"]) * CELL, (r["row"] + r["h"]) * CELL))


def estilos(sprites):
    """Lo mismo que `ha_assets::interior_styles()`, leído del manifest."""
    out = []
    for k in sorted(sprites):
        suf = k[len("wall/int_"):] if k.startswith("wall/int_") else None
        if suf is None:
            continue
        if f"doorway/int_{suf}" in sprites and f"floor/int_{suf}" in sprites:
            out.append({"pared": k, "vano": f"doorway/int_{suf}", "piso": f"floor/int_{suf}"})
    return out


# ---------------------------------------------------------------------------
# La planta, como la arma `ha_shared::city`
# ---------------------------------------------------------------------------

def planta(w, d, tab_x, tab_z):
    """`{(x,z): tile}` con perímetro, puerta al sur y dos tabiques con su vano."""
    m = {}
    for z in range(d):
        for x in range(w):
            borde = x == 0 or x == w - 1 or z == 0 or z == d - 1
            m[(x, z)] = "wall" if borde else "floor"
    m[(w // 2, d - 1)] = "door"                      # la puerta a la calle
    for z in range(1, d - 1):                        # tabique NORTE-SUR
        m[(tab_x, z)] = "door" if z == tab_z else "wall"
    for x in range(1, w - 1):                        # tabique ESTE-OESTE
        if x != tab_x:
            m[(x, tab_z)] = "wall" if x != tab_x + 2 else "door"
    return m


def cara_actual(m, x, z, w, d, tile):
    """La regla vieja: la cara sale del BORDE del rect."""
    perimetro = z == 0 or z == d - 1 or x == 0 or x == w - 1
    if tile not in ("wall",):
        return "center"
    if z == 0:
        return "front"
    if perimetro:
        return "back"
    return "front"


def cara_por_cuarto(m, x, z, w, d, tile):
    """La regla nueva: la pieza del kit sale de **dónde está el cuarto**.

    `Room_Builder_3d_walls` trae el cuarto entero en 3/4 —muro horizontal con su
    tapa, verticales vistos de canto y las esquinas en ángulo—, así que lo que hay
    que contestar no es "¿en qué borde del rect cae esta celda?" sino "¿para dónde
    le queda el piso?". Un tabique y una pared de borde son la misma pieza mirada
    desde el mismo lado.
    """
    piso = lambda dx, dz: m.get((x + dx, z + dz)) in ("floor", "door")
    vano = lambda dx, dz: m.get((x + dx, z + dz)) == "door"
    s_, e_, w_, n_ = piso(0, 1), piso(1, 0), piso(-1, 0), piso(0, -1)
    if s_ or n_:
        # Muro horizontal. Si al lado se abre un vano, remata con su JAMBA: en
        # este kit una puerta es un hueco por el que se ve el piso, y lo que la
        # dibuja son los dos muros que la flanquean.
        if vano(1, 0):
            return "end_e"
        if vano(-1, 0):
            return "end_w"
    if s_:
        return "edge_n"
    if e_:
        return "edge_w"
    if w_:
        return "edge_e"
    if piso(1, 1):
        return "corner_nw"
    if piso(-1, 1):
        return "corner_ne"
    # El cuarto le queda al NORTE: es la pared que da a la calle. De ella se ve la
    # tapa y nada más, y lo que mira al frente es la FACHADA.
    if n_:
        return "edge_s"
    return "edge_n"


def render(sprites, est, w, d, tab_x, tab_z, regla):
    m = planta(w, d, tab_x, tab_z)
    img = Image.new("RGBA", ((w + 2) * CELL, (d + 4) * CELL), (24, 24, 32, 255))
    ox, oy = CELL, 3 * CELL

    def poner(key, pose, x, z):
        im = recorte(sprites, key, pose)
        if im is None:
            return
        h = im.size[1] // CELL
        img.alpha_composite(im, (ox + x * CELL, oy + (z + 1 - h) * CELL))

    # 1. el piso, siempre debajo
    for (x, z), t in m.items():
        if t in ("floor", "door"):
            poner(est["piso"], "center", x, z)
    # 2. lo que se levanta, de norte a sur (y-sort)
    for z in range(d):
        for x in range(w):
            t = m[(x, z)]
            if t == "floor":
                continue
            perimetro = z == 0 or z == d - 1 or x == 0 or x == w - 1
            if t == "door":
                continue   # un vano es un HUECO: se ve el piso y lo rematan las jambas
            key, pose = regla(m, x, z, w, d, t, est)
            if key and pose:
                poner(key, pose, x, z)
    return img


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--estilo", type=int, default=0)
    ap.add_argument("--out", default="/tmp/casa.png")
    ap.add_argument("--zoom", type=int, default=3)
    ap.add_argument("--regla", choices=("vieja", "nueva"), default="nueva")
    ap.add_argument("--w", type=int, default=11)
    ap.add_argument("--d", type=int, default=8)
    args = ap.parse_args()

    sprites = json.load(open(MANIFEST))["sprites"]
    est = estilos(sprites)
    print(f"{len(est)} estilos de interior")
    e = est[args.estilo % len(est)]
    print("estilo:", e["pared"])

    caras = {"vieja": cara_actual, "nueva": cara_por_cuarto}[args.regla]
    regla = lambda m, x, z, w, d, t, es: (es["pared"], caras(m, x, z, w, d, t))
    img = render(sprites, e, args.w, args.d, args.w // 2 + 1, args.d // 2, regla)
    img = img.resize((img.size[0] * args.zoom, img.size[1] * args.zoom), Image.NEAREST)
    img.convert("RGB").save(args.out)
    print("→", args.out)


if __name__ == "__main__":
    main()
