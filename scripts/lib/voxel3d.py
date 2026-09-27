#!/usr/bin/env python3
"""**DE UN SPRITE 3/4 A VÓXELES: el armario sale caja, no almohadón.**

Reemplaza al inflado de silueta (`volumen3d.alturas`) para los MODELOS. El
inflado redondeaba todo —un armario parecía una almohada— y dejaba costuras
entre frente y dorso. Esto lee el dibujo como lo que es: el pack usa proyección
oblicua de RPG, donde subir una fila en pantalla es subir un píxel de alto O
alejarse un píxel de fondo. Entonces cada columna del sprite es:

```text
    fila top ────┐  TAPA: se aleja hacia el fondo (z = D … 0), a altura H
                 │
    fila top+D ──┤  FRENTE: pared vertical en z = 0, de altura H … 0
                 │
    fila bot ────┘  (apoyo)
```

`D`, la profundidad de la tapa, se MIDE: en el pack la tapa está más iluminada
que el frente y la separa un canto oscuro, así que se busca el salto de luz más
fuerte en la mitad de arriba del objeto. Si no hay salto claro, 30 % del ancho.

Cada columna es un bloque SÓLIDO (vóxeles llenos, sin costuras: se acabaron los
huecos de la fogata), y las ranuras oscuras del dibujo —entre libros, juntas de
tablas— se HUNDEN un vóxel, así los libros quedan separados del mueble.

Mirado con la cámara del juego (oblicua: `y_pantalla = alto + fondo`) el modelo
es el PNG píxel por píxel, porque cada píxel visible es la cara de su vóxel.
"""

from __future__ import annotations

import json
import struct
from pathlib import Path

import numpy as np
from scipy import ndimage


def _luz(rgb):
    return 0.2126 * rgb[..., 0] + 0.7152 * rgb[..., 1] + 0.0722 * rgb[..., 2]


def profundidad_tapa(rgba: np.ndarray) -> int:
    solido = rgba[..., 3] == 255
    filas = np.nonzero(solido.any(1))[0]
    ancho = int(solido.any(0).sum())
    if len(filas) < 4:
        return 1
    top, bot = filas[0], filas[-1]
    alto = bot - top + 1
    lum = _luz(rgba[..., :3].astype(np.float32))
    media = np.array([lum[y][solido[y]].mean() if solido[y].any() else np.nan for y in range(rgba.shape[0])])
    # salto hacia abajo (tapa clara → canto/frente oscuro) en la mitad de arriba
    mejor, fila = 0.0, None
    for y in range(top + 2, top + max(3, alto // 2)):
        if np.isnan(media[y]) or np.isnan(media[y - 1]):
            continue
        salto = media[y - 1] - media[y]
        if salto > mejor:
            mejor, fila = salto, y
    if fila is not None and mejor > 10:
        d = fila - top
    else:
        # sin tapa visible: es un frente (cartel, reja, puerta) — grosor chico
        d = round(0.15 * ancho)
    return int(np.clip(d, 1, max(1, min(alto * 0.6, 48))))


def reconstruir(rgba: np.ndarray, D: int | None = None, hundir: bool = True):
    """Devuelve (colores, lleno): arrays [x, Y, Z] con el color RGB de cada vóxel
    y la máscara de ocupación. Y = alto (0 = suelo), Z = fondo (0 = frente)."""
    h, w = rgba.shape[:2]
    solido = rgba[..., 3] == 255
    if D is None:
        D = profundidad_tapa(rgba)
    lum = _luz(rgba[..., :3].astype(np.float32))
    peso = ndimage.gaussian_filter(solido.astype(np.float32), 1.5)
    suave = ndimage.gaussian_filter(lum * solido, 1.5) / np.maximum(peso, 1e-3)
    ranura = solido & ((suave - lum) > 28) if hundir else np.zeros_like(solido)

    Dmax = D + 2
    col = np.zeros((w, h + 1, Dmax + 1, 3), np.uint8)
    lleno = np.zeros((w, h + 1, Dmax + 1), bool)
    for x in range(w):
        ys = np.nonzero(solido[:, x])[0]
        if not len(ys):
            continue
        # tramos verticales: el de más arriba es el bloque; los de abajo (patas,
        # ruedas, lo que cuelga separado) son losas finas al frente
        cortes = np.nonzero(np.diff(ys) > 1)[0]
        tramos = np.split(ys, cortes + 1)
        bot = tramos[-1][-1]
        for n, tr in enumerate(tramos):
            top, fin = tr[0], tr[-1]
            largo = fin - top + 1
            if n == 0:
                dc = int(min(D, max(1, round(largo * 0.6)))) if largo > 2 else 1
                H = bot - (top + dc)
                for y in tr:
                    if y < top + dc:  # tapa
                        Z, Y = (top + dc) - y, H
                    else:              # frente
                        Z, Y = 0, bot - y
                    if ranura[y, x]:
                        if Z == 0:
                            Z = 1
                        else:
                            Y = max(0, Y - 1)
                    lleno[x, Y, Z] = True
                    col[x, Y, Z] = rgba[y, x, :3]
                # relleno sólido del bloque: cada vóxel interior toma el color de
                # su cara (frente a esa altura, o tapa a ese fondo)
                frente = {bot - y: rgba[y, x, :3] for y in tr if y >= top + dc}
                tapa = {(top + dc) - y: rgba[y, x, :3] for y in tr if y < top + dc}
                Yb = min(frente) if frente else 0
                for Y in range(Yb, H + 1):
                    for Z in range(0, dc + 1):
                        if lleno[x, Y, Z]:
                            continue
                        c = frente.get(Y) if Z <= 1 or Y < H else tapa.get(Z)
                        if c is None:
                            c = frente.get(Y, tapa.get(Z, rgba[tr[0], x, :3]))
                        lleno[x, Y, Z] = True
                        col[x, Y, Z] = c
            else:
                for y in tr:
                    Y = bot - y
                    for Z in (0, 1):
                        lleno[x, Y, Z] = True
                        col[x, Y, Z] = rgba[y, x, :3]
    return col, lleno


def reconstruir_organico(rgba: np.ndarray, piso=None, despegue: int = 0, R_fijo: int | None = None):
    """**Lo que no es caja**: fuego, copa, arbusto, roca, bicho. Va parado (el
    fuego sube, no se acuesta como una tapa) y con volumen REDONDO: el grosor en
    cada píxel sale de su distancia al borde de la silueta, con un radio
    proporcional al lado corto de cada pieza. Sólido y cerrado, sin costuras."""
    h, w = rgba.shape[:2]
    solido = rgba[..., 3] == 255
    d = ndimage.distance_transform_edt(solido)
    etiquetas, _ = ndimage.label(solido)
    radio = np.ones((h, w), np.float32)
    for i, sl in enumerate(ndimage.find_objects(etiquetas), start=1):
        if sl is None:
            continue
        lado = min(sl[0].stop - sl[0].start, sl[1].stop - sl[1].start)
        radio[sl][etiquetas[sl] == i] = float(np.clip(lado * 0.35, 1, 12))
    t = np.clip(d / radio, 0, 1)
    medio = np.where(solido, np.round(np.sqrt(1 - (1 - t) ** 2) * radio), 0).astype(int)
    # capas de personaje: cada una envuelve a la de abajo (`piso`) + `despegue`
    if piso is not None:
        medio = np.where(solido, np.maximum(medio, piso) + despegue, 0)
    R = R_fijo if R_fijo is not None else (int(medio.max()) if solido.any() else 0)
    col = np.zeros((w, h + 1, 2 * R + 2, 3), np.uint8)
    lleno = np.zeros((w, h + 1, 2 * R + 2), bool)
    ys, xs = np.nonzero(solido)
    bot = (h - 1) if piso is not None else (ys.max() if len(ys) else 0)
    for y, x in zip(ys, xs):
        m = medio[y, x]
        Y = bot - y
        lleno[x, Y, R - m:R + m + 1] = True
        col[x, Y, R - m:R + m + 1] = rgba[y, x, :3]
    return col, lleno, medio


# Lo que se modela como volumen redondo y no como caja.
ORGANICO_CATEGORIAS = {"nature", "particles", "npc", "agua", "decor", "items"}
ORGANICO_PALABRAS = ("arbol", "arbusto", "planta", "flor", "seto", "pasto", "hongo", "roca",
                     "piedra", "fuego", "campfire", "fogata", "llama", "humo", "copa", "tree",
                     "bush", "plant", "flower", "rock", "fire", "maceta", "cactus")


def es_organico(clave: str) -> bool:
    cat = clave.split("/")[0]
    return cat in ORGANICO_CATEGORIAS or any(p in clave.lower() for p in ORGANICO_PALABRAS)


# ---------------------------------------------------------------------------
# Superficie → .glb (color por vértice, caras fusionadas en tiras)
# ---------------------------------------------------------------------------

# (eje normal, signo, ejes de la cara (u, v))
_CARAS = [(0, -1), (0, 1), (1, -1), (1, 1), (2, -1), (2, 1)]


def superficie(col, lleno, escala=1 / 32):
    """Caras expuestas; tiras del mismo color fusionadas a lo largo de un eje.
    Coordenadas del .glb: x a la derecha, y arriba, z HACIA LA CÁMARA (el frente
    del objeto mira a +z, como en glTF)."""
    W, H, D = lleno.shape
    P, C, I = [], [], []
    pad = np.pad(lleno, 1)
    for eje, s in _CARAS:
        vec = np.roll(pad, -s, axis=eje)[1:-1, 1:-1, 1:-1]
        expuesta = lleno & ~vec
        # eje de fusión: x para caras ±Y/±Z, y para caras ±X
        fus = 1 if eje == 0 else 0
        idx = np.argwhere(expuesta)
        if not len(idx):
            continue
        # ordenar por (los otros dos ejes, eje de fusión)
        otros = [a for a in range(3) if a != fus]
        orden = np.lexsort((idx[:, fus], idx[:, otros[1]], idx[:, otros[0]]))
        idx = idx[orden]
        i = 0
        n = len(idx)
        while i < n:
            a = idx[i]
            c = col[tuple(a)]
            j = i
            while (j + 1 < n and all(idx[j + 1][o] == a[o] for o in otros)
                   and idx[j + 1][fus] == idx[j][fus] + 1 and (col[tuple(idx[j + 1])] == c).all()):
                j += 1
            lo = a.astype(float)
            hi = a.astype(float) + 1
            hi[fus] = idx[j][fus] + 1
            # la cara: plano en eje = lo o hi según signo
            k = hi[eje] if s > 0 else lo[eje]
            u, v = [x for x in range(3) if x != eje]
            esquinas = []
            for du, dv in ((0, 0), (1, 0), (1, 1), (0, 1)):
                p = [0.0, 0.0, 0.0]
                p[eje] = k
                p[u] = hi[u] if du else lo[u]
                p[v] = hi[v] if dv else lo[v]
                esquinas.append(p)
            base = len(P)
            for p in esquinas:
                # vóxel (x, Y, Z) → glb (x, y, −z): el frente (Z = 0) mira a la cámara
                P.append((p[0] * escala, p[1] * escala, -p[2] * escala))
                C.append(c)
            # orientación del triángulo hacia afuera
            n_out = np.zeros(3)
            n_out[eje] = s
            e0 = np.subtract(esquinas[1], esquinas[0])
            e1 = np.subtract(esquinas[2], esquinas[0])
            if np.dot(np.cross(e0, e1), n_out) > 0:
                I += [base, base + 1, base + 2, base, base + 2, base + 3]
            else:
                I += [base, base + 2, base + 1, base, base + 3, base + 2]
            i = j + 1
    return np.array(P, np.float32), np.array(C, np.uint8), np.array(I, np.uint32)


def exportar_glb(ruta: Path, col, lleno) -> bool:
    P, C, I = superficie(col, lleno)
    if not len(I):
        return False
    # z invertida al exportar → el winding también
    I = I.reshape(-1, 3)[:, ::-1].reshape(-1)
    Cn = np.hstack([C, np.full((len(C), 1), 255, np.uint8)])

    def pad4(b, r=b"\x00"):
        return b + r * ((4 - len(b) % 4) % 4)

    bin_, vistas = b"", []
    for blob, tgt in ((P.tobytes(), 34962), (Cn.tobytes(), 34962), (I.tobytes(), 34963)):
        vistas.append({"buffer": 0, "byteOffset": len(bin_), "byteLength": len(blob), "target": tgt})
        bin_ = pad4(bin_ + blob)
    g = {"asset": {"version": "2.0", "generator": "phaser-arena voxel3d"}, "scene": 0,
         "scenes": [{"nodes": [0]}], "nodes": [{"mesh": 0}],
         "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "COLOR_0": 1}, "indices": 2,
                                     "material": 0}]}],
         "materials": [{"pbrMetallicRoughness": {"baseColorFactor": [1, 1, 1, 1], "metallicFactor": 0,
                                                 "roughnessFactor": 1},
                        "extensions": {"KHR_materials_unlit": {}}}],
         "extensionsUsed": ["KHR_materials_unlit"],
         "accessors": [
             {"bufferView": 0, "componentType": 5126, "count": len(P), "type": "VEC3",
              "min": P.min(0).tolist(), "max": P.max(0).tolist()},
             {"bufferView": 1, "componentType": 5121, "normalized": True, "count": len(Cn), "type": "VEC4"},
             {"bufferView": 2, "componentType": 5125, "count": len(I), "type": "SCALAR"}],
         "bufferViews": vistas, "buffers": [{"byteLength": len(bin_)}]}
    js = pad4(json.dumps(g, separators=(",", ":")).encode(), b" ")
    ruta.parent.mkdir(parents=True, exist_ok=True)
    with open(ruta, "wb") as f:
        f.write(struct.pack("<III", 0x46546C67, 2, 12 + 8 + len(js) + 8 + len(bin_)))
        f.write(struct.pack("<II", len(js), 0x4E4F534A) + js)
        f.write(struct.pack("<II", len(bin_), 0x004E4942) + bin_)
    return True
