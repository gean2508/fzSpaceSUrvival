#!/usr/bin/env python3
"""**El kit modular de edificios de Modern Exteriors, entero y sin cortar.**

    python3 scripts/import-modulares.py --dry-run --hoja /tmp/kit.png
    python3 scripts/import-modulares.py

## Por qué existe

`import-pack-masivo.py` recorre las carpetas `*_Singles_32x32` del pack, y para la
hoja de edificios modulares eso es **exactamente lo que no hay que hacer**: los
"singles" de esa carpeta son recortes de 1 celda de bandas que el artista dibujó
anchas. En el manifest quedaron 92 columnas de 1×3 y 31 de 1×4 donde el pack tiene
bandas de 9 celdas. Construir con esos pedazos da media pared con medio cristal.

Este script lee **la hoja cruda** (`5_Floor_Modular_Buildings_32x32.png`, 32×259
celdas) y la segmenta por **islas de PIXEL opaco**. La primera versión segmentaba
por islas de CELDA y era el mismo error una capa más arriba: dos piezas que se
tocan en media celda quedaban pegadas, y el rótulo impreso de la hoja se volvía un
slot. Con islas de pixel salen 337 piezas y la gramática aparece sola.

## Lo que la hoja declara, medido y no supuesto

Los tres números que sostienen todo salieron de medir la hoja, y el script los
vuelve a verificar en cada corrida (`verificar_estructura`):

    MODULO   =    9 celdas   el frente de un edificio modular
    PERIODO  = 2432 px       lo que ocupa un TEMA completo
    Y0       =  504 px       dónde arranca el primer tema

La hoja trae **tres temas** —ladrillo, azul y hormigón— dibujados uno debajo del
otro con la MISMA planta: la pieza que en el tema 1 está en `(off, x)` está en los
tres. De ahí sale el filtro que limpia todo lo demás:

> **Una pieza pertenece al kit si su firma `(offset dentro del tema, x, ancho,
> alto)` aparece en los tres temas.**

Eso barre de un saque el encabezado (`"Floor-Modular Buildings"`, `"combine the
pieces to get tons of different buildings"`), la ilustración de muestra con el
personajito —que NO es una azotea, aunque mida 8×14,6— y la sección suelta del
final. Y de la misma comparación sale gratis el otro dato: si las tres copias
tienen **el mismo pixel**, la pieza es COMPARTIDA (un solo sprite); si difieren,
es del tema (tres variantes).

## Las islas de 18 celdas son DOS módulos, no una banda de 18

Las bandas anchas miden 576 px = 18 celdas exactas, y no repiten: la mitad
izquierda y la derecha son dibujos DISTINTOS que se tocan en el pilar del medio.
Son dos módulos de 9 que el artista puso pegados. Se parten al medio y a cada
mitad se le recalcula su bounding box, porque no empiezan a la misma altura (la
azotea de la izquierda mide 9×6 justas y la de la derecha 9×6,25).

Ojo con `Image.getbbox()` para comparar dos recortes: desde Pillow 10 mira
**sólo el alfa**, así que `difference(a, b).getbbox() is None` da "iguales" para
dos paños opacos completamente distintos. Acá se compara con numpy.

## Qué NO hace

No recorta bandas a columnas, no compone piezas que el pack no dibujó juntas y no
genera un solo pixel. Lo que no está en `GRAMATICA` no se exporta: se cuenta y se
lista al final de la corrida, para que nada se pierda en silencio.

## Qué escribe

- `art/pack/modulares_N.png` — páginas de 1024×1024 (la medida del resto del
  pack; ver biblia §10 sobre por qué no 2048). El pixel va **abajo y centrado**
  dentro de su bloque de celdas, que es la convención que dejó
  `reparar-importados.py --solo asentar`: el mundo planta con el pivote al pie.
- Las entradas de `art/manifest.json` (sprites + objetos con variantes).
- El bloque `// === MODULARES:START/END ===` de `ha-assets/src/slots.rs`.
- `scripts/modulares-import-index.json` — lo que escribió, para que una segunda
  corrida pise **lo suyo** y nada más.
"""

import argparse
import json
import math
import os
from collections import defaultdict

import numpy as np
from PIL import Image, ImageDraw

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ASSETS = os.path.join(ROOT, "native/crates/ha-game/assets")
PACK = os.path.join(ROOT, "PRUEBAS-CODIGOS/modernexteriors-win/Modern_Exteriors_32x32")
HOJA = os.path.join(PACK, "ME_Theme_Sorter_32x32/5_Floor_Modular_Buildings_32x32.png")
MANIFEST = os.path.join(ASSETS, "art/manifest.json")
SLOTS_RS = os.path.join(ROOT, "native/crates/ha-assets/src/slots.rs")
INDEX = os.path.join(ROOT, "scripts/modulares-import-index.json")
PAGES_DIR = os.path.join(ASSETS, "art/pack")
PAGES_REL = "art/pack"

CELL = 32
PAGE = 1024  # px por página de atlas = 32×32 celdas. Igual que el resto del pack.
OPACO = 16   # alfa mínimo para que un pixel cuente como dibujo

#: El frente de un edificio modular, en celdas. Sale de medir la hoja: TODAS las
#: bandas miden 9 celdas o 18 (= dos de 9). No es un número elegido.
MODULO = 9
#: Lo que ocupa un tema completo, en px, y dónde arranca el primero.
PERIODO = 2432
Y0 = 504
#: Cuántos temas trae la hoja, en el orden en que están dibujados.
TEMAS = [("ladrillo", "Ladrillo"), ("azul", "Azul"), ("hormigon", "Hormigón")]


# ---------------------------------------------------------------------------
# La gramática: qué es cada pieza. La clave es (offset dentro del tema, x).
#
# Los offsets salen de la segmentación —no están escritos a ojo— y los NOMBRES
# de mirar el pixel: la hoja no trae rótulos, así que cada entrada se leyó del
# contact sheet (`--hoja`). Si el pack cambia, la corrida avisa qué firma nueva
# apareció y cuál se fue.
#
# `familia` agrupa varias piezas intercambiables bajo UN slot con variantes (los
# muretes, las rejillas, los tendederos): son la misma cosa en distintos largos y
# no merecen un slot cada una.
# ---------------------------------------------------------------------------

# (off, x) -> (clave, label)
GRAMATICA = {
    # — LA CÁSCARA — lo que apila un edificio. Todo mide 9 celdas de frente.
    (8, 0): ("edificio/azotea", "Azotea con parapeto"),
    (0, 288): ("edificio/azotea_clara", "Azotea con parapeto claro"),
    (208, 0): ("edificio/tejado", "Tejado con chimeneas"),
    (232, 288): ("edificio/tejado_liso", "Tejado liso"),
    (456, 0): ("edificio/piso_ventanal", "Piso: ventanales altos"),
    (456, 288): ("edificio/piso_rejilla", "Piso: ventanas chicas"),
    (616, 0): ("edificio/piso_balcon", "Piso: balcón"),
    (616, 288): ("edificio/piso_doble", "Piso: ventana doble"),
    (424, 608): ("edificio/pb_vidrio", "Planta baja: puerta de vidrio"),
    (552, 608): ("edificio/pb_cajero", "Planta baja: cajero automático"),
    (680, 608): ("edificio/pb_arco", "Planta baja: puerta de arco"),
    (808, 608): ("edificio/pb_madera", "Planta baja: puerta de madera"),
    (776, 0): ("edificio/local", "Local con vidriera"),
    (776, 320): ("edificio/local_angosto", "Local angosto"),
    # — LO QUE VIVE ARRIBA — el juego es cenital: la azotea es lo que más se ve.
    (18, 740): ("azotea/equipo", "Equipo de azotea"),
    (160, 810): ("azotea/aire", "Aire acondicionado"),
    (256, 846): ("azotea/cantero", "Equipo con canteros"),
    (8, 608): ("azotea/conducto", "Conducto de ventilación"),
    (136, 608): ("azotea/conducto_claro", "Conducto de ventilación claro"),
    (44, 932): ("azotea/cajero", "Cajero automático"),
    (160, 934): ("azotea/tablero", "Tablero de servicio"),
    (88, 704): ("azotea/respiradero", "Respiradero"),
    (264, 708): ("azotea/extractor", "Extractor"),
    (264, 784): ("azotea/extractor_doble", "Extractor doble"),
    # — TIENDAS — plantas bajas con su comercio adentro, tal como las dibujó.
    (936, 0): ("edificio/tienda_mascotas", "Tienda: mascotas"),
    (936, 320): ("edificio/tienda_cerrada", "Tienda cerrada"),
    (936, 544): ("edificio/tienda_discos", "Tienda: discos"),
    (936, 768): ("edificio/tienda_galletas", "Tienda: galletas"),
    (1064, 0): ("edificio/tienda_persiana", "Tienda con la persiana baja"),
    (1064, 544): ("edificio/tienda_juguetes", "Tienda: juguetes"),
    (1064, 768): ("edificio/tienda_deposito", "Depósito"),
    (1192, 0): ("edificio/tienda_vitrina_pan", "Tienda: vitrina de panadería"),
    (1192, 256): ("edificio/tienda_vitrina_juguetes", "Tienda: vitrina de juguetería"),
    (1192, 704): ("edificio/tienda_vacia", "Local vacío"),
    (1320, 320): ("edificio/tienda_entera", "Carnicería entera (9×10)"),
    (1544, 640): ("edificio/tienda_pesca", "Tienda: pesca"),
    (1672, 0): ("edificio/tienda_panaderia", "Tienda: panadería"),
    (1672, 320): ("edificio/tienda_carniceria", "Tienda: carnicería"),
    (1976, 320): ("edificio/tienda_gimnasio", "Gimnasio"),
    (2130, 320): ("edificio/tienda_gimnasio_bajo", "Gimnasio (marquesina baja)"),
    (2152, 0): ("edificio/tienda_heladeria", "Heladería"),
    (2280, 320): ("edificio/tienda_vidriera", "Local con vidriera corrida"),
    # — CARTELES — se cuelgan sobre la planta baja; el pack los dibuja sueltos.
    (1192, 512): ("cartel/discos", "Cartel: disquería"),
    (1658, 680): ("cartel/pesca", "Cartel: pesca"),
    (1800, 390): ("cartel/carniceria", "Cartel: carnicería"),
    (2136, 640): ("cartel/gimnasio", "Cartel: gimnasio"),
    (2226, 628): ("cartel/gimnasio_ancho", "Cartel: gimnasio (ancho)"),
    (2260, 40): ("cartel/heladeria", "Cartel: heladería"),
    # — SOMBRA — la hoja trae su propio bloque, rotulado "BUILDING SHADOWS".
    (2280, 864): ("edificio/sombra", "Sombra de edificio"),
}

#: Familias: varias piezas intercambiables bajo un solo slot con variantes. El
#: nombre de cada variante se puede escribir; si va `None`, sale de su MEDIDA, que
#: en esos casos es lo único que las distingue.
FAMILIAS = {
    "edificio/murete": (
        "Murete de ladrillo",
        [((1090, 336), None), ((1096, 466), None), ((1888, 48), None), ((1896, 178), None),
         ((1896, 374), None), ((1896, 418), None), ((1896, 482), None), ((1896, 518), None),
         ((2312, 762), None), ((2314, 634), None), ((2320, 222), None)],
    ),
    "azotea/rejilla": (
        "Rejilla de azotea",
        [((170, 706), "baja"), ((170, 738), "baja ancha"), ((202, 706), "media"),
         ((202, 738), "media ancha"), ((234, 706), "alta"), ((234, 738), "alta ancha")],
    ),
    "edificio/tendedero": (
        "Tendedero",
        [((1976, 800), None), ((1976, 832), None), ((1976, 1018), None), ((1986, 960), None),
         ((2008, 832), None), ((2014, 928), None), ((2072, 832), None), ((2080, 928), None),
         ((2136, 832), None), ((2144, 928), None)],
    ),
    "cartel/panaderia": (
        "Cartel: panadería",
        [((1784, 6), "con queso"), ((1794, 180), "simple")],
    ),
    "prop/heladeria": (
        "Mobiliario de heladería",
        [((2346, 34), None), ((2346, 162), None), ((2356, 92), None),
         ((2364, 70), None), ((2364, 134), None)],
    ),
}

#: Lo que la hoja dice de sí misma: rótulos impresos que no son arte del juego.
#: Se listan para que la corrida no los reporte como "sin clasificar" — no son un
#: pendiente, son tinta del catálogo.
TINTA = {(1220, 918), (1220, 940), (1220, 980)} | {
    (2216, x) for x in (834, 854, 874, 892, 908, 928, 948, 970)
} | {(2248, x) for x in (832, 856, 878, 900, 920, 946, 976)}

#: Copias que la hoja dibuja PISADAS por su vecina: el cartel de carnicería y el
#: de pesca aparecen tres veces cada uno, y las dos copias de los costados quedan
#: cortadas al medio ("BUT…", "…ERY", "B… S…"). Son la misma pieza que ya se
#: exporta entera, no piezas nuevas: sacarlas es la única forma de no meter medio
#: cartel en el juego. Se ve en `--hoja`.
RECORTADAS = {(1804, 352), (1806, 544), (1786, 648), (1786, 732)}


# ---------------------------------------------------------------------------
# 1. Segmentar: cada isla de pixel opaco es una pieza del artista
# ---------------------------------------------------------------------------


def etiquetar(mascara):
    """Componentes conexas de 8 vecinos, por corridas de fila + union-find.

    Hecho a mano y no con `scipy.ndimage.label` para no traerse scipy por una
    función: el repo ya usa numpy y nada más.
    """
    alto, ancho = mascara.shape
    padre = [0]

    def raiz(x):
        while padre[x] != x:
            padre[x] = padre[padre[x]]
            x = padre[x]
        return x

    def unir(a, b):
        ra, rb = raiz(a), raiz(b)
        if ra != rb:
            padre[max(ra, rb)] = min(ra, rb)

    lab = np.zeros((alto, ancho), np.int32)
    previa = []  # corridas de la fila anterior: (x0, x1, id)
    for y in range(alto):
        fila = mascara[y]
        if not fila.any():
            previa = []
            continue
        d = np.diff(np.concatenate(([0], fila.view(np.int8), [0])))
        cur = []
        for x0, x1 in zip(np.flatnonzero(d == 1), np.flatnonzero(d == -1)):
            # 8 vecinos: una corrida toca a la de arriba si se solapan en diagonal.
            ids = [i for (px0, px1, i) in previa if px0 - 1 < x1 and x0 - 1 < px1]
            if ids:
                mio = ids[0]
                for otro in ids[1:]:
                    unir(mio, otro)
            else:
                mio = len(padre)
                padre.append(mio)
            lab[y, x0:x1] = mio
            cur.append((x0, x1, mio))
        previa = cur
    return np.array([raiz(i) for i in range(len(padre))], np.int32)[lab]


def ajustar(arr, y, x, h, w):
    """Bounding box apretado del pixel opaco dentro de un rectángulo."""
    sub = arr[y:y + h, x:x + w]
    m = sub[:, :, 3] > OPACO
    if not m.any():
        return None
    ys, xs = np.nonzero(m)
    return dict(x=x + int(xs.min()), y=y + int(ys.min()),
                w=int(xs.max() - xs.min() + 1), h=int(ys.max() - ys.min() + 1),
                px=int(m.sum()))


def piezas(arr):
    """Islas de pixel, con las de 18 celdas partidas en sus dos módulos de 9."""
    lab = etiquetar(arr[:, :, 3] > OPACO)
    ys, xs = np.nonzero(lab)
    ids = lab[ys, xs]
    orden = np.argsort(ids, kind="stable")
    ids, ys, xs = ids[orden], ys[orden], xs[orden]
    cortes = np.flatnonzero(np.diff(ids)) + 1
    out = []
    for sy, sx in zip(np.split(ys, cortes), np.split(xs, cortes)):
        if len(sy) < 40:  # motas sueltas del antialias
            continue
        y0, y1 = int(sy.min()), int(sy.max()) + 1
        x0, x1 = int(sx.min()), int(sx.max()) + 1
        ancho = x1 - x0
        paso = MODULO * CELL
        n = ancho // paso if ancho % paso == 0 and ancho > paso else 1
        for k in range(n):
            p = ajustar(arr, y0, x0 + k * paso if n > 1 else x0,
                        y1 - y0, paso if n > 1 else ancho)
            if p:
                out.append(p)
    return out


# ---------------------------------------------------------------------------
# 2. Agrupar por tema y quedarse con el kit
# ---------------------------------------------------------------------------


def verificar_estructura(arr, pzs):
    """Los tres números medidos siguen valiendo para esta hoja."""
    alto = arr.shape[0]
    esperados = (alto - Y0) // PERIODO
    if esperados < len(TEMAS):
        raise SystemExit(
            f"la hoja mide {alto} px: no entran los {len(TEMAS)} temas de {PERIODO} px "
            f"desde y={Y0}. Volvé a medir antes de tocar nada.")
    # Dentro de los temas no puede quedar nada más ancho que el módulo: si queda,
    # es una banda que no se partió y estaríamos a punto de exportar media pared.
    # (Arriba de `Y0` vive el encabezado de la hoja, que sí trae dibujos anchos.)
    anchos = {p["w"] for p in pzs
              if p["y"] >= Y0 and (p["y"] - Y0) // PERIODO < len(TEMAS) and p["w"] > MODULO * CELL}
    if anchos:
        raise SystemExit(f"quedaron piezas más anchas que el módulo: {sorted(anchos)}")


def kit(arr, pzs):
    """`(off, x) -> [instancia por tema]`, sólo lo que está en los tres temas."""
    firmas = defaultdict(dict)
    for p in pzs:
        if p["y"] < Y0:
            continue
        tema = (p["y"] - Y0) // PERIODO
        if tema >= len(TEMAS):
            continue
        firmas[((p["y"] - Y0) % PERIODO, p["x"], p["w"], p["h"])][tema] = p
    out = {}
    for (off, x, w, h), porTema in firmas.items():
        if len(porTema) == len(TEMAS):
            out[(off, x)] = [porTema[t] for t in range(len(TEMAS))]
    return out


def unicos(arr, instancias):
    """Las instancias distintas por PIXEL, y qué tema usa cada una.

    Si las tres son iguales la pieza es compartida (un sprite, sin variantes); si
    difieren, hay una variante por tema.
    """
    grupos = []  # [(indice de la instancia modelo, [temas que la usan])]
    for t, p in enumerate(instancias):
        img = arr[p["y"]:p["y"] + p["h"], p["x"]:p["x"] + p["w"]]
        for g in grupos:
            q = instancias[g[0]]
            if np.array_equal(img, arr[q["y"]:q["y"] + q["h"], q["x"]:q["x"] + q["w"]]):
                g[1].append(t)
                break
        else:
            grupos.append((t, [t]))
    return grupos


# ---------------------------------------------------------------------------
# 3. Objetos a escribir
# ---------------------------------------------------------------------------


class Objeto:
    """Un slot con sus variantes. `sprites` = [(id de variante, label, pieza)]."""

    def __init__(self, clave, label):
        self.clave = clave
        self.label = label
        self.sprites = []


def armar(arr, elkit):
    objetos, sobrantes = [], []
    en_familia = {pos for _, miembros in FAMILIAS.values() for pos, _ in miembros}

    for pos, instancias in sorted(elkit.items()):
        if pos in TINTA or pos in RECORTADAS or pos in GRAMATICA or pos in en_familia:
            continue
        sobrantes.append((pos, instancias[0]))

    for pos, (clave, label) in sorted(GRAMATICA.items(), key=lambda kv: kv[1][0]):
        if pos not in elkit:
            print(f"  ⚠ {clave}: la firma {pos} ya no está en la hoja")
            continue
        obj = Objeto(clave, label)
        grupos = unicos(arr, elkit[pos])
        if len(grupos) == 1:
            obj.sprites.append((None, label, elkit[pos][0]))
        else:
            for modelo, temas in grupos:
                slug, nombre = TEMAS[modelo]
                if len(temas) > 1:
                    nombre = " y ".join(TEMAS[t][1] for t in temas)
                obj.sprites.append((slug, f"{label} · {nombre}", elkit[pos][modelo]))
        objetos.append(obj)

    for clave, (label, miembros) in sorted(FAMILIAS.items()):
        obj = Objeto(clave, label)
        for pos, nombre_pieza in miembros:
            if pos not in elkit:
                print(f"  ⚠ {clave}: la firma {pos} ya no está en la hoja")
                continue
            grupos = unicos(arr, elkit[pos])
            for modelo, temas in grupos:
                p = elkit[pos][modelo]
                cw, ch = math.ceil(p["w"] / CELL), math.ceil(p["h"] / CELL)
                vid = f"{pos[0]}_{pos[1]}"
                nombre = f"{label} · {nombre_pieza}" if nombre_pieza else f"{label} de {cw}×{ch}"
                if len(grupos) > 1:
                    vid += f"_{TEMAS[modelo][0]}"
                    nombre += f" · {TEMAS[modelo][1]}"
                obj.sprites.append((vid, nombre, p))
        # Dos variantes con el mismo nombre se distinguen por un ordinal, no por
        # el offset crudo: en el editor se lee "Murete de ladrillo de 4×2 (2)".
        vistos = defaultdict(int)
        renombradas = []
        for vid, nombre, p in obj.sprites:
            vistos[nombre] += 1
            if vistos[nombre] > 1:
                nombre = f"{nombre} ({vistos[nombre]})"
            renombradas.append((vid, nombre, p))
        obj.sprites = renombradas
        if obj.sprites:
            objetos.append(obj)
    return objetos, sobrantes


# ---------------------------------------------------------------------------
# 4. Empaquetar en páginas
# ---------------------------------------------------------------------------


def empaquetar(arr, hoja, objetos, dry_run=False):
    """Pega cada pieza en su bloque de celdas: **abajo y centrada**.

    El mundo planta con el pivote al pie, así que el aire tiene que quedar arriba
    y no abajo. Es la misma convención que dejó `reparar-importados.py --solo
    asentar` después de despegar 7.718 sprites del piso.
    """
    lado = PAGE // CELL
    piezas_ = []
    for obj in objetos:
        for vid, _, p in obj.sprites:
            cw, ch = math.ceil(p["w"] / CELL), math.ceil(p["h"] / CELL)
            if cw > lado or ch > lado:
                raise SystemExit(f"{obj.clave}: {cw}×{ch} celdas no entra en una página de {lado}")
            piezas_.append((obj.clave, vid, p, cw, ch))
    # Alto primero, ancho después: las bandas de 9 se apilan de a tres por fila y
    # los props chicos se cuelan en las 5 columnas que sobran al costado.
    piezas_.sort(key=lambda t: (-t[4], -t[3]))

    paginas, ubicacion = [], {}
    estantes = []  # (indice de página, fila, alto de la fila, columna libre)

    def abrir(ch):
        """Un estante nuevo: al final de la última página, o en una página nueva."""
        usado = sum(e[2] for e in estantes if e[0] == len(paginas) - 1) if paginas else lado
        if not paginas or usado + ch > lado:
            paginas.append(Image.new("RGBA", (PAGE, PAGE), (0, 0, 0, 0)))
            usado = 0
        estantes.append([len(paginas) - 1, usado, ch, 0])
        return estantes[-1]

    for clave, vid, p, cw, ch in piezas_:
        est = next((e for e in estantes if e[2] >= ch and e[3] + cw <= lado), None) or abrir(ch)
        ipag, fila, _, col = est
        paginas[ipag].paste(
            hoja.crop((p["x"], p["y"], p["x"] + p["w"], p["y"] + p["h"])),
            (col * CELL + (cw * CELL - p["w"]) // 2, (fila + ch) * CELL - p["h"]),
        )
        ubicacion[(clave, vid)] = (ipag, col, fila, cw, ch)
        est[3] += cw

    ocupadas = sum(cw * ch for _, _, _, cw, ch in piezas_)
    print(f"  {ocupadas} celdas de sprite en {len(paginas) * lado * lado} disponibles "
          f"({100 * ocupadas / (len(paginas) * lado * lado):.0f}% de aprovechamiento)")
    if not dry_run:
        for i, im in enumerate(paginas, start=1):
            im.save(os.path.join(PAGES_DIR, f"modulares_{i}.png"))
    return len(paginas), ubicacion


# ---------------------------------------------------------------------------
# 5. Escribir manifest, slots.rs e índice
# ---------------------------------------------------------------------------


def clave_sprite(clave, vid):
    return clave if vid is None else f"{clave}#{vid}"


def escribir_manifest(objetos, ubicacion, dry_run=False):
    man = json.load(open(MANIFEST))
    sprites, objs = man.setdefault("sprites", {}), man.setdefault("objects", {})
    viejo = json.load(open(INDEX)) if os.path.exists(INDEX) else {"sprites": [], "objects": []}
    for k in viejo["sprites"]:
        sprites.pop(k, None)
    for k in viejo["objects"]:
        objs.pop(k, None)

    nuevo = {"sprites": [], "objects": []}
    for obj in objetos:
        variantes = []
        for i, (vid, label, _) in enumerate(obj.sprites):
            pag, col, row, w, h = ubicacion[(obj.clave, vid)]
            entrada = {"grid": [CELL, CELL], "png": f"{PAGES_REL}/modulares_{pag + 1}.png",
                       "rect": {"col": col, "row": row, "w": w, "h": h}}
            k = clave_sprite(obj.clave, vid)
            sprites[k] = entrada
            nuevo["sprites"].append(k)
            if vid is not None:
                variantes.append({"id": vid, "label": label})
            if i == 0 and vid is not None:
                # El slot pelado dibuja la primera variante: sin esto un
                # `edificio/azotea` sin variante pedida cae al fallback por código.
                sprites[obj.clave] = entrada
                nuevo["sprites"].append(obj.clave)
        if variantes:
            objs[obj.clave] = {"label": obj.label, "variants": variantes}
            nuevo["objects"].append(obj.clave)

    if not dry_run:
        json.dump(man, open(MANIFEST, "w"), indent=1, sort_keys=True)
        json.dump(nuevo, open(INDEX, "w"), indent=1)
    return len(nuevo["sprites"]), len(nuevo["objects"])


def escribir_slots(objetos, dry_run=False):
    txt = open(SLOTS_RS).read()
    ini, fin = "    // === MODULARES:START ===", "    // === MODULARES:END ==="
    if ini not in txt:
        raise SystemExit(f"falta el bloque {ini.strip()} en slots.rs")
    cuerpo = "".join(
        f'    ("{obj.clave}", "{obj.label}"),\n' for obj in objetos
    )
    a = txt.index(ini) + len(ini) + 1
    b = txt.rindex("\n", 0, txt.index(fin)) + 1
    txt = txt[:a] + cuerpo + txt[b:]
    if not dry_run:
        open(SLOTS_RS, "w").write(txt)
    return len(objetos)


# ---------------------------------------------------------------------------


def contact_sheet(arr, hoja, objetos, ruta):
    """Las piezas elegidas, rotuladas con su nombre. **Se mira antes de escribir.**"""
    ancho = 1700
    filas, fila, x, alto = [], [], 0, 0
    for obj in objetos:
        for vid, label, p in obj.sprites:
            if x + p["w"] + 10 > ancho and fila:
                filas.append((fila, alto))
                fila, x, alto = [], 0, 0
            fila.append((label, p))
            x += p["w"] + 10
            alto = max(alto, p["h"])
    if fila:
        filas.append((fila, alto))
    cv = Image.new("RGB", (ancho, sum(a + 26 for _, a in filas)), (18, 18, 24))
    d = ImageDraw.Draw(cv)
    y = 0
    for fila, alto in filas:
        x = 0
        for label, p in fila:
            img = hoja.crop((p["x"], p["y"], p["x"] + p["w"], p["y"] + p["h"]))
            cv.paste(img.convert("RGB"), (x, y + alto - p["h"]), img)
            d.text((x + 1, y + alto + 3), label[:34], fill=(255, 210, 90))
            x += p["w"] + 10
        y += alto + 26
    cv.save(ruta)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--hoja", metavar="PNG", help="contact sheet rotulado, para revisar")
    args = ap.parse_args()

    hoja = Image.open(HOJA).convert("RGBA")
    arr = np.asarray(hoja)
    print(f"hoja: {hoja.size[0]}×{hoja.size[1]} px = {hoja.size[0]//CELL}×{hoja.size[1]//CELL} celdas")
    pzs = piezas(arr)
    verificar_estructura(arr, pzs)
    print(f"{len(pzs)} piezas segmentadas (islas de pixel, módulos de {MODULO} partidos)")

    elkit = kit(arr, pzs)
    print(f"{len(elkit)} piezas presentes en los {len(TEMAS)} temas → el kit")

    objetos, sobrantes = armar(arr, elkit)
    nsp = sum(len(o.sprites) for o in objetos)
    print(f"{len(objetos)} slots · {nsp} sprites")
    if sobrantes:
        print(f"{len(sobrantes)} piezas del kit sin entrada en GRAMATICA (no se exportan):")
        for pos, p in sobrantes:
            print(f"    off={pos[0]:5d} x={pos[1]:4d}  {p['w']/CELL:.2f}×{p['h']/CELL:.2f} celdas")

    if args.hoja:
        contact_sheet(arr, hoja, objetos, args.hoja)
        print(f"contact sheet: {args.hoja}")

    npag, ubicacion = empaquetar(arr, hoja, objetos, args.dry_run)
    print(f"{npag} páginas de atlas de {PAGE}×{PAGE}")
    ns, no = escribir_manifest(objetos, ubicacion, args.dry_run)
    print(f"manifest: {ns} sprites, {no} objetos con variantes")
    print(f"slots.rs: {escribir_slots(objetos, args.dry_run)} slots")
    if args.dry_run:
        print("(dry-run: no se escribió nada)")


if __name__ == "__main__":
    main()
