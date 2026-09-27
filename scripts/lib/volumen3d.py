#!/usr/bin/env python3
"""**RELIEVE 3D DE UN SPRITE, SIN PERDER SU DIBUJO.**

Es la mitad "3D" de `scripts/arte3d.py`. Recibe la página RGBA de un pack y la
máscara de lo que es volumen (no suelo), y devuelve:

- `alturas`  — un campo de altura por píxel (float, en píxeles). Es el modelo:
  lo que `exportar_glb` vuelve malla.
- `normales` — el mapa de normales que sale de esas alturas (RGB clásico,
  `0.5 + 0.5·n`), para quien algún día ilumine por píxel.
- `hornear`  — el sprite RE-ILUMINADO desde el modelo, con la luz del pack.

## Cómo se reconstruye (el método de SpriteIlluminator / Laigter)

Un sprite 3/4 no trae profundidad, así que no se inventa una: se infiere.

1. **Bisel de silueta.** La distancia al borde del alfa, topada en `BISEL` px y
   pasada por un cuarto de seno: el canto se redondea, el cuerpo queda plano.
   Una fachada de 200 px no se infla como un globo — es un bloque con cantos.
2. **Relieve por luminancia.** El paso alto de la luminancia hunde las líneas
   oscuras (juntas de tablas, ranuras) y levanta los brillos. Va al MODELO y
   al mapa de normales, pero NO al horneado: horneado, ese detalle sale como
   motas de un píxel, que es dithering y el pack tiene 0,0 %.

## Por qué el horneado es tan suave (y no un "efecto 3D")

El pack YA viene sombreado. Las reglas medidas (skill `pixel-art`) dicen lo que
se le puede agregar sin que deje de ser LimeZu:

- luz de arriba-izquierda **+5 / +4** de luminancia — no más;
- **un escalón de rampa (12)** más oscuro abajo y a la derecha, el lado en
  sombra — es lo único que el pack oscurece en el borde;
- **cero contorno, cero anti-alias, cero dithering**.

Así que `hornear` sólo toca los píxeles donde el modelo pide al menos
`UMBRAL` de luminancia de diferencia, y los lleva al color existente más
cercano (los del propio sprite primero, después la paleta del pack). El resto
queda BIT A BIT igual. El color y el diseño no se pierden porque no se tocan.
"""

from __future__ import annotations

import json
import struct
from pathlib import Path

import numpy as np
from scipy import ndimage
from scipy.spatial import cKDTree

from lib.pixel import PALETA

# Canto redondeado, en píxeles de sprite. 3 es el grosor de un canto en el pack
# (medido a ojo en las tapas de muebles/props — un canto de 1 es un filo, de 6
# ya es un almohadón).
BISEL = 3.0
# Radio de la forma: fracción del lado corto de cada pieza, con techo.
FORMA = 0.3
RADIO_MAX = 10.0
# Cuánto hunde/levanta el detalle por luminancia (px de altura por 255 de luz).
DETALLE = 1.2
# Luz del pack: arriba-izquierda y alta (imagen: x→derecha, y→abajo, z→ojo).
LUZ = np.array([-0.55, -0.62, 1.0])
LUZ = LUZ / np.linalg.norm(LUZ)
# Ganancias en luminancia: el lado a la luz sube hasta +5, el de sombra baja
# hasta un escalón de rampa (-12). Son las dos cifras medidas del pack.
SUBE_MAX = 5.0
BAJA_MAX = 12.0
# Por debajo de esta diferencia el píxel queda INTACTO.
UMBRAL = 4.0


def _luz(rgb: np.ndarray) -> np.ndarray:
    return 0.2126 * rgb[..., 0] + 0.7152 * rgb[..., 1] + 0.0722 * rgb[..., 2]


def alturas(rgba: np.ndarray, volumen: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """Campo de altura (px) y altura del bisel solo, sobre los píxeles sólidos
    de `volumen`. La sombra de contacto (alfa 100) no es sólido: es suelo."""
    solido = (rgba[..., 3] == 255) & volumen
    d = ndimage.distance_transform_edt(solido)
    # **La forma, no sólo el canto.** Cada pieza conexa se redondea con un radio
    # proporcional a su lado corto (`FORMA` de él, entre `BISEL` y `RADIO_MAX`):
    # un tanque de agua sale cilindro, una silla sale con cantos, y una fachada
    # de 200 px sigue siendo un bloque plano con los bordes vueltos.
    etiquetas, n = ndimage.label(solido)
    radio = np.full(solido.shape, BISEL, np.float32)
    for i, sl in enumerate(ndimage.find_objects(etiquetas), start=1):
        if sl is None:
            continue
        lado = min(sl[0].stop - sl[0].start, sl[1].stop - sl[1].start)
        r = float(np.clip(lado * FORMA, BISEL, RADIO_MAX))
        radio[sl][etiquetas[sl] == i] = r
    t = np.clip(d / radio, 0.0, 1.0)
    bisel = np.sin(t * np.pi / 2) * radio
    lum = _luz(rgba[..., :3].astype(np.float32))
    # el paso alto se mide sólo dentro del sólido, para que el borde contra
    # el aire no cuente como "ranura"
    peso = ndimage.gaussian_filter(solido.astype(np.float32), 1.2)
    suave = ndimage.gaussian_filter(lum * solido, 1.2) / np.maximum(peso, 1e-3)
    detalle = (lum - suave) / 255.0 * DETALLE * BISEL
    h = np.where(solido, bisel + detalle, 0.0)
    return h.astype(np.float32), np.where(solido, bisel, 0.0).astype(np.float32)


def normales(h: np.ndarray) -> np.ndarray:
    """Normales por diferencias centrales (imagen: y hacia abajo)."""
    gx = ndimage.sobel(h, axis=1) / 8.0
    gy = ndimage.sobel(h, axis=0) / 8.0
    n = np.stack([-gx, -gy, np.ones_like(h)], axis=-1)
    return n / np.linalg.norm(n, axis=-1, keepdims=True)


def mapa_de_normales(n: np.ndarray, solido: np.ndarray) -> np.ndarray:
    rgb = np.clip((n * 0.5 + 0.5) * 255.0 + 0.5, 0, 255).astype(np.uint8)
    a = np.where(solido, 255, 0).astype(np.uint8)
    return np.dstack([rgb, a])


def hornear(rgba: np.ndarray, volumen: np.ndarray, bisel: np.ndarray) -> tuple[np.ndarray, int]:
    """El sprite re-iluminado desde el bisel del modelo. Devuelve (rgba, n
    píxeles tocados)."""
    solido = (rgba[..., 3] == 255) & volumen
    n = normales(bisel)
    plano = LUZ[2]
    rel = n @ LUZ - plano  # >0 mira a la luz, <0 le da la espalda
    delta = np.where(rel > 0, rel / (1 - plano) * SUBE_MAX * 2.0, rel / plano * BAJA_MAX * 2.0)
    delta = np.clip(delta, -BAJA_MAX, SUBE_MAX)
    # **En escalones, no en degradé**: el pack cambia de tono de golpe. Tres
    # bandas: sube, medio escalón abajo, un escalón abajo.
    delta = np.select([delta >= UMBRAL, delta <= -BAJA_MAX * 0.75, delta <= -UMBRAL * 1.5],
                      [SUBE_MAX, -BAJA_MAX, -BAJA_MAX / 2], 0.0)
    tocar = solido & (np.abs(delta) >= UMBRAL)
    if not tocar.any():
        return rgba, 0

    fuera = rgba.copy()
    rgb = rgba[..., :3].astype(np.float32)
    lum = np.maximum(_luz(rgb), 24.0)
    objetivo = np.clip(rgb * ((lum + delta) / lum)[..., None], 0, 255)

    # Candidatos: los colores que el sprite YA usa (su propia rampa) más la
    # paleta del pack. Un píxel nunca sale de ese conjunto.
    propios = np.unique(rgba[solido][:, :3], axis=0)
    cand = np.unique(np.vstack([propios, np.array(PALETA, dtype=np.uint8)]), axis=0)
    w = np.array([np.sqrt(2.0), 2.0, np.sqrt(3.0)])  # el mismo peso que pixel._dist
    arbol = cKDTree(cand.astype(np.float32) * w)
    ys, xs = np.nonzero(tocar)
    _, idx = arbol.query(objetivo[ys, xs] * w)
    elegido = cand[idx]
    # Si lo más cercano ES el color original, no hubo cambio real.
    fuera[ys, xs, :3] = elegido
    cambiados = int((elegido != rgba[ys, xs, :3]).any(axis=1).sum())
    return fuera, cambiados


# ---------------------------------------------------------------------------
# Exportar a glTF binario (.glb) — sin dependencias
# ---------------------------------------------------------------------------


def exportar_glb(ruta: Path, h: np.ndarray, solido: np.ndarray, rgba_png: bytes,
                 paso: int = 1, escala: float = 1 / 32):
    """**Un sólido CERRADO que de frente es el PNG, píxel por píxel.**

    Cara de adelante en z = +h, dorso espejado en z = −h, y los dos se cierran
    en z = 0 sobre la silueta: se lo puede girar y tiene volumen. Los vértices
    caen en las esquinas de los píxeles y la textura (el PNG ORIGINAL,
    embebido) se muestrea NEAREST, así que mirado de frente en ortográfica es
    EXACTAMENTE el sprite: la altura sólo mueve z, nunca x ni y.
    1 unidad = 1 celda de 32 px."""
    alto, ancho = h.shape
    gy = list(range(0, alto + 1, paso))
    gx = list(range(0, ancho + 1, paso))
    if gy[-1] != alto:
        gy.append(alto)
    if gx[-1] != ancho:
        gx.append(ancho)
    sp = np.pad(solido, 1)
    hp = np.pad(np.where(solido, h, 0.0), 1)
    pos, uv, ind = [], [], []
    vid = {}

    def v(ix, iy, lado):
        x, y = gx[ix], gy[iy]
        vecinos = sp[y:y + 2, x:x + 2]
        z = 0.0 if not vecinos.all() else float(hp[y:y + 2, x:x + 2].mean())
        if z == 0.0:
            lado = 0  # la costura de la silueta la comparten frente y dorso
        k = (ix, iy, lado)
        if k not in vid:
            vid[k] = len(pos)
            pos.append((x * escala, -y * escala, (z if lado >= 0 else -z) * escala))
            uv.append((x / ancho, y / alto))
        return vid[k]

    for iy in range(len(gy) - 1):
        for ix in range(len(gx) - 1):
            if not solido[gy[iy]:gy[iy + 1], gx[ix]:gx[ix + 1]].any():
                continue
            for lado in (1, -1):
                a, b = v(ix, iy, lado), v(ix + 1, iy, lado)
                c, d = v(ix + 1, iy + 1, lado), v(ix, iy + 1, lado)
                ind += [a, d, b, b, d, c] if lado > 0 else [a, b, d, b, c, d]
    if not ind:
        return False

    P = np.array(pos, dtype=np.float32)
    U = np.array(uv, dtype=np.float32)
    I = np.array(ind, dtype=np.uint32)

    def pad4(b: bytes, relleno=b"\x00"):
        return b + relleno * ((4 - len(b) % 4) % 4)

    cuerpos = [P.tobytes(), U.tobytes(), I.tobytes(), rgba_png]
    vistas, bin_ = [], b""
    for c in cuerpos:
        vistas.append({"buffer": 0, "byteOffset": len(bin_), "byteLength": len(c)})
        bin_ = pad4(bin_ + c)
    vistas[0]["target"] = 34962
    vistas[1]["target"] = 34962
    vistas[2]["target"] = 34963
    gltf = {
        "asset": {"version": "2.0", "generator": "phaser-arena arte3d"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0}],
        "meshes": [{"primitives": [{
            "attributes": {"POSITION": 0, "TEXCOORD_0": 1},
            "indices": 2, "material": 0}]}],
        "materials": [{
            "pbrMetallicRoughness": {"baseColorTexture": {"index": 0},
                                     "metallicFactor": 0.0, "roughnessFactor": 1.0},
            "alphaMode": "MASK", "alphaCutoff": 0.5, "doubleSided": True}],
        "textures": [{"source": 0, "sampler": 0}],
        # filtro 9728 = NEAREST: es pixel art, no se suaviza
        "samplers": [{"magFilter": 9728, "minFilter": 9728}],
        "images": [{"bufferView": 3, "mimeType": "image/png"}],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": len(P), "type": "VEC3",
             "min": P.min(0).tolist(), "max": P.max(0).tolist()},
            {"bufferView": 1, "componentType": 5126, "count": len(U), "type": "VEC2"},
            {"bufferView": 2, "componentType": 5125, "count": len(I), "type": "SCALAR"},
        ],
        "bufferViews": vistas,
        "buffers": [{"byteLength": len(bin_)}],
    }
    js = pad4(json.dumps(gltf, separators=(",", ":")).encode(), b" ")
    total = 12 + 8 + len(js) + 8 + len(bin_)
    ruta.parent.mkdir(parents=True, exist_ok=True)
    with open(ruta, "wb") as f:
        f.write(struct.pack("<III", 0x46546C67, 2, total))
        f.write(struct.pack("<II", len(js), 0x4E4F534A) + js)
        f.write(struct.pack("<II", len(bin_), 0x004E4942) + bin_)
    return True
