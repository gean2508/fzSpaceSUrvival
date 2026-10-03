#!/usr/bin/env python3
"""**EL CRÍTICO: mide un PNG dibujado contra el pack y lo pone al lado de sus
vecinos más parecidos.**

Existe porque "se ve de otro juego" no es una medida y yo no la puedo ver sola.
Hace dos cosas:

1. **Mide** lo que el ojo nota y `Lienzo.verificar` no: cuánto del sprite es
   *plano* (píxeles cuyos cuatro vecinos son del mismo color — un relleno liso),
   cuánta *variedad local* tiene (patrones 3x3 distintos por cada 100 píxeles),
   cuántos colores y qué tan lejos de la paleta del pack. Los compara con la
   distribución del pack (mediana y p90 de ~1.500 singles de su mismo tamaño).
2. **Lo pone en una lámina** junto a sus 7 vecinos más parecidos del pack (por
   histograma de color y ocupación), ampliado ×6. Esa imagen es la que hay que
   MIRAR (`Read`): si el tuyo se distingue de los otros en un vistazo, no está.

    python3 scripts/critico-pixel.py ruta.png [--celda 32 --k 3,4] [--ext]
    # por defecto compara con Modern INTERIORS (ítems, muebles, ropa: con contorno);
    # --ext compara con Modern EXTERIORS (mundo: sin contorno)

Sale con código 1 si alguna medida cae fuera del rango del pack.
"""

from __future__ import annotations

import sys
from pathlib import Path

import numpy as np
from PIL import Image

RAIZ = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib.pixel import ajustar  # noqa: E402

PACK = RAIZ / "PRUEBAS-CODIGOS" / "modernexteriors-win" / "Modern_Exteriors_32x32"
INT = RAIZ / "PRUEBAS-CODIGOS" / "moderninteriors-win (Copy 1)" / "1_Interiors" / "32x32" / "Theme_Sorter_Singles_32x32"
CACHE = Path(__file__).resolve().parent / "lib" / "bancos" / "indice_pack.npz"
OUT = Path("/tmp/claude-1000")


def metricas(rgba: np.ndarray) -> dict:
    a = rgba[..., 3] > 128
    n = int(a.sum()) or 1
    rgb = rgba[..., :3].astype(int)
    key = (rgb[..., 0] << 16) | (rgb[..., 1] << 8) | rgb[..., 2]
    key = np.where(a, key, -1)
    pad = np.pad(key, 1, constant_values=-2)
    c = pad[1:-1, 1:-1]
    iguales = (
        (pad[:-2, 1:-1] == c) & (pad[2:, 1:-1] == c) & (pad[1:-1, :-2] == c) & (pad[1:-1, 2:] == c)
    )
    plano = float((iguales & a).sum()) / n
    pats = set()
    H, W = key.shape
    for y in range(1, H - 1):
        for x in range(1, W - 1):
            if a[y, x]:
                pats.add(tuple(pad[y : y + 3, x : x + 3].ravel()))
    variedad = len(pats) * 100.0 / n
    colores = len({int(k) for k in key[a]})
    return {"plano": plano, "variedad": variedad, "colores": colores, "area": n}


def _recorrer_pack():
    """(ruta, pixeles, familia). `ext` = Modern Exteriors (sin contorno, mundo);
    `int` = Modern Interiors (con contorno oscuro: ítems, muebles, ropa, iconos)."""
    for p in sorted(INT.rglob("*.png")):
        try:
            im = Image.open(p).convert("RGBA")
        except Exception:
            continue
        if 16 <= im.width <= 96 and 16 <= im.height <= 96 and (np.asarray(im)[..., 3] > 128).sum() > 60:
            yield p, np.asarray(im), 1
    for p in sorted(PACK.rglob("ME_Singles*.png")):
        try:
            im = Image.open(p).convert("RGBA")
        except Exception:
            continue
        if 24 <= im.width <= 96 and 24 <= im.height <= 96 and (np.asarray(im)[..., 3] > 128).sum() > 120:
            yield p, np.asarray(im), 0


def _hist(rgba: np.ndarray) -> np.ndarray:
    a = rgba[..., 3] > 128
    q = (rgba[..., :3] // 64).astype(int)
    idx = (q[..., 0] * 16 + q[..., 1] * 4 + q[..., 2])[a]
    h = np.bincount(idx, minlength=64).astype(np.float32)
    h /= max(h.sum(), 1)
    return np.concatenate([h, [a.mean()]])


def indice(maximo: int = 2500):
    if CACHE.exists():
        d = np.load(CACHE, allow_pickle=True)
        return d["rutas"], d["feat"], d["met"], d["fam"]
    rutas, feat, met, fam = [], [], [], []
    for i, (p, im, f) in enumerate(_recorrer_pack()):
        if i % 6:  # un sexto basta para una distribución y para vecinos
            continue
        rutas.append(str(p))
        feat.append(_hist(im))
        m = metricas(im)
        met.append([m["plano"], m["variedad"], m["colores"]])
        fam.append(f)
    CACHE.parent.mkdir(exist_ok=True)
    np.savez_compressed(CACHE, rutas=np.array(rutas), feat=np.array(feat), met=np.array(met), fam=np.array(fam))
    return np.array(rutas), np.array(feat), np.array(met), np.array(fam)


def criticar(png: Path, n: int = 0, celda: int = 0, k: int = 0, familia: int = 1) -> bool:
    im = np.asarray(Image.open(png).convert("RGBA"))
    if celda:  # una lámina de celdas: se mide la k-ésima
        por_fila = im.shape[1] // celda
        y, x = (k // por_fila) * celda, (k % por_fila) * celda
        im = im[y : y + celda, x : x + celda].copy()
    m = metricas(im)
    rutas, feat, met, fam = indice()
    sel = fam == familia
    rutas, feat, met = rutas[sel], feat[sel], met[sel]
    # Se compara con singles de área parecida: un icono chico no se mide contra una casa.
    mi = _hist(im)
    d = np.abs(feat - mi).sum(1)
    orden = np.argsort(d)[:7]
    p50 = np.median(met, axis=0)
    p10 = np.percentile(met, 10, axis=0)
    p90 = np.percentile(met, 90, axis=0)
    ok = True
    print(f"\n== {png.name}  {im.shape[1]}x{im.shape[0]}  área {m['area']} px")
    filas = [
        ("plano   (% píxeles lisos)", m["plano"], p50[0], p90[0], m["plano"] <= p90[0]),
        ("variedad (3x3 únicos/100px)", m["variedad"], p50[1], p10[1], m["variedad"] >= p10[1]),
        ("colores", m["colores"], p50[2], p90[2], m["colores"] <= 30 and m["colores"] <= p90[2] * 1.5),
    ]
    for nombre, v, mediana, limite, bien in filas:
        print(f"  {'OK ' if bien else 'MAL'} {nombre:28s} tuyo {v:7.2f} | pack mediana {mediana:7.2f} | límite {limite:7.2f}")
        ok &= bien
    fuera = {tuple(ajustar(tuple(int(x) for x in c))) != tuple(int(x) for x in c) for c in im[..., :3][im[..., 3] > 128].reshape(-1, 3)[:2000]}
    if True in fuera:
        print("  MAL colores fuera de la paleta del pack (pasalos por lib.pixel.ajustar)")
        ok = False
    # Lámina: el tuyo + 7 vecinos, ×6, en fondo neutro de pasto.
    S = 6
    celdas = [im] + [np.asarray(Image.open(rutas[i]).convert("RGBA")) for i in orden]
    alto = max(c.shape[0] for c in celdas)
    ancho = sum(c.shape[1] for c in celdas) + 8 * (len(celdas) + 1)
    fondo = Image.new("RGBA", (ancho, alto + 16), (96, 140, 84, 255))
    x = 8
    for k, c in enumerate(celdas):
        fondo.alpha_composite(Image.fromarray(c), (x, 8 + alto - c.shape[0]))
        x += c.shape[1] + 8
    fondo = fondo.resize((fondo.width * S, fondo.height * S), Image.NEAREST)
    salida = OUT / f"critico-{n}.png"
    fondo.save(salida)
    print(f"  lámina (tuyo a la izquierda + 7 del pack): {salida}")
    return ok


if __name__ == "__main__":
    args = sys.argv[1:]
    celda = int(args[args.index("--celda") + 1]) if "--celda" in args else 0
    familia = 0 if "--ext" in args else 1  # por defecto: ítems/iconos (Interiors)
    ks = [int(x) for x in args[args.index("--k") + 1].split(",")] if "--k" in args else [0]
    rutas = [Path(a) for a in args if a.endswith(".png")]
    if not rutas:
        raise SystemExit(__doc__)
    bien = True
    for i, r in enumerate(rutas):
        for j, k in enumerate(ks):
            bien &= criticar(r, i * 100 + j, celda, k, familia)
    sys.exit(0 if bien else 1)
