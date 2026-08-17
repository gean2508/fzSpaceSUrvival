#!/usr/bin/env python3
"""Splash, menú principal y creación de personaje — mockups a 2400x1080.

El fondo de las dos pantallas de menú es parallax en bucle (`parallax.py`): se
exporta un PNG del primer cuadro y un GIF con el ciclo entero.
"""

import math
import random
import sys
from pathlib import Path

from PIL import Image, ImageDraw

sys.path.insert(0, str(Path(__file__).resolve().parent))
import personaje as PJ  # noqa: E402
from parallax import Fondo  # noqa: E402
from uikit import ACENTO, TEXTO, TEXTO_OSCURO, icono_en, marco, recorte_ui, sprite, texto  # noqa: E402
from mock_inv import S, btn_icono as btn  # noqa: E402

W, H = 2400, 1080

# — paleta de FZ studios: morado y turquesa fresco, con naranja neón y fucsia —
MORADO = (124, 62, 214)
MORADO_HI = (162, 106, 240)
TURQUESA = (38, 224, 197)
TURQUESA_HI = (126, 246, 226)
NARANJA = (255, 138, 42)
FUCSIA = (240, 60, 160)
NEGRO = (18, 16, 26)
CREMA = (240, 244, 242)

LOOK = {"body": 2, "eyes": 1, "outfit": 20, "hair": 56, "accessory": None}
OTROS = [{"body": 5, "eyes": 3, "outfit": 80, "hair": 24, "accessory": None},
         {"body": 7, "eyes": 4, "outfit": 110, "hair": 120, "accessory": 3}]


# ══ splash ═══════════════════════════════════════════════════════════════════
def _blob(d, cx, cy, r, color):
    d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=color)


def salpicon(d, cx, cy, r, color, rnd, brazos=7, crecer=1.0):
    """Mancha de pintura: núcleo deforme + chorros que se afinan + gotas.

    Se dibuja en baja resolución y se amplía con vecino más cercano, así el borde
    queda escalonado como el pixel del juego. Va PLANA: un degradado la convierte
    en una pelota.
    """
    r = r * crecer
    if r < 1:
        return
    for _ in range(11):
        a, dd = rnd.uniform(0, 6.28), rnd.uniform(0, r * .72)
        _blob(d, cx + dd * math.cos(a), cy + dd * math.sin(a) * .8,
              r * rnd.uniform(.38, .72), color)
    for _ in range(brazos):
        a = rnd.uniform(0, 6.28)
        largo = r * rnd.uniform(1.3, 2.8)
        for i in range(9):
            t = (i + 1) / 9
            rr = max(.6, r * .42 * (1 - t) ** 1.5)
            _blob(d, cx + largo * t * math.cos(a), cy + largo * t * math.sin(a) * .82, rr, color)
        if rnd.random() < .7:
            gr = r * rnd.uniform(.10, .20)
            _blob(d, cx + (largo + gr * 2.4) * math.cos(a),
                  cy + (largo + gr * 2.4) * math.sin(a) * .82, gr, color)
    for _ in range(brazos * 3):
        a, largo = rnd.uniform(0, 6.28), r * rnd.uniform(1.6, 3.6)
        _blob(d, cx + largo * math.cos(a), cy + largo * math.sin(a) * .82,
              max(.6, r * rnd.uniform(.04, .13)), color)


# Más juntas alrededor del centro, donde va el logo — pero sin taparlo.
MANCHAS = [
    (132, 76, 34, MORADO),
    (268, 118, 32, TURQUESA),
    (238, 54, 19, NARANJA),
    (152, 146, 20, FUCSIA),
    (316, 64, 14, MORADO_HI),
    (78, 132, 15, TURQUESA_HI),
    (200, 156, 11, NARANJA),
    (296, 26, 10, FUCSIA),
]


def splash(paso=1.0):
    """`paso` de 0 a 1: las manchas caen escalonadas y el logo entra al final."""
    z = 6
    lo = Image.new("RGBA", (W // z, H // z), NEGRO + (255,))
    d = ImageDraw.Draw(lo)
    for i, (cx, cy, r, color) in enumerate(MANCHAS):
        t0 = i * 0.075
        k = max(0.0, min(1.0, (paso - t0) / 0.16))
        if k > 0:
            crecer = k * (1.12 - 0.12 * k)      # rebote corto al aterrizar
            salpicon(d, cx, cy, r, color, random.Random(20 + i), crecer=crecer)
    img = lo.resize((W, H), Image.NEAREST)
    d = ImageDraw.Draw(img)
    if paso > 0.55:
        k = min(1.0, (paso - 0.55) / 0.2)
        texto(d, (W // 2, H // 2 - 6), "FZ STUDIOS", int(120 + 84 * k), fill=CREMA, anchor="mm")
    if paso > 0.85:
        texto(d, (W // 2, H // 2 + 132), "presenta", 50, fill=TURQUESA_HI,
              anchor="mm", sombra=False)
    return img


def splash_tira():
    ancho = W // 3
    tira = Image.new("RGBA", (W, H // 3 + 66), (26, 24, 34, 255))
    d = ImageDraw.Draw(tira)
    for i, paso in enumerate([0.30, 0.68, 1.0]):
        f = splash(paso).resize((ancho - 16, H // 3 - 16), Image.NEAREST)
        tira.alpha_composite(f, (i * ancho + 8, 56))
        texto(d, (i * ancho + ancho // 2, 32),
              ["1 · salpica", "2 · entra el logo", "3 · completo"][i],
              34, fill=(196, 186, 206), anchor="mm", sombra=False)
    return tira


# ══ piezas comunes ═══════════════════════════════════════════════════════════
def cartel(img, d, cx, y, ancho, alto, etiqueta, px_txt=54):
    """Un cartel colgado del borde del panel — el título no es texto suelto."""
    x = cx - ancho // 2
    marco(img, "boton", x, y, ancho, alto, S)
    d.rectangle([x + 15, y + 15, x + ancho - 15, y + alto - 15], outline=(150, 108, 70), width=3)
    texto(d, (cx, y + alto // 2 + 2), etiqueta, px_txt, fill=TEXTO_OSCURO,
          anchor="mm", sombra=False)
    for lado in (-1, 1):
        rx = cx + lado * (ancho // 2 - 30)
        d.ellipse([rx - 7, y + alto // 2 - 7, rx + 7, y + alto // 2 + 7], fill=(150, 108, 70))


def barra_estado(img, d, datos):
    """La barra de arriba: chapa del juego + fichas con icono y separador."""
    marco(img, "panel_liso", 18, 12, W - 36, 114, S)
    marco(img, "boton", 36, 24, 570, 90, S)
    texto(d, (66, 66), "KI", 56, fill=TEXTO_OSCURO, anchor="lm", sombra=False)
    texto(d, (146, 50), "MUNDO", 26, fill=(150, 120, 92), anchor="lm", sombra=False)
    texto(d, (146, 82), "ABIERTO", 26, fill=(150, 120, 92), anchor="lm", sombra=False)
    d.rectangle([300, 40, 304, 98], fill=(176, 140, 104))
    icono_en(img, "ui/menu_tierra", 356, 68, 58)
    texto(d, (400, 68), "TIERRA", 30, fill=TEXTO_OSCURO, anchor="lm", sombra=False)
    x = 700
    for icono, et, val in datos:
        d.rectangle([x - 30, 32, x - 26, 94], fill=(196, 168, 134))
        recorte_ui(img, icono, x, 42, 2)
        texto(d, (x + 76, 50), et, 24, fill=(150, 120, 92), anchor="lm", sombra=False)
        texto(d, (x + 76, 84), val, 34, fill=TEXTO_OSCURO, anchor="lm", sombra=False)
        x += 424


def halo(img, cx, cy, r):
    """Un claro de luz detrás del personaje. Sin esto, con el parallax encima, un
    árbol del primer plano se lo come."""
    capa = Image.new("RGBA", (r * 2, r * 2), (0, 0, 0, 0))
    dd = ImageDraw.Draw(capa)
    for i in range(24, 0, -1):
        rr = r * i / 24
        a = int(52 * (1 - i / 24) ** 1.5)
        dd.ellipse([r - rr, r - rr * .92, r + rr, r + rr * .92], fill=(255, 244, 214, a))
    img.alpha_composite(capa, (cx - r, cy - r))


def rail(img, d, x, y, items):
    for i, (caja, et) in enumerate(items):
        yy = y + i * 126
        marco(img, "slot", x, yy, 96, 96, S)
        recorte_ui(img, caja, x + 16, yy + 16, 2)
        texto(d, (x + 116, yy + 50), et, 32, fill=(238, 230, 214), anchor="lm")


# ══ menú principal ═══════════════════════════════════════════════════════════
def menu(fondo):
    img = fondo.copy()
    d = ImageDraw.Draw(img)

    barra_estado(img, d, [(btn(2, 0), "SEMILLA", "48 213"),
                          (btn(6, 0), "PARTIDA", "Valle del Río"),
                          (btn(0, 2), "DÍA", "12"),
                          (btn(4, 0), "PERSONAJES", "3")])

    rail(img, d, 40, 200, [(btn(5, 0), "AJUSTES"), (btn(8, 3), "CONTROLES"),
                           (btn(6, 1), "ACTUALIZAR"), (btn(1, 3), "CRÉDITOS")])

    per = PJ.personaje(LOOK, PJ.FRENTE, escala=7)
    cx = 900
    halo(img, cx, 690, 300)
    d.ellipse([cx - 108, 878, cx + 108, 914], fill=(20, 16, 26, 110))
    img.alpha_composite(per, (cx - per.width // 2, 900 - per.height))
    for dx, izq in [(-330, True), (330, False)]:
        bx = cx + dx - 48
        marco(img, "slot", bx, 640, 96, 96, S)
        recorte_ui(img, btn(5 if izq else 4, 1), bx + 16, 656, 2)
    texto(d, (cx, 960), "LEO", 52, fill=TEXTO, anchor="mm")
    for i in range(3):
        col = ACENTO if i == 0 else (150, 130, 120)
        px0 = cx - 34 + i * 34
        d.ellipse([px0 - 8, 1000, px0 + 8, 1016], fill=col)

    pxu, pyu, pw, ph = 1344, 186, 1020, 828
    marco(img, "panel", pxu, pyu, pw, ph, S)

    def ficha(tx, ty, tw, th, et, deco, activa=False, caras=False):
        marco(img, "slot", tx, ty, tw, th, S)
        vin = Image.new("RGBA", (tw - 48, th - 108), (0, 0, 0, 0))
        t = 32 * 2
        tile = sprite("tiles/grass").resize((t, t), Image.NEAREST)
        for yy in range(0, vin.height, t):
            for xx in range(0, vin.width, t):
                vin.alpha_composite(tile, (xx, yy))
        if caras:
            for j, lk in enumerate([LOOK] + OTROS):
                pj = PJ.personaje(lk, PJ.FRENTE, escala=2)
                vin.alpha_composite(pj, (60 + j * 90, vin.height - pj.height - 4))
        else:
            for j, k in enumerate(deco):
                im = sprite(k)
                im = im.resize((im.width * 2, im.height * 2), Image.NEAREST)
                vin.alpha_composite(im, (40 + j * 150, vin.height - im.height - 6))
        img.alpha_composite(vin, (tx + 24, ty + 22))
        d.rectangle([tx + 24, ty + 22, tx + 24 + vin.width, ty + 22 + vin.height],
                    outline=(120, 92, 66), width=3)
        texto(d, (tx + tw // 2, ty + th - 30), et, 40, fill=TEXTO_OSCURO,
              anchor="mm", sombra=False)
        if activa:
            d.rectangle([tx - 3, ty - 3, tx + tw - 3, ty + th - 3], outline=ACENTO, width=6)

    ox, oy, tw, th, gap = pxu + 42, pyu + 84, 462, 216, 24
    # CONTINUAR es lo que se toca el 90 % de las veces: se lleva el ancho entero.
    ficha(ox, oy, tw * 2 + gap, 228, "CONTINUAR", ["nature/arbol_1", "room/casa_1",
                                                  "prop/farol_de_calle_1"], activa=True)
    resto = [("PARTIDAS", ["edificio/local", "nature/piedra_1"], False),
             ("PERSONAJES", None, True),
             ("NUEVA PARTIDA", ["agua/bote_abajo_1", "nature/arbol_10"], False),
             ("SALIR", ["door/wood", "prop/farol_de_calle_1"], False)]
    for i, (et, deco, caras) in enumerate(resto):
        ficha(ox + (i % 2) * (tw + gap), oy + 228 + gap + (i // 2) * (th + gap),
              tw, th, et, deco, caras=caras)
    cartel(img, d, pxu + pw // 2, pyu - 48, 420, 96, "MENÚ")
    return img


# ══ creación de personaje ════════════════════════════════════════════════════
CAPAS = [("PIEL", "piel"), ("OJOS", "ojos"), ("ROPA", "ropa"),
         ("PELO", "pelo"), ("ACCESORIOS", "accesorio")]


def crear_personaje(fondo, capa_activa="pelo", estilo=None, color=2, scroll=0.34):
    """El chargen no es una lista plana: son ESTILOS por COLORES, que es como lo
    modela `character_create.rs` (`cycle_style` / `cycle_color`). La grilla muestra
    los estilos y la fila de abajo, los colores del elegido."""
    img = fondo.copy()
    d = ImageDraw.Draw(img)

    ests = PJ.estilos(capa_activa)
    cols_disp = PJ.colores(capa_activa)
    if estilo is None:
        estilo = ests[len(ests) // 3][0]
    color = color if color in cols_disp else cols_disp[0]
    idx_sel = PJ.indice(capa_activa, estilo, color)

    look = dict(LOOK)
    look[{"piel": "body", "ojos": "eyes", "ropa": "outfit",
          "pelo": "hair", "accesorio": "accessory"}[capa_activa]] = idx_sel

    nombre_capa = next(e for e, c in CAPAS if c == capa_activa)
    barra_estado(img, d, [(btn(7, 0), "HUECOS", "3 de 5"),
                          (btn(2, 0), "CAPA", nombre_capa),
                          (btn(4, 0), "NOMBRE", "Leo")])

    per = PJ.personaje(look, PJ.FRENTE, escala=7)
    cx = 560
    halo(img, cx, 650, 300)
    d.ellipse([cx - 108, 838, cx + 108, 874], fill=(20, 16, 26, 110))
    img.alpha_composite(per, (cx - per.width // 2, 860 - per.height))
    for i, (celda, et) in enumerate([(PJ.DERECHA, "DERECHA"), (PJ.ESPALDAS, "DE ESPALDAS"),
                                     (PJ.IZQUIERDA, "IZQUIERDA")]):
        pj = PJ.personaje(look, celda, escala=2)
        x = cx - 216 + i * 156
        marco(img, "slot", x, 890, 132, 144, S)
        img.alpha_composite(pj, (x + 66 - pj.width // 2, 898))
        texto(d, (x + 66, 1056), et, 22, fill=(214, 202, 186), anchor="mm")
    texto(d, (cx, 186), "LEO", 56, fill=TEXTO, anchor="mm")
    texto(d, (cx, 238), "tocá el nombre para cambiarlo", 26,
          fill=(198, 186, 170), anchor="mm")
    texto(d, (cx, 292), PJ.etiqueta(capa_activa, idx_sel), 34, fill=ACENTO, anchor="mm")

    pxu, pyu, pw, ph = 1170, 186, 1194, 828
    marco(img, "panel", pxu, pyu, pw, ph, S)
    ix, iy = pxu + 66, pyu + 54
    inner_w = pw - 132

    tab_w = (inner_w - 4 * 9) // 5
    for i, (et, capa) in enumerate(CAPAS):
        tx = ix + i * (tab_w + 9)
        act = capa == capa_activa
        marco(img, "slot" if act else "slot_chico", tx, iy, tab_w, 84, S)
        texto(d, (tx + tab_w // 2, iy + 44), et, 28, fill=TEXTO_OSCURO,
              anchor="mm", sombra=False)
        if act:
            d.rectangle([tx - 3, iy - 3, tx + tab_w - 3, iy + 84 - 3], outline=ACENTO, width=6)

    # — grilla de ESTILOS, con su barra de scroll cuando no entran —
    gy = iy + 84 + 21
    cols, filas, lado, sep = 7, 3, 130, 12
    celdas = cols * filas
    grid_w = cols * lado + (cols - 1) * sep
    total = len(ests)
    primero = 0
    if total > celdas:
        max_scroll = ((total + cols - 1) // cols) - filas
        primero = int(round(max_scroll * scroll)) * cols
    for r in range(filas):
        for c in range(cols):
            i = primero + r * cols + c
            if i >= total:
                continue
            st, mapa = ests[i]
            x, y = ix + c * (lado + sep), gy + r * (lado + sep)
            marco(img, "slot", x, y, lado, lado, S)
            j = mapa.get(color, next(iter(mapa.values())))
            if capa_activa in ("ropa", "accesorio"):
                pj = PJ.personaje(dict(look, **{"outfit" if capa_activa == "ropa"
                                                else "accessory": j}), PJ.FRENTE, escala=2)
                img.alpha_composite(pj, (x + lado // 2 - pj.width // 2, y + 2))
            else:
                ret = PJ.retrato_compuesto(look, capa_activa, j, 2)
                img.alpha_composite(ret, (x + lado // 2 - ret.width // 2,
                                          y + lado // 2 - ret.height // 2))
            if st == estilo:
                d.rectangle([x - 3, y - 3, x + lado - 3, y + lado - 3], outline=ACENTO, width=6)
    grid_h = filas * lado + (filas - 1) * sep
    if total > celdas:
        sx = ix + grid_w + 14
        marco(img, "hueco", sx, gy, 24, grid_h, S)
        alto_pulgar = max(90, int(grid_h * celdas / total))
        top = gy + int((grid_h - alto_pulgar) * scroll)
        d.rectangle([sx + 6, top, sx + 18, top + alto_pulgar], fill=(176, 118, 82))

    # — fila de COLORES: la otra mitad del chargen —
    cy = gy + grid_h + 20
    if len(cols_disp) > 1:
        texto(d, (ix, cy + 30), "COLOR", 26, fill=(214, 196, 168), anchor="lm", sombra=False)
        for k, cc in enumerate(cols_disp):
            j = PJ.indice(capa_activa, estilo, cc)
            x = ix + 130 + k * 74
            marco(img, "slot_chico", x, cy, 62, 62, S)
            d.rectangle([x + 14, y_col := cy + 14, x + 48, cy + 48],
                        fill=PJ.color_medio(capa_activa, j))
            if cc == color:
                d.rectangle([x - 3, cy - 3, x + 62 - 3, cy + 62 - 3], outline=ACENTO, width=5)
    # El contador va corto porque comparte fila con las muestras de color; el
    # nombre de lo elegido vive al lado del personaje, que es donde se mira.
    pos = next(i for i, (st, _) in enumerate(ests) if st == estilo) + 1
    texto(d, (ix + inner_w, cy + 30), f"{pos}/{total}", 30,
          fill=(214, 196, 168), anchor="rm", sombra=False)

    by = pyu + ph - 54 - 84
    anchos = [(300, "ALEATORIO", False), (300, "REINICIAR", False), (390, "CONFIRMAR", True)]
    tot = sum(a for a, _, _ in anchos) + 24 * (len(anchos) - 1)
    bx = ix + inner_w - tot
    for ancho, et, fuerte in anchos:
        marco(img, "boton", bx, by, ancho, 84, S)
        if fuerte:
            d.rectangle([bx + 24, by + 21, bx + ancho - 24, by + 63], fill=ACENTO)
        texto(d, (bx + ancho // 2, by + 46), et, 34,
              fill=(56, 42, 20) if fuerte else TEXTO_OSCURO, anchor="mm", sombra=False)
        bx += ancho + 24
    cartel(img, d, pxu + pw // 2, pyu - 48, 640, 96, "CREAR PERSONAJE", 44)
    return img


# ══ salida ═══════════════════════════════════════════════════════════════════
def gif(cuadros, ruta, ms=90, ancho=1200):
    chicos = [c.convert("RGB").resize((ancho, ancho * H // W), Image.LANCZOS)
              .quantize(colors=96, method=Image.MEDIANCUT, dither=Image.NONE)
              for c in cuadros]
    chicos[0].save(ruta, save_all=True, append_images=chicos[1:], duration=ms,
                   loop=0, optimize=True, disposal=2)


if __name__ == "__main__":
    out = Path(sys.argv[1] if len(sys.argv) > 1 else ".")
    N = 20

    splash_tira().convert("RGB").save(out / "04-splash-fz-studios.png")
    splash(1.0).convert("RGB").save(out / "04b-splash-final.png")
    gif([splash(i / 17) for i in range(18)] + [splash(1.0)] * 6,
        out / "04c-splash.gif", 80)

    bosque = Fondo("bosque", 3)
    menu(bosque.cuadro(0, N)).convert("RGB").save(out / "05-menu-principal.png")
    gif([menu(bosque.cuadro(t, N)) for t in range(N)], out / "05b-menu-parallax.gif")

    noche = Fondo("noche", 11)
    crear_personaje(noche.cuadro(0, N)).convert("RGB").save(out / "06-crear-personaje.png")
    crear_personaje(noche.cuadro(0, N), "ropa", 12, 4).convert("RGB").save(
        out / "06b-crear-personaje-ropa.png")
    gif([crear_personaje(noche.cuadro(t, N)) for t in range(N)],
        out / "06c-crear-personaje.gif")
    print("listo")
