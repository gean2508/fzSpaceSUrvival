#!/usr/bin/env python3
"""**LAS CERCAS Y LOS MURETES, CON SU ORIENTACIÓN MEDIDA.**

    python3 scripts/import-cercas.py
    python3 scripts/import-cercas.py --dry-run
    python3 scripts/import-cercas.py --informe    # sólo la clasificación, sin escribir

## Por qué existe

`Tile::Fence` se dibujaba con **una sola pieza para los cuatro lados**
(`world2d.rs`: `Tile::Fence => "fence"`), y el respaldo por código
(`procedural_art::fence`) pinta travesaños **siempre horizontales**. O sea que el
lado este y el lado oeste de la parcela —que son tramos VERTICALES— salían con el
dibujo de un tramo horizontal. El pedido fue literal: *"tenemos vallas muros
verticales, no pongas horizontales en vertical, se ve horrible"*.

El pack sí trae el juego completo, en `1_Terrains_and_Fences`:

| familia | piezas | qué es |
|---|---|---|
| `Fence_1` | 22 (1×1) | reja metálica clara |
| `Fence_2` | 16 (1×2 y 1×3) | cerca alta |
| `Fence_3` | 16 (1×2 y 1×3) | cerca alta, otro material |
| `Wall_1` | 20 (1×1) | **murete de arenisca con capitel** — el de la referencia del claro |
| `Wall_2` | 19 (1×1, 1×2, 1×4) | muro macizo |
| `Wall_3` | 19 | muro macizo, otro material |
| `Grass_Fenced_1` | 22 (1×1) | pasto ya cercado |

## Cómo se clasifica — tabla leída del pack, VERIFICADA por medida

Mismo trato que `import-suelo-bordes.py`: una tabla por layout y un `verificar()`
que la vuelve a comprobar en cada corrida, así que si el pack cambia el script se
planta acá y no seis meses después con las parcelas llenas de cercas al revés.

**Por qué no clasifica solo.** Se probaron dos medidas automáticas y las dos
fallan por el mismo motivo, que conviene dejar escrito:

1. *Empalme de costura* (¿el borde este de la pieza continúa en su propio borde
   oeste?). Sirve para los muros macizos y **no** para una cerca de estacas: las
   estacas no caen en el borde de celda, así que un tramo horizontal perfectamente
   repetible no empalma consigo mismo. Y con los bordes vacíos da falso positivo —
   dos vacíos "coinciden" siempre—, que es lo que hacía pasar los cuatro postes
   sueltos de `Fence_1` por tramos horizontales.
2. *Componente conexa y por qué bordes sale* (la que quedó, como verificación).
   En vista 3/4 **la base del sprite toca el borde sur por su propia sombra**, así
   que un tramo horizontal puro mide `esw` en vez de `ew`. Como medida absoluta
   miente; como verificación es perfecta.

Así que la tabla dice la forma y la medida comprueba lo que sí es fiable: **toda
letra declarada tiene que aparecer en la medida**. Un `ns` cuya pieza no toque el
norte o el sur no es un tramo vertical, y el script lo caza. Lo que la medida
agregue de más (la `s` de la sombra, el muñón de rail al este) se perdona, porque
es ruido conocido de la proyección.

## Qué produce

`art/pack/cercas.png` y una clave por familia con **una pose por forma**:

    fence/madera      poses: ns · ew · ne · nw · se · sw · nes · … · nesw
    fence/alambre
    fence/piedra      (murete bajo)
    fence/seto
    fence/muro_bajo

Los nombres de forma son los de `world2d::forma_de_cerca`, que es el que el juego
pide. **No son los de `autotile_shape`**: aquél describe el borde de un ÁREA
(`edge_n`, `inner_ne`) y una cerca es una LÍNEA — lo que importa es hacia dónde
sigue, no de qué lado queda el relleno.

Las formas que el pack dibujó más de una vez entran como variantes (`#2`, `#3`),
así que un tramo largo no repite la misma tabla cada celda.
"""

import argparse
import glob
import json
import os
import re

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
PAGINA_REL = "art/pack/cercas.png"
INDEX = os.path.join(ROOT, "scripts/cercas-import-index.json")

CELL = 32
#: Ancho de la página en celdas. Igual que el resto de las hojas curadas: chico a
#: propósito, que el Redmi paga por página cargada (§10).
PAGINA_COLS = 16

#: **Qué familia del pack llena cada slot del juego.**
#:
#: El slot es lo que el mundo pide (`fence/madera`); la familia es lo que el
#: artista dibujó. Los tres primeros estaban declarados en `slots.rs` y **sin un
#: pixel asignado** desde siempre: por eso la parcela caía al respaldo por código.
#:
#: `Wall_2` y `Wall_3` quedan afuera a propósito: no son cercas de parcela sino
#: **muro macizo** (sus 15 piezas de 1×1 miden `nesw`, o sea relleno, y las de 1×4
#: son paños enteros). Van con los edificios, no acá.
#: `Grass_Fenced_1` también: es un autotile de ÁREA —pasto ya cercado— y se pide
#: con `autotile_shape`, no con la forma de línea de este importador.
SLOTS = [
    ("fence/madera", "Fence_2", "Cerca de madera"),
    ("fence/reja", "Fence_3", "Reja"),
    ("fence/alambre", "Fence_1", "Cerca de alambre"),
    ("fence/piedra", "Wall_1", "Muro bajo de piedra"),
]

#: **La forma de cada pieza, por layout del pack.**
#:
#: Los nombres son máscaras de conexión: hacia dónde SIGUE la cerca. `ns` es el
#: tramo vertical, `ew` el horizontal, y los de dos letras perpendiculares son
#: codos (`es` = sigue al este y al sur, o sea la esquina noroeste de un cerco).
#:
#: **Los dos tramos verticales no son intercambiables.** En `Fence_2`/`Fence_3` la
#: pieza 4 lleva los postes contra el borde IZQUIERDO y la 8 contra el DERECHO:
#: son el lado oeste y el lado este de un cerco. Por eso se publican como
#: `ns@w` / `ns@e` y el juego elige según de qué lado le queda el interior de la
#: parcela — un poste dibujado del lado de afuera se ve mal aunque la orientación
#: del tramo sea la correcta. Lo mismo con `ew@n` / `ew@s`.
LAYOUT_CERCA_16 = {
    1: "es", 2: "ew@s", 3: "sw", 4: "ns@w", 5: "ne", 6: "ew@s", 7: "nw", 8: "ns@e",
    9: "s", 10: "s", 11: "ns@e", 12: "ns@w",
    # 13..16 son las diagonales de 1×3 (la cerca que baja un talud). Todavía no
    # hay quién las pida: el relieve no traza cercas en pendiente. Se dejan sin
    # declarar en vez de inventarles un uso.
}
#: `Fence_1` es una reja metálica de 1×1, y es la familia donde la medida sí
#: alcanzó sola: al ser una reja calada, la componente conexa sale limpia por los
#: bordes que de verdad conectan. La tabla es literalmente lo que midió.
#:
#: Ojo con dos cosas que ya costaron una vuelta acá:
#: - **su tramo horizontal mide `new`**, no `ew`: las estacas llegan al borde de
#:   arriba del recorte. Se declara `ew` porque es lo que conecta; la `n` de más
#:   la perdona `verificar`.
#: - **las piezas 1, 3, 11 y 12 son TES, no codos.** Se leyeron como tramos
#:   horizontales de la hoja de contacto y la verificación las rebotó (`nes` y
#:   `nsw`). Usar una te de codo deja un muñón de reja apuntando a la nada.
LAYOUT_REJA_22 = {
    4: "ns@w", 5: "ns@w", 9: "ns@e", 10: "ns@e",
    2: "ew@s", 7: "ew@s",
    8: "ne", 14: "ne", 18: "ne", 21: "ne",
    6: "nw", 13: "nw", 15: "nw", 20: "nw",
    16: "es", 19: "es",
    17: "sw", 22: "sw",
    1: "nes", 11: "nes",
    3: "nsw", 12: "nsw",
}
#: `Wall_1`: murete de arenisca con capitel. Mismo esqueleto que la cerca de 16,
#: más las ocho diagonales (13..20) que son el zigzag de parapeto del pack.
LAYOUT_MURETE_20 = {
    1: "es", 2: "ew@s", 3: "sw", 4: "ns@w", 5: "ne", 6: "ew@s", 7: "nw", 8: "ns@e",
    9: "es", 10: "sw", 11: "ns@e", 12: "ne",
    # 13..20: diagonales a 45°. Mismo motivo que arriba — sin quién las pida.
}

LAYOUTS = {
    "Fence_2": LAYOUT_CERCA_16,
    "Fence_3": LAYOUT_CERCA_16,
    "Fence_1": LAYOUT_REJA_22,
    "Wall_1": LAYOUT_MURETE_20,
}

#: Tolerancia de color al comparar dos bordes que tienen que continuar el mismo
#: dibujo. 48 sobre la suma de los tres canales: aguanta el grano del pixel art y
#: no deja pasar dos materiales distintos.
TOL_COLOR = 48
#: Fracción de píxeles del borde que pueden discrepar y aun así considerarse la
#: misma continuación. El pack dibuja sombras que rompen una o dos filas.
TOL_DISCREPA = 0.12
#: Fracción mínima del borde que tiene que tener tinta para que cuente como
#: conexión. Por debajo es una esquinita suelta, no un empalme.
MIN_TINTA = 0.15


def familia(nombre):
    """`{índice: Image}` de una familia del pack, por el número del archivo."""
    patron = re.compile(rf"_{nombre}_(\d+)\.png$")
    out = {}
    for f in glob.glob(os.path.join(TERRENOS, f"*_{nombre}_*.png")):
        m = patron.search(f)
        if m:
            out[int(m.group(1))] = Image.open(f).convert("RGBA")
    return out


def borde(im, lado):
    """La franja de un borde, como lista de píxeles."""
    if lado == "n":
        return [im.getpixel((x, 0)) for x in range(im.width)]
    if lado == "s":
        return [im.getpixel((x, im.height - 1)) for x in range(im.width)]
    if lado == "w":
        return [im.getpixel((0, y)) for y in range(im.height)]
    return [im.getpixel((im.width - 1, y)) for y in range(im.height)]


def salidas(im):
    """**Por qué bordes SALE la línea de tinta de este recorte.**

    Se toma la componente conexa más grande de píxeles opacos y se mira qué
    bordes toca. Es la medida que verifica la tabla: una pieza declarada `ns`
    tiene que tocar el norte y el sur, se la mire como se la mire.

    Devuelve una cadena con las letras de `nesw` que toca.
    """
    w, h = im.size
    px = im.load()
    opaco = [[px[x, y][3] > 16 for x in range(w)] for y in range(h)]
    visto = [[False] * w for _ in range(h)]
    mejor = ("", 0)
    for y0 in range(h):
        for x0 in range(w):
            if not opaco[y0][x0] or visto[y0][x0]:
                continue
            pila = [(x0, y0)]
            visto[y0][x0] = True
            n = 0
            toca = {c: 0 for c in "nesw"}
            while pila:
                x, y = pila.pop()
                n += 1
                if y == 0:
                    toca["n"] += 1
                if y == h - 1:
                    toca["s"] += 1
                if x == 0:
                    toca["w"] += 1
                if x == w - 1:
                    toca["e"] += 1
                for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                    a, b = x + dx, y + dy
                    if 0 <= a < w and 0 <= b < h and opaco[b][a] and not visto[b][a]:
                        visto[b][a] = True
                        pila.append((a, b))
            if n > mejor[1]:
                largo = {"n": w, "s": w, "e": h, "w": h}
                mejor = ("".join(c for c in "nesw" if toca[c] > largo[c] * MIN_TINTA), n)
    return mejor[0]


def conexiones(forma):
    """Las letras de conexión de una forma: `ns@w` → `ns`, `ew@s` → `ew`."""
    return forma.split("@")[0]


def verificar(nombre, kit, layout):
    """La tabla dice la verdad, o el script se planta.

    Se comprueba lo que la proyección 3/4 **no** puede falsear: toda letra
    declarada tiene que aparecer en la medida. Lo que la medida agregue de más se
    perdona — la base del sprite toca el sur por su sombra y el muñón de rail toca
    los costados, y eso es ruido conocido, no una conexión.
    """
    fallos = []
    for idx, forma in sorted(layout.items()):
        if idx not in kit:
            fallos.append(f"falta el recorte {nombre}_{idx}")
            continue
        medido = salidas(kit[idx])
        faltan = [c for c in conexiones(forma) if c not in medido]
        if faltan:
            fallos.append(
                f"{nombre}_{idx} está declarada `{forma}` pero no sale por "
                f"{'/'.join(faltan)} (mide `{medido or 'nada'}`)"
            )
    return fallos


class Hoja:
    """Empaquetador por celdas, en filas de [`PAGINA_COLS`].

    Las piezas de una cerca alta miden 1×2 o 1×3 celdas, así que la hoja reserva
    el alto de la más grande por fila. Es lo mismo que hace `import-acantilado`
    con las caras de distinto talle.
    """

    def __init__(self):
        self.filas = [[]]
        self.alto = [0]

    def poner(self, tile):
        w, h = tile.width // CELL, tile.height // CELL
        fila = self.filas[-1]
        usado = sum(t.width // CELL for t in fila)
        if usado + w > PAGINA_COLS:
            self.filas.append([])
            self.alto.append(0)
            fila = self.filas[-1]
            usado = 0
        fila.append(tile)
        self.alto[-1] = max(self.alto[-1], h)
        row = sum(self.alto[:-1])
        return usado, row, w, h

    def imagen(self):
        alto = sum(self.alto) or 1
        im = Image.new("RGBA", (PAGINA_COLS * CELL, alto * CELL), (0, 0, 0, 0))
        y = 0
        for fila, h in zip(self.filas, self.alto):
            x = 0
            for t in fila:
                im.paste(t, (x * CELL, y * CELL))
                x += t.width // CELL
            y += h
        return im


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--informe", action="store_true", help="sólo imprimir la clasificación")
    args = ap.parse_args()

    pagina = Hoja()
    sprites = {}
    objetos = {}
    problemas = []

    for clave, fam, etiqueta in SLOTS:
        kit = familia(fam)
        if not kit:
            problemas.append(f"{clave}: no está la familia {fam} en el pack")
            continue
        layout = LAYOUTS[fam]
        fallos = verificar(fam, kit, layout)
        if fallos:
            problemas += [f"{clave}: {f}" for f in fallos]
            continue

        # Una pose por forma; las repetidas van de variante para que un tramo
        # largo no repita la misma tabla cada celda.
        por_forma = {}
        for i in sorted(layout):
            por_forma.setdefault(layout[i], []).append(i)
        print(f"{clave}  ({fam}, {len(kit)} piezas, {len(layout)} clasificadas)")
        poses = {}
        for forma, idxs in sorted(por_forma.items()):
            for n, i in enumerate(idxs):
                col, row, w, h = pagina.poner(kit[i])
                nombre = forma if n == 0 else f"{forma}#{n + 1}"
                poses[nombre] = {"rect": {"col": col, "row": row, "w": w, "h": h}}
            print(f"    {forma:6} ← piezas {idxs}")

        # **EL TRAMO PELADO TIENE QUE EXISTIR.**
        #
        # `cercas::cadena` pide `ew@n` → `ew` → `solo`, y el pack no dibuja el
        # lado norte de una cerca horizontal: en 3/4 la cerca enseña siempre la
        # misma cara, así que hay UNA pieza y vale para los dos lados. Sin el
        # alias, el lado norte de cada parcela agotaba la cadena, caía a la clave
        # base y volvía a dibujar la pieza equivocada — o sea el bug entero,
        # sobreviviendo en la mitad del perímetro. Lo mismo con `ns` cuando una
        # familia trae un solo costado.
        for eje in ("ns", "ew"):
            if eje in poses:
                continue
            lado = next((p for p in sorted(poses) if p.startswith(f"{eje}@") and "#" not in p), None)
            if lado:
                poses[eje] = dict(poses[lado])
                print(f"    {eje:6} ← alias de {lado} (el pack dibuja un solo costado)")
        sprites[clave] = {"png": PAGINA_REL, "grid": [CELL, CELL], "poses": poses}
        objetos[clave] = etiqueta

    if problemas:
        for p in problemas:
            print(f"⚠ {p}")
    if not sprites:
        raise SystemExit("ninguna familia clasificó: no se escribe nada")

    im = pagina.imagen()
    if args.informe:
        return
    if args.dry_run:
        print(f"\n[dry-run] hoja {im.size[0]}x{im.size[1]} px, {len(sprites)} claves")
        return

    os.makedirs(os.path.join(ASSETS, "art/pack"), exist_ok=True)
    im.save(os.path.join(ASSETS, PAGINA_REL), optimize=True)
    print(f"\n{PAGINA_REL}: {im.size[0]}x{im.size[1]} px")

    man = json.load(open(MANIFEST))
    hoja = man.setdefault("sprites", {})
    # Idempotencia: se borra lo de la corrida anterior antes de escribir lo nuevo,
    # o el manifest acumula claves muertas de familias que ya no clasifican.
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
    print(f"manifest: {len(sprites)} cercas con orientación")


if __name__ == "__main__":
    main()
