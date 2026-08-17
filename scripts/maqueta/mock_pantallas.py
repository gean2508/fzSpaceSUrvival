#!/usr/bin/env python3
"""Partidas, personajes, nueva partida y la consola escondida.

El personaje y el mundo son dos listas SEPARADAS: cualquier personaje entra a
cualquier partida, y en una partida pueden vivir varios. Los planetas, lunas y
estrellas no son partidas: son el gameplay de adentro de una.
"""

import random
import sys
from pathlib import Path

from PIL import Image, ImageDraw

sys.path.insert(0, str(Path(__file__).resolve().parent))
import personaje as PJ  # noqa: E402
from parallax import Fondo  # noqa: E402
from uikit import ACENTO, TEXTO, TEXTO_OSCURO, marco, recorte_ui, sprite, texto  # noqa: E402
from mock_inv import S, btn_icono as btn  # noqa: E402
from mock_menu import LOOK, OTROS, barra_estado, cartel  # noqa: E402

W, H = 2400, 1080
TRASH = btn(8, 0)
CHECK = btn(0, 1)
DADO = btn(2, 0)
LOOKS = [LOOK] + OTROS + [{"body": 1, "eyes": 2, "outfit": 60, "hair": 96, "accessory": None},
                          {"body": 3, "eyes": 5, "outfit": 12, "hair": 8, "accessory": 20}]
NOMBRES = ["Leo", "Mica", "Tobi", "Nube", "Rulo"]


def vineta(w, h, deco, semilla=1, suelo="tiles/grass"):
    vin = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    t = 64
    tile = sprite(suelo).resize((t, t), Image.NEAREST)
    for yy in range(0, h, t):
        for xx in range(0, w, t):
            vin.alpha_composite(tile, (xx, yy))
    rnd = random.Random(semilla)
    for k in deco:
        im = sprite(k)
        im = im.resize((im.width * 2, im.height * 2), Image.NEAREST)
        vin.alpha_composite(im, (rnd.randrange(0, max(1, w - im.width)),
                                 h - im.height - rnd.randrange(0, 20)))
    return vin


def boton(img, d, x, y, w, h, etiqueta=None, icono=None, fuerte=False):
    marco(img, "boton", x, y, w, h, S)
    if fuerte:
        d.rectangle([x + 18, y + 16, x + w - 18, y + h - 16], fill=ACENTO)
    if icono:
        recorte_ui(img, icono, x + w // 2 - 32, y + h // 2 - 32, 2)
    if etiqueta:
        texto(d, (x + w // 2, y + h // 2 + 2), etiqueta, 32,
              fill=(56, 42, 20) if fuerte else TEXTO_OSCURO, anchor="mm", sombra=False)


# ══ partidas ═════════════════════════════════════════════════════════════════
PARTIDAS = [
    ("Valle del Río", "48 213", "día 12", [0, 1], ["nature/arbol_1", "agua/bote_abajo_1"]),
    ("La Meseta", "10 077", "día 3", [2], ["nature/piedra_1", "structure/boulder"]),
    ("Ciudad Baja", "77 412", "día 40", [0, 3, 4], ["edificio/local", "vehicle/auto"]),
]


def partidas(fondo):
    img = fondo.copy()
    d = ImageDraw.Draw(img)
    barra_estado(img, d, [(btn(7, 0), "PARTIDAS", "3 de 5"),
                          (btn(4, 0), "PERSONAJES", "5"),
                          (btn(0, 2), "ÚLTIMA", "hace 2 h")])

    pw, ph = 1800, 810
    pxu, pyu = (W - pw) // 2, 210
    marco(img, "panel", pxu, pyu, pw, ph, S)
    cartel(img, d, pxu + pw // 2, pyu - 48, 520, 96, "PARTIDAS", 48)

    ix, iy = pxu + 66, pyu + 60
    fila_h, sep = 186, 18
    for i, (nombre, semilla, dia, quienes, deco) in enumerate(PARTIDAS):
        y = iy + i * (fila_h + sep)
        marco(img, "slot", ix, y, pw - 132, fila_h, S)
        vin = vineta(300, fila_h - 40, deco, i + 3)
        img.alpha_composite(vin, (ix + 20, y + 20))
        d.rectangle([ix + 20, y + 20, ix + 20 + vin.width, y + 20 + vin.height],
                    outline=(120, 92, 66), width=3)
        texto(d, (ix + 350, y + 52), nombre, 44, fill=TEXTO_OSCURO, anchor="lm", sombra=False)
        texto(d, (ix + 350, y + 100), f"semilla {semilla} · {dia}", 28,
              fill=(140, 116, 92), anchor="lm", sombra=False)
        texto(d, (ix + 350, y + 140), "viven acá:", 26, fill=(140, 116, 92),
              anchor="lm", sombra=False)
        for j, q in enumerate(quienes):
            pj = PJ.personaje(LOOKS[q], PJ.FRENTE, escala=1)
            img.alpha_composite(pj, (ix + 500 + j * 46, y + 112))
        boton(img, d, ix + pw - 132 - 520, y + 46, 260, 92, "JUGAR", fuerte=(i == 0))
        boton(img, d, ix + pw - 132 - 230, y + 46, 92, 92, icono=TRASH)
        boton(img, d, ix + pw - 132 - 122, y + 46, 92, 92, icono=btn(4, 4))
    texto(d, (pxu + pw // 2, pyu + ph - 46),
          "La papelera pide confirmación y borra sólo esa partida — el personaje no se toca.",
          28, fill=(206, 186, 160), anchor="mm")
    return img


# ══ personajes ═══════════════════════════════════════════════════════════════
def personajes(fondo):
    img = fondo.copy()
    d = ImageDraw.Draw(img)
    barra_estado(img, d, [(btn(4, 0), "PERSONAJES", "5 de 8"),
                          (btn(7, 0), "PARTIDAS", "3"),
                          (btn(0, 2), "ELEGIDO", "Leo")])

    pw, ph = 2040, 810
    pxu, pyu = (W - pw) // 2, 210
    marco(img, "panel", pxu, pyu, pw, ph, S)
    cartel(img, d, pxu + pw // 2, pyu - 48, 560, 96, "PERSONAJES", 48)

    ix, iy = pxu + 60, pyu + 54
    cw, ch, sep = 372, 330, 18
    for i in range(6):
        x = ix + (i % 5) * (cw + sep) if i < 5 else 0
        y = iy
        if i == 5:
            continue
        marco(img, "slot", x, y, cw, ch, S)
        if i < len(LOOKS):
            pj = PJ.personaje(LOOKS[i], PJ.FRENTE, escala=4)
            img.alpha_composite(pj, (x + cw // 2 - pj.width // 2, y + 26))
            texto(d, (x + cw // 2, y + 296), NOMBRES[i], 38, fill=TEXTO_OSCURO,
                  anchor="mm", sombra=False)
            if i == 0:
                d.rectangle([x - 3, y - 3, x + cw - 3, y + ch - 3], outline=ACENTO, width=6)
    # segunda fila: acciones del elegido + hueco para uno nuevo
    y2 = iy + ch + 30
    marco(img, "slot", ix, y2, cw, ch - 40, S)
    texto(d, (ix + cw // 2, y2 + 60), "+", 92, fill=(150, 124, 98), anchor="mm", sombra=False)
    texto(d, (ix + cw // 2, y2 + 150), "NUEVO", 34, fill=TEXTO_OSCURO, anchor="mm", sombra=False)
    texto(d, (ix + cw // 2, y2 + 200), "quedan 3 huecos", 24, fill=(150, 124, 98),
          anchor="mm", sombra=False)

    px2 = ix + cw + sep
    marco(img, "panel_liso", px2, y2, pw - 120 - cw - sep, ch - 40, S)
    texto(d, (px2 + 40, y2 + 56), "LEO", 44, fill=TEXTO_OSCURO, anchor="lm", sombra=False)
    texto(d, (px2 + 40, y2 + 106), "Estuvo en Valle del Río y en Ciudad Baja.", 30,
          fill=(140, 116, 92), anchor="lm", sombra=False)
    texto(d, (px2 + 40, y2 + 150),
          "Un personaje no pertenece a un mundo: entra al que elijas.", 28,
          fill=(140, 116, 92), anchor="lm", sombra=False)
    bw = 260
    bx = px2 + (pw - 120 - cw - sep) - 60 - (bw * 3 + 36)
    for k, (et, fuerte, ic) in enumerate([("EDITAR", False, None), ("BORRAR", False, None),
                                          ("ELEGIR", True, None)]):
        boton(img, d, bx + k * (bw + 18), y2 + 180, bw, 92, et, ic, fuerte)
    return img


# ══ nueva partida ════════════════════════════════════════════════════════════
def nueva_partida(fondo):
    img = fondo.copy()
    d = ImageDraw.Draw(img)
    barra_estado(img, d, [(btn(7, 0), "PARTIDAS", "3 de 5"),
                          (btn(4, 0), "PERSONAJE", "Leo")])

    pw, ph = 1560, 780
    pxu, pyu = (W - pw) // 2, 222
    marco(img, "panel", pxu, pyu, pw, ph, S)
    cartel(img, d, pxu + pw // 2, pyu - 48, 620, 96, "NUEVA PARTIDA", 46)

    ix, iy = pxu + 66, pyu + 54
    inner = pw - 132

    texto(d, (ix, iy + 26), "NOMBRE", 28, fill=(150, 124, 98), anchor="lm", sombra=False)
    marco(img, "boton_ancho", ix, iy + 48, inner, 92, S)
    texto(d, (ix + 36, iy + 96), "Valle del Río", 40, fill=TEXTO_OSCURO,
          anchor="lm", sombra=False)

    texto(d, (ix, iy + 190), "SEMILLA", 28, fill=(150, 124, 98), anchor="lm", sombra=False)
    marco(img, "boton_ancho", ix, iy + 212, inner - 220, 92, S)
    texto(d, (ix + 36, iy + 260), "48 213", 40, fill=TEXTO_OSCURO, anchor="lm", sombra=False)
    boton(img, d, ix + inner - 200, iy + 212, 200, 92, "AL AZAR", icono=None)

    texto(d, (ix, iy + 354), "MODO", 28, fill=(150, 124, 98), anchor="lm", sombra=False)
    modos = [("SUPERVIVENCIA", "Se junta, se fabrica, se pasa hambre.", True),
             ("CREATIVO", "Todo el catálogo a mano y vuelo libre.", False)]
    mw = (inner - 24) // 2
    for k, (et, sub, elegido) in enumerate(modos):
        x = ix + k * (mw + 24)
        marco(img, "slot", x, iy + 386, mw, 150, S)
        texto(d, (x + 30, iy + 440), et, 40, fill=TEXTO_OSCURO, anchor="lm", sombra=False)
        texto(d, (x + 30, iy + 490), sub, 26, fill=(140, 116, 92), anchor="lm", sombra=False)
        if elegido:
            d.rectangle([x - 3, iy + 383, x + mw - 3, iy + 533], outline=ACENTO, width=6)
            recorte_ui(img, CHECK, x + mw - 84, iy + 410, 2)
    texto(d, (ix, iy + 566),
          "El modo se elige acá. Después, en el juego, se cambia con la consola.", 26,
          fill=(150, 124, 98), anchor="lm", sombra=False)
    boton(img, d, ix + inner - 420, iy + 596, 420, 96, "CREAR MUNDO", fuerte=True)
    return img


# ══ consola escondida ════════════════════════════════════════════════════════
def consola():
    from mock_hud import pantalla_hud
    img = pantalla_hud("libre")
    d = ImageDraw.Draw(img)
    img.alpha_composite(Image.new("RGBA", img.size, (12, 10, 20, 120)))

    cw, chh = 1500, 476
    cx, cy = (W - cw) // 2, 300
    marco(img, "panel", cx, cy, cw, chh, S)
    cartel(img, d, cx + cw // 2, cy - 48, 480, 92, "CONSOLA", 44)
    ix = cx + 60
    marco(img, "boton_ancho", ix, cy + 56, cw - 120, 92, S)
    texto(d, (ix + 34, cy + 104), "/creativo", 44, fill=TEXTO_OSCURO, anchor="lm", sombra=False)
    d.rectangle([ix + 258, cy + 76, ix + 264, cy + 132], fill=(120, 96, 74))
    for k, (cmd, expl) in enumerate([("/creativo", "prende o apaga el modo creativo"),
                                     ("/dar <ítem> <n>", "te pone algo en la mochila"),
                                     ("/ir <x> <z>", "te lleva a esa coordenada")]):
        y = cy + 176 + k * 68
        texto(d, (ix + 12, y + 26), cmd, 34, fill=TEXTO_OSCURO, anchor="lm", sombra=False)
        texto(d, (ix + 460, y + 28), expl, 28, fill=(140, 116, 92), anchor="lm", sombra=False)
    texto(d, (cx + cw // 2, cy + chh - 42),
          "Se destraba tocando siete veces el número de versión, en AJUSTES.", 28,
          fill=(206, 186, 160), anchor="mm")
    return img


if __name__ == "__main__":
    out = Path(sys.argv[1] if len(sys.argv) > 1 else ".")
    bosque = Fondo("bosque", 3).cuadro(6, 20)
    noche = Fondo("noche", 11).cuadro(4, 20)
    partidas(bosque).convert("RGB").save(out / "07-partidas.png")
    personajes(noche).convert("RGB").save(out / "07b-personajes.png")
    nueva_partida(bosque).convert("RGB").save(out / "08-nueva-partida.png")
    consola().convert("RGB").save(out / "09-consola.png")
    print("listo")
