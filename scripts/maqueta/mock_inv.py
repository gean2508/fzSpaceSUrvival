#!/usr/bin/env python3
"""Mockups de la nueva interfaz de Ki — inventario creativo y de supervivencia.

Todo el pixel es real: marcos, botones e iconos del pack Modern UI (Style 1),
los objetos del manifest del juego y la fuente que embarca el APK.
Resolución: 2400x1080 (el Redmi, apaisado) con escala de pixel-art x3.
"""

import random
import sys
from pathlib import Path

from PIL import Image, ImageDraw

sys.path.insert(0, str(Path(__file__).resolve().parent))
from uikit import (ACENTO, AZUL, TEXTO, TEXTO_OSCURO, icono_en, marco, nine,  # noqa: E402
                   recorte_ui, sprite, texto)

S = 3                  # escala de pixel-art
W_U, H_U = 800, 360    # pantalla en unidades (2400x1080 / 3)

SLOT, GAP = 40, 4
PITCH = SLOT + GAP
COLS, ROWS = 11, 4
BORDE, PAD = 11, 8
COL_PERS = 152
TABS_H, HEAD_H, SCROLL_W = 36, 26, 6

# — botones-icono del pack: bloque en (768,32), grilla de 32 —
def btn_icono(col, fila):
    x, y = 768 + col * 32, 32 + fila * 32
    return (x, y, x + 32, y + 32)


GRID9 = btn_icono(2, 0)
EQUIS = btn_icono(1, 1)
ENGRANAJE = btn_icono(5, 0)
BASURA = btn_icono(8, 0)
FLECHA_ARR = btn_icono(7, 1)
FLECHA_ABA = btn_icono(6, 1)

# — iconos sueltos (medidos sobre la hoja) —
LUPA = (452, 294, 478, 314)
CORAZON = (432, 320, 464, 352)
CORAZON_VACIO = (432, 352, 466, 382)
MOCHILA = (336, 204, 374, 238)

# — las pestañas son NUESTRAS categorías, con un objeto real de cada una —
TABS_CREATIVO = [
    ("TODO", None), ("ÍTEMS", "items/pickaxe"), ("NATURALEZA", "nature/arbol_1"),
    ("MUEBLES", "furniture/cama"), ("CALLE", "prop/farol_de_calle_1"),
    ("PATIO", "prop/tobogan"), ("EDIFICIOS", "edificio/local"),
    ("AGUA", "agua/bote_abajo_1"), ("VEHÍCULOS", "vehicle/auto"),
    ("ESTRUCTURAS", "structure/statue"), ("FAUNA", "npc/fauna_chicken"),
    ("PIEZAS", "floor/baldosa_lila"), ("PAREDES", "wall/fachada_local"),
    ("TECHOS", "ceiling/madera"), ("EDIFICIO", "edificio/pb_madera"),
    ("PREFABS", "room/casa_1"),
]

TABS_SURVIVAL = [
    ("MOCHILA", None), ("FABRICAR", "items/stone_axe"), ("CONSTRUIR", "wall/stone"),
]

GRID_CREATIVO = [
    "nature/arbol_1", "nature/arbol_10", "nature/arbusto_1", "nature/piedra_1",
    "prop/manzanas_1", "prop/parrilla", "prop/tobogan", "prop/hamaca",
    "furniture/silla", "furniture/cama", "prop/farol_de_calle_1", "prop/cesped_cortado",
    "agua/bote_abajo_1", "vehicle/auto", "structure/statue", "structure/boulder",
    "edificio/local", "room/casa_1", "wall/wood", "wall/stone",
    "ceiling/madera", "ceiling/piedra", "floor/baldosa_blanca", "floor/asfalto",
    "items/pickaxe", "items/axe", "items/wood", "items/stone",
    "items/torch", "items/lantern", "items/apple", "items/bread",
    "items/iron_ingot", "items/gold_ingot", "items/gem_blue", "items/gem_red",
    "items/potion_blue", "items/potion_green", "items/jetpack", "items/leather",
    "npc/fauna_chicken", "npc/fauna_cow", "npc/fauna_sheep", "npc/fauna_pig",
]

GRID_SURVIVAL = [
    ("items/wood", 47), ("items/stone", 32), ("items/sticks", 18), ("items/leather", 4),
    ("items/iron_ingot", 9), ("items/potion_blue", 2), ("items/gem_blue", 1),
    ("items/bread", 3), ("items/meat", 2), ("items/sand", 24),
]

# La hotbar no tiene huecos reservados: el martillo es un ítem más y una casilla
# puede quedar vacía.
HOTBAR_SURVIVAL = [
    ("items/pickaxe", 1), ("items/axe", 1), ("items/torch", 12), ("items/wood", 47),
    ("items/stone", 32), ("items/apple", 6), ("items/lantern", 1), (None, 0), (None, 0),
]
HOTBAR_CREATIVO = [
    ("nature/arbol_1", 0), ("wall/stone", 0), ("furniture/cama", 0),
    ("prop/farol_de_calle_1", 0), ("items/torch", 0), ("agua/bote_abajo_1", 0),
    ("vehicle/auto", 0), ("npc/fauna_chicken", 0), (None, 0),
]


def px(u):
    return u * S


# ── fondo: el mundo real detrás del panel ────────────────────────────────────
def fondo_mundo(dst, semilla=11):
    rnd = random.Random(semilla)
    t = px(32)
    tile = sprite("tiles/grass").resize((t, t), Image.NEAREST)
    for y in range(0, dst.height, t):
        for x in range(0, dst.width, t):
            dst.alpha_composite(tile, (x, y))
    props = ["nature/arbol_1", "nature/arbol_10", "nature/arbusto_1", "nature/piedra_1",
             "nature/arbusto_10", "nature/arbol_100", "nature/arbol_103", "prop/cesped_cortado"]
    puestos = 0
    while puestos < 44:
        im = sprite(rnd.choice(props))
        im = im.resize((im.width * S, im.height * S), Image.NEAREST)
        x, y = rnd.randrange(-60, dst.width), rnd.randrange(-60, dst.height)
        if abs(x - dst.width // 2) < 260 and abs(y - dst.height // 2) < 260:
            continue        # el claro donde está el personaje
        dst.alpha_composite(im, (x, y))
        puestos += 1
    per = sprite("npc/char_1")
    per = per.resize((per.width * S, per.height * S), Image.NEAREST)
    dst.alpha_composite(per, (dst.width // 2 - per.width // 2, dst.height // 2 - per.height))
    dst.alpha_composite(Image.new("RGBA", dst.size, (16, 12, 24, 176)))


# ── piezas compuestas ────────────────────────────────────────────────────────
def slot(dst, x_u, y_u, clave=None, cant=0, sel=False, vacio=False):
    """Una celda: marco del pack + objeto del manifest + cantidad."""
    x, y = px(x_u), px(y_u)
    m = nine("slot", SLOT, SLOT, S)
    if vacio:
        m = Image.blend(Image.new("RGBA", m.size, (0, 0, 0, 0)), m, 0.55)
    dst.alpha_composite(m, (x, y))
    if clave:
        icono_en(dst, clave, x + px(SLOT) // 2 - 4, y + px(SLOT) // 2 - 4, px(26))
    if cant > 1:
        d = ImageDraw.Draw(dst)
        texto(d, (x + px(SLOT) - 16, y + px(SLOT) - 18), str(cant), 32, fill=TEXTO, anchor="rs")
    if sel:
        d = ImageDraw.Draw(dst)
        d.rectangle([x - 3, y - 3, x + px(SLOT) - 6, y + px(SLOT) - 6], outline=ACENTO, width=6)


def boton_icono(dst, x_u, y_u, lado, caja_icono):
    marco(dst, "slot_chico", px(x_u), px(y_u), px(lado), px(lado), S)
    w = (caja_icono[2] - caja_icono[0]) * 2
    recorte_ui(dst, caja_icono, px(x_u) + (px(lado) - w) // 2 - 3,
               px(y_u) + (px(lado) - w) // 2 - 3, 2)


def barra(dst, x_u, y_u, w_u, pct, color):
    marco(dst, "hueco", px(x_u), px(y_u), px(w_u), px(8), S)
    d = ImageDraw.Draw(dst)
    d.rectangle([px(x_u) + 6, px(y_u) + 6, px(x_u) + 6 + int((px(w_u) - 12) * pct),
                 px(y_u + 8) - 6], fill=color)


# ── la pantalla ──────────────────────────────────────────────────────────────
def pantalla_inventario(creativo=True):
    img = Image.new("RGBA", (px(W_U), px(H_U)), (0, 0, 0, 255))
    fondo_mundo(img)
    d = ImageDraw.Draw(img)

    tabs = TABS_CREATIVO if creativo else TABS_SURVIVAL
    grid_w = COLS * PITCH - GAP
    inner_w = COL_PERS + 14 + grid_w + 4 + SCROLL_W
    panel_w = inner_w + 2 * PAD + 2 * BORDE
    inner_h = TABS_H + 6 + HEAD_H + 6 + (ROWS * PITCH - GAP)
    panel_h = inner_h + 2 * PAD + 2 * BORDE
    pxu = (W_U - panel_w) // 2
    pyu = 6

    marco(img, "panel", px(pxu), px(pyu), px(panel_w), px(panel_h), S)
    ix, iy = pxu + BORDE + PAD, pyu + BORDE + PAD

    # — barra de pestañas: una categoría = una pestaña —
    # En creativo son 16 categorías: entran de a un icono. En supervivencia son
    # cuatro, así que la pestaña se da el lujo de decir su nombre.
    tab_w = ((inner_w - (len(tabs) - 1) * 3) // len(tabs)) if creativo else 118
    for i, (nombre, clave) in enumerate(tabs):
        tx = ix + i * (tab_w + 3)
        activa = i == 0
        marco(img, "slot" if activa else "slot_chico", px(tx), px(iy), px(tab_w), px(TABS_H), S)
        cx, cy = px(tx + tab_w / 2), px(iy + TABS_H / 2)
        if not creativo:
            cx = px(tx + 20)
        if clave:
            icono_en(img, clave, cx, cy, px(24))
        else:
            caja = GRID9 if creativo else MOCHILA
            recorte_ui(img, caja, int(cx - (caja[2] - caja[0]) * S / 2),
                       int(cy - (caja[3] - caja[1]) * S / 2), S)
        if not creativo:
            texto(d, (px(tx + 40), cy + 3), nombre, 32,
                  fill=TEXTO_OSCURO, anchor="lm", sombra=False)
        if activa:
            d.rectangle([px(tx) - 3, px(iy) - 3, px(tx + tab_w) - 6, px(iy + TABS_H) - 6],
                        outline=ACENTO, width=6)

    # — columna del personaje (izquierda, alto completo) —
    cy0 = iy + TABS_H + 6
    alto_col = HEAD_H + 6 + (ROWS * PITCH - GAP)
    alto_pers = alto_col - 66
    marco(img, "panel_liso", px(ix), px(cy0), px(COL_PERS), px(alto_pers), S)
    texto(d, (px(ix + COL_PERS / 2), px(cy0) + 26), "PERSONAJE", 28,
          fill=TEXTO_OSCURO, anchor="mm", sombra=False)
    per = sprite("npc/char_1")
    per = per.resize((per.width * S, per.height * S), Image.NEAREST)
    img.alpha_composite(per, (px(ix + COL_PERS // 2) - per.width // 2, px(cy0 + 26)))
    # cuatro huecos de equipo REALES: mano, escudo, espalda, luz
    equipo = [("items/pickaxe", "MANO"), ("items/shield_wood", "ESCUDO"),
              ("items/jetpack", "ESPALDA"), ("items/lantern", "LUZ")]
    for i, (clave, et) in enumerate(equipo):
        ex = ix + 3 if i < 2 else ix + COL_PERS - SLOT - 3
        ey = cy0 + 26 + (i % 2) * (SLOT + 8)
        slot(img, ex, ey, clave if creativo or i != 2 else None, vacio=(not creativo and i == 2))
        texto(d, (px(ex + SLOT / 2), px(ey + SLOT) + 16), et, 20,
              fill=TEXTO_OSCURO, anchor="mm", sombra=False)

    # — estado: lo que el HUD ya mide, acá con nombre —
    sy = cy0 + alto_pers + 6
    marco(img, "panel_liso", px(ix), px(sy), px(COL_PERS), px(60), S)
    for i in range(5):
        recorte_ui(img, CORAZON if i < 4 else CORAZON_VACIO, px(ix + 8) + i * px(15), px(sy + 5), 1)
    for j, (et, pct, color) in enumerate([("HAMBRE", 0.72, (198, 146, 68)),
                                          ("SED", 0.55, AZUL),
                                          ("KI", 0.90, (110, 200, 220))]):
        by = sy + 22 + j * 13
        texto(d, (px(ix + 8), px(by) + 11), et, 22, fill=TEXTO_OSCURO, anchor="lm", sombra=False)
        barra(img, ix + 56, by, COL_PERS - 64, pct, color)

    # — encabezado del catálogo: título + búsqueda —
    gx = ix + COL_PERS + 14
    hy = cy0
    titulo = "TODO" if creativo else "MOCHILA"
    texto(d, (px(gx), px(hy + HEAD_H / 2) + 2), titulo, 40, fill=TEXTO, anchor="lm")
    sub = "modo creativo · todo a mano" if creativo else "lo que llevás encima"
    from uikit import fuente
    ancho = d.textlength(titulo, font=fuente(40))
    texto(d, (px(gx) + ancho + 24, px(hy + HEAD_H / 2) + 6), sub, 26,
          fill=(206, 180, 150), anchor="lm")
    buscar_w = 200
    bx = gx + grid_w - buscar_w
    marco(img, "boton_ancho", px(bx), px(hy), px(buscar_w), px(HEAD_H), S)
    recorte_ui(img, LUPA, px(bx) + 20, px(hy) + 15, 2)
    texto(d, (px(bx) + 100, px(hy + HEAD_H / 2) + 3), "Buscar…", 32,
          fill=(120, 100, 86), anchor="lm", sombra=False)

    # — cerrar / ajustes, en el borde de arriba a la derecha —
    boton_icono(img, pxu + panel_w - BORDE - 34, pyu - 4, 30, EQUIS)
    boton_icono(img, pxu + panel_w - BORDE - 68, pyu - 4, 30, ENGRANAJE)

    # — la grilla —
    gy = hy + HEAD_H + 6
    for r in range(ROWS):
        for c in range(COLS):
            i = r * COLS + c
            x, y = gx + c * PITCH, gy + r * PITCH
            if creativo:
                slot(img, x, y, GRID_CREATIVO[i % len(GRID_CREATIVO)], 0, sel=(i == 0))
            else:
                clave, cant = GRID_SURVIVAL[i] if i < len(GRID_SURVIVAL) else (None, 0)
                slot(img, x, y, clave, cant, sel=(i == 0), vacio=clave is None)

    # — barra de scroll —
    sx = gx + grid_w + 4
    marco(img, "hueco", px(sx), px(gy), px(SCROLL_W), px(ROWS * PITCH - GAP), S)
    d.rectangle([px(sx) + 6, px(gy) + 6, px(sx + SCROLL_W) - 6, px(gy) + px(56)],
                fill=(176, 118, 82))

    # — hotbar: SIEMPRE a la vista, fuera del panel —
    hb = HOTBAR_CREATIVO if creativo else HOTBAR_SURVIVAL
    n = len(hb) + 1
    hb_w = n * PITCH - GAP
    hbx = (W_U - hb_w) // 2
    hby = H_U - 6 - SLOT
    for i, (clave, cant) in enumerate(hb):
        slot(img, hbx + i * PITCH, hby, clave, cant, sel=(i == 0), vacio=clave is None)
        texto(d, (px(hbx + i * PITCH) + 10, px(hby) + 22), str(i + 1), 22,
              fill=(120, 100, 86), anchor="lm", sombra=False)
    bagx = hbx + len(hb) * PITCH
    marco(img, "slot", px(bagx), px(hby), px(SLOT), px(SLOT), S)
    recorte_ui(img, MOCHILA, px(bagx) + 15, px(hby) + 18, 2)

    return img


if __name__ == "__main__":
    out = Path(sys.argv[1] if len(sys.argv) > 1 else ".")
    pantalla_inventario(True).convert("RGB").save(out / "01-inventario-creativo.png")
    pantalla_inventario(False).convert("RGB").save(out / "02-inventario-supervivencia.png")
    print("listo")
