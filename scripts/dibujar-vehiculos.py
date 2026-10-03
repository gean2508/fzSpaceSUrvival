#!/usr/bin/env python3
"""**MOTO Y BICI: los dos vehículos que el pack no trae**, con conductor sentado.

    python3 scripts/dibujar-vehiculos.py                  # escribe PNG + hoja + manifest
    python3 scripts/dibujar-vehiculos.py --preview o.png  # sólo la lámina ampliada

Cuatro poses por vehículo (`abajo`/`arriba` alargadas en vertical, `izquierda`/`derecha` en horizontal;
`derecha` es el espejo exacto de `izquierda`). Cada parte es una máscara (polígono / blob) esculpida con
`lib.volumen.esculpir` (cúpula iluminada arriba-izquierda, 5 tonos) y todo el conjunto lleva el contorno
azul-violeta oscuro de 1 px que tienen los autos del pack (`vehiculos.png`), más sombra de contacto.

Publica `vehicle/moto` y `vehicle/bici` con el formato de `import-vehiculos.py`, en la hoja propia
`art/pack/vehiculos_dibujados.png`. Registra las claves en `scripts/vehiculos-dibujados-index.json`.
"""
import argparse
import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib.pixel import SOMBRA, SOMBRA_ALFA, Lienzo, ajustar, rampa  # noqa: E402
from lib.volumen import blob, esculpir, exigir_liston  # noqa: E402

RAIZ = Path(__file__).resolve().parent.parent
ASSETS = RAIZ / "native/crates/ha-game/assets"
MANIFEST = ASSETS / "art/manifest.json"
GEN = ASSETS / "art/gen/vehiculos"
PAGINA_REL = "art/pack/vehiculos_dibujados.png"
INDEX = RAIZ / "scripts/vehiculos-dibujados-index.json"
AIRE = 2

CONTORNO = (36, 34, 62)
GOMA = (64, 66, 94)
METAL = (150, 156, 176)
ROJO = (214, 62, 58)
CHAQUETA_MOTO = (58, 110, 200)
PANTALON = (66, 70, 100)
CASCO_MOTO = (236, 236, 244)
VISOR = (70, 150, 200)
FARO = (250, 232, 150)
CUADRO_BICI = (70, 180, 130)
CHAQUETA_BICI = (240, 150, 50)
CASCO_BICI = (250, 220, 70)


# ── máscaras ─────────────────────────────────────────────────────────────────────────────────────

class Pieza:
    """Una vista en construcción: partes en orden de pintado sobre un lienzo W×H."""

    def __init__(self, w, h):
        self.w, self.h = w, h
        self.partes = []  # (mask, color, relieve)

    def _nueva(self):
        return Image.new("L", (self.w, self.h), 0)

    def poli(self, pts, color, relieve=1.0):
        im = self._nueva()
        ImageDraw.Draw(im).polygon([(x, y) for x, y in pts], fill=255)
        self.partes.append((np.array(im) > 0, color, relieve))

    def rect(self, x0, y0, x1, y1, color, relieve=1.0):
        self.poli([(x0, y0), (x1, y0), (x1, y1), (x0, y1)], color, relieve)

    def banda(self, a, b, g, color, relieve=1.0):
        import math

        dx, dy = b[0] - a[0], b[1] - a[1]
        n = math.hypot(dx, dy) or 1
        px, py = -dy / n * g / 2, dx / n * g / 2
        self.poli([(a[0] + px, a[1] + py), (b[0] + px, b[1] + py), (b[0] - px, b[1] - py), (a[0] - px, a[1] - py)], color, relieve)

    def bola(self, cx, cy, rx, ry, color, semilla, relieve=1.0, rug=0.05):
        self.partes.append((blob(self.w, self.h, cx, cy, rx, ry, semilla, rug), color, relieve))

    def rueda(self, cx, cy, r, semilla):
        """Rueda de costado: goma, llanta clara y buje."""
        self.bola(cx, cy, r, r, GOMA, semilla, 0.8, 0.02)
        self.bola(cx, cy, r * 0.55, r * 0.55, METAL, semilla + 1, 1.0, 0.02)
        self.bola(cx, cy, 1.2, 1.2, (96, 100, 130), semilla + 2, 0.6, 0.0)

    def aro(self, cx, cy, r, grosor, semilla):
        """Rueda de bici: sólo el aro de goma, con rayos de un píxel."""
        ys, xs = np.mgrid[0:self.h, 0:self.w]
        d = np.hypot(xs + 0.5 - cx, ys + 0.5 - cy)
        self.partes.append(((d <= r) & (d > r - grosor), GOMA, 0.6))
        rayo = np.zeros((self.h, self.w), bool)
        for a in (0.0, 0.8, 1.6, 2.4):
            for t in np.linspace(-r + grosor, r - grosor, int(2 * r * 2)):
                x, y = int(cx + t * np.cos(a)), int(cy + t * np.sin(a))
                if 0 <= x < self.w and 0 <= y < self.h:
                    rayo[y, x] = True
        self.partes.append((rayo, (176, 180, 200), 0.5))
        self.bola(cx, cy, 1.3, 1.3, METAL, semilla, 0.6, 0.0)

    def llantas(self, x0, y0, x1, y1, semilla):
        """Rueda de frente/espalda: goma con dibujo (tacos más claros)."""
        self.rect(x0, y0, x1, y1, GOMA, 0.7)
        m = np.zeros((self.h, self.w), bool)
        for y in range(int(y0) + 1, int(y1) - 1, 3):
            m[y, int(x0) + 1 : int(x1) - 1] = True
        self.partes.append((m, (92, 94, 120), 0.4))

    # ── render ──
    def dibujar(self, forma_sombra="pie"):
        cuerpo = np.zeros((self.h, self.w), bool)
        for m, _, _ in self.partes:
            cuerpo |= m
        l = Lienzo(self.w, self.h)
        for k, (m, color, relieve) in enumerate(self.partes):
            esculpir(l, m, rampa(color, 4), relieve=relieve, suavizar=0 if m.sum() < 40 else 1)
        # contorno de 1 px de TODA la silueta (el de los autos del pack)
        from scipy import ndimage

        borde = ndimage.binary_dilation(cuerpo) & ~cuerpo
        for j, i in zip(*np.nonzero(borde)):
            l.punto(int(i), int(j), CONTORNO)
        # sombra de contacto: siluetas desplazadas, debajo de todo
        sombra = ndimage.binary_dilation(cuerpo | borde, iterations=1)
        dj, di = (2, 1) if forma_sombra == "pie" else (1, 1)
        sh = np.roll(np.roll(sombra, dj, 0), di, 1)
        base = Image.new("RGBA", (self.w, self.h), (0, 0, 0, 0))
        bp = base.load()
        for j, i in zip(*np.nonzero(sh)):
            bp[int(i), int(j)] = (*ajustar(SOMBRA), SOMBRA_ALFA)
        base.alpha_composite(l.im)
        return base


def guardia(im, nombre):
    exigir_liston(im, nombre)


# ── MOTO ─────────────────────────────────────────────────────────────────────────────────────────

def moto_frente(arriba: bool) -> Image.Image:
    """26×62. `arriba` = vista trasera (la moto va hacia el norte)."""
    p = Pieza(26, 62)
    cx = 13
    if not arriba:
        # rueda trasera asoma arriba, rueda delantera abajo
        p.llantas(10, 2, 16, 15, 1)
        p.poli([(9, 11), (17, 11), (18, 24), (8, 24)], ROJO)
        p.rect(5, 33, 9, 46, PANTALON, 0.8)
        p.rect(17, 33, 21, 46, PANTALON, 0.8)
        p.poli([(9, 29), (17, 29), (18, 43), (8, 43)], ROJO)
        p.rect(11, 38, 15, 53, METAL, 0.8)
        p.llantas(10, 47, 16, 60, 2)
        p.poli([(8, 45), (18, 45), (18, 52), (8, 52)], ROJO)
        p.rect(10, 38, 16, 43, FARO, 1.0)
        p.rect(1, 34, 25, 37, (96, 100, 130), 0.8)
        p.rect(1, 30, 3, 33, METAL, 0.6)
        p.rect(23, 30, 25, 33, METAL, 0.6)
        p.poli([(6, 23), (20, 23), (21, 36), (5, 36)], CHAQUETA_MOTO)
        p.banda((6, 25), (2, 35), 4, CHAQUETA_MOTO)
        p.banda((20, 25), (24, 35), 4, CHAQUETA_MOTO)
        p.bola(cx, 19, 6.2, 6.2, CASCO_MOTO, 11, 1.0, 0.05)
        p.rect(9, 20, 18, 23, VISOR, 0.7)
    else:
        p.llantas(10, 2, 16, 15, 3)
        p.rect(11, 10, 15, 27, METAL, 0.8)
        p.poli([(8, 12), (18, 12), (18, 19), (8, 19)], ROJO)
        p.rect(1, 24, 25, 27, (96, 100, 130), 0.8)
        p.rect(1, 21, 3, 24, METAL, 0.6)
        p.rect(23, 21, 25, 24, METAL, 0.6)
        p.rect(5, 32, 9, 46, PANTALON, 0.8)
        p.rect(17, 32, 21, 46, PANTALON, 0.8)
        p.poli([(9, 40), (17, 40), (18, 52), (8, 52)], ROJO)
        p.llantas(10, 47, 16, 60, 4)
        p.poli([(8, 50), (18, 50), (18, 56), (8, 56)], ROJO)
        p.rect(11, 55, 15, 57, (250, 90, 80), 1.0)  # luz de freno
        p.poli([(6, 27), (20, 27), (21, 42), (5, 42)], CHAQUETA_MOTO)
        p.banda((6, 29), (2, 25), 4, CHAQUETA_MOTO)
        p.banda((20, 29), (24, 25), 4, CHAQUETA_MOTO)
        p.bola(cx, 24, 6.2, 6.2, CASCO_MOTO, 12, 1.0, 0.05)
        p.rect(11, 19, 16, 20, (170, 176, 200), 0.5)
    return p.dibujar("pie")


def moto_lado() -> Image.Image:
    """62×28 mirando a la izquierda."""
    p = Pieza(62, 28)
    p.rueda(53.5, 20.5, 6.5, 21)
    p.rueda(9.5, 20.5, 6.5, 22)
    p.poli([(18, 14), (44, 14), (47, 22), (20, 23)], (96, 100, 130), 0.8)
    p.rect(24, 15, 38, 23, METAL, 0.9)
    p.banda((22, 23), (52, 24), 2, (190, 194, 210), 0.5)
    # horquilla, guardabarros y faro
    p.banda((10, 19), (18, 8), 2.5, METAL, 0.7)
    p.poli([(3, 15), (9, 11), (15, 13), (15, 15), (3, 17)], ROJO)
    p.poli([(2, 11), (7, 10), (8, 15), (2, 15)], FARO)
    # tanque, asiento, cola
    p.poli([(19, 10), (32, 9), (35, 15), (19, 16)], ROJO)
    p.poli([(33, 12), (46, 11), (50, 14), (35, 15)], (60, 62, 96), 0.8)
    p.poli([(46, 12), (58, 13), (57, 17), (46, 16)], ROJO)
    p.rect(57, 13, 59, 15, (250, 90, 80), 1.0)
    # conductor inclinado hacia adelante
    p.banda((41, 12), (30, 15), 4, PANTALON)
    p.banda((30, 15), (31, 22), 3, PANTALON)
    p.rect(28, 21, 33, 24, GOMA, 0.7)
    p.poli([(40, 12), (43, 10), (35, 3), (29, 6), (34, 12)], CHAQUETA_MOTO)
    p.banda((32, 6), (21, 9), 3.2, CHAQUETA_MOTO)
    p.rect(18, 7, 22, 10, GOMA, 0.7)
    p.bola(27, 5, 4.6, 4.4, CASCO_MOTO, 23, 1.0, 0.05)
    p.rect(22, 4, 26, 7, VISOR, 0.7)
    return p.dibujar("pie")


# ── BICI ─────────────────────────────────────────────────────────────────────────────────────────

def bici_frente(arriba: bool) -> Image.Image:
    """18×50."""
    p = Pieza(18, 50)
    cx = 9
    if not arriba:
        p.llantas(7, 2, 11, 14, 31)
        p.rect(8, 8, 10, 28, CUADRO_BICI, 0.8)
        p.rect(7, 29, 11, 34, CUADRO_BICI, 0.8)
        p.llantas(7, 37, 11, 49, 32)
        p.rect(7, 36, 11, 40, CUADRO_BICI, 0.8)
        p.rect(7, 30, 11, 32, (240, 236, 200), 1.0)
        p.rect(1, 27, 17, 29, (96, 100, 130), 0.8)
        p.rect(4, 31, 7, 37, PANTALON, 0.8)
        p.rect(11, 31, 14, 37, PANTALON, 0.8)
        p.poli([(4, 17), (14, 17), (14, 27), (4, 27)], CHAQUETA_BICI)
        p.banda((4, 19), (2, 28), 3, CHAQUETA_BICI)
        p.banda((14, 19), (16, 28), 3, CHAQUETA_BICI)
        p.bola(cx, 14, 4.1, 4.1, CASCO_BICI, 33, 1.0, 0.05)
        p.rect(7, 16, 12, 17, VISOR, 0.5)
    else:
        p.llantas(7, 2, 11, 14, 34)
        p.rect(8, 10, 10, 28, CUADRO_BICI, 0.8)
        p.rect(1, 21, 17, 23, (96, 100, 130), 0.8)
        p.rect(4, 31, 7, 38, PANTALON, 0.8)
        p.rect(11, 31, 14, 38, PANTALON, 0.8)
        p.llantas(7, 38, 11, 49, 35)
        p.rect(6, 37, 12, 41, CUADRO_BICI, 0.8)
        p.rect(8, 40, 10, 42, (250, 90, 80), 1.0)
        p.poli([(4, 24), (14, 24), (14, 35), (4, 35)], CHAQUETA_BICI)
        p.banda((4, 26), (2, 22), 3, CHAQUETA_BICI)
        p.banda((14, 26), (16, 22), 3, CHAQUETA_BICI)
        p.bola(cx, 21, 4.1, 4.1, CASCO_BICI, 36, 1.0, 0.05)
    return p.dibujar("pie")


def bici_lado() -> Image.Image:
    """50×22 mirando a la izquierda."""
    p = Pieza(50, 22)
    p.aro(8.5, 15.5, 5.6, 1.8, 41)
    p.aro(41.5, 15.5, 5.6, 1.8, 42)
    # cuadro: tubo superior, diagonal, vainas, horquilla y manubrio
    p.banda((15, 7), (30, 7), 1.6, CUADRO_BICI, 0.6)
    p.banda((15, 7), (25, 15), 1.6, CUADRO_BICI, 0.6)
    p.banda((30, 7), (25, 15), 1.6, CUADRO_BICI, 0.6)
    p.banda((25, 15), (41, 15), 1.6, CUADRO_BICI, 0.6)
    p.banda((30, 7), (41, 15), 1.6, CUADRO_BICI, 0.6)
    p.banda((15, 7), (8, 15), 1.6, METAL, 0.6)
    p.banda((15, 7), (12, 4), 1.6, METAL, 0.6)
    p.rect(10, 3, 14, 5, GOMA, 0.7)
    p.rect(28, 5, 33, 7, GOMA, 0.7)  # asiento
    p.rect(22, 15, 28, 17, (96, 100, 130), 0.6)  # pedal
    # ciclista inclinado
    p.banda((30, 7), (24, 12), 3.6, PANTALON)
    p.banda((24, 12), (25, 17), 3, PANTALON)
    p.banda((29, 8), (21, 5), 5.5, CHAQUETA_BICI)
    p.banda((21, 5), (13, 6), 3, CHAQUETA_BICI)
    p.bola(17, 3.2, 3.6, 3.4, CASCO_BICI, 43, 1.0, 0.05)
    return p.dibujar("pie")


# ── publicación ──────────────────────────────────────────────────────────────────────────────────

def modelos():
    moto_i = moto_lado()
    bici_i = bici_lado()
    return {
        "moto": ("Moto", {
            "abajo": moto_frente(False), "arriba": moto_frente(True),
            "izquierda": moto_i, "derecha": moto_i.transpose(Image.FLIP_LEFT_RIGHT),
        }),
        "bici": ("Bicicleta", {
            "abajo": bici_frente(False), "arriba": bici_frente(True),
            "izquierda": bici_i, "derecha": bici_i.transpose(Image.FLIP_LEFT_RIGHT),
        }),
    }


def lamina(mods, ruta, k=8):
    """Las ocho vistas ampliadas, junto a un auto del pack para comparar."""
    hoja = Image.open(ASSETS / "art/pack/vehiculos.png").convert("RGBA")
    man = json.loads(MANIFEST.read_text())["sprites"]
    ref = []
    for clave in ("vehicle/auto_1", "vehicle/colectivo_1"):
        for pose in ("abajo", "izquierda"):
            r = man[clave]["poses"][pose]["rect"]
            ref.append(hoja.crop((r["col"], r["row"], r["col"] + r["w"], r["row"] + r["h"])))
    piezas = [im for _, poses in mods.values() for im in poses.values()] + ref
    W = sum(i.width for i in piezas) // 2 + 40
    fila_a = [i for i in piezas[:8]]
    ancho = max(sum(i.width + 6 for i in fila_a), sum(i.width + 6 for i in ref)) * k // 2
    lam = Image.new("RGBA", (ancho + 20, 1400), (150, 190, 120, 255))
    y = 6
    for fila, kk in ((fila_a, 10), (ref, 4)):
        x = 6
        alto = 0
        for im in fila:
            g = im.resize((im.width * kk, im.height * kk), Image.NEAREST)
            lam.alpha_composite(g, (x, y))
            x += g.width + 10
            alto = max(alto, g.height)
        y += alto + 10
    lam.crop((0, 0, lam.width, y)).save(ruta)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--preview")
    args = ap.parse_args()
    mods = modelos()
    for clave, (_, poses) in mods.items():
        for pose, im in poses.items():
            exigir_liston(im, f"{clave}_{pose}")
    if args.preview:
        lamina(mods, args.preview)
        return
    GEN.mkdir(parents=True, exist_ok=True)
    for clave, (_, poses) in mods.items():
        for pose, im in poses.items():
            im.save(GEN / f"{clave}_{pose}.png")

    # hoja propia, empaquetada por estantes con 2 px de aire
    ancho = 512
    x, y, fila = AIRE, AIRE, 0
    puestas, sprites, objetos = [], {}, {}
    for clave, (etiqueta, poses) in mods.items():
        rects = {}
        for pose, im in poses.items():
            bb = im.getbbox()
            im = im.crop(bb)
            w, h = im.size
            if x + w + AIRE > ancho:
                x, y, fila = AIRE, y + fila + AIRE, 0
            rects[pose] = {"rect": {"col": x, "row": y, "w": w, "h": h}}
            puestas.append((im, x, y))
            x += w + AIRE
            fila = max(fila, h)
        sprites[f"vehicle/{clave}"] = {
            "png": PAGINA_REL, "grid": [1, 1],
            "rect": rects["abajo"]["rect"], "poses": rects,
        }
        objetos[f"vehicle/{clave}"] = etiqueta
    alto = (y + fila + AIRE + 31) // 32 * 32
    hoja = Image.new("RGBA", (ancho, alto), (0, 0, 0, 0))
    for im, px, py in puestas:
        hoja.paste(im, (px, py))
    hoja.save(ASSETS / PAGINA_REL, optimize=True)

    man = json.loads(MANIFEST.read_text())
    man.setdefault("sprites", {}).update(sprites)
    for k, etiqueta in objetos.items():
        man.setdefault("objects", {}).setdefault(k, {})["label"] = etiqueta
    MANIFEST.write_text(json.dumps(man, indent=1, ensure_ascii=False, sort_keys=True))
    INDEX.write_text(json.dumps({"sprites": sorted(sprites), "pagina": PAGINA_REL}, indent=1, ensure_ascii=False))
    for clave, (_, poses) in mods.items():
        print(clave, {p: f"{i.width}x{i.height}" for p, i in poses.items()})
    print(f"{PAGINA_REL}: {ancho}x{alto} px · {len(sprites)} claves")


if __name__ == "__main__":
    main()
