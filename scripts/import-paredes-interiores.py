#!/usr/bin/env python3
"""**Las paredes de INTERIOR del pack, las 54, con sus vanos y sus pisos.**

    python3 scripts/import-paredes-interiores.py --dry-run   # informa y no escribe
    python3 scripts/import-paredes-interiores.py             # página + manifest + slots

## Por qué existe (y por qué no alcanzaba con `import-construccion.py`)

`import-construccion.py` mira las mismas hojas de Room_Builder y saca **dos**
estilos: el más de madera y el más de piedra (`wall/wood`, `wall/stone`). Eso
estaba bien cuando lo único que había era "levantá un galpón": dos materiales,
dos costos, y listo.

Pero el pack dibujó **54 paredes de interior** —empapelados, azulejos, boiserie,
yeso pintado, ladrillo visto— y un cuarto se lee por su pared. Con dos estilos,
la cocina, el baño y el dormitorio de toda la ciudad son el mismo cuarto pintado
del mismo marrón. Las otras 52 estaban en el disco desde el primer día.

**Lo mismo con las hojas.** `Room_Bulder_subfiles_32x32/` trae nueve y el juego
tocaba tres. Las seis que faltaban no son adorno: son las que hacen que una
pared de interior remate como pared y no como un rectángulo pegado.

## Qué hoja aporta qué

| hoja | qué es | qué sale de acá |
|---|---|---|
| `Room_Builder_Walls` | 3 grupos × 10 col × 2 filas, 54 estilos | el paño: `front` (cara al cuarto, con su zócalo), `left`/`right` (sus remates de canto) y `back` (la cara lisa de afuera, col 5) |
| `Room_Builder_Arched_Entryways` | bloques 2×2 | el VANO de cada estilo: la pared con el hueco ya dibujado |
| `Room_Builder_Floors` | 4 grupos × 3 col × 2 filas | la losa de cada estilo |
| `Room_Builder_baseboards` | 6×6 | el zócalo suelto, para rematar contra un piso que no es el suyo |
| `Room_Builder_borders` | 45×10 | las orlas de piso |
| `Room_Builder_Floor_Connectors` | 7×36 | el empalme entre dos pisos distintos |
| `Room_Builder_Floor_Paths` | 42×12 | las sendas / alfombras corridas |
| `Room_Builder_Floor_Shadows` | 16×5 | la sombra de contacto al pie de la pared |

## Las reglas que se respetan (biblia §8)

- **Los rects se MIDEN.** Ninguna coordenada de recorte está escrita a ojo: sale
  de la geometría de la hoja, que `verificar_hojas` comprueba contra el píxel
  antes de cortar nada. Si LimeZu reordena una hoja, esto se planta acá.
- **El estilo se empareja por COLOR, no por orden.** El vano y el piso de cada
  pared se eligen por distancia de color al paño, igual que hace
  `import-construccion.py::vano_de_la_pared`. Los tres catálogos no tienen por
  qué venir en el mismo orden entre versiones del pack.
- ⚠ **En esta hoja NO HAY cara de afuera.** La primera versión de este script
  daba por sentado que la columna 5 era "el paño liso de la vereda" y escribía
  con ella las poses `back`/`espalda`/`center`. **Está medido que es mentira**:
  el color medio de la col 5 es (204,208,211) contra (208,212,215) de la col 1
  —el MISMO empapelado— y lo que cambia entre las dos es que la 5 no trae la
  tapa, o sea que el dibujo arranca 10 px más arriba. Con eso, toda casa
  generada del mundo enseñaba el empapelado del dormitorio mirando a la calle.

  `Room_Builder_*` es un kit de INTERIOR y nada más. La cara de afuera sale de
  Modern Exteriors y la escribe `scripts/import-fachadas.py`; acá no se emite
  `back` ni `espalda` **a propósito**, para que nadie vuelva a colgar un
  empapelado de una fachada.
- **Idempotente por índice propio.** `scripts/interiores-import-index.json`
  guarda lo que escribió esta corrida; la siguiente pisa **lo suyo** y nada más.
  El arte curado a mano en HA Studio no se toca.
- **No se pisa a `import-construccion.py`.** `wall/wood` y `wall/stone` siguen
  siendo suyos: acá se escriben claves `wall/int_*` nuevas.
- **El friso SÍ va: es la TAPA del muro.** La versión anterior lo tiraba
  ("son 10 px del mockup de LimeZu") y con eso la pared perdía su remate y su
  línea de contorno: el paño quedaba flotando contra el piso y un cuarto se leía
  como una mancha de color. Está comprobado contra el dibujo del propio artista
  —`6_Home_Designs/Generic_Home_1`, que es una casa armada por LimeZu con este
  mismo kit—: ahí **toda** pared horizontal son DOS celdas, tapa blanca arriba y
  paño abajo, y es esa tapa la que hace que se lea como pared vista desde
  arriba. Así que el recorte va de `y=0` a `y=64`, la celda entera, y el
  guardián de salida es el contrario del de antes: se planta si una pared se
  quedó SIN su tapa (ver `verificar_tapas`).
"""

import argparse
import colorsys
import json
import os
import sys

from PIL import Image

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib.origen import INTERIOR, marcar_origen  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACK = os.path.join(ROOT, "PRUEBAS-CODIGOS")
INT = os.path.join(PACK, "moderninteriors-win (Copy 1)/1_Interiors/32x32")
RB = os.path.join(INT, "Room_Bulder_subfiles_32x32")

HOJAS = {
    "muros": "Room_Builder_3d_walls_32x32.png",
    "paredes": "Room_Builder_Walls_32x32.png",
    "vanos": "Room_Builder_Arched_Entryways_32x32.png",
    "pisos": "Room_Builder_Floors_32x32.png",
    "zocalos": "Room_Builder_baseboards_32x32.png",
    "orlas": "Room_Builder_borders_32x32.png",
    "empalmes": "Room_Builder_Floor_Connectors_32x32.png",
    "sendas": "Room_Builder_Floor_Paths_32x32.png",
    "sombras": "Room_Builder_Floor_Shadows_32x32.png",
}

ASSETS = os.path.join(ROOT, "native/crates/ha-game/assets")
MANIFEST = os.path.join(ASSETS, "art/manifest.json")
PAGINA_REL = "art/pack/interiores.png"
INDEX = os.path.join(ROOT, "scripts/interiores-import-index.json")
SLOTS_RS = os.path.join(ROOT, "native/crates/ha-assets/src/slots.rs")

CELL = 32

# --- Geometría de la hoja de paredes -----------------------------------------
# Igual que `import-construccion.py`: tres grupos de 10 columnas separados por
# una columna vacía, cada estilo ocupa 2 filas. Dentro del bloque de 10×2:
#   col 0..3  pared que MIRA a la habitación (friso arriba, zócalo abajo)
#   col 4..6  la misma pared sin friso ni zócalo (la cara de afuera)
#   col 7..9  copia de 0..3
GRUPOS_PARED = (0, 11, 22)
#: **La celda entera.** Los primeros 10 px son la TAPA del muro (contorno + la
#: banda clara que se ve desde arriba) y los últimos 2 su línea de apoyo: las dos
#: cosas son parte del tile, no basura de la hoja — ver el docstring.
PARED_Y0, PARED_Y1 = 0, 64
#: Franja que se mide para clasificar el estilo: sólo el centro del paño. El
#: borde de arriba y el zócalo son iguales en todos los estilos y le regalan
#: grano a las paredes lisas.
MEDIR_Y0, MEDIR_Y1 = 22, 54
#: Columnas del bloque. `COL_SEGUIDO` (la 5) es el MISMO paño pero sin tapa: es
#: el tramo que va debajo de otro tramo, no "la cara de afuera" (ver docstring).
COL_FRENTE, COL_IZQ, COL_DER, COL_SEGUIDO = 1, 0, 2, 5

# --- Geometría del KIT DE CUARTO (`Room_Builder_3d_walls`) --------------------
#
# **Es LA hoja para armar un cuarto, y estaba sin importar.** `Room_Builder_Walls`
# dibuja el paño de frente y nada más; con eso, un muro norte-sur salía como una
# hilera de carteles de frente puestos de canto. Esta hoja trae el cuarto entero
# en 3/4: el muro horizontal con su tapa, los verticales vistos desde arriba y
# las esquinas EN ÁNGULO que empalman los dos.
#
# La grilla no está escrita a ojo: sale de cruzar la hoja contra las casas que
# LimeZu armó con ella (`6_Home_Designs/*/32x32/*Layer_1*`). Comparando celda por
# celda (hash del pixel) qué recorte usó el artista y en qué lugar del cuarto, el
# uso se amontona en un bloque de 8×7 que se repite cada 8 columnas y cada 7
# filas — 178 usos de (3,2) y 223 de (3,3) contra 0 de sus vecinas.
BLOQUE_W, BLOQUE_H = 8, 7
#: Las piezas del bloque, en coordenadas locales. `None` = probá la siguiente.
#:
#: - el muro horizontal son DOS celdas, tapa arriba y paño abajo: así lo dibuja
#:   el pack y así se lee como pared vista de arriba;
#: - los verticales son la tira de tapa (col 1 al oeste, col 6 al este): de un
#:   muro norte-sur, en cenital 3/4, se ve el canto, no el empapelado;
#: - las esquinas son las cuñas en ángulo (2,2) y (5,2), que sólo trae la primera
#:   banda de la hoja; en las demás se cae al vertical y no queda hueco.
#: - una PUERTA de este kit es un HUECO, no un arco: el pack deja pasar el piso y
#:   remata el muro a cada lado con su jamba. Medido en `Generic_Home_1`, la celda
#:   a la izquierda del hueco usa (0,3)+(0,4) y la de la derecha (0,0)+(0,1).
PIEZAS = {
    "edge_n": [((3, 2), (3, 3))],
    "end_e": [((0, 3), (0, 4))],
    "end_w": [((0, 0), (0, 1))],
    "edge_s": [((3, 5),)],
    "edge_w": [((1, 2),), ((1, 1),)],
    "edge_e": [((6, 2),), ((6, 1),)],
    "corner_nw": [((2, 2),), ((1, 2),)],
    "corner_ne": [((5, 2),), ((6, 2),)],
    "center": [((3, 2), (3, 3))],
}
#: Franja del paño que se mide para bautizar el estilo y emparejarle vano y piso.
CUARTO_MEDIR = (3, 3)


# --- Geometría de la hoja de VANOS -------------------------------------------
VANOS_FILA0 = 4
VANOS_COLS = 8
VANO_HUECO_Y = 36
VANO_MEDIR_Y0, VANO_MEDIR_Y1 = 10, 30

# --- Geometría de la hoja de PISOS -------------------------------------------
#: Cuatro grupos de 3 columnas, separados por una columna vacía; cada estilo
#: ocupa 2 filas y la celda (0,0) del bloque es la losa de campo.
GRUPOS_PISO = (0, 4, 8, 12)


def hsv(rgb):
    r, g, b = (c / 255.0 for c in rgb)
    return colorsys.rgb_to_hsv(r, g, b)


def medir(im, x0, y0, w, h):
    """Color medio de un rect, ignorando lo transparente."""
    px = im.load()
    n = r = g = b = 0
    total = 0
    for y in range(y0, y0 + h):
        for x in range(x0, x0 + w):
            total += 1
            p = px[x, y]
            if p[3] > 0:
                r += p[0]
                g += p[1]
                b += p[2]
                n += 1
    if not n:
        return None
    rgb = (r // n, g // n, b // n)
    return {"rgb": rgb, "hsv": hsv(rgb), "opaco": n / total}


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


# ---------------------------------------------------------------------------
# Nombres en castellano, DERIVADOS DEL COLOR
# ---------------------------------------------------------------------------
# El pack no bautiza estos bloques: son celdas de una hoja, no archivos con
# nombre. Así que el nombre sale de lo único que hay, que es el pixel — el mismo
# criterio con el que `import-construccion.py` decide cuál es "de madera".
#
# Que sean nombres de COLOR y no de material es a propósito: "Pared verde claro"
# es verdad mirando el dibujo; "Pared de azulejo" sería una suposición sobre un
# bloque de 32×52 px que nadie midió.

#: (hue_max, nombre). Se recorre en orden; el hue está en grados.
TONOS = (
    (15, "roja"),
    (45, "madera"),
    (70, "arena"),
    (160, "verde"),
    (200, "turquesa"),
    (250, "azul"),
    (290, "violeta"),
    (345, "rosa"),
    (361, "roja"),
)


def nombre_de_color(m):
    """Nombre castellano del paño: tono + claridad. Sin número (lo pone el caller)."""
    h, s, v = m["hsv"]
    if s < 0.10:
        # Gris de verdad: el tono no dice nada, manda el brillo.
        if v >= 0.88:
            return "blanca"
        if v >= 0.72:
            return "gris clara"
        if v >= 0.48:
            return "gris"
        return "gris oscura"
    grados = h * 360.0
    tono = next(n for lim, n in TONOS if grados < lim)
    # La madera desaturada y clara es crema, no madera: es el yeso del pack.
    if tono == "madera" and s < 0.22:
        tono = "crema"
    if v >= 0.86:
        return f"{tono} clara"
    if v <= 0.50:
        return f"{tono} oscura"
    return tono


#: El nombre del slot sale del de la etiqueta: sin acentos, con guión bajo.
ACENTOS = str.maketrans("áéíóúñ", "aeioun")


def clave_de(nombre, n):
    base = nombre.translate(ACENTOS).replace(" ", "_")
    return f"{base}_{n}"


# ---------------------------------------------------------------------------
# Verificación: las suposiciones, comprobadas contra el píxel
# ---------------------------------------------------------------------------

def verificar_hojas(h):
    """Se planta ANTES de cortar si el pack cambió de forma."""
    oc = ocupacion(h["paredes"])
    assert len(oc[0]) == 32 and len(oc) == 40, f"hoja de paredes {len(oc[0])}x{len(oc)}"
    for gx in GRUPOS_PARED:
        assert all(oc[0][gx + c] for c in range(10)), f"grupo {gx} incompleto"
        assert not oc[1][gx + 4] and not oc[1][gx + 6], f"grupo {gx} sin remates"
        assert oc[1][gx + COL_SEGUIDO], f"grupo {gx} sin tramo seguido en la col 5"
    # La TAPA del muro: la fila 0 de un bloque es mucho más clara arriba que abajo.
    arriba = medir(h["paredes"], (GRUPOS_PARED[0] + 1) * CELL, 2, CELL, 8)
    abajo = medir(h["paredes"], (GRUPOS_PARED[0] + 1) * CELL, 12, CELL, 18)
    assert arriba["hsv"][2] > abajo["hsv"][2] + 0.05, "no encuentro la tapa del muro"

    ocv = ocupacion(h["vanos"])
    assert len(ocv[0]) == 10, f"hoja de vanos {len(ocv[0])} columnas"
    assert all(ocv[VANOS_FILA0][c] for c in range(VANOS_COLS)), "banda de vanos incompleta"
    px = h["vanos"].load()
    y = VANOS_FILA0 * CELL + VANO_HUECO_Y + 12
    assert px[32, y][3] == 0, "el vano no tiene hueco en el medio"
    assert px[2, y][3] > 0 and px[62, y][3] > 0, "el vano no tiene jambas"

    ocp = ocupacion(h["pisos"])
    assert len(ocp[0]) == 15, f"hoja de pisos {len(ocp[0])} columnas"
    for c in (3, 7, 11):
        assert not ocp[10][c], f"la col {c} de pisos deberia ser separador"

    # Las seis hojas que nunca se importaron: se comprueba que estén y que
    # tengan pixel, no su reparto interno (cada una se recorta por bbox).
    for k in ("zocalos", "orlas", "empalmes", "sendas", "sombras"):
        assert h[k].getbbox() is not None, f"la hoja {k} está vacía"


#: Cuántos píxeles de 1.024 tienen que cambiar entre la TAPA de un muro y su
#: paño. Medido sobre los 19 estilos del kit: el que menos cambia da 456 y el que
#: más, 1.024 — o sea que las dos celdas del muro horizontal son SIEMPRE dibujos
#: distintos. Con 300 el guardián no roza ningún estilo y sí caza la regresión que
#: motivó todo esto: recortar las dos filas del mismo paño, que deja la pared sin
#: su remate y el cuarto convertido en una mancha de color.
TAPA_CAMBIA = 300

#: **Se probó y NO sirve: pedirle a la tapa que sea más CLARA que su paño.** Era
#: la primera versión de este guardián y parece obvia —el remate se ve iluminado
#: desde arriba— pero no se sostiene contra la hoja: medida la distancia de color
#: entre tapa y paño en los 19 estilos, va de 4,2 a 150,6, y cinco estilos
#: (`gris_clara_2`, `gris_5`, `turquesa_2`, `azul_3`, `gris_clara_3`) tienen la
#: tapa del mismo tono que la pared. Lo mismo con "arriba tiene que haber una
#: línea de contorno oscura": la traen 15 de 19. Queda anotado para no volver a
#: intentarlo con otro número.


def verificar_tapas(pagina, sitio, piezas):
    """Invariante de SALIDA: **el muro horizontal conserva sus DOS celdas.**

    Es el guardián contrario al que tenía este script, y el cambio no es de
    gusto: la versión vieja recortaba desde `y=10` para tirar lo que llamaba "el
    friso del mockup", y eso que tiraba era la TAPA del muro. Sin tapa el paño
    queda flotando contra el piso y un cuarto se lee como una mancha de color.
    Las casas que LimeZu armó con este mismo kit (`6_Home_Designs`) tienen tapa en
    todas sus paredes.

    Se comprueba lo que sí vale para los 19 estilos: que el recorte sea opaco de
    punta a punta —un muro no tiene agujeros— y que su celda de arriba **no sea
    el mismo dibujo** que la de abajo. Si alguien vuelve a mover la grilla y las
    dos filas caen sobre el paño, esto se planta acá y no veinte minutos después
    mirando el juego.
    """
    px = pagina.load()
    malos = []
    for nombre in piezas:
        c, r, w, h = sitio[nombre]
        x0, y0 = c * CELL, r * CELL
        huecos = sum(
            1
            for y in range(y0, y0 + h * CELL)
            for x in range(x0, x0 + w * CELL)
            if px[x, y][3] == 0
        )
        cambia = sum(
            1
            for y in range(CELL)
            for x in range(w * CELL)
            if px[x0 + x, y0 + y] != px[x0 + x, y0 + CELL + y]
        )
        if huecos or cambia < TAPA_CAMBIA:
            malos.append((nombre, huecos, cambia))
    if malos:
        detalle = ", ".join(f"{n} ({hu} huecos, {ca} px de tapa)" for n, hu, ca in malos[:6])
        raise SystemExit(
            f"{len(malos)} muros salieron sin su tapa: {detalle}. "
            f"Revisá la grilla del kit: la celda de arriba del muro va ENTERA."
        )


# ---------------------------------------------------------------------------
# Enumerar los bloques de cada hoja
# ---------------------------------------------------------------------------

def bloques_pared(im):
    """(gx, fila, medida) de cada estilo. Son 54 en el pack de dic 2024."""
    oc = ocupacion(im)
    out = []
    for gx in GRUPOS_PARED:
        for ry in range(0, len(oc) - 1, 2):
            if not oc[ry][gx + COL_FRENTE]:
                continue
            m = medir(im, (gx + COL_FRENTE) * CELL, ry * CELL + MEDIR_Y0, CELL, MEDIR_Y1 - MEDIR_Y0)
            out.append((gx, ry, m))
    return out


def bloques_cuarto(im):
    """Los estilos de cuarto de la hoja 3d: `(bx, by, medida)` por bloque.

    Un bloque cuenta si trae sus piezas obligatorias — el muro horizontal y al
    menos un vertical. Las últimas bandas de la hoja son recortes sueltos y se
    caen solas por acá, sin lista negra escrita a mano.
    """
    W, H = im.size
    out = []
    for by in range(0, H // CELL - BLOQUE_H + 1, BLOQUE_H):
        for bx in range(0, W // CELL - BLOQUE_W + 1, BLOQUE_W):
            def celda(c, r):
                x, y = (bx + c) * CELL, (by + r) * CELL
                t = im.crop((x, y, x + CELL, y + CELL))
                return t if t.getbbox() else None
            if any(celda(*p) is None for p in ((3, 2), (3, 3))):
                continue
            if celda(1, 2) is None and celda(6, 2) is None:
                continue
            m = medir(im, (bx + CUARTO_MEDIR[0]) * CELL, (by + CUARTO_MEDIR[1]) * CELL, CELL, CELL)
            if m is None or m["opaco"] < 0.99:
                continue
            out.append((bx, by, m))
    return out


def bloques_vano(im):
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


def bloques_piso(im):
    oc = ocupacion(im)
    out = []
    for gx in GRUPOS_PISO:
        for ry in range(0, len(oc)):
            if not oc[ry][gx]:
                continue
            m = medir(im, gx * CELL, ry * CELL, CELL, CELL)
            if m is None or m["opaco"] < 0.99:
                continue
            out.append((gx, ry, m))
    return out


def mas_parecido(candidatos, objetivo):
    """El bloque de color más cercano. Empareja catálogos sin depender del orden."""
    d = lambda m: sum((m[2]["rgb"][i] - objetivo[i]) ** 2 for i in range(3))
    return min(candidatos, key=d)


def luma(rgb):
    return 0.299 * rgb[0] + 0.587 * rgb[1] + 0.114 * rgb[2]


#: **Cuánto tiene que separarse el piso de su pared, en brillo.**
#:
#: El emparejado por color a secas —el que tenía este script— elegía para cada
#: pared la losa MÁS PARECIDA que hubiera, y eso es exactamente lo que no se
#: puede hacer: medido sobre el estilo `azul_clara_4`, pared (208,212,215) y piso
#: (206,214,220) dan 8 de distancia y el cuarto entero sale de un solo azul, sin
#: que se vea dónde termina el piso y empieza la pared.
#:
#: El número sale del dibujo del propio artista: en `6_Home_Designs/
#: Generic_Home_1` la pared gris (190) va con un piso de madera (145), o sea 45
#: de separación. Se pide la mitad de eso —el mínimo con el que el corte todavía
#: se lee— y entre las losas que la cumplen gana la MÁS parecida, así que el
#: cuarto sigue combinando: es la misma familia de color, un tono más abajo.
CONTRASTE_MINIMO = 22


def piso_de_la_pared(pisos, pared_rgb):
    """La losa de esta pared: la más parecida **que se distinga de ella**."""
    lejos = [p for p in pisos if abs(luma(p[2]["rgb"]) - luma(pared_rgb)) >= CONTRASTE_MINIMO]
    return mas_parecido(lejos or pisos, pared_rgb)


# ---------------------------------------------------------------------------
# Página
# ---------------------------------------------------------------------------

class Pagina:
    """Acumula recortes de píxel arbitrario y devuelve rects EN CELDAS."""

    def __init__(self, cols=32):
        self.cols = cols
        self.items = []

    def add(self, nombre, im, w_celdas, h_celdas, al_pie=True):
        lienzo = Image.new("RGBA", (w_celdas * CELL, h_celdas * CELL), (0, 0, 0, 0))
        dy = h_celdas * CELL - im.size[1] if al_pie else 0
        lienzo.alpha_composite(im, (0, dy))
        self.items.append((nombre, lienzo, w_celdas, h_celdas))
        return nombre

    def componer(self):
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


def trozos(im, ancho, alto):
    """Parte una hoja en bloques de `ancho`×`alto` celdas y descarta los vacíos."""
    W, H = im.size
    out = []
    for r in range(H // (alto * CELL)):
        for c in range(W // (ancho * CELL)):
            t = im.crop((c * ancho * CELL, r * alto * CELL,
                         (c + 1) * ancho * CELL, (r + 1) * alto * CELL))
            if t.getbbox() is None:
                continue
            out.append((c, r, t))
    return out


# ---------------------------------------------------------------------------
# Las familias sueltas: las seis hojas que nunca se importaron
# ---------------------------------------------------------------------------
# Cada una se corta por la grilla PROPIA de la hoja (32 px: el pack las dibujó
# así, el nombre del archivo lo dice) y se descarta la celda vacía. No se
# compone nada: son piezas de kit, y una pieza de kit no se siembra sola — se
# coloca desde el editor o la compone quien la necesite (regla 8 de biblia §2).
FAMILIAS = (
    # (hoja, prefijo de clave, singular para la etiqueta)
    ("zocalos", "zocalo/int", "Zócalo"),
    ("orlas", "orla/int", "Orla de piso"),
    ("empalmes", "empalme/int", "Empalme de piso"),
    ("sendas", "senda/int", "Senda de piso"),
    ("sombras", "sombra/int", "Sombra al pie"),
)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true", help="informa y no escribe")
    ap.add_argument("--preview", metavar="PNG", help="guarda la página ampliada")
    args = ap.parse_args()

    hojas = {k: Image.open(os.path.join(RB, v)).convert("RGBA") for k, v in HOJAS.items()}
    verificar_hojas(hojas)

    muros, vanos, pisos = hojas["muros"], hojas["vanos"], hojas["pisos"]
    bc = bloques_cuarto(muros)
    bv = bloques_vano(vanos)
    bf = bloques_piso(pisos)
    print(f"kit de cuartos: {len(bc)} estilos · vanos: {len(bv)} · pisos: {len(bf)}")
    if len(bc) < 12:
        raise SystemExit(f"esperaba ~19 estilos de cuarto y encontré {len(bc)}")

    # --- nombres, y que no se repitan ---
    vistos = {}
    estilos = []
    for bx, by, m in bc:
        base = nombre_de_color(m)
        vistos[base] = vistos.get(base, 0) + 1
        estilos.append({
            "gx": bx, "ry": by, "m": m,
            "nombre": base, "n": vistos[base],
        })
    for e in estilos:
        e["clave"] = clave_de(e["nombre"], e["n"])
        # "Pared verde clara 2" — el nombre humano que ve el editor.
        e["label"] = f"Cuarto {e['nombre']} {e['n']}"

    pag = Pagina()

    def pieza(bx, by, celdas):
        """Apila las celdas locales de un bloque en UN recorte de 1×N."""
        alto = len(celdas)
        im = Image.new("RGBA", (CELL, alto * CELL), (0, 0, 0, 0))
        for i, (c, r) in enumerate(celdas):
            x, y = (bx + c) * CELL, (by + r) * CELL
            im.alpha_composite(muros.crop((x, y, x + CELL, y + CELL)), (0, i * CELL))
        return im

    def hay(bx, by, celdas):
        return all(
            muros.crop(((bx + c) * CELL, (by + r) * CELL,
                        (bx + c + 1) * CELL, (by + r + 1) * CELL)).getbbox()
            for c, r in celdas
        )

    for e in estilos:
        gx, ry, k = e["gx"], e["ry"], e["clave"]
        # **Las piezas del CUARTO, una por orientación.**
        #
        # No hay cara de afuera: `Room_Builder_*` es un kit de interior y la
        # vereda la dibuja la fachada de Modern Exteriors (`import-fachadas.py`).
        # Por eso acá NO se escriben `back` ni `espalda` — ver el docstring.
        e["poses"] = {}
        for pose, opciones in PIEZAS.items():
            celdas = next((c for c in opciones if hay(gx, ry, c)), None)
            if celdas is None:
                continue
            e["poses"][pose] = pag.add(
                f"wall/int_{k}:{pose}", pieza(gx, ry, celdas), 1, len(celdas)
            )
        # `front` es el nombre viejo del muro horizontal: se mantiene para que
        # nada que ya lo pida se quede sin pixel.
        e["poses"]["front"] = e["poses"]["edge_n"]

        # El VANO del MISMO estilo, emparejado por color.
        cx, vry, _ = mas_parecido(bv, e["m"]["rgb"])
        bloque = vanos.crop((cx * CELL, vry * CELL + PARED_Y0, (cx + 2) * CELL, vry * CELL + PARED_Y1))
        e["vano"] = pag.add(f"doorway/int_{k}", bloque, 2, 2)

        # Y su LOSA: la más parecida de las que SE DISTINGUEN de la pared.
        fx, fry, _ = piso_de_la_pared(bf, e["m"]["rgb"])
        e["piso"] = pag.add(f"floor/int_{k}", pisos.crop(
            (fx * CELL, fry * CELL, (fx + 1) * CELL, (fry + 1) * CELL)), 1, 1)

    # --- las cinco familias sueltas ---
    familias = []
    for hoja, pref, singular in FAMILIAS:
        piezas = trozos(hojas[hoja], 1, 1)
        for i, (c, r, t) in enumerate(piezas, 1):
            clave = f"{pref}_{i}"
            familias.append((clave, f"{singular} {i}",
                             pag.add(clave, t, 1, 1, al_pie=False)))
        print(f"  {pref:14s} {len(piezas):4d} piezas")

    pagina, sitio = pag.componer()
    # `center` es el tramo SEGUIDO y no lleva tapa por definición (col 5): el
    # guardián se le aplica a las tres piezas que sí la tienen que traer.
    # Todas las piezas del kit tienen que traer su remate claro arriba: es lo que
    # las hace leerse como muro visto de arriba y no como un cartel de color.
    # Sólo el muro horizontal: es el único que lleva tapa Y paño en la misma
    # pieza. Los verticales SON tapa de punta a punta (de un muro norte-sur se ve
    # el canto y nada más), así que compararlos contra su propio paño no tiene
    # sentido.
    verificar_tapas(pagina, sitio, [e["poses"]["edge_n"] for e in estilos])
    print(f"página: {pagina.size[0]}x{pagina.size[1]} px · {len(sitio)} recortes")
    if args.preview:
        fondo = Image.new("RGBA", pagina.size, (28, 28, 38, 255))
        fondo.alpha_composite(pagina)
        fondo.convert("RGB").save(args.preview)
        print(f"vista previa: {args.preview}")
    if args.dry_run:
        for e in estilos[:8]:
            print(f"  {e['clave']:22s} {e['label']}")
        print(f"  … y {len(estilos) - 8} más")
        return

    os.makedirs(os.path.join(ASSETS, "art/pack"), exist_ok=True)
    pagina.save(os.path.join(ASSETS, PAGINA_REL), optimize=True)

    with open(MANIFEST) as fh:
        man = json.load(fh)
    sprites = man.setdefault("sprites", {})
    if os.path.exists(INDEX):
        with open(INDEX) as fh:
            for k in json.load(fh).get("sprites", []):
                sprites.pop(k, None)

    def rect(nombre):
        c, r, w, h = sitio[nombre]
        return {"col": c, "row": r, "w": w, "h": h}

    nuevos = []
    slots = []

    def escribir(clave, entrada, label):
        sprites[clave] = entrada
        nuevos.append(clave)
        slots.append((clave, label))

    for e in estilos:
        k = e["clave"]
        p = {n: {"rect": rect(v)} for n, v in e["poses"].items()}
        # El recorte base es el muro horizontal: la pieza que se dibuja si
        # alguien pide esta pared sin decir para dónde mira.
        escribir(f"wall/int_{k}", {
            "png": PAGINA_REL, "grid": [CELL, CELL],
            "rect": rect(e["poses"]["edge_n"]), "poses": p,
        }, e["label"])
        # El vano se ve igual de los dos lados: las cinco poses al mismo recorte.
        escribir(f"doorway/int_{k}", {
            "png": PAGINA_REL, "grid": [CELL, CELL], "rect": rect(e["vano"]),
            "poses": {n: {"rect": rect(e["vano"])}
                      for n in ("front", "edge_n", "edge_s", "edge_w", "edge_e", "center")},
        }, f"Vano {e['nombre']} {e['n']}")
        escribir(f"floor/int_{k}", {
            "png": PAGINA_REL, "grid": [CELL, CELL], "layer": "Floor",
            "rect": rect(e["piso"]),
            "poses": {"center": {"rect": rect(e["piso"])}},
        }, f"Piso {e['nombre']} {e['n']}")

    for clave, label, nombre in familias:
        escribir(clave, {
            "png": PAGINA_REL, "grid": [CELL, CELL], "rect": rect(nombre),
            "poses": {"center": {"rect": rect(nombre)}},
        }, label)

    # Esta página es de Modern INTERIORS y se declara como tal: es el dato que
    # impide que un muro de living termine de fachada en la calle. Ver
    # `scripts/lib/origen.py`.
    paginas = marcar_origen(man, [PAGINA_REL], INTERIOR)

    with open(MANIFEST, "w") as fh:
        json.dump(man, fh, indent=1, ensure_ascii=False, sort_keys=True)
    with open(INDEX, "w") as fh:
        json.dump({"sprites": nuevos, "origen": paginas}, fh, indent=1)

    escribir_slots(slots)
    print(f"manifest: {len(nuevos)} claves · slots.rs: {len(slots)} slots")


def escribir_slots(slots):
    """Reemplaza el bloque `INTERIORES:START/END` de `slots.rs`.

    Bloque propio, aparte del de `import-pack-masivo.py`: dos importadores que
    escriben el mismo bloque se pisan, y el masivo corre mucho más seguido.
    """
    with open(SLOTS_RS) as fh:
        texto = fh.read()
    ini_m, fin_m = "    // === INTERIORES:START ===", "    // === INTERIORES:END ==="
    if ini_m not in texto:
        raise SystemExit(
            f"falta el marcador {ini_m!r} en slots.rs — agregalo dentro de una "
            "tabla de slots antes de correr este importador"
        )
    ini = texto.index(ini_m)
    fin = texto.index(fin_m)
    cuerpo = "".join(
        f'    ({json.dumps(k)}, {json.dumps(l, ensure_ascii=False)}),\n' for k, l in slots
    )
    texto = texto[: ini + len(ini_m)] + "\n" + cuerpo + texto[fin:]
    with open(SLOTS_RS, "w") as fh:
        fh.write(texto)


if __name__ == "__main__":
    main()
