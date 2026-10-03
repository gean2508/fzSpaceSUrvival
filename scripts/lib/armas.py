#!/usr/bin/env python3
"""**ARMAS DE FUEGO EN PIXEL ART, CON CAMUFLAJES TIPO BLACK OPS.**

Cada arma es una lista de PIEZAS (polígonos) con un material. Cada pieza se sombrea como una cúpula iluminada desde
arriba-izquierda (la luz del resto del arte) y luego se pinta:

* `M` metal oscuro (cañón, cargador, detalles): acero azulado, nunca lleva camuflaje.
* `B` cuerpo (cajón de mecanismos, corredera): acero o, con camuflaje, el patrón.
* `F` mueble (culata, guardamanos, empuñadura): madera o polímero o, con camuflaje, el patrón.
* `L` luz (cámara de energía, ánima): brilla, ignora el sombreado.

El camuflaje es un patrón procedural determinista (celdas de Voronoi, ruido deformado, franjas onduladas, circuitos)
que se modula con el sombreado de la pieza, así que conserva el volumen. Todo con vecino más cercano, sin anti-alias:
pixel art de verdad, pero a una paleta propia (los camuflajes de las armas son vivos; el pack, apagado).
"""
from __future__ import annotations

import math
import random
from dataclasses import dataclass

import numpy as np
from PIL import Image, ImageDraw
from scipy import ndimage

W = H = 64          # celda del ícono (2×2 celdas de 32): el arma de ~56×24 va centrada
OX, OY = 4, 20      # dónde cae la caja del arma (56×24) dentro de la celda
CAJA = (56, 24)

LUZ = np.array([-0.55, -0.75, 0.62], np.float32)
LUZ /= np.linalg.norm(LUZ)
TINTA = (22, 16, 38)


@dataclass
class Pieza:
    puntos: list
    mat: str                       # M B F L
    tono: tuple | None = None      # color base propio (madera, polímero…)


def _mask(puntos):
    im = Image.new("L", (W, H), 0)
    # un 12 % más gruesas en vertical: de perfil las armas salían flacas
    ImageDraw.Draw(im).polygon([(OX + float(x), OY + 12 + (float(y) - 12) * 1.14) for x, y in puntos], fill=255)
    return np.asarray(im) > 0


def _sombreado(mask):
    """Luz 0..1 por píxel de la pieza: cúpula (distancia al borde) iluminada desde arriba-izquierda."""
    edt = ndimage.distance_transform_edt(mask)
    dmax = max(float(edt.max()), 1.0)
    h = np.sqrt(np.clip(edt / dmax, 0, 1)) * dmax * 0.9 * 1.2
    gy, gx = np.gradient(h.astype(np.float32))
    n = np.stack([-gx, -gy, np.ones_like(gx)], -1)
    n /= np.linalg.norm(n, axis=-1, keepdims=True)
    lamb = np.clip((n * LUZ).sum(-1), 0, 1)
    v = lamb[mask]
    lo, hi = np.percentile(v, 5), np.percentile(v, 97)
    s = np.clip((lamb - lo) / max(hi - lo, 1e-3), 0, 1)
    # cuantizada a 5 escalones: manchas de tono, no degradado (como el resto del arte)
    return np.floor(s * 4.999) / 4.0


# ── camuflajes: patrón RGB (H, W, 3) en 0..255 + "brillo" extra 0..1 ─────────────────────────────────────────
def _ruido(semilla, escala, octavas=3):
    rng = np.random.default_rng(semilla)
    out = np.zeros((H, W), np.float32)
    amp, tot = 1.0, 0.0
    for o in range(octavas):
        n = max(2, int(round(H / (escala / (2 ** o)))))
        g = rng.random((n + 2, n + 2)).astype(np.float32)
        z = ndimage.zoom(g, (H / n, W / n), order=1)[:H, :W]
        if z.shape != (H, W):
            z = np.pad(z, ((0, H - z.shape[0]), (0, W - z.shape[1])), mode="edge")
        out += z * amp
        tot += amp
        amp *= 0.5
    return out / tot


def _voronoi(semilla, n):
    rng = np.random.default_rng(semilla)
    pts = rng.random((n, 2)) * [W, H]
    ys, xs = np.mgrid[0:H, 0:W]
    d = np.stack([np.hypot(xs - px, ys - py) for px, py in pts], -1)
    ord_ = np.sort(d, -1)
    return np.argmin(d, -1), (ord_[..., 1] - ord_[..., 0])      # celda, distancia al borde


def camo_115():
    """Weaponized 115: verde ácido con grietas brillantes que laten (Black Ops, zombis)."""
    celda, borde = _voronoi(115, 26)
    base = np.array([14, 58, 28], np.float32)
    interior = np.array([30, 120, 50], np.float32)
    grieta = np.array([170, 255, 80], np.float32)
    t = np.clip(_ruido(3, 8) * 1.4 - 0.2, 0, 1)[..., None]
    img = base * (1 - t) + interior * t
    luz = np.clip(1.0 - borde / 2.2, 0, 1)[..., None]
    img = img * (1 - luz) + grieta * luz
    return img, luz[..., 0] * 0.9


def camo_singularidad():
    """Singularity: azul cósmico con remolinos cian y puntitos de estrella."""
    n = _ruido(21, 14, 4)
    base = np.array([10, 18, 70], np.float32)
    medio = np.array([30, 90, 210], np.float32)
    claro = np.array([120, 220, 255], np.float32)
    t = np.clip((np.sin(n * 18.0) * 0.5 + 0.5), 0, 1)[..., None]
    img = base * (1 - t) + medio * t
    veta = (np.abs(np.sin(n * 18.0)) < 0.10)[..., None]
    img = np.where(veta, claro, img)
    rng = np.random.default_rng(5)
    ys, xs = rng.integers(0, H, 40), rng.integers(0, W, 40)
    img[ys, xs] = (255, 255, 255)
    glow = np.zeros((H, W), np.float32)
    glow[ys, xs] = 1.0
    glow = np.maximum(glow, veta[..., 0] * 0.5)
    return img, glow


def camo_grafiti():
    """Graffiti: parches de colores saturados con contorno negro, como un muro pintado."""
    celda, borde = _voronoi(77, 22)
    pal = np.array([[255, 70, 160], [255, 200, 40], [40, 220, 230], [140, 80, 255], [255, 120, 30], [90, 230, 90], [250, 250, 250]], np.float32)
    rng = np.random.default_rng(3)
    asigna = rng.integers(0, len(pal), celda.max() + 1)
    img = pal[asigna[celda]]
    img = np.where((borde < 1.2)[..., None], np.array([20, 14, 30], np.float32), img)
    return img, np.zeros((H, W), np.float32)


def camo_bacon():
    """Bacon: franjas onduladas rojo carne y crema grasa, con vetas oscuras."""
    ys, xs = np.mgrid[0:H, 0:W].astype(np.float32)
    n = _ruido(9, 10, 3)
    onda = np.sin((ys + n * 14 + np.sin(xs * 0.35) * 2.0) * 0.95)
    rojo = np.array([178, 52, 52], np.float32)
    rojo_os = np.array([120, 28, 36], np.float32)
    crema = np.array([246, 214, 176], np.float32)
    img = np.where((onda > 0.35)[..., None], crema, np.where((onda > -0.4)[..., None], rojo, rojo_os))
    return img, np.zeros((H, W), np.float32)


def camo_ciclon():
    """Ciclón: turquesa con franjas naranjas y blancas en curvas (el azul de la referencia)."""
    ys, xs = np.mgrid[0:H, 0:W].astype(np.float32)
    n = _ruido(14, 12, 3)
    f = np.sin((xs * 0.6 + ys * 0.35 + n * 11) * 0.8)
    base = np.array([30, 150, 210], np.float32)
    osc = np.array([16, 96, 160], np.float32)
    naranja = np.array([250, 140, 30], np.float32)
    blanco = np.array([235, 245, 250], np.float32)
    img = np.where((f > 0.55)[..., None], naranja, np.where((f > 0.40)[..., None], blanco, np.where((f < -0.5)[..., None], osc, base)))
    return img, np.zeros((H, W), np.float32)


def camo_materia_oscura():
    """Materia oscura: negro con circuitos dorados (la primera referencia)."""
    img = np.tile(np.array([18, 18, 26], np.float32), (H, W, 1))
    rng = random.Random(9)
    oro = np.array([255, 190, 40], np.float32)
    glow = np.zeros((H, W), np.float32)
    for _ in range(26):
        x, y = rng.randrange(W), rng.randrange(H)
        for _ in range(rng.randrange(3, 7)):
            dx, dy = rng.choice([(1, 0), (0, 1), (1, 1), (-1, 1)])
            for _ in range(rng.randrange(2, 7)):
                x, y = (x + dx) % W, (y + dy) % H
                img[y, x] = oro
                glow[y, x] = 0.8
    return img, glow


CAMOS = {
    "115": ("Weaponized 115", camo_115),
    "singularidad": ("Singularidad", camo_singularidad),
    "grafiti": ("Grafiti", camo_grafiti),
    "bacon": ("Bacon", camo_bacon),
    "ciclon": ("Ciclón", camo_ciclon),
    "oscura": ("Materia oscura", camo_materia_oscura),
}

# ── materiales base ────────────────────────────────────────────────────────────────────────────────────
ACERO = np.array([128, 138, 158], np.float32)
ACERO_OSC = np.array([78, 86, 108], np.float32)
MADERA = np.array([156, 104, 58], np.float32)
POLIMERO = np.array([54, 56, 66], np.float32)


def _tono(base, s):
    """Tono sombreado de `base` para la luz `s` (0..1): oscuro tintado a violeta, claro tintado a cálido."""
    k = 0.42 + 0.95 * s
    c = base * k
    c = c + np.array([10, 0, 18], np.float32) * (1 - s) - np.array([0, 4, 12], np.float32) * s
    c = c + np.array([14, 10, 0], np.float32) * s
    return np.clip(c, 0, 255)


def dibujar(piezas: list[Pieza], camo: str | None = None) -> Image.Image:
    img = np.zeros((H, W, 4), np.uint8)
    patron, brillo = (CAMOS[camo][1]() if camo else (None, None))
    for p in piezas:
        m = _mask(p.puntos)
        if not m.any():
            continue
        s = _sombreado(m)
        for j, i in zip(*np.nonzero(m)):
            sj = float(s[j, i])
            if p.mat == "L":
                col = np.array(p.tono if p.tono else (120, 255, 120), np.float32) * (0.75 + 0.3 * sj)
                col = np.clip(col, 0, 255)
            elif p.mat == "M":
                col = _tono(np.array(p.tono, np.float32) if p.tono else ACERO_OSC, sj)
            elif camo and p.mat in "BF":
                base = patron[j, i]
                col = base * (0.55 + 0.62 * sj)
                if brillo[j, i] > 0.3:
                    col = col * (1 - brillo[j, i] * 0.4) + base * brillo[j, i] * 0.55 + 40 * brillo[j, i]
                col = np.clip(col, 0, 255)
            elif p.mat == "F":
                col = _tono(np.array(p.tono, np.float32) if p.tono else MADERA, sj)
            else:
                col = _tono(np.array(p.tono, np.float32) if p.tono else ACERO, sj)
            img[j, i] = (*[int(v) for v in col], 255)
        # luz de borde: la fila de arriba de cada pieza refleja (metal pulido) y la de abajo se hunde
        arriba = m & ~np.roll(m, 1, axis=0)
        abajo = m & ~np.roll(m, -1, axis=0)
        if p.mat in "MBL":
            for j, i in zip(*np.nonzero(arriba)):
                c = img[j, i, :3].astype(np.int32) + (46 if p.mat != "L" else 20)
                img[j, i, :3] = np.clip(c, 0, 255)
        for j, i in zip(*np.nonzero(abajo)):
            c = img[j, i, :3].astype(np.int32) - 26
            img[j, i, :3] = np.clip(c, 0, 255)
    # reflejo puntual (brillo) arriba-izquierda de cada pieza grande
    op = img[..., 3] > 0
    borde = ndimage.binary_dilation(op) & ~op
    for j, i in zip(*np.nonzero(borde)):
        img[j, i] = (*TINTA, 255)
    return Image.fromarray(img, "RGBA")
