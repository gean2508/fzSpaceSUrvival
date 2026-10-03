#!/usr/bin/env python3
"""**EL ÍCONO DEL JUEGO: un guerrero del Ki cruzando el amanecer sobre la Tierra.**

    python3 scripts/dibujar-icono.py [--vista]     # --vista: sólo deja la lámina en /tmp/claude-1000/icono.png

Pixel art de 108×108 (la grilla del ícono adaptativo de Android) ampliado ×4 con vecino más cercano → 432×432.
Dos capas, como pide el ícono adaptativo: FONDO (cielo, estrellas, nebulosa, la Tierra con su atmósfera y el sol
saliendo) y PRIMER PLANO (aura de Ki, estela, el guerrero). Todo dentro de la zona segura (72×72 centrales) para que
ninguna máscara lo corte. También se generan los íconos viejos (mipmap-*) para Android < 8.

Se dibuja con cúpulas iluminadas desde arriba-izquierda (`lib/volumen.py`), igual que el resto del arte.
"""
import math
import random
import sys
from pathlib import Path

import numpy as np
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
from scipy import ndimage  # noqa: E402

RAIZ = Path(__file__).resolve().parent.parent
RES = RAIZ / "native/android/app/src/main/res"
N = 108
ESC = 4


def lienzo():
    return np.zeros((N, N, 4), np.uint8)


def poner(a, x, y, c):
    if 0 <= x < N and 0 <= y < N:
        a[y, x] = (*c[:3], c[3] if len(c) > 3 else 255)


def mascara_poligono(pts):
    im = Image.new("L", (N, N), 0)
    from PIL import ImageDraw
    ImageDraw.Draw(im).polygon([(float(x), float(y)) for x, y in pts], fill=255)
    return np.asarray(im) > 0


def mascara_disco(cx, cy, rx, ry=None):
    ry = ry or rx
    ys, xs = np.mgrid[0:N, 0:N]
    return ((xs + 0.5 - cx) / rx) ** 2 + ((ys + 0.5 - cy) / ry) ** 2 <= 1.0


def rampa(base, n=5, claro=1.45, oscuro=0.45):
    """Tonos de oscuro a claro: el oscuro se tiñe hacia violeta, el claro hacia amarillo (como el pack)."""
    r, g, b = base
    out = []
    for i in range(n):
        t = i / (n - 1)
        k = oscuro + (claro - oscuro) * t
        c = [r * k, g * k, b * k]
        if t < 0.5:
            c = [c[0] * 0.95 + 10 * (0.5 - t), c[1] * 0.9, c[2] * 1.12 + 14 * (0.5 - t)]
        else:
            c = [c[0] + 18 * (t - 0.5), c[1] + 12 * (t - 0.5), c[2] - 6 * (t - 0.5)]
        out.append(tuple(int(max(0, min(255, v))) for v in c))
    return out


LUZ = np.array([-0.55, -0.75, 0.62], np.float32)
LUZ /= np.linalg.norm(LUZ)


def esculpir(a, mask, tonos, relieve=1.4, base_oscura=0.1, fuerza=1.0):
    """Cúpula iluminada desde arriba-izquierda, cuantizada a manchas de la rampa."""
    edt = ndimage.distance_transform_edt(mask)
    dmax = max(float(edt.max()), 1.0)
    h = np.sqrt(np.clip(edt / dmax, 0, 1)) * relieve * dmax * 0.9
    gy, gx = np.gradient(h.astype(np.float32))
    n = np.stack([-gx, -gy, np.ones_like(gx)], -1)
    n /= np.linalg.norm(n, axis=-1, keepdims=True)
    lamb = np.clip((n * LUZ).sum(-1), 0, 1)
    v = lamb[mask]
    lo, hi = np.percentile(v, 4), np.percentile(v, 98)
    s = np.clip((lamb - lo) / max(hi - lo, 1e-3), 0, 0.999)
    yy = np.mgrid[0:N, 0:N][0]
    ys = np.nonzero(mask)[0]
    if len(ys):
        s = s - base_oscura * np.clip((yy - ys.min() - 0.55 * (ys.max() - ys.min())) / max(1, 0.45 * (ys.max() - ys.min())), 0, 1)
    idx = np.clip((s * len(tonos)).astype(int), 0, len(tonos) - 1)
    for j, i in zip(*np.nonzero(mask)):
        poner(a, i, j, tonos[idx[j, i]])


def contorno(a, color=(24, 18, 44)):
    op = a[..., 3] > 200
    borde = ndimage.binary_dilation(op) & ~op
    for j, i in zip(*np.nonzero(borde)):
        poner(a, i, j, color)


# ── FONDO ──────────────────────────────────────────────────────────────────────────────────────────
def fondo():
    rng = random.Random(7)
    a = lienzo()
    # cielo: bandas de violeta profundo a índigo, más cálido hacia el horizonte (abajo-izquierda)
    bandas = [(10, 8, 30), (14, 11, 42), (19, 14, 56), (26, 18, 70), (36, 22, 84), (52, 28, 92), (78, 36, 94)]
    for j in range(N):
        for i in range(N):
            t = (j * 0.75 + (N - i) * 0.25) / N
            k = min(len(bandas) - 1, int(t ** 1.4 * len(bandas)))
            poner(a, i, j, bandas[k])
    # nebulosa: dos manchas suaves en bandas
    for cx, cy, rx, ry, col in [(78, 24, 34, 16, (120, 46, 140)), (24, 38, 26, 12, (46, 70, 150))]:
        ys, xs = np.mgrid[0:N, 0:N]
        d = ((xs - cx) / rx) ** 2 + ((ys - cy) / ry) ** 2
        for lvl, alpha in ((1.0, 0.16), (0.55, 0.16), (0.25, 0.18)):
            m = d <= lvl
            for j, i in zip(*np.nonzero(m)):
                p = a[j, i, :3].astype(np.float32)
                a[j, i, :3] = (p * (1 - alpha) + np.array(col) * alpha).astype(np.uint8)
    # estrellas
    for _ in range(46):
        x, y = rng.randrange(N), rng.randrange(int(N * 0.62))
        poner(a, x, y, (226, 222, 255) if rng.random() < 0.6 else (255, 226, 160))
    for x, y in [(14, 14), (92, 12), (60, 8), (100, 44), (8, 52)]:      # destellos en cruz
        for dx, dy in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)]:
            poner(a, x + dx, y + dy, (255, 250, 230))
        poner(a, x + 2, y, (170, 160, 240)); poner(a, x - 2, y, (170, 160, 240))
        poner(a, x, y + 2, (170, 160, 240)); poner(a, x, y - 2, (170, 160, 240))
    # la TIERRA: un disco enorme cuyo borde de arriba cruza la parte baja
    cx, cy, R = 40, 168, 100
    tierra = mascara_disco(cx, cy, R)
    atm_ext = mascara_disco(cx, cy, R + 5)
    atm_med = mascara_disco(cx, cy, R + 3)
    atm_int = mascara_disco(cx, cy, R + 1)
    for m, col in ((atm_ext, (40, 90, 200)), (atm_med, (80, 160, 240)), (atm_int, (170, 225, 255))):
        for j, i in zip(*np.nonzero(m & ~tierra)):
            poner(a, i, j, col)
    oceano = rampa((38, 98, 178), 6, 1.5, 0.45)
    esculpir(a, tierra, oceano, relieve=0.5, base_oscura=0.15)
    # continentes (manchas verdes con relieve) y nubes
    cont = Image.new("L", (N, N), 0)
    from PIL import ImageDraw
    d = ImageDraw.Draw(cont)
    for x, y, rx, ry in [(24, 90, 13, 5), (60, 84, 8, 3), (86, 94, 10, 5), (10, 80, 5, 3), (46, 101, 14, 5)]:
        d.ellipse([x - rx, y - ry, x + rx, y + ry], fill=255)
    cont = np.asarray(cont.filter(__import__("PIL.ImageFilter", fromlist=["x"]).GaussianBlur(1.2))) > 150
    verde = rampa((70, 140, 70), 5, 1.4, 0.5)
    sub = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    suba = np.asarray(sub).copy()
    esculpir(suba, cont & tierra & mascara_disco(40, 168, 96), verde, relieve=0.45, base_oscura=0.25)
    for j, i in zip(*np.nonzero(suba[..., 3] > 0)):
        a[j, i] = suba[j, i]
    for x, y, w in [(18, 80, 12), (66, 78, 10), (50, 94, 14), (90, 86, 9)]:
        for k in range(w):
            poner(a, x + k, y + (k % 3 == 0), (240, 244, 255))
            poner(a, x + k, y + 1 + (k % 3 == 0), (190, 205, 235))
    # amanecer en el limbo (arriba-derecha de la Tierra): resplandor dorado
    sol = (82, 76)
    for rad, col in ((12, (255, 214, 120, 255)), (9, (255, 236, 170, 255)), (6, (255, 252, 226, 255))):
        for j, i in zip(*np.nonzero(mascara_disco(*sol, rad) & atm_ext)):
            poner(a, i, j, col[:3])
    return a


# ── PRIMER PLANO ───────────────────────────────────────────────────────────────────────────────────
def aura(a, centro, escala=1.0, semilla=3):
    """Llama de Ki: tres capas translúcidas (naranja, oro, blanco) con lenguas que suben; se funden con lo de atrás."""
    rng = random.Random(semilla)
    cx, cy = centro
    capas = [((255, 140, 24), 1.0, 175), ((255, 204, 60), 0.80, 225), ((255, 246, 200), 0.56, 255)]
    for col, k, alfa in capas:
        pts = []
        picos = 9
        for i in range(picos * 2):
            ang = math.tau * i / (picos * 2)
            r = (30 if i % 2 == 0 else 25) * k * escala
            r *= 1.0 + 0.10 * math.sin(ang * 3 + 1.0) + rng.uniform(-0.03, 0.05)
            if math.sin(ang) < -0.3:
                r *= 1.28
            pts.append((cx + math.cos(ang) * r, cy + math.sin(ang) * r * 1.08))
        m = mascara_poligono(pts)
        for j, i in zip(*np.nonzero(m)):
            a[j, i] = (*col, alfa)


def estela(a):
    """Rayos de velocidad hacia atrás (abajo-izquierda)."""
    for (x0, y0, x1, y1, g, col) in [(40, 66, 12, 92, 3, (255, 190, 70)), (46, 72, 22, 98, 2, (255, 226, 130)),
                                      (34, 58, 8, 80, 2, (255, 150, 40)), (52, 76, 36, 100, 2, (255, 240, 190))]:
        n = max(abs(x1 - x0), abs(y1 - y0))
        for k in range(n):
            t = k / n
            w = max(0.5, g * (1 - t))
            x, y = x0 + (x1 - x0) * t, y0 + (y1 - y0) * t
            for dx in range(-int(w), int(w) + 1):
                poner(a, int(x) + dx, int(y), col if t < 0.7 else (col[0] // 2 + 60, col[1] // 2 + 40, 120))


def guerrero(a):
    """Cuerpo en vuelo diagonal (hacia arriba-derecha), puño adelante, pelo dorado en púas."""
    piel = rampa((236, 184, 148), 5, 1.25, 0.6)
    gi = rampa((236, 112, 24), 5, 1.3, 0.5)
    azul = rampa((40, 84, 190), 5, 1.35, 0.5)
    oro = rampa((255, 210, 50), 5, 1.2, 0.65)
    bota = rampa((210, 60, 50), 4, 1.2, 0.6)
    # piernas (hacia atrás-abajo)
    esculpir(a, mascara_poligono([(46, 62), (53, 58), (47, 76), (41, 74)]), azul, relieve=1.0)   # pierna de atrás
    esculpir(a, mascara_poligono([(53, 58), (60, 60), (56, 78), (49, 78)]), azul, relieve=1.0)   # pierna de adelante
    esculpir(a, mascara_poligono([(40, 73), (47, 76), (46, 81), (38, 79)]), bota, relieve=0.8)
    esculpir(a, mascara_poligono([(49, 77), (56, 78), (57, 83), (49, 82)]), bota, relieve=0.8)
    # torso
    esculpir(a, mascara_poligono([(45, 44), (60, 42), (62, 60), (50, 62), (44, 56)]), gi, relieve=1.2)
    esculpir(a, mascara_poligono([(45, 56), (62, 58), (62, 61), (49, 63), (44, 59)]), azul, relieve=0.8)   # faja
    # brazo de atrás (cruzado) y brazo que apunta adelante con puño
    esculpir(a, mascara_poligono([(46, 46), (40, 52), (38, 49), (43, 43)]), piel, relieve=0.9)
    esculpir(a, mascara_poligono([(58, 45), (71, 37), (73, 40), (61, 49)]), piel, relieve=1.0)
    esculpir(a, mascara_disco(75, 37, 4.2, 4.2), piel, relieve=1.4)                                      # puño
    # cabeza
    esculpir(a, mascara_disco(54, 35, 8.6, 9.2), piel, relieve=1.5)
    # pelo en púas, dorado (6 púas)
    pelo = mascara_poligono([(44, 36), (41, 24), (47, 29), (46, 15), (52, 25), (55, 11), (58, 24), (66, 15), (63, 28), (71, 26), (63, 36), (58, 31), (50, 32)])
    esculpir(a, pelo, oro, relieve=1.1, base_oscura=0.0)
    # cara
    for x, y in ((56, 38), (60, 38)):
        poner(a, x, y, (20, 30, 70)); poner(a, x, y - 1, (240, 250, 255))
    poner(a, 55, 36, (70, 40, 30)); poner(a, 56, 36, (70, 40, 30)); poner(a, 60, 36, (70, 40, 30)); poner(a, 61, 36, (70, 40, 30))
    poner(a, 58, 42, (150, 70, 60)); poner(a, 59, 42, (150, 70, 60))
    poner(a, 51, 40, (230, 130, 110)); poner(a, 52, 40, (230, 130, 110))


def primer_plano():
    a = lienzo()
    aura(a, (57, 50), 1.0)
    estela(a)
    # chispas de Ki
    rng = random.Random(11)
    for _ in range(20):
        ang = rng.uniform(0, math.tau)
        r = rng.uniform(26, 38)
        x, y = int(54 + math.cos(ang) * r), int(52 + math.sin(ang) * r * 1.1)
        poner(a, x, y, (255, 240, 170)); poner(a, x + 1, y, (255, 200, 80))
    # el guerrero con su contorno
    g = lienzo()
    guerrero(g)
    gi = Image.fromarray(g, "RGBA").rotate(-32, resample=Image.NEAREST, center=(56, 52))
    g = np.asarray(gi).copy()
    contorno(g)
    for j, i in zip(*np.nonzero(g[..., 3] > 0)):
        a[j, i] = g[j, i]
    # destello blanco en el puño
    for dx, dy in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1), (2, 0), (-2, 0), (0, 2), (0, -2)]:
        poner(a, 79 + dx, 33 + dy, (255, 255, 240))
    return a


def ampliar(a):
    return Image.fromarray(a, "RGBA").resize((N * ESC, N * ESC), Image.NEAREST)


def main():
    bg, fg = fondo(), primer_plano()
    bg_im, fg_im = ampliar(bg), ampliar(fg)
    comp = bg_im.copy()
    comp.alpha_composite(fg_im)
    Path("/tmp/claude-1000").mkdir(exist_ok=True)
    comp.convert("RGB").save("/tmp/claude-1000/icono.png")
    if "--vista" in sys.argv:
        return
    (RES / "drawable-nodpi").mkdir(exist_ok=True)
    bg_im.convert("RGB").save(RES / "drawable-nodpi/ic_launcher_background.png", optimize=True)
    fg_im.save(RES / "drawable-nodpi/ic_launcher_foreground.png", optimize=True)
    xml = RES / "drawable/ic_launcher_background.xml"
    if xml.exists():
        xml.unlink()
    adapt = '<?xml version="1.0" encoding="utf-8"?>\n<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">\n    <background android:drawable="@drawable/ic_launcher_background" />\n    <foreground android:drawable="@drawable/ic_launcher_foreground" />\n</adaptive-icon>\n'
    for n in ("ic_launcher.xml", "ic_launcher_round.xml"):
        (RES / "mipmap-anydpi-v26" / n).write_text(adapt)
    # íconos clásicos (Android < 8): la composición, con esquinas redondeadas y versión circular
    tam = {"mdpi": 48, "hdpi": 72, "xhdpi": 96, "xxhdpi": 144, "xxxhdpi": 192}
    from PIL import ImageDraw
    for d, t in tam.items():
        base = comp.resize((t, t), Image.LANCZOS).convert("RGBA")
        m = Image.new("L", (t * 4, t * 4), 0)
        ImageDraw.Draw(m).rounded_rectangle([0, 0, t * 4 - 1, t * 4 - 1], radius=t * 4 // 5, fill=255)
        sq = base.copy(); sq.putalpha(m.resize((t, t), Image.LANCZOS))
        mc = Image.new("L", (t * 4, t * 4), 0)
        ImageDraw.Draw(mc).ellipse([0, 0, t * 4 - 1, t * 4 - 1], fill=255)
        rd = base.copy(); rd.putalpha(mc.resize((t, t), Image.LANCZOS))
        sq.save(RES / f"mipmap-{d}/ic_launcher.png", optimize=True)
        rd.save(RES / f"mipmap-{d}/ic_launcher_round.png", optimize=True)
    print("ícono escrito: fondo, primer plano y mipmaps")


if __name__ == "__main__":
    main()
