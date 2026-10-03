#!/usr/bin/env python3
"""**LO ÚLTIMO QUE QUEDABA «HECHO EN PAINT»**: manos, mochila de vuelo, sonda, piezas de nave, gemas, iconos
de acción y de menú. Todo con cúpula iluminada desde arriba-izquierda (`lib/volumen.py`), contorno de Interiors
y tonos de la paleta del pack."""
from __future__ import annotations

import math
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from lib.granja_v2 import mascara  # noqa: E402
from lib.iconos import CELDA, _disco, ladrillo, silueta  # noqa: E402
from lib.pixel import Lienzo, rampa  # noqa: E402
from lib.volumen import esculpir  # noqa: E402

ACERO = (150, 158, 172)
ROJO = (196, 70, 60)
PIEL = (226, 168, 132)


def _fin(l: Lienzo, apoyado=False) -> Lienzo:
    silueta(l)
    if apoyado:  # el objeto se apoya al ras del borde de abajo, como en el pack
        caja = l.im.getbbox()
        if caja and caja[3] < CELDA:
            from PIL import Image
            nuevo = Image.new("RGBA", l.im.size, (0, 0, 0, 0))
            nuevo.paste(l.im, (0, CELDA - caja[3]))
            l.im = nuevo
            l.px = l.im.load()
    l.verificar(apoyado=apoyado)
    return l


def _cilindro(l, x0, y0, x1, y1, tonos, rad=2):
    """Rectángulo redondeado con relieve de cilindro (luz arriba-izquierda)."""
    m = np.zeros((CELDA, CELDA), bool)
    m[y0:y1, x0:x1] = True
    for a, b in ((x0, y0), (x1 - 1, y0), (x0, y1 - 1), (x1 - 1, y1 - 1)):
        for k in range(rad):
            for q in range(rad - k):
                m[b + (q if b == y0 else -q), a + (k if a == x0 else -k)] = False
    esculpir(l, m, tonos, relieve=1.3, base_oscura=0.1)


def mano():
    l = Lienzo()
    c = rampa(PIEL, 5)
    esculpir(l, mascara([(9, 17), (22, 17), (23, 26), (10, 28)]), c, relieve=1.3)                     # palma
    for x, alto in [(9, 8), (13, 5), (17, 5), (21, 8)]:                                                # dedos
        esculpir(l, mascara([(x, 18), (x + 3, 18), (x + 3, alto + 2), (x, alto + 2)]), c, relieve=1.0)
    esculpir(l, mascara([(7, 20), (10, 18), (11, 24), (8, 25)]), c, relieve=0.9)                       # pulgar
    for x in (12, 16, 20):
        l.rect(x, 12, 1, 5, c[1])                                                                      # hendiduras
    return _fin(l)


def jetpack():
    l = Lienzo()
    a = rampa(ACERO, 5)
    n = rampa((235, 130, 40), 5)
    _cilindro(l, 6, 5, 14, 22, a)
    _cilindro(l, 18, 5, 26, 22, a)
    l.rect(6, 8, 8, 2, rampa(ROJO, 4)[2])
    l.rect(18, 8, 8, 2, rampa(ROJO, 4)[2])
    l.rect(13, 12, 6, 3, a[1])                                                                          # soporte
    for x in (7, 19):
        l.poligono([(x, 22), (x + 6, 22), (x + 5, 25), (x + 1, 25)], a[1])                              # tobera
        l.poligono([(x + 1, 25), (x + 5, 25), (x + 3, 30)], n[3])                                       # llama
        l.poligono([(x + 2, 25), (x + 4, 25), (x + 3, 28)], (255, 224, 120))
    l.rect(7, 6, 1, 14, (255, 255, 255))
    l.rect(19, 6, 1, 14, (255, 255, 255))
    return _fin(l)


def sonda():
    l = Lienzo()
    a = rampa(ACERO, 5)
    p = rampa((60, 100, 190), 5)
    for x0 in (2, 22):                                                                                  # paneles solares
        esculpir(l, mascara([(x0, 11), (x0 + 8, 11), (x0 + 8, 21), (x0, 21)]), p, relieve=0.6)
        for k in range(1, 4):
            l.rect(x0 + 2 * k, 11, 1, 10, p[0])
        l.rect(x0, 16, 8, 1, p[0])
    l.rect(10, 15, 12, 2, a[1])
    esculpir(l, _disco(16, 16, 6.5, 6.5), a, relieve=1.6)
    l.rect(14, 14, 4, 3, rampa((240, 220, 90), 3)[1])                                                    # ojo
    l.linea(16, 10, 16, 5, a[2])
    l.punto(16, 4, ROJO)
    return _fin(l)


def motor():
    l = Lienzo()
    a = rampa(ACERO, 5)
    esculpir(l, mascara([(10, 6), (22, 6), (23, 14), (27, 25), (5, 25), (9, 14)]), a, relieve=1.3)
    esculpir(l, mascara([(7, 24), (25, 24), (26, 28), (6, 28)]), rampa(ROJO, 5), relieve=0.8)
    l.rect(12, 8, 1, 14, a[4])
    return _fin(l, True)


def aleta():
    l = Lienzo()
    a = rampa(ACERO, 5)
    esculpir(l, mascara([(9, 5), (21, 9), (21, 27), (9, 27)]), a, relieve=1.0)
    esculpir(l, mascara([(21, 14), (27, 27), (21, 27)]), rampa(ROJO, 5), relieve=0.8)
    l.linea(9, 5, 9, 27, a[4])
    return _fin(l, True)


def modulo():
    l = Lienzo()
    a = rampa(ACERO, 5)
    esculpir(l, mascara([(8, 8), (24, 8), (26, 20), (6, 20)]), a, relieve=1.2)
    esculpir(l, _disco(16, 13, 3.5, 3.5), rampa((120, 190, 230), 5), relieve=1.2)                        # ventana
    for x0, x1 in ((7, 4), (25, 28)):
        l.linea(x0, 20, x1, 28, a[1])
        l.rect(x1 - 1, 28, 3, 2, rampa(ROJO, 4)[2])
    return _fin(l, True)


def nariz():
    l = Lienzo()
    a = rampa(ACERO, 5)
    esculpir(l, mascara([(16, 3), (24, 22), (8, 22)]), a, relieve=1.5)
    esculpir(l, mascara([(8, 22), (24, 22), (25, 28), (7, 28)]), rampa(ROJO, 5), relieve=0.9)
    return _fin(l, True)


def tanque():
    l = Lienzo()
    a = rampa(ACERO, 5)
    _cilindro(l, 9, 4, 23, 29, a, rad=3)
    l.rect(9, 12, 14, 3, rampa(ROJO, 4)[2])
    l.rect(9, 12, 14, 1, rampa(ROJO, 4)[3])
    l.rect(9, 20, 14, 1, a[1])
    return _fin(l, True)


def gema(base, semilla=0):
    """Cristal facetado: cada cara toma un tono de la rampa según hacia dónde mira."""
    c = rampa(base, 6)
    l = Lienzo()
    l.poligono([(16, 3), (24, 10), (21, 25), (11, 25), (8, 10)], c[3])
    l.poligono([(16, 3), (24, 10), (16, 12)], c[5])                  # faceta de luz
    l.poligono([(16, 3), (8, 10), (16, 12)], c[4])
    l.poligono([(24, 10), (21, 25), (16, 12)], c[2])
    l.poligono([(8, 10), (11, 25), (16, 12)], c[3])
    l.poligono([(11, 25), (21, 25), (16, 12)], c[1])
    l.punto(13, 9, (255, 255, 255)); l.punto(14, 8, (255, 255, 255)); l.punto(13, 10, c[5])
    return _fin(l, True)


def combustible():
    """Un terrón de carbón con vetas brillantes."""
    from lib.iconos import bulto
    return bulto((70, 66, 84), semilla=7, rx=9.5, ry=8, relieve=1.6, fuerza=0.6, motas_color=(180, 190, 220), n_motas=4)


def plataforma():
    l = Lienzo()
    a = rampa((120, 126, 140), 5)
    esculpir(l, mascara([(3, 12), (29, 12), (29, 26), (3, 26)]), a, relieve=0.8, base_oscura=0.15)
    esculpir(l, mascara([(3, 12), (7, 6), (25, 6), (29, 12)]), rampa((150, 156, 170), 5), relieve=0.7)
    amar = rampa((236, 190, 50), 4)
    for x in range(5, 28, 7):
        l.poligono([(x, 26), (x + 3, 26), (x + 6, 14), (x + 3, 14)], amar[2])
        l.linea(x + 3, 14, x + 6, 14, amar[3])
    return _fin(l, True)


def cartel_construir():
    return ladrillo((196, 92, 62))


def engranaje():
    l = Lienzo()
    a = rampa((150, 156, 168), 5)
    m = _disco(16, 16, 9.5, 9.5)
    for k in range(8):
        ang = k * math.tau / 8
        cx, cy = 16 + 11.5 * math.cos(ang), 16 + 11.5 * math.sin(ang)
        m |= _disco(cx, cy, 3.0, 3.0)
    m &= ~_disco(16, 16, 4.0, 4.0)
    esculpir(l, m, a, relieve=1.4)
    return _fin(l)


def varita():
    """Varita mágica: palo con estrella dorada."""
    l = Lienzo()
    l.banda(6, 27, 20, 13, 2.8, rampa((130, 92, 60), 4)[2])
    l.banda(5, 26, 19, 12, 1.0, rampa((130, 92, 60), 4)[3])
    o = rampa((240, 196, 70), 5)
    pts = []
    for k in range(10):
        r = 9.0 if k % 2 == 0 else 3.8
        ang = -math.pi / 2 + k * math.pi / 5
        pts.append((22 + r * math.cos(ang), 10 + r * math.sin(ang)))
    esculpir(l, mascara([(int(round(x)), int(round(y))) for x, y in pts]), o, relieve=1.2)
    return _fin(l)


def planeta_anillado():
    l = Lienzo()
    anillo = rampa((240, 214, 150), 4)
    l.elipse(16, 17, 14, 4.5, anillo[1])                                  # el anillo, por detrás
    l.elipse(16, 17, 11, 2.6, (0, 0, 0), 0)                               # (hueco: sin efecto, el planeta lo tapa)
    esculpir(l, _disco(16, 15, 8.0, 8.0), rampa((150, 112, 190), 5), relieve=1.6)
    # por delante pasa sólo la mitad de abajo del anillo
    delante = Lienzo()
    delante.elipse(16, 17, 14, 4.5, anillo[2])
    delante.elipse(16, 17, 11, 2.6, (0, 0, 0), 0)
    for j in range(18, 22):
        for i in range(CELDA):
            if delante.px[i, j][3] == 255 and not (abs(i - 16) < 11 and j < 20 and ((i - 16) / 11) ** 2 + ((j - 17) / 2.6) ** 2 <= 1):
                l.px[i, j] = delante.px[i, j]
    return _fin(l)


def luna_llena():
    l = Lienzo()
    r = rampa((206, 210, 224), 5)
    esculpir(l, _disco(16, 16, 11, 11), r, relieve=1.6)
    for cx, cy, rr in [(12, 12, 2.6), (21, 17, 3.2), (13, 21, 2.0)]:
        cr = _disco(cx, cy, rr, rr)
        esculpir(l, cr, rampa((150, 156, 176), 4), relieve=0.8)
    return _fin(l)


def cesped_cortado():
    """Losa de césped segado: pasto REAL del pack con franjas alternas más claras y más oscuras (la huella
    de la cortadora). Es terreno (llena la celda), así que no pasa por la silueta."""
    from PIL import Image
    from lib.pixel import ajustar
    atlas = Image.open(Path(__file__).resolve().parents[2] / "native/crates/ha-game/assets/art/pack/suelo.png").convert("RGB")
    t = np.asarray(atlas.crop((4 * CELDA, 0, 5 * CELDA, CELDA))).astype(np.float32)   # el pasto liso del propio juego
    l = Lienzo()
    for j in range(CELDA):
        franja = (j // 8) % 2
        for i in range(CELDA):
            f = 1.06 if franja == 0 else 0.94
            c = np.clip(t[j, i] * f + (3 if franja == 0 else -3), 0, 255)
            l.punto(i, j, ajustar(tuple(int(v) for v in c)))
    return l


def cancha(tinte=(1.0, 1.0, 1.0), clave="prop/cancha_basket"):
    """Losa de cancha: el asfalto liso y las líneas crema de la cancha del pack (`school_3`), con las MISMAS
    marcas que tenía la losa anterior (dónde pasa la línea lo decide el juego, no yo)."""
    import json
    from PIL import Image
    from lib.pixel import ajustar
    base = Path(__file__).resolve().parents[2] / "native/crates/ha-game/assets"
    man = json.loads((base / "art/manifest.json").read_text())["sprites"]
    v = man[clave]
    r = v["rect"]
    g = v.get("grid", [32, 32])
    viejo = Image.open(base / v["png"]).convert("RGBA").crop((r["col"] * g[0], r["row"] * g[1], (r["col"] + r["w"]) * g[0], (r["row"] + r["h"]) * g[1]))
    pack = Image.open(base / "art/pack/school_3.png").convert("RGB")
    asfalto = pack.crop((48, 230, 80, 262))
    l = Lienzo()
    for j in range(CELDA):
        for i in range(CELDA):
            ar, ag, ab, aa = viejo.getpixel((i, j))
            lum = 0.299 * ar + 0.587 * ag + 0.114 * ab
            if lum > 185 and aa == 255:
                l.punto(i, j, ajustar((226, 212, 176)))                       # línea crema
            else:
                c = asfalto.getpixel((i, j))
                l.punto(i, j, ajustar(tuple(min(255, int(c[k] * tinte[k])) for k in range(3))))
    return l


def cohete():
    """Cohete de 3x4 celdas (96x128): casco cilíndrico con cúpula de luz, cono rojo, ventana, aletas y toberas."""
    from lib.pixel import Lienzo as L
    l = L(96, 128)
    blanco = rampa((226, 224, 236), 6)
    rojo = rampa(ROJO, 5)
    acero = rampa(ACERO, 5)
    def m(pts):
        t = L(96, 128)
        t.poligono(pts, (255, 255, 255))
        return np.array([[t.px[i, j][3] > 0 for i in range(96)] for j in range(128)])
    esculpir(l, m([(30, 52), (66, 52), (66, 112), (30, 112)]), blanco, relieve=1.6, base_oscura=0.12)           # casco
    esculpir(l, m([(48, 4), (66, 52), (30, 52)]), rojo, relieve=1.5)                                                # cono
    for x0, x1 in ((6, 30), (90, 66)):                                                                              # aletas
        esculpir(l, m([(x0, 126), (x1, 90), (x1, 120)]) if x0 < 48 else m([(x0, 126), (x1, 90), (x1, 120)]), rojo, relieve=1.0)
    esculpir(l, m([(34, 112), (62, 112), (64, 120), (32, 120)]), acero, relieve=0.8)                                # tobera
    esculpir(l, m([(40, 120), (56, 120), (58, 127), (38, 127)]), rampa((70, 74, 86), 4), relieve=0.5)
    esculpir(l, np.array([[((i - 48) / 8.5) ** 2 + ((j - 70) / 8.5) ** 2 <= 1 for i in range(96)] for j in range(128)]), rampa((110, 184, 232), 5), relieve=1.4)  # ventana
    l.rect(30, 94, 36, 3, rojo[3])
    l.rect(30, 94, 36, 1, rojo[4])
    silueta(l)
    return l


def mapa_tesoro():
    """**Mapa del tesoro** (3 oct 2026): pergamino gastado con los dos extremos enrollados, una isla en tinta
    sepia, el camino punteado y la X roja. Es lo que da la guía hasta un alijo (`ha-game/alijos.rs`)."""
    from lib.iconos import textura_roca

    l = Lienzo()
    papel = rampa((226, 200, 146), 5)
    tinta = (112, 74, 50)
    # La hoja, apenas torcida y con el borde mordido (no es un rectángulo).
    hoja = mascara([(6, 8), (11, 7), (15, 9), (20, 7), (26, 8), (26, 25), (21, 24), (16, 26), (11, 24), (6, 25)])
    grano = np.tile(textura_roca(), (2, 2))[:CELDA, :CELDA]
    esculpir(l, hoja, papel, relieve=0.55, textura=grano, fuerza_textura=0.45, base_oscura=0.12)
    # Los extremos enrollados: dos cilindros de papel más tostado, el de la derecha en sombra.
    rollo = rampa((186, 128, 76), 5)
    _cilindro(l, 3, 6, 8, 27, rollo, rad=2)
    _cilindro(l, 25, 7, 30, 26, rampa((168, 112, 66), 5), rad=2)
    l.rect(7, 8, 1, 17, rollo[1])                       # el pliegue donde la hoja entra al rollo
    # La isla: tierra un tono más oscura con la costa entintada.
    isla = mascara([(10, 13), (14, 11), (19, 12), (22, 15), (21, 20), (16, 22), (12, 21), (9, 17)])
    for j in range(CELDA):
        for i in range(CELDA):
            if isla[j, i]:
                borde = not all(isla[b, a] for a, b in ((i + 1, j), (i - 1, j), (i, j + 1), (i, j - 1)))
                l.punto(i, j, tinta if borde else (206, 186, 128))
    l.rect(12, 14, 2, 2, (118, 132, 74))               # un bosquecito
    l.punto(12, 14, (150, 164, 96))
    # El camino punteado hasta la X.
    for x, y in [(11, 19), (13, 19), (14, 17), (16, 17)]:
        l.punto(x, y, tinta)
    rojo = rampa((200, 56, 52), 4)
    for k in range(5):                                   # la X, de dos píxeles de grueso
        for g in (0, 1):
            l.punto(17 + k + g, 13 + k, rojo[2] if g == 0 else rojo[1])
            l.punto(21 - k + g, 13 + k, rojo[2] if g == 0 else rojo[1])
    l.punto(17, 13, rojo[3])
    l.punto(21, 13, rojo[3])
    # Brillos donde pega la luz (arriba-izquierda de cada rollo).
    l.punto(4, 8, (255, 255, 255))
    l.punto(4, 9, rollo[4])
    l.punto(26, 9, rollo[4])
    return _fin(l)
