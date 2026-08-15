#!/usr/bin/env python3
"""**Las piezas de construcción, con el pixel del pack.**

    python3 scripts/import-construccion.py --dry-run   # sólo informa qué eligió
    python3 scripts/import-construccion.py             # escribe página + manifest

## Por qué faltaban justo éstas

`import-pack-masivo.py` metió los 11.691 singles del pack — están todos. Pero
recorre carpetas `*_Singles_32x32`, y las hojas que NO tienen carpeta de singles
nunca entraron. Ésas son, exactamente, las de construir:

- `Room_Bulder_subfiles_32x32/` (paredes y pisos de Modern Interiors),
- `Theme_Sorter_32x32/17_Visibile_Upstairs_System_32x32.png` (las gradas).

Por eso levantabas una pared y salía un cuadrado marrón: `wall/wood`,
`wall/stone`, `window/basic`, `stairs/up`, `stairs/down`, `floor/stone`,
`ceiling/madera`, `roof/teja` y `roof/chapa` no tenían PNG en el manifest y
caían a `walls::fallback_color` / `ceilings::fallback_color`.

## Qué elige, y cómo

**Midiendo, no a ojo.** La hoja de paredes son bloques de 10×2 celdas repetidos
54 veces (una por estilo); la de pisos, bloques de 3×2. Cuál de esos bloques es
"madera" y cuál "piedra" sale del color medio del bloque en HSV —cálido y
saturado vs. gris desaturado— igual que `import-calles.py` separa el asfalto de
la vereda por brillo. Los rects no se escriben a mano: salen de la geometría de
la hoja, que se verifica antes de cortar (`verificar_hojas`).

## Qué escribe

- `art/pack/construccion.png` — una página de 32 px por celda.
- Las entradas del manifest de los slots de arriba.
- `scripts/construccion-import-index.json` — lo que escribió, para que una
  segunda corrida pise **lo suyo** y nada más (mismo contrato que
  `import-calles.py`). El arte curado a mano en HA Studio no se toca.

`floor/wood` y `door/wood` NO se tocan: ya tienen pixel elegido por el usuario.
"""

import argparse
import colorsys
import json
import os
from glob import glob

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACK = os.path.join(ROOT, "PRUEBAS-CODIGOS")
INT = os.path.join(PACK, "moderninteriors-win (Copy 1)/1_Interiors/32x32")
RB = os.path.join(INT, "Room_Bulder_subfiles_32x32")
HOJA_PAREDES = os.path.join(RB, "Room_Builder_Walls_32x32.png")
HOJA_PISOS = os.path.join(RB, "Room_Builder_Floors_32x32.png")
HOJA_GRADAS = os.path.join(INT, "Theme_Sorter_32x32/17_Visibile_Upstairs_System_32x32.png")
#: **La pared con el hueco de la puerta** — el "marco" que pedía el jugador. Está
#: en el pack desde el primer día y nunca se importó: es la hoja de vanos de
#: Room_Builder, con el mismo catálogo de estilos que la de paredes.
HOJA_VANOS = os.path.join(RB, "Room_Builder_Arched_Entryways_32x32.png")
EXT_SINGLES = os.path.join(
    PACK, "modernexteriors-win/Modern_Exteriors_32x32/Modern_Exteriors_Complete_Singles_32x32"
)

ASSETS = os.path.join(ROOT, "native/crates/ha-game/assets")
MANIFEST = os.path.join(ASSETS, "art/manifest.json")
PAGINA_REL = "art/pack/construccion.png"
INDEX = os.path.join(ROOT, "scripts/construccion-import-index.json")

CELL = 32

# --- Geometría de la hoja de paredes -----------------------------------------
# Tres grupos de 10 columnas separados por una columna vacía; cada estilo ocupa
# 2 filas. Dentro del bloque de 10×2:
#   col 0..3  pared que MIRA a la habitación (con friso arriba y zócalo abajo)
#   col 4..6  la misma pared sin friso ni zócalo (la cara de afuera)
#   col 7..9  copia de 0..3
# `verificar_hojas` comprueba esto antes de cortar nada.
GRUPOS_PARED = (0, 11, 22)
# La celda de arriba del par trae 2 px de línea de hoja, 8 px de TECHO blanco y
# 2 px de sombra; la pared propiamente dicha arranca en y=10 y llega a y=62.
PARED_Y0, PARED_Y1 = 10, 62
# Para CLASIFICAR se mira sólo el centro del paño: el borde de arriba y el
# zócalo de abajo son iguales en todos los estilos, y meterlos en la cuenta le
# regala grano a las paredes lisas (una pared pintada de verde salía "de piedra"
# por su zócalo).
MEDIR_Y0, MEDIR_Y1 = 22, 54

# --- Geometría de la hoja de gradas ------------------------------------------
# Escalera de madera en cols 0..2, filas 18..21 (fila 18 = rellano de arriba);
# escalera de piedra/metal en las filas 22..26. La columna 1 es el tramo de
# escalones puro (los pasamanos caen en las cols 0 y 2).
#
# **Madera para subir, piedra para bajar.** No es capricho: son dos piezas
# distintas del menú y en una celda de 32 px el tramo de escalones se ve igual
# hacia arriba que hacia abajo. El material las separa de un vistazo, y coincide
# con a dónde llevan — la de madera al piso de arriba, la de piedra al sótano
# que cavaste.
GRADA_COL = 1
GRADA_ARRIBA = (GRADA_COL, 19)  # escalera de madera, primer escalón bajo el rellano
GRADA_ABAJO = (GRADA_COL, 24)  # escalera de piedra, medio del tramo

# Baranda de madera del mismo sistema de pisos altos: el tramo corrido (pasamanos
# + balaustres) en la col 10, filas 20-21. Es la pieza que hace que el borde de
# un balcón se lea como un borde y no como el final del dibujo. El dibujo mide 28
# px de alto dentro de ese bloque, así que entra en UNA celda y se pega al pie —
# dibujarlo en dos celdas lo dejaría flotando media celda por encima del borde.
BARANDA = (10, 20, 1, 2)

# --- Geometría de la hoja de VANOS -------------------------------------------
# Bloques de 2×2 celdas (64×64 px), cuatro por banda, en las columnas 0..7. Las
# primeras cuatro filas son los arcos sueltos (sin pared) y no se usan: lo que
# hace falta es la pared CON el hueco, que es lo que se coloca en un borde.
#
# Dentro del bloque, el hueco arranca en `VANO_HUECO_Y` y deja jambas de 12 px a
# cada lado; arriba queda el paño de pared, que es lo que se mide para saber de
# qué estilo es el bloque.
#
# **El bloque arranca con el MISMO friso que la pared** (`PARED_Y0`): 2 px de
# línea de hoja, 8 px de techo blanco y 2 px de sombra. La hoja de vanos es la
# misma familia que la de paredes y se corta igual — recortarla desde y=0 metía
# esos 8 px de blanco puro dentro del sprite, y en el juego el vano aparecía con
# un filo blanco arriba que no es del dibujo, es el techo del cuarto de la hoja.
# Lo cuida `verificar_pagina`.
VANOS_FILA0 = 4
VANOS_COLS = 8
VANO_HUECO_Y = 36
#: Franja del bloque que se mide para clasificar el estilo (pared pura, sin arco).
#: Arranca DESPUÉS del friso: medir el techo blanco aclaraba el promedio de todos
#: los bloques por igual y emparejaba el vano con la pared equivocada.
VANO_MEDIR_Y0, VANO_MEDIR_Y1 = PARED_Y0, 30

# Ancho del paño de vidrio que se pega sobre la pared, y a qué altura del cuerpo
# de la pared va. 20 de 32 deja pared a los dos lados: se lee "pared con
# ventana" y no "un vidrio del tamaño del tile".
ANCHO_VENTANA = 20
ALTO_VENTANA_Y = 8


def hsv(rgb):
    r, g, b = (c / 255.0 for c in rgb)
    return colorsys.rgb_to_hsv(r, g, b)


def medir(im, x0, y0, w, h):
    """Color medio + desviación + opacidad de un rectángulo de píxeles."""
    px = im.load()
    vals, opac = [], 0
    for y in range(y0, y0 + h):
        for x in range(x0, x0 + w):
            p = px[x, y]
            if p[3] > 0:
                opac += 1
                vals.append(p[:3])
    if not vals:
        return None
    n = len(vals)
    medio = tuple(sum(v[i] for v in vals) // n for i in range(3))
    brillo = [sum(v) / 3 for v in vals]
    m = sum(brillo) / n
    desv = (sum((b - m) ** 2 for b in brillo) / n) ** 0.5
    return {"rgb": medio, "hsv": hsv(medio), "desv": desv, "opaco": opac / (w * h)}


def es_madera(m):
    """Marrón/naranja y con veta: el tono manda, la saturación descarta los grises."""
    if m is None:
        return False
    h, s, v = m["hsv"]
    return 0.02 <= h <= 0.13 and s >= 0.30 and 0.35 <= v <= 0.90


def es_piedra_pared(m):
    """Gris de obra: sin color y de tono medio.

    La hoja de Modern Interiors es de INTERIORES: no trae un muro de piedra
    vista, trae paredes. La que más se lee como piedra es la de hormigón gris
    moteado — ni el yeso blanco (demasiado claro) ni un empapelado de color. Por
    eso el criterio es el color y no la textura: el grano de esa pared es de ±2
    de brillo, así que pedirle relieve la descartaría y dejaría entrar un
    empapelado con dibujo, que es peor.
    """
    if m is None:
        return False
    _, s, v = m["hsv"]
    return s <= 0.10 and 0.35 <= v <= 0.70


def es_piedra_piso(m):
    """Baldosa gris: sin color, con junta o veta visible."""
    if m is None:
        return False
    _, s, v = m["hsv"]
    return s <= 0.15 and 0.30 <= v <= 0.80 and m["desv"] >= 3.0


def puntaje_madera(m):
    """Cuanto más saturado y más veteado, más se lee como madera."""
    _, s, _ = m["hsv"]
    return s * 100 + min(m["desv"], 25)


def puntaje_piedra_pared(m):
    """El gris más limpio, y entre grises el más oscuro (el claro es yeso)."""
    _, s, v = m["hsv"]
    return -s * 100 - v * 10


def puntaje_piedra_piso(m):
    """Manda la junta (que se vea la baldosa); el color sólo desempata."""
    _, s, _ = m["hsv"]
    return min(m["desv"], 30) * 2 - s * 100


# ---------------------------------------------------------------------------
# 1. Verificar que las hojas son como este script cree
# ---------------------------------------------------------------------------

def ocupacion(im):
    px = im.load()
    W, H = im.size
    return [
        [
            any(
                px[x, y][3] > 0
                for y in range(r * CELL, r * CELL + CELL, 2)
                for x in range(c * CELL, c * CELL + CELL, 2)
            )
            for c in range(W // CELL)
        ]
        for r in range(H // CELL)
    ]


def verificar_hojas(paredes, pisos, gradas, vanos):
    """Las suposiciones de geometría, comprobadas contra el pixel.

    Si LimeZu reordena una hoja en una versión futura, esto se planta acá en vez
    de escribir rects que apuntan a cualquier cosa.
    """
    oc = ocupacion(paredes)
    assert len(oc[0]) == 32 and len(oc) == 40, f"hoja de paredes {len(oc[0])}x{len(oc)}"
    for gx in GRUPOS_PARED:
        # Fila par: las 10 columnas llenas. Fila impar: 4,6 vacías (son la cara
        # de afuera, que mide una celda de alto).
        assert all(oc[0][gx + c] for c in range(10)), f"grupo {gx} incompleto"
        assert not oc[1][gx + 4] and not oc[1][gx + 6], f"grupo {gx} no tiene la cara de afuera"
        assert oc[1][gx + 5], f"grupo {gx} sin pared llena en la col 5"
    # La franja blanca de techo: la fila 0 de un bloque tiene que ser mucho más
    # clara arriba (y=2..9) que abajo (y=12..30).
    arriba = medir(paredes, (GRUPOS_PARED[0] + 1) * CELL, 2, CELL, 8)
    abajo = medir(paredes, (GRUPOS_PARED[0] + 1) * CELL, 12, CELL, 18)
    assert arriba["hsv"][2] > abajo["hsv"][2] + 0.05, "no encuentro el friso de techo"

    ocp = ocupacion(pisos)
    assert len(ocp[0]) == 15, f"hoja de pisos {len(ocp[0])} columnas"
    for c in (3, 7, 11):
        assert not ocp[10][c], f"la col {c} de pisos deberia ser separador"

    ocg = ocupacion(gradas)
    assert len(ocg[0]) == 16 and len(ocg) == 27, "hoja de gradas cambiada"
    for _, fila in (GRADA_ARRIBA, GRADA_ABAJO):
        assert ocg[fila][GRADA_COL], f"no hay escalones en la fila {fila}"

    # Vanos: bandas de 2 filas con ocho columnas llenas, y el HUECO de verdad
    # abierto (alpha 0) en el medio del bloque. Lo segundo es lo que distingue
    # esta hoja de la de paredes: si el pack la reordenara y dejáramos de cortar
    # el hueco, el "vano" sería una pared más y nadie se enteraría hasta jugar.
    ocv = ocupacion(vanos)
    assert len(ocv[0]) == 10, f"hoja de vanos {len(ocv[0])} columnas"
    assert all(ocv[VANOS_FILA0][c] for c in range(VANOS_COLS)), "banda de vanos incompleta"
    px = vanos.load()
    y = VANOS_FILA0 * CELL + VANO_HUECO_Y + 12
    assert px[32, y][3] == 0, "el vano no tiene hueco en el medio"
    assert px[2, y][3] > 0 and px[62, y][3] > 0, "el vano no tiene jambas"


#: Brillo a partir del cual un píxel cuenta como "blanco de hoja". El friso de
#: techo de Room_Builder es 253 puro; ningún material del pack (ni el yeso, ni la
#: baldosa clara) llega a una FILA ENTERA de ese brillo.
BLANCO = 245


#: Recortes a los que el friso SÍ les corresponde. Hoy ninguno: todo lo que se
#: construye se mira desde afuera. El día que entre arte de INTERIOR —la pared
#: vista desde adentro del cuarto, que es donde el cielorraso se lee— su recorte
#: se anota acá en vez de aflojar la regla para todos.
CON_FRISO = frozenset()


def verificar_pagina(pagina, sitio):
    """**Ningún recorte de EXTERIOR puede llevar el friso puesto de sombrero.**

    Las hojas de Room_Builder dibujan cada pared dentro de un cuarto: la que mira
    a la habitación (cols 0-3) remata arriba con una franja de TECHO BLANCO, y la
    cara de afuera (cols 4-6) no la trae. O sea que el friso no es basura de la
    hoja: es el cielorraso, y **sólo tiene sentido mirando la pared desde
    adentro**.

    Lo que no puede pasar es mezclarlos. La pared ya se recortaba sin friso
    (`PARED_Y0`) y el vano no, así que en un muro construido a campo abierto el
    marco de la puerta salía con un filo blanco y sus vecinas no — se lee como un
    error de carga de textura aunque el pixel esté bien.

    Se cazan filas enteras de blanco opaco en el tercio superior del recorte:
    ancho completo, porque una fila así sólo puede ser el friso (un dibujo con
    blancos los tiene sueltos, no de borde a borde).
    """
    px = pagina.load()
    malos = []
    for nombre, (c, r, w, h) in sitio.items():
        if nombre in CON_FRISO:
            continue
        x0, y0 = c * CELL, r * CELL
        ancho, alto = w * CELL, h * CELL
        for y in range(y0, y0 + max(1, alto // 3)):
            if all(
                px[x, y][3] == 255 and min(px[x, y][:3]) >= BLANCO
                for x in range(x0, x0 + ancho)
            ):
                malos.append((nombre, y - y0))
                break
    if malos:
        detalle = ", ".join(f"{n} (fila {y})" for n, y in malos)
        raise SystemExit(
            "friso de cielorraso sin recortar en: "
            f"{detalle}. Recortá el bloque desde PARED_Y0, como `cara()` — o "
            "anotalo en CON_FRISO si de verdad es arte de interior."
        )


# ---------------------------------------------------------------------------
# 2. Elegir bloques
# ---------------------------------------------------------------------------

def bloques_pared(im):
    """(gx, fila) de cada estilo de pared, con la medida de su cuerpo."""
    oc = ocupacion(im)
    out = []
    for gx in GRUPOS_PARED:
        for ry in range(0, len(oc) - 1, 2):
            if not oc[ry][gx + 1]:
                continue
            m = medir(im, (gx + 1) * CELL, ry * CELL + MEDIR_Y0, CELL, MEDIR_Y1 - MEDIR_Y0)
            out.append((gx, ry, m))
    return out


def bloques_vano(im):
    """(col, fila) de cada pared-con-hueco, con la medida de su paño."""
    oc = ocupacion(im)
    out = []
    for ry in range(VANOS_FILA0, len(oc) - 1, 2):
        for cx in range(0, VANOS_COLS, 2):
            if cx >= len(oc[ry]) or not oc[ry][cx]:
                continue
            m = medir(im, cx * CELL, ry * CELL + VANO_MEDIR_Y0, CELL, VANO_MEDIR_Y1 - VANO_MEDIR_Y0)
            if m is None or m["opaco"] < 0.99:
                continue
            out.append((cx, ry, m))
    return out


def vano_de_la_pared(vanos, pared, que):
    """**El vano que combina con esa pared**: el de color más parecido.

    No se clasifica el vano por su cuenta (¿es madera?, ¿es piedra?) sino que se
    lo empareja con la pared ya elegida. Es la diferencia entre "hay un vano de
    madera" y "el vano ES de la misma madera que tu pared": las dos hojas traen
    el mismo catálogo de estilos, así que emparejar por color acierta el estilo
    exacto sin depender de que los dos catálogos estén en el mismo orden — que es
    justo lo que no se puede dar por sentado entre versiones del pack.
    """
    objetivo = pared[2]["rgb"]
    if not vanos:
        raise SystemExit(f"no encontré ningún bloque de vano para {que}")
    dist = lambda m: sum((m["rgb"][i] - objetivo[i]) ** 2 for i in range(3)) ** 0.5
    return min(vanos, key=lambda b: dist(b[2]))


def bloques_piso(im):
    """(col, fila) de cada estilo de piso, con la medida de su primera celda."""
    oc = ocupacion(im)
    out = []
    for gx in (0, 4, 8, 12):
        for ry in range(0, len(oc)):
            if not oc[ry][gx]:
                continue
            m = medir(im, gx * CELL, ry * CELL, CELL, CELL)
            if m is None or m["opaco"] < 0.99:
                continue  # una baldosa cubre su celda entera
            out.append((gx, ry, m))
    return out


def elegir(candidatos, filtro, puntaje, que):
    ok = [c for c in candidatos if filtro(c[2])]
    if not ok:
        raise SystemExit(f"no encontré ningún bloque de {que} en la hoja")
    return max(ok, key=lambda c: puntaje(c[2]))


def mejor_parche(paths, filtro, puntaje, que):
    """Mejor celda 32×32 OPACA entre varios PNG (para techos: teja y chapa)."""
    mejor = None
    for f in paths:
        im = Image.open(f).convert("RGBA")
        W, H = im.size
        for y in range(0, H - CELL + 1, CELL // 2):
            for x in range(0, W - CELL + 1, CELL // 2):
                m = medir(im, x, y, CELL, CELL)
                if m is None or m["opaco"] < 1.0 or not filtro(m):
                    continue
                p = puntaje(m)
                if mejor is None or p > mejor[0]:
                    mejor = (p, im, x, y, os.path.basename(f), m)
    if mejor is None:
        raise SystemExit(f"no encontré parche de {que}")
    return mejor


def es_teja(m):
    h, s, v = m["hsv"]
    return (h <= 0.06 or h >= 0.93) and s >= 0.35 and 0.25 <= v <= 0.85


def puntaje_teja(m):
    _, s, _ = m["hsv"]
    # Con grano: una teja tiene relieve, un cartel rojo es plano.
    return s * 60 + min(m["desv"], 30) * 2


def es_chapa(m):
    h, s, v = m["hsv"]
    return s <= 0.18 and 0.35 <= v <= 0.85


def puntaje_chapa(m):
    _, s, _ = m["hsv"]
    return -s * 60 + min(m["desv"], 25) * 2


# ---------------------------------------------------------------------------
# 3. Armar la página
# ---------------------------------------------------------------------------

class Pagina:
    """Acumula recortes de píxel arbitrario y devuelve rects EN CELDAS."""

    def __init__(self, cols=32):
        self.cols = cols
        self.items = []  # (nombre, imagen, ancho_celdas, alto_celdas)

    def add(self, nombre, im, w_celdas, h_celdas):
        lienzo = Image.new("RGBA", (w_celdas * CELL, h_celdas * CELL), (0, 0, 0, 0))
        # Al pie: la pared crece hacia arriba desde el borde, así que lo que
        # sobra tiene que quedar arriba y transparente.
        lienzo.alpha_composite(im, (0, h_celdas * CELL - im.size[1]))
        self.items.append((nombre, lienzo, w_celdas, h_celdas))
        return nombre

    def componer(self):
        """Coloca cada item en la grilla y devuelve {nombre: (col, row, w, h)}."""
        sitio, col, row, alto_fila = {}, 0, 0, 0
        for nombre, im, w, h in self.items:
            if col + w > self.cols:
                col, row, alto_fila = 0, row + alto_fila, 0
            sitio[nombre] = (col, row, w, h)
            col += w
            alto_fila = max(alto_fila, h)
        filas = row + alto_fila
        pagina = Image.new("RGBA", (self.cols * CELL, filas * CELL), (0, 0, 0, 0))
        for nombre, im, w, h in self.items:
            c, r, _, _ = sitio[nombre]
            pagina.alpha_composite(im, (c * CELL, r * CELL))
        return pagina, sitio


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true", help="informa y no escribe")
    ap.add_argument("--preview", metavar="PNG", help="guarda la página ampliada para mirarla")
    args = ap.parse_args()

    paredes = Image.open(HOJA_PAREDES).convert("RGBA")
    pisos = Image.open(HOJA_PISOS).convert("RGBA")
    gradas = Image.open(HOJA_GRADAS).convert("RGBA")
    vanos = Image.open(HOJA_VANOS).convert("RGBA")
    verificar_hojas(paredes, pisos, gradas, vanos)

    bp = bloques_pared(paredes)
    madera = elegir(bp, es_madera, puntaje_madera, "pared de madera")
    piedra = elegir(bp, es_piedra_pared, puntaje_piedra_pared, "pared de piedra")
    bv = bloques_vano(vanos)
    vano_madera = vano_de_la_pared(bv, madera, "madera")
    vano_piedra = vano_de_la_pared(bv, piedra, "piedra")
    bf = bloques_piso(pisos)
    piso_piedra = elegir(bf, es_piedra_piso, puntaje_piedra_piso, "piso de piedra")
    tumbado = elegir(bf, es_madera, puntaje_madera, "tumbado de madera")

    # Los techos de los galpones de jardín son las ÚNICAS cubiertas del pack
    # dibujadas como superficie repetible (el resto son fachadas enteras). Vienen
    # en seis colores: el más rojo es la teja, el más gris la chapa.
    techos = sorted(glob(os.path.join(EXT_SINGLES, "*Storage_Roof*.png")))
    teja = mejor_parche(techos, es_teja, puntaje_teja, "teja")
    chapa = mejor_parche(techos, es_chapa, puntaje_chapa, "chapa")

    print(f"pared madera : grupo {madera[0]} fila {madera[1]} rgb={madera[2]['rgb']}")
    print(f"pared piedra : grupo {piedra[0]} fila {piedra[1]} rgb={piedra[2]['rgb']}")
    print(f"vano madera  : col {vano_madera[0]} fila {vano_madera[1]} rgb={vano_madera[2]['rgb']}")
    print(f"vano piedra  : col {vano_piedra[0]} fila {vano_piedra[1]} rgb={vano_piedra[2]['rgb']}")
    print(f"piso piedra  : col {piso_piedra[0]} fila {piso_piedra[1]} rgb={piso_piedra[2]['rgb']}")
    print(f"tumbado      : col {tumbado[0]} fila {tumbado[1]} rgb={tumbado[2]['rgb']}")
    print(f"teja         : {teja[4]} ({teja[2]},{teja[3]}) rgb={teja[5]['rgb']}")
    print(f"chapa        : {chapa[4]} ({chapa[2]},{chapa[3]}) rgb={chapa[5]['rgb']}")

    pag = Pagina()

    def cara(gx, ry, col):
        """El cuerpo de la pared (sin friso ni línea de hoja) de una columna."""
        x = (gx + col) * CELL
        y = ry * CELL + PARED_Y0
        return paredes.crop((x, y, x + CELL, y + PARED_Y1 - PARED_Y0))

    def afuera(gx, ry):
        """La cara de afuera: pared plana, sin friso ni zócalo (col 5, 2 celdas)."""
        x = (gx + 5) * CELL
        return paredes.crop((x, ry * CELL, x + CELL, ry * CELL + 2 * CELL))

    poses_pared = {}
    for clave, (gx, ry, _) in (("wall/wood", madera), ("wall/stone", piedra)):
        poses_pared[clave] = {
            "front": pag.add(f"{clave}:front", cara(gx, ry, 1), 1, 2),
            "left": pag.add(f"{clave}:left", cara(gx, ry, 0), 1, 2),
            "right": pag.add(f"{clave}:right", cara(gx, ry, 2), 1, 2),
            "back": pag.add(f"{clave}:back", afuera(gx, ry), 1, 2),
        }

    # **El VANO**: la pared con el hueco de la puerta, tal cual la trae el pack.
    #
    # El bloque mide dos celdas de ancho y la pieza se dibuja de UNA (el borde
    # entre dos celdas), así que el rect va de 2×2 y el sprite lo comprime al
    # colocarlo. Se prefiere eso a recortar la mitad: el hueco está centrado en el
    # bloque y mide 40 px, o sea que ninguna ventana de 32 px lo contiene con sus
    # dos jambas — cortarlo daría medio arco, que no se lee como paso.
    poses_vano = {}
    for clave, (cx, ry, _) in (("doorway/wood", vano_madera), ("doorway/stone", vano_piedra)):
        # Mismo recorte vertical que la pared (`cara`): sin el friso de techo
        # blanco de la hoja y a la misma altura, así el vano y la pared que lo
        # rodea rematan a la misma altura en vez de llevar el vano un filo blanco
        # de sombrero.
        bloque = vanos.crop(
            (cx * CELL, ry * CELL + PARED_Y0, (cx + 2) * CELL, ry * CELL + PARED_Y1)
        )
        poses_vano[clave] = pag.add(f"{clave}:front", bloque, 2, 2)

    # Ventana = la pared con el paño del pack encima. Es cómo el pack está pensado
    # (capas modulares): un vidrio suelto flotando no se lee como ventana, y un
    # agujero tampoco. Se hace para los DOS materiales — la casa de piedra también
    # tiene ventanas, y hasta ahora le tocaba una de madera.
    vidrio = elegir_ventana()
    ventanas = {}
    for clave, pared in (("window/basic", madera), ("window/stone", piedra)):
        pared_v = cara(pared[0], pared[1], 1).copy()
        vw, _ = vidrio.size
        pared_v.alpha_composite(vidrio, ((CELL - vw) // 2, ALTO_VENTANA_Y))
        ventanas[clave] = pag.add(f"{clave}:front", pared_v, 1, 2)

    # **Puerta de piedra** = la hoja de madera que el usuario ya eligió
    # (`door/wood`, primer cuadro de su tira) montada en el muro de piedra. Una
    # puerta de piedra maciza no existe en ninguna casa; lo que cambia con el
    # material es el MURO que la enmarca, y eso es exactamente lo que se compone
    # acá — con la textura que ya está en uso, sin pedir pixel nuevo.
    hoja = hoja_de_puerta()
    pared_p = cara(piedra[0], piedra[1], 1).copy()
    hw, hh = hoja.size
    pared_p.alpha_composite(hoja, ((CELL - hw) // 2, pared_p.size[1] - hh))
    pag.add("door/stone:front", pared_p, 1, 2)

    def celda(im, col, row):
        return im.crop((col * CELL, row * CELL, (col + 1) * CELL, (row + 1) * CELL))

    bc, br, bw, bh = BARANDA
    baranda = gradas.crop((bc * CELL, br * CELL, (bc + bw) * CELL, (br + bh) * CELL))
    bb = baranda.getbbox()
    baranda = baranda.crop(bb)
    if baranda.size[1] > CELL:
        raise SystemExit(f"la baranda mide {baranda.size[1]} px: no entra en una celda")
    pag.add("railing/madera", baranda, 1, 1)

    pag.add("floor/stone", celda(pisos, piso_piedra[0], piso_piedra[1]), 1, 1)
    pag.add("ceiling/madera", celda(pisos, tumbado[0], tumbado[1]), 1, 1)
    # El tumbado es "poner el piso como techo", así que el de piedra sale de la
    # misma losa que el piso de piedra: mirá tu casa desde adentro y el cielorraso
    # es del material con el que la levantaste.
    pag.add("ceiling/piedra", celda(pisos, piso_piedra[0], piso_piedra[1]), 1, 1)
    pag.add("stairs/up", celda(gradas, *GRADA_ARRIBA), 1, 1)
    pag.add("stairs/down", celda(gradas, *GRADA_ABAJO), 1, 1)
    pag.add("roof/teja", teja[1].crop((teja[2], teja[3], teja[2] + CELL, teja[3] + CELL)), 1, 1)
    pag.add("roof/chapa", chapa[1].crop((chapa[2], chapa[3], chapa[2] + CELL, chapa[3] + CELL)), 1, 1)

    pagina, sitio = pag.componer()
    verificar_pagina(pagina, sitio)
    print(f"página: {pagina.size[0]}x{pagina.size[1]} px, {len(sitio)} recortes")
    for nombre, (c, r, w, h) in sitio.items():
        print(f"  {nombre:24s} col={c} row={r} {w}x{h}")
    if args.preview:
        fondo = Image.new("RGBA", pagina.size, (28, 28, 38, 255))
        fondo.alpha_composite(pagina)
        s = 5
        fondo.convert("RGB").resize(
            (pagina.size[0] * s, pagina.size[1] * s), Image.NEAREST
        ).save(args.preview)
        print(f"vista previa: {args.preview}")
    if args.dry_run:
        return

    os.makedirs(os.path.join(ASSETS, "art/pack"), exist_ok=True)
    pagina.save(os.path.join(ASSETS, PAGINA_REL), optimize=True)

    with open(MANIFEST) as fh:
        man = json.load(fh)
    sprites = man.setdefault("sprites", {})
    previo = {}
    if os.path.exists(INDEX):
        with open(INDEX) as fh:
            previo = json.load(fh)
        for k in previo.get("sprites", []):
            sprites.pop(k, None)

    def rect(nombre):
        c, r, w, h = sitio[nombre]
        return {"col": c, "row": r, "w": w, "h": h}

    nuevos = []

    def escribir(clave, entrada):
        sprites[clave] = entrada
        nuevos.append(clave)

    for clave, poses in poses_pared.items():
        # `center` es el último eslabón de `walls::pose_chain`: sin él una pared
        # cuya pose no exista cae al sprite plano, que ya es el mismo dibujo pero
        # sin pasar por la cadena. Se declara para que la cadena nunca falle.
        p = {n: {"rect": rect(k)} for n, k in poses.items()}
        p["center"] = {"rect": rect(poses["front"])}
        escribir(
            clave,
            {"png": PAGINA_REL, "grid": [CELL, CELL], "rect": rect(poses["front"]), "poses": p},
        )
    # La ventana, el vano, la puerta de piedra y la baranda tienen UNA cara: se
    # ven igual de los dos lados y en los dos ejes. Se declaran las cuatro poses
    # apuntando al mismo recorte para que `walls::pose_chain` nunca se quede sin
    # eslabón.
    una_cara = [
        ("window/basic", "window/basic:front"),
        ("window/stone", "window/stone:front"),
        ("doorway/wood", "doorway/wood:front"),
        ("doorway/stone", "doorway/stone:front"),
        ("railing/madera", "railing/madera"),
    ]
    # La puerta de piedra además necesita sus poses de apertura: `walls` pide
    # `closed`/`open` y sin ellas no se anima. Es una hoja compuesta, no una tira,
    # así que las dos poses son el mismo dibujo — la puerta de piedra no se abre
    # sola, se cruza. (La de madera sigue con su tira animada, intacta.)
    for clave, recorte in [("door/stone", "door/stone:front")] + una_cara:
        caras = ["front", "back", "left", "right", "center"]
        if clave == "door/stone":
            caras += ["closed", "open"]
        escribir(
            clave,
            {
                "png": PAGINA_REL,
                "grid": [CELL, CELL],
                "rect": rect(recorte),
                "poses": {n: {"rect": rect(recorte)} for n in caras},
            },
        )
    for clave, capa in (
        ("floor/stone", "Floor"),
        ("ceiling/madera", None),
        ("ceiling/piedra", None),
        ("stairs/up", "Floor"),
        ("stairs/down", "Floor"),
        ("roof/teja", None),
        ("roof/chapa", None),
    ):
        e = {"png": PAGINA_REL, "grid": [CELL, CELL], "rect": rect(clave)}
        if capa:
            e["layer"] = capa
        # `center` para que el autotile lo encuentre y `tile_has_art` diga que sí.
        e["poses"] = {"center": {"rect": rect(clave)}}
        escribir(clave, e)

    losas, variantes = enganchar_losas(man, previo)
    nuevos += losas

    with open(MANIFEST, "w") as fh:
        json.dump(man, fh, indent=1, ensure_ascii=False, sort_keys=True)
    with open(INDEX, "w") as fh:
        json.dump({"sprites": nuevos, "variantes": variantes}, fh, indent=1)
    print(f"manifest: {len(nuevos) - len(losas)} slots de construcción con pixel, "
          f"{len(losas)} losas del pack enganchadas a la calle")


# ---------------------------------------------------------------------------
# 4. Las 169 losas del pack que nadie dibujaba
# ---------------------------------------------------------------------------

#: Corte de brillo entre calzada y acera, el mismo que midió `import-calles.py`
#: sobre las 351 losas del tema de terrenos urbanos.
BRILLO_ASFALTO = 120
BRILLO_VEREDA = 135
#: Desviación máxima para que una celda sea una LOSA y no un dibujo (una senda
#: peatonal, una tapa de alcantarilla, un cartel pintado en el piso).
LOSA_DESV = 22.0


def losas_del_pack(man):
    """`{'floor/vereda': [claves], 'floor/asfalto': [claves]}` medido en el pixel.

    El pack entró con 169 claves `floor/*` —veredas, asfaltos, suelos urbanos—
    que tienen arte y slot pero que el mundo no dibujaba nunca: la calle usaba
    siempre la primera. Acá se eligen las que son LOSA REPETIBLE para engancharlas
    como variantes de la vereda y la calzada.

    **Se clasifican por el pixel, no por el nombre**: el importador masivo bautizó
    estas claves automáticamente y muchas quedaron mal (hay `..._vereda_...` que
    son asfalto y al revés). Lo que decide es el brillo medio, igual que en
    `import-calles.py`.
    """
    sprites = man["sprites"]
    cache, salida, vistos = {}, {"floor/vereda": [], "floor/asfalto": []}, set()
    for clave in sorted(sprites):
        if "#" in clave or not clave.startswith("floor/"):
            continue
        if clave in ("floor/vereda", "floor/asfalto", "floor/wood", "floor/stone"):
            continue
        e = sprites[clave]
        r = e.get("rect")
        if not r or r.get("w") != 1 or r.get("h") != 1:
            continue  # un rect grande es una escena, no una baldosa
        png = e.get("png")
        ruta = os.path.join(ASSETS, png or "")
        if not png or not os.path.exists(ruta):
            continue
        if png not in cache:
            cache[png] = Image.open(ruta).convert("RGBA")
        gw, gh = e.get("grid", [CELL, CELL])
        im = cache[png].crop((r["col"] * gw, r["row"] * gh, (r["col"] + 1) * gw, (r["row"] + 1) * gh))
        m = medir(im, 0, 0, im.size[0], im.size[1])
        if m is None or m["opaco"] < 1.0 or m["desv"] > LOSA_DESV:
            continue  # una losa de suelo cubre su celda y es pareja
        huella = im.tobytes()
        if huella in vistos:
            continue  # el pack repite la misma losa en varios temas
        vistos.add(huella)
        brillo = sum(m["rgb"]) / 3
        if brillo <= BRILLO_ASFALTO:
            salida["floor/asfalto"].append((clave, e))
        elif brillo >= BRILLO_VEREDA:
            salida["floor/vereda"].append((clave, e))
    return salida


def enganchar_losas(man, previo):
    """Declara las losas como variantes de la calzada y la acera.

    No copia píxeles: la variante apunta al MISMO png y rect que la clave del
    pack, así que no crece un byte el atlas. Lo que cambia es que ahora el mundo
    las pide (`world2d::spawn_chunk_tile_art` elige una por celda).
    """
    sprites, objects = man["sprites"], man.setdefault("objects", {})
    # Limpiar lo de la corrida anterior: sólo las variantes con NUESTRO prefijo.
    for base, ids in (previo.get("variantes") or {}).items():
        obj = objects.get(base)
        if obj:
            obj["variants"] = [v for v in obj.get("variants", []) if v["id"] not in ids]
        for i in ids:
            sprites.pop(f"{base}#{i}", None)

    hechas, nuevas_ids = [], {}
    for base, losas in losas_del_pack(man).items():
        obj = objects.setdefault(base, {"label": base, "variants": []})
        ids = []
        for n, (clave, entrada) in enumerate(losas, 1):
            vid = f"p{n}"  # prefijo propio: no pisa las variantes de import-calles
            ids.append(vid)
            sprites[f"{base}#{vid}"] = dict(entrada)
            hechas.append(f"{base}#{vid}")
            obj["variants"].append({"id": vid, "label": f"{clave.split('/')[-1]}"})
        nuevas_ids[base] = ids
        print(f"{base}: +{len(ids)} losas del pack como variantes")
    return hechas, nuevas_ids


def hoja_de_puerta():
    """**La hoja de la puerta que ya está en uso**, recortada a su dibujo.

    Sale del primer cuadro de la tira animada que el usuario le asignó a
    `door/wood` en HA Studio, leída del manifest. No se elige una puerta nueva del
    pack a propósito: la del juego ya está decidida, y una segunda puerta con otro
    pixel haría que la casa de piedra y la de madera parecieran de dos juegos
    distintos. Es la regla de derivar del arte en uso en vez de pedir arte nuevo.
    """
    with open(MANIFEST) as fh:
        man = json.load(fh)
    e = man.get("sprites", {}).get("door/wood")
    if not e or "png" not in e:
        raise SystemExit("`door/wood` no tiene arte: asignalo en HA Studio antes de esto")
    im = Image.open(os.path.join(ASSETS, e["png"])).convert("RGBA")
    strip = e.get("strip", {"col": 0, "row": 0, "w": 1, "h": 2})
    x0, y0 = strip["col"] * CELL, strip["row"] * CELL
    cuadro = im.crop((x0, y0, x0 + strip["w"] * CELL, y0 + strip["h"] * CELL))
    bb = cuadro.getbbox()
    if bb is None:
        raise SystemExit("el primer cuadro de `door/wood` está vacío")
    hoja = cuadro.crop(bb)
    print(f"hoja puerta  : {os.path.basename(e['png'])} cuadro 0 {hoja.size}")
    return hoja


def elegir_ventana():
    """El paño de vidrio que se pega sobre la pared.

    Las ventanas del pack son de FACHADA: vienen con su marco y sus postigos, y
    miden hasta 52 px de ancho — casi dos tiles. Como la pieza de borde se dibuja
    de UNA celda de ancho, se recorta la franja central del vidrio
    (`ANCHO_VENTANA`), que es el paño en sí; los postigos quedan afuera y la
    pared del slot se sigue viendo a los costados. Sin eso la ventana taparía el
    tile entero y no se leería como "pared con ventana" sino como un vidrio
    suelto.

    Gana la más vidriosa: azulada y con contraste (reflejos), medido, no por el
    nombre — el pack llama "Window" también a las claraboyas de techo.
    """
    cands = sorted(glob(os.path.join(EXT_SINGLES, "*Window_With_Shutter*.png")))
    cands += sorted(glob(os.path.join(EXT_SINGLES, "*Office_32x32_Window_*_Modular.png")))
    mejor = None
    for f in cands:
        im = Image.open(f).convert("RGBA")
        bb = im.getbbox()
        # Tiene que caber DENTRO del cuerpo de la pared, con pared arriba y
        # abajo: si el paño mide lo mismo que la pared no hay ventana, hay vidrio.
        if bb is None or bb[3] - bb[1] > (PARED_Y1 - PARED_Y0) - ALTO_VENTANA_Y:
            continue
        cx = (bb[0] + bb[2]) // 2
        pano = im.crop((cx - ANCHO_VENTANA // 2, bb[1], cx + ANCHO_VENTANA // 2, bb[3]))
        m = medir(pano, 0, 0, pano.size[0], pano.size[1])
        if m is None or m["opaco"] < 0.95:
            continue
        h, s, v = m["hsv"]
        vidrioso = 0.50 <= h <= 0.72 and v >= 0.55
        p = (s * 80 + v * 40 if vidrioso else 0) + min(m["desv"], 25)
        if mejor is None or p > mejor[0]:
            mejor = (p, pano, os.path.basename(f))
    if mejor is None:
        raise SystemExit("no encontré una ventana usable en los singles")
    print(f"ventana      : {mejor[2]} paño {mejor[1].size}")
    return mejor[1]


if __name__ == "__main__":
    main()
