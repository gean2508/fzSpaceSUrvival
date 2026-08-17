#!/usr/bin/env python3
"""**LAS ORILLAS DEL SUELO BASE: tierra, arena y nieve contra el pasto.**

    python3 scripts/import-suelo-bordes.py
    python3 scripts/import-suelo-bordes.py --dry-run

## Por qué existe

El suelo del juego lo pinta la malla del chunk (`world2d::build_chunk_mesh`) con
**una losa plana por celda** de `art/pack/suelo.png`. Medido sobre la hoja
publicada: `tiles/grass` son 1.024 píxeles de (71,151,87) y ni uno más, y lo
mismo vale para la arena, la tierra y el hielo. O sea que donde dos materiales se
tocan hay un **corte a escuadra entre dos colores planos**, y por eso la cumbre
nevada se ve como un rectángulo celeste pegado sobre el prado y el pozo que cavás
sale como un cuadrado marrón con el borde a cuchillo.

El pack ya dibujó la transición y estaba sin usar. `Grass_3` de
`1_Terrains_and_Fences` es un **autotile completo de tierra contra pasto**: 22
recortes de 32×32 con las cuatro caras, las cuatro esquinas convexas, las cuatro
cóncavas y los dos rellenos. Clasificados por el propio script (ver
[`FORMA_DE_LA_PIEZA`] y `verificar`), no leídos de un `.tsx` que nadie tiene.

## Qué produce

`art/pack/suelo_bordes.png` y las claves `borde/*`:

| clave | qué orilla | de dónde sale el color |
|---|---|---|
| `borde/tierra` | tierra contra pasto | el kit, sin teñir |
| `borde/arena`  | arena contra pasto | la arena DEL MUNDO del pack (230,174,85) |
| `borde/nieve`  | nieve contra pasto | la rampa con la que ya se tiñe `cliff/nieve` |

y una variante por clima de pasto (`borde/tierra_bosque`, `_seco`, `_pantano`,
`_sierra`), porque el verde del kit tiene que ser el MISMO que el del tile de al
lado o la orilla se ve pegada. El clima lo elige el juego con `pasto_de_bioma`,
igual que ya hace con `cliff/pasto_*`.

## Lo que NO hace

**No toca el relleno.** El interior de una mancha de tierra lo sigue pintando la
malla del chunk, que es un solo draw call por chunk; estas piezas son sprites y
sólo se piden para las celdas de BORDE (`world2d::spawn_chunk_orilla` saltea la
forma `center`). Es la regla 2 del proyecto: el borde de una laguna son decenas
de celdas, su interior son miles.

## La arena es la del MUNDO, no la del suelo base

`atlas-suelo.py` elegía la losa de arena por parecido de color contra
`Tile::Sand::color()` y no encontraba ninguna (`SIN LOSA parecida`), así que la
playa quedaba en color plano inventado. La arena que el pack usa para el mundo es
**(230,174,85)** — medida en `Sea_Water_Tileset` (la orilla de `tiles/water_arena`)
y en la meseta del kit de acantilado (`import-acantilado.py::ARENA_ESPERADA`), que
son el mismo amarillo. Es la que entra acá, así que la playa, la orilla del agua y
el acantilado de arena son literalmente el mismo pixel.
"""

import argparse
import collections
import glob
import json
import os

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACK = os.path.join(ROOT, "PRUEBAS-CODIGOS")
TERRENOS = os.path.join(
    PACK,
    "modernexteriors-win/Modern_Exteriors_32x32/ME_Theme_Sorter_32x32",
    "1_Terrains_and_Fences_Singles_32x32",
)
ASSETS = os.path.join(ROOT, "native/crates/ha-game/assets")
MANIFEST = os.path.join(ASSETS, "art/manifest.json")
PAGINA_REL = "art/pack/suelo_bordes.png"
INDEX = os.path.join(ROOT, "scripts/suelo-bordes-import-index.json")

CELL = 32
#: La familia del pack: tierra khaki contra pasto, autotile entero.
FAMILIA = "Grass_3"

#: **Qué forma es cada recorte de la familia**, medido y no supuesto.
#:
#: Cada pieza se clasifica en una grilla de 3×3 mirando si el tercio es pasto o
#: tierra; de ahí sale hacia dónde mira el borde. El resultado (ver `verificar`,
#: que lo vuelve a comprobar en cada corrida) es esta tabla. Los nombres son los
#: de `world2d::autotile_shape`, así que el juego pide la pose por el mismo
#: nombre que ya usa para el agua y para el acantilado.
#:
#: Las formas que aparecen dos veces en el pack (las cóncavas) traen la misma
#: silueta con otra piedrita: se queda la primera y la segunda va de variante.
FORMA_DE_LA_PIEZA = {
    2: "edge_n",
    6: "edge_s",
    4: "edge_e",
    8: "edge_w",
    1: "corner_nw",
    3: "corner_ne",
    5: "corner_se",
    7: "corner_sw",
    9: "inner_se",
    10: "inner_sw",
    11: "inner_nw",
    12: "inner_ne",
    21: "center",
}
#: Los duplicados: misma forma, otra piedra. Entran como variante `#2`.
SEGUNDA_PIEZA = {17: "inner_se", 20: "inner_sw", 13: "inner_nw", 16: "inner_ne"}

#: **El verde del kit**, medido sobre las 13 piezas: el relleno del pasto y sus
#: tres tonos de mata. Es exactamente la rampa con la que `import-acantilado.py`
#: tiñe `cliff/pasto`, y no es casualidad — las dos salen del mismo pack.
VERDE_DEL_KIT = [(55, 133, 78), (71, 151, 87), (83, 166, 93), (121, 161, 92)]
#: El tono dominante del pasto: el ancla del teñido por clima.
VERDE_ANCLA = (71, 151, 87)
#: El tono dominante de la tierra: el ancla del teñido de material.
TIERRA_ANCLA = (170, 143, 90)

#: **La arena del mundo.** Medida en `Sea_Water_Tileset` (la orilla que ya usa
#: `tiles/water_arena`) y en la meseta del kit de acantilado. No es el color
#: inventado que tenía `Tile::Sand`.
ARENA_DEL_PACK = (230, 174, 85)
#: La nieve, del mismo sitio del que sale `cliff/nieve` (ver
#: `atlas-suelo.py::NIEVE_DEL_KIT`): la cumbre y su pared son el mismo blanco.
NIEVE_DEL_KIT = (212, 225, 239)

#: Los materiales que llevan orilla. `None` = el kit sin teñir.
#:
#: Los tres primeros muerden el pasto y su color se mide del pack. Los cuatro
#: **pastos de clima** se agregan en `main` porque su destino se mide del tile
#: que el juego ya pinta (`tiles/pasto_*`) y eso necesita el manifiesto abierto.
MATERIALES = [
    ("tierra", None, "Orilla de tierra"),
    ("arena", ARENA_DEL_PACK, "Orilla de arena"),
    ("nieve", NIEVE_DEL_KIT, "Orilla de nieve"),
]

#: **El orden total de los pastos**, espejo de `world2d::rango_de_pasto`.
#:
#: Una costura la dibuja el pasto de MAYOR rango y su fleco se tiñe del de menor,
#: así que sólo hacen falta las combinaciones en ese sentido: `pasto_sierra` con
#: fleco de bosque existe, `pasto_bosque` con fleco de sierra no se pide nunca.
#: Si el orden cambia allá y no acá sobran claves, no faltan: el juego cae a la
#: variante de referencia y no se pierde una celda.
RANGO_DE_PASTO = {"": 0, "bosque": 1, "seco": 2, "pantano": 3, "sierra": 4}

#: Los climas de pasto contra los que puede tocar una orilla. La clave es el sufijo
#: de la clave del slot; el color se MIDE del tile que el juego ya pinta al lado
#: (`tiles/pasto_*`), no se escribe a mano.
CLIMAS = [
    ("bosque", "tiles/pasto_bosque"),
    ("seco", "tiles/pasto_seco"),
    ("pantano", "tiles/pasto_pantano"),
    ("sierra", "tiles/pasto_sierra"),
]

PAGINA_COLS = 16


def luminancia(c):
    return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]


def piezas():
    """`{índice: Image}` de la familia, por el número del nombre de archivo."""
    fs = sorted(
        glob.glob(os.path.join(TERRENOS, f"*_{FAMILIA}_*.png")),
        key=lambda p: int(p.rsplit("_", 1)[1].split(".")[0]),
    )
    if not fs:
        raise SystemExit(f"no está la familia {FAMILIA} en {TERRENOS}")
    return {int(f.rsplit("_", 1)[1].split(".")[0]): Image.open(f).convert("RGBA") for f in fs}


def cuadrantes(im):
    """La pieza reducida a 3×3 letras: `G` pasto, `D` tierra.

    Es la medida con la que se clasifica el autotile. Se muestrea el centro de
    cada noveno (8×8 de los 10×10) para no leer el propio borde.
    """
    out = ""
    for gy in range(3):
        for gx in range(3):
            box = im.crop((gx * 10 + 1, gy * 10 + 1, gx * 10 + 9, gy * 10 + 9))
            verde = sum(1 for q in box.getdata() if q[3] > 0 and q[1] > q[0] + 15)
            out += "G" if verde > 32 else "D"
    return out


#: Lo que TIENE que medir cada forma, en cuadrantes. Si el pack cambia, falla acá
#: y no seis meses después con el mapa lleno de bordes al revés.
CUADRANTES_ESPERADOS = {
    "edge_n": "GGGDDDDDD",
    "edge_s": "DDDDDDGGG",
    "edge_e": "DDGDDGDDG",
    "edge_w": "GDDGDDGDD",
    "corner_nw": "GGGGGDGDD",
    "corner_ne": "GGGDGGDDG",
    "corner_se": "DDGDGGGGG",
    "corner_sw": "GDDGGDGGG",
    "inner_se": "DDDDDDDDG",
    "inner_sw": "DDDDDDGDD",
    "inner_nw": "GDDDDDDDD",
    "inner_ne": "DDGDDDDDD",
    "center": "DDDDDDDDD",
}


def verificar(kit):
    """El autotile es el que dice ser, o el script se planta."""
    for idx, forma in list(FORMA_DE_LA_PIEZA.items()) + list(SEGUNDA_PIEZA.items()):
        if idx not in kit:
            raise SystemExit(f"falta el recorte {FAMILIA}_{idx} del pack")
        medido = cuadrantes(kit[idx])
        if medido != CUADRANTES_ESPERADOS[forma]:
            raise SystemExit(
                f"{FAMILIA}_{idx} mide {medido} y para ser {forma} tendría que medir "
                f"{CUADRANTES_ESPERADOS[forma]}: el pack cambió"
            )


def paleta(kit):
    """Los colores del kit repartidos en `(verde, tierra)`.

    El verde es [`VERDE_DEL_KIT`] —los cuatro tonos del pasto—; todo lo demás es
    material. Se separan porque se tiñen por caminos distintos: el material lo
    decide la ORILLA (tierra, arena o nieve) y el verde lo decide el CLIMA del
    pasto que tiene al lado.
    """
    todos = collections.Counter()
    for idx in list(FORMA_DE_LA_PIEZA) + list(SEGUNDA_PIEZA):
        todos.update(q[:3] for q in kit[idx].getdata() if q[3] > 0)
    verde = {c for c in todos if c in set(VERDE_DEL_KIT)}
    return verde, set(todos) - verde


def mapa_anclado(origen, ancla, destino, aplana=1.0):
    """`{color: color teñido}` con el dominante clavado en `destino`.

    Mismo criterio que `import-acantilado.py::mapa_por_losa`: el tono que manda
    va exactamente al color pedido y el resto lo sigue por luminancia, así que la
    parte plana de la pieza queda indistinguible del tile de al lado y las matas
    y las piedritas conservan su volumen.

    `aplana` comprime la rampa hacia el ancla (1,0 = tal cual). Existe por la
    costura entre dos pastos: el lóbulo de material del kit es tierra con
    piedritas y el pasto es mucho más liso, así que teñido tal cual el borde
    salía empedrado. El número lo mide [`compresion_de_grano`].
    """
    base = max(luminancia(ancla), 1.0)
    out = {}
    for c in origen:
        f = luminancia(c) / base
        f = 1.0 + (f - 1.0) * aplana
        out[c] = tuple(min(255, max(0, int(destino[k] * f))) for k in range(3))
    return out


def compresion_de_grano(verde, material):
    """Cuánto hay que comprimir la rampa del material para que granule como el pasto.

    Es el cociente de los dos RANGOS de luminancia, medido sobre los colores del
    propio kit. Se imprime en cada corrida: si el pack cambia, el número cambia
    con él y no hay que tocar nada.
    """
    def rango(cs):
        ls = [luminancia(c) for c in cs]
        return max(ls) - min(ls) if ls else 0.0

    rv, rm = rango(verde), rango(material)
    if rm <= 1e-3:
        return 1.0
    return min(1.0, rv / rm)


def tenir(im, mapa):
    if not mapa:
        return im.copy()
    out = im.copy()
    px = out.load()
    for y in range(out.height):
        for x in range(out.width):
            p = px[x, y]
            if p[3] == 0:
                continue
            v = mapa.get(p[:3])
            if v:
                px[x, y] = (v[0], v[1], v[2], p[3])
    return out


def color_de_la_losa(clave):
    """El color dominante del tile de suelo `clave`, o `None` si no está curado.

    Es el mismo truco que usa `import-acantilado.py`: el verde de la orilla no se
    elige, se MIDE del pasto que el juego pinta pegado a ella.
    """
    man = json.load(open(MANIFEST))
    sp = man.get("sprites", {}).get(clave)
    if not sp or "rect" not in sp:
        return None
    png = os.path.join(ASSETS, sp["png"])
    if not os.path.exists(png):
        return None
    gw, gh = sp.get("grid", [CELL, CELL])
    r = sp["rect"]
    im = Image.open(png).convert("RGBA")
    box = (r["col"] * gw, r["row"] * gh, (r["col"] + r["w"]) * gw, (r["row"] + r["h"]) * gh)
    cuenta = collections.Counter(q[:3] for q in im.crop(box).getdata() if q[3] > 0)
    return cuenta.most_common(1)[0][0] if cuenta else None


class Hoja:
    """Empaquetador de celdas de 1×1, en filas de [`PAGINA_COLS`]."""

    def __init__(self):
        self.celdas = []

    def poner(self, tile):
        i = len(self.celdas)
        self.celdas.append(tile)
        return i % PAGINA_COLS, i // PAGINA_COLS

    def imagen(self):
        filas = (len(self.celdas) + PAGINA_COLS - 1) // PAGINA_COLS
        im = Image.new("RGBA", (PAGINA_COLS * CELL, max(filas, 1) * CELL), (0, 0, 0, 0))
        for i, t in enumerate(self.celdas):
            im.paste(t, ((i % PAGINA_COLS) * CELL, (i // PAGINA_COLS) * CELL))
        return im


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    kit = piezas()
    verificar(kit)
    verde, material = paleta(kit)
    print(f"{FAMILIA}: {len(kit)} recortes · {len(verde)} tonos de pasto · {len(material)} de material")

    # El verde de referencia es el del propio kit; los climas se miden del tile
    # que el juego ya pinta al lado.
    climas = [("", None)]
    for sufijo, losa in CLIMAS:
        c = color_de_la_losa(losa)
        if c is None:
            print(f"  (sin {losa} curado: la orilla {sufijo} se queda con el verde de referencia)")
            continue
        climas.append((sufijo, c))

    # **LA COSTURA ENTRE DOS PASTOS ES LA MISMA PIEZA.** Lo único que cambia es
    # que el lóbulo de material va a otro VERDE en vez de a un marrón: la sierra
    # contra la pradera es un cambio de losa tan visible como la arena contra el
    # pasto —(106,148,88) contra (71,151,87)— y sin pieza salía a escuadra.
    materiales = list(MATERIALES)
    nombre_del_clima = {"bosque": "bosque", "seco": "sabana", "pantano": "pantano", "sierra": "sierra"}
    for sufijo, losa in CLIMAS:
        c = color_de_la_losa(losa)
        if c is None:
            print(f"  (sin {losa} curado: no hay costura de {sufijo})")
            continue
        materiales.append((f"pasto_{sufijo}", c, f"Costura de {nombre_del_clima[sufijo]}"))

    pagina = Hoja()
    sprites = {}
    objetos = {}
    for mat, destino, etiqueta in materiales:
        # **El grano del lóbulo de material se comprime al del pasto.** La pieza
        # del kit es tierra con piedritas y el pasto es mucho más liso: teñida de
        # verde tal cual, la costura entre dos pastos salía empedrada. La
        # compresión es el cociente MEDIDO de los dos rangos de luminancia, no un
        # número a ojo, y sólo se aplica a las costuras pasto-pasto: en la tierra,
        # la arena y la nieve el grano es del material y tiene que estar.
        aplana = compresion_de_grano(verde, material) if mat.startswith("pasto_") else 1.0
        if aplana != 1.0:
            print(f"  {mat}: grano del material comprimido x{aplana:.2f} (medido del kit)")
        mapa_mat = (
            mapa_anclado(material, TIERRA_ANCLA, destino, aplana) if destino else {}
        )
        for sufijo, verde_destino in climas:
            # Una costura contra sí misma no existe, y contra un pasto de rango
            # mayor tampoco: la dibuja el otro lado (ver [`RANGO_DE_PASTO`]).
            if mat.startswith("pasto_"):
                mio = mat.removeprefix("pasto_")
                if RANGO_DE_PASTO[sufijo] >= RANGO_DE_PASTO[mio]:
                    continue
            mapa = dict(mapa_mat)
            if verde_destino:
                mapa.update(mapa_anclado(verde, VERDE_ANCLA, verde_destino))
            poses = {}
            for idx, forma in sorted(FORMA_DE_LA_PIEZA.items()):
                col, row = pagina.poner(tenir(kit[idx], mapa))
                poses[forma] = {"rect": {"col": col, "row": row, "w": 1, "h": 1}}
            clave = f"borde/{mat}" if not sufijo else f"borde/{mat}_{sufijo}"
            sprites[clave] = {"png": PAGINA_REL, "grid": [CELL, CELL], "poses": poses}
            objetos[clave] = etiqueta if not sufijo else f"{etiqueta} ({sufijo})"
            print(f"{clave}: {len(poses)} poses")

    im = pagina.imagen()
    if args.dry_run:
        print(f"\n[dry-run] hoja {im.size[0]}x{im.size[1]} px, {len(sprites)} claves")
        return

    os.makedirs(os.path.join(ASSETS, "art/pack"), exist_ok=True)
    im.save(os.path.join(ASSETS, PAGINA_REL), optimize=True)
    print(f"\n{PAGINA_REL}: {im.size[0]}x{im.size[1]} px")

    man = json.load(open(MANIFEST))
    hoja = man.setdefault("sprites", {})
    if os.path.exists(INDEX):
        for k in json.load(open(INDEX)).get("sprites", []):
            hoja.pop(k, None)
    hoja.update(sprites)
    objs = man.setdefault("objects", {})
    for clave, etiqueta in objetos.items():
        objs.setdefault(clave, {})["label"] = etiqueta
    with open(MANIFEST, "w") as f:
        json.dump(man, f, indent=1, ensure_ascii=False, sort_keys=True)
    with open(INDEX, "w") as f:
        json.dump({"sprites": sorted(sprites), "pagina": PAGINA_REL}, f, indent=1, ensure_ascii=False)
    print(f"manifest: {len(sprites)} claves de orilla")


if __name__ == "__main__":
    main()
