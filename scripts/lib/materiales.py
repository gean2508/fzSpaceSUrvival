#!/usr/bin/env python3
"""**TEXTURA REAL DEL PACK, NO COLOR PLANO.**

Lo que le faltaba a lo que dibujo a mano: un relleno de un solo tono (o una rampa
de tres) no tiene la *textura* de un tejado, una veta de madera o un ladrillo, y
eso es lo que se ve "de otro juego" al lado del pack. Esta librería sintetiza
superficies **con los píxeles del propio pack**, así que el relleno sale con su
paleta, su grano y sus juntas por construcción.

Técnica: **Image Quilting** (Efros & Freeman, SIGGRAPH 2001) — se arma la
superficie con bloques de una muestra real, eligiendo en cada paso el bloque que
mejor empalma con lo ya puesto y cortando la costura por el camino de menor
error. Con una **guía de luminancia** (`guia=`) se hace *texture transfer*: la
textura sigue el sombreado de la forma que dibujaste (la tapa clara, el lado en
sombra), en vez de salir pareja.

    from lib.materiales import banco, sintetizar
    tiles = banco("madera")                    # muestras opacas del pack
    img = sintetizar(tiles[0], 24, 14, semilla=3, guia=luz_de_la_forma)

El banco se arma solo la primera vez (recorre los PNG de `PRUEBAS-CODIGOS/`, se
queda con los cuadros 32x32 totalmente opacos y los clasifica por nombre y por
color) y se cachea en `scripts/lib/bancos/`.
"""

from __future__ import annotations

import colorsys
import json
import random
from pathlib import Path

import numpy as np
from PIL import Image

RAIZ = Path(__file__).resolve().parent.parent.parent
PACKS = RAIZ / "PRUEBAS-CODIGOS"
CACHE = Path(__file__).resolve().parent / "bancos"
CELDA = 32

# Materiales y cómo se reconocen. `nombre`: palabras del archivo (Exteriors tiene
# nombres descriptivos). `color`: (hue_min, hue_max, sat_min, sat_max, lum_min, lum_max)
# para clasificar los cuadros de las láminas del Room Builder, que no los tienen.
MATERIALES = {
    "techo": {"nombre": ["roof"]},
    "pared_ext": {"nombre": ["terrains_and_fences_32x32_wall"]},
    "pasto": {"nombre": ["grass_wall", "grass_"]},
    "madera": {"color": (15, 45, 0.22, 0.75, 40, 190), "hoja": ["floors", "walls"]},
    "ladrillo": {"color": (0, 18, 0.30, 0.80, 50, 190), "hoja": ["walls", "floors"]},
    "piedra": {"color": (0, 360, 0.0, 0.14, 60, 200), "hoja": ["floors", "walls"]},
}


def _tiles_opacos(png: Path, tam: int = CELDA):
    try:
        im = Image.open(png).convert("RGBA")
    except Exception:
        return
    a = np.asarray(im)
    H, W = a.shape[:2]
    if W == tam and H == tam:
        if a[..., 3].min() == 255:
            yield a[..., :3].copy()
        return
    if W % tam or H % tam or W > 2048:
        return
    for y in range(0, H, tam):
        for x in range(0, W, tam):
            t = a[y : y + tam, x : x + tam]
            if t[..., 3].min() == 255 and len({tuple(p) for p in t[..., :3].reshape(-1, 3)}) > 3:
                yield t[..., :3].copy()


def _seamless(t) -> float:
    """Cuánto 'cierra' el cuadro contra sí mismo (menor = más repetible). Se compara
    el salto borde↔borde opuesto con el salto normal entre columnas/filas vecinas:
    una textura de material (piso, pared, techo) da ≈1; una pieza de edificio (ventana,
    aleros, orilla) da mucho más."""
    f = t.astype(np.float32)
    col = np.abs(f[:, 0] - f[:, -1]).mean()
    fil = np.abs(f[0] - f[-1]).mean()
    veci_c = np.abs(f[:, 1:] - f[:, :-1]).mean() + 1e-3
    veci_f = np.abs(f[1:] - f[:-1]).mean() + 1e-3
    return float(max(col / veci_c, fil / veci_f))


def _sirve(t) -> bool:
    """Descarta lo que no es superficie: blancos de pared, agua, planos sin textura."""
    lum = _lum(t.astype(np.float32))
    if (lum > 235).mean() > 0.06:
        return False
    # Sin damero: el pack no tramea (las sombras traslúcidas del Room Builder sí).
    iz, ce, de = t[:, :-2].astype(int), t[:, 1:-1].astype(int), t[:, 2:].astype(int)
    damero = ((np.abs(iz - de).sum(-1) < 6) & (np.abs(iz - ce).sum(-1) > 24)).mean()
    if damero > 0.08:
        return False
    px = t.reshape(-1, 3)
    azul = ((px[:, 2] > px[:, 0] + 40) & (px[:, 2] > px[:, 1] - 10)).mean()
    if azul > 0.10:
        return False
    return len({tuple(p) for p in px}) >= 4


def _color_medio(t):
    m = t.reshape(-1, 3).mean(axis=0) / 255.0
    h, s, v = colorsys.rgb_to_hsv(*m)
    lum = 0.299 * m[0] * 255 + 0.587 * m[1] * 255 + 0.114 * m[2] * 255
    return h * 360, s, lum


def _construir(material: str, maximo: int = 48):
    reglas = MATERIALES[material]
    elegidos = []
    for png in sorted(PACKS.rglob("*.png")):
        n = png.name.lower()
        if "nombre" in reglas and any(k in n for k in reglas["nombre"]):
            ok_nombre = True
        else:
            ok_nombre = False
        ok_hoja = "hoja" in reglas and any(k in n for k in reglas["hoja"]) and "32x32" in n
        if not (ok_nombre or ok_hoja):
            continue
        for t in _tiles_opacos(png):
            if ok_nombre:
                elegidos.append(t)
            else:
                h, s, l = _color_medio(t)
                h0, h1, s0, s1, l0, l1 = reglas["color"]
                if h0 <= h <= h1 and s0 <= s <= s1 and l0 <= l <= l1:
                    elegidos.append(t)
        if len(elegidos) > maximo * 6:
            break
    # Sólo superficies repetibles; entre ellas, las que más textura traen.
    utiles = [(t, _seamless(t)) for t in elegidos if _sirve(t)]
    buenas = [t for t, sc in utiles if sc <= 1.6]
    if len(buenas) < 6:
        buenas = [t for t, _ in sorted(utiles, key=lambda x: x[1])[:12]]
    buenas.sort(key=lambda t: -len({tuple(p) for p in t.reshape(-1, 3)}))
    return buenas[:maximo]


def banco(material: str) -> list[np.ndarray]:
    """Cuadros 32x32 opacos del pack para ese material (RGB, uint8)."""
    CACHE.mkdir(exist_ok=True)
    f = CACHE / f"{material}.npy"
    if f.exists():
        return list(np.load(f))
    t = _construir(material)
    if not t:
        raise SystemExit(f"materiales: no encontré cuadros de «{material}» en {PACKS}")
    np.save(f, np.stack(t))
    return t


def _lum(a):
    return a[..., 0] * 0.299 + a[..., 1] * 0.587 + a[..., 2] * 0.114


def _corte_vertical(err):
    """Camino de menor error de arriba a abajo (DP). `err`: (alto, ancho)."""
    h, w = err.shape
    costo = err.copy()
    for y in range(1, h):
        for x in range(w):
            lo, hi = max(0, x - 1), min(w, x + 2)
            costo[y, x] += costo[y - 1, lo:hi].min()
    x = int(costo[-1].argmin())
    camino = [x]
    for y in range(h - 2, -1, -1):
        lo, hi = max(0, x - 1), min(w, x + 2)
        x = lo + int(costo[y, lo:hi].argmin())
        camino.append(x)
    return camino[::-1]


def sintetizar(
    muestra: np.ndarray,
    w: int,
    h: int,
    semilla: int = 0,
    guia: np.ndarray | None = None,
    bloque: int = 8,
    solape: int = 3,
    alfa: float = 0.55,
    candidatos: int = 60,
) -> np.ndarray:
    """Superficie (h, w, 3) con la textura de `muestra`, hecha de píxeles del pack.

    `guia`: luminancia objetivo (h, w) 0..255 — activa el *texture transfer*:
    cada bloque se elige mitad por empalme y mitad por parecerse al sombreado
    que dibujaste (`alfa` = peso del empalme).
    """
    rng = random.Random(semilla)
    src = np.tile(muestra.astype(np.float32), (2, 2, 1))  # la muestra es seamless
    SH, SW = src.shape[:2]
    out = np.zeros((h + bloque, w + bloque, 3), np.float32)
    puesto = np.zeros(out.shape[:2], bool)
    paso = bloque - solape
    g = None
    if guia is not None:
        g = np.zeros(out.shape[:2], np.float32)
        g[:h, :w] = guia
        g[h:, :] = g[h - 1 : h, :].mean() if h else 0
        g[:, w:] = g[:, w - 1 : w].mean() if w else 0
    slum = _lum(src)

    def candidato(y, x):
        mejores = []
        for _ in range(candidatos):
            sy, sx = rng.randrange(0, SH - bloque), rng.randrange(0, SW - bloque)
            blk = src[sy : sy + bloque, sx : sx + bloque]
            err = 0.0
            m = puesto[y : y + bloque, x : x + bloque]
            if m.any():
                d = ((blk - out[y : y + bloque, x : x + bloque]) ** 2).sum(-1)
                err = float(d[m].mean()) / (3 * 255**2) * 4
            gerr = 0.0
            if g is not None:
                gerr = float(((slum[sy : sy + bloque, sx : sx + bloque] - g[y : y + bloque, x : x + bloque]) ** 2).mean()) / 255**2
            mejores.append((alfa * err + (1 - alfa) * gerr if g is not None else err, sy, sx))
        mejores.sort()
        _, sy, sx = mejores[min(len(mejores) - 1, rng.randrange(0, 3))]
        return src[sy : sy + bloque, sx : sx + bloque]

    for y in range(0, h, paso):
        for x in range(0, w, paso):
            blk = candidato(y, x).copy()
            ya = puesto[y : y + bloque, x : x + bloque]
            if ya.any():
                old = out[y : y + bloque, x : x + bloque]
                d = ((blk - old) ** 2).sum(-1)
                mask = np.ones((bloque, bloque), bool)  # True = tomar el bloque nuevo
                if x > 0 and ya[:, :solape].all():
                    cam = _corte_vertical(d[:, :solape])
                    for r, c in enumerate(cam):
                        mask[r, :c] = False
                if y > 0 and ya[:solape, :].all():
                    cam = _corte_vertical(d[:solape, :].T)
                    for c, r in enumerate(cam):
                        mask[:r, c] = False
                mask |= ~ya  # lo que todavía no se pintó es del bloque nuevo
                blk = np.where(mask[..., None], blk, old)
            out[y : y + bloque, x : x + bloque] = blk
            puesto[y : y + bloque, x : x + bloque] = True
    return np.clip(out[:h, :w] + 0.5, 0, 255).astype(np.uint8)
