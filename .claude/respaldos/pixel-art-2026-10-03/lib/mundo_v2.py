#!/usr/bin/env python3
"""**LOS OBJETOS DEL MUNDO CON VOLUMEN** (familia Exteriors: sin contorno, sombra de contacto al pie).

Rehace con `lib/volumen.py` lo que `dibujar-mundo.py` dibujaba con elipses y rectángulos: troncos,
cactus, rocas lunar y marciana, juncos, caña de pescar, pozo, cofre, estantes y taxi. Cada función
devuelve un `Lienzo` de 32x32 apoyado al ras de la celda.
"""
from __future__ import annotations

import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from lib.granja_v2 import mascara  # noqa: E402
from lib.iconos import CELDA, _disco, textura_roca  # noqa: E402
from lib.pixel import Lienzo, rampa  # noqa: E402
from lib.volumen import blob, esculpir  # noqa: E402


def _v(l: Lienzo):
    l.verificar(apoyado=True)
    return l


def tronco():
    l = Lienzo()
    c = rampa((132, 88, 52), 5)
    l.sombra_al_pie(3, 26, alto=3)
    m = np.zeros((CELDA, CELDA), bool)
    m[16:28, 4:25] = True
    rng = np.random.default_rng(3)
    t = np.zeros((CELDA, CELDA), np.float32) + 128
    for y in range(CELDA):
        t[y, :] += rng.normal(0, 38) if rng.random() < 0.45 else 0       # vetas a lo largo
    esculpir(l, m, c, textura=t, fuerza_textura=0.5, relieve=0.9)
    esculpir(l, _disco(25, 22, 4.2, 6.0), rampa((226, 188, 128), 4), relieve=1.0)   # el corte
    l.banda(24, 22, 27, 22, 1.2, (150, 104, 62))
    return _v(l)


def cactus():
    l = Lienzo()
    v = rampa((74, 150, 86), 5)
    l.sombra_al_pie(8, 16, alto=3)
    cuerpo = np.zeros((CELDA, CELDA), bool)
    cuerpo[7:29, 12:20] = True
    cuerpo |= _disco(16, 8, 4.0, 3.2)
    brazo_i = mascara([(12, 18), (6, 18), (6, 10), (9, 10), (9, 15), (12, 15)])
    brazo_d = mascara([(20, 15), (26, 15), (26, 7), (23, 7), (23, 12), (20, 12)])
    esculpir(l, cuerpo | brazo_i | brazo_d, v, relieve=1.5)
    for x in (14, 17):                                    # las costillas
        l.rect(x, 9, 1, 19, v[1])
    for (x, y) in [(13, 12), (18, 16), (14, 22), (7, 12), (24, 9)]:
        l.punto(x, y, (232, 232, 200))                     # espinas
    l.punto(16, 5, (232, 120, 150))                        # la flor
    l.punto(15, 5, (250, 170, 190))
    return _v(l)


def roca(base, semilla, crateres=True):
    l = Lienzo()
    mask = blob(CELDA, CELDA, 16, 21, 10.5, 8.5, semilla, 0.2)
    tex = np.tile(textura_roca(), (2, 2))[:CELDA, :CELDA]
    l.sombra_al_pie(5, 22, y=27, alto=3)
    esculpir(l, mask, rampa(base, 5), textura=tex, fuerza_textura=0.55, base_oscura=0.18, relieve=1.5)
    if crateres:
        c = rampa(base, 5)
        for (x, y) in [(12, 19), (20, 23)]:
            l.rect(x, y, 3, 2, c[1])
            l.rect(x, y, 3, 1, c[0])
    return _v(l)


def junco():
    l = Lienzo()
    l.sombra_al_pie(6, 20, alto=3)
    verde = rampa((96, 150, 70), 5)
    marron = rampa((150, 96, 52), 5)
    for k, (x, alto) in enumerate([(9, 20), (15, 25), (21, 18)]):
        l.banda(x, 29, x + (k - 1), 29 - alto + 6, 1.8, verde[2])
        l.banda(x - 1, 29, x + (k - 1) - 1, 29 - alto + 6, 0.8, verde[4])
        esculpir(l, _disco(x + (k - 1) + 0.5, 29 - alto + 3, 2.2, 4.2), marron, relieve=1.3)   # la espiga
    for x0, x1 in [(6, 4), (24, 27)]:
        l.poligono([(x0, 29), (x0 + 1, 29), (x1, 14)], verde[3])
    return _v(l)


def cana():
    l = Lienzo()
    b = rampa((196, 156, 84), 5)
    l.banda(6, 28, 24, 5, 2.2, b[2])
    l.banda(5, 27, 23, 4, 0.9, b[4])
    for t in (0.28, 0.5, 0.72):                                  # los nudos
        x, y = 6 + 18 * t, 28 - 23 * t
        l.rect(int(x) - 1, int(y), 3, 1, b[0])
    l.linea(24, 5, 26, 20, (214, 214, 224))                      # el hilo
    esculpir(l, _disco(26, 22, 2.2, 2.2), rampa((226, 64, 64), 4), relieve=1.0)    # el flotador
    l.punto(25, 21, (255, 255, 255))
    l.verificar(apoyado=False)                                    # va en la mano: no se apoya
    return l


def pozo():
    l = Lienzo()
    l.sombra_al_pie(4, 24, alto=3)
    piedra = rampa((130, 128, 140), 5)
    madera = rampa((140, 98, 58), 5)
    tex = np.tile(textura_roca(), (2, 2))[:CELDA, :CELDA]
    aro = _disco(16, 23, 11.5, 6.0)
    esculpir(l, aro, piedra, textura=tex, fuerza_textura=0.5, relieve=1.2)
    esculpir(l, _disco(16, 22, 8.0, 3.6), rampa((34, 40, 60), 4), relieve=0.8)       # el agua negra
    l.rect(7, 8, 2, 14, madera[2])                                                    # postes
    l.rect(23, 8, 2, 14, madera[1])
    l.rect(7, 8, 1, 14, madera[4])
    l.poligono([(5, 9), (16, 3), (27, 9), (25, 11), (16, 6), (7, 11)], madera[3])    # el techito
    l.poligono([(16, 3), (27, 9), (25, 11), (16, 6)], madera[1])
    l.banda(16, 10, 16, 17, 1.0, (190, 190, 200))                                    # soga
    return _v(l)


def cofre():
    l = Lienzo()
    l.sombra_al_pie(3, 26, alto=3)
    m = rampa((150, 100, 56), 5)
    metal = rampa((196, 176, 90), 4)
    # cuerpo
    l.poligono([(5, 16), (27, 16), (27, 28), (5, 28)], m[2])
    l.poligono([(27, 16), (27, 28), (24, 28), (24, 16)], m[1])
    # tapa abovedada
    esculpir(l, mascara([(5, 16), (6, 10), (16, 6), (26, 10), (27, 16)]), m, relieve=1.3)
    # flejes y cerradura
    l.rect(5, 18, 22, 2, metal[2])
    l.rect(5, 18, 22, 1, metal[3])
    l.rect(14, 15, 4, 7, metal[2])
    l.rect(15, 18, 2, 3, (60, 44, 34))
    l.rect(14, 15, 4, 1, metal[3])
    return _v(l)


def estante(semilla=0):
    l = Lienzo()
    l.sombra_al_pie(4, 24, alto=3)
    m = rampa((140, 96, 58), 5)
    l.rect(5, 4, 22, 25, m[2])
    l.rect(5, 4, 22, 2, m[4])
    l.rect(26, 4, 1, 25, m[0])
    for y0 in (7, 17):
        l.rect(7, y0, 18, 9, (60, 42, 34))                                          # el fondo
        x = 7
        libros = [(196, 66, 58), (70, 120, 190), (226, 180, 70), (88, 160, 96), (140, 80, 160), (226, 120, 60)]
        k = semilla
        while x < 24:
            w = 2 + (k % 2)
            c = rampa(libros[k % len(libros)], 4)
            alto = 7 + (k % 3 == 0)
            l.rect(x, y0 + 9 - alto, w, alto, c[2])
            l.rect(x, y0 + 9 - alto, 1, alto, c[3])
            l.rect(x + w - 1, y0 + 9 - alto, 1, alto, c[1])
            x += w
            k += 1
        l.rect(6, y0 + 9, 20, 1, m[3])                                              # la tabla
    return _v(l)


def taxi():
    l = Lienzo()
    l.sombra_al_pie(2, 28, alto=3)
    a = rampa((236, 186, 54), 5)
    cuerpo = mascara([(3, 16), (6, 9), (26, 9), (29, 16), (29, 25), (3, 25)])
    esculpir(l, cuerpo, a, relieve=1.0, base_oscura=0.2)
    vidrio = rampa((140, 190, 220), 4)
    l.poligono([(8, 11), (24, 11), (26, 16), (6, 16)], vidrio[2])
    l.poligono([(8, 11), (16, 11), (14, 16), (6, 16)], vidrio[3])
    l.rect(13, 6, 6, 3, (236, 236, 236))                                              # el cartel de techo
    l.rect(13, 6, 6, 1, (255, 255, 255))
    rueda = rampa((60, 60, 72), 4)
    for cx in (9, 23):
        esculpir(l, _disco(cx, 26, 3.6, 3.6), rueda, relieve=1.0)
    l.rect(4, 20, 3, 2, (255, 240, 170))                                              # faros
    l.rect(25, 20, 3, 2, (230, 80, 70))
    return _v(l)
