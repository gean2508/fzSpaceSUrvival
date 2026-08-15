#!/usr/bin/env python3
"""**El agua del mundo, entera y por familias de orilla.**

    python3 scripts/import-agua.py
    python3 scripts/import-agua.py --dry-run

## Por qué existe

El agua del juego estaba *mal armada*, no *sin arte*:

1. **La animación leía los tiles del vecino.** `Water_Tileset_32x32.png` son 8
   frames de un bloque de 3x3, uno al lado del otro: 24 columnas. El manifest
   apuntaba la pose `center` a la celda (1,1) con `frames: 8, dir: Right`, y una
   tira avanza **el ancho del frame** — que en esa pose es UNA celda. O sea que
   la animación del centro del lago recorría columnas 1,2,3,4… : el centro, la
   orilla este, la esquina del frame siguiente… El agua no ondulaba, se
   *deslizaba* por su propia orilla. Acá cada pose se copia a una tira propia de
   8 celdas contiguas, que es lo único que esa animación sabe leer.
2. **Faltaba media forma.** Sólo estaban las 9 del bloque básico: sin esquinas
   cóncavas (`inner_*`) un lago con una entrante dibujaba centro liso, y sin
   diagonales (`diag_*`) todos los lagos salían rectangulares.
3. **Una sola orilla para todo el planeta.** El pack trae tres familias — ribera
   de pasto, playa de arena y orilla pelada — y el juego usaba la de pasto en el
   desierto y en la sierra. La familia la elige el bioma
   (`ha_shared::agua::orilla_de`), igual que el material de un acantilado.
4. **El agua honda no tenía arte.** `Tile::WaterDeep` es medio lago: se dibujaba
   como un cuadrado azul plano en el medio del agua animada.

## Qué produce

`art/pack/agua.png` — una sola hoja, celdas de 32 px — y las claves del manifest:

| slot | fuente | dónde va |
|---|---|---|
| `tiles/water` | `Water_Tileset` | ribera de pasto: pradera, bosque, pantano |
| `tiles/water_arena` | `Sea_Water_Tileset` | playa de arena: costa, desierto, sabana |
| `tiles/water_roca` | `Sea_Water_Tileset_No_Sand` | orilla pelada: sierra, montaña, nieve |
| `tiles/water_deep` | `Deep_Water_1_*` | la hondura, dentro del bajío |
| `agua/cascada` (+2 variantes, ×9 materiales teñidos) | `Waterfall*` modulares | el salto entre dos niveles |
| `agua/desague` | `Waterfall_Drain_Tube` | el caño que descarga en el lago |
| `agua/nenufar`, `agua/mata_flotante`, `agua/hondura`, `agua/onda` | `Props_Water_*` | lo que flota |

Las tres familias de agua y las cascadas son **animadas** (8 y 6 frames): el arte
animado del pack se usa siempre que exista, nunca se congela en un frame.

## Nada de rects inventados

Ninguna forma se asigna a ojo. Cada celda del pack se **mide**: se toma la
paleta del tile de agua pura del bloque y se calcula qué fracción de cada borde
y de cada esquina es agua. De ahí sale el nombre de la forma
(`center`/`edge_n`/`corner_ne`/`inner_sw`/`diag_nw`…), que es el mismo que
escribe HA Studio y el que pide `world2d::autotile_shape`. Si una familia no
cierra su juego de formas, el script **falla** en vez de publicar media orilla.

Anota lo suyo en `scripts/agua-import-index.json`, así una segunda corrida
reemplaza sus claves sin tocar el resto del manifest.
"""

import json
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
PACK = ROOT / "PRUEBAS-CODIGOS/modernexteriors-win/Modern_Exteriors_32x32"
TERRENOS = PACK / "Animated_32x32/Animated_Terrains_32x32"
ANIMADOS = PACK / "Animated_32x32/Animated_sheets_32x32"
SINGLES = PACK / "ME_Theme_Sorter_32x32/1_Terrains_and_Fences_Singles_32x32"
SINGLE_PREF = "ME_Singles_Terrains_and_Fences_32x32_"

ASSETS = ROOT / "native/crates/ha-game/assets"
MANIFEST = ASSETS / "art/manifest.json"
PAGINA_REL = "art/pack/agua.png"
INDEX = ROOT / "scripts/agua-import-index.json"

CELL = 32
#: **Ancho de la hoja en celdas, medido contra la tira más ancha que se guarda.**
#:
#: Valía 32 y con las cascadas teñidas eso reventaba la hoja: una tira de cascada
#: son 3 celdas por 6 frames = **18 celdas**, y dos no entran en 32. El
#: empaquetador por estantes le daba un renglón entero a cada tira y desperdiciaba
#: 14 columnas de cada 32 — la hoja se fue a 1024×7008 px, por encima del máximo
#: de textura de buena parte de las GPU móviles (4096). Con 36 entran dos tiras
#: justas y no sobra nada.
HOJA_COLS = 36
#: Las tres familias de agua vienen a 8 frames; las cascadas a 6.
FPS_AGUA = 6.0
FPS_CASCADA = 8.0

# ---------------------------------------------------------------------------
# Medir: de qué forma es cada celda del pack
# ---------------------------------------------------------------------------

#: **Se mide por ESQUINAS, no por bordes.** La orilla del pack es una curva: en
#: la celda de una esquina convexa el agua no llega a tocar ningún borde entero,
#: así que medir las cuatro bandas laterales daba "tierra por los cuatro lados"
#: para media hoja. Los cuatro cuadrantes de 10x10, en cambio, dicen exactamente
#: hacia dónde sigue el agua, que es lo que define la forma.
CUADRANTES = {
    "nw": (0, 0, 10, 10),
    "ne": (22, 0, 32, 10),
    "sw": (0, 22, 10, 32),
    "se": (22, 22, 32, 32),
}
#: Un cuadrante cuenta como mojado por encima de esto. Los valores medidos son
#: casi binarios (0.00-0.12 o 0.96-1.00), así que el umbral no es delicado.
MOJADO = 0.55

#: `esquinas mojadas -> forma`. El nombre mira hacia donde NO hay agua, igual
#: que `world2d::autotile_shape`: `edge_n` es la celda con tierra al norte.
POR_ESQUINAS = {
    frozenset(("nw", "ne", "sw", "se")): "center",
    frozenset(("nw", "ne", "sw")): "inner_se",
    frozenset(("nw", "ne", "se")): "inner_sw",
    frozenset(("nw", "sw", "se")): "inner_ne",
    frozenset(("ne", "sw", "se")): "inner_nw",
    frozenset(("sw", "se")): "edge_n",
    frozenset(("nw", "ne")): "edge_s",
    frozenset(("ne", "se")): "edge_w",
    frozenset(("nw", "sw")): "edge_e",
    frozenset(("se",)): "corner_nw",
    frozenset(("sw",)): "corner_ne",
    frozenset(("ne",)): "corner_sw",
    frozenset(("nw",)): "corner_se",
}


def forma_de(tile, es_agua):
    """**El nombre autotile de esta celda, medido.** `None` si no es ninguna.

    `es_agua(pixel) -> bool` decide qué cuenta como líquido: para las orillas es
    "el pixel tira a azul" (la ribera es verde, la playa naranja y el rompiente
    blanco, todos fuera); para la hondura es la paleta exacta del azul oscuro.
    """
    mojadas = set()
    for nombre, caja in CUADRANTES.items():
        px = list(tile.crop(caja).getdata())
        if sum(1 for p in px if es_agua(p)) / len(px) > MOJADO:
            mojadas.add(nombre)
    return POR_ESQUINAS.get(frozenset(mojadas))


def azulado(p):
    """El pixel es líquido: el azul manda sobre el rojo y llega al verde.

    Sirve para las tres familias sin ajustar nada — el pasto es verde, la arena
    naranja, la tierra de la ribera marrón y la espuma blanca (`b == r`), y
    ninguno pasa. Es lo que permite medir la forma sin listar colores a mano.
    """
    r, g, b, a = p
    return a > 0 and b > r and b >= g


def paleta_de(tile):
    return {p for p in tile.getdata() if p[3] > 0}


# ---------------------------------------------------------------------------
# Las familias de orilla
# ---------------------------------------------------------------------------

#: `slot -> (bloque básico, esquinas cóncavas, ancho, alto, frames)`.
#:
#: Las hojas `*_Diagonals_*` del pack NO entran, y no es un olvido: medidas,
#: sus celdas dan la MISMA firma de esquinas que las convexas del bloque básico
#: (la orilla del pack ya es curva, así que su "diagonal" es otro dibujo de la
#: misma forma). Un autotile de 8 vecinos no puede pedirlas por separado, y
#: meterlas como variante pondría media orilla redonda y media en punta en el
#: mismo lago. La variedad de forma sale de la GEOMETRÍA del agua, no de más
#: tiles.
FAMILIAS = {
    "tiles/water": (
        "Water_Tileset_32x32.png",
        "Water_Tileset_Outer_Corners_32x32.png",
        3,
        3,
        8,
    ),
    "tiles/water_arena": (
        "Sea_Water_Tileset_Basic_32x32.png",
        "Sea_Water_Tileset_Outer_Corners_32x32.png",
        4,
        4,
        8,
    ),
    "tiles/water_roca": (
        "Sea_Water_Tileset_No_Sand_Basic_32x32.png",
        "Sea_Water_Tileset_No_Sand_Outer_Corners_32x32.png",
        4,
        4,
        8,
    ),
}

#: Sin estas formas una orilla no cierra: son las nueve del bloque básico. Las
#: `inner_*` y las `diag_*` mejoran el dibujo pero el juego cae solo a `center`
#: si faltan, así que no se exigen.
FORMAS_MINIMAS = (
    "center",
    "edge_n",
    "edge_e",
    "edge_s",
    "edge_w",
    "corner_nw",
    "corner_ne",
    "corner_sw",
    "corner_se",
)


def celdas_del_bloque(hoja, w, h, frames):
    """Las celdas del frame 0 de una hoja de `frames` bloques de `w`x`h`."""
    im = Image.open(hoja).convert("RGBA")
    esperado = (w * frames * CELL, h * CELL)
    if im.size != esperado:
        raise SystemExit(f"{hoja.name} mide {im.size}, se esperaba {esperado}")
    return im, [
        [(c, r, im.crop((c * CELL, r * CELL, (c + 1) * CELL, (r + 1) * CELL))) for c in range(w)]
        for r in range(h)
    ]


def leer_familia(nombre, basico, esquinas, w, h, frames):
    """`{forma: (Image hoja, col, row, w_bloque)}` — medido, no supuesto."""
    hb, celdas = celdas_del_bloque(TERRENOS / basico, w, h, frames)

    formas = {}
    alternas = {}

    def registrar(hoja, ancho_bloque, cuadricula):
        for fila in cuadricula:
            for c, r, tile in fila:
                f = forma_de(tile, azulado)
                if f is None:
                    continue
                if f not in formas:
                    formas[f] = (hoja, c, r, ancho_bloque)
                elif f not in alternas:
                    # El bloque de 4x4 trae DOS versiones de cada borde y cuatro
                    # del centro. La segunda es la variante `#2` de la familia:
                    # una orilla que no repite el mismo rompiente cada 32 px.
                    alternas[f] = (hoja, c, r, ancho_bloque)

    registrar(hb, w, celdas)
    ruta = TERRENOS / esquinas
    if ruta.exists():
        him, cel = celdas_del_bloque(ruta, 2, 2, frames)
        registrar(him, 2, cel)

    faltan = [f for f in FORMAS_MINIMAS if f not in formas]
    if faltan:
        raise SystemExit(f"{nombre}: la familia no cierra, faltan {faltan}")
    return formas, alternas


# ---------------------------------------------------------------------------
# El agua honda: un autotile estático dentro del bajío
# ---------------------------------------------------------------------------


def leer_honda():
    """`Deep_Water_1_*`: 22 recortes sueltos, cada uno una forma. Se mide igual.

    La paleta de referencia acá no es "el agua" sino **lo hondo**: el azul
    oscuro. El fondo de estos tiles es el bajío, que para este autotile hace de
    tierra.
    """
    fs = sorted(SINGLES.glob(f"{SINGLE_PREF}Deep_Water_1_*.png"), key=lambda p: int(p.stem.rsplit("_", 1)[1]))
    if not fs:
        raise SystemExit("faltan los recortes Deep_Water_1_* del pack")
    tiles = [(p, Image.open(p).convert("RGBA")) for p in fs]
    # El centro es el tile con menos colores y más oscuro: hondo puro.
    def oscuridad(t):
        px = list(t.getdata())
        return sum(p[0] + p[1] + p[2] for p in px) / len(px)

    centro = min(tiles, key=lambda pt: (len(paleta_de(pt[1])), oscuridad(pt[1])))[1]
    paleta = paleta_de(centro)
    formas = {}
    for p, t in tiles:
        f = forma_de(t, lambda px: px in paleta)
        if f and f not in formas:
            formas[f] = t
    if "center" not in formas:
        raise SystemExit("agua honda: no se encontró el tile de centro")
    return formas


# ---------------------------------------------------------------------------
# Cascadas y caño de descarga
# ---------------------------------------------------------------------------

#: `variante -> {pose: (archivo, ancho, alto)}`. La cascada del pack es MODULAR:
#: una boca arriba, tantos tramos de caída como haga falta y una poza abajo. Se
#: importa como tres poses del mismo slot para que una cascada de cuatro
#: escalones sea boca + 2 tramos + poza y no cuatro dibujos distintos.
CASCADAS = {
    "1": {
        "boca": ("Waterfall_Modular_Top_Part_32x32.png", 3, 1),
        "tramo": ("Waterfall_Modular_Middle_Part_32x32.png", 3, 1),
        "poza": ("Waterfall_Modular_Bottom_Part_32x32.png", 3, 2),
        "entera": ("Waterfall_32x32.png", 3, 3),
    },
    "2": {
        "boca": ("Waterfall_2_Modular_Top_Part_32x32.png", 3, 1),
        "entera": ("Waterfall_2_32x32.png", 3, 3),
    },
    "3": {
        "poza": ("Waterfall_3_Modular_Bottom_Part_32x32.png", 3, 2),
        "entera": ("Waterfall_3_32x32.png", 3, 3),
    },
}
#: **LA CASCADA MÁS ALTA QUE DIBUJÓ EL PACK**, de la que se recorta todo lo que
#: falta. Regla del usuario, textual: *"esa es la cascada más alta, de ahí varía
#: el tamaño hacia abajo, por eso todo tiene que poderse reutilizar fácil
#: modular"*.
#:
#: Cuatro filas: boca · caída · remate · poza. La caída (fila 1) es la única que
#: se puede REPETIR — es agua y nada más—, y por eso es la que da el `tramo`.
#:
#: # Por qué no sirve la de tres filas (`entera`)
#:
#: Porque ahí el remate y la caída están fundidos en una sola fila, y en la
#: cascada 3 esa fila **son las rocas donde revienta el chorro**. Recortando el
#: `tramo` de ahí —que es lo que se hacía— un salto de diez pisos salía con diez
#: filas de rocas cayendo por el aire dentro de un canal de agua. Es el bug que
#: reportó el usuario: *"la cascada repite assets erróneos"*.
CASCADAS_ALTAS = {
    "1": "Waterfall_3_Tiles_Height_32x32.png",
    "2": "Waterfall_2_3_Tiles_Height_32x32.png",
    "3": "Waterfall_3_3_Tiles_Height_32x32.png",
}

#: Qué fila de [`CASCADAS_ALTAS`] es cada pieza, y cuánto mide.
PIEZAS_DE_LA_ALTA = (("boca", 0, 1), ("tramo", 1, 1), ("poza", 2, 2))

FRAMES_CASCADA = 6

# ---------------------------------------------------------------------------
# **LA GARGANTA SE TIÑE DEL MATERIAL DE LA PARED QUE LA RODEA**
# ---------------------------------------------------------------------------
#
# El recorte del pack no es un chorro suelto: trae su garganta dibujada, dos
# columnas de roca TERRACOTA con estrías verticales. El kit de acantilado, en
# cambio, se tiñe con la losa que corona (`import-acantilado.py`) y sale un canto
# rodado gris malva. Puestos uno al lado del otro —que es literalmente cómo se
# dibujan, la garganta dentro del muro— el salto es una cicatriz naranja en una
# pared verde grisácea.
#
# Medido con `medir_cascadas::material_de_las_cascadas` sobre 1.800×1.800 tiles:
# de las 94 paredes que flanquean una cascada, **91 son de pasto** (`cliff/pasto`
# y `ledge/pasto`), 2 de sabana y 1 de arena. O sea: casi todas las cascadas del
# mundo caen justo donde el recorte peor pega.
#
# Se tiñe con la misma regla que el acantilado y sin elegir un color a ojo:
#
# - el CÉSPED del recorte se ancla en la losa del material (`mapa_por_losa`);
# - la ROCA se lleva a la paleta de la pared YA CONSTRUIDA de ese material, que
#   se lee del propio `cliff/<material>` del manifest — el arte que el usuario
#   asignó, no una rampa escrita a mano;
# - el AGUA no se toca: es la misma que la del río.
#
# Y se tiñe la HOJA ENTERA del pack antes de recortar, así que **los seis frames
# de cada pose salen teñidos**, que es la única forma de que la animación no
# parpadee de color.

#: Los nueve materiales que corta `import-acantilado.py`.
#:
#: **El color sale de la pared ya construida y de ningún otro sitio.** El primer
#: intento fue leer la losa de suelo (`tiles/grass`, `tiles/sand`…) como hace
#: `import-acantilado.py`, y tenía dos agujeros: la mitad de esas claves no
#: existen con ese nombre (`tiles/sand` no está curada) y, sobre todo, era una
#: SEGUNDA fuente para el mismo color. `cliff/<material>` ya trae las dos cosas
#: que hacen falta —su meseta arriba y su roca en el cuerpo—, ya está teñido con
#: la losa, y es literalmente el dibujo que va a quedar pegado a la garganta.
MATERIALES_CASCADA = [
    "pasto",
    "pasto_bosque",
    "pasto_seco",
    "pasto_pantano",
    "pasto_sierra",
    "arena",
    "nieve",
    "regolito",
    "basalto",
]

#: El verde DOMINANTE del recorte, que es el que va exactamente al color de la
#: losa; el resto de los verdes lo sigue por luminancia. Medido: los cuatro
#: verdes de la garganta son la rampa de pasto del pack, y (71,151,87) es el
#: mismo plano de 1.024 píxeles con el que está pintado `tiles/grass`.
VERDE_DOMINANTE = (71, 151, 87)

#: Un color es AGUA si el azul le gana claramente al rojo. Medido separando los
#: costados del recorte de su columna central, la paleta se parte sin
#: solapamiento: nueve azules y turquesas en el chorro, siete tierras cálidas en
#: la roca y los cuatro verdes de la orilla. El umbral no es delicado — el color
#: más ajustado de los dos lados se pasa por 30.
AZUL_MINIMO = 40


def luminancia(c):
    return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]


def paleta(im):
    """Los colores opacos de una imagen, sin repetir."""
    return {px[:3] for px in im.convert("RGBA").getdata() if px[3] > 0}


def dominante(im):
    """El color opaco más frecuente."""
    import collections

    cuenta = collections.Counter(px[:3] for px in im.convert("RGBA").getdata() if px[3] > 0)
    return cuenta.most_common(1)[0][0] if cuenta else None


def tenir(im, mapa):
    """Aplica `{color: color}` a la imagen entera, alfa incluido."""
    out = im.convert("RGBA").copy()
    px = out.load()
    for y in range(out.height):
        for x in range(out.width):
            r, g, b, a = px[x, y]
            if a == 0:
                continue
            nuevo = mapa.get((r, g, b))
            if nuevo:
                px[x, y] = (*nuevo, a)
    return out


def mapa_de_tinte(origen, destino):
    """`{color de origen: color de destino}`, repartido por LUMINANCIA.

    Misma regla que `import-acantilado.py`: se ordenan las dos paletas de oscuro
    a claro y se reparte proporcionalmente, así el degradado que le da volumen a
    la roca sobrevive al cambio de color en vez de aplastarse a un color plano.
    """
    orden = sorted(origen, key=luminancia)
    fin = sorted(destino, key=luminancia)
    if not orden or not fin:
        return {}
    mapa = {}
    for i, c in enumerate(orden):
        t = i / max(len(orden) - 1, 1)
        j = t * (len(fin) - 1)
        a, b = fin[int(j)], fin[min(int(j) + 1, len(fin) - 1)]
        f = j - int(j)
        mapa[c] = tuple(int(a[k] + (b[k] - a[k]) * f) for k in range(3))
    return mapa


def mapa_por_losa(origen, referencia, destino):
    """`{color del recorte: color teñido}` **anclado** en el tono dominante.

    El tono `referencia` va exactamente al color de la losa y el resto lo sigue
    por luminancia, así la parte plana queda indistinguible del pasto que tiene
    pegado y el filo y la sombra conservan su relieve.
    """
    base = max(luminancia(referencia), 1.0)
    return {
        c: tuple(min(255, max(0, int(destino[k] * luminancia(c) / base))) for k in range(3))
        for c in origen
    }


def _recorte_del_manifest(man, clave, pose_nombre=None):
    """La imagen de una pose ya publicada, o `None` si esa clave no tiene arte.

    Es la regla de "derivar el arte de la textura EN USO": el color al que se
    tiñe la garganta sale de la pared que el usuario ya tiene asignada, no de una
    rampa escrita a mano que se desincroniza en cuanto toca el editor.

    Acepta las dos formas del manifest: un slot con `poses` (el relieve, que
    tiene una por forma de autotile) y uno con un `rect` suelto (las losas de
    suelo, que son un dibujo y nada más).
    """
    entrada = man.get("sprites", {}).get(clave)
    if not entrada:
        return None
    p = entrada["poses"].get(pose_nombre) if pose_nombre else entrada
    if not p or "rect" not in p:
        return None
    r = p["rect"]
    gw, gh = entrada.get("grid", [CELL, CELL])
    ruta = ASSETS / entrada["png"]
    if not ruta.exists():
        return None
    im = Image.open(ruta).convert("RGBA")
    return im.crop(
        (r["col"] * gw, r["row"] * gh, (r["col"] + r["w"]) * gw, (r["row"] + r["h"]) * gh)
    )


def clasificar_garganta(hojas):
    """Parte la paleta del recorte en `(agua, roca, cesped)`, con su recuento.

    No se elige a ojo: el agua es lo que tira a azul ([`AZUL_MINIMO`]), el césped
    son los verdes y la roca es todo lo demás. Medido separando los costados del
    recorte de su columna central, las tres familias salen sin un solo color
    compartido. Se devuelve con el recuento porque el tinte del agua necesita
    saber cuál es su tono DOMINANTE, y se imprime para poder auditarlo.
    """
    import collections

    todos = collections.Counter()
    for im in hojas:
        todos.update(px[:3] for px in im.convert("RGBA").getdata() if px[3] > 0)
    agua = {c: n for c, n in todos.items() if c[2] >= c[0] + AZUL_MINIMO}
    cesped = {c: n for c, n in todos.items() if c not in agua and c[1] > c[0] and c[1] > c[2]}
    roca = {c: n for c, n in todos.items() if c not in agua and c not in cesped}
    return agua, roca, cesped


def color_del_rio():
    """El tono del agua quieta, medido del PROPIO recorte del pack.

    Se lee de la fuente y no del manifest a propósito: el manifest es lo que este
    script escribe, y leer la corrida anterior es cómo se arma una deriva que se
    nota a los tres imports.

    El pack pinta el centro de su agua de un color plano (medido: las 1.024
    celdas del centro de `Water_Tileset` son un solo tono), así que el dominante
    de la celda (1,1) del primer frame ES el color del río.
    """
    im = Image.open(TERRENOS / "Water_Tileset_32x32.png").convert("RGBA")
    return dominante(im.crop((CELL, CELL, 2 * CELL, 2 * CELL)))


def tinte_del_agua(agua):
    """Lleva el chorro al color del río **sin aplanarlo**.

    Medido sobre la captura del driver (`HA_AGUATEST`): el río es un plano de
    (60,163,178) y el chorro va de (42,112,155) a (90,204,195) con dominante
    (54,144,174). O sea que la cascada cae más turquesa y más clara que el agua
    de la que sale y en la que revienta — es la misma pregunta que el usuario hizo
    de la roca, aplicada al líquido.

    Se ancla igual que todo lo demás: el tono dominante del chorro va exactamente
    al del río y el resto lo sigue por luminancia. Así la espuma sigue siendo
    espuma y la veta oscura sigue siendo veta — lo que cambia es el tono sobre el
    que están, no el dibujo.
    """
    rio = color_del_rio()
    if not rio or not agua:
        return {}
    dom = max(agua, key=lambda c: agua[c]) if isinstance(agua, dict) else None
    return mapa_por_losa(agua, dom or rio, rio)


def clave_de_cascada(variante, material):
    """**La clave de una cascada: variante del pack × material de la pared.**

    Son dos ejes independientes y por eso van los dos en el nombre. La variante
    (1, 2, 3) es qué dibujo del pack le tocó al salto y la elige la posición
    (`agua::variante_cascada`); el material es de qué está hecha la pared que lo
    rodea y lo elige el bioma (`world2d::material_de_cascada`). El juego arma
    esta misma cadena — si acá cambia el formato, allá también.

    Las teñidas NO se declaran como variantes del objeto en el manifest: no son
    una elección del usuario en HA Studio, son la misma cascada vestida del color
    que le toca. Sólo las tres del pack salen en el menú del editor.
    """
    base = "agua/cascada" if variante == "1" else f"agua/cascada#{variante}"
    if material is None:
        return base
    return f"agua/cascada#{variante}_{material}"


def tinte_del_material(man, material, roca, cesped):
    """El `{color: color}` que lleva la garganta al material `material`.

    Las dos mitades salen del MISMO dibujo, `cliff/<material>`, que es el que va
    a quedar pegado a la garganta:

    - la **meseta** (la celda de arriba de `edge_s`, el canto de la terraza) da
      el color al que van los verdes del recorte;
    - el **cuerpo** (la fila de pared repetible, roca pura) da la paleta a la que
      van sus tierras cálidas.

    Devuelve `None` si esa pared no tiene arte: sin pared construida no hay a qué
    parecerse, y el recorte crudo del pack es mejor que un tinte inventado.
    """
    cara = _recorte_del_manifest(man, f"cliff/{material}", "edge_s")
    pared = _recorte_del_manifest(man, f"cliff/{material}", "cuerpo")
    if cara is None or pared is None:
        return None
    # La primera celda de la cara: la meseta, que es suelo y no roca.
    color_suelo = dominante(cara.crop((0, 0, cara.width, CELL)))
    paleta_pared = paleta(pared)
    if not color_suelo or not paleta_pared:
        return None
    mapa = mapa_por_losa(cesped, VERDE_DOMINANTE, color_suelo)
    mapa.update(mapa_de_tinte(roca, paleta_pared))
    return mapa

#: El caño de hormigón que descarga en el lago. No es decoración suelta: va
#: donde una obra civil toca el agua (ver `ha_shared::agua::desague_at`).
DESAGUE = ("Waterfall_Drain_Tube_32x32.png", 2, 4, 6)


# ---------------------------------------------------------------------------
# Lo que flota
# ---------------------------------------------------------------------------

#: `Props_Water_*` son TILES (traen el agua de fondo), no recortes con alfa. Si
#: se pegaran tal cual sobre el agua animada se vería el parche quieto del
#: fondo, así que se le saca el fondo midiendo: todo pixel del color del agua
#: del propio recorte se vuelve transparente y queda sólo el nenúfar.
FLOTANTES = {
    "agua/nenufar": [10, 11, 12, 13],
    "agua/mata_flotante": [14, 15, 16],
    "agua/hondura": [6, 7, 8, 9],
    "agua/onda": [1, 2, 3, 4, 5],
}


def recorte_flotante(n):
    p = SINGLES / f"{SINGLE_PREF}Props_Water_{n}.png"
    if not p.exists():
        raise SystemExit(f"falta {p.name}")
    im = Image.open(p).convert("RGBA")
    # El fondo es el color MÁS repetido del recorte: el agua lisa que lo rodea.
    conteo = {}
    for px in im.getdata():
        conteo[px] = conteo.get(px, 0) + 1
    fondo = max(conteo, key=conteo.get)
    if conteo[fondo] < im.width * im.height * 0.25:
        # No hay un fondo dominante: el recorte ya viene con alfa, no se toca.
        return im
    out = im.copy()
    px = out.load()
    for y in range(out.height):
        for x in range(out.width):
            if px[x, y] == fondo:
                px[x, y] = (0, 0, 0, 0)
    return out


# ---------------------------------------------------------------------------
# Armar la hoja
# ---------------------------------------------------------------------------


class Hoja:
    """Empaquetador por estantes: reserva tiras de celdas contiguas.

    Una tira de animación TIENE que quedar contigua y en la misma fila: la
    animación avanza el ancho del frame hacia la derecha y no sabe saltar de
    renglón.
    """

    def __init__(self):
        self.im = Image.new("RGBA", (HOJA_COLS * CELL, CELL), (0, 0, 0, 0))
        self.fila = 0
        self.col = 0
        self.alto_fila = 0

    def _crecer(self, filas):
        alto = (filas) * CELL
        if alto > self.im.height:
            nueva = Image.new("RGBA", (self.im.width, alto), (0, 0, 0, 0))
            nueva.paste(self.im, (0, 0))
            self.im = nueva

    def reservar(self, w, h):
        """Devuelve `(col, row)` de un hueco de `w`x`h` celdas."""
        if w > HOJA_COLS:
            raise SystemExit(f"una tira de {w} celdas no entra en una hoja de {HOJA_COLS}")
        if self.col + w > HOJA_COLS:
            self.fila += self.alto_fila
            self.col = 0
            self.alto_fila = 0
        self.alto_fila = max(self.alto_fila, h)
        self._crecer(self.fila + self.alto_fila)
        c, r = self.col, self.fila
        self.col += w
        return c, r

    def pegar(self, im, col, row):
        self.im.paste(im, (col * CELL, row * CELL))


def tira_animada(hoja, pagina, col_src, row_src, ancho_bloque, w, h, frames):
    """Copia los `frames` de una celda del pack a una tira contigua de la hoja.

    En el pack los frames son bloques enteros uno al lado del otro, así que el
    frame *f* de la celda `(c,r)` vive en `(c + f*ancho_bloque, r)`. Acá quedan
    pegados: `(C, R), (C+w, R), …` — que es lo único que sabe leer una tira.
    """
    c0, r0 = pagina.reservar(w * frames, h)
    for f in range(frames):
        src = hoja.crop(
            (
                (col_src + f * ancho_bloque) * CELL,
                row_src * CELL,
                (col_src + f * ancho_bloque + w) * CELL,
                (row_src + h) * CELL,
            )
        )
        pagina.pegar(src, c0 + f * w, r0)
    return c0, r0


def pose(col, row, w=1, h=1, frames=None, fps=FPS_AGUA, capa=None):
    p = {"rect": {"col": col, "row": row, "w": w, "h": h}}
    if frames:
        p["anim"] = {"frames": frames, "fps": fps}
        p["dir"] = "Right"
    if capa:
        p["layer"] = capa
    return p


def main():
    seco = "--dry-run" in sys.argv
    pagina = Hoja()
    sprites = {}

    # --- las tres familias de orilla -------------------------------------
    for slot, (basico, esquinas, w, h, frames) in FAMILIAS.items():
        formas, alternas = leer_familia(slot, basico, esquinas, w, h, frames)
        for clave, juego in (("", formas), ("#2", alternas)):
            if not juego:
                continue
            poses = {}
            for nombre, (hoja, c, r, ancho) in sorted(juego.items()):
                col, row = tira_animada(hoja, pagina, c, r, ancho, 1, 1, frames)
                poses[nombre] = pose(col, row, frames=frames, capa="Floor")
            if clave == "#2":
                # La variante alterna sólo trae los bordes repetidos del pack;
                # lo que le falte lo completa la base (`tile_sprite` cae a
                # `center`, y `variant_key_seeded` sólo elige variantes declaradas).
                for nombre in FORMAS_MINIMAS:
                    if nombre in poses:
                        continue
                    hoja, c, r, ancho = formas[nombre]
                    col, row = tira_animada(hoja, pagina, c, r, ancho, 1, 1, frames)
                    poses[nombre] = pose(col, row, frames=frames, capa="Floor")
            sprites[slot + clave] = {
                "png": PAGINA_REL,
                "grid": [CELL, CELL],
                "poses": poses,
            }
        print(f"{slot}: {len(formas)} formas" + (f" + variante de {len(alternas)}" if alternas else ""))

    # --- el agua honda ----------------------------------------------------
    honda = leer_honda()
    poses = {}
    for nombre, tile in sorted(honda.items()):
        col, row = pagina.reservar(1, 1)
        pagina.pegar(tile, col, row)
        poses[nombre] = pose(col, row, capa="Floor")
    sprites["tiles/water_deep"] = {"png": PAGINA_REL, "grid": [CELL, CELL], "poses": poses}
    print(f"tiles/water_deep: {len(poses)} formas")

    # --- cascadas ---------------------------------------------------------
    # El manifest tal como está AHORA: de ahí sale el color al que se tiñe cada
    # garganta (la losa de suelo y la pared de acantilado que el usuario ya tiene
    # asignadas). Se lee aparte del que se escribe al final, que todavía no
    # existe cuando hace falta.
    man_previo = json.loads(MANIFEST.read_text()) if MANIFEST.exists() else {}
    # Las hojas del pack, sin recortar: la clasificación de la paleta es de la
    # cascada entera y tiene que ser la misma para las tres variantes.
    hojas_cascada = {}
    for variante, partes in CASCADAS.items():
        for nombre, (archivo, w, h) in partes.items():
            ruta = ANIMADOS / archivo
            if not ruta.exists():
                raise SystemExit(f"falta la cascada {archivo}")
            im = Image.open(ruta).convert("RGBA")
            if im.size != (w * FRAMES_CASCADA * CELL, h * CELL):
                raise SystemExit(f"{archivo} mide {im.size}, se esperaban {FRAMES_CASCADA} frames de {w}x{h}")
            hojas_cascada[(variante, nombre)] = im
    # Y la cascada más alta de cada variante, que es de donde salen las piezas
    # que el pack no publicó sueltas.
    for variante, archivo in CASCADAS_ALTAS.items():
        ruta = ANIMADOS / archivo
        if not ruta.exists():
            raise SystemExit(f"falta la cascada alta {archivo}")
        im = Image.open(ruta).convert("RGBA")
        if im.size != (3 * FRAMES_CASCADA * CELL, 4 * CELL):
            raise SystemExit(f"{archivo} mide {im.size}, se esperaban {FRAMES_CASCADA} frames de 3x4")
        hojas_cascada[(variante, "alta")] = im
    agua_pal, roca_pal, cesped_pal = clasificar_garganta(hojas_cascada.values())
    print(
        f"garganta: {len(agua_pal)} tonos de agua, "
        f"{len(roca_pal)} de roca, {len(cesped_pal)} de césped"
    )
    # **El agua del chorro se lleva al azul del río SIEMPRE**, tenga o no material
    # su pared: es el mismo líquido y sale del mismo cauce. Va en el respaldo
    # también, así que `None` ya no es el recorte crudo — es el recorte con su
    # agua en su sitio.
    mapa_agua = tinte_del_agua(agua_pal)
    print(f"  el chorro se lleva al azul del río: {color_del_rio()}")
    tintes = {None: dict(mapa_agua)}
    for material in MATERIALES_CASCADA:
        mapa = tinte_del_material(man_previo, material, roca_pal, cesped_pal)
        if mapa:
            tintes[material] = {**mapa_agua, **mapa}
        else:
            print(f"  (sin arte de cliff/{material}: esa cascada no se tiñe de roca)")

    for variante, partes in CASCADAS.items():
        for material, mapa in tintes.items():
            poses = {}
            for nombre, (archivo, w, h) in partes.items():
                # La cascada entera de 3x3 es la vista previa del editor y el
                # juego no la pide nunca (arma boca + tramos + poza). Guardar una
                # copia teñida por material serían 1.458 celdas de hoja para algo
                # que no se dibuja: las teñidas se quedan con las piezas.
                if nombre == "entera" and material is not None:
                    continue
                # **La hoja ENTERA se tiñe antes de recortar**, así que los seis
                # frames de la tira salen del mismo tinte. Teñir sólo el primero
                # era el bug obvio: la animación parpadearía de color.
                im = hojas_cascada[(variante, nombre)]
                if mapa:
                    im = tenir(im, mapa)
                col, row = tira_animada(im, pagina, 0, 0, w, w, h, FRAMES_CASCADA)
                poses[nombre] = pose(col, row, w=w, h=h, frames=FRAMES_CASCADA, fps=FPS_CASCADA)
            # **Las tres variantes tienen que ser modulares, no sólo la primera.**
            # El pack sólo publicó las piezas sueltas de la cascada 1; las otras
            # dos hay que recortarlas de [`CASCADAS_ALTAS`], que es el mismo
            # dibujo apilado a su altura máxima (biblia: el asset se usa como está
            # dibujado, y lo que falta se deriva de lo que ya se usa).
            im = hojas_cascada[(variante, "alta")]
            if mapa:
                im = tenir(im, mapa)
            for nombre, fila, alto in PIEZAS_DE_LA_ALTA:
                if nombre in poses:
                    continue
                col, row = tira_animada(im, pagina, 0, fila, 3, 3, alto, FRAMES_CASCADA)
                poses[nombre] = pose(col, row, w=3, h=alto, frames=FRAMES_CASCADA, fps=FPS_CASCADA)
            clave = clave_de_cascada(variante, material)
            sprites[clave] = {"png": PAGINA_REL, "grid": [CELL, CELL], "poses": poses}
        print(f"agua/cascada variante {variante}: {len(tintes)} materiales")

    archivo, w, h, frames = DESAGUE
    im = Image.open(ANIMADOS / archivo).convert("RGBA")
    if im.size != (w * frames * CELL, h * CELL):
        raise SystemExit(f"{archivo} mide {im.size}, se esperaban {frames} frames de {w}x{h}")
    col, row = tira_animada(im, pagina, 0, 0, w, w, h, frames)
    sprites["agua/desague"] = {
        "png": PAGINA_REL,
        "grid": [CELL, CELL],
        "strip": {"col": col, "row": row, "w": w, "h": h},
        "strip_dir": "Right",
        "anim": {"frames": frames, "fps": FPS_CASCADA},
    }
    print(f"agua/desague: {frames} frames de {w}x{h}")

    # --- lo que flota -----------------------------------------------------
    variantes = {}
    for slot, ns in FLOTANTES.items():
        for i, n in enumerate(ns, start=1):
            im = recorte_flotante(n)
            col, row = pagina.reservar(1, 1)
            pagina.pegar(im, col, row)
            clave = slot if i == 1 else f"{slot}#{i}"
            sprites[clave] = {
                "png": PAGINA_REL,
                "grid": [CELL, CELL],
                "rect": {"col": col, "row": row, "w": 1, "h": 1},
            }
            if i > 1:
                variantes.setdefault(slot, []).append(str(i))
        print(f"{slot}: {len(ns)} recortes")

    if seco:
        print(f"\n[dry-run] hoja {pagina.im.size[0]}x{pagina.im.size[1]} px, {len(sprites)} claves")
        return

    (ASSETS / "art/pack").mkdir(parents=True, exist_ok=True)
    pagina.im.save(ASSETS / PAGINA_REL, optimize=True)
    print(f"\n{PAGINA_REL}: {pagina.im.size[0]}x{pagina.im.size[1]} px")

    man = json.loads(MANIFEST.read_text())
    hoja_sprites = man.setdefault("sprites", {})
    objetos = man.setdefault("objects", {})
    if INDEX.exists():
        viejo = json.loads(INDEX.read_text())
        for k in viejo.get("sprites", []):
            hoja_sprites.pop(k, None)
    hoja_sprites.update(sprites)

    # Las variantes se DECLARAN en el objeto (es lo que lee el editor y lo que
    # hace que `variant_key_seeded` las sortee). Sin esto el arte existe pero
    # nadie lo pide nunca.
    etiquetas = {
        "agua/nenufar": "Nenúfar",
        "agua/mata_flotante": "Mata flotante",
        "agua/hondura": "Hondura",
        "agua/onda": "Onda",
        "agua/cascada": "Cascada",
        "tiles/water": "Agua",
        "tiles/water_arena": "Agua (orilla de arena)",
        "tiles/water_roca": "Agua (orilla pelada)",
    }
    for slot, ids in list(variantes.items()) + [("agua/cascada", ["2", "3"]), ("tiles/water", ["2"]),
                                                ("tiles/water_arena", ["2"]), ("tiles/water_roca", ["2"])]:
        ids = [i for i in ids if (slot + "#" + i) in hoja_sprites]
        if not ids:
            continue
        obj = objetos.setdefault(slot, {"label": etiquetas.get(slot, slot)})
        obj.setdefault("label", etiquetas.get(slot, slot))
        vs = obj.setdefault("variants", [])
        tiene = {v["id"] for v in vs}
        for i in ids:
            if i not in tiene:
                vs.append({"id": i, "label": f"{etiquetas.get(slot, slot)} {i}"})

    MANIFEST.write_text(json.dumps(man, indent=1, ensure_ascii=False, sort_keys=True))
    INDEX.write_text(json.dumps({"sprites": sorted(sprites), "pagina": PAGINA_REL}, indent=1, ensure_ascii=False))
    print(f"manifest: {len(sprites)} claves de agua")


if __name__ == "__main__":
    main()
