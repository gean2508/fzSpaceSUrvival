#!/usr/bin/env python3
"""**VOLUMEN CON LA LUZ DEL PACK, NO ELIPSES CON TRES TONOS.**

Lo que hace que un objeto mío se lea como "hecho en Paint" al lado del pack no es la paleta —ya
pasa por `lib/pixel.py::ajustar`— sino que **no tiene forma**: una elipse rellena con un relieve
de tres tonos sale plana, simétrica y con los bordes iguales. El pack tiene siluetas irregulares,
un cuerpo que se curva hacia la luz (arriba-izquierda) y se hunde hacia abajo-derecha, superficies
con grano, y brillos puntuales.

Esta librería construye eso con cuentas, no con ojo:

1. [`blob`]: una silueta irregular (elipse perturbada con ruido de bajo orden), con la semilla del
   objeto — dos rocas nunca salen iguales.
2. [`esculpir`]: de la silueta saca una **cúpula** (distancia al borde → altura), le calcula la
   normal en cada píxel y la ilumina desde arriba-izquierda (la dirección medida en el pack). El
   resultado se cuantiza a **bandas de tono de la rampa**, y se le pasa un filtro de moda para que
   las bandas sean MANCHAS (como pinta LimeZu) y no ruido pixel a pixel (= dithering).
3. Una **textura** opcional (`lib.materiales`, Image Quilting con cuadros del pack) entra como
   relieve: las juntas y lajas desplazan la banda de tono sin cambiar la paleta.

Todo color sale por `Lienzo.punto`, o sea por la paleta del pack.
"""

from __future__ import annotations

import math
import random
import sys
from pathlib import Path

import numpy as np
from scipy import ndimage

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from lib.pixel import Lienzo, luz, rampa  # noqa: E402

#: Dirección de la luz (x hacia la derecha, y hacia ABAJO): viene de arriba-izquierda.
LUZ = np.array([-0.55, -0.75, 0.62], dtype=np.float32)
LUZ /= np.linalg.norm(LUZ)


def blob(w: int, h: int, cx: float, cy: float, rx: float, ry: float, semilla: int = 0, rugosidad: float = 0.18) -> np.ndarray:
    """Máscara (h, w) de una forma redondeada e irregular."""
    rng = random.Random(semilla)
    n = 7
    fases = [rng.uniform(0, math.tau) for _ in range(n)]
    amps = [rng.uniform(0.3, 1.0) / (k + 1) for k in range(n)]
    ys, xs = np.mgrid[0:h, 0:w]
    dx, dy = (xs + 0.5 - cx) / rx, (ys + 0.5 - cy) / ry
    ang = np.arctan2(dy, dx)
    r = np.hypot(dx, dy)
    borde = 1.0 + rugosidad * sum(a * np.sin((k + 2) * ang + f) for k, (a, f) in enumerate(zip(amps, fases)))
    return r <= borde


def _moda(idx: np.ndarray, mask: np.ndarray, n: int) -> np.ndarray:
    """Filtro de moda 3x3 sobre el índice de tono: junta los píxeles sueltos en manchas."""
    out = idx.copy()
    H, W = idx.shape
    for j in range(H):
        for i in range(W):
            if not mask[j, i]:
                continue
            vec = [idx[b, a] for b in range(max(0, j - 1), min(H, j + 2)) for a in range(max(0, i - 1), min(W, i + 2)) if mask[b, a]]
            vals, cuentas = np.unique(vec, return_counts=True)
            # sólo cambia si el vecindario está claramente de acuerdo en otra cosa
            if cuentas.max() >= 5 and vals[cuentas.argmax()] != idx[j, i]:
                out[j, i] = vals[cuentas.argmax()]
    return out


def esculpir(
    l: Lienzo,
    mask: np.ndarray,
    tonos,
    dx: int = 0,
    dy: int = 0,
    relieve: float = 1.0,
    textura: np.ndarray | None = None,
    fuerza_textura: float = 0.35,
    suavizar: int = 1,
    base_oscura: float = 0.0,
):
    """Pinta `mask` en el lienzo con volumen de cúpula iluminado desde arriba-izquierda.

    `tonos`: la rampa del material, de oscuro a claro. `textura`: luminancia 0..255 (h, w) del
    material que entra como relieve. `base_oscura`: cuánto se hunde el borde de abajo (el lado en
    sombra del pack).
    """
    H, W = mask.shape
    edt = ndimage.distance_transform_edt(mask)
    dmax = max(float(edt.max()), 1.0)
    altura = np.sqrt(np.clip(edt / dmax, 0.0, 1.0)) * relieve * dmax * 0.9
    gy, gx = np.gradient(altura.astype(np.float32))
    nz = np.ones_like(gx)
    norm = np.sqrt(gx**2 + gy**2 + nz**2)
    n = np.stack([-gx / norm, -gy / norm, nz / norm], axis=-1)
    lamb = np.clip((n * LUZ).sum(-1), 0.0, 1.0)
    # normaliza a 0..1 dentro de la forma
    v = lamb[mask]
    lo, hi = float(np.percentile(v, 4)), float(np.percentile(v, 98))
    s = np.clip((lamb - lo) / max(hi - lo, 1e-3), 0.0, 1.0)
    if base_oscura:
        yy = np.mgrid[0:H, 0:W][0] / max(H - 1, 1)
        s = s - base_oscura * np.clip((yy - 0.55) / 0.45, 0.0, 1.0)
    if textura is not None:
        t = (textura.astype(np.float32) - textura[mask].mean()) / 255.0
        s = s + fuerza_textura * t
    s = np.clip(s, 0.0, 0.999)
    idx = (s * len(tonos)).astype(int)
    for _ in range(suavizar):
        idx = _moda(idx, mask, len(tonos))
    for j in range(H):
        for i in range(W):
            if mask[j, i]:
                l.punto(i + dx, j + dy, tonos[int(idx[j, i])])


def motas(l: Lienzo, mask: np.ndarray, color_base, n: int, semilla: int, tam=(1, 2), margen: int = 2):
    """Granos de mineral (vetas, pepitas) repartidos DENTRO de la forma: cada uno es un racimo de 2-4
    píxeles con un tono oscuro abajo-derecha y uno claro arriba-izquierda — así brillan."""
    rng = random.Random(semilla)
    r = rampa(color_base, 4)
    ys, xs = np.nonzero(ndimage.binary_erosion(mask, iterations=margen))
    if len(xs) == 0:
        return
    puestos = []
    for _ in range(n * 6):
        if len(puestos) >= n:
            break
        k = rng.randrange(len(xs))
        x, y = int(xs[k]), int(ys[k])
        if any(abs(x - a) + abs(y - b) < 5 for a, b in puestos):
            continue
        puestos.append((x, y))
        w = rng.randint(*tam) + 1
        h = rng.randint(*tam)
        for jj in range(h):
            for ii in range(w):
                l.punto(x + ii, y + jj, r[2])
        l.punto(x, y, r[3])
        l.punto(x + w - 1, y + h - 1, r[0])
