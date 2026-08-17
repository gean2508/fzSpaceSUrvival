"""Fondo de parallax en loop para las pantallas de menú.

Tres capas del mismo mundo a distinta velocidad + una capa de motas flotando.
El ciclo cierra exacto: cada capa recorre un múltiplo ENTERO del ancho en los N
cuadros, así el bucle no tiene salto.
"""

import random

from PIL import Image, ImageDraw

from uikit import sprite

W, H = 2400, 1080
S = 3


def _capa(props, cuantos, semilla, escala, y0, y1, sombra=0.0):
    """Una capa de ancho W que hace tile perfecto: lo que sale por la derecha se
    vuelve a pegar por la izquierda."""
    capa = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    rnd = random.Random(semilla)
    for _ in range(cuantos):
        im = sprite(rnd.choice(props))
        im = im.resize((im.width * escala, im.height * escala), Image.NEAREST)
        if sombra > 0:
            velo = Image.new("RGBA", im.size, (10, 8, 18, int(255 * sombra)))
            velo.putalpha(Image.eval(im.getchannel("A"), lambda a: int(a * sombra)))
            im = im.copy()
            im.alpha_composite(velo)
        x = rnd.randrange(0, W)
        y = rnd.randrange(y0, y1)
        capa.alpha_composite(im, (x, y))
        if x + im.width > W:                     # su mitad que asoma por la izquierda
            capa.alpha_composite(im, (x - W, y))
    return capa


def _suelo(clave="tiles/grass"):
    base = Image.new("RGBA", (W, H), (0, 0, 0, 255))
    t = 32 * S
    tile = sprite(clave).resize((t, t), Image.NEAREST)
    for y in range(0, H, t):
        for x in range(0, W, t):
            base.alpha_composite(tile, (x, y))
    return base


LEJOS = ["nature/arbusto_1", "nature/arbusto_10", "prop/cesped_cortado", "nature/piedra_1"]
MEDIO = ["nature/arbol_10", "nature/arbusto_1", "nature/piedra_1", "prop/farol_de_calle_1"]
CERCA = ["nature/arbol_1", "nature/arbol_100", "nature/arbol_103"]

NOCHE_MEDIO = ["prop/farol_de_calle_1", "prop/farol_de_calle_2", "nature/arbol_10",
               "nature/arbusto_1", "prop/farol"]

PLAYA_LEJOS = ["agua/playa_caracol_blanco_1", "agua/playa_caracol_dorado_1",
               "agua/playa_caracol_gris_1", "nature/piedra_1"]
PLAYA_MEDIO = ["agua/bote_abajo_1", "agua/boya_1", "agua/boya_4", "nature/arbusto_1"]
PLAYA_CERCA = ["nature/arbol_100", "nature/arbol_1"]


class Fondo:
    """Capas ya rasterizadas: se arman una vez y cada cuadro sólo las desplaza."""

    def __init__(self, tema="bosque", semilla=3):
        # El velo es lo que da la hora del día: el mismo bosque de noche es otro
        # lugar, y sale mucho mejor que cambiar el tile a arena (que queda barro).
        self.velo = {"bosque": (16, 12, 26, 112), "noche": (18, 24, 68, 150),
                     "playa": (16, 12, 26, 112)}.get(tema, (16, 12, 26, 112))
        self.luciernagas = tema == "noche"
        self.suelo = _suelo("tiles/sand" if tema == "playa" else "tiles/grass")
        if tema == "playa":
            self.l1 = _capa(PLAYA_LEJOS, 26, semilla, 2, -40, H)
            self.l2 = _capa(PLAYA_MEDIO, 18, semilla + 1, 3, -60, H - 100)
            self.l3 = _capa(PLAYA_CERCA, 7, semilla + 2, 5, -260, H - 320, sombra=0.32)
        elif tema == "noche":
            self.l1 = _capa(LEJOS, 30, semilla, 2, -40, H)
            self.l2 = _capa(NOCHE_MEDIO, 22, semilla + 1, 3, -60, H - 100)
            self.l3 = _capa(CERCA, 8, semilla + 2, 5, -300, H - 340, sombra=0.5)
        else:
            self.l1 = _capa(LEJOS, 34, semilla, 2, -40, H)
            self.l2 = _capa(MEDIO, 20, semilla + 1, 3, -60, H - 100)
            self.l3 = _capa(CERCA, 8, semilla + 2, 5, -300, H - 340, sombra=0.34)
        rnd = random.Random(semilla + 7)
        self.motas = [(rnd.randrange(0, W), rnd.randrange(0, H),
                       rnd.choice([3, 4, 6]), rnd.uniform(0.35, 0.9)) for _ in range(60)]

    def cuadro(self, t, n):
        """t de 0 a n-1. Las capas recorren 1, 2 y 3 anchos por ciclo."""
        img = self.suelo.copy()
        for capa, k in ((self.l1, 1), (self.l2, 2), (self.l3, 3)):
            dx = int(W * k * t / n) % W
            img.alpha_composite(capa, (-dx, 0))
            img.alpha_composite(capa, (W - dx, 0))
        img.alpha_composite(Image.new("RGBA", img.size, self.velo))
        d = ImageDraw.Draw(img)
        for i, (mx, my, r, v) in enumerate(self.motas):     # motas que suben
            y = int((my - H * 2 * t / n * v) % H)
            x = int((mx + 40 * (t / n)) % W)
            a = int(120 + 90 * ((i + t) % 5) / 5)
            col = (188, 255, 176, a) if self.luciernagas else (255, 238, 190, a)
            if self.luciernagas:
                d.ellipse([x - r, y - r, x + r * 2, y + r * 2], fill=(140, 220, 120, a // 4))
            d.ellipse([x, y, x + r, y + r], fill=col)
        return img
