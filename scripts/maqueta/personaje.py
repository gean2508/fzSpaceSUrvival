"""Compone personajes con las capas reales del chargen del juego.

Orden de apilado y direcciones salen de `character_create.rs`:
`ROWS = [Body, Eyes, Outfit, Hair, Accessory]` y `SHOWCASE` dice que la celda 3
es DE FRENTE (el orden de la hoja es R, U, L, D).
"""

import json
from pathlib import Path

from PIL import Image

ASSETS = Path("/home/fzion/Documents/mods/phaser-arena/native/crates/ha-game/assets")
IDX = json.loads((ASSETS / "art/chargen/index.json").read_text())
CELDA = tuple(IDX["cell"])          # 32x64
FRENTE, DERECHA, ESPALDAS, IZQUIERDA = 3, 0, 1, 2

_cache = {}


def _hoja(rel):
    if rel not in _cache:
        _cache[rel] = Image.open(ASSETS / rel).convert("RGBA")
    return _cache[rel]


def capas(look):
    """look = dict(body, eyes, outfit, hair, accessory|None) -> rutas en orden."""
    out = [IDX["bodies"][look["body"]]["png"],
           IDX["eyes"][look["eyes"]]["png"],
           IDX["outfits"][look["outfit"]]["png"],
           IDX["hair"][look["hair"]]["png"]]
    if look.get("accessory") is not None:
        out.append(IDX["accessories"][look["accessory"]]["png"])
    return out


def personaje(look, celda=FRENTE, fila=0, escala=1):
    w, h = CELDA
    fr = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    for rel in capas(look):
        hoja = _hoja(rel)
        fr.alpha_composite(hoja.crop((celda * w, fila * h, celda * w + w, fila * h + h)))
    if escala != 1:
        fr = fr.resize((w * escala, h * escala), Image.NEAREST)
    return fr


def retrato(capa, idx, escala=1):
    """Miniatura del pack para la grilla del creador (64x64). None si no hay."""
    tabla = {"piel": "bodies", "ojos": "eyes", "pelo": "hair", "accesorio": "accessories"}[capa]
    rel = IDX[tabla][idx].get("portrait")
    if not rel:
        return None
    im = _hoja(rel)
    pw, ph = IDX["portrait_cell"]
    im = im.crop((0, 0, pw, ph))
    if escala != 1:
        im = im.resize((pw * escala, ph * escala), Image.NEAREST)
    return im


def retrato_compuesto(look, capa=None, idx=None, escala=1):
    """Cara para la grilla del creador: piel + ojos + pelo, apilados igual que el
    cuerpo. Un retrato de pelo solo es una peluca flotando."""
    l = dict(look)
    if capa and idx is not None:
        l[{"piel": "body", "ojos": "eyes", "pelo": "hair",
           "accesorio": "accessory"}[capa]] = idx
    pw, ph = IDX["portrait_cell"]
    fr = Image.new("RGBA", (pw, ph), (0, 0, 0, 0))
    for c, i in [("piel", l["body"]), ("ojos", l["eyes"]), ("pelo", l["hair"]),
                 ("accesorio", l.get("accessory"))]:
        if i is None:
            continue
        r = retrato(c, i)
        if r is not None:
            fr.alpha_composite(r)
    if escala != 1:
        fr = fr.resize((pw * escala, ph * escala), Image.NEAREST)
    return fr


def etiqueta(capa, idx):
    tabla = {"piel": "bodies", "ojos": "eyes", "ropa": "outfits",
             "pelo": "hair", "accesorio": "accessories"}[capa]
    return IDX[tabla][idx]["label"]


def cuantos(capa):
    tabla = {"piel": "bodies", "ojos": "eyes", "ropa": "outfits",
             "pelo": "hair", "accesorio": "accessories"}[capa]
    return len(IDX[tabla])


# ── estilos y colores ───────────────────────────────────────────────────────
# El chargen no es una lista plana: cada capa son ESTILOS (el corte de pelo, la
# prenda) por COLORES. `character_create.rs` ya lo modela así — `cycle_style` y
# `cycle_color` son dos ejes distintos — y la pantalla tiene que mostrarlo igual:
# grilla de estilos arriba, muestras de color abajo.
_TABLA = {"piel": "bodies", "ojos": "eyes", "ropa": "outfits",
          "pelo": "hair", "accesorio": "accessories"}


def estilos(capa):
    """[(style, {color: idx})] en el orden del índice."""
    out, vistos = [], {}
    for i, e in enumerate(IDX[_TABLA[capa]]):
        s = e["style"]
        if s not in vistos:
            vistos[s] = {}
            out.append((s, vistos[s]))
        vistos[s][e["color"]] = i
    return out


def colores(capa):
    """Los colores que existen en esa capa, ordenados."""
    return sorted({e["color"] for e in IDX[_TABLA[capa]]})


def indice(capa, style, color):
    """El índice plano de (estilo, color), cayendo al primer color que exista."""
    for s, mapa in estilos(capa):
        if s == style:
            return mapa.get(color, next(iter(mapa.values())))
    return 0


def color_medio(capa, idx):
    """Color representativo de una variante — se MIDE del pixel, no se elige."""
    rel = IDX[_TABLA[capa]][idx].get("portrait")
    im = _hoja(rel) if rel else _hoja(IDX[_TABLA[capa]][idx]["png"])
    if rel:
        pw, ph = IDX["portrait_cell"]
        im = im.crop((0, 0, pw, ph))
    else:
        w, h = CELDA
        im = im.crop((FRENTE * w, 0, FRENTE * w + w, h))
    px = [p for p in im.getdata() if p[3] > 200]
    if not px:
        return (150, 130, 120)
    # la mediana por canal aguanta mejor los brillos sueltos que el promedio
    px.sort(key=lambda p: p[0] * 2 + p[1] * 3 + p[2])
    return px[len(px) * 3 // 5][:3]
