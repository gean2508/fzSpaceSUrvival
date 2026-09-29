#!/usr/bin/env python3
"""**LA FAUNA DE LA TIERRA, MODELADA EN 3D Y RENDERIZADA A PIXEL ART.**

Reemplaza los animales de `dibujar-mundo.py` (un poroto de color con cuatro
palitos: *"arte basura"*, dijo el usuario, con razón). Acá cada animal es un
MODELO: elipsoides con material, articulados (patas que giran en la cadera,
cabeza en el cuello). El pixel art NO se dibuja: se renderiza desde el modelo
con la cámara 3/4 del juego y la luz del pack, y cada tono sale de la rampa del
material en la paleta medida (`lib/pixel.py`). Por eso el pixel art y el modelo son la
misma cosa: el PNG es el render del modelo (el modelo no se exporta: los
.glb se descartaron el 27 sep 2026, ver biblia §14).

Y porque es un modelo, las poses salen solas: tres direcciones (la izquierda es
el espejo de la derecha, lo hace `fauna.rs::apply_pose`) por `idle`, `walk`,
`run`, `eat` y, el perro, `bark`.

```sh
python3 scripts/fauna3d.py          # hojas art/gen/fauna/*.png + manifest
python3 scripts/fauna3d.py --ver    # lámina ampliada en el scratchpad, sin publicar
```

La escala es la del pack: una celda de 32 px son 0,95 m (biblia §8), así que el
sprite se dibuja a 33,7 px/m y `SpeciesDef.size` tiene que ser el cuadro en
metros — el script lo imprime para pegarlo en `fauna.rs::earth_species`.
"""

from __future__ import annotations

import json
import math
import sys
from dataclasses import dataclass, field
from pathlib import Path

import numpy as np
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib.pixel import SOMBRA, SOMBRA_ALFA, Lienzo, rampa  # noqa: E402

RAIZ = Path(__file__).resolve().parent.parent
#: Donde van las láminas de control (`--ver`). `FAUNA_SCRATCH` lo cambia.
SCRATCH = Path(__import__("os").environ.get("FAUNA_SCRATCH", "/tmp/fauna3d"))
SCRATCH.mkdir(parents=True, exist_ok=True)
PX_M = 32 / 0.95
PITCH = math.radians(38)
LUZ = np.array([-0.55, 0.75, 0.45])
LUZ /= np.linalg.norm(LUZ)
# Umbrales de banda sobre n·L: claro / medio / sombra / oscuro. El medio manda
# (es el tono que el pack usa en la mayor parte del objeto).
BANDAS = (0.80, 0.30, -0.05)


def rot_z(a):
    c, s = math.cos(a), math.sin(a)
    return np.array([[c, -s, 0], [s, c, 0], [0, 0, 1]])


def rot_y(a):
    c, s = math.cos(a), math.sin(a)
    return np.array([[c, 0, s], [0, 1, 0], [-s, 0, c]])


@dataclass
class Prim:
    c: tuple
    r: tuple
    mat: str
    grupo: str = "cuerpo"
    rot: np.ndarray = field(default_factory=lambda: np.eye(3))
    #: Segmento de la pata DEBAJO de la rodilla (caña + pezuña): gira además
    #: alrededor de este punto (la rodilla, en reposo), no sólo de la cadera.
    rodilla: tuple | None = None


@dataclass
class Animal:
    clave: str
    nombre: str
    mats: dict            # material → color base
    prims: list
    caderas: dict         # grupo de pata → (punto de cadera, fase)
    cuello: tuple         # pivote de la cabeza
    manchas: list = field(default_factory=list)  # (centro local, radio) sobre el material "piel"
    ladra: bool = False
    zancada: float = 0.45
    #: cuánto se dobla la rodilla en el paso (rad) — la pata que avanza se recoge.
    #: Sólo cuenta en las patas armadas con `pata()` (aves y conejo: patas
    #: demasiado cortas para que se lea una rodilla, quedan rígidas).
    flexion: float = 0.95


# — los modelos ————————————————————————————————————————————————————————

def pata(x, z, largo, grueso, mat, pezuna, g):
    """**Una pata ARTICULADA**: muslo (cadera→rodilla) y caña (rodilla→pezuña).

    Antes era un solo elipsoide que giraba en la cadera: dos palos que se
    balanceaban (*"que las patas no sean como dos palos moviéndose, que
    flexionen"*, pidió el usuario). Ahora la rodilla está al 45 % del largo y la
    caña gira sobre ella en el paso (`posar`). La caña es un pelo más fina que el
    muslo, como en el animal de verdad, y los dos segmentos se solapan en la
    rodilla para que al doblar no se abra un hueco."""
    base = grueso * 0.6
    y_rod = base + largo * 0.45
    y_top = base + largo
    sup = Prim((x, (y_rod + y_top) / 2, z), (grueso * 1.05, (y_top - y_rod) / 2 + grueso * 0.35, grueso * 1.05), mat, g)
    rod = (x, y_rod, z)
    inf = Prim((x, (base + y_rod) / 2, z), (grueso * 0.85, (y_rod - base) / 2 + grueso * 0.3, grueso * 0.85), mat, g,
               rodilla=rod)
    pez = Prim((x, base, z), (grueso * 1.08, grueso * 0.7, grueso * 1.08), pezuna, g, rodilla=rod)
    return [sup, inf, pez]


def vaca():
    p = [Prim((0, 0.80, 0), (0.60, 0.30, 0.27), "piel"),
         Prim((0.34, 0.86, 0), (0.30, 0.27, 0.25), "piel"),
         Prim((-0.14, 0.52, 0), (0.12, 0.07, 0.11), "rosa"),
         Prim((-0.62, 0.78, 0), (0.03, 0.22, 0.03), "piel", "cola"),
         Prim((-0.63, 0.55, 0), (0.05, 0.07, 0.05), "oscuro", "cola"),
         Prim((0.70, 1.02, 0), (0.19, 0.16, 0.15), "piel", "cabeza"),
         Prim((0.86, 0.94, 0), (0.09, 0.09, 0.12), "rosa", "cabeza"),
         Prim((0.82, 1.08, 0.12), (0.03, 0.035, 0.03), "ojo", "cabeza"),
         Prim((0.82, 1.08, -0.12), (0.03, 0.035, 0.03), "ojo", "cabeza"),
         Prim((0.64, 1.10, 0.19), (0.05, 0.035, 0.09), "piel", "cabeza"),
         Prim((0.64, 1.10, -0.19), (0.05, 0.035, 0.09), "piel", "cabeza"),
         Prim((0.70, 1.19, 0.09), (0.03, 0.06, 0.03), "cuerno", "cabeza"),
         Prim((0.70, 1.19, -0.09), (0.03, 0.06, 0.03), "cuerno", "cabeza")]
    cad = {}
    for i, (x, z) in enumerate([(0.42, 0.15), (0.42, -0.15), (-0.42, 0.15), (-0.42, -0.15)]):
        g = f"pata{i}"
        p += pata(x, z, 0.52, 0.075, "piel", "oscuro", g)
        cad[g] = ((x, 0.62, z), 0 if i in (0, 3) else math.pi)
    return Animal("npc/fauna_cow", "Vaca",
                  {"piel": (236, 232, 222), "mancha": (66, 60, 70), "rosa": (226, 164, 164),
                   "oscuro": (80, 66, 62), "ojo": (46, 40, 52), "cuerno": (232, 222, 190)},
                  p, cad, (0.55, 0.95, 0),
                  manchas=[((0.15, 0.95, 0.2), 0.17), ((-0.3, 0.8, 0.22), 0.14), ((0.3, 0.7, -0.2), 0.16),
                           ((-0.25, 0.98, -0.1), 0.15), ((0.72, 1.08, 0.08), 0.07)])


def gallina():
    p = [Prim((0, 0.30, 0), (0.17, 0.14, 0.13), "piel"),
         Prim((-0.16, 0.40, 0), (0.07, 0.10, 0.06), "piel"),
         Prim((-0.01, 0.32, 0.12), (0.11, 0.07, 0.025), "ala"),
         Prim((-0.01, 0.32, -0.12), (0.11, 0.07, 0.025), "ala"),
         Prim((0.13, 0.48, 0), (0.07, 0.08, 0.065), "piel", "cabeza"),
         Prim((0.13, 0.58, 0), (0.05, 0.035, 0.02), "cresta", "cabeza"),
         Prim((0.21, 0.47, 0), (0.045, 0.02, 0.02), "pico", "cabeza"),
         Prim((0.18, 0.41, 0), (0.018, 0.03, 0.016), "cresta", "cabeza"),
         Prim((0.17, 0.51, 0.05), (0.018, 0.02, 0.015), "ojo", "cabeza"),
         Prim((0.17, 0.51, -0.05), (0.018, 0.02, 0.015), "ojo", "cabeza")]
    cad = {}
    for i, z in enumerate([0.05, -0.05]):
        g = f"pata{i}"
        p += [Prim((0.0, 0.09, z), (0.016, 0.09, 0.016), "pico", g),
              Prim((0.03, 0.015, z), (0.045, 0.015, 0.03), "pico", g)]
        cad[g] = ((0.0, 0.18, z), 0 if i == 0 else math.pi)
    return Animal("npc/fauna_chicken", "Gallina",
                  {"piel": (242, 238, 228), "ala": (220, 214, 204), "cresta": (204, 64, 60),
                   "pico": (230, 172, 64), "ojo": (46, 40, 52)},
                  p, cad, (0.10, 0.40, 0), zancada=0.6)


def oveja():
    p = [Prim((0, 0.62, 0), (0.45, 0.28, 0.27), "lana")]
    # los rulos: bultos de lana que rompen la silueta del poroto
    for x, y, z in [(0.3, 0.78, 0.14), (0.3, 0.78, -0.14), (-0.3, 0.8, 0.12), (-0.3, 0.8, -0.12),
                    (0.0, 0.88, 0.0), (0.0, 0.66, 0.24), (0.0, 0.66, -0.24), (-0.42, 0.62, 0),
                    (0.15, 0.86, 0.1), (-0.15, 0.88, -0.1)]:
        p.append(Prim((x, y, z), (0.17, 0.14, 0.14), "lana"))
    p += [Prim((0.50, 0.76, 0), (0.15, 0.13, 0.11), "cara", "cabeza"),
          Prim((0.44, 0.88, 0), (0.12, 0.07, 0.11), "lana", "cabeza"),
          Prim((0.46, 0.80, 0.14), (0.05, 0.025, 0.07), "cara", "cabeza"),
          Prim((0.46, 0.80, -0.14), (0.05, 0.025, 0.07), "cara", "cabeza"),
          Prim((0.60, 0.81, 0.07), (0.025, 0.028, 0.02), "ojo", "cabeza"),
          Prim((0.60, 0.81, -0.07), (0.025, 0.028, 0.02), "ojo", "cabeza")]
    cad = {}
    for i, (x, z) in enumerate([(0.28, 0.13), (0.28, -0.13), (-0.28, 0.13), (-0.28, -0.13)]):
        g = f"pata{i}"
        p += pata(x, z, 0.38, 0.055, "cara", "cara", g)
        cad[g] = ((x, 0.44, z), 0 if i in (0, 3) else math.pi)
    return Animal("npc/fauna_sheep", "Oveja",
                  {"lana": (236, 232, 218), "cara": (78, 68, 72), "ojo": (40, 34, 44)},
                  p, cad, (0.40, 0.72, 0))


def cerdo():
    p = [Prim((0, 0.50, 0), (0.50, 0.26, 0.25), "piel"),
         Prim((-0.49, 0.58, 0), (0.035, 0.035, 0.035), "piel", "cola"),
         Prim((-0.53, 0.63, 0), (0.03, 0.03, 0.03), "piel", "cola"),
         Prim((0.50, 0.56, 0), (0.17, 0.16, 0.16), "piel", "cabeza"),
         Prim((0.66, 0.52, 0), (0.05, 0.075, 0.085), "hocico", "cabeza"),
         Prim((0.70, 0.53, 0.03), (0.02, 0.02, 0.02), "ojo", "cabeza"),
         Prim((0.70, 0.53, -0.03), (0.02, 0.02, 0.02), "ojo", "cabeza"),
         Prim((0.62, 0.62, 0.09), (0.022, 0.026, 0.02), "ojo", "cabeza"),
         Prim((0.62, 0.62, -0.09), (0.022, 0.026, 0.02), "ojo", "cabeza"),
         Prim((0.52, 0.73, 0.1), (0.06, 0.06, 0.03), "oreja", "cabeza"),
         Prim((0.52, 0.73, -0.1), (0.06, 0.06, 0.03), "oreja", "cabeza")]
    cad = {}
    for i, (x, z) in enumerate([(0.3, 0.14), (0.3, -0.14), (-0.3, 0.14), (-0.3, -0.14)]):
        g = f"pata{i}"
        p += pata(x, z, 0.22, 0.065, "piel", "hocico", g)
        cad[g] = ((x, 0.30, z), 0 if i in (0, 3) else math.pi)
    return Animal("npc/fauna_pig", "Cerdo",
                  {"piel": (234, 170, 168), "hocico": (212, 128, 138), "oreja": (218, 146, 150),
                   "ojo": (46, 40, 52)},
                  p, cad, (0.40, 0.55, 0))


def perro():
    p = [Prim((0, 0.46, 0), (0.34, 0.16, 0.15), "piel"),
         Prim((0.24, 0.47, 0), (0.12, 0.14, 0.13), "claro"),
         Prim((-0.36, 0.56, 0), (0.035, 0.13, 0.035), "piel", "cola"),
         Prim((0.40, 0.68, 0), (0.14, 0.13, 0.12), "piel", "cabeza"),
         Prim((0.54, 0.63, 0), (0.09, 0.065, 0.065), "claro", "cabeza"),
         Prim((0.62, 0.65, 0), (0.025, 0.025, 0.03), "ojo", "cabeza"),
         Prim((0.49, 0.73, 0.07), (0.022, 0.025, 0.02), "ojo", "cabeza"),
         Prim((0.49, 0.73, -0.07), (0.022, 0.025, 0.02), "ojo", "cabeza"),
         Prim((0.37, 0.70, 0.13), (0.045, 0.11, 0.03), "oreja", "cabeza"),
         Prim((0.37, 0.70, -0.13), (0.045, 0.11, 0.03), "oreja", "cabeza")]
    p[2].rot = rot_z(-0.6)
    cad = {}
    for i, (x, z) in enumerate([(0.22, 0.09), (0.22, -0.09), (-0.22, 0.09), (-0.22, -0.09)]):
        g = f"pata{i}"
        p += pata(x, z, 0.30, 0.05, "piel", "claro", g)
        cad[g] = ((x, 0.36, z), 0 if i in (0, 3) else math.pi)
    return Animal("npc/fauna_dog", "Perro",
                  {"piel": (178, 122, 72), "claro": (226, 196, 156), "oreja": (116, 76, 50),
                   "ojo": (44, 36, 44)},
                  p, cad, (0.30, 0.60, 0), ladra=True)


def escalar(a: Animal, f: float) -> Animal:
    """El mismo modelo a otro tamaño (el pixel art de un bicho muy chico se
    vuelve ruido: el sombreado arma damero)."""
    for p in a.prims:
        p.c = tuple(v * f for v in p.c)
        p.r = tuple(v * f for v in p.r)
        if p.rodilla is not None:
            p.rodilla = tuple(v * f for v in p.rodilla)
    a.caderas = {g: (tuple(v * f for v in c), ph) for g, (c, ph) in a.caderas.items()}
    a.cuello = tuple(v * f for v in a.cuello)
    a.manchas = [(tuple(v * f for v in c), r * f) for c, r in a.manchas]
    return a


def cabra():
    p = [Prim((0, 0.62, 0), (0.40, 0.20, 0.18), "piel"),
         Prim((0.26, 0.64, 0), (0.15, 0.18, 0.16), "piel"),
         Prim((-0.40, 0.76, 0), (0.04, 0.08, 0.035), "piel", "cola"),
         Prim((0.44, 0.92, 0), (0.12, 0.12, 0.10), "piel", "cabeza"),
         Prim((0.57, 0.86, 0), (0.08, 0.07, 0.07), "piel", "cabeza"),
         Prim((0.56, 0.74, 0), (0.025, 0.07, 0.025), "barba", "cabeza"),
         Prim((0.63, 0.88, 0), (0.02, 0.02, 0.03), "ojo", "cabeza"),
         Prim((0.52, 0.97, 0.08), (0.022, 0.025, 0.02), "ojo", "cabeza"),
         Prim((0.52, 0.97, -0.08), (0.022, 0.025, 0.02), "ojo", "cabeza"),
         Prim((0.40, 1.07, 0.06), (0.03, 0.09, 0.03), "cuerno", "cabeza"),
         Prim((0.40, 1.07, -0.06), (0.03, 0.09, 0.03), "cuerno", "cabeza"),
         Prim((0.40, 0.96, 0.13), (0.05, 0.025, 0.07), "piel", "cabeza"),
         Prim((0.40, 0.96, -0.13), (0.05, 0.025, 0.07), "piel", "cabeza")]
    p[9].rot = rot_z(0.5)
    p[10].rot = rot_z(0.5)
    cad = {}
    for i, (x, z) in enumerate([(0.26, 0.1), (0.26, -0.1), (-0.26, 0.1), (-0.26, -0.1)]):
        g = f"pata{i}"
        p += pata(x, z, 0.42, 0.045, "piel", "oscuro", g)
        cad[g] = ((x, 0.48, z), 0 if i in (0, 3) else math.pi)
    return Animal("npc/fauna_goat", "Cabra",
                  {"piel": (232, 226, 214), "barba": (206, 198, 186), "oscuro": (92, 80, 76),
                   "ojo": (46, 40, 52), "cuerno": (150, 132, 112)},
                  p, cad, (0.30, 0.80, 0))


def burro():
    p = [Prim((0, 0.80, 0), (0.52, 0.25, 0.22), "piel"),
         Prim((0.36, 0.95, 0), (0.12, 0.22, 0.12), "piel"),
         Prim((0.34, 1.10, 0), (0.12, 0.10, 0.03), "crin"),
         Prim((-0.52, 0.80, 0), (0.035, 0.2, 0.035), "piel", "cola"),
         Prim((-0.54, 0.58, 0), (0.05, 0.07, 0.05), "crin", "cola"),
         Prim((0.58, 1.10, 0), (0.19, 0.12, 0.11), "piel", "cabeza"),
         Prim((0.74, 1.05, 0), (0.08, 0.09, 0.10), "hocico", "cabeza"),
         Prim((0.62, 1.16, 0.10), (0.025, 0.03, 0.02), "ojo", "cabeza"),
         Prim((0.62, 1.16, -0.10), (0.025, 0.03, 0.02), "ojo", "cabeza"),
         Prim((0.46, 1.34, 0.07), (0.04, 0.16, 0.035), "piel", "cabeza"),
         Prim((0.46, 1.34, -0.07), (0.04, 0.16, 0.035), "piel", "cabeza"),
         Prim((0.46, 1.34, 0.085), (0.02, 0.11, 0.02), "oreja", "cabeza"),
         Prim((0.46, 1.34, -0.085), (0.02, 0.11, 0.02), "oreja", "cabeza")]
    cad = {}
    for i, (x, z) in enumerate([(0.36, 0.12), (0.36, -0.12), (-0.36, 0.12), (-0.36, -0.12)]):
        g = f"pata{i}"
        p += pata(x, z, 0.58, 0.06, "piel", "crin", g)
        cad[g] = ((x, 0.66, z), 0 if i in (0, 3) else math.pi)
    return Animal("npc/fauna_donkey", "Burro",
                  {"piel": (140, 134, 140), "crin": (74, 66, 74), "hocico": (214, 208, 204),
                   "oreja": (206, 170, 170), "ojo": (40, 34, 44)},
                  p, cad, (0.42, 1.00, 0))


def conejo():
    p = [Prim((0, 0.18, 0), (0.16, 0.13, 0.12), "piel"),
         Prim((-0.15, 0.22, 0), (0.05, 0.05, 0.05), "cola"),
         Prim((0.14, 0.28, 0), (0.09, 0.08, 0.08), "piel", "cabeza"),
         Prim((0.22, 0.26, 0), (0.02, 0.02, 0.025), "hocico", "cabeza"),
         Prim((0.19, 0.31, 0.05), (0.018, 0.02, 0.015), "ojo", "cabeza"),
         Prim((0.19, 0.31, -0.05), (0.018, 0.02, 0.015), "ojo", "cabeza"),
         Prim((0.10, 0.44, 0.04), (0.03, 0.11, 0.022), "piel", "cabeza"),
         Prim((0.10, 0.44, -0.04), (0.03, 0.11, 0.022), "piel", "cabeza"),
         ]
    p[6].rot = rot_z(0.25)
    p[7].rot = rot_z(0.25)
    cad = {}
    # salta: las dos de adelante juntas y las dos de atrás juntas
    for i, (x, z, f) in enumerate([(0.08, 0.06, 0), (0.08, -0.06, 0), (-0.08, 0.08, math.pi),
                                   (-0.08, -0.08, math.pi)]):
        g = f"pata{i}"
        largo = 0.1 if i < 2 else 0.08
        p += [Prim((x, largo / 2 + 0.02, z), (0.03, largo / 2 + 0.02, 0.03), "piel", g),
              Prim((x + 0.03, 0.015, z), (0.05 if i >= 2 else 0.03, 0.015, 0.03), "piel", g)]
        cad[g] = ((x, 0.12, z), f)
    return escalar(Animal("npc/fauna_rabbit", "Conejo",
                          {"piel": (182, 150, 118), "cola": (236, 230, 220), "hocico": (218, 150, 156),
                           "ojo": (40, 34, 44)},
                          p, cad, (0.10, 0.24, 0), zancada=0.7), 1.5)


def pato():
    p = [Prim((0, 0.24, 0), (0.18, 0.12, 0.12), "piel"),
         Prim((-0.18, 0.30, 0), (0.06, 0.05, 0.05), "piel"),
         Prim((0.0, 0.26, 0.11), (0.12, 0.07, 0.025), "ala"),
         Prim((0.0, 0.26, -0.11), (0.12, 0.07, 0.025), "ala"),
         Prim((0.13, 0.40, 0), (0.05, 0.1, 0.05), "piel", "cabeza"),
         Prim((0.16, 0.48, 0), (0.07, 0.065, 0.06), "cabeza", "cabeza"),
         Prim((0.25, 0.46, 0), (0.055, 0.017, 0.032), "pico", "cabeza"),
         Prim((0.20, 0.51, 0.045), (0.016, 0.018, 0.014), "ojo", "cabeza"),
         Prim((0.20, 0.51, -0.045), (0.016, 0.018, 0.014), "ojo", "cabeza")]
    cad = {}
    for i, z in enumerate([0.05, -0.05]):
        g = f"pata{i}"
        p += [Prim((0.0, 0.07, z), (0.016, 0.07, 0.016), "pico", g),
              Prim((0.04, 0.012, z), (0.05, 0.012, 0.04), "pico", g)]
        cad[g] = ((0.0, 0.14, z), 0 if i == 0 else math.pi)
    return Animal("npc/fauna_duck", "Pato",
                  {"piel": (240, 238, 230), "ala": (214, 210, 202), "cabeza": (72, 132, 96),
                   "pico": (232, 170, 60), "ojo": (40, 34, 44)},
                  p, cad, (0.10, 0.34, 0), zancada=0.55)


ANIMALES = [vaca, gallina, oveja, cerdo, perro, cabra, burro, conejo, pato]

# — poses ——————————————————————————————————————————————————————————————

POSES = {  # verbo → (cuadros, fps)
    # 6 cuadros de paso (antes 4): con 4 la rodilla sólo aparecía doblada en uno
    # y el ojo no la registraba.
    "idle": (2, 3.0), "walk": (6, 10.0), "run": (6, 15.0), "eat": (2, 3.0), "bark": (2, 6.0),
}
DIRS = {"right": 0.0, "down": -math.pi / 2, "up": math.pi / 2}


def posar(a: Animal, verbo: str, k: int, n: int) -> list[Prim]:
    """Las primitivas del animal en el cuadro `k` de `n` del verbo."""
    fase = 2 * math.pi * k / n
    swing = {"walk": a.zancada, "run": a.zancada * 1.6}.get(verbo, 0.0)
    bob = {"walk": 0.015, "run": 0.035, "idle": 0.01}.get(verbo, 0.0)
    cabeza = {"eat": 0.9 if k == 0 else 1.05, "bark": -0.35 if k == 0 else -0.2}.get(verbo, 0.0)
    dy = bob * (abs(math.sin(fase)) if verbo != "idle" else (k % 2))
    fuera = []
    for p in a.prims:
        c = np.array(p.c, float)
        R = p.rot.copy()
        if p.grupo in a.caderas:
            cad, f0 = a.caderas[p.grupo]
            ph = fase + f0
            # La RODILLA: la caña se recoge mientras la pata AVANZA en el aire
            # (cuando la cadera gira hacia adelante, d/dt sin = cos > 0) y queda
            # derecha mientras apoya y empuja. Es lo que hace que el paso se lea
            # como un paso y no como un péndulo. La pezuña va hacia atrás: así
            # doblan el carpo (delante) y el corvejón (atrás) de un cuadrúpedo.
            if p.rodilla is not None and swing:
                dobla = a.flexion * (swing / a.zancada) * max(0.0, math.cos(ph)) ** 1.5
                if verbo == "run":
                    dobla *= 1.25
                Rk = rot_z(-dobla)
                kn = np.array(p.rodilla)
                c = kn + Rk @ (c - kn)
                R = Rk @ R
            Rg = rot_z(swing * math.sin(ph))
            c = np.array(cad) + Rg @ (c - np.array(cad))
            R = Rg @ R
        else:
            c = c + np.array([0, dy, 0])
            if p.grupo == "cabeza" and cabeza:
                piv = np.array(a.cuello) + np.array([0, dy, 0])
                Rg = rot_z(-cabeza)
                c = piv + Rg @ (c - piv)
                R = Rg @ R
            if p.grupo == "cola" and verbo in ("walk", "run", "bark"):
                piv = np.array(p.c) + np.array([0, -p.r[1], 0])
                Rg = rot_z(0.25 * math.sin(fase * 2))
                c = piv + Rg @ (c - piv)
                R = Rg @ R
        fuera.append(Prim(tuple(c), p.r, p.mat, p.grupo, R))
    return fuera


# — el render ——————————————————————————————————————————————————————————

def ejes():
    der = np.array([1.0, 0, 0])
    arriba = np.array([0, math.cos(PITCH), -math.sin(PITCH)])
    mira = np.array([0, -math.sin(PITCH), -math.cos(PITCH)])
    return der, arriba, mira


def trazar(prims, yaw, W, H, ox, oy, a: Animal, rampas):
    """Un rayo por píxel (ortográfica). Devuelve el RGBA del cuadro."""
    der, arriba, mira = ejes()
    Ry = rot_y(yaw)
    js, is_ = np.mgrid[0:H, 0:W]
    sx = (is_ + 0.5 - ox) / PX_M
    sy = (oy - (js + 0.5)) / PX_M
    orig = sx[..., None] * der + sy[..., None] * arriba - mira * 5.0
    mejor = np.full((H, W), np.inf)
    nrm = np.zeros((H, W, 3))
    mat = np.full((H, W), "", dtype=object)
    loc = np.zeros((H, W, 3))
    for p in prims:
        R = Ry @ p.rot
        c = Ry @ np.array(p.c)
        r = np.array(p.r)
        o = ((orig - c) @ R) / r
        d = (mira @ R) / r
        A = d @ d
        B = 2 * (o @ d)
        C = (o * o).sum(-1) - 1
        disc = B * B - 4 * A * C
        ok = disc >= 0
        t = np.where(ok, (-B - np.sqrt(np.maximum(disc, 0))) / (2 * A), np.inf)
        gana = ok & (t > 0) & (t < mejor)
        if not gana.any():
            continue
        mejor = np.where(gana, t, mejor)
        q = o + t[..., None] * d
        n = (q / r) @ R.T
        nrm[gana] = n[gana]
        mat[gana] = p.mat
        # punto en coordenadas del animal SIN girar (para las manchas)
        loc[gana] = ((orig + t[..., None] * mira) @ Ry)[gana]
    hit = np.isfinite(mejor)
    nrm /= np.maximum(np.linalg.norm(nrm, axis=-1, keepdims=True), 1e-6)
    lam = nrm @ LUZ
    out = np.zeros((H, W, 4), np.uint8)
    for j, i in zip(*np.nonzero(hit)):
        m = mat[j, i]
        if m == "piel" and a.manchas:
            if any(np.linalg.norm(loc[j, i] - np.array(cm)) < rm for cm, rm in a.manchas):
                m = "mancha"
        rp = rampas[m]
        l = lam[j, i]
        b = 3 if l > BANDAS[0] else 2 if l > BANDAS[1] else 1 if l > BANDAS[2] else 0
        out[j, i, :3] = rp[b]
        out[j, i, 3] = 255
    return limpiar(out)


def limpiar(im):
    """Píxel huérfano → el color de sus vecinos. Dos pasadas: la frontera entre
    dos bandas de luz deja motas sueltas que se leen como ruido (y como damero)."""
    for _ in range(2):
        src = im.copy()
        H, W = im.shape[:2]
        for j in range(1, H - 1):
            for i in range(1, W - 1):
                if src[j, i, 3] != 255:
                    continue
                vec = [tuple(src[j + dj, i + di]) for dj, di in ((0, 1), (0, -1), (1, 0), (-1, 0))]
                vec = [v for v in vec if v[3] == 255]
                if len(vec) < 3:
                    continue
                cuenta = {}
                for v in vec:
                    cuenta[v] = cuenta.get(v, 0) + 1
                mejor, n = max(cuenta.items(), key=lambda kv: kv[1])
                if n >= 3 and mejor != tuple(src[j, i]):
                    im[j, i] = mejor
    return im


def sombra(W, H, ox, oy, rx, rz, yaw):
    """La sombra de contacto del pack, proyectada del suelo (y = 0)."""
    _, arriba, _ = ejes()
    ex = abs(math.cos(yaw)) * rx + abs(math.sin(yaw)) * rz
    ez = abs(math.sin(yaw)) * rx + abs(math.cos(yaw)) * rz
    ez_px = ez * math.sin(PITCH) * PX_M
    js, is_ = np.mgrid[0:H, 0:W]
    dentro = ((is_ + 0.5 - ox) / (ex * PX_M)) ** 2 + ((js + 0.5 - oy) / max(ez_px, 1.5)) ** 2 <= 1
    out = np.zeros((H, W, 4), np.uint8)
    out[dentro] = (*SOMBRA, SOMBRA_ALFA)
    return out


def medir(a: Animal):
    """Tamaño de cuadro que alcanza para TODAS las poses, con el suelo fijo."""
    W = H = 96
    ox, oy = W / 2, H - 8
    rampas = {m: rampa(c, 4) for m, c in a.mats.items()}
    x0 = y0 = 10**9
    x1 = y1 = -1
    for verbo, (n, _) in POSES.items():
        for yaw in DIRS.values():
            for k in range(n):
                im = trazar(posar(a, verbo, k, n), yaw, W, H, ox, oy, a, rampas)
                ys, xs = np.nonzero(im[..., 3])
                if len(xs):
                    x0, x1 = min(x0, xs.min()), max(x1, xs.max())
                    y0, y1 = min(y0, ys.min()), max(y1, ys.max())
    # simétrico en x respecto del ancla (el espejo izquierda↔derecha no se corre)
    media = max(ox - x0, x1 + 1 - ox) + 1
    Wf = int(math.ceil(media)) * 2
    arriba = int(math.ceil(oy - y0)) + 1
    abajo = int(math.ceil(y1 + 1 - oy)) + 2
    return Wf, arriba + abajo, Wf / 2, arriba, rampas


def cuerpo_rx_rz(a: Animal):
    xs = [p.c[0] for p in a.prims if p.grupo == "cuerpo"]
    return (max(xs) - min(xs)) / 2 + 0.2, max(p.r[2] for p in a.prims if p.grupo == "cuerpo") + 0.03


# — armar la hoja ———————————————————————————————————————————————————————

def main():
    ver = "--ver" in sys.argv
    filas, sprites, objetos, tamanos = [], {}, {}, {}
    for fab in ANIMALES:
        a = fab()
        W, H, ox, oy, rampas = medir(a)
        rx, rz = cuerpo_rx_rz(a)
        cuadros, poses = [], {}
        for verbo, (n, fps) in POSES.items():
            if verbo == "bark" and not a.ladra:
                continue
            for dnom, yaw in DIRS.items():
                inicio = len(cuadros)
                for k in range(n):
                    base = sombra(W, H, ox, oy, rx, rz, yaw)
                    cuerpo = trazar(posar(a, verbo, k, n), yaw, W, H, ox, oy, a, rampas)
                    im = Image.fromarray(base)
                    im.alpha_composite(Image.fromarray(cuerpo))
                    l = Lienzo(W, H)
                    arr = np.array(im)
                    for j, i in zip(*np.nonzero(arr[..., 3])):
                        l.punto(int(i), int(j), tuple(int(v) for v in arr[j, i, :3]), int(arr[j, i, 3]))
                    # todas las poses comparten el ancla del suelo (si no, el animal
                    # salta al cambiar de pose): el ras se mide en la hoja, no por cuadro
                    try:
                        l.verificar(apoyado=False)
                    except SystemExit as e:
                        l.im.resize((W * 12, H * 12), Image.NEAREST).save(
                            str(SCRATCH / "falla.png"))
                        raise SystemExit(f"{a.clave} {verbo}_{dnom} #{k}: {e}")
                    cuadros.append(l.im)
                poses[f"{verbo}_{dnom}"] = {"rect": {"col": inicio, "row": len(filas), "w": 1, "h": 1},
                                            "anim": {"fps": fps, "frames": n}, "dir": "Right"}
        filas.append((a, W, H, cuadros, poses))
        tamanos[a.clave] = (W / PX_M, H / PX_M)

    # Una hoja por animal (cada una con su propia grilla = su cuadro).
    gen = RAIZ / "native/crates/ha-game/assets/art/gen/fauna"
    lamina = []
    for a, W, H, cuadros, poses in filas:
        hoja = Image.new("RGBA", (W * len(cuadros), H), (0, 0, 0, 0))
        for k, c in enumerate(cuadros):
            hoja.paste(c, (k * W, 0))
        nombre = a.clave.split("_")[-1]
        rel = f"art/gen/fauna/{nombre}.png"
        for pz in poses.values():
            pz["rect"]["row"] = 0
        primero = poses["idle_down"]["rect"]
        sprites[a.clave] = {"grid": [W, H], "png": rel, "rect": dict(primero), "poses": poses}
        objetos[a.clave] = a.nombre
        lamina.append(hoja)
        if not ver:
            gen.mkdir(parents=True, exist_ok=True)
            hoja.save(gen / f"{nombre}.png")

    scratch = SCRATCH
    ancho = max(h.width for h in lamina)
    L = Image.new("RGBA", (ancho, sum(h.height + 4 for h in lamina)), (122, 160, 90, 255))
    y = 0
    for h in lamina:
        L.alpha_composite(h, (0, y))
        y += h.height + 4
    L.resize((L.width * 3, L.height * 3), Image.NEAREST).save(scratch / "fauna3d.png")

    for k, (w, h) in tamanos.items():
        print(f"{k}: Vec2::new({w:.2f}, {h:.2f})")
    if ver:
        return
    man_p = RAIZ / "native/crates/ha-game/assets/art/manifest.json"
    man = json.loads(man_p.read_text())
    man["sprites"].update(sprites)
    for k, label in objetos.items():
        man["objects"].setdefault(k, {})["label"] = label
    man_p.write_text(json.dumps(man, ensure_ascii=False, indent=1))
    print(f"{len(sprites)} animales publicados en el manifest")


if __name__ == "__main__":
    main()
