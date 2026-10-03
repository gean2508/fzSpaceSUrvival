#!/usr/bin/env python3
"""**LA GRANJA CON VOLUMEN**: los mismos 23 slots de `dibujar-granja.py`, rehechos con `lib/volumen.py`.

Cada receta recibe un `Lienzo` de 32x32 y lo dibuja (la misma firma que las viejas), así que
`dibujar-granja.py` sólo cambia de dónde saca las funciones. Luz arriba-izquierda, cúpulas, tonos de la
paleta del pack, y el contorno de Interiors en los ítems de mochila.
"""
from __future__ import annotations

import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from lib.iconos import CELDA, _disco, silueta  # noqa: E402
from lib.pixel import Lienzo, rampa  # noqa: E402
from lib.volumen import blob, esculpir  # noqa: E402


def mascara(puntos) -> np.ndarray:
    t = Lienzo()
    t.poligono(puntos, (255, 255, 255))
    return np.array([[t.px[i, j][3] > 0 for i in range(CELDA)] for j in range(CELDA)])


def _moda_toroidal(idx: np.ndarray) -> np.ndarray:
    """Junta el ruido en manchas SIN romper la costura (la losa se repite)."""
    out = idx.copy()
    for j in range(CELDA):
        for i in range(CELDA):
            vec = [idx[(j + b) % CELDA, (i + a) % CELDA] for b in (-1, 0, 1) for a in (-1, 0, 1)]
            vals, c = np.unique(vec, return_counts=True)
            if c.max() >= 5:
                out[j, i] = vals[c.argmax()]
    return out


# ── suelo ───────────────────────────────────────────────────────────────────────────────────────

def tierra_arada(base, mojada=False):
    def f(l: Lienzo):
        t = rampa(base, 5)
        rng = np.random.default_rng(11 if not mojada else 12)
        ys = np.arange(CELDA)[:, None]
        # perfil de surco: 3 lomas por losa; la pendiente que mira a la luz (hacia arriba) es clara
        fase = 2 * np.pi * ys / (CELDA / 3.0)
        pendiente = -np.cos(fase)                       # >0 mira hacia arriba
        s = 0.5 + 0.38 * pendiente + 0.18 * (rng.random((CELDA, CELDA)) - 0.5)
        idx = np.clip((s * len(t)).astype(int), 0, len(t) - 1)
        idx = _moda_toroidal(idx)
        for j in range(CELDA):
            for i in range(CELDA):
                l.punto(i, j, t[int(idx[j, i])])
        if mojada:
            c = rampa((120, 150, 190), 3)
            for j in range(CELDA):
                if (j % 11) in (8, 9):                 # el agua se junta en el fondo del surco
                    for i in range(CELDA):
                        if (i * 7 + j * 3) % 5 < 2:
                            l.punto(i, j, c[1])
    return f, False


def sembrado(l: Lienzo):
    t = rampa((214, 188, 124), 4)
    for y in (5, 15, 25):
        for x in (5, 13, 21, 28):
            xx = x + (y // 5) % 3
            l.rect(xx, y, 2, 1, t[3])
            l.punto(xx, y + 1, t[1])
            l.punto(xx + 1, y + 1, t[0])


# ── cultivos maduros ────────────────────────────────────────────────────────────────────────────

def trigo_maduro(l: Lienzo):
    c = rampa((222, 176, 70), 5)
    tallo = rampa((150, 150, 70), 4)
    l.sombra_al_pie(4, 24, alto=3)
    for k, (x, alto, incl) in enumerate([(7, 21, -1), (12, 25, 0), (17, 23, 1), (22, 26, 0), (26, 20, 1)]):
        base_y = 29
        l.banda(x, base_y, x + incl, base_y - alto + 5, 1.3, tallo[2])
        # espiga: elipse alta con volumen, escamas de grano y aristas hacia arriba
        ex, ey = x + incl, base_y - alto
        esculpir(l, _disco(ex + 0.5, ey + 4, 2.6, 5.2), c, relieve=1.4)
        l.rect(ex - 1, ey + 1, 1, 6, c[4])                                        # el lado de la luz
        l.rect(ex + 1, ey + 3, 1, 4, c[1])                                        # el de la sombra
        for dx in (-1, 0, 1):
            l.linea(ex + dx, ey - 1, ex + dx * 2, ey - 5, c[3])                   # las aristas
    l.verificar(apoyado=True)


def zanahoria_madura(l: Lienzo):
    l.sombra_al_pie(3, 26, alto=3)
    naranja = rampa((236, 130, 36), 5)
    verde = rampa((90, 160, 70), 5)
    tierra = rampa((120, 84, 56), 4)
    for cx in (9, 16, 23):
        esculpir(l, _disco(cx, 25, 4.4, 3.6), naranja, relieve=1.2)        # el hombro asomando
        esculpir(l, _disco(cx, 28, 5.5, 2.2), tierra, relieve=0.8)         # la tierra alrededor
        for dx, alto in ((-2, 10), (0, 13), (2, 10)):
            l.poligono([(cx + dx - 1, 23), (cx + dx + 1, 23), (cx + dx + dx // 2, 23 - alto)], verde[3])
            l.linea(cx + dx - 1, 23, cx + dx + dx // 2, 23 - alto, verde[4])
    l.verificar(apoyado=True)


def calabaza(l: Lienzo, cx=16, cy=22, rx=11, ry=8, con_hojas=True):
    c = rampa((226, 126, 36), 5)
    esculpir(l, _disco(cx, cy, rx, ry), c, relieve=1.6, base_oscura=0.1)
    for dx in (-5, 0, 5):                                                      # las costillas
        l.banda(cx + dx, cy - ry + 2, cx + dx * 1.15, cy + ry - 1, 1.2, c[1])
    py = int(cy - ry) - 2
    l.rect(int(cx) - 1, py, 3, 3, (92, 120, 52))                              # el pedúnculo
    l.rect(int(cx) - 1, py, 1, 3, (130, 160, 80))
    if con_hojas:
        v = rampa((84, 150, 70), 4)
        l.poligono([(cx + rx - 2, cy - 2), (cx + rx + 3, cy - 6), (cx + rx + 2, cy + 1)], v[2])
        l.poligono([(cx - rx + 2, cy - 2), (cx - rx - 3, cy - 6), (cx - rx - 2, cy + 1)], v[3])


def calabaza_madura(l: Lienzo):
    l.sombra_al_pie(4, 24, alto=3)
    calabaza(l, 16, 22, 11, 7.5, True)
    l.verificar(apoyado=True)


# ── ítems de mochila ───────────────────────────────────────────────────────────────────────────

def icono_semillas(color):
    def f(l: Lienzo):
        t = rampa(color, 4)
        puntos = [(11, 22), (15, 21), (19, 22), (13, 25), (17, 25), (21, 25), (9, 25), (15, 18), (12, 19), (18, 19)]
        for x, y in puntos:
            l.rect(x, y, 3, 2, t[2])
            l.punto(x, y, t[3])
            l.punto(x + 2, y + 1, t[0])
        silueta(l)
    return f, False


def icono_trigo(l: Lienzo):
    c = rampa((222, 176, 70), 5)
    for x, y0 in ((9, 12), (13, 9), (17, 8), (21, 10), (25, 13)):
        l.banda(16 + (x - 17) * 0.4, 28, x, y0 + 8, 1.5, (156, 150, 72))
        esculpir(l, _disco(x + 0.5, y0 + 3, 2.4, 4.6), c, relieve=1.4)
        l.linea(x, y0 - 2, x + (x - 17) // 4, y0 - 5, c[3])
    l.rect(11, 21, 12, 2, (120, 84, 60))             # la atadura
    l.rect(11, 21, 12, 1, (160, 118, 82))
    silueta(l)


def icono_zanahoria(l: Lienzo):
    n = rampa((236, 130, 36), 5)
    m = np.zeros((CELDA, CELDA), bool)
    for j in range(10, 29):
        w = (29 - j) * 0.33 + 0.5
        for i in range(CELDA):
            if abs(i - 15 - (j - 10) * 0.2) <= w * 1.4:
                m[j, i] = True
    esculpir(l, m, n, relieve=1.2)
    for y in (14, 18, 22):                             # las marcas
        l.linea(12, y, 15, y, n[1])
    v = rampa((90, 160, 70), 5)
    for dx in (-3, 0, 3):
        l.poligono([(14 + dx, 11), (17 + dx, 11), (15 + dx * 2, 3)], v[3])
    silueta(l)


def icono_calabaza(l: Lienzo):
    calabaza(l, 16, 19, 10.5, 9, True)
    silueta(l)


def corazon(l: Lienzo):
    c = rampa((210, 70, 80), 5)
    m = mascara([(16, 28), (4, 14), (5, 8), (10, 5), (16, 9), (22, 5), (27, 8), (28, 14)])
    esculpir(l, m, c, relieve=1.6)
    l.rect(9, 9, 2, 2, (255, 214, 214))
    silueta(l)


def icono_pollo(l: Lienzo):
    carne = rampa((226, 140, 130), 5)
    m = blob(CELDA, CELDA, 14, 12, 8.5, 7, 5, 0.16)
    esculpir(l, m, carne, relieve=1.5, base_oscura=0.1)
    hueso = rampa((240, 232, 214), 4)
    l.banda(18, 18, 25, 25, 3, hueso[2])
    esculpir(l, _disco(26, 26, 2.6, 2.6), hueso, relieve=1.0)
    esculpir(l, _disco(23, 27, 2.4, 2.4), hueso, relieve=1.0)
    silueta(l)


def icono_conejo(l: Lienzo):
    carne = rampa((186, 100, 96), 5)
    m = blob(CELDA, CELDA, 14, 16, 9.5, 7, 9, 0.16)
    esculpir(l, m, carne, relieve=1.5, base_oscura=0.1)
    hueso = rampa((240, 232, 214), 4)
    l.banda(19, 20, 25, 25, 2.6, hueso[2])
    esculpir(l, _disco(26, 25, 2.2, 2.2), hueso, relieve=1.0)
    silueta(l)


def icono_pluma(l: Lienzo):
    c = rampa((236, 238, 244), 5)
    m = mascara([(6, 26), (10, 15), (18, 6), (26, 5), (25, 13), (18, 21), (10, 26)])
    esculpir(l, m, c, relieve=1.0)
    l.banda(6, 27, 24, 7, 1.4, rampa((170, 176, 190), 3)[1])      # el raquis
    silueta(l)


def icono_lana(l: Lienzo):
    c = rampa((240, 238, 228), 5)
    for cx, cy, r in [(11, 18, 5), (18, 15, 6), (22, 21, 5), (14, 23, 5), (17, 20, 5)]:
        esculpir(l, _disco(cx, cy, r, r), c, relieve=1.3)
    silueta(l)


def icono_hueso(l: Lienzo):
    c = rampa((240, 232, 214), 5)
    l.banda(9, 23, 23, 9, 3.8, c[3])
    l.banda(8, 22, 22, 8, 1.4, c[4])
    for (x, y) in [(7, 20), (10, 25), (22, 7), (25, 10)]:
        esculpir(l, _disco(x, y, 3.2, 3.2), c, relieve=1.1)
    silueta(l)


def icono_huevo(l: Lienzo):
    c = rampa((240, 224, 196), 5)
    m = np.array([[((i - 16) / 7.2) ** 2 + ((j - 17) / (10.0 if j < 17 else 8.6)) ** 2 <= 1 for i in range(CELDA)] for j in range(CELDA)])
    esculpir(l, m, c, relieve=1.6)
    l.rect(12, 11, 2, 3, (255, 252, 240))
    silueta(l)


def icono_etiqueta(l: Lienzo):
    c = rampa((228, 208, 150), 5)
    m = mascara([(8, 10), (22, 10), (27, 16), (22, 23), (8, 23)])
    esculpir(l, m, c, relieve=0.8)
    l.rect(11, 14, 10, 1, (120, 96, 70))
    l.rect(11, 18, 8, 1, (120, 96, 70))
    l.punto(23, 16, (60, 48, 40))
    l.banda(23, 16, 28, 8, 1.2, (150, 120, 80))
    silueta(l)


def cubo(contenido=None):
    def f(l: Lienzo):
        metal = rampa((150, 158, 170), 5)
        m = mascara([(7, 12), (25, 12), (22, 28), (10, 28)])
        esculpir(l, m, metal, relieve=1.0)
        l.rect(8, 16, 16, 2, metal[1])                 # aros
        l.rect(9, 23, 14, 2, metal[1])
        esculpir(l, _disco(16, 12, 9, 3), rampa(contenido, 5) if contenido else rampa((70, 70, 84), 4), relieve=1.0)
        l.banda(8, 12, 12, 3, 1.4, metal[3])
        l.banda(12, 3, 20, 3, 1.4, metal[3])
        l.banda(20, 3, 24, 12, 1.4, metal[3])
        silueta(l)
    return f, False
