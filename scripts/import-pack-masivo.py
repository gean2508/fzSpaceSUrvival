#!/usr/bin/env python3
"""**Mete el pack ENTERO en el juego**: los ~11.700 sprites de Modern Exteriors
y Modern Interiors, sin repetir ninguno y sin dejar ninguno afuera.

    python3 scripts/import-pack-masivo.py            # importa y escribe todo
    python3 scripts/import-pack-masivo.py --dry-run  # sólo informa

## Por qué existe

El importador viejo (`import-pack-objects.py`) trabajaba con una TABLA a mano
(`pack-objects.tsv`, 178 líneas): alguien miraba la hoja de contacto, elegía un
sprite y le escribía el nombre. Así entraron 176 objetos de 11.700 — el 98 % del
arte comprado nunca llegó al juego. Las calles, sin ir más lejos, se dibujaban
con el color plano del basalto porque nadie había curado el asfalto.

Este script hace lo mismo pero completo y automático. Lo que antes era criterio
humano ahora sale de tres cosas que están EN EL PIXEL:

1. **Qué es un objeto.** Los singles del pack no son todos objetos enteros: los
   interiores sí (una cama es un PNG de 80×80), pero los exteriores vienen
   *partidos* en piezas modulares (`Condo_1_1..10` son diez tajadas verticales
   del mismo edificio). Una tajada suelta no se lee como nada. Acá cada single se
   UBICA en la hoja de su tema (por hash de celda, ver `ubicar_en_hoja`) y las
   piezas de una misma familia que quedan pegadas se recortan JUNTAS: el objeto
   que entra al juego es el edificio entero, no la tajada.

2. **Qué es una variante y qué es otro objeto.** Dos sprites con la MISMA
   silueta (idéntico canal alfa) y distinto color son el mismo mueble en otro
   color — no dos muebles. Se declaran como variantes (`objeto#2`) del mismo
   slot, y el mundo elige una por celda con `variant_key_seeded`. Por eso 556
   PNG de dormitorio son 205 objetos: la cama es UNA cama con 56 colores, y un
   pueblo deja de verse fotocopiado sin costar un slot más.

3. **Cuánto mide.** La huella sale del tamaño del PNG por `ESCALA_TILE`, que no
   se elige a ojo: es el metraje de una celda medido contra el personaje (ver la
   constante).

## Sin repetir

Se deduplica por hash de píxeles ANTES de agrupar: el pack trae el mismo sprite
en varios temas (la arena de la playa está también en terrenos) y esas copias no
generan slots nuevos. Los 11.691 archivos son 11.304 dibujos distintos.

## Qué escribe

- `art/pack/<tema>_<n>.png` — páginas de atlas de 1024×1024. Una página por
  tema (o varias si no entra) para que el lote de dibujo no se rompa y para que
  la carga perezosa (`art2d`) traiga sólo lo que se está mirando. Nada de 11.700
  texturas sueltas: eso es lo que mataba al Redmi por OOM.
- `art/manifest.json` — una entrada por sprite con su `grid` + `rect` en celdas,
  y un `objects` con las variantes de cada objeto.
- Los bloques `// === PACK:START/END ===` de `ha-assets/src/slots.rs` y
  `ha-shared/src/placeables.rs`.

Todo lo que el script escribe queda anotado en `scripts/pack-import-index.json`:
en la corrida siguiente borra **lo suyo** y nada más, así el arte que curaste a
mano en HA Studio no se pisa nunca.
"""

import argparse
import hashlib
import json
import os
import re
import sys
import unicodedata
from collections import Counter, defaultdict

from PIL import Image

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pack_lexico import (  # noqa: E402
    ADJETIVOS,
    FRASES,
    FRASES_ADJETIVAS,
    PALABRAS,
    PARTES,
    RUIDO,
    TEMAS,
    genero,
)

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACK = os.path.join(ROOT, "PRUEBAS-CODIGOS")
ART = os.path.join(ROOT, "native/crates/ha-game/assets/art")
MANIFEST = os.path.join(ART, "manifest.json")
PAGES_DIR = os.path.join(ART, "pack")
PAGES_REL = "art/pack"
SLOTS_RS = os.path.join(ROOT, "native/crates/ha-assets/src/slots.rs")
PLACEABLES_RS = os.path.join(ROOT, "native/crates/ha-shared/src/placeables.rs")
INDEX = os.path.join(ROOT, "scripts/pack-import-index.json")

EXT_DIR = os.path.join(PACK, "modernexteriors-win/Modern_Exteriors_32x32/ME_Theme_Sorter_32x32")
INT_DIR = os.path.join(PACK, "moderninteriors-win (Copy 1)/1_Interiors/32x32")

CELL = 32
# 1024 px por página: 4 MB de textura. Chico a propósito — con carga perezosa lo
# que se paga es por página, y una página de 2048 (16 MB) por tocar un solo banco
# es exactamente el gasto que hay que evitar en un teléfono de 3,8 GB.
PAGE = 1024
# **Metros de mundo que mide una celda de 32 px del pack. NO se elige a ojo.**
#
# Decía 1,5 con el argumento de que "LimeZu dibuja a 32 px por tile de ~0,66 m".
# Ese 0,66 nunca se midió contra nada del juego, y la escala del juego ya estaba
# fijada por el ÚNICO objeto cuyo tamaño real conocemos — el personaje:
#
#     marco del personaje (LimeZu)   32 × 64 px  = 1 × 2 celdas
#     dibujado en el juego           0,95 × 1,90 m   (`ha-game::scene`)
#     ⇒ una celda                    0,95 m
#
# Con 1,5 todo el pack entró un 58 % más grande que el personaje que camina entre
# medio: bancos de tres metros, tachos más altos que el jugador, latas de metro y
# medio. Se ve en cualquier captura de calle. Si algún día el personaje se dibuja
# a otra altura, este número sale de la misma división, no de la intuición.
ESCALA_TILE = 0.95
# Lado máximo (tiles) de un colocable: el de un chunk de superficie
# (`ha-shared::surface_chunk::SURFACE_CHUNK_SIZE` / `TILE_M`).
TOPE_COLOCABLE = 48


# ---------------------------------------------------------------------------
# 1. Descubrir los temas del pack
# ---------------------------------------------------------------------------

def temas():
    """Los 48 temas del pack: nombre, carpeta de singles y hoja completa.

    La hoja hace falta para saber CÓMO se arman las piezas modulares; sin ella un
    edificio partido en diez tajadas entra al juego como diez tajadas.
    """
    out = []
    for d in sorted(os.listdir(EXT_DIR)):
        full = os.path.join(EXT_DIR, d)
        if not os.path.isdir(full) or not d.endswith("_Singles_32x32"):
            continue
        num = d.split("_", 1)[0]
        tema = re.sub(r"^\d+_", "", d).replace("_Singles_32x32", "")
        hoja = next(
            (
                os.path.join(EXT_DIR, f)
                for f in sorted(os.listdir(EXT_DIR))
                if f.endswith(".png") and f.split("_", 1)[0] == num
            ),
            None,
        )
        out.append(("ext", tema, full, hoja))
    singles_int = os.path.join(INT_DIR, "Theme_Sorter_Singles_32x32")
    hojas_int = os.path.join(INT_DIR, "Theme_Sorter_32x32")
    for d in sorted(os.listdir(singles_int)):
        full = os.path.join(singles_int, d)
        if not os.path.isdir(full):
            continue
        num = d.split("_", 1)[0]
        tema = re.sub(r"^\d+_", "", d)
        tema = re.sub(r"_S[Ii]ngles_32x32$", "", tema)
        tema = re.sub(r"_32x32$", "", tema)
        hoja = next(
            (
                os.path.join(hojas_int, f)
                for f in sorted(os.listdir(hojas_int))
                if f.endswith(".png") and f.split("_", 1)[0] == num
            ),
            None,
        )
        out.append(("int", tema, full, hoja))
    return out


def orden_natural(nombre):
    """`x_10` va después de `x_9`, no antes. El orden del filesystem no es orden."""
    return [int(t) if t.isdigit() else t for t in re.split(r"(\d+)", nombre)]


# Basura que encabeza los nombres de archivo del pack y no dice nada del objeto.
PREFIJOS_MUERTOS = {"me", "singles", "single", "v", "modern", "exteriors", "interiors"}


def familia_e_indice(kind, tema, stem):
    """Parte el nombre de archivo en (familia, índice).

    Exteriores: `ME_Singles_City_Props_32x32_Bench_3` → (`Bench`, 3).
    Interiores: `Bedroom_Singles_32x32_137` → (`Bedroom`, 137). Ahí la familia no
    dice nada (el pack no los nombra), así que el objeto lo decide la silueta.

    El pack escribe el tamaño en cualquier parte del nombre (`..._32x32_Tree_3`
    pero también `..._Country_House_32x32`), y repite el nombre del tema delante
    de todo. Las dos cosas se tiran acá: dejarlas producía claves como
    `escuela/escuela_yard_tubo_32x` — el tema dos veces y media medida suelta.
    """
    s = re.sub(r"\d+x\d+", "_", stem)
    s = re.sub(r"_+", "_", s).strip("_")
    tokens = [t for t in s.split("_") if t]
    tema_tokens = {t.lower() for t in re.sub(r"^\d+_", "", tema).split("_")}
    i = 0
    while i < len(tokens):
        t = tokens[i].lower()
        if t in PREFIJOS_MUERTOS or t in tema_tokens or (i == 0 and t.isdigit()):
            i += 1
            continue
        break
    tokens = tokens[i:]
    s = "_".join(tokens)
    m = re.match(r"^(.*?)_?(?:Variation_)?(\d+)$", s)
    if m and m.group(1):
        return m.group(1), int(m.group(2))
    if m and not m.group(1):
        return tema, int(m.group(2))
    return s or tema, 0


# ---------------------------------------------------------------------------
# 2. Ubicar cada single en la hoja de su tema
# ---------------------------------------------------------------------------

def celdas_de_hoja(path):
    """Índice `hash de celda 32×32 → posiciones` de una hoja. Las celdas vacías
    no entran: son la mitad de la hoja y no identifican nada."""
    im = Image.open(path).convert("RGBA")
    w, h = im.size
    idx = defaultdict(list)
    px = im.tobytes()
    stride = w * 4
    for row in range(h // CELL):
        for col in range(w // CELL):
            trozos = []
            vacio = True
            for y in range(CELL):
                off = (row * CELL + y) * stride + col * CELL * 4
                linea = px[off : off + CELL * 4]
                trozos.append(linea)
                if vacio and any(linea[3::4]):
                    vacio = False
            if vacio:
                continue
            idx[hashlib.md5(b"".join(trozos)).hexdigest()].append((col, row))
    return im, {h: set(v) for h, v in idx.items()}


def celdas_de_sprite(im):
    """Celdas no vacías de un single, como `(dcol, drow) → hash`."""
    w, h = im.size
    px = im.tobytes()
    stride = w * 4
    out = {}
    for row in range((h + CELL - 1) // CELL):
        for col in range((w + CELL - 1) // CELL):
            trozos = []
            vacio = True
            for y in range(CELL):
                yy = row * CELL + y
                if yy >= h:
                    trozos.append(b"\0" * (CELL * 4))
                    continue
                off = yy * stride + col * CELL * 4
                linea = px[off : off + CELL * 4]
                if len(linea) < CELL * 4:
                    linea = linea + b"\0" * (CELL * 4 - len(linea))
                trozos.append(linea)
                if vacio and any(linea[3::4]):
                    vacio = False
            if vacio:
                continue
            out[(col, row)] = hashlib.md5(b"".join(trozos)).hexdigest()
    return out


def ubicar_en_hoja(sprite_celdas, idx_hoja):
    """Dónde cae este single dentro de la hoja, en celdas. `None` si no se puede
    afirmar (celda repetida en mil lados o recorte que la hoja no tiene).

    Sólo acierta con los singles **alineados a la grilla**, que son justo los que
    importan: las piezas modulares (tajadas de edificio, tramos de cerca) se
    recortan celda a celda. Un banco o un barril vienen recortados a su silueta,
    caen a media celda y no se ubican — pero tampoco hace falta, porque ya son
    objetos enteros y no hay nada que componer.
    """
    if not sprite_celdas:
        return None
    # Se ancla en la celda MENOS común: una celda de pasto liso aparece 400 veces
    # y no ubica nada; una con el remate de un techo aparece una sola vez.
    ancla = min(
        sprite_celdas.items(),
        key=lambda kv: len(idx_hoja.get(kv[1], ())) or 10**9,
    )
    (acol, arow), ahash = ancla
    candidatos = idx_hoja.get(ahash, ())
    if not candidatos or len(candidatos) > 64:
        return None
    for col, row in candidatos:
        base = (col - acol, row - arow)
        if base[0] < 0 or base[1] < 0:
            continue
        if all(
            (base[0] + c, base[1] + r) in idx_hoja.get(h, ())
            for (c, r), h in sprite_celdas.items()
        ):
            return base
    return None


# ---------------------------------------------------------------------------
# 3. Nombres: clave de código y etiqueta en castellano
# ---------------------------------------------------------------------------

def ascii_slug(s):
    s = unicodedata.normalize("NFKD", s).encode("ascii", "ignore").decode()
    s = re.sub(r"[^A-Za-z0-9]+", "_", s).strip("_").lower()
    return re.sub(r"_+", "_", s) or "objeto"


def castellano(familia, tema, tema_es, indice):
    """El nombre humano del objeto. Sin nombre en el pack (interiores), se usa el
    tema + el número, que al menos ubica: "Dormitorio 137" se encuentra."""
    crudo = familia.strip().strip("_")
    sin_nombre = (
        not crudo
        or crudo.lower() in (tema.lower(), tema_es.lower())
        or re.fullmatch(r"\d+", crudo)
    )
    if sin_nombre:
        return f"{tema_es} {indice}" if indice else tema_es
    low = crudo.lower()
    # Frases hechas primero: "Trash_Can" es un tacho, no una lata de basura. Se
    # exige borde de palabra o `solar_panels` se come a `solar_panel` y deja la
    # `s` colgando fuera de la frase ("panel solar»s").
    #
    # Las adjetivas van ANTES que las de núcleo (son más largas y más
    # específicas: `no_banisters` tiene que ganarle a `banisters`) y se marcan
    # distinto —`‹m|f›`— porque su lugar en la frase es otro: detrás del
    # sustantivo, concordando. Ver `pack_lexico::FRASES_ADJETIVAS`.
    for frase, (m, f) in sorted(FRASES_ADJETIVAS.items(), key=lambda kv: -len(kv[0])):
        low = re.sub(rf"(?<![a-z]){re.escape(frase)}(?![a-z])", f"‹{m}|{f}›", low)
    for frase, es in sorted(FRASES.items(), key=lambda kv: -len(kv[0])):
        low = re.sub(rf"(?<![a-z]){re.escape(frase)}(?![a-z])", f"«{es}»", low)
    piezas = re.split(r"_+", low)
    nucleo, adjetivos, partes = [], [], []
    for p in piezas:
        if not p or p in RUIDO or p.isdigit():
            continue
        if p.startswith("‹"):
            adjetivos.append(tuple(p.strip("‹›").split("|")))
        elif p.startswith("«"):
            nucleo.append(p.strip("«»"))
        elif p in ADJETIVOS:
            adjetivos.append(ADJETIVOS[p])
        elif p in PARTES:
            partes.append(PARTES[p])
        elif p in PALABRAS:
            nucleo.append(PALABRAS[p])
        else:
            nucleo.append(p)
    if not nucleo:
        nucleo = [tema_es.lower()] if not partes else [partes.pop(0)]
    frase = " ".join(nucleo)
    g = genero(frase)
    if adjetivos:
        frase += " " + " ".join(a[0] if g == "m" else a[1] for a in adjetivos)
    if partes:
        frase += " (" + " ".join(partes) + ")"
    frase = re.sub(r"\s+", " ", frase).strip()
    return frase[:1].upper() + frase[1:]


# Carpeta (= prefijo de la clave) por tema. Es la agrupación que ya usa el editor.
CARPETA_POR_TEMA = {
    "Terrains_and_Fences": "nature",
    "City_Terrains": "floor",
    "City_Props": "prop",
    "Generic_Building": "building",
    "Floor_Modular_Building": "building",
    "Garage_Sales": "prop",
    "Villas": "building",
    "Worksite": "prop",
    "Shopping_Center_and_Markets": "building",
    "Vehicles": "vehicle",
    "Camping": "prop",
    "Hotel_and_Hospital": "building",
    "School": "building",
    "Swimming_Pool": "prop",
    "Police_Station": "building",
    "Office": "furniture",
    "Garden": "nature",
    "Fire_Station": "building",
    "Graveyard": "prop",
    "Subway_and_Train_Station": "building",
    "Beach": "agua",
    "Post_Office": "building",
    "MIlitary_Base": "building",
    "Additional_Houses": "building",
}
# Los interiores son muebles salvo que la palabra diga otra cosa.
CARPETA_INTERIOR = "furniture"

# Palabras que mandan sobre el tema: un árbol del cementerio sigue siendo flora.
CARPETA_POR_PALABRA = [
    ("nature", ("tree", "trees", "bush", "shrub", "hedge", "flower", "flowers",
                "sprout", "grass", "mushroom", "moss", "palm", "cactus", "bamboo",
                "leaf", "leaves", "log", "stump", "rock", "rocks", "stone",
                "pebbles", "boulder", "sand", "dirt", "mound", "vine", "clover",
                "sunflower", "plant", "plants")),
    ("agua", ("water", "sea", "lake", "river", "wave", "waves", "buoy", "pier",
              "dock", "boat", "fish", "starfish", "shell", "coral", "seaweed",
              "anchor", "pool", "swim", "surf", "lifeguard", "fishing")),
    ("vehicle", ("car", "truck", "van", "bus", "taxi", "bike", "bicycle",
                 "motorbike", "motorcycle", "train", "wagon", "tram", "ship",
                 "yacht", "helicopter", "plane", "ambulance", "camper",
                 "caravan", "trailer", "tractor", "forklift", "crane")),
    ("fence", ("fence", "railing", "banister", "gate", "barrier", "barbed",
               "hedge_wall", "wall")),
    ("building", ("house", "houses", "condo", "villa", "building", "roof",
                  "shop", "store", "tower", "church", "castle", "hangar",
                  "garage", "shed", "barn", "kiosk", "station", "greenhouse")),
    ("floor", ("sidewalk", "asphalt", "pavement", "road", "tile", "tiles",
               "carpet", "rug", "ground_floor_tile")),
]


#: **Carpetas que son DE AFUERA.** Un objeto de Modern Interiors no puede caer en
#: ninguna de éstas, dijera lo que dijera su nombre de archivo.
#:
#: `CARPETA_POR_PALABRA` manda sobre el tema, y eso está bien para los exteriores
#: (un árbol del cementerio sigue siendo flora) — pero aplicado a los interiores
#: mandaba el pack de adentro para afuera, porque el pack de interiores usa esas
#: mismas palabras para otra cosa:
#:
#:     Clothing_Store  → "store"   → building/tienda_de_ropa_11   (una PERCHA)
#:     Grocery_Store   → "store"   → building/almacen_1           (una GÓNDOLA)
#:     Ice_Cream_Shop  → "shop"    → building/heladeria_1         (un MOSTRADOR)
#:     Bathroom        → "wall"    → fence/…                      (un AZULEJO)
#:     Living_Room     → "plant"   → nature/…                     (una MACETA)
#:
#: Los tres primeros no quedaron en el catálogo y ya está: `building/heladeria_1`
#: llegó a ser un LOTE de la ciudad (`city::LOT_PROGRAMS`), o sea que el generador
#: plantaba un mostrador de heladería de 2×3 celdas en una parcela de la avenida
#: como si fuera el local. Y los `nature/*` de interior entran a las tablas de
#: siembra, que es como una maceta de living termina creciendo en el bosque.
#:
#: La regla es de una línea y no admite excepción: **lo de adentro se queda
#: adentro.** Lo cuida el test `nada_de_interiores_en_las_carpetas_de_afuera`.
CARPETAS_DE_AFUERA = ("nature", "agua", "vehicle", "fence", "building", "floor", "prop")


def carpeta(kind, tema, familia):
    low = familia.lower()
    for destino, palabras in CARPETA_POR_PALABRA:
        if kind != "ext" and destino in CARPETAS_DE_AFUERA:
            continue
        if any(re.search(rf"(^|_){re.escape(p)}(_|$)", low) for p in palabras):
            return destino
    return CARPETA_POR_TEMA.get(tema, CARPETA_INTERIOR) if kind == "ext" else CARPETA_INTERIOR


# Categoría del menú CONSTRUIR por carpeta. `floor` no es colocable: es suelo, se
# pinta con el editor de mundo, no se planta como un banco.
CAT_POR_CARPETA = {
    "nature": "Naturaleza",
    "prop": "Calle",
    "furniture": "Mueble",
    "building": "Edificio",
    "agua": "Agua",
    "vehicle": "Vehiculo",
    "fence": "Parcela",
    "structure": "Estructura",
}
MATERIAL_POR_CARPETA = {
    "nature": "Planta",
    "prop": "Madera",
    "furniture": "Madera",
    "building": "Piedra",
    "agua": "Madera",
    "vehicle": "Metal",
    "fence": "Madera",
    "structure": "Piedra",
}
MATERIAL_POR_PALABRA = [
    ("Metal", ("metal", "steel", "iron", "car", "truck", "machine", "pipe",
               "antenna", "pole", "grate", "container", "tank", "gun", "rail")),
    ("Piedra", ("stone", "rock", "marble", "brick", "concrete", "cement",
                "statue", "tomb", "grave", "fountain", "wall")),
    ("Madera", ("wood", "wooden", "plank", "log", "crate", "barrel", "bench",
                "table", "chair", "shelf", "fence")),
    ("Tela", ("tent", "towel", "clothes", "cloth", "curtain", "flag", "umbrella",
              "sofa", "cushion", "pillow", "blanket", "carpet", "rug", "bag")),
    ("Planta", ("tree", "bush", "flower", "grass", "plant", "sprout", "leaf",
                "mushroom", "hedge", "moss", "palm", "cactus")),
    ("Tierra", ("dirt", "sand", "mound", "ditch", "hole", "sandbox")),
]


def material(carpeta_, familia):
    low = familia.lower()
    for mat, palabras in MATERIAL_POR_PALABRA:
        if any(re.search(rf"(^|_){re.escape(p)}(_|$)", low) for p in palabras):
            return mat
    return MATERIAL_POR_CARPETA.get(carpeta_, "Madera")


# ---------------------------------------------------------------------------
# 4. Recolectar y agrupar
# ---------------------------------------------------------------------------

class Objeto:
    """Un objeto del juego: una clave, un nombre y N variantes con arte."""

    def __init__(self, kind, tema, tema_es, familia, indice):
        self.kind = kind
        self.tema = tema
        self.tema_es = tema_es
        self.familia = familia
        self.indice = indice
        self.variantes = []  # [(nombre_origen, PIL.Image)]
        self.clave = ""
        self.etiqueta = ""
        # La familia dio varios objetos (piezas que no se pudieron componer):
        # sin el número, el menú muestra ocho "Condominio" y no se sabe cuál es
        # cuál.
        self.desambiguar = False

    @property
    def size(self):
        return self.variantes[0][1].size


def recolectar(verbose=True):
    """Todos los objetos del pack, ya deduplicados, compuestos y agrupados."""
    vistos = {}
    objetos = []
    stats = Counter()
    for kind, tema, dir_singles, hoja_path in temas():
        tema_es = TEMAS.get(tema, tema.replace("_", " "))
        archivos = sorted(
            (f for f in os.listdir(dir_singles) if f.lower().endswith(".png")),
            key=orden_natural,
        )
        if not archivos:
            continue
        # Cargar los singles una vez: se usan para ubicar, componer y empaquetar.
        singles = []
        for f in archivos:
            im = Image.open(os.path.join(dir_singles, f)).convert("RGBA")
            if im.getbbox() is None:
                stats["vacios"] += 1
                continue
            fam, idx = familia_e_indice(kind, tema, f[:-4])
            singles.append({"file": f, "im": im, "fam": fam, "idx": idx})
        stats["singles"] += len(singles)

        # Ubicación en la hoja: sólo hace falta para los exteriores, que son los
        # que vienen partidos. Los interiores ya son objetos enteros.
        pos = {}
        if kind == "ext" and hoja_path and os.path.exists(hoja_path):
            _, idx_hoja = celdas_de_hoja(hoja_path)
            hoja_im = Image.open(hoja_path).convert("RGBA")
            for s in singles:
                p = ubicar_en_hoja(celdas_de_sprite(s["im"]), idx_hoja)
                if p:
                    pos[s["file"]] = p
            stats["ubicados"] += len(pos)
        else:
            hoja_im = None

        # Agrupar por familia. Dentro de una familia hay dos casos:
        #  · piezas (siluetas distintas y PEGADAS en la hoja) → se componen;
        #  · variantes (misma silueta) → un objeto con N variantes.
        por_familia = defaultdict(list)
        for s in singles:
            por_familia[s["fam"]].append(s)

        for fam, miembros in por_familia.items():
            miembros.sort(key=lambda s: s["idx"])
            grupos = componer(miembros, pos, hoja_im, kind)
            multiples = len(grupos) > 1
            for g in grupos:
                obj = Objeto(kind, tema, tema_es, fam, g["indice"])
                obj.desambiguar = multiples
                for nombre, im in g["imgs"]:
                    h = hashlib.md5(im.tobytes() + bytes(str(im.size), "ascii")).hexdigest()
                    if h in vistos:
                        stats["duplicados"] += 1
                        continue
                    vistos[h] = True
                    obj.variantes.append((nombre, im))
                if obj.variantes:
                    objetos.append(obj)
        if verbose:
            print(f"  {tema:34s} {len(singles):5d} singles", flush=True)
    return objetos, stats


def silueta(im):
    """Firma de la FORMA, no del color: el canal alfa y el tamaño. Dos sprites
    con la misma firma son el mismo objeto pintado distinto."""
    return hashlib.md5(im.getchannel("A").tobytes()).hexdigest() + str(im.size)


def tandas_por_silueta(miembros):
    """Parte una lista ordenada por índice en TANDAS de silueta idéntica.

    Es lo que separa "56 camas" de "una cama de 56 colores". El pack ordena las
    variantes de color seguidas —es cómo está dibujada la hoja—, así que la tanda
    se corta cuando cambia la forma. Se exige que los índices sean CONSECUTIVOS:
    sin eso, dos objetos distintos que casualmente comparten silueta (dos cajas
    lisas del mismo tamaño en puntas opuestas del tema) caerían en el mismo slot.
    """
    tandas = []
    for m in miembros:
        firma = silueta(m["im"])
        if tandas and tandas[-1]["firma"] == firma and m["idx"] == tandas[-1]["ultimo"] + 1:
            tandas[-1]["miembros"].append(m)
            tandas[-1]["ultimo"] = m["idx"]
        else:
            tandas.append({"firma": firma, "miembros": [m], "ultimo": m["idx"]})
    return tandas


def pegadas(alfa, ancho, alto, celda, d):
    """**¿El dibujo SIGUE de esta celda a la vecina?**

    Es la diferencia entre dos tajadas de un mismo edificio y dos edificios
    dibujados uno al lado del otro en la hoja, y el pack no la dice por ningún
    otro lado: las piezas de un condominio comparten el borde con la pared
    continuada —el techo cruza de una celda a la siguiente sin un pixel de
    aire—, mientras que dos casas enteras vecinas están separadas por la calle
    de transparencia que el autor dejó entre ellas.

    Se mide en la propia hoja: en la línea de contacto tiene que haber alguna
    fila (o columna) donde los dos píxeles enfrentados estén dibujados. Si en
    todo el borde hay aire de un lado, no se tocan y son dos objetos.

    ## Por qué existe

    La vecindad era la de la GRILLA: dos celdas ocupadas que se tocan iban al
    mismo grupo, y el grupo entero se recortaba como UN objeto. En
    `24_Additional_Houses` eso pegó `Terraced_House_1` y `Terraced_House_2` —dos
    casas de 12×15 completas, con 22 columnas de transparencia entre medio— en
    un recorte de 24×15, y ese recorte se convirtió en el arte de
    `building/casa_country` y de `building/casa_grande#17`. O sea: **el mapa
    plantaba dos casas en la parcela de una**, con la mitad del dibujo colgando
    fuera de su terreno. Lo mismo con `Modern_House` (23×14) contra su vecina y
    con media docena más.

    El tope de "un bloque de más de 24 celdas no es un objeto" que había no lo
    cazaba: 24 no es *más* de 24, y aunque lo fuera, el problema no es el tamaño
    —hay edificios legítimos de 24— sino que el criterio no miraba el dibujo.
    """
    c, r = celda
    if d[0] != 0:
        # Contacto vertical: la última columna de una contra la primera de la otra.
        x0 = (c + 1) * CELL - 1 if d[0] > 0 else c * CELL
        x1 = x0 + 1 if d[0] > 0 else x0 - 1
        if not (0 <= x0 < ancho and 0 <= x1 < ancho):
            return False
        y_ini, y_fin = r * CELL, min((r + 1) * CELL, alto)
        return any(alfa[x0, y] and alfa[x1, y] for y in range(y_ini, y_fin))
    y0 = (r + 1) * CELL - 1 if d[1] > 0 else r * CELL
    y1 = y0 + 1 if d[1] > 0 else y0 - 1
    if not (0 <= y0 < alto and 0 <= y1 < alto):
        return False
    x_ini, x_fin = c * CELL, min((c + 1) * CELL, ancho)
    return any(alfa[x, y0] and alfa[x, y1] for x in range(x_ini, x_fin))


def componer(miembros, pos, hoja_im, kind):
    """Parte una familia en objetos.

    Dos cosas pasan acá, y las dos existen porque el pack no viene en objetos:

    1. **Variantes.** Sprites seguidos con la misma silueta son el mismo objeto
       en otro color ([`tandas_por_silueta`]). Van a un solo slot con N variantes.
    2. **Piezas.** Las piezas de un mismo objeto quedan PEGADAS en la hoja del
       pack (las diez tajadas de un condominio comparten borde). Cuando eso pasa
       se recorta el bloque entero de la hoja: el objeto que entra al juego es el
       edificio, no la tajada.
    """
    tandas = tandas_por_silueta(miembros)
    sueltas = [
        {"indice": t["miembros"][0]["idx"], "imgs": [(m["file"], m["im"]) for m in t["miembros"]]}
        for t in tandas
    ]
    if kind != "ext" or hoja_im is None or len(tandas) < 2:
        return sueltas

    # ¿Son piezas? Sólo se puede afirmar de las que se ubicaron en la hoja, y
    # sólo del representante de cada tanda (las variantes de color de una pieza
    # caen todas en el mismo hueco de la hoja).
    ubicados = [t["miembros"][0] for t in tandas if t["miembros"][0]["file"] in pos]
    sueltos = [m for t in tandas for m in t["miembros"] if m["file"] not in pos]
    if len(ubicados) < 2:
        return sueltas

    ocupa = {}
    for m in ubicados:
        col, row = pos[m["file"]]
        w = (m["im"].size[0] + CELL - 1) // CELL
        h = (m["im"].size[1] + CELL - 1) // CELL
        for c in range(col, col + w):
            for r in range(row, row + h):
                ocupa[(c, r)] = m["file"]

    # Componentes conexas sobre las celdas que ocupa la familia — pero la
    # vecindad NO es "estas dos celdas se tocan en la grilla": es **el dibujo
    # sigue de una a la otra** ([`pegadas`]). Ver ahí por qué.
    alfa = hoja_im.getchannel("A").load()
    ancho_hoja, alto_hoja = hoja_im.size
    visto, clusters = set(), []
    for celda in ocupa:
        if celda in visto:
            continue
        pila, grupo = [celda], []
        visto.add(celda)
        while pila:
            c, r = pila.pop()
            grupo.append((c, r))
            for d in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                v = (c + d[0], r + d[1])
                if v not in ocupa or v in visto:
                    continue
                if ocupa[v] != ocupa[(c, r)] and not pegadas(
                    alfa, ancho_hoja, alto_hoja, (c, r), d
                ):
                    continue
                visto.add(v)
                pila.append(v)
        clusters.append(grupo)

    salida = []
    for grupo in clusters:
        piezas = {ocupa[c] for c in grupo}
        if len(piezas) == 1:
            m = next(m for m in ubicados if m["file"] in piezas)
            salida.append({"indice": m["idx"], "imgs": [(m["file"], m["im"])]})
            continue
        c0 = min(c for c, _ in grupo)
        r0 = min(r for _, r in grupo)
        c1 = max(c for c, _ in grupo) + 1
        r1 = max(r for _, r in grupo) + 1
        # Un bloque enorme casi nunca es UN objeto: es media hoja pegada. Se corta
        # por lo sano y las piezas entran sueltas, que es peor nombre pero no es
        # una mentira de 30 tiles.
        if (c1 - c0) > 24 or (r1 - r0) > 24:
            for m in (m for m in ubicados if m["file"] in piezas):
                salida.append({"indice": m["idx"], "imgs": [(m["file"], m["im"])]})
            continue
        recorte = hoja_im.crop((c0 * CELL, r0 * CELL, c1 * CELL, r1 * CELL))
        idx = min(m["idx"] for m in ubicados if m["file"] in piezas)
        nombre = sorted(piezas)[0]
        salida.append({"indice": idx, "imgs": [(nombre, recorte)], "compuesto": True})
        # Los colores alternativos de una pieza compuesta NO se pierden: entran
        # sueltos. Componerlos pediría recortar la hoja una vez por color, y el
        # objetivo es que no falte ni un dibujo, no que sobre elegancia.
        for t in tandas:
            if t["miembros"][0]["file"] in piezas:
                for m in t["miembros"][1:]:
                    salida.append({"indice": m["idx"], "imgs": [(m["file"], m["im"])]})
    for m in sueltos:
        salida.append({"indice": m["idx"], "imgs": [(m["file"], m["im"])]})
    return salida


# ---------------------------------------------------------------------------
# 5. Claves únicas
# ---------------------------------------------------------------------------

def fuera_del_bloque(texto):
    """El archivo Rust sin lo que este script genera — o sea, lo CURADO a mano."""
    ini = texto.index("// === PACK:START ===")
    fin = texto.index("// === PACK:END ===")
    return texto[:ini] + texto[fin:]


def claves_curadas():
    """Las claves de slot que ya existen escritas a mano.

    Hay que mirarlas aunque no estén en el manifest: un slot curado puede no tener
    arte todavía (`prop/parrilla` era exactamente ese caso) y aun así el nombre
    está tomado. Sin esto el importador generaba una clave repetida y lo cazaba el
    test `keys_are_unique`, ya con todo escrito.
    """
    usadas = set()
    if os.path.exists(MANIFEST):
        with open(MANIFEST) as fh:
            usadas |= set(json.load(fh).get("sprites", {}).keys())
    # **Menos lo que escribió la corrida ANTERIOR de este mismo script.**
    #
    # Sin esto el importador no es idempotente y se pisa a sí mismo: en la segunda
    # corrida el manifest ya trae sus 12.000 claves, así que TODAS chocan y todos
    # los objetos salen rebautizados (`prop/pileta_1` → `prop/pileta_pileta_1` →
    # `prop/pileta_pileta_pileta_1`…). Cada corrida dejaba muertas las referencias
    # curadas de la anterior y engordaba los nombres un tema por vez.
    #
    # Las claves de la corrida anterior están anotadas en `pack-import-index.json`
    # justamente para poder soltarlas: son SUYAS, no de nadie. Lo que queda en
    # `usadas` es lo que de verdad está ocupado por otro dueño — los slots curados
    # a mano y lo que escriben los otros importadores.
    if os.path.exists(INDEX):
        with open(INDEX) as fh:
            usadas -= {k.split("#")[0] for k in json.load(fh).get("sprites", [])}
    with open(SLOTS_RS) as fh:
        curado = fuera_del_bloque(fh.read())
    usadas |= set(re.findall(r'"([a-z_]+/[A-Za-z0-9_ñáéíóú#/-]+)"', curado))
    # `items/*` y `machines/*` se arman en código a partir de una lista de ids.
    for lista, prefijo in (("ITEM_IDS", "items"), ("MACHINE_IDS", "machines")):
        m = re.search(rf"{lista}: &\[\(&str, &str\)\] = &\[(.*?)\];", curado, re.S)
        if m:
            usadas |= {f"{prefijo}/{i}" for i in re.findall(r'\("([^"]+)",', m.group(1))}
    return usadas


def ids_curados():
    """Los ids de colocable escritos a mano en `placeables.rs`."""
    with open(PLACEABLES_RS) as fh:
        curado = fuera_del_bloque(fh.read())
    return set(re.findall(r'p\("([^"]+)"', curado))


#: **Nombres de familia que no dicen nada solos.** Llevan el tema adelante SIEMPRE.
#:
#: El pack bautiza el dibujo principal de un tema con la palabra pelada: el
#: cuartel de bomberos entero es `18_Fire_Station_32x32_Building`, o sea familia
#: `Building`. Traducido palabra por palabra eso es "Edificio", y como la clave
#: `building/edificio` estaba libre se la quedó — en un catálogo de 7.800 slots,
#: el cuartel de bomberos se llamaba "Edificio".
#:
#: No es sólo feo. `import-edificios.py` reparte fachadas por PÁGINA, no por
#: nombre, así que el cuartel seguía siendo elegible; pero cualquiera que abra
#: HA Studio, el menú de construir o `slots.rs` buscando el cuartel no lo
#: encuentra, y el que quiera saber qué es `building/edificio` tiene que ir a
#: mirar el pixel. El desempate por tema ya existía para las colisiones de clave
#: (`{tema}_{raiz}`); esto lo hace obligatorio cuando el nombre es genérico,
#: colisione o no.
FAMILIAS_GENERICAS = {
    "building", "buildings", "house", "houses", "shop", "store", "sign",
    "signs", "door", "doors", "window", "windows", "wall", "walls", "roof",
    "floor", "props", "prop", "object", "objects", "item", "items",
}


def asignar_claves(objetos):
    usadas = claves_curadas()
    for obj in objetos:
        obj.etiqueta = castellano(obj.familia, obj.tema, obj.tema_es, obj.indice)
        if obj.desambiguar and obj.indice and not re.search(r"\d$", obj.etiqueta):
            obj.etiqueta = f"{obj.etiqueta} {obj.indice}"
        carp = carpeta(obj.kind, obj.tema, obj.familia)
        raiz = ascii_slug(obj.etiqueta)[:44]
        generica = obj.familia.strip("_").lower() in FAMILIAS_GENERICAS
        if generica:
            obj.etiqueta = f"{obj.etiqueta} de {obj.tema_es.lower()}"
        clave = f"{carp}/{raiz}"
        if generica or clave in usadas:
            clave = f"{carp}/{ascii_slug(obj.tema_es)}_{raiz}"[:64]
        n = 2
        base = clave
        while clave in usadas:
            clave = f"{base}_{n}"
            n += 1
        usadas.add(clave)
        obj.clave = clave
        obj.carpeta = carp
    return objetos


# ---------------------------------------------------------------------------
# 6. Atlas
# ---------------------------------------------------------------------------

def empaquetar(objetos, del_mundo=(), dry_run=False):
    """Pega los sprites en páginas de 1024×1024, una tanda por tema.

    Estantes por altura (shelf packing): el arte de un tema tiene pocas alturas
    distintas, así que ordenar por alto y llenar filas desperdicia poquísimo y se
    lee de un vistazo en el PNG — que importa cuando algo sale mal.

    **`del_mundo` va aparte, y no es un detalle.** Lo que el mundo siembra se
    dibuja por miles y con Y-sort las texturas se alternan en el orden de dibujo:
    si cada banco, cada arbusto y cada tacho viven en la página de SU tema, el
    batcher de Bevy rompe el lote en casi cada sprite. Medido en el Redmi con las
    tablas de siembra repartidas por tema: **106 draw calls y 20 fps** con la CPU
    en cero — o sea, no era trabajo, eran lotes rotos. Juntando lo sembrado en
    unas pocas páginas `mundo_N`, la aldea entera vuelve a caber en un puñado de
    lotes. Es la misma lección que ya había dejado escrita `pack-world-atlas.py`.
    """
    paginas = {}          # nombre de página → PIL.Image
    ubicacion = {}        # (clave sprite) → (pagina, col, row, w, h)
    del_mundo = set(del_mundo)
    por_tema = defaultdict(list)
    for obj in objetos:
        # El tema de empaquetado NO es el tema del pack para lo sembrado: es
        # "mundo", que es donde se dibuja.
        tema = "mundo" if obj.clave in del_mundo else obj.tema
        for i, (nombre, im) in enumerate(obj.variantes):
            por_tema[tema].append((obj, i, im))

    for tema, items in por_tema.items():
        items.sort(key=lambda t: (-t[2].size[1], -t[2].size[0]))
        slug = ascii_slug(tema)
        pag_i, x, y, fila_alto = 0, 0, 0, 0
        pag = Image.new("RGBA", (PAGE, PAGE), (0, 0, 0, 0))
        for obj, i, im in items:
            w, h = im.size
            cw, ch = (w + CELL - 1) // CELL, (h + CELL - 1) // CELL
            pw, ph = cw * CELL, ch * CELL
            if pw > PAGE or ph > PAGE:
                # Sprite más grande que la página: se lleva una hoja propia. Son
                # cuatro casas gigantes; partirlas sería peor.
                nombre_pag = f"{slug}_xl{pag_i}"
                sola = Image.new("RGBA", (pw, ph), (0, 0, 0, 0))
                sola.paste(im, (0, 0))
                paginas[nombre_pag] = sola
                ubicacion[(obj.clave, i)] = (nombre_pag, 0, 0, cw, ch)
                pag_i += 1
                continue
            if x + pw > PAGE:
                x, y = 0, y + fila_alto
                fila_alto = 0
            if y + ph > PAGE:
                paginas[f"{slug}_{pag_i}"] = pag
                pag_i += 1
                pag = Image.new("RGBA", (PAGE, PAGE), (0, 0, 0, 0))
                x, y, fila_alto = 0, 0, 0
            pag.paste(im, (x, y))
            ubicacion[(obj.clave, i)] = (f"{slug}_{pag_i}", x // CELL, y // CELL, cw, ch)
            x += pw
            fila_alto = max(fila_alto, ph)
        paginas[f"{slug}_{pag_i}"] = pag

    if not dry_run:
        os.makedirs(PAGES_DIR, exist_ok=True)
        for nombre, im in paginas.items():
            im.save(os.path.join(PAGES_DIR, f"{nombre}.png"), optimize=True)
    return paginas, ubicacion


# ---------------------------------------------------------------------------
# 7. Escribir manifest, slots.rs y placeables.rs
# ---------------------------------------------------------------------------

#: Cómo se dice cada `kind` del pack en el manifest.
ORIGEN = {"ext": "exterior", "int": "interior"}

#: **El juego pide una cosa y el pack la dibujó con otro nombre.**
#:
#: `clave del juego -> clave que este importador le puso al arte del pack`. Se
#: publica la clave del juego apuntando a los MISMOS píxeles (mismo `png`, mismo
#: `rect`), con las mismas variantes. No copia un pixel y no se puede quedar vieja:
#: los rects salen del empaquetado de ESTA corrida.
#:
#: Existe porque el catálogo de slots (`ha-assets::slots`) es la lista de lo que el
#: JUEGO necesita —tiene sentido de gameplay: "zona de pesca" es donde se pesca— y
#: el nombre que sale del pack es el del archivo del artista. Cuando las dos listas
#: hablan de lo mismo hay que unirlas en algún lado, y el único lugar donde eso no
#: se pudre es acá, al lado de la medición.
#:
#: El caso que la motivó: `fishing/spot` estaba documentado en `slots.rs` como "los
#: `Fishing_Spot_1..4` de `11_Camping`" (el muelle con las cañas, exterior) y su
#: entrada del manifest apuntaba a **un acuario de Modern Interiors**. Se sembraba
#: en `SHORE_DECOR`, así que las orillas del mundo tenían peceras de living
#: apoyadas en el barro. Lo caza `la_ciudad_no_se_construye_con_interiores`.
#: `clave del juego -> (clave del pack, etiqueta en castellano)`.
ALIAS_CURADOS = {
    "fishing/spot": ("agua/zona_de_pesca", "Zona de pesca"),
}


def origen_de_paginas(objetos, ubicacion):
    """`art/pack/<pagina>.png` → `"exterior"` / `"interior"`.

    **Por qué se publica y no se deduce.** Del lado del juego, saber de qué pack
    salió un dibujo es la única forma barata de que nadie lo use donde no va: una
    página es de afuera o es de adentro, y con eso un test puede plantarse si un
    lote de la ciudad, una tabla de siembra o un slot `building/*` terminan
    apuntando a un mostrador de heladería. Deducirlo del nombre de la página no
    sirve —`office`, `hospital` y `condominium` son temas de LOS DOS packs— y
    deducirlo de la carpeta de la clave es circular: la carpeta es justamente lo
    que se está verificando.

    Una página que mezcle los dos orígenes es un error de empaquetado, no un dato:
    se marca `"mixto"` para que el test la cace en vez de elegir uno por mayoría.
    """
    origen = {}
    for obj in objetos:
        quiero = ORIGEN.get(obj.kind, obj.kind)
        for i, _ in enumerate(obj.variantes):
            pagina = ubicacion[(obj.clave, i)][0]
            rel = f"{PAGES_REL}/{pagina}.png"
            previo = origen.setdefault(rel, quiero)
            if previo != quiero:
                origen[rel] = "mixto"
    return origen


def escribir_manifest(objetos, ubicacion, dry_run=False):
    with open(MANIFEST) as fh:
        man = json.load(fh)
    sprites = man.setdefault("sprites", {})
    objects = man.setdefault("objects", {})
    origen = man.setdefault("origen", {})

    previo = {"sprites": [], "objects": []}
    if os.path.exists(INDEX):
        with open(INDEX) as fh:
            previo = json.load(fh)
    for k in previo.get("sprites", []):
        sprites.pop(k, None)
    for k in previo.get("objects", []):
        objects.pop(k, None)
    for k in previo.get("origen", []):
        origen.pop(k, None)
    nuevo_origen = origen_de_paginas(objetos, ubicacion)
    origen.update(nuevo_origen)

    nuevos_sprites, nuevos_objects = [], []
    for obj in objetos:
        for i, _ in enumerate(obj.variantes):
            pagina, col, row, w, h = ubicacion[(obj.clave, i)]
            entrada = {
                "png": f"{PAGES_REL}/{pagina}.png",
                "grid": [CELL, CELL],
                "rect": {"col": col, "row": row, "w": w, "h": h},
            }
            clave = obj.clave if i == 0 else f"{obj.clave}#{i + 1}"
            sprites[clave] = entrada
            nuevos_sprites.append(clave)
            if len(obj.variantes) > 1:
                # La variante 1 también necesita su clave `#1`: el juego pide
                # SIEMPRE `objeto#n` cuando el objeto declara variantes.
                if i == 0:
                    sprites[f"{obj.clave}#1"] = dict(entrada)
                    nuevos_sprites.append(f"{obj.clave}#1")
        if len(obj.variantes) > 1:
            objects[obj.clave] = {
                "label": obj.etiqueta,
                "variants": [
                    {"id": str(i + 1), "label": f"{obj.etiqueta} {i + 1}"}
                    for i in range(len(obj.variantes))
                ],
            }
            nuevos_objects.append(obj.clave)

    for clave_juego, (clave_pack, etiqueta) in sorted(ALIAS_CURADOS.items()):
        fuente = [k for k in nuevos_sprites if k == clave_pack or k.startswith(f"{clave_pack}_")]
        fuente = sorted({k.split("#")[0] for k in fuente})
        if not fuente:
            print(f"  ⚠ alias {clave_juego}: no encontré {clave_pack}* en el pack")
            continue
        for i, k in enumerate(fuente, 1):
            for destino in ([clave_juego] if i == 1 else []) + [f"{clave_juego}#{i}"]:
                sprites[destino] = dict(sprites[k])
                nuevos_sprites.append(destino)
        if len(fuente) > 1:
            objects[clave_juego] = {
                "label": etiqueta,
                "variants": [{"id": str(i), "label": f"{etiqueta} {i}"} for i in range(1, len(fuente) + 1)],
            }
            nuevos_objects.append(clave_juego)

    if not dry_run:
        with open(MANIFEST, "w") as fh:
            json.dump(man, fh, indent=1, ensure_ascii=False, sort_keys=True)
        with open(INDEX, "w") as fh:
            json.dump(
                {
                    "sprites": nuevos_sprites,
                    "objects": nuevos_objects,
                    "origen": sorted(nuevo_origen),
                },
                fh,
                indent=1,
            )
    return len(nuevos_sprites), len(nuevos_objects)


def bloque(texto, cuerpo):
    ini = texto.index("// === PACK:START ===")
    fin = texto.index("// === PACK:END ===")
    salto = texto.rindex("\n", 0, ini) + 1
    sangria = texto[salto:ini]
    return (
        texto[: ini + len("// === PACK:START ===")]
        + "\n"
        + cuerpo
        + sangria
        + texto[fin:]
    )


def escribir_rust(objetos, dry_run=False):
    def esc(s):
        return s.replace("\\", "\\\\").replace('"', '\\"')

    slots = "".join(
        f'    ("{o.clave}", "{esc(o.etiqueta)}"),\n' for o in objetos
    )
    with open(SLOTS_RS) as fh:
        txt = fh.read()
    txt = bloque(txt, slots)
    if not dry_run:
        with open(SLOTS_RS, "w") as fh:
            fh.write(txt)

    lineas, ids = [], ids_curados()
    for o in objetos:
        cat = CAT_POR_CARPETA.get(o.carpeta)
        if cat is None:
            continue
        w, h = o.size
        tw = max(1, round(w / CELL * ESCALA_TILE))
        th = max(1, round(h / CELL * ESCALA_TILE))
        # Lo que no entra en un chunk de superficie no se puede COLOCAR: cruzaría
        # siempre un borde y se streamearía en dos pedazos (`nada_es_gratis_ni_
        # absurdo`). Sigue existiendo como slot —el editor de mundo lo pinta—,
        # sólo que no aparece en el menú de construir.
        if tw > TOPE_COLOCABLE or th > TOPE_COLOCABLE:
            continue
        pid = ascii_slug(o.clave)
        n = 2
        base = pid
        while pid in ids:
            pid = f"{base}_{n}"
            n += 1
        ids.add(pid)
        mat = material(o.carpeta, o.familia)
        lineas.append(
            f'    p("{pid}", "{esc(o.etiqueta)}", "{o.clave}", '
            f"PlaceCat::{cat}, Material::{mat}, {tw}, {th}),\n"
        )
    with open(PLACEABLES_RS) as fh:
        txt = fh.read()
    txt = bloque(txt, "".join(lineas))
    if not dry_run:
        with open(PLACEABLES_RS, "w") as fh:
            fh.write(txt)
    return len(lineas)


# ---------------------------------------------------------------------------
# 8. Las tablas de siembra: qué se ve caminando por cada sitio
# ---------------------------------------------------------------------------

# Contexto → de qué temas y carpetas sale, y de qué tamaño máximo (en tiles).
#
# **Esto es lo que hace que importar sirva de algo.** Un slot declarado y no
# sembrado es un slot que sólo existe en el menú: el mundo se sigue viendo igual.
# Cada contexto es un sitio del mapa, y la lista que sale de acá es lo que se
# encuentra el jugador al llegar.
CONTEXTOS = {
    "VEREDA": {
        "temas": ("City_Props",),
        "carpetas": ("prop", "nature"),
        # **Lista blanca** (27 sep 2026): el tema City_Props trae de todo —escalera
        # rota, pozos, chatarra, piezas de estructuras, señales sueltas— y la vereda
        # salía hecha un depósito. En una vereda hay mobiliario urbano.
        "solo": r"banco|tacho_de_basura|contenedor_de_basura|trashbin|calle_hidrante|calle_buzon|parquimetro|maceta|arbusto|flores|paloma|cartel_informativo",
        # Dos celdas y pico. Con el tope en una sólo entraban los objetos de UNA
        # celda y la vereda se llenaba de basura y tapas de alcantarilla: el
        # banco, el farol y el hidrante miden dos.
        "max": 2.33,
    },
    # **En la CALZADA sólo va lo que está pintado o tirado en el suelo.**
    #
    # Con la regla de antes (`max: 5`, las mismas carpetas que la vereda) el
    # asfalto se sembraba con cajeros automáticos, bancos, bebederos y tachos:
    # los mismos objetos que la acera, pero en medio de la calle. Se veía
    # exactamente como lo que es, un error de colocación.
    #
    # Ahora manda el ALTO: `max_alto` deja pasar sólo lo de una celda —tapas de
    # alcantarilla, marcas de frenada, cartones, latas, conos— y todo lo que se
    # LEVANTA del suelo se queda en la vereda, que es su sitio. Es medible y no
    # depende del nombre, que en este pack no es de fiar.
    "CALLE": {
        "temas": ("City_Props",),
        "carpetas": ("prop",),
        # En la calzada sólo va lo que es PISO de la calle.
        "solo": r"tapa_de_alcantarilla|rejilla|marcas_de_frenada|lata_tirada",
        "max": 3.33,
        "max_alto": 1.07,
    },
    "PARQUE": {
        # Lista blanca (27 sep 2026): el tema de jardín trae el vivero entero
        # —regaderas, rastrillos, palas tiradas, estanterías, carretillas— y el
        # parque salía hecho un galpón. En un parque hay bancos, flores y fuentes.
        "solo": r"^(?!.*estante).*(banco|flor|girasol|arbusto|fuente|arco|estatua|brote)",
        "temas": ("Garden",),
        "carpetas": ("nature", "prop"),
        "max": 3.33,
    },
    "PLAYA": {
        "temas": ("Beach",),
        "carpetas": ("agua", "prop", "nature"),
        "max": 3.33,
    },
    "CEMENTERIO": {
        "temas": ("Graveyard",),
        "carpetas": ("prop", "nature", "fence"),
        "max": 3.33,
    },
    "BOSQUE": {
        "temas": ("Camping", "Terrains_and_Fences"),
        "carpetas": ("nature",),
        "max": 4.0,
    },
    "CAMPAMENTO": {
        "temas": ("Camping",),
        "carpetas": ("prop", "agua"),
        "max": 3.33,
    },
    "OBRA": {
        "temas": ("Worksite", "MIlitary_Base"),
        "carpetas": ("prop", "vehicle"),
        "max": 3.33,
    },
    # Sin `Swimming_Pool` ni `fence` (27 sep 2026): la pileta es un KIT —bordes,
    # partidores, soporte de salvavidas, juguetes que flotan ADENTRO del agua— y
    # sembrados sueltos en el pasto de la parcela eran "piezas que nada que ver".
    "PATIO": {
        "temas": ("Villas", "Additional_Houses"),
        "carpetas": ("prop", "nature"),
        "max": 3.33,
    },
}
# Tope por contexto. Con sesenta objetos distintos ya no se ve un patrón caminando
# una ciudad entera; más sólo engorda la tabla y las páginas de atlas que hay que
# tener cargadas a la vez.
TOPE_CONTEXTO = 60

#: **Lo que el pack dibujó como PIEZA de un kit, no como objeto.**
#:
#: Media biblioteca de este pack es modular: el semáforo se arma con el poste más
#: la calcomanía de lámparas, la cerca con doce losas de autotile, el galpón del
#: jardín con cuerpo + puerta + techo, el seto con tramos de pared. Sembrar una de
#: esas piezas suelta no decora nada — pone media cosa en el pasto. Es lo que se
#: veía: postes sin lámpara en las esquinas, un tramo de cerca solo en mitad de un
#: patio y arcos de seto plantados a campo abierto.
#:
#: **El pack lo dice en el nombre del archivo**, y por eso el filtro es por nombre
#: y no por pixel: `_Mod_` (de MODULAR), `Bottom_Left` y sus once hermanas del
#: autotile, `Inner_Corner`, `_Pass` (el hueco de la cerca), `_Layer`, `_Wall_`
#: (tramo de seto), `Diagonal` (los brazos del pórtico) y las piezas sueltas
#: `_Door` / `_Roof`. Lo que caza esto SIGUE EXISTIENDO como slot y como
#: colocable: se puede poner a mano desde el editor, que es donde una pieza
#: modular tiene sentido. Lo único que no puede es salir sorteada.
#:
#: `Traffic_Light` entero queda fuera aparte: el semáforo ya no se siembra, lo
#: pone la geometría del cruce (`ha_shared::worldgen::esquina_decor_at`) con las
#: cuatro caras que compone `scripts/import-semaforos.py`.
MARCAS_MODULARES = tuple(
    re.compile(p, re.I)
    for p in (
        r"(?:^|_)mod(?:ular)?(?:_|$)",
        r"(?:^|_)layer(?:_|$)",
        r"(?:top|bottom|middle)_(?:left|middle|right)(?:_|$)",
        r"(?:inner|outer)_corner",
        r"(?:^|_)corner(?:_|$)",
        r"(?:^|_)pass(?:_|$)",
        r"(?:^|_)wall(?:_|$)",
        r"(?:^|_)diagonal(?:_|$)",
        r"(?:^|_)(?:door|roof)(?:_\d+)?$",
        r"traffic_light",
        # La casa del árbol se arma con escalera + plataforma + copa: la
        # escalera sola en el pasto es media cosa.
        r"tree_?house",
        # **`Tree_Props_*` es la COPA, no el árbol.** Los 24 recortes de esa
        # familia miden 4×2 celdas y son la copa con el arranque del tronco,
        # cortada a ras por arriba y por abajo: van encima de un `Tree_N`, que
        # es el árbol entero. Sembrados solos salen como una losa de follaje
        # flotando sobre un tocón —el "¿qué carajos es ese árbol cortado?" de la
        # captura—, y así entraron cinco al bosque (`arbol_4`, `_10`, `_16`,
        # `_19`, `_22`). Ojo con generalizar a `props` a secas: `City_Props` es
        # el nombre de un TEMA y ahí dentro hay 900 objetos enteros.
        r"(?:^|_)tree_props(?:_|$)",
        # **LOS BLOBS DE TERRENO.** `Grass_1`…`Grass_4`, `Grass_Fenced_1` y
        # `Grass_Water_1`…`_4` no son nueve pastos: son nueve AUTOTILES de 22
        # piezas cada uno —el contorno, las diagonales, las esquinas internas y,
        # al final, los rellenos—, y lo que cambia entre familias no es el pasto
        # sino contra QUÉ hace la costura: tierra naranja la 1, el vacío la 2,
        # tierra khaki la 3 y la 4, la cerca de piedra la `Fenced`, el agua las
        # `Water`. El relleno de las nueve es el MISMO verde plano (71,151,87).
        #
        # Sembradas sueltas ponen media costura en el pasto, y así entraron al
        # BOSQUE: `pasto_1`, `pasto_13`…`pasto_18` (bordes de `Grass_2`) y sobre
        # todo `pasto_cercado_15/16/19`, que son **tramos del muro de piedra** de
        # `Grass_Fenced_1` — un pedazo de cerca tirado en mitad del monte.
        #
        # El número doble es lo que las distingue de un prop: la pieza del blob es
        # `Grass_2_13` (familia 2, pieza 13) y el prop es `Props_Grass_13`, que
        # lleva uno solo. Por eso el `\d+_\d+$` y no `grass` a secas.
        r"(?:^|_)grass(?:_water|_fenced)?_\d+_\d+$",
        # **`Mound_1` y `Mound_2` son UN montículo cada uno**, partido en tres y
        # en cinco piezas que se ponen en fila (más `Mound_2_6`, que es su relleno
        # de tierra). Sembrar una sola pone un tercio de montículo con el corte
        # vertical a la vista, y había ocho en el bosque: `monticulo_1`…`_5` y
        # `terreno_y_cercas_monticulo_1`…`_3`.
        r"(?:^|_)mound_\d+_\d+$",
    )
)


def limpiar_nombre(nombre):
    """El nombre de archivo listo para preguntarle si es una pieza.

    Saca dos cosas y las dos importan: la **resolución** (`32x32`), que si no se
    come el `_\\d+_` de cualquier patrón, y la **extensión**, que es lo que hacía
    inútil a toda marca anclada con `$`. `o.variantes` guarda el nombre CON `.png`
    (`singles[]["file"]`), así que `(?:^|_)(?:door|roof)(?:_\\d+)?$` no cazaba una
    sola puerta desde que se escribió, y las marcas nuevas de blob de terreno
    tampoco iban a cazar nada. Se vio en que `pasto_1` y `monticulo_1` seguían
    apareciendo en `BOSQUE` con el filtro puesto.
    """
    return re.sub(r"\.png$", "", re.sub(r"\d+x\d+", "_", nombre), flags=re.I)


def es_modular(o):
    """¿Este objeto es una PIEZA de un kit? Ver [`MARCAS_MODULARES`].

    Mira los nombres de archivo de TODAS sus variantes: el importador agrupa por
    familia, así que un objeto puede haber juntado una pieza suelta con una
    entera, y con que una lo sea ya no se puede sembrar a ciegas.
    """
    return any(
        rx.search(limpiar_nombre(nombre))
        for nombre, _ in o.variantes
        for rx in MARCAS_MODULARES
    )


#: **Lo que NO es decoración: es GAMEPLAY.**
#:
#: Un objeto que el jugador va a poder usar no puede estar además sembrado por
#: hash como adorno. El reporte fue el dron: *"en la ciudad yo veía el dron en la
#: vereda y era como que eso no es basura, pero tampoco debería ir ahí, porque el
#: dron va a ser un vehículo utilizable, incluso control remoto"*. Sembrado como
#: adorno, cada vereda del mundo tendría un dron que no se puede agarrar — y el
#: día que se pueda, un dron gratis cada seis pasos.
#:
#: Esto es distinto de [`MARCAS_MODULARES`]: aquello caza MEDIA COSA (una pieza de
#: kit), esto caza una cosa entera que **le pertenece a otro sistema**. Los dos
#: dejan el slot y el pixel intactos: lo único que se les quita es el derecho a
#: aparecer sorteados en el suelo.
#:
#: Se agrega acá lo que vaya teniendo dueño. Lo que hoy tiene:
#: - **dron, su control y su hélice** — vehículo de control remoto.
NO_ES_DECORACION = tuple(
    re.compile(p, re.I)
    for p in (
        r"(?:^|_)drones?(?:_|$)",
        r"(?:^|_)drone_(?:controller|propeller|blade)",
    )
)


def es_gameplay(o):
    """¿Este objeto le pertenece a otro sistema? Ver [`NO_ES_DECORACION`].

    Mira los nombres de archivo de todas las variantes, igual que `es_modular`:
    con que una lo sea, el objeto entero deja de sembrarse.
    """
    return any(
        rx.search(limpiar_nombre(nombre))
        for nombre, _ in o.variantes
        for rx in NO_ES_DECORACION
    )


def cobertura(im):
    """Fracción del recorte que tiene pixel opaco. 1,0 = no hay silueta."""
    alfa = im.split()[-1]
    return sum(1 for v in alfa.getdata() if v > 16) / max(1, im.width * im.height)


def elegir_siembra(objetos):
    """Qué objetos siembra el mundo en cada sitio. Devuelve
    `contexto → [(clave, ancho, alto, peso)]`."""
    por_contexto = {}
    for nombre, regla in CONTEXTOS.items():
        elegidos = []
        for o in objetos:
            if o.tema not in regla["temas"] or o.carpeta not in regla["carpetas"]:
                continue
            if "solo" in regla and not re.search(regla["solo"], o.clave):
                continue
            # **Primero: ¿es una pieza o es un objeto?** Antes de mirar dónde
            # entra hay que saber si va sola. Ver `MARCAS_MODULARES`.
            if es_modular(o):
                continue
            # **Y segundo: ¿es de otro sistema?** Un dron es un vehículo, no un
            # adorno de vereda. Ver `NO_ES_DECORACION`.
            if es_gameplay(o):
                continue
            w, h = o.size
            # **Una baldosa no es un objeto suelto.** Un recorte con el pixel al
            # 100 % no tiene silueta: es superficie (un autotile de terreno) o un
            # objeto con el suelo pegado detrás. Plantarlo deja un CUADRADO PLANO
            # sobre el pasto, y así entraron 18 —`pasto_3`, `pasto_agua_5`, el
            # empedrado, el piso de obra—, que es lo que se ve como manchas verdes
            # en cualquier captura de pueblo. El suelo del juego lo dibuja la
            # malla del chunk, no un sprite.
            #
            # **Y el tamaño no tiene nada que ver.** La regla miraba sólo los
            # recortes de una celda, así que las superficies GRANDES seguían
            # entrando: el entablado del muelle, la ladera de arena modular, el
            # suelo de obra. Son el mismo cuadrado plano, más grande. Lo que define
            # a una superficie es que no tenga silueta, y eso se mide igual midas
            # lo que midas.
            #
            # El corte del multi-celda es más duro (`ni un pixel transparente`) y
            # eso se midió: en un recorte grande el aire de un borde pesa muy poco,
            # así que con 0,98 caían objetos de verdad —un banco de jardín de 3×1
            # da 0,9987— mientras los que hay que echar dan 1,0000 clavado, porque
            # están dibujados para embaldosar. **Es el mismo número que
            # `reparar-importados.py::COBERTURA_BALDOSA_GRANDE`**: si se separan,
            # una corrida del importador vuelve a meter lo que la reparación echó.
            if cobertura(o.variantes[0][1]) >= (0.98 if w <= CELL and h <= CELL else 0.999):
                continue
            # **Los topes se miden en CELDAS del pack, no en metros.** Estaban en
            # metros y por eso se movían solos cada vez que alguien tocaba
            # `ESCALA_TILE`: al corregir la escala, "cabe en la vereda" pasaba a
            # querer decir otra cosa sin que nadie cambiara la regla. La celda es
            # la unidad en la que el artista dibujó, y es la que no se mueve.
            cw, ch = w / CELL, h / CELL
            if cw > regla["max"] or ch > regla["max"]:
                continue
            # Tope de ALTO aparte, para los sitios donde lo que importa es que el
            # objeto no se levante del suelo (la calzada, ver `CALLE`).
            if ch > regla.get("max_alto", 1e9):
                continue
            tw, th = cw * ESCALA_TILE, ch * ESCALA_TILE
            # El peso favorece lo que tiene variantes: un objeto con doce colores
            # puede repetirse doce veces sin que se note, uno solo no.
            peso = min(24, 6 + 3 * (len(o.variantes) - 1))
            elegidos.append((o.familia, o.clave, round(tw, 2), round(th, 2), peso))
        # **Una de cada cosa antes que ocho de la misma.** Ordenar sólo por peso
        # llenaba la vereda con `calle_arbusto_1..8` —ocho arbustos casi iguales—
        # y dejaba fuera el banco, el buzón y el semáforo. Se reparte por familia
        # en rondas: primero un objeto de cada familia, después el segundo de
        # cada una, y así.
        familias = defaultdict(list)
        for fam, clave, tw, th, peso in sorted(elegidos, key=lambda e: (-e[4], e[1])):
            familias[fam].append((clave, tw, th, peso))
        rondas, i = [], 0
        while len(rondas) < TOPE_CONTEXTO and any(len(v) > i for v in familias.values()):
            for fam in sorted(familias):
                if len(familias[fam]) > i and len(rondas) < TOPE_CONTEXTO:
                    rondas.append(familias[fam][i])
            i += 1
        por_contexto[nombre] = rondas
    return por_contexto


def escribir_siembra(por_contexto, dry_run=False):
    """Genera `ha-shared/src/pack_siembra.rs` a partir de [`elegir_siembra`]."""
    cuerpo = [
        "//! **Qué se ve caminando por cada sitio del mundo** — generado por\n"
        "//! `scripts/import-pack-masivo.py`, no se edita a mano.\n"
        "//!\n"
        "//! El pack entró al juego con 7.800 objetos, pero un objeto declarado y no\n"
        "//! sembrado no existe: el mundo se sigue viendo igual de vacío. Estas tablas\n"
        "//! son el puente — cada una es un SITIO (la vereda, el parque, la playa, el\n"
        "//! cementerio) y lo que hay en ella es lo que el jugador se encuentra al\n"
        "//! llegar. Las consume `crate::worldgen`.\n"
        "//!\n"
        "//! El peso favorece a los objetos con variantes de color: uno con doce\n"
        "//! colores puede salir doce veces sin que se note, uno solo cansa a la\n"
        "//! tercera.\n"
        "\n"
        "use crate::worldgen::ExtraDecor;\n"
    ]
    for nombre, elegidos in por_contexto.items():
        cuerpo.append(f"\npub const {nombre}: &[ExtraDecor] = &[\n")
        for clave, tw, th, peso in elegidos:
            cuerpo.append(
                f'    ExtraDecor {{ key: "{clave}", size: [{tw:.2f}, {th:.2f}], weight: {peso} }},\n'
            )
        cuerpo.append("];\n")
    destino = os.path.join(ROOT, "native/crates/ha-shared/src/pack_siembra.rs")
    if not dry_run:
        with open(destino, "w") as fh:
            fh.write("".join(cuerpo))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    print("Leyendo el pack…")
    objetos, stats = recolectar()
    asignar_claves(objetos)
    variantes = sum(len(o.variantes) for o in objetos)
    print(
        f"\n{len(objetos)} objetos · {variantes} sprites únicos "
        f"(de {stats['singles']} singles; {stats['duplicados']} repetidos, "
        f"{stats['ubicados']} ubicados en su hoja)"
    )
    # La siembra se elige ANTES de empaquetar: lo que el mundo planta se junta en
    # sus propias páginas para no romper el lote de dibujo (ver `empaquetar`).
    siembra = elegir_siembra(objetos)
    del_mundo = {clave for tabla in siembra.values() for clave, *_ in tabla}
    paginas, ubicacion = empaquetar(objetos, del_mundo, args.dry_run)
    del_mundo_pags = sum(1 for p in paginas if p.startswith("mundo"))
    print(f"{len(paginas)} páginas de atlas ({del_mundo_pags} de mundo)")
    ns, no = escribir_manifest(objetos, ubicacion, args.dry_run)
    print(f"manifest: {ns} sprites, {no} objetos con variantes")
    np_ = escribir_rust(objetos, args.dry_run)
    print(f"slots.rs: {len(objetos)} slots · placeables.rs: {np_} colocables")
    escribir_siembra(siembra, args.dry_run)
    print("siembra: " + " · ".join(f"{k} {len(v)}" for k, v in siembra.items()))
    if args.dry_run:
        print("(dry-run: no se escribió nada)")


if __name__ == "__main__":
    main()
