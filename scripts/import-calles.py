#!/usr/bin/env python3
"""**Las calles, con asfalto de verdad.**

    python3 scripts/import-calles.py

Hasta acá una carretera del juego era `Tile::Basalt` pintado del color plano del
basalto: gris parejo, sin textura, sin línea de carril, sin cordón. El pack traía
el asfalto y las veredas desde el primer día (`2_City_Terrains`, 351 sprites)
pero nadie los había conectado a los slots que el mundo dibuja
(`floor/asfalto`, `floor/vereda`), así que el manifest ni los mencionaba.

Este script trae **lo que se PINTA y se LEVANTA sobre la calzada**. El suelo en sí
—el asfalto y la baldosa de la vereda— ya no es asunto suyo: lo dibuja la malla
del chunk contra `art/pack/suelo.png`, y su dueño es `atlas-suelo.py`. Los tres
importadores se estuvieron pisando `floor/asfalto` y `floor/vereda` durante meses
(cada uno apuntando a otra página, y ganaba el último que corriera); ahora cada
slot tiene un solo dueño y el orden de las corridas deja de importar.

- **`road/linea`** — la línea pintada, en poses por CONEXIÓN (`ns`, `ew`, `ne`,
  `nse`, `cross`…). No es un autotile de región como el agua: una línea es una
  RED, y lo que decide el dibujo es hacia qué lados sigue. Se clasifican solas
  mirando en qué brazos del sprite hay píxeles claros (ver `conexiones`), que es
  más fiable que copiar a mano el orden de la hoja.
- **`road/cordon`** — el CORDÓN: el borde por donde la vereda se levanta sobre la
  calzada, en poses de autotile (`edge_n`, `corner_ne`, `inner_sw`…). Es lo que
  separa la vereda de la calle en una ciudad de verdad; sin él, el asfalto y la
  acera se tocaban a filo y la calle se leía como dos colores de piso pegados.
  Ver `recortar_pintura` para por qué va con fondo transparente.

Escribe sus propias entradas en el manifest y las anota en
`scripts/calles-import-index.json`, así una segunda corrida reemplaza lo suyo sin
tocar nada más.

"""

import json
import os
import re
from glob import glob

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACK = os.path.join(
    ROOT,
    "PRUEBAS-CODIGOS/modernexteriors-win/Modern_Exteriors_32x32/ME_Theme_Sorter_32x32",
    "2_City_Terrains_Singles_32x32",
)
ART = os.path.join(ROOT, "native/crates/ha-game/assets/art")
MANIFEST = os.path.join(ART, "manifest.json")
PAGINA_REL = "art/pack/calles.png"
INDEX = os.path.join(ROOT, "scripts/calles-import-index.json")
CELL = 32

# **El cordón, por qué lados sube la vereda.** Las 20 losas de 32×32 de la
# familia `Sidewalk_1` son el autotile del borde: el dibujo es casi todo asfalto
# y la vereda asoma por uno o dos lados. Se leyeron del pixel (una por una, a 8×8
# de resolución) y de ahí sale esta tabla — el nombre es el de `autotile_shape`
# en el cliente, donde el borde MIRA hacia el lado distinto.
#
# `center` no está a propósito: una celda de asfalto rodeada de asfalto no lleva
# cordón, y dibujarle un sprite transparente sería un quad por celda de calzada
# de toda la ciudad. Sin pose, el cliente no spawnea nada.
#: **La tabla leída a mano, que ahora es la PRUEBA del detector.**
#:
#: Estos doce índices se leyeron pixel a pixel de `Sidewalk_1` y son correctos;
#: el problema era que no servían para las otras cinco familias, porque cada una
#: numera sus piezas distinto (`Sidewalk_1` pone las esquinas en 11–14 y el resto
#: en 13–16). Copiarlos a mano seis veces es justo el trabajo que se equivoca
#: solo, así que la forma ahora se LEE del pixel (`forma_del_cordon`) — y esta
#: tabla se quedó como test: si el detector no la reproduce entera sobre la
#: familia 1, el script no publica nada. Ver `POSES`.
CORDON_A_MANO = {
    "edge_n": 19,      # la vereda está al norte: se ve la CARA del cordón
    "edge_s": 2,       # al sur: sólo el filo, el cordón mira para el otro lado
    "edge_w": 4,
    "edge_e": 8,
    "corner_nw": 13,
    "corner_ne": 14,
    "corner_sw": 16,
    "corner_se": 15,
    "inner_nw": 5,     # la vereda sólo asoma por la diagonal: la esquina cóncava
    "inner_ne": 7,
    "inner_sw": 3,
    "inner_se": 1,
}
#: Las doce poses que tiene que traer una familia para poder usarse. Son las de
#: `world2d::autotile_shape` menos `center`, que no lleva cordón a propósito.
POSES = tuple(CORDON_A_MANO)

#: **Las familias de vereda del pack.** Seis: crema, crema cálido, gris frío,
#: gris neutro, gris azulado y gris cálido. Cada CIUDAD usa una entera —baldosa y
#: cordón—, y cuál le toca lo decide el juego por el centro del asentamiento
#: (`world2d::vereda_de_la_ciudad`). Lo que no puede pasar es que se sortee por
#: celda: la baldosa la dibuja la malla del chunk y el escalón lo dibuja este
#: slot, así que un sorteo por celda pondría cordón crema sobre vereda azul cada
#: tres metros. Ése es el contrato con `atlas-suelo.py`, que numera sus
#: `floor/vereda_N` con estos mismos números.
FAMILIAS = (1, 2, 3, 4, 5, 6)
#: La familia de la primera ciudad conserva los nombres de slot SIN sufijo
#: (`floor/vereda`, `road/cordon`): son los que ya existían y los que el fallback
#: sigue pidiendo cuando una ciudad no tiene familia propia.
FAMILIA_BASE = 1


def singles(patron):
    fs = glob(os.path.join(PACK, patron))
    return sorted(fs, key=lambda f: [int(t) if t.isdigit() else t for t in re.split(r"(\d+)", f)])


def brillo(p):
    return (p[0] + p[1] + p[2]) / 3 if p[3] > 0 else 0


def recortar_contra(im, losa):
    """Deja lo que el pack dibujó ENCIMA de `losa`; lo que es `losa` se va a alfa.

    **El corte del cordón, y por qué no puede ser por brillo.** El primer intento
    conservaba lo más CLARO que el asfalto, y eso arrancaba justo lo que hace que
    una vereda se lea levantada: bajo la cara del cordón el pack pinta una sombra
    de contacto que es más OSCURA que la calzada (en la pieza 19 el asfalto está
    en 84 y la sombra baja a 68, ocho píxeles de degradado). Recortada por brillo,
    la vereda quedaba pegada al piso como una calcomanía de papel.

    Acá el fondo no se adivina: cada familia del pack trae su propia losa de
    asfalto lisa, y las piezas de borde están dibujadas SOBRE ella, pixel por
    pixel. Así que el fondo es exactamente eso — se borra lo idéntico y se queda
    todo el resto, claro y oscuro. Medido: de la pieza 19 sobrevive la mitad de
    arriba entera; de la 4, la columna de vereda con su sombra.
    """
    px, ref = im.load(), losa.load()
    w, h = im.size
    out = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    dst = out.load()
    for y in range(h):
        for x in range(w):
            if px[x, y] != ref[x, y]:
                dst[x, y] = px[x, y]
    return out


def recortar_pintura(im, margen=28.0, rampa=22.0):
    """Deja sólo lo que está PINTADO sobre el asfalto; el asfalto se vuelve alfa.

    **Por qué.** El pack dibuja la línea de carril y el cordón sobre su propia
    losa de asfalto, y esas losas vienen opacas al 100 %. Puestas encima de la
    calzada tapaban la celda entera con SU gris — y como la calzada sortea una
    variante por celda, la que llevaba línea se veía de otro color que sus dos
    vecinas: un parche rectangular cada pocos metros a lo largo de toda la
    avenida. Es lo que se reportó como "hay segmentos donde la línea se buguea".

    Recortada a alfa, el sprite deja de ser una losa y pasa a ser lo que siempre
    tuvo que ser: una CALCOMANÍA. Debajo se ve el asfalto de la celda, sea la
    variante que sea, y la línea deja de traer fondo propio.

    El corte se mide contra el fondo de CADA losa (su brillo mediano) y no contra
    un número fijo, porque las seis familias de vereda del pack tienen asfaltos
    de distinto gris. La `rampa` conserva el antialias del borde: sin ella la
    pintura queda con escalera dura y se nota más el pixel que el dibujo.
    """
    px = im.load()
    w, h = im.size
    fondo = sorted(brillo(px[x, y]) for x in range(w) for y in range(h))[w * h // 2]
    out = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    dst = out.load()
    for y in range(h):
        for x in range(w):
            r, g, b, a = px[x, y]
            if a == 0:
                continue
            sobra = brillo((r, g, b, a)) - (fondo + margen)
            alfa = max(0.0, min(1.0, sobra / rampa))
            if alfa > 0.0:
                dst[x, y] = (r, g, b, int(round(a * alfa)))
    return out


def slot_cordon(familia):
    """El slot del cordón de una familia. La base se queda sin sufijo: es la
    clave que ya existía y a la que cae cualquier ciudad sin familia propia."""
    return "road/cordon" if familia == FAMILIA_BASE else f"road/cordon_{familia}"


def losas_de(familia):
    """Las losas de 32×32 de una familia de vereda, por su número de pieza."""
    out = {}
    for f in singles(f"*Sidewalk_{familia}_*.png"):
        im = Image.open(f).convert("RGBA")
        if im.size == (CELL, CELL):
            out[int(re.search(r"_(\d+)\.png$", f).group(1))] = im
    return dict(sorted(out.items()))


def asfalto_liso_de(familia):
    """La losa de asfalto LISA de la familia: el fondo contra el que están
    dibujadas todas sus piezas de borde (ver `recortar_contra`).

    Gana la más PAREJA de las oscuras, no la más oscura. Una esquina cóncava es
    casi toda asfalto y por brillo medio le ganaba a la losa lisa — con eso,
    `Sidewalk_5` recortaba todas sus piezas contra una esquina y salía basura.
    """
    cand = []
    for i, im in losas_de(familia).items():
        px = im.load()
        vals = [px[x, y] for y in range(CELL) for x in range(CELL)]
        if any(p[3] < 255 for p in vals):
            continue
        b = [brillo(p) for p in vals]
        m = sum(b) / len(b)
        desv = (sum((x - m) ** 2 for x in b) / len(b)) ** 0.5
        if desv <= 14:
            cand.append((round(desv), m, i, im))
    if not cand:
        return None
    cand.sort()
    return cand[0][2], cand[0][3]


def forma_del_cordon(im, asfalto):
    """Qué pose de autotile es esta losa, leyendo DÓNDE está la vereda.

    La acera es lo que en la losa está más CLARO que el asfalto liso de su propia
    familia. Con eso, la pose sale de la geometría y no de un índice copiado a
    mano: un lado entero de acera es un `edge_`, dos lados adyacentes son un
    `corner_`, y una cuña suelta en una diagonal es una esquina cóncava
    (`inner_`). Devuelve `None` para lo que no lleva cordón: la losa de asfalto
    limpio y la de vereda entera.

    Validado contra [`CORDON_A_MANO`]: sobre `Sidewalk_1` da las doce.
    """
    px, ref = im.load(), asfalto.load()
    base = sum(brillo(ref[x, y]) for y in range(CELL) for x in range(CELL)) / (CELL * CELL)
    acera = [
        (x, y)
        for y in range(CELL)
        for x in range(CELL)
        if px[x, y][3] == 255 and brillo(px[x, y]) > base + 22
    ]
    total = len(acera) / (CELL * CELL)
    # Menos de medio por ciento es ruido de antialias; más del 90 % es la losa de
    # vereda entera, que va por la malla del chunk y no lleva escalón.
    if total < 0.005 or total > 0.90:
        return None
    k = 6
    puesta = set(acera)
    franja = {
        "n": {(x, y) for y in range(k) for x in range(k, CELL - k)},
        "s": {(x, y) for y in range(CELL - k, CELL) for x in range(k, CELL - k)},
        "w": {(x, y) for x in range(k) for y in range(k, CELL - k)},
        "e": {(x, y) for x in range(CELL - k, CELL) for y in range(k, CELL - k)},
    }
    lados = [d for d, cs in franja.items() if len(puesta & cs) / len(cs) > 0.55]
    if len(lados) == 1:
        return f"edge_{lados[0]}"
    if len(lados) == 2:
        for a, b in (("n", "e"), ("n", "w"), ("s", "e"), ("s", "w")):
            if set(lados) == {a, b}:
                return f"corner_{a}{b}"
        return None  # dos lados opuestos: eso es un pasaje, no una esquina
    if not lados:
        # La vereda asoma sólo por una DIAGONAL. El cuadrante lo dice el centroide
        # de lo pintado, que no depende de qué tan chica sea la cuña — en este
        # pack son unos veinte píxeles y una ventana de esquina fija no la caza.
        cx = sum(p[0] for p in acera) / len(acera)
        cy = sum(p[1] for p in acera) / len(acera)
        return f"inner_{'n' if cy < CELL / 2 else 's'}{'w' if cx < CELL / 2 else 'e'}"
    return None


def cordon_de(familia):
    """Las doce poses del cordón de una familia, ya recortadas a alfa.

    `None` si a la familia le falta alguna: media familia no se publica. Una
    ciudad con `edge_n` faltante tendría un hueco en cada cordón que mira al
    norte, y eso se ve peor que usar la vereda de la ciudad de al lado.
    """
    liso = asfalto_liso_de(familia)
    if liso is None:
        return None, "no tiene losa de asfalto lisa"
    idx_liso, asfalto = liso
    hallado = {}
    for i, im in losas_de(familia).items():
        if i == idx_liso:
            continue
        pose = forma_del_cordon(im, asfalto)
        # `setdefault`: el pack repite la misma forma varias veces y con una
        # alcanza. La primera que llega se queda con la pose.
        if pose:
            hallado.setdefault(pose, im)
    faltan = [p for p in POSES if p not in hallado]
    if faltan:
        return None, f"le faltan {', '.join(faltan)}"
    poses = {p: recortar_contra(hallado[p], asfalto) for p in POSES}
    # LA CUNETA. Ver `rejilla_de`.
    rejilla = rejilla_de(familia, idx_liso, asfalto)
    if rejilla is not None:
        poses[POSE_REJILLA] = rejilla
    return poses, None


#: La pose del cordón que trae la **rejilla de la cuneta** dibujada en su cara.
#: Sólo existe para `edge_n`, que es el único borde donde se ve la CARA del
#: cordón — en los otros tres se ve el filo y no habría dónde ponerla.
POSE_REJILLA = "edge_n_rejilla"


def rejilla_de(familia, idx_liso, asfalto):
    """La losa de cordón norte que trae la REJILLA de la cuneta, si la hay.

    **De acá salió un bug de meses.** La tabla leída a mano le daba a `edge_n` la
    pieza 19, que no es un cordón liso: es el cordón **con la rejilla del
    sumidero**. Como el autotile la usa en toda celda de calzada que tiene vereda
    al norte, cada ciudad del mundo tenía una alcantarilla cada 95 cm, a lo largo
    de todas sus cuadras. El cordón liso (la 6) estaba al lado y nunca se usó.

    Ahora la lisa es la pose normal —el detector la encuentra primero— y ésta
    entra con PERIODO, que es lo que una cuneta es: un sumidero cada tantos
    metros. La elige `world2d::spawn_chunk_cordon`.

    Se reconoce por el pixel: la rejilla son barrotes verticales oscuros bajo el
    filo del cordón, o sea muchos CAMBIOS de claro a oscuro a lo largo de una
    fila. El cordón liso no tiene ninguno.
    """
    mejor = None
    for i, im in losas_de(familia).items():
        if i == idx_liso or forma_del_cordon(im, asfalto) != "edge_n":
            continue
        px = im.load()
        # **La medida es la ALTERNANCIA de una fila, no el contraste.** Los
        # barrotes están dibujados con cuatro puntos de brillo de diferencia
        # (68 contra 72 en la familia 1), así que cualquier corte por contraste
        # los pierde — el primer intento los perdió. Lo que ninguna otra pieza de
        # esta hoja hace es CAMBIAR de tono ocho veces a lo ancho de una fila: un
        # borde vertical cambia dos, el asfalto liso ninguna, y la rejilla de
        # ocho barrotes, quince.
        picos = 0
        for y in range(CELL):
            v = [brillo(px[x, y]) for x in range(CELL)]
            m = sum(v) / CELL
            fila = [x > m for x in v]
            picos = max(picos, sum(1 for a, b in zip(fila, fila[1:]) if a != b))
        if mejor is None or picos > mejor[0]:
            mejor = (picos, i, im)
    if mejor is None or mejor[0] < 10:
        return None
    return recortar_contra(mejor[2], asfalto)


#: **La cebra del pack, y de dónde sale.** Cada familia de vereda trae el paso de
#: peatones dibujado en dos orientaciones y en tres tramos: el que se pega a la
#: vereda de un lado, los del medio, y el del otro lado. Los de los EXTREMOS
#: traen su pedazo de acera pintado, y ésos no se usan — esa acera ya la dibuja
#: la malla del chunk con la familia de la ciudad, y el escalón ya lo pone
#: `road/cordon`. Superponerlos metería una vereda crema dentro de una ciudad
#: gris. Así que entran sólo los tramos del MEDIO, recortados a pura pintura:
#: una calcomanía de rayas sobre el asfalto que haya debajo.
#:
#: `_ns` son las barras verticales (cruzan una calle norte-sur) y `_ew` las
#: horizontales, la misma convención que el respaldo dibujado por código
#: (`procedural_art::zebra`).
CEBRA = {
    # clave del slot: (pieza del pack, qué celda de la pieza recortar)
    "road/senda_ns": (35, (0, 0)),   # 32×64, barras verticales → celda de arriba
    "road/senda_ew": (30, (0, 0)),   # 64×32, barras horizontales → celda izquierda
}


def cebra_de(familia):
    """Las dos losas de cebra de una familia, recortadas a pintura.

    Devuelve `{clave: imagen}` de 32×32. `None` si a esta familia le faltan las
    piezas — el respaldo por código sigue funcionando y nadie se queda sin senda.
    """
    liso = asfalto_liso_de(familia)
    if liso is None:
        return None
    _, asfalto = liso
    out = {}
    for clave, (pieza, (cx, cz)) in CEBRA.items():
        f = os.path.join(PACK, f"ME_Singles_City_Terrains_32x32_Sidewalk_{familia}_{pieza}.png")
        if not os.path.exists(f):
            return None
        im = Image.open(f).convert("RGBA")
        celda = im.crop((cx * CELL, cz * CELL, (cx + 1) * CELL, (cz + 1) * CELL))
        # Contra el asfalto de su propia familia, igual que el cordón: lo que
        # queda es exactamente lo que el pack pintó encima. Y después, sólo lo
        # CLARO: en el borde de la pieza sobreviven cuatro píxeles del cordón
        # vecino —apenas más claros que la calzada— y puestos sobre la calle se veían
        # como una mancha sucia en la esquina de cada senda.
        recorte = recortar_contra(celda, asfalto)
        px = recorte.load()
        fondo = sum(
            brillo(asfalto.load()[x, y]) for y in range(CELL) for x in range(CELL)
        ) / (CELL * CELL)
        for y in range(CELL):
            for x in range(CELL):
                if px[x, y][3] and brillo(px[x, y]) < fondo + 60:
                    px[x, y] = (0, 0, 0, 0)
        out[clave] = recorte
    return out


def conexiones(im):
    """Hacia qué lados sale la línea pintada de este sprite: `"nsew"` en orden.

    La marca no llega al borde de la celda —son rayas con margen, así se ven
    discontinuas como en la calle—, así que mirar la fila 0 no sirve: se mira el
    BRAZO, la franja central a un tercio de camino del borde.
    """
    px = im.load()
    w, h = im.size
    fondo = sorted(brillo(px[x, y]) for x in range(w) for y in range(h))[w * h // 2]
    claro = lambda x, y: brillo(px[x, y]) > fondo + 25  # noqa: E731
    medio = range(w * 7 // 16, w * 9 // 16)
    mediov = range(h * 7 // 16, h * 9 // 16)
    n = any(claro(x, y) for x in medio for y in range(1, h // 4))
    s = any(claro(x, y) for x in medio for y in range(h * 3 // 4, h - 1))
    w_ = any(claro(x, y) for y in mediov for x in range(1, w // 4))
    e = any(claro(x, y) for y in mediov for x in range(w * 3 // 4, w - 1))
    return "".join(c for c, v in (("n", n), ("s", s), ("e", e), ("w", w_)) if v)


def main():
    lineas = {}
    for f in singles("*Asphalt*.png"):
        im = Image.open(f).convert("RGBA")
        con = conexiones(im)
        if con:
            # La primera que llega se queda con la pose: el pack repite varias
            # veces la raya recta y no hace falta más que una de cada forma.
            lineas.setdefault(con, im)

    # EL CORDÓN, familia por familia. La forma de cada losa —por qué lados sube
    # la vereda— se LEE del pixel (`forma_del_cordon`), y la tabla que se había
    # leído a mano para `Sidewalk_1` se quedó como prueba del detector: si no la
    # reproduce entera, acá se corta. Es la única forma de abrir las seis
    # familias sin copiar sesenta y dos índices a mano.
    liso_1 = asfalto_liso_de(FAMILIA_BASE)
    if liso_1 is None:
        raise SystemExit("la familia base no tiene asfalto liso: ¿cambió el pack?")
    _, asfalto_1 = liso_1
    losas_1 = losas_de(FAMILIA_BASE)
    mal = [
        f"{pose}→{i} da {forma_del_cordon(losas_1[i], asfalto_1)}"
        for pose, i in CORDON_A_MANO.items()
        if forma_del_cordon(losas_1[i], asfalto_1) != pose
    ]
    if mal:
        raise SystemExit("el detector de cordón no reproduce la tabla a mano: " + "; ".join(mal))

    cordones, descartadas = {}, []
    for fam in FAMILIAS:
        poses, motivo = cordon_de(fam)
        if poses is None:
            descartadas.append(f"Sidewalk_{fam} ({motivo})")
            continue
        cordones[fam] = poses

    if FAMILIA_BASE not in cordones:
        raise SystemExit("sin la familia base no hay cordón: el pack cambió")

    # LA CEBRA. Es la misma para todas las ciudades porque es pintura sobre el
    # asfalto, no vereda: recortada así no arrastra el color de ninguna familia.
    cebra = cebra_de(FAMILIA_BASE) or {}
    if cebra:
        print(f"cebra: {sorted(cebra)} (recortada a pintura)")
    else:
        print("cebra: el pack no la trae — queda el respaldo por código")

    # La línea también va recortada: ver `recortar_pintura`.
    lineas = {con: recortar_pintura(im) for con, im in lineas.items()}

    print(f"líneas: {sorted(lineas)}")
    con_rejilla = [f for f, p in cordones.items() if POSE_REJILLA in p]
    print(f"cordón: {len(cordones)} familias {sorted(cordones)} × {len(POSES)} poses")
    print(f"cuneta: rejilla en las familias {con_rejilla}")
    if descartadas:
        print("  sin usar: " + " · ".join(descartadas))
    if "ns" not in lineas or "ew" not in lineas:
        raise SystemExit("el pack no trae lo que hace falta para las calles")

    # Una sola página: 32 columnas de 32 px = 1024. Entra todo de sobra.
    items = (
        [("road/linea", con, im) for con, im in sorted(lineas.items())]
        + [
            (slot_cordon(fam), pose, im)
            for fam in sorted(cordones)
            for pose, im in sorted(cordones[fam].items())
        ]
        + [(clave, "", im) for clave, im in sorted(cebra.items())]
    )
    cols = 32
    pagina = Image.new("RGBA", (cols * CELL, ((len(items) + cols - 1) // cols) * CELL), (0, 0, 0, 0))
    sitio = {}
    for i, (clave, sub, im) in enumerate(items):
        col, row = i % cols, i // cols
        pagina.paste(im, (col * CELL, row * CELL))
        sitio[(clave, sub)] = (col, row)
    os.makedirs(os.path.dirname(os.path.join(ART, "..", PAGINA_REL)), exist_ok=True)
    pagina.save(os.path.join(ROOT, "native/crates/ha-game/assets", PAGINA_REL), optimize=True)

    with open(MANIFEST) as fh:
        man = json.load(fh)
    sprites, objects = man.setdefault("sprites", {}), man.setdefault("objects", {})
    if os.path.exists(INDEX):
        with open(INDEX) as fh:
            previo = json.load(fh)
        for k in previo.get("sprites", []):
            sprites.pop(k, None)
        for k in previo.get("objects", []):
            objects.pop(k, None)

    def entrada(col, row):
        return {
            "png": PAGINA_REL,
            "grid": [CELL, CELL],
            "rect": {"col": col, "row": row, "w": 1, "h": 1},
            "layer": "Floor",
        }

    nuevos_s, nuevos_o = [], []

    def slot_con_poses(clave, sub_por_pose, base):
        """Un slot con poses, todas en esta página. `base` es la pose de reposo."""
        poses = {
            nombre: {"rect": {"col": c, "row": r, "w": 1, "h": 1}}
            for nombre, (c, r) in sub_por_pose.items()
        }
        col, row = sub_por_pose[base]
        e = entrada(col, row)
        e["poses"] = poses
        sprites[clave] = e
        nuevos_s.append(clave)

    # La línea pintada es UN slot con poses por conexión: el juego pide
    # `pose("road/linea", "nse")` y dibuja el empalme correcto.
    slot_con_poses(
        "road/linea",
        {con: sitio[("road/linea", con)] for con in lineas},
        "ns",
    )
    # El cordón es un AUTOTILE, no una red: sus poses son las de
    # `world2d::autotile_shape`, y el cliente le pregunta a la celda de asfalto
    # por qué lados tiene vereda. Sin `center` a propósito — ver `CORDON_A_MANO`.
    # Un slot por familia: la ciudad elige cuál (`world2d::vereda_de_la_ciudad`).
    for fam, poses in cordones.items():
        clave = slot_cordon(fam)
        slot_con_poses(clave, {pose: sitio[(clave, pose)] for pose in poses}, "edge_n")

    # La cebra es una losa sola, sin poses: la orientación ya está en la clave
    # (`_ns` / `_ew`), que es lo que pregunta `spawn_chunk_lane_marks`.
    for clave in cebra:
        col, row = sitio[(clave, "")]
        sprites[clave] = entrada(col, row)
        nuevos_s.append(clave)

    with open(MANIFEST, "w") as fh:
        json.dump(man, fh, indent=1, ensure_ascii=False, sort_keys=True)
    with open(INDEX, "w") as fh:
        json.dump({"sprites": nuevos_s, "objects": nuevos_o}, fh, indent=1)
    print(f"manifest: +{len(nuevos_s)} sprites, {len(nuevos_o)} objetos con variantes")


if __name__ == "__main__":
    main()
