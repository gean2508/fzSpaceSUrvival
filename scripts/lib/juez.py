#!/usr/bin/env python3
"""**EL JUEZ DEL REEMPLAZO: ¿la pieza dibujada está a la altura del asset del pack que viene a reemplazar?**

`lib/volumen.py::exigir_liston` es un PISO (corta lo «de Paint»). `critico-pixel.py` compara contra el pack en
general. Ninguno de los dos contesta la pregunta que importa cuando se quiere **sacar un asset comprado y poner
uno nuestro** (3 oct 2026: «solo si los dibujos están a la altura de los assets o los superan»):

    ¿esta pieza, puesta en el lugar de ESA otra, pierde algo?

Por eso el juez mide **celda por celda** (32x32), no el sprite entero: una casa de 12x15 celdas puede tener un
promedio de detalle lindo y aun así tres celdas de pared lisa «de Paint» que el ojo encuentra en un segundo. Lo
que manda es la celda más pobre (percentil 10), no el promedio.

Medidas (todas sobre píxeles opacos, alfa > 200):

| medida | qué caza | sentido |
|---|---|---|
| `variedad` | patrones 3x3 distintos por cada 100 px, mediana por celda | más = más oficio |
| `variedad_p10` | lo mismo en la celda más pobre (p10) | el parche liso que delata |
| `plano` | % de píxeles con sus 4 vecinos del mismo color | menos = mejor |
| `bordes` | % de pares vecinos de distinto color | grano/juntas/detalle |
| `tonos` | colores con peso por celda (≥ 2 % del área) | volumen de verdad |
| `paso` | mediana del salto de luminancia entre vecinos distintos | ni plastificado ni ruido |
| `colores` | colores distintos por celda, p90 | techo del pack |
| `luz` | luminancia arriba-izquierda menos abajo-derecha | la luz del pack |
| `huecos` | celdas casi vacías dentro de la caja opaca | una pieza que «crece» rellenando con transparente |
| `ancho`/`alto` | caja opaca | la escala: área entre 0,6× y 1,6× de la del pack |

**Veredicto.** `A LA ALTURA` si todas caen dentro de la banda de las referencias (lo peor de ellas con 15 % de
gracia en el sentido malo; el `paso` en [0,6×, 1,7×] de su mediana; los colores nunca por encima de 30 por celda).
`SUPERA` si además gana en `variedad` y `variedad_p10` y no pierde en `plano`. Si no, `POR DEBAJO` y la lista de
qué falta. ⚠ Igual que el listón: **los números son necesarios, no suficientes.** La última palabra es la lámina a
ciegas (`lamina_a_ciegas`): si al mirarla se adivina cuál es la dibujada, no reemplaza, aunque los números pasen.
"""

from __future__ import annotations

import json
import random
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

CELDA = 32
SENTIDO = {  # +1: más es mejor · -1: menos es mejor · 0: banda
    "variedad": +1,
    "variedad_p10": +1,
    "plano": -1,
    "bordes": +1,
    "tonos": +1,
    "paso": 0,
    "colores": -1,
    "luz": 0,
    "huecos": -1,
}


def _rgba(im) -> np.ndarray:
    if isinstance(im, (str, Path)):
        im = Image.open(im)
    if isinstance(im, Image.Image):
        return np.asarray(im.convert("RGBA"))
    return im


def _lum(rgb):
    return rgb[..., 0] * 0.299 + rgb[..., 1] * 0.587 + rgb[..., 2] * 0.114


def _celda(t: np.ndarray) -> dict | None:
    """Medidas de una celda; None si casi no tiene píxeles opacos."""
    a = t[..., 3] > 200
    n = int(a.sum())
    if n < CELDA * CELDA * 0.35:
        return None
    rgb = t[..., :3].astype(np.int64)
    key = np.where(a, (rgb[..., 0] << 16) | (rgb[..., 1] << 8) | rgb[..., 2], -1)
    pad = np.pad(key, 1, constant_values=-2)
    c = pad[1:-1, 1:-1]
    vec = [pad[:-2, 1:-1], pad[2:, 1:-1], pad[1:-1, :-2], pad[1:-1, 2:]]
    iguales = np.logical_and.reduce([v == c for v in vec])
    plano = float((iguales & a).sum()) / n
    pats = set()
    H, W = key.shape
    for y in range(1, H - 1):
        for x in range(1, W - 1):
            if a[y, x]:
                pats.add(pad[y : y + 3, x : x + 3].tobytes())
    variedad = len(pats) * 100.0 / n
    # bordes y paso: pares horizontales y verticales, ambos opacos
    lum = _lum(t[..., :3].astype(np.float32))
    difs, total = [], 0
    for (k1, l1), (k2, l2), (m1, m2) in (
        ((key[:, :-1], lum[:, :-1]), (key[:, 1:], lum[:, 1:]), (a[:, :-1], a[:, 1:])),
        ((key[:-1], lum[:-1]), (key[1:], lum[1:]), (a[:-1], a[1:])),
    ):
        ambos = m1 & m2
        total += int(ambos.sum())
        dif = ambos & (k1 != k2)
        difs.append(np.abs(l1 - l2)[dif])
    difs = np.concatenate(difs)
    bordes = float(len(difs)) / max(total, 1)
    paso = float(np.median(difs)) if len(difs) else 0.0
    vals, cuentas = np.unique(key[a], return_counts=True)
    tonos = int((cuentas >= 0.02 * n).sum())
    return {"variedad": variedad, "plano": plano, "bordes": bordes, "tonos": tonos, "paso": paso, "colores": len(vals)}


def medir(im) -> dict:
    """Medidas del sprite entero, agregadas por celda de 32x32."""
    a = _rgba(im)
    H, W = a.shape[:2]
    celdas = []
    for y in range(0, H - H % CELDA or H, CELDA):
        for x in range(0, W - W % CELDA or W, CELDA):
            m = _celda(a[y : y + CELDA, x : x + CELDA])
            if m:
                celdas.append(m)
    if not celdas:
        raise SystemExit("juez: la pieza no tiene ninguna celda de 32x32 con al menos 35 % opaco")
    col = {k: np.array([c[k] for c in celdas], dtype=np.float32) for k in celdas[0]}
    op = a[..., 3] > 200
    ys, xs = np.nonzero(op)
    # huecos: celdas de la caja opaca que quedaron casi vacías (una pieza re-medida que rellena con transparente)
    caja = op[ys.min() : ys.max() + 1, xs.min() : xs.max() + 1]
    ch, cw = max(1, caja.shape[0] // CELDA), max(1, caja.shape[1] // CELDA)
    vacias = sum(caja[j * CELDA : (j + 1) * CELDA, i * CELDA : (i + 1) * CELDA].mean() < 0.35 for j in range(ch) for i in range(cw))
    huecos = vacias / float(ch * cw)
    lum = _lum(a[..., :3].astype(np.float32))
    diag = ys + xs <= ys.mean() + xs.mean()
    luz = float(lum[ys[diag], xs[diag]].mean() - lum[ys[~diag], xs[~diag]].mean())
    return {
        "celdas": len(celdas),
        "variedad": float(np.median(col["variedad"])),
        "variedad_p10": float(np.percentile(col["variedad"], 10)),
        "plano": float(np.median(col["plano"])),
        "bordes": float(np.median(col["bordes"])),
        "tonos": float(np.median(col["tonos"])),
        "paso": float(np.median(col["paso"])),
        "colores": float(np.percentile(col["colores"], 90)),
        "luz": luz,
        "huecos": float(huecos),
        "ancho": int(xs.max() - xs.min() + 1),
        "alto": int(ys.max() - ys.min() + 1),
    }


def juzgar(candidata, referencias: list, escala: bool = True) -> dict:
    """Compara la candidata contra el/los asset(s) del pack que reemplazaría.

    `escala=False` cuando la pieza se re-mide A PROPÓSITO (una fachada llevada a la huella de la sim): ahí que no
    ocupe lo mismo es la idea, y lo que se juzga es el oficio por celda.

    Devuelve `{"veredicto": "SUPERA"|"A LA ALTURA"|"POR DEBAJO", "faltas": [...], "tuya": {...}, "pack": [...]}`."""
    tuya = medir(candidata)
    pack = [medir(r) for r in referencias]
    faltas = []
    for k, s in SENTIDO.items():
        refs = np.array([p[k] for p in pack])
        v = tuya[k]
        if s > 0 and v < refs.min() * 0.85:
            faltas.append(f"{k} {v:.2f} < {refs.min() * 0.85:.2f} (lo peor del pack {refs.min():.2f} −15 %)")
        elif s < 0 and k not in ("colores", "huecos") and v > refs.max() * 1.15 + 0.01:
            faltas.append(f"{k} {v:.2f} > {refs.max() * 1.15:.2f} (lo peor del pack {refs.max():.2f} +15 %)")
        elif k == "colores" and v > max(30.0, refs.max() * 1.3):
            faltas.append(f"colores por celda {v:.0f}: el pack no pasa de {max(30.0, refs.max()):.0f}")
        elif k == "paso":
            med = float(np.median(refs))
            if not (0.6 * med <= v <= 1.7 * med):
                faltas.append(f"paso de luz {v:.1f} fuera de [{0.6 * med:.1f}, {1.7 * med:.1f}] (pack {med:.1f})")
        elif k == "huecos" and v > refs.max() + 0.08:
            faltas.append(f"huecos {v:.2f}: celdas vacías dentro de la caja (pack {refs.max():.2f})")
        elif k == "luz" and v < min(refs.min(), 0.0) - 20:
            faltas.append(f"luz {v:.0f}: viene de abajo-derecha (pack {refs.min():.0f})")
    # escala: la pieza ocupa en el mundo lo mismo que la que reemplaza (el taxi de 1 celda al lado de autos de
    # 3x4 se adivina a un metro aunque su sombreado fuera perfecto)
    area = tuya["ancho"] * tuya["alto"]
    areas = [p["ancho"] * p["alto"] for p in pack]
    if escala and (area < 0.6 * min(areas) or area > 1.6 * max(areas)):
        faltas.append(f"escala {tuya['ancho']}x{tuya['alto']} px contra {pack[0]['ancho']}x{pack[0]['alto']} del pack: no ocupa lo mismo")
    if faltas:
        veredicto = "POR DEBAJO"
    else:
        mejor = max(p["variedad"] for p in pack)
        mejor10 = max(p["variedad_p10"] for p in pack)
        plano = min(p["plano"] for p in pack)
        gana = tuya["variedad"] >= mejor and tuya["variedad_p10"] >= mejor10 and tuya["plano"] <= plano * 1.05
        veredicto = "SUPERA" if gana else "A LA ALTURA"
    return {"veredicto": veredicto, "faltas": faltas, "tuya": tuya, "pack": pack}


def lamina_a_ciegas(candidata, referencias: list, salida: Path, semilla: int | None = None, escala: int = 2) -> Path:
    """Pone la candidata mezclada entre las referencias, con letras y sin nombres, sobre pasto.

    La respuesta (qué letra es la dibujada) va a `<salida>.respuesta.json`: **mirá la lámina ANTES de abrirla** y
    anotá cuál te parece la dibujada. Si acertás a la primera y sin dudar, la pieza no reemplaza nada."""
    ims = [Image.fromarray(_rgba(candidata))] + [Image.fromarray(_rgba(r)) for r in referencias]
    orden = list(range(len(ims)))
    random.Random(semilla).shuffle(orden)
    alto = max(i.height for i in ims) * escala
    ancho = sum(i.width * escala + 12 for i in ims) + 12
    L = Image.new("RGBA", (ancho, alto + 28), (104, 140, 92, 255))
    d = ImageDraw.Draw(L)
    x = 12
    letras = "ABCDEFGHIJ"
    respuesta = None
    for pos, k in enumerate(orden):
        im = ims[k].resize((ims[k].width * escala, ims[k].height * escala), Image.NEAREST)
        L.alpha_composite(im, (x, 6 + alto - im.height))
        d.text((x + im.width // 2 - 3, alto + 12), letras[pos], fill=(255, 255, 255, 255))
        if k == 0:
            respuesta = letras[pos]
        x += im.width + 12
    salida = Path(salida)
    L.convert("RGB").save(salida)
    salida.with_suffix(".respuesta.json").write_text(json.dumps({"dibujada": respuesta}))
    return salida


def informe(r: dict, nombre: str = "pieza") -> str:
    filas = [f"== juez · {nombre}: {r['veredicto']}"]
    claves = list(SENTIDO)
    filas.append(f"  {'medida':13s} {'tuya':>8s}  " + "  ".join(f"{'pack' + str(i + 1):>8s}" for i in range(len(r["pack"]))))
    for k in claves:
        filas.append(f"  {k:13s} {r['tuya'][k]:8.2f}  " + "  ".join(f"{p[k]:8.2f}" for p in r["pack"]))
    for f in r["faltas"]:
        filas.append(f"  ✗ {f}")
    return "\n".join(filas)
