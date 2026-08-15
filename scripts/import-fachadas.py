#!/usr/bin/env python3
"""**La cara de AFUERA de una casa, con el pixel de Modern Exteriors.**

    python3 scripts/import-fachadas.py --dry-run --preview /tmp/f.png
    python3 scripts/import-fachadas.py

## El problema que resuelve

Los dos packs dibujan un edificio en proyecciones distintas:

- **Modern Exteriors** lo dibuja como **elevación frontal** — ves el frente de la
  casa (paño, ventana, puerta, cornisa) y el techo desde arriba. No hay costados:
  sólo el frente.
- **Modern Interiors** (`Room_Builder_*`) dibuja el **interior en corte cenital**:
  paredes finas vistas desde adentro.

`import-construccion.py` trajo las segundas, así que hasta hoy la construcción por
bordes estaba pintada ENTERA con arte de interior: una casa que levanta el jugador,
mirada desde la vereda, era un interior dado vuelta. El pedido fue literal —
*"tendríamos que construir sólo el frente, y de ahí con el modern interior hacer el
interior"*.

El mecanismo ya existía: `walls::Cara` elige una POSE por cara. Lo que faltaba era
el pixel de la cara de afuera (`back`), que es lo que escribe este script.

## Qué elige, y cómo

**Midiendo, no a ojo**, y **sin recortar composiciones**. Los frentes de local del
pack (`planta_baja_*` de 7 celdas) son dibujos ENTEROS: rebanarlos cada 32 px da
media vidriera, medio maniquí, medio cajero. Lo repetible del pack son las columnas
que el propio artista marcó `_modular` (1 celda de ancho) y las plantas modulares
de 7×4 — el sistema de edificios apilables de LimeZu.

Así que cada material de fachada sale de **un paño y su juego de piezas**:

1. **PUERTA de la calle** — una columna `_modular` de planta baja cuyo hueco llega
   al piso. `local` la busca vidriada (vidriera de comercio), `portal` en la
   familia condominio (entrada de vivienda). Es la pieza que le da IDENTIDAD al
   material, así que es la que se elige primero.
2. **PAÑO** — de todas las columnas de la familia modular, la más UNIFORME (repite
   filas, poca variedad de color, bajo contraste) cuyo color medio empareja con el
   de la puerta. Es el mismo emparejado por color que usa `vano_de_la_pared` en
   `import-construccion.py`: no "un paño de ladrillo", sino **el MISMO ladrillo que
   tu puerta**.
3. **VENTANA** — la columna `piso_modular_medio` del mismo paño. La dibujó el
   artista para repetirse; no hace falta componer nada.
4. **VANO** — una columna `_modular` con hueco al piso y OSCURO (se ve el adentro,
   no hay hoja). Si el paño no tiene ninguna, el slot queda sin arte y el juego
   dibuja el marco por código (`walls::marco_de_vano`), que es el flujo de siempre.

La puerta y el vano se **componen sobre el paño** —igual que `import-construccion.py`
monta la hoja de madera sobre el muro de piedra para `door/stone`— así las cuatro
piezas miden lo mismo y rematan a la misma altura. Sin eso, poner una puerta en
medio de un muro dejaba un escalón.

## Qué escribe

- `art/pack/fachadas.png` — UNA página de 32 px por celda. No se referencian los
  rects de las 7 páginas `floor_modular_building_*` a propósito: el Redmi es
  sprite-bound y 7 texturas son hasta 7 lotes de dibujo (biblia §10). Una página,
  un lote.
- Las entradas del manifest de los ocho slots `*/fachada_local` y `*/fachada_portal`,
  con las CINCO poses. La cara de afuera (`back`) es la elevación; `front` (tabique)
  y `center` siguen siendo el paño de interior que ya eligió `import-construccion.py`,
  porque adentro de la casa se sigue viendo Modern Interiors. Los costados
  (`left`/`right`) son una tira fina del paño de afuera: desde la calle, el lado de
  una casa es ladrillo, no empapelado.
- `scripts/fachadas-import-index.json` — lo que escribió, para que una segunda
  corrida pise **lo suyo** y nada más (mismo contrato que `import-construccion.py`).

No se genera ni un pixel nuevo: todo sale de recortes de las páginas que ya están
en el manifest. El usuario puede recortar cualquiera de estas poses a mano en HA
Studio y este script no le pisa nada que no haya escrito él mismo.
"""

import argparse
import json
import os
import statistics

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ASSETS = os.path.join(ROOT, "native/crates/ha-game/assets")
MANIFEST = os.path.join(ASSETS, "art/manifest.json")
INDEX = os.path.join(ROOT, "scripts/fachadas-import-index.json")
PAGINA_REL = "art/pack/fachadas.png"
CELL = 32

#: Alto de una pieza de fachada, en celdas. Es el de las plantas modulares del
#: pack (`piso_medio` es 7×4), y en tiles son 4 m: más del doble que el jugador,
#: que es lo que hace que desde la vereda se lea EDIFICIO y no tapia. Las cuatro
#: piezas del material miden esto, o un muro con una puerta en el medio queda con
#: un escalón.
ALTO = 4

#: La familia del pack de la que sale todo. Es el sistema de edificios apilables
#: de LimeZu: planta baja + N plantas medias + remate.
FAM = "building/edificios_modulares_"

#: **Los dos materiales, definidos por SU PUERTA.** Lo que hace que una fachada se
#: lea como comercio o como vivienda es la entrada, no el paño: el mismo ladrillo
#: rojo es una panadería o un edificio de departamentos según lo que tenga abajo.
#:
#: Es la única decisión del script que no sale de una medida —igual que
#: `TEMA_DEL_LOTE` en `import-edificios.py`— y por eso está escrita acá y no
#: escondida en un puntaje.
TEMAS = {
    "local": dict(
        etiqueta="local",
        puertas=[
            "planta_baja_tienda_",
            "planta_baja_panaderia_",
            "planta_baja_carniceria",
            "planta_baja_helado_tienda",
            "planta_baja_music",
            "planta_baja_gimnasio_",
            "planta_baja_carnada_tienda_",
        ],
        vidriada=True,
    ),
    "portal": dict(
        etiqueta="portal",
        puertas=["planta_baja_condominio_"],
        vidriada=False,
    ),
}

#: Distancia RGB máxima para decir "este paño es el MISMO que aquél". Sale de la
#: paleta del pack: los cuatro paños (ladrillo rojo, revoque gris, revoque azul,
#: ladrillo con friso verde) están a más de 60 unidades entre sí.
PAÑO_IGUAL = 60
#: Distancia a la que un pixel deja de ser paño y pasa a ser hueco/marco.
FUERA_DEL_PAÑO = 70
#: Alto (px) de la banda de abajo que decide si un hueco "llega al piso": una
#: puerta lo hace, una ventana no.
BANDA_PISO = 14
#: Tope de colores distintos DENTRO del hueco para que siga siendo una abertura.
#: Medido en las candidatas del pack: la puerta vidriada de local da 11, y el
#: cajero automático que se colaba como "entrada del edificio" da 17.
COLORES_DE_UNA_ABERTURA = 14


# ---------------------------------------------------------------------------
# 1. Leer el pack a través del manifest
# ---------------------------------------------------------------------------

_hojas = {}


def hoja(rel):
    if rel not in _hojas:
        _hojas[rel] = Image.open(os.path.join(ASSETS, rel)).convert("RGBA")
    return _hojas[rel]


def recorte(sprites, key):
    e = sprites[key]
    r = e["rect"]
    return hoja(e["png"]).crop(
        (r["col"] * CELL, r["row"] * CELL, (r["col"] + r["w"]) * CELL, (r["row"] + r["h"]) * CELL)
    )


def familia(sprites):
    """Las claves de la familia modular, **variantes incluidas**.

    ⚠ Las variantes NO se filtran, y ahí me equivoqué la primera vez: el importador
    masivo agrupó las 31 plantas medias como `piso_modular_medio` + `#1..#30`
    (`art_manifest::VARIANT_SEP`), así que descartar las que llevan `#` dejaba UNA
    sola planta media y el material de vivienda se quedaba sin ventana. Acá una
    variante es un paño de otro color, que es exactamente lo que hace falta para
    emparejar.
    """
    for k in sorted(sprites):
        if k.startswith(FAM) and "rect" in sprites[k]:
            yield k, k[len(FAM):]


def columna(sprites, key, altos):
    """**La columna repetible de una clave, si esa clave ES una columna.**

    ⚠ No se rebanan los frentes anchos. Medido: rebanar cada 32 px un
    `planta_baja_tienda_*` de 7 celdas da media vidriera, medio maniquí y medio
    cajero automático — probé el clasificador de uniformidad sobre las 417 columnas
    de la familia y el "paño más liso" que elegía era la persiana de un local y la
    "ventana" era la pantalla de un ATM. Los frentes de local del pack son DIBUJOS
    ENTEROS, exactamente como dice biblia §8 de los interiores.

    Lo repetible del pack es lo que el artista publicó de **una celda de ancho**:
    `piso_modular_medio` (la planta media, 1×4) y `planta_baja_*_modular` (la
    planta baja, 1×3). Ese es el sistema de edificios apilables de LimeZu, y es de
    ahí y de ningún otro lado que sale una pieza de borde.
    """
    r = sprites[key]["rect"]
    if r["w"] != 1 or r["h"] not in altos:
        return None
    im = recorte(sprites, key)
    return im if im.getbbox() else None


# ---------------------------------------------------------------------------
# 2. Medir una columna
# ---------------------------------------------------------------------------


def cuerpo(col):
    """Los pixeles del PAÑO, sin la cornisa de arriba ni la vereda de abajo.

    Las dos bandas se detectan, no se descuentan con un número: la cornisa son las
    filas de arriba cuyo color no es el dominante de la columna, y la vereda las de
    abajo. Con un `8` fijo, un paño con friso alto perdía media ventana.
    """
    w, h = col.size
    px = col.load()
    filas = []
    for y in range(h):
        fila = [px[x, y] for x in range(w)]
        if any(a < 128 for *_, a in fila):
            filas.append(None)
        else:
            filas.append([(r, g, b) for r, g, b, _ in fila])
    llenas = [y for y, f in enumerate(filas) if f]
    if len(llenas) < CELL:
        return None
    y0, y1 = llenas[0], llenas[-1] + 1
    # El tramo tiene que ser OPACO de punta a punta: una columna con un agujero de
    # transparencia en el medio no es un paño de fachada, es un recorte a silueta
    # (§8: el 92 % de los singles de interiores viene así).
    if any(filas[y] is None for y in range(y0, y1)):
        return None
    dom = paño_de(filas, y0, y1)
    # La cornisa: filas de arriba donde el paño casi no aparece.
    while y0 < y1 - CELL and sum(1 for c in filas[y0] if dist(c, dom) < FUERA_DEL_PAÑO) < w * 0.5:
        y0 += 1
    # La vereda: ídem por abajo.
    while y1 > y0 + CELL and sum(1 for c in filas[y1 - 1] if dist(c, dom) < FUERA_DEL_PAÑO) < w * 0.5:
        y1 -= 1
    return filas, y0, y1, dom


def paño_de(filas, y0, y1):
    """El color más frecuente del tramo: el paño."""
    cnt = {}
    for y in range(y0, y1):
        if not filas[y]:
            continue
        for c in filas[y]:
            cnt[c] = cnt.get(c, 0) + 1
    return max(cnt.items(), key=lambda t: t[1])[0] if cnt else (0, 0, 0)


def dist(a, b):
    return sum((x - y) ** 2 for x, y in zip(a, b)) ** 0.5


def medir(col):
    """Todo lo que hace falta saber de una columna, en una pasada."""
    c = cuerpo(col)
    if c is None:
        return None
    filas, y0, y1, dom = c
    lum, fuera, banda_tot, banda_fuera, iguales, previa = [], 0, 0, 0, 0, None
    variedad, total = set(), 0
    ys, xs, en_hueco = [], [], set()
    for y in range(y0, y1):
        f = filas[y]
        if not f:
            return None
        if previa is not None and f == previa:
            iguales += 1
        previa = f
        baja = y >= y1 - BANDA_PISO
        for x, (r, g, b) in enumerate(f):
            total += 1
            lum.append(0.299 * r + 0.587 * g + 0.114 * b)
            variedad.add((r // 24, g // 24, b // 24))
            afuera = dist((r, g, b), dom) > FUERA_DEL_PAÑO
            if afuera:
                fuera += 1
                ys.append(y)
                xs.append(x)
                en_hueco.add((r // 24, g // 24, b // 24))
            if baja:
                banda_tot += 1
                if afuera:
                    banda_fuera += 1
    if total == 0:
        return None
    alto_cuerpo = max(1, y1 - y0)
    return dict(
        paño=dom,
        y0=y0,
        y1=y1,
        # cuánto de la columna NO es paño (el hueco y su marco)
        hueco=fuera / total,
        # cuánto de la banda de abajo es hueco: una puerta llega al piso, una
        # ventana no. Es LA pregunta que separa las dos piezas.
        al_piso=banda_fuera / max(1, banda_tot),
        # **Y qué tan ALTO es ese hueco.** Sin esto, cualquier trasto apoyado en la
        # vereda pasa por puerta: la primera corrida eligió un CAJERO AUTOMÁTICO
        # como entrada del edificio de departamentos, porque su gabinete llega al
        # piso igual que una puerta. Una puerta ocupa el paño de arriba abajo.
        alto_hueco=(max(ys) - min(ys) + 1) / alto_cuerpo if ys else 0.0,
        ancho_hueco=(max(xs) - min(xs) + 1) / CELL if xs else 0.0,
        # **Cuántos colores distintos hay DENTRO del hueco.** Es lo que separa una
        # abertura de un trasto: un paño de vidrio o un hueco oscuro tienen cuatro o
        # cinco tonos; un cajero automático tiene su pantalla, sus botones, su
        # ranura y su cartel, y pasa de diez. Sin esta medida, el "cajero" ganaba el
        # puesto de entrada del edificio de departamentos con todos los demás
        # filtros en verde: llega al piso, es alto y es ancho.
        variedad_hueco=len(en_hueco),
        # un paño repite sus filas; una composición, no
        repite=iguales / max(1, alto_cuerpo - 1),
        variedad=len(variedad),
        contraste=statistics.pstdev(lum) if len(lum) > 1 else 0.0,
        oscuro=sum(1 for v in lum if v < 50) / len(lum),
    )


# ---------------------------------------------------------------------------
# 3. Elegir el juego de piezas de cada material
# ---------------------------------------------------------------------------


def inventario(sprites):
    """Cada columna repetible de la familia modular, medida una sola vez."""
    inv = []
    for key, fam in familia(sprites):
        col = columna(sprites, key, {3, ALTO})
        if col is None:
            continue
        mm = medir(col)
        if mm is None:
            continue
        inv.append(dict(key=key, fam=fam, col=col, **mm))
    return inv


def mismo_paño(inv, ref):
    return [c for c in inv if dist(c["paño"], ref["paño"]) < PAÑO_IGUAL]


def candidatas_de_puerta(inv, tema, usados=()):
    """Las entradas de calle del tema, de la mejor a la peor."""
    cand = [
        c
        for c in inv
        if any(c["fam"].startswith(p) for p in tema["puertas"])
        and c["al_piso"] > 0.55
        and c["hueco"] > 0.15
        # Una puerta ocupa el paño de arriba abajo. Ver `alto_hueco`: sin este
        # filtro, un cajero automático apoyado en la vereda pasaba por entrada.
        and c["alto_hueco"] > 0.60
        and c["ancho_hueco"] > 0.40
        and c["variedad_hueco"] <= COLORES_DE_UNA_ABERTURA
        # Y de un paño que no esté ya tomado por otro material: si los dos temas
        # eligen el mismo ladrillo, la paleta ofrece dos veces la misma fachada.
        and all(dist(c["paño"], u) >= PAÑO_IGUAL for u in usados)
    ]
    # Vidriada = el hueco es claro (vidrio, se ve el local). Entrada de vivienda =
    # hoja opaca. A igualdad, el hueco más grande: una entrada se tiene que LEER
    # como entrada de un vistazo.
    cand.sort(
        key=lambda c: (c["oscuro"] if tema["vidriada"] else -c["oscuro"], -c["al_piso"], -c["hueco"])
    )
    return cand


def juego_de_piezas(inv, tema, usados):
    """**El juego completo de un material, elegido de una sola vez.**

    La elección NO puede ser en cascada (primero la puerta, después su paño): la
    entrada más vistosa del pack puede ser de un color que no tiene ninguna columna
    lisa, y ahí te quedás sin material. Se recorren las entradas candidatas de la
    mejor a la peor y se devuelve la PRIMERA que trae paño y ventana de su color —
    que es la misma idea de `import-edificios.py`, donde un lote se queda con el
    mejor candidato que de verdad calza, no con el más lindo.
    """
    puertas = candidatas_de_puerta(inv, tema, usados["paños"])
    if not puertas and usados["paños"]:
        # **Medido: el pack trae entradas de calle en UN solo paño.** De las 123
        # columnas repetibles, las que pasan el filtro de puerta (hueco alto, ancho
        # y al piso) son tres, y las tres son del ladrillo rojo. Así que los dos
        # materiales comparten el paño y se diferencian por la ABERTURA — que es
        # exactamente cómo se ve una cuadra de verdad: el mismo ladrillo, abajo el
        # local y al lado la entrada de los departamentos.
        print("   (el pack sólo trae entradas en este paño: se comparte)")
        puertas = candidatas_de_puerta(inv, tema)
    if not puertas:
        # **El set repetible del pack no trae entrada de vivienda.** Medido: de las
        # 123 columnas modulares, la única que pasa el filtro de abertura es la
        # puerta vidriada de local; la de condominio es el cajero. Antes que dejar
        # el material sin puerta, se le presta la que hay y se avisa: el usuario le
        # recorta la suya en HA Studio y este script no se la pisa (regla 7 — el
        # código crea el slot, el pixel lo pone él).
        prestadas = usados["puertas"]
        if not prestadas:
            return None
        print("   (el pack no trae entrada propia: se presta la de otro material)")
        puertas = prestadas
    for puerta in puertas:
        hermanas = mismo_paño(inv, puerta)
        # PAÑO: la columna del color con MENOS hueco, y a igualdad la más
        # uniforme (repite filas, poca variedad de color, bajo contraste). El paño
        # sale de la planta media, que es la que el pack dibuja para apilarse.
        lisas = sorted(
            (c for c in hermanas if c["hueco"] < 0.22),
            key=lambda c: (c["hueco"], -c["repite"], c["variedad"], c["contraste"]),
        )
        if not lisas:
            continue
        paño = lisas[0]
        # VENTANA: la que el artista ya dibujó para repetir (`piso_modular_medio`),
        # y si su color no la tiene, cualquier columna del paño con hueco alto.
        ventanas = [
            c
            for c in hermanas
            if c["fam"].startswith("piso_modular_medio")
            and 0.22 < c["hueco"] < 0.60
            and c["al_piso"] < 0.30
        ]
        # Que no sea la misma que ya se llevó otro material: si el paño se comparte,
        # la ventana es la otra mitad de lo que distingue a los dos.
        propias = [c for c in ventanas if c["key"] not in usados["ventanas"]]
        ventanas = propias or ventanas
        ventanas.sort(key=lambda c: -c["hueco"])
        if not ventanas:
            continue
        # VANO: hueco al piso y OSCURO (se ve el adentro, no hay hoja). Opcional:
        # sin él, el juego dibuja el marco por código (`walls::marco_de_vano`).
        vanos = sorted(
            (c for c in hermanas if c["al_piso"] > 0.55 and c["oscuro"] > 0.25),
            key=lambda c: -c["oscuro"],
        )
        return dict(puerta=puerta, paño=paño, ventana=ventanas[0], vano=vanos[0] if vanos else None)
    return None


def componer(paño, pieza):
    """La pieza montada SOBRE el paño, al pie y a la altura del material.

    Es lo mismo que hace `import-construccion.py` con `door/stone` (la hoja de
    madera sobre el muro de piedra) y con `window/basic` (el paño de vidrio sobre
    la pared): la pieza del pack trae su hueco y su marco, el paño le da el resto
    de la altura, y las cuatro piezas del material terminan midiendo lo mismo.

    Se recorta la CORNISA de la pieza (`y0`) antes de pegarla: si no, el material
    queda con una moldura suelta en el medio del muro.
    """
    base = Image.new("RGBA", (CELL, ALTO * CELL), (0, 0, 0, 0))
    fondo = paño["col"]
    base.alpha_composite(fondo, (0, ALTO * CELL - fondo.size[1]))
    cuerpo_pieza = pieza["col"].crop((0, pieza["y0"], CELL, pieza["col"].size[1]))
    base.alpha_composite(cuerpo_pieza, (0, ALTO * CELL - cuerpo_pieza.size[1]))
    return base


def a_lienzo(pieza):
    """La columna tal cual, al pie de un lienzo del alto del material."""
    base = Image.new("RGBA", (CELL, ALTO * CELL), (0, 0, 0, 0))
    base.alpha_composite(pieza["col"], (0, ALTO * CELL - pieza["col"].size[1]))
    return base


def canto(paño):
    """El COSTADO de la casa: una tira fina del paño, repetida a lo ancho.

    Modern Exteriors no dibuja costados —sólo el frente—, así que el lado este y
    oeste de una casa se ve de canto. Estirar el frente ahí daría una segunda
    fachada mirando a cámara, que es exactamente el bug que `walls::Cara` vino a
    matar. Una tira del paño es lo honesto: desde la calle, el lado de una casa es
    ladrillo.
    """
    col = paño["col"]
    tira = col.crop((CELL // 2 - 3, 0, CELL // 2 + 3, col.size[1]))
    base = Image.new("RGBA", (CELL, ALTO * CELL), (0, 0, 0, 0))
    for x in range(0, CELL, tira.size[0]):
        base.alpha_composite(tira, (x, ALTO * CELL - col.size[1]))
    return base


# ---------------------------------------------------------------------------
# 4. La página (misma clase que import-construccion.py)
# ---------------------------------------------------------------------------


class Pagina:
    """Acumula recortes de píxel arbitrario y devuelve rects EN CELDAS."""

    def __init__(self, cols=32):
        self.cols = cols
        self.items = []

    def add(self, nombre, im, w_celdas, h_celdas):
        lienzo = Image.new("RGBA", (w_celdas * CELL, h_celdas * CELL), (0, 0, 0, 0))
        # Al pie: la pared crece hacia arriba desde el borde.
        lienzo.alpha_composite(im, (0, h_celdas * CELL - im.size[1]))
        self.items.append((nombre, lienzo, w_celdas, h_celdas))
        return nombre

    def componer(self):
        sitio, col, row, alto_fila = {}, 0, 0, 0
        for nombre, im, w, h in self.items:
            if col + w > self.cols:
                col, row, alto_fila = 0, row + alto_fila, 0
            sitio[nombre] = (col, row, w, h)
            col += w
            alto_fila = max(alto_fila, h)
        pagina = Image.new("RGBA", (self.cols * CELL, (row + alto_fila) * CELL), (0, 0, 0, 0))
        for nombre, im, w, h in self.items:
            c, r, _, _ = sitio[nombre]
            pagina.alpha_composite(im, (c * CELL, r * CELL))
        return pagina, sitio


# ---------------------------------------------------------------------------


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true", help="informa y no escribe")
    ap.add_argument("--preview", metavar="PNG", help="guarda la página ampliada")
    args = ap.parse_args()

    with open(MANIFEST) as fh:
        man = json.load(fh)
    sprites = man["sprites"]

    inv = inventario(sprites)
    print(f"{len(inv)} columnas modulares medidas")

    pag = Pagina()
    materiales = {}
    usados = {"paños": [], "ventanas": [], "puertas": []}
    # **El tema con MENOS entradas candidatas elige primero.** El pack trae una sola
    # columna modular de condominio (con sus variantes de color) y cuarenta de local:
    # si el local elige antes, se lleva el ladrillo rojo y la vivienda se queda sin
    # paño. Es el mismo criterio que `import-edificios.py` usa para los lotes —
    # servir primero al que tiene menos dónde elegir.
    # El que no tiene NINGUNA va último de todos: no elige, pide prestado.
    orden = sorted(
        TEMAS.items(),
        key=lambda t: (len(candidatas_de_puerta(inv, t[1])) == 0, len(candidatas_de_puerta(inv, t[1]))),
    )
    for nombre, tema in orden:
        juego = juego_de_piezas(inv, tema, usados)
        if juego is None:
            raise SystemExit(f"{nombre}: ninguna entrada de {tema['puertas']} tiene paño y ventana")
        puerta, paño = juego["puerta"], juego["paño"]
        ventana, vano = juego["ventana"], juego["vano"]
        usados["paños"].append(paño["paño"])
        usados["ventanas"].append(ventana["key"])
        usados["puertas"].append(puerta)
        suf = tema["etiqueta"]
        print(f"== fachada {suf}  paño rgb={paño['paño']}")
        for etq, c in (("puerta", puerta), ("paño", paño), ("ventana", ventana), ("vano", vano)):
            if c is None:
                print(f"   {etq:8s} — SIN PIXEL (el juego lo dibuja por código)")
                continue
            print(
                f"   {etq:8s} {c['fam']:36s} hueco={c['hueco']:.2f} "
                f"piso={c['al_piso']:.2f} altoh={c['alto_hueco']:.2f} "
                f"colh={c['variedad_hueco']:2d} osc={c['oscuro']:.2f} repite={c['repite']:.2f}"
            )

        piezas = {
            f"wall/fachada_{suf}": a_lienzo(paño),
            f"window/fachada_{suf}": a_lienzo(ventana) if ventana else None,
            f"door/fachada_{suf}": componer(paño, puerta),
            f"doorway/fachada_{suf}": componer(paño, vano) if vano else None,
        }
        recortes = {}
        for clave, im in piezas.items():
            if im is None:
                continue
            r = {
                "back": pag.add(f"{clave}:back", im, 1, ALTO),
                "left": pag.add(f"{clave}:left", canto(paño), 1, ALTO),
            }
            # **La ESPALDA del edificio** (`walls::Cara::Fondo`): sólo para la PARED.
            //
            # Modern Exteriors no dibuja el reverso de nada — un edificio visto desde
            # atrás es su techo. Lo honesto para un paño ciego es el paño liso: desde
            # la calle de atrás, la casa es ladrillo y no el empapelado de adentro.
            #
            # Pero la puerta y la ventana NO llevan espalda a propósito. Taparlas con
            # ladrillo liso las volvería INVISIBLES desde el norte: una puerta que se
            # cruza dibujada como muro macizo es peor que una puerta con el pixel de
            # adentro. Sin pose propia caen a `front` por la cadena y al menos se ven
            # como lo que son.
            if clave.startswith("wall/"):
                r["espalda"] = pag.add(f"{clave}:espalda", a_lienzo(paño), 1, ALTO)
            recortes[clave] = r
        materiales[suf] = recortes

    pagina, sitio = pag.componer()
    print(f"página: {pagina.size[0]}x{pagina.size[1]} px, {len(sitio)} recortes")

    if args.preview:
        fondo = Image.new("RGBA", pagina.size, (28, 28, 38, 255))
        fondo.alpha_composite(pagina)
        s = 4
        fondo.convert("RGB").resize(
            (pagina.size[0] * s, pagina.size[1] * s), Image.NEAREST
        ).save(args.preview)
        print(f"vista previa: {args.preview}")
    if args.dry_run:
        return

    pagina.save(os.path.join(ASSETS, PAGINA_REL), optimize=True)

    previo = {}
    if os.path.exists(INDEX):
        with open(INDEX) as fh:
            previo = json.load(fh)
        for k in previo.get("sprites", []):
            sprites.pop(k, None)

    def rect(nombre):
        c, r, w, h = sitio[nombre]
        return {"col": c, "row": r, "w": w, "h": h}

    nuevos = []
    for suf, recortes in materiales.items():
        # De qué slot de interior sale la cara de ADENTRO. Es el punto entero del
        # trabajo: afuera Modern Exteriors, adentro Modern Interiors — y la de
        # piedra, porque una fachada de ladrillo por dentro es revoque, no tablas.
        adentro = sprites.get("wall/stone", {})
        for clave, caras in recortes.items():
            poses = {
                # La cara de AFUERA: la elevación del pack.
                "back": {"rect": rect(caras["back"])},
                # Los COSTADOS: la tira fina del paño (el pack no trae costados).
                "left": {"rect": rect(caras["left"])},
                "right": {"rect": rect(caras["left"])},
            }
            for p in poses.values():
                p["png"] = PAGINA_REL
                p["grid"] = [CELL, CELL]
            # La cara de ADENTRO (`front`, o sea el tabique) y `center` se quedan
            # con el paño de interior que ya eligió `import-construccion.py`. Una
            # `Pose` admite su propio `png`/`grid` (`art_manifest::Pose::png_or`),
            # así que un mismo slot mezcla las dos hojas sin copiar un pixel.
            if "rect" in adentro:
                dentro = {
                    "rect": adentro["rect"],
                    "png": adentro["png"],
                    "grid": adentro["grid"],
                }
                sub = (adentro.get("poses") or {}).get("front")
                if sub and "rect" in sub:
                    dentro["rect"] = sub["rect"]
                poses["front"] = dict(dentro)
                poses["center"] = dict(dentro)
            else:
                poses["front"] = dict(poses["back"])
                poses["center"] = dict(poses["back"])
            # Una puerta necesita sus reposos o no se anima (`walls::pose_de_puerta`).
            # El pack no trae la hoja abierta de estas fachadas, así que `back_open`
            # es el mismo dibujo: la puerta del frente se cruza, no se abre sola.
            # Cuando aparezca el cuadro abierto, se agrega acá y el juego lo usa.
            if clave.startswith("door/"):
                poses["back_closed"] = dict(poses["back"])
                poses["back_open"] = dict(poses["back"])
                poses["closed"] = dict(poses["back"])
            sprites[clave] = {
                "png": PAGINA_REL,
                "grid": [CELL, CELL],
                "rect": rect(caras["back"]),
                "poses": poses,
            }
            nuevos.append(clave)

    with open(MANIFEST, "w") as fh:
        json.dump(man, fh, indent=1, ensure_ascii=False, sort_keys=True)
    with open(INDEX, "w") as fh:
        json.dump({"sprites": nuevos}, fh, indent=1)
    print(f"manifest: {len(nuevos)} claves · índice: {INDEX}")


if __name__ == "__main__":
    main()
