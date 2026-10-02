#!/usr/bin/env python3
"""**ICONOS DE INTERFAZ Y MÁQUINAS CON VOLUMEN** (reemplazan los de `dibujar-ui.py` y `dibujar-mundo.py`)."""
from __future__ import annotations

import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from lib.granja_v2 import mascara  # noqa: E402
from lib.iconos import CELDA, _disco, silueta  # noqa: E402
from lib.pixel import Lienzo, rampa  # noqa: E402
from lib.volumen import esculpir  # noqa: E402


def _icono(l: Lienzo) -> Lienzo:
    silueta(l)
    l.verificar(apoyado=False)
    return l


def mochila():
    l = Lienzo()
    c = rampa((176, 112, 62), 5)
    esculpir(l, mascara([(8, 12), (11, 7), (21, 7), (24, 12), (25, 27), (7, 27)]), c, relieve=1.4)
    esculpir(l, mascara([(8, 12), (11, 7), (21, 7), (24, 12), (24, 16), (8, 16)]), rampa((150, 90, 50), 5), relieve=1.0)  # la solapa
    l.rect(12, 20, 8, 5, c[1])                                                                                          # el bolsillo
    l.rect(12, 20, 8, 1, c[4])
    l.rect(15, 15, 2, 3, (226, 196, 90))                                                                               # la hebilla
    return _icono(l)


def martillo_yunque():
    l = Lienzo()
    g = rampa((120, 126, 140), 5)
    l.poligono([(6, 22), (26, 22), (24, 18), (10, 18), (8, 20)], g[3])
    l.poligono([(10, 18), (24, 18), (26, 14), (14, 14), (12, 16)], g[4])
    l.rect(11, 22, 10, 5, g[2])
    l.rect(8, 27, 16, 2, g[1])
    m = rampa((150, 104, 62), 5)
    l.banda(9, 13, 20, 4, 2.4, m[2])
    esculpir(l, mascara([(17, 3), (25, 5), (24, 10), (16, 8)]), g, relieve=1.0)
    return _icono(l)


def casita():
    l = Lienzo()
    p = rampa((226, 200, 156), 5)
    t = rampa((190, 80, 60), 5)
    l.rect(7, 15, 18, 12, p[3])
    l.rect(7, 15, 18, 2, p[4])
    l.rect(23, 15, 2, 12, p[1])
    l.poligono([(4, 16), (16, 5), (28, 16)], t[3])
    l.poligono([(16, 5), (28, 16), (24, 16), (16, 9)], t[1])
    l.rect(14, 20, 5, 7, (110, 76, 52))
    l.rect(14, 20, 5, 1, (160, 116, 80))
    l.rect(9, 18, 4, 4, (140, 190, 220))
    return _icono(l)


def mira():
    l = Lienzo()
    c = (246, 246, 252)
    for x0, y0, x1, y1 in [(16, 4, 16, 11), (16, 21, 16, 28), (4, 16, 11, 16), (21, 16, 28, 16)]:
        l.linea(x0, y0, x1, y1, c)
    l.punto(16, 16, c)
    silueta(l, (40, 40, 70))
    l.verificar(apoyado=False)
    return l


def tierra():
    l = Lienzo()
    esculpir(l, _disco(16, 16, 12, 12), rampa((60, 120, 210), 5), relieve=1.6)
    cont = rampa((84, 160, 84), 5)
    l.poligono([(8, 10), (14, 8), (16, 13), (11, 17), (8, 15)], cont[3])
    l.poligono([(18, 16), (24, 14), (23, 22), (18, 24)], cont[2])
    l.poligono([(8, 10), (14, 8), (13, 10), (9, 12)], cont[4])
    l.rect(11, 9, 2, 2, (240, 248, 255))
    return _icono(l)


# ── máquinas (Exteriors: sin contorno, con sombra de contacto) ─────────────────────────────────

def _v(l):
    l.verificar(apoyado=True)
    return l


def extractor(material, herramienta):
    l = Lienzo()
    l.sombra_al_pie(3, 26, alto=3)
    m = rampa(material, 5)
    esculpir(l, mascara([(5, 14), (27, 14), (27, 28), (5, 28)]), m, relieve=0.8, base_oscura=0.15)
    l.rect(5, 14, 22, 3, m[4])
    l.rect(8, 19, 16, 6, (40, 36, 52))                           # la boca de la tolva
    l.rect(8, 19, 16, 1, (76, 70, 92))
    h = rampa(herramienta, 5)
    l.banda(16, 4, 16, 14, 2.4, h[2])
    l.poligono([(11, 3), (21, 3), (19, 7), (13, 7)], h[3])
    l.rect(11, 3, 10, 1, h[4])
    return _v(l)


def taladro():
    l = Lienzo()
    l.sombra_al_pie(4, 24, alto=3)
    m = rampa((150, 156, 170), 5)
    esculpir(l, mascara([(7, 18), (25, 18), (27, 28), (5, 28)]), m, relieve=1.0, base_oscura=0.15)
    l.rect(14, 4, 4, 15, m[2])
    l.rect(14, 4, 1, 15, m[4])
    l.poligono([(12, 20), (20, 20), (16, 28)], (210, 150, 60))
    l.rect(8, 21, 3, 3, (226, 70, 60))
    return _v(l)


def habitat():
    l = Lienzo()
    l.sombra_al_pie(3, 26, alto=3)
    c = rampa((236, 238, 246), 5)
    cuerpo = _disco(16, 24, 13, 15)
    cuerpo &= np.mgrid[0:CELDA, 0:CELDA][0] < 29
    esculpir(l, cuerpo, c, relieve=1.8)
    l.rect(13, 20, 6, 9, (86, 100, 130))                          # la puerta
    l.rect(13, 20, 6, 1, (140, 160, 200))
    l.rect(10, 12, 3, 3, (130, 190, 230))                         # ojos de buey
    l.rect(19, 12, 3, 3, (130, 190, 230))
    return _v(l)


def plataforma():
    l = Lienzo()
    l.sombra_al_pie(2, 28, alto=3)
    g = rampa((130, 134, 148), 5)
    l.poligono([(3, 18), (29, 18), (29, 28), (3, 28)], g[2])
    l.poligono([(3, 18), (29, 18), (26, 13), (6, 13)], g[4])
    l.rect(3, 18, 26, 1, g[4])
    for x in range(5, 28, 6):
        l.poligono([(x, 28), (x + 3, 28), (x + 6, 19), (x + 3, 19)], (226, 190, 60))   # franjas
    return _v(l)
