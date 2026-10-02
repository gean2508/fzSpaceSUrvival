#!/usr/bin/env python3
"""**ÍCONOS DE MOCHILA CON EL IDIOMA DE MODERN INTERIORS**: contorno oscuro de 1 px, volumen por
bandas de tono con luz arriba-izquierda y relieve de la textura real del pack.

Cada generador devuelve un `Lienzo` de 32x32 centrado (los ítems de mochila no se apoyan en el suelo).
Lo usan `dibujar-materias.py` y `dibujar-minerales.py`.
"""
from __future__ import annotations

import random
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from lib.pixel import Lienzo, ajustar, luz, rampa  # noqa: E402
from lib.volumen import blob, esculpir, motas  # noqa: E402

CELDA = 32
TINTA = (44, 36, 62)  # el contorno de Interiors: azul-violeta oscuro, jamás negro


def textura_roca() -> np.ndarray:
    import json
    from PIL import Image

    raiz = Path(__file__).resolve().parent.parent.parent
    a = raiz / "native/crates/ha-game/assets"
    r = json.loads((a / "art/manifest.json").read_text())["sprites"]["tiles/roca"]["rect"]
    hoja = Image.open(a / "art/pack/suelo.png").convert("RGB")
    t = hoja.crop((r["col"] * 32, r["row"] * 32, (r["col"] + 1) * 32, (r["row"] + 1) * 32))
    return (np.asarray(t).astype(np.float32) * [0.299, 0.587, 0.114]).sum(-1)


def silueta(l: Lienzo, tinta=TINTA):
    """Contorno de 1 px por fuera de TODA la silueta (el de Interiors)."""
    t = ajustar(tinta)
    marca = []
    for j in range(l.h):
        for i in range(l.w):
            if l.px[i, j][3] == 255:
                continue
            for a, b in ((i + 1, j), (i - 1, j), (i, j + 1), (i, j - 1)):
                if 0 <= a < l.w and 0 <= b < l.h and l.px[a, b][3] == 255:
                    marca.append((i, j))
                    break
    for i, j in marca:
        l.px[i, j] = (*t, 255)


def bulto(base, semilla=1, rx=9.0, ry=7.5, textura=True, motas_color=None, n_motas=0, relieve=1.4, fuerza=0.5):
    """Un terrón/piedra con volumen (arcilla, carbón, caliza, mena…)."""
    l = Lienzo()
    mask = blob(CELDA, CELDA, 16, 17, rx, ry, semilla, rugosidad=0.2)
    tex = np.tile(textura_roca(), (2, 2))[:CELDA, :CELDA] if textura else None
    esculpir(l, mask, rampa(base, 5), textura=tex, fuerza_textura=fuerza, base_oscura=0.16, relieve=relieve)
    if motas_color:
        motas(l, mask, motas_color, n_motas, semilla * 3 + 1, tam=(1, 1))
    silueta(l)
    l.verificar(apoyado=False)
    return l


def lingote(base):
    """Un lingote trapezoidal: cara de arriba clara, frente media, lado derecho oscuro."""
    c = rampa(base, 5)
    l = Lienzo()
    # tapa (paralelogramo), frente (rectángulo) y costado (paralelogramo)
    l.poligono([(8, 13), (22, 13), (26, 9), (12, 9)], c[4])
    l.poligono([(8, 13), (22, 13), (22, 22), (8, 22)], c[3])
    l.poligono([(22, 13), (26, 9), (26, 18), (22, 22)], c[1])
    l.linea(9, 14, 21, 14, c[4])      # filo de la tapa
    l.rect(10, 17, 8, 1, c[2])        # marca de molde
    l.punto(12, 10, (255, 255, 255))   # brillo
    silueta(l)
    l.verificar(apoyado=False)
    return l


def ladrillo(base):
    c = rampa(base, 5)
    l = Lienzo()
    l.poligono([(6, 14), (23, 14), (27, 10), (10, 10)], c[4])
    l.poligono([(6, 14), (23, 14), (23, 23), (6, 23)], c[3])
    l.poligono([(23, 14), (27, 10), (27, 19), (23, 23)], c[1])
    l.rect(6, 18, 17, 1, c[1])        # junta
    l.rect(14, 14, 1, 4, c[1])
    l.rect(10, 19, 1, 4, c[1])
    silueta(l)
    l.verificar(apoyado=False)
    return l


def esquirla(base):
    """Un trozo de vidrio: polígono facetado con brillo."""
    c = rampa(base, 5)
    l = Lienzo()
    l.poligono([(8, 20), (13, 7), (21, 10), (25, 22), (15, 25)], c[3])
    l.poligono([(13, 7), (21, 10), (15, 17)], c[4])
    l.poligono([(21, 10), (25, 22), (15, 17)], c[2])
    l.poligono([(8, 20), (15, 25), (15, 17)], c[1])
    l.punto(14, 10, (255, 255, 255))
    l.punto(15, 11, (255, 255, 255))
    silueta(l)
    l.verificar(apoyado=False)
    return l


def cubos(base, semilla=2):
    """Granos de sal: racimo de cubitos con cara clara arriba y oscura a la derecha."""
    c = rampa(base, 5)
    rng = random.Random(semilla)
    l = Lienzo()
    pos = [(7, 15, 8), (16, 12, 9), (12, 19, 7), (20, 19, 6)]
    for x, y, s in pos:
        l.rect(x, y + 2, s, s - 1, c[3])
        l.rect(x, y, s, 2, c[4])
        l.rect(x + s - 1, y + 2, 1, s - 1, c[1])
        l.punto(x + 1, y, (255, 255, 255))
    silueta(l)
    l.verificar(apoyado=False)
    return l


def gota(base):
    """Una gota de resina/ámbar: forma de lágrima con brillo."""
    c = rampa(base, 5)
    l = Lienzo()
    mask = np.zeros((CELDA, CELDA), bool)
    for j in range(CELDA):
        for i in range(CELDA):
            # lágrima: círculo abajo, punta arriba
            if (i - 16) ** 2 / 36 + (j - 20) ** 2 / 36 <= 1 or (abs(i - 16) <= (j - 6) * 0.45 and 6 <= j <= 20):
                mask[j, i] = True
    esculpir(l, mask, c, relieve=1.3, suavizar=1)
    l.rect(12, 19, 2, 3, (255, 244, 214))
    silueta(l)
    l.verificar(apoyado=False)
    return l


def ramitas(base):
    """Carbón vegetal: tres palitos negros con punta lustrosa y vetas."""
    c = rampa(base, 6)
    l = Lienzo()
    for (x0, y0, x1, y1) in [(8, 22, 22, 9), (11, 26, 26, 15), (7, 16, 17, 7)]:
        l.banda(x0, y0, x1, y1, 5.0, c[2])
        l.banda(x0 - 1, y0 - 1, x1 - 1, y1 - 1, 1.8, c[4])
        l.banda(x0 + 1, y0 + 1, x1 + 1, y1 + 1, 1.4, c[0])
        l.punto((x0 + x1) // 2, (y0 + y1) // 2, c[5])
        l.punto((x0 + x1) // 2 + 2, (y0 + y1) // 2 - 2, c[3])
    silueta(l)
    l.verificar(apoyado=False)
    return l


def polvo(base, semilla=3):
    """Un montoncito de polvo/aglutinante: cono bajo, ancho, con granos."""
    l = Lienzo()
    mask = blob(CELDA, CELDA, 16, 20, 11, 6.5, semilla, rugosidad=0.12)
    esculpir(l, mask, rampa(base, 5), relieve=1.2, base_oscura=0.1)
    silueta(l)
    l.verificar(apoyado=False)
    return l


# ── comida, madera y frascos ────────────────────────────────────────────────────────────────────


def _disco(cx, cy, rx, ry):
    ys, xs = np.mgrid[0:CELDA, 0:CELDA]
    return ((xs + 0.5 - cx) / rx) ** 2 + ((ys + 0.5 - cy) / ry) ** 2 <= 1.0


def naranja():
    l = Lienzo()
    esculpir(l, _disco(16, 18, 9.5, 9.0), rampa((232, 140, 40), 5), relieve=1.6)
    l.poligono([(16, 8), (22, 5), (24, 8), (19, 10)], (84, 150, 70))   # hoja
    l.punto(16, 9, (92, 60, 40))                                          # pedúnculo
    l.rect(12, 14, 2, 2, (255, 214, 140))                                  # brillo
    silueta(l)
    l.verificar(apoyado=False)
    return l


def uvas():
    l = Lienzo()
    base = rampa((118, 70, 150), 5)
    for cx, cy in [(11, 13), (17, 12), (22, 15), (13, 19), (19, 20), (16, 25), (24, 22)]:
        m = _disco(cx, cy, 4.2, 4.2)
        esculpir(l, m, base, relieve=1.4)
        l.punto(cx - 1, cy - 1, (222, 196, 240))
    l.banda(16, 6, 17, 11, 2, (96, 70, 44))
    l.poligono([(17, 6), (23, 4), (22, 8)], (84, 150, 70))
    silueta(l)
    l.verificar(apoyado=False)
    return l


def pan():
    l = Lienzo()
    m = blob(CELDA, CELDA, 16, 18, 12, 7.5, 3, 0.1)
    esculpir(l, m, rampa((200, 140, 70), 5), relieve=1.5, base_oscura=0.14)
    c = rampa((236, 196, 124), 4)
    for x0 in (9, 14, 19):                      # los tajos de la corteza
        l.banda(x0, 16, x0 + 3, 12, 1.6, c[3])
    silueta(l)
    l.verificar(apoyado=False)
    return l


def carne():
    l = Lienzo()
    m = blob(CELDA, CELDA, 16, 17, 10.5, 8, 7, 0.16)
    esculpir(l, m, rampa((178, 68, 62), 5), relieve=1.5, base_oscura=0.12)
    grasa = rampa((246, 218, 208), 3)
    l.banda(9, 21, 20, 12, 2, grasa[2])        # veta de grasa
    l.punto(21, 13, grasa[2])
    silueta(l)
    l.verificar(apoyado=False)
    return l


def tarta():
    l = Lienzo()
    esculpir(l, _disco(16, 19, 12, 7.5), rampa((196, 138, 74), 5), relieve=1.3)
    esculpir(l, _disco(16, 18, 8.5, 5), rampa((184, 52, 66), 5), relieve=1.2)
    c = rampa((232, 188, 120), 3)
    l.banda(9, 16, 22, 21, 1.4, c[2])
    l.banda(9, 21, 22, 16, 1.4, c[2])
    silueta(l)
    l.verificar(apoyado=False)
    return l


def sopa():
    l = Lienzo()
    esculpir(l, _disco(16, 20, 12, 8), rampa((236, 226, 206), 5), relieve=1.5)
    esculpir(l, _disco(16, 18, 9.5, 5), rampa((220, 120, 40), 5), relieve=1.1)
    l.punto(12, 17, (255, 230, 150))
    l.punto(20, 19, (255, 230, 150))
    for x, y in [(13, 9), (17, 7), (21, 9)]:  # vapor
        l.punto(x, y, (232, 232, 240))
        l.punto(x, y + 1, (214, 214, 226))
    silueta(l)
    l.verificar(apoyado=False)
    return l


def tronco():
    l = Lienzo()
    c = rampa((132, 88, 52), 5)
    corteza = textura_corteza()
    m = np.zeros((CELDA, CELDA), bool)
    m[11:24, 5:26] = True
    esculpir(l, m, c, textura=corteza, fuerza_textura=0.5, relieve=0.9)
    esculpir(l, _disco(25, 17, 4.2, 6.6), rampa((226, 188, 128), 4), relieve=1.0)   # el corte
    l.banda(23, 17, 27, 17, 1.2, (150, 104, 62))
    silueta(l)
    l.verificar(apoyado=False)
    return l


def textura_corteza() -> np.ndarray:
    rng = np.random.default_rng(3)
    t = np.zeros((CELDA, CELDA), np.float32) + 128
    for x in range(CELDA):                       # vetas verticales
        t[:, x] += rng.normal(0, 40) if rng.random() < 0.4 else 0
    t += rng.normal(0, 14, (CELDA, CELDA))
    return t


def palos():
    l = Lienzo()
    c = rampa((140, 98, 58), 5)
    for (x0, y0, x1, y1, tono) in [(7, 24, 25, 9, 2), (9, 9, 24, 25, 3)]:
        l.banda(x0, y0, x1, y1, 4.6, c[tono])
        l.banda(x0 - 1, y0 - 1, x1 - 1, y1 - 1, 1.6, c[4])
        l.banda(x0 + 1, y0 + 1, x1 + 1, y1 + 1, 1.4, c[0])
        mx, my = (x0 + x1) // 2, (y0 + y1) // 2
        l.rect(mx - 1, my - 1, 3, 2, c[1])                          # un nudo
    silueta(l)
    l.verificar(apoyado=False)
    return l


def bola_de_nieve():
    l = Lienzo()
    esculpir(l, _disco(16, 18, 9.5, 8.5), rampa((226, 236, 246), 5), relieve=1.6)
    l.punto(12, 14, (255, 255, 255))
    l.punto(13, 14, (255, 255, 255))
    silueta(l)
    l.verificar(apoyado=False)
    return l


def cuero():
    l = Lienzo()
    m = blob(CELDA, CELDA, 16, 17, 11, 9, 4, 0.28)
    esculpir(l, m, rampa((170, 112, 66), 5), relieve=0.8, base_oscura=0.1)
    l.punto(12, 14, (96, 60, 40))
    l.punto(21, 19, (96, 60, 40))
    silueta(l)
    l.verificar(apoyado=False)
    return l


def frasco(liquido):
    l = Lienzo()
    vidrio = rampa((196, 226, 236), 4)
    cuerpo = _disco(16, 20, 8.5, 7.5)
    esculpir(l, cuerpo, vidrio, relieve=1.4)
    liq = _disco(16, 22, 6.5, 5.0)
    esculpir(l, liq, rampa(liquido, 5), relieve=1.0)
    l.rect(14, 9, 5, 6, vidrio[2])                # cuello
    l.rect(14, 9, 1, 6, vidrio[3])
    l.rect(13, 6, 7, 3, (156, 112, 70))           # corcho
    l.rect(13, 6, 7, 1, (200, 150, 98))
    l.punto(12, 17, (255, 255, 255))
    silueta(l)
    l.verificar(apoyado=False)
    return l


# ── herramientas ────────────────────────────────────────────────────────────────────────────────


def _mango(l: Lienzo, madera=(150, 104, 62)):
    c = rampa(madera, 5)
    l.banda(7, 27, 22, 12, 3.4, c[2])
    l.banda(6, 26, 21, 11, 1.3, c[4])        # filo claro de la luz
    l.banda(8, 28, 23, 13, 1.1, c[0])        # lado en sombra


def hacha(cabeza, madera=(150, 104, 62)):
    l = Lienzo()
    _mango(l, madera)
    c = rampa(cabeza, 5)
    l.poligono([(15, 4), (24, 5), (28, 12), (23, 16), (19, 14), (18, 9)], c[3])      # hoja
    l.poligono([(15, 4), (24, 5), (21, 8), (18, 9)], c[4])                              # cara de luz
    l.poligono([(28, 12), (23, 16), (19, 14), (22, 11)], c[1])                          # cara de sombra
    l.linea(24, 5, 28, 12, c[4])                                                         # filo
    silueta(l)
    l.verificar(apoyado=False)
    return l


def pico(cabeza, madera=(150, 104, 62)):
    l = Lienzo()
    _mango(l, madera)
    c = rampa(cabeza, 5)
    l.poligono([(6, 11), (11, 5), (19, 3), (27, 7), (29, 14), (25, 13), (22, 8), (16, 7), (11, 11)], c[3])
    l.poligono([(11, 5), (19, 3), (27, 7), (22, 8), (16, 7)], c[4])
    l.poligono([(29, 14), (25, 13), (22, 8), (27, 7)], c[1])
    l.punto(14, 5, (255, 255, 255))
    silueta(l)
    l.verificar(apoyado=False)
    return l


def antorcha():
    l = Lienzo()
    c = rampa((150, 104, 62), 5)
    l.banda(12, 28, 19, 12, 3.8, c[2])
    l.banda(11, 27, 18, 11, 1.4, c[4])
    l.banda(13, 29, 20, 13, 1.2, c[0])
    # llama: gota naranja con núcleo amarillo
    esculpir(l, _disco(21, 9, 5.0, 6.0), rampa((240, 120, 40), 5), relieve=1.2)
    esculpir(l, _disco(21, 10, 2.6, 3.6), rampa((255, 220, 110), 4), relieve=1.0)
    silueta(l)
    l.verificar(apoyado=False)
    return l


def escudo():
    l = Lienzo()
    c = rampa((150, 104, 62), 5)
    cuerpo = [(7, 6), (25, 6), (25, 18), (16, 28), (7, 18)]
    l.poligono(cuerpo, c[3])
    l.poligono([(7, 6), (25, 6), (25, 9), (7, 9)], c[4])
    l.poligono([(16, 6), (25, 6), (25, 18), (16, 28)], c[2])
    l.rect(15, 8, 2, 18, c[1])                    # tabla central
    l.rect(8, 14, 16, 2, c[1])
    l.punto(16, 15, (214, 214, 224))              # remache
    silueta(l)
    l.verificar(apoyado=False)
    return l
