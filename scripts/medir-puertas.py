#!/usr/bin/env python3
"""**¿Qué edificio trae la puerta dibujada, cuál trae el hueco, y de quién es cada puerta?**

    python3 scripts/medir-puertas.py            # informe + scripts/puertas-index.json

Pedido del usuario (28 sep 2026): clasificar todas las casas, búnkeres,
edificios —todo lo que el juego usa— en las que **ya traen la puerta dibujada**
y las que **traen el espacio vacío** porque el pack dibuja la puerta aparte (la
puerta tiene animación), y decir **a qué edificio pertenece cada puerta**. Es el
paso previo a que todas las puertas del juego se abran y se cierren al
interactuar.

Sólo MIDE; no toca el manifest. Todo sale de píxeles:

- **las puertas**: cada hoja animada del pack cuyo nombre dice puerta, portón,
  persiana o trampilla (`Door`, `Garage`, `Gate`, `Shutter`, `Trapdoor`). El
  ancho del cuadro se deduce por continuidad de la animación
  (`import-casas.py::ancho_de_cuadro`) y el cuadro 0 es la puerta CERRADA.
- **los edificios en uso**: toda clave `building/…` o `edificio/…` que el código
  nombra (`ha-shared`, `ha-game`), con sus variantes del manifest, recortada de
  su página tal como la dibuja el juego (el rect o cada pose).
- **puerta incluida**: los píxeles opacos del cuadro cerrado calzan EXACTO dentro
  del dibujo del edificio (`buscar_opaco`). Se anota la hoja y el píxel.
- **hueco para puerta**: un rectángulo totalmente transparente del tamaño de la
  puerta (su caja opaca), abierto hacia abajo y con pared a los dos costados y
  arriba, en la mitad de abajo del dibujo. Se anota qué puertas caben.
"""

import importlib.util
import json
import os
import re
import subprocess
import sys
from collections import defaultdict
from multiprocessing import Pool

import numpy as np
from PIL import Image

RAIZ = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ASSETS = os.path.join(RAIZ, "native/crates/ha-game/assets")
MANIFEST = os.path.join(ASSETS, "art/manifest.json")
ANIMADAS = os.path.join(
    RAIZ, "PRUEBAS-CODIGOS/modernexteriors-win/Modern_Exteriors_32x32/Animated_32x32/Animated_sheets_32x32"
)
SALIDA = os.path.join(RAIZ, "scripts/puertas-index.json")
CELL = 32
OPACO = 16
ES_PUERTA = re.compile(r"door|garage|gate|shutter|trapdoor", re.I)
# Una hoja de vehículo o de vagón no es de un edificio.
NO_EDIFICIO = re.compile(r"bus_|subway|truck|ambulance|helicopter|trailer_door|rv_door", re.I)

_spec = importlib.util.spec_from_file_location("casas", os.path.join(RAIZ, "scripts/import-casas.py"))
casas = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(casas)


def png(ruta):
    return np.array(Image.open(ruta).convert("RGBA"))


def hojas_de_puerta():
    out = []
    for f in sorted(os.listdir(ANIMADAS)):
        if not f.lower().endswith(".png") or not ES_PUERTA.search(f) or NO_EDIFICIO.search(f):
            continue
        arr = png(os.path.join(ANIMADAS, f))
        ancho = casas.ancho_de_cuadro(arr)
        if ancho is None:
            # Una hoja que no se divide: un solo cuadro (o de un ancho raro).
            ancho = arr.shape[1] // CELL
        c0 = arr[:, : ancho * CELL]
        op = c0[:, :, 3] > OPACO
        if not op.any():
            continue
        ys, xs = np.where(op)
        caja = (int(xs.min()), int(ys.min()), int(xs.max() - xs.min() + 1), int(ys.max() - ys.min() + 1))
        n = arr.shape[1] // (ancho * CELL)
        out.append({"hoja": f[:-4], "cuadro_celdas": [ancho, arr.shape[0] // CELL], "cuadros": n,
                    "caja": caja, "c0": c0})
    return out


def claves_en_uso():
    crates = os.path.join(RAIZ, "native/crates")
    txt = subprocess.run(
        ["grep", "-rhoE", r'"(building|edificio)/[a-z0-9_]+"', os.path.join(crates, "ha-shared/src"),
         os.path.join(crates, "ha-game/src")],
        capture_output=True, text=True).stdout
    return sorted({l.strip('"') for l in txt.split()})


def dibujos(man, claves):
    """`[(clave_sprite, pose|None, array)]` de todo lo que el juego puede dibujar."""
    paginas = {}
    out = []

    def recorte(png_rel, rect, grid):
        if png_rel not in paginas:
            ruta = os.path.join(ASSETS, png_rel)
            paginas[png_rel] = png(ruta) if os.path.exists(ruta) else None
        p = paginas[png_rel]
        if p is None:
            return None
        gw, gh = grid
        x0, y0 = rect["col"] * gw, rect["row"] * gh
        return p[y0: y0 + rect["h"] * gh, x0: x0 + rect["w"] * gw]

    for base in claves:
        vs = [base] + [f"{base}#{v['id']}" for v in man["objects"].get(base, {}).get("variants", [])]
        for k in vs:
            e = man["sprites"].get(k)
            if not e or not e.get("png"):
                continue
            grid = e.get("grid") or [CELL, CELL]
            if "rect" in e:
                a = recorte(e["png"], e["rect"], grid)
                if a is not None and a.size:
                    out.append((k, None, a))
            elif "poses" in e:
                for pn, p in e["poses"].items():
                    if "rect" in p and not pn.startswith("puerta_en"):
                        a = recorte(e["png"], p["rect"], p.get("grid") or grid)
                        if a is not None and a.size:
                            out.append((k, pn, a))
            else:
                ruta = os.path.join(ASSETS, e["png"])
                if os.path.exists(ruta):
                    out.append((k, None, png(ruta)))
    return out


TOL = 24          # suma de |ΔRGB| con la que dos píxeles cuentan como iguales
DIBUJADA = 0.75   # fracción de la puerta cerrada que tiene que coincidir
HUECO = 0.30      # fracción de la puerta que tiene que caer en aire


def agujas(d, n=6, largo=12):
    """Tramos opacos cortos de filas repartidas: la puerta del edificio casi
    nunca es IDÉNTICA al cuadro cerrado de la hoja (un reflejo, un pixel de
    sombra), así que una sola aguja larga no alcanza. Con varias, alguna cae en
    una fila que sí coincide."""
    op = d[:, :, 3] > OPACO
    out = []
    filas = [y for y in range(d.shape[0]) if op[y].sum() >= largo]
    if not filas:
        return out
    for y in filas[:: max(1, len(filas) // n)][:n]:
        xs = np.where(op[y])[0]
        for x0 in xs:
            if op[y, x0: x0 + largo].all() and x0 + largo <= d.shape[1]:
                out.append((y, int(x0), d[y, x0: x0 + largo].tobytes()))
                break
    return out


def puntuar(b, d, op, x, y):
    h, w = d.shape[:2]
    sub = b[y: y + h, x: x + w]
    if sub.shape[:2] != (h, w):
        return 0.0, 0.0
    sop = sub[:, :, 3] > OPACO
    igual = (np.abs(sub[:, :, :3].astype(int) - d[:, :, :3].astype(int)).sum(2) <= TOL) & op & sop
    n = op.sum()
    return igual.sum() / n, (op & ~sop).sum() / n


def mejor_encaje(b, d, ag, cb=None):
    """`(fraccion_igual, fraccion_aire, (x, y))` del mejor lugar para la puerta.

    Los candidatos salen de dos lados: las agujas (tramos de fila exactos) y los
    **colores más raros** de la puerta dentro del edificio — cada píxel del
    edificio con ese color propone la posición que lo pondría en su sitio. Lo
    segundo no depende de que una fila entera coincida, que es justo lo que
    falla cuando la puerta dibujada difiere en un reflejo del cuadro cerrado
    (medido en la casa moderna: 90 % igual por fuerza bruta, 0 candidatos por
    agujas)."""
    op = d[:, :, 3] > OPACO
    H, W = b.shape[:2]
    h, w = d.shape[:2]
    if h > H or w > W:
        return 0.0, 0.0, None
    cand = set()
    plano = b.tobytes()
    for (dy, dx, aguja) in ag:
        i = plano.find(aguja)
        n = 0
        while i >= 0 and n < 300:
            if i % 4 == 0:
                py, px = divmod(i // 4, W)
                cand.add((px - dx, py - dy))
                n += 1
            i = plano.find(aguja, i + 1)
    if cb is not None:
        cd = mapa_de_colores(d)
        valores, cuenta_b = np.unique(cb[cb > 0], return_counts=True)
        raros = []
        for v in np.unique(cd[op]):
            j = np.searchsorted(valores, v)
            if j < len(valores) and valores[j] == v:
                raros.append((int(cuenta_b[j]), int(v)))
        raros.sort()
        for _, v in raros[:3]:
            ys_d, xs_d = np.where(cd == v)
            ys_b, xs_b = np.where(cb == v)
            if len(ys_b) > 600:
                continue
            for k in range(min(2, len(ys_d))):
                for yb, xb in zip(ys_b, xs_b):
                    cand.add((int(xb - xs_d[k]), int(yb - ys_d[k])))
    mejor = (0.0, 0.0, None)
    for (x, y) in cand:
        if 0 <= x <= W - w and 0 <= y <= H - h:
            f, aire = puntuar(b, d, op, x, y)
            if f > mejor[0]:
                mejor = (f, aire, (int(x), int(y)))
    return mejor


def mapa_de_colores(a):
    """Cada píxel opaco como entero RGB; 0 en el aire."""
    op = a[:, :, 3] > OPACO
    out = np.zeros(a.shape[:2], np.uint32)
    rgb = a[:, :, :3].astype(np.uint32)
    out[op] = ((rgb[:, :, 0] << 16) | (rgb[:, :, 1] << 8) | rgb[:, :, 2])[op] | 0x1000000
    return out


def con_pared(a, d, xy):
    """Un HUECO de puerta es aire con pared a los dos costados y arriba, y que
    llega al piso del edificio. Sin esto contaban como hueco el aire debajo de
    un cartel, entre las orugas de un tanque o dentro de un invernadero."""
    x, y = xy
    h, w = d.shape[:2]
    op = a[:, :, 3] > OPACO
    H, W = op.shape
    if x < 1 or y < 1 or x + w >= W:
        return False
    aire = (d[:, :, 3] > OPACO) & ~op[y: y + h, x: x + w]
    ys, xs = np.where(aire)
    if not len(ys):
        return False
    x0, x1, y0, y1 = x + xs.min(), x + xs.max(), y + ys.min(), y + ys.max()
    izq = op[y0: y1 + 1, max(0, x0 - 2)].mean()
    der = op[y0: y1 + 1, min(W - 1, x1 + 2)].mean()
    arriba = op[max(0, y0 - 2), x0: x1 + 1].mean()
    piso = np.where(op[:, x0: x1 + 1].any(1))[0].max()
    # Pared de UN costado alcanza: en la casa en hilera el hueco está en la
    # unión con el módulo vecino, que es el que pone el otro lado.
    return (izq > 0.7 or der > 0.7) and arriba > 0.7 and piso - y1 <= 8


#: El tema del pack de cada hoja de puerta, por el comienzo de su nombre, y la
#: página del atlas donde el importador masivo dejó ese tema.
TEMAS = [
    ("additional_houses", "additional_houses"),
    ("garden_villa", "villas"),
    ("condo", "condominio"),
    ("floor_modular", "modulares"),
    ("metropolis", "metropolis"),
    ("police", "police"),
    ("post_office", "post"),
    ("military", "military"),
    ("23_military", "military"),
    ("hospital", "hotel_and_hospital"),
    ("school", "school"),
    ("shopping", "shopping"),
    ("market", "shopping"),
    ("gas_station", "gas"),
    ("power_house", "power"),
    ("garden_tool", "garden"),
    ("haunted", "haunted"),
    ("office", "office"),
    ("worksite", "worksite"),
    ("watchtower", "military"),
]


def tema_de_puerta(hoja):
    n = hoja.lower()
    for pref, tema in TEMAS:
        if n.startswith(pref):
            return tema
    return None


def tema_de_pagina(man, k):
    e = man["sprites"].get(k, {})
    return os.path.basename(e.get("png", "")).lower()


def colores(arr):
    """Los colores opacos de un dibujo, empaquetados a un entero por píxel."""
    op = arr[:, :, 3] > OPACO
    rgb = arr[:, :, :3][op].astype(np.uint32)
    return (rgb[:, 0] << 16) | (rgb[:, 1] << 8) | rgb[:, 2]


def medir(args):
    k, pose, a, puertas, _tema = args
    dibujadas, huecos = [], []
    paleta_b = np.unique(colores(a))
    cb = mapa_de_colores(a)
    for p in puertas:
        # **Descarte barato por paleta.** Una puerta de otro edificio trae otros
        # colores: si menos de la mitad de sus píxeles tiene un color que el
        # dibujo usa, no puede estar dibujada ahí. (Un hueco sí puede estar en
        # un edificio de otros colores, pero la hoja trae su marco de pared, y
        # ese marco es del edificio.)
        if np.isin(p["col"], paleta_b).mean() < 0.7:
            continue
        f, aire, xy = mejor_encaje(a, p["c0"], p["agujas"], cb)
        if xy is None:
            continue
        if f >= DIBUJADA:
            dibujadas.append((p["hoja"], list(xy), round(float(f), 3)))

    dibujadas.sort(key=lambda t: -t[2])
    if not dibujadas:
        m = muesca(a)
        if m:
            x, y, w, h = m
            for p in puertas:
                if not p["tema"] or p["tema"] not in args[4]:
                    continue
                pw, ph = p["hoja_px"]
                if 0.6 * w <= pw <= 1.8 * w and 0.6 * h <= ph <= 2.2 * h:
                    huecos.append((p["hoja"], [x, y], round(pw / w, 2)))
            if not huecos:
                huecos.append(("(hueco a revisar: ninguna puerta del tema entra)", [x, y], 0.0))
    return k, pose, dibujadas[:3], huecos[:6]


def muesca(a):
    """`(x, y, w, h)` del hueco de puerta al pie del edificio, o `None`.

    Es aire con dintel arriba y pared al menos de un costado (en la casa en
    hilera el hueco está en la unión con el módulo vecino), que llega hasta el
    piso del dibujo y mide lo que mide una puerta: de 24 a 110 px de ancho y de
    28 a 110 de alto. Se recorre columna por columna desde el piso: una columna
    de hueco es aire en sus últimas filas y pared encima."""
    op = a[:, :, 3] > OPACO
    H, W = op.shape
    if H < 64 or W < 48:
        return None
    filas = np.where(op.any(1))[0]
    piso = filas.max()
    # alto del aire en cada columna, contando desde el piso hacia arriba, y si
    # arriba de ese aire hay pared
    alto = np.zeros(W, int)
    for x in range(W):
        col = op[: piso + 1, x]
        if not col.any():
            continue
        y = piso
        while y >= 0 and not col[y]:
            y -= 1
        if y < 0:
            continue
        alto[x] = piso - y
    # corridas de columnas con aire de altura parecida y pared encima
    mejor = None
    x = 0
    while x < W:
        if 28 <= alto[x] <= 110:
            x0 = x
            h0 = alto[x]
            while x < W and abs(alto[x] - h0) <= 4 and alto[x] >= 28:
                x += 1
            w = x - x0
            if 24 <= w <= 110:
                lados = (x0 > 0 and alto[x0 - 1] < h0 - 8) + (x < W and alto[min(x, W - 1)] < h0 - 8)
                if lados >= 1:
                    c = (x0, int(piso - h0 + 1), w, int(h0))
                    if mejor is None or w * h0 > mejor[2] * mejor[3]:
                        mejor = c
        else:
            x += 1
    return mejor


def main():
    man = json.load(open(MANIFEST))
    puertas = hojas_de_puerta()
    print(f"{len(puertas)} hojas de puerta/portón/persiana/trampilla de edificio en el pack")
    claves = claves_en_uso()
    todos = dibujos(man, claves)
    print(f"{len(claves)} claves de edificio en el código → {len(todos)} dibujos")
    livianas = []
    for p in puertas:
        x0, y0, w, h = p["caja"]
        c0 = np.ascontiguousarray(p["c0"][y0: y0 + h, x0: x0 + w])
        livianas.append({"hoja": p["hoja"], "c0": c0, "agujas": agujas(c0), "col": colores(c0),
                         "tema": tema_de_puerta(p["hoja"]), "hoja_px": (w, h)})
    # Muchas variantes son el MISMO dibujo (la misma casa con otro nombre): se
    # mide una vez por imagen.
    import hashlib
    unicos, de_quien = {}, []
    tema_de = {}
    for k, pose, a in todos:
        h = hashlib.sha1(a.tobytes() + bytes(str(a.shape), "ascii")).hexdigest()
        unicos.setdefault(h, a)
        tema_de.setdefault(h, tema_de_pagina(man, k))
        de_quien.append((k, pose, h))
    print(f"{len(unicos)} dibujos distintos")
    orden = list(unicos)
    with Pool() as pool:
        medidos = pool.map(medir, [(h, None, unicos[h], livianas, tema_de[h]) for h in orden], chunksize=4)
    por_hash = {h: (inc, hs) for (h, _, inc, hs) in medidos}
    res = [(k, pose, *por_hash[h]) for (k, pose, h) in de_quien]
    por_edificio = {}
    de_puerta = defaultdict(list)
    for k, pose, inc, hs in res:
        nombre = k if pose is None else f"{k}@{pose}"
        if inc:
            estado = "con_puerta"
        elif hs:
            estado = "con_hueco"
        else:
            estado = "sin_puerta_detectada"
        por_edificio[nombre] = {"estado": estado, "incluidas": inc, "huecos": hs}
        for hoja, xy, f in inc:
            de_puerta[hoja].append({"edificio": nombre, "como": "dibujada", "xy": xy, "igual": f})
        for hoja, xy, f in hs:
            de_puerta[hoja].append({"edificio": nombre, "como": "hueco", "xy": xy, "aire": f})
    resumen = defaultdict(int)
    for v in por_edificio.values():
        resumen[v["estado"]] += 1
    json.dump({
        "resumen": resumen,
        "puertas": {p["hoja"]: {"cuadro_celdas": p["cuadro_celdas"], "cuadros": p["cuadros"],
                                "caja_px": p["caja"], "edificios": de_puerta.get(p["hoja"], [])}
                    for p in puertas},
        "edificios": por_edificio,
    }, open(SALIDA, "w"), indent=1, ensure_ascii=False)
    print(dict(resumen))
    sin_casa = [p["hoja"] for p in puertas if not de_puerta.get(p["hoja"])]
    print(f"puertas con casa: {len(puertas) - len(sin_casa)} · sin casa en uso: {len(sin_casa)}")
    print(f"→ {os.path.relpath(SALIDA, RAIZ)}")


if __name__ == "__main__":
    main()
