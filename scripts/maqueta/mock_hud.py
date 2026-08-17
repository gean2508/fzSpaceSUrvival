#!/usr/bin/env python3
"""HUD en juego: el diseño que ya tenía el juego, pulido, y con los botones de
acción SÓLO cuando sirven.

Se conserva lo de siempre: corazones, barritas finas y el reloj con el dial del
cielo (`spawn_day_clock` + `spawn_sky_dial`). Cambian tres cosas:

1. cada barrita lleva su iconito, así se sabe cuál es sin memorizar el color;
2. la fila de patas se va — el hambre ya está en su barra, estaba dos veces;
3. los botones de acción aparecen cuando hay algo que hacer y si no, no están.
"""

import sys
from pathlib import Path

from PIL import Image, ImageDraw

sys.path.insert(0, str(Path(__file__).resolve().parent))
from uikit import ACENTO, TEXTO_OSCURO, icono_en, marco, recorte_ui, sprite, texto  # noqa: E402
from mock_inv import HOTBAR_SURVIVAL, MOCHILA, PITCH, S, SLOT, px, slot  # noqa: E402
from parallax import Fondo  # noqa: E402

W, H = 2400, 1080

AMBAR = (240, 161, 64)
AZUL_AGUA = (77, 166, 242)
ORO_KI = (255, 209, 56)


def pega(dst, clave, x, y, escala):
    im = sprite(clave)
    im = im.resize((int(im.width * escala), int(im.height * escala)), Image.NEAREST)
    dst.alpha_composite(im, (x, y))


def velo(dst, x, y, w, h, alfa=96, r=18):
    capa = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    ImageDraw.Draw(capa).rounded_rectangle([0, 0, w - 1, h - 1], r, fill=(14, 11, 20, alfa))
    dst.alpha_composite(capa, (x, y))


def gota(dst, x, y, lado, color):
    """La sed no tiene icono en el pack: esta gota es de las tres piezas que hay
    que dibujar. Va acá para ver si el tamaño alcanza."""
    d = ImageDraw.Draw(dst)
    d.ellipse([x, y + lado * .32, x + lado, y + lado], fill=color)
    d.polygon([(x + lado / 2, y), (x + lado * .88, y + lado * .62),
               (x + lado * .12, y + lado * .62)], fill=color)


def barra_fina(dst, x, y, w, h, pct, color):
    d = ImageDraw.Draw(dst)
    d.rectangle([x, y, x + w, y + h], fill=(24, 20, 32, 210), outline=(58, 58, 80), width=3)
    ancho = int((w - 6) * pct)
    d.rectangle([x + 3, y + 3, x + 3 + ancho, y + h - 3], fill=color)
    if ancho > 8:
        d.rectangle([x + 3 + ancho - 6, y + 3, x + 3 + ancho, y + h - 3],
                    fill=tuple(min(255, c + 40) for c in color))


def dial_del_cielo(dst, x, y, w, h, t=0.32):
    import math
    capa = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(capa)
    d.rounded_rectangle([0, 0, w - 1, h - 1], radius=w // 2, fill=(26, 41, 77))
    for sx, sy in [(0.16, 0.30), (0.34, 0.14), (0.52, 0.34), (0.70, 0.18), (0.84, 0.42)]:
        d.rectangle([int(sx * w), int(sy * h), int(sx * w) + 4, int(sy * h) + 4],
                    fill=(206, 214, 246, 120))
    ang = math.pi * (1 - t)
    ax, ay = w / 2 + math.cos(ang) * w * 0.38, h * 0.86 - math.sin(ang) * h * 0.66
    r = w * 0.11
    d.ellipse([ax - r, ay - r, ax + r, ay + r], fill=(255, 219, 107))
    d.ellipse([-w * 0.24, h * 0.62, w * 1.24, h * 1.9], fill=(51, 107, 56))
    borde = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    ImageDraw.Draw(borde).rounded_rectangle([0, 0, w - 1, h - 1], radius=w // 2,
                                            outline=(176, 118, 82), width=4)
    capa.alpha_composite(borde)
    dst.alpha_composite(capa, (x, y))


def estado(img, d):
    """Vida en corazones + tres barritas, cada una con su icono."""
    velo(img, 24, 24, 520, 190)
    for i in range(10):
        pega(img, "ui/heart_full" if i < 7 else "ui/heart_empty", 46 + i * 46, 40, 1.3)
    filas = [("ui/hunger_full", 0.62, AMBAR), (None, 0.45, AZUL_AGUA),
             ("ui/energy_full", 0.88, ORO_KI)]
    for j, (icono, pct, color) in enumerate(filas):
        y = 106 + j * 32
        if icono:
            pega(img, icono, 46, y - 4, 0.9)
        else:
            gota(img, 50, y - 2, 22, AZUL_AGUA)
        barra_fina(img, 84, y, 428, 18, pct, color)


def reloj(img, d):
    rw, rh = 672, 168
    rx, ry = W - 24 - rw, 24
    marco(img, "panel_liso", rx, ry, rw, rh, S)
    dial_del_cielo(img, rx + 30, ry + 30, 156, 108)
    texto(d, (rx + 214, ry + 58), "07:40", 54, fill=TEXTO_OSCURO, anchor="lm", sombra=False)
    texto(d, (rx + 380, ry + 60), "DÍA 12", 34, fill=(140, 116, 92), anchor="lm", sombra=False)
    texto(d, (rx + 214, ry + 104), "Bosque del río · 1.240, -318", 27,
          fill=(140, 116, 92), anchor="lm", sombra=False)
    d.rectangle([rx + 214, ry + 126, rx + rw - 40, ry + 134], fill=(150, 124, 98))
    d.rectangle([rx + 214, ry + 126, rx + 214 + int((rw - 254) * 0.32), ry + 134], fill=ACENTO)
    return rx, ry + rh


def hotbar(img, d, resaltado="Pico de piedra"):
    n = len(HOTBAR_SURVIVAL) + 1
    hb_w = n * PITCH - 4
    hbx = (800 - hb_w) // 2
    hby = 360 - 6 - SLOT
    for i, (clave, cant) in enumerate(HOTBAR_SURVIVAL):
        slot(img, hbx + i * PITCH, hby, clave, cant, sel=(i == 0), vacio=clave is None)
        texto(d, (px(hbx + i * PITCH) + 10, px(hby) + 22), str(i + 1), 22,
              fill=(120, 100, 86), anchor="lm", sombra=False)
    bagx = hbx + len(HOTBAR_SURVIVAL) * PITCH
    marco(img, "slot", px(bagx), px(hby), px(SLOT), px(SLOT), S)
    recorte_ui(img, MOCHILA, px(bagx) + 15, px(hby) + 18, 2)
    if resaltado:
        texto(d, (W // 2, px(hby) - 24), resaltado, 34, fill=ACENTO, anchor="mm")


def joystick(img, d):
    jcx, jcy, jr = 260, 850, 150
    d.ellipse([jcx - jr, jcy - jr, jcx + jr, jcy + jr], fill=(58, 58, 80, 130),
              outline=(176, 118, 82), width=9)
    d.ellipse([jcx - 66, jcy - 66, jcx + 66, jcy + 66], fill=(203, 180, 148),
              outline=(140, 100, 70), width=8)


def boton_accion(img, d, x, y, clave, etiqueta, lado=190):
    marco(img, "slot", x - lado // 2, y - lado // 2, lado, lado, S)
    icono_en(img, clave, x, y - 8, 104)
    texto(d, (x, y + lado // 2 - 26), etiqueta, 32, fill=TEXTO_OSCURO, anchor="mm", sombra=False)


def pantalla_hud(contexto="libre"):
    fondo = Fondo("bosque", 5)
    img = fondo.cuadro(0, 20)
    img.alpha_composite(Image.new("RGBA", img.size, (255, 240, 220, 40)))
    d = ImageDraw.Draw(img)

    if contexto == "arbol":
        # el objetivo bajo el dedo se marca en el MUNDO, no con un botón siempre puesto
        ax, ay = 1500, 470
        arbol = sprite("nature/arbol_1")
        arbol = arbol.resize((arbol.width * 5, arbol.height * 5), Image.NEAREST)
        img.alpha_composite(arbol, (ax - arbol.width // 2, ay - arbol.height // 2))
        base = ay + arbol.height // 2 - 24
        d.ellipse([ax - 140, base - 30, ax + 140, base + 30], outline=ACENTO, width=7)
        texto(d, (ax, base + 74), "Roble · 3 golpes", 32, fill=(245, 236, 214), anchor="mm")

    estado(img, d)
    reloj(img, d)
    joystick(img, d)

    if contexto == "arbol":
        boton_accion(img, d, W - 260, 800, "items/axe", "TALAR")
        hotbar(img, d, "Hacha de piedra")
    else:
        hotbar(img, d, "Pico de piedra")
    return img


def comparacion():
    """Las dos situaciones, una al lado de la otra."""
    a = pantalla_hud("libre").convert("RGB")
    b = pantalla_hud("arbol").convert("RGB")
    esc = 1180
    ah = esc * H // W
    out = Image.new("RGB", (esc * 2 + 60, ah + 76), (26, 24, 34))
    d = ImageDraw.Draw(out)
    for i, (im, et) in enumerate([(a, "sin nada cerca · no hay botones"),
                                  (b, "frente a un árbol · aparece TALAR")]):
        out.paste(im.resize((esc, ah), Image.LANCZOS), (20 + i * (esc + 20), 60))
        texto(d, (20 + i * (esc + 20) + esc // 2, 34), et, 32,
              fill=(206, 196, 216), anchor="mm", sombra=False)
    return out


if __name__ == "__main__":
    out = Path(sys.argv[1] if len(sys.argv) > 1 else ".")
    pantalla_hud("libre").convert("RGB").save(out / "03-hud-en-juego.png")
    pantalla_hud("arbol").convert("RGB").save(out / "03b-hud-accion.png")
    comparacion().save(out / "03c-hud-botones.png")
    print("listo")
