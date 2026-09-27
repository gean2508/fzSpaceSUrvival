#!/usr/bin/env python3
"""**El kit de casas MODULARES de `24_Additional_Houses`, medido y publicado.**

    python3 scripts/import-casas.py --dry-run
    python3 scripts/import-casas.py

## Por qué existe

`24_Additional_Houses` no trae sólo casas enteras: trae un **kit modular**, que
es la respuesta del pack a "esta misma casa, más grande o más chica".

    ┌──────────┬──────────┬────────────┐
    │  tramo   │  tramo   │   remate   │  tramo  = 10 celdas, se repite
    │ (10 c.)  │ (10 c.)  │  (12 c.)   │  remate = cierra la fila por la derecha,
    └──────────┴──────────┴────────────┘            con su pared de costado

Hasta hoy esas piezas entraban al mundo **sueltas**, porque
`import-pack-masivo.py` recorta la carpeta `*_Singles_32x32` sin preguntarse para
qué se dibujó cada archivo e `import-edificios.py` elegía la fachada de cada lote
por TAMAÑO. Dos resultados medidos:

- `building/casa_chica` —la casa más común del mapa— era
  `casa_base_en_terrazas_costado`: **las dos paredes de costado apiladas, sin
  techo** (el empaquetador las dejó pegadas en un recorte de 10×12 que apoya,
  llena su caja y pasa todos los filtros de pixel).
- Las cuatro `casa_en_terrazas_*` que le tocaban al barrio son REMATES de hilera
  plantados solos, con el techo cortado a cuchillo por la izquierda, que es por
  donde iba la casa vecina.

Este script publica el kit COMO KIT, y la puerta ANIMADA que le corresponde a
cada diseño, para que `ha-game::building` componga la fachada del ancho exacto de
la casa que generó la ciudad y le clave la puerta encima sin un pixel de
corrimiento.

## Lo que se mide (nada de esto está escrito a ojo)

1. **Dónde vive cada pieza en las páginas del atlas.** No se le cree al
   importador masivo: cada single del pack se BUSCA por pixel exacto dentro de
   `art/pack/additional_houses_*.png`. Si el pack o el empaquetado cambian, la
   corrida se planta en vez de publicar un rect viejo.
2. **Qué puerta animada es la de cada casa, y en qué píxel va.** Las hojas de
   `Animated_sheets_32x32/Additional_Houses_Door_*.png` son tiras de cuadros de
   2×3 celdas. El cuadro 0 de cada hoja se busca —comparando SÓLO sus píxeles
   opacos, porque alrededor del batiente hay aire y en la casa eso es pared—
   dentro de cada pieza: donde calza, ésa es su puerta y ése es su sitio.
3. **Cuál es el cuadro de "abierta".** El más distinto del cuadro 0. De ahí salen
   las cuatro poses que el juego ya sabe reproducir (`building::animate_doors`):
   `closed`, `opening`, `open`, `closing`.

## Cómo viaja un offset en píxeles dentro del manifest

Un `rect` del manifest se dice en CELDAS, y las puertas del pack **no caen en la
grilla de 32** (la de `Terraced_House_Modular_2` arranca en el píxel 204, o sea
6,375 celdas). No hace falta inventar un campo: una `Pose` puede declarar su
propia `grid` (está ahí para que convivan hojas de 16 y de 32 px), así que las
poses `puerta_en_tramo` / `puerta_en_remate` se publican con `grid: [1, 1]` y su
rect ES el rect en píxeles dentro de la página. El cliente le resta el origen de
la pieza y tiene el offset exacto.

## Qué escribe

- `art/pack/casas_puertas.png` — UNA página con las tiras de puerta. Es lo único
  que copia píxeles, y son ocho tiras de 3 celdas de alto: nada al lado de las
  páginas de casas, que se referencian donde ya están.
- `building/casa_hilera` (+ variantes `#1`…`#6`) en el manifest, con las poses
  `tramo`, `remate`, `puerta_en_*` y las cuatro del batiente.
- `scripts/casas-import-index.json` — lo suyo, para pisarse sólo a sí mismo.

**Orden**: después de `import-pack-masivo.py` (que arma las páginas) y de
`import-animados.py`. No pisa nada de `import-edificios.py`: aquél reparte
fachadas ENTERAS entre los lotes que ya existían, éste publica un slot propio.
"""

import argparse
import json
import os
import re

import numpy as np
from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ASSETS = os.path.join(ROOT, "native/crates/ha-game/assets")
MANIFEST = os.path.join(ASSETS, "art/manifest.json")
INDEX = os.path.join(ROOT, "scripts/casas-import-index.json")
PACK = os.path.join(ROOT, "PRUEBAS-CODIGOS/modernexteriors-win/Modern_Exteriors_32x32")
SINGLES = os.path.join(PACK, "ME_Theme_Sorter_32x32/24_Additional_Houses_Singles_32x32")
ANIMADAS = os.path.join(PACK, "Animated_32x32/Animated_sheets_32x32")
PAGINAS = os.path.join(ASSETS, "art/pack")
PUERTAS_REL = "art/pack/casas_puertas.png"

CELL = 32
OPACO = 16
#: Ancho MÍNIMO de la página de puertas, en píxeles. El resto del pack usa 1024,
#: pero acá manda la tira más ancha: `Additional_Houses_Door_2` mide 1344 px (14
#: cuadros de 3 celdas) y con la página clavada en 1024 la tira **se cortaba** —
#: `Image.paste` recorta sin avisar—, así que los cuadros de `closing` apuntaban
#: fuera de la imagen y el juego dibujaba un bloque plano encima de la fachada.
PAGINA_MIN = 1024
#: Cuadros por segundo del batiente. La hoja trae el ciclo entero (abrir y
#: cerrar); a 24 fps cada mitad dura ~0,3 s, que es lo que tarda una puerta.
FPS_PUERTA = 24.0

#: Clave del slot que publica este script. Es un PROGRAMA DE LOTE
#: (`city::LOT_PROGRAMS`), no una variante de los que ya había: así el barrio
#: gana la casa en hilera de verdad y ningún otro importador le pisa las
#: variantes.
SLOT = "building/casa_hilera"
LABEL = "Casa en hilera"


# ---------------------------------------------------------------------------
# EL KIT — qué archivo del pack es cada pieza.
#
# Es la ÚNICA lista escrita a mano de todo el script, y es la que no se puede
# medir: que `Terraced_House_Modular_3` sea un tramo y `Terraced_House_3` su
# remate lo dice el nombre que le puso el autor, no el pixel. Todo lo demás
# —dónde está, cuánto mide, dónde tiene la puerta— sale de mirar la imagen.
# ---------------------------------------------------------------------------

HILERA = [
    ("Terraced_House_Modular_1", "Terraced_House_1"),
    ("Terraced_House_Modular_2", "Terraced_House_2"),
    ("Terraced_House_Modular_3", "Terraced_House_3"),
    ("Terraced_House_Modular_4", "Terraced_House_4"),
    ("Terraced_House_Modular_5", "Terraced_House_5"),
    ("Terraced_House_Modular_6", "Terraced_House_6"),
]

#: **Casas ENTERAS con puerta animada.** No son un kit —se plantan de una— pero
#: la puerta se les clava igual, y es la mitad de lo que hace que un barrio se lea
#: vivo. Sus fachadas las reparte `import-edificios.py` entre los lotes; acá sólo
#: se les cuelga el batiente encima, en el pixel donde el pack lo dibujó cerrado.
ENTERAS = [
    "Modern_House",
    "Modern_House_Unadorned",
    "One_Story_House",
    "Post_Apocalyptic_House_1",
    "Post_Apocalyptic_House_2",
    "Japanese_House",
    "Country_House",
    "Country_House_No_Banisters",
    "Haunted_House",
    "Haunted_House_No_Lantern",
    "Victorian_House_1",
    "Victorian_House_2",
    "Victorian_House_3",
    "Victorian_House_4",
    "Victorian_House_5",
    "Victorian_House_6",
    "Victorian_House_7",
]

#: Las hojas de puerta animada del tema. Se buscan TODAS dentro de TODAS las
#: piezas: quién va con quién lo dice el pixel.
HOJAS_DE_PUERTA = [
    "Additional_Houses_Door_2",
    "Additional_Houses_Door_3",
    "Additional_Houses_Door_4",
    "Additional_Houses_Door_Japanese_Suburban_House",
    "Additional_Houses_Door_Modern_House",
    "Additional_Houses_Door_One_Story_House",
    "Additional_Houses_Door_Post_Apocalyptic_House",
    "Additional_Houses_Door_Victorian_House",
]

#: Anchos de cuadro que se prueban, en celdas. Las hojas no traen separación
#: entre cuadros, así que el ancho hay que MEDIRLO (ver [`ancho_de_cuadro`]).
ANCHOS_DE_CUADRO = (2, 3, 4, 6)


# ---------------------------------------------------------------------------
# Herramientas de medición
# ---------------------------------------------------------------------------


def png(ruta):
    return np.array(Image.open(ruta).convert("RGBA"))


def single(nombre):
    return png(os.path.join(SINGLES, f"24_Additional_Houses_{nombre}_32x32.png"))


def buscar_exacto(grande, chico):
    """`(x, y)` donde `chico` calza pixel a pixel dentro de `grande`.

    Se busca por la FILA 0 del parche serializada a bytes: `bytes.find` está en C
    y recorre la página entera en un pestañeo, mientras que comparar ventana por
    ventana con numpy tarda minutos por página. La verificación completa se hace
    sólo sobre los candidatos que esa fila ya aprobó.
    """
    alto_g, ancho_g = grande.shape[:2]
    alto_c, ancho_c = chico.shape[:2]
    if alto_c > alto_g or ancho_c > ancho_g:
        return None
    aguja = chico[0].tobytes()
    for y in range(alto_g - alto_c + 1):
        fila = grande[y].tobytes()
        i = fila.find(aguja)
        while i >= 0:
            x = i // 4
            if i % 4 == 0 and x + ancho_c <= ancho_g:
                if np.array_equal(grande[y : y + alto_c, x : x + ancho_c], chico):
                    return (x, y)
            i = fila.find(aguja, i + 1)
    return None


def tramo_opaco_mas_largo(parche):
    """`(dy, dx, n)`: la corrida horizontal de píxeles opacos más larga.

    Es la aguja con la que se busca un parche que tiene transparencias: comparar
    su fila 0 no sirve cuando esa fila es aire.
    """
    op = parche[:, :, 3] > OPACO
    mejor = (0, 0, 0)
    for y in range(parche.shape[0]):
        x0 = None
        for x, v in enumerate(list(op[y]) + [False]):
            if v and x0 is None:
                x0 = x
            elif not v and x0 is not None:
                if x - x0 > mejor[2]:
                    mejor = (y, x0, x - x0)
                x0 = None
    return mejor


def ancho_de_cuadro(arr):
    """**Cuántas celdas mide un cuadro de esta tira**, medido por continuidad.

    Las hojas de animación del pack no separan los cuadros con aire, así que el
    ancho no se lee: se deduce. Lo primero que se probó fue "el primer ancho que
    divida la tira y cuyo cuadro 0 aparezca dentro de una casa", y está MAL:
    `Additional_Houses_Door_2` mide 42 celdas y sus cuadros son de 3, pero las
    dos primeras celdas de un cuadro de 3 también contienen la puerta cerrada, así
    que 2 "calzaba" y la tira quedaba cortada en diagonal.

    Lo que separa el ancho bueno de sus tajadas es que **una animación es
    continua**: dos cuadros seguidos se parecen. Cortada con el ancho equivocado,
    la misma tira salta (medido en `Door_2`: con 2 celdas los saltos entre
    cuadros vecinos son 4× más grandes que con 3). Así que se elige el ancho que
    minimiza el salto medio entre cuadros consecutivos.
    """
    ancho_px = arr.shape[1]
    mejor = None
    for celdas in ANCHOS_DE_CUADRO:
        paso = celdas * CELL
        if ancho_px % paso or ancho_px // paso < 3:
            continue
        n = ancho_px // paso
        cuadros = [arr[:, k * paso : (k + 1) * paso].astype(int) for k in range(n)]
        salto = sum(
            np.abs(cuadros[k] - cuadros[(k + 1) % n]).mean() for k in range(n)
        ) / n
        if mejor is None or salto < mejor[1]:
            mejor = (celdas, salto)
    return None if mejor is None else mejor[0]


def buscar_opaco(grande, chico):
    """`(x, y)` donde los píxeles OPACOS de `chico` calzan dentro de `grande`."""
    op = chico[:, :, 3] > OPACO
    dy, dx, largo = tramo_opaco_mas_largo(chico)
    if largo < 8:
        return None
    aguja = chico[dy, dx : dx + largo].tobytes()
    alto_g, ancho_g = grande.shape[:2]
    alto_c, ancho_c = chico.shape[:2]
    if alto_c > alto_g or ancho_c > ancho_g:
        return None
    for y in range(alto_g - alto_c + 1):
        fila = grande[y + dy].tobytes()
        i = fila.find(aguja)
        while i >= 0:
            x = i // 4 - dx
            if i % 4 == 0 and 0 <= x and x + ancho_c <= ancho_g:
                sub = grande[y : y + alto_c, x : x + ancho_c]
                if np.array_equal(sub[op], chico[op]):
                    return (x, y)
            i = fila.find(aguja, i + 1)
    return None


def solapa(izq, der, maximo=1):
    """**Cuánto se pisan dos módulos vecinos**, en celdas.

    Los módulos del pack NO se pegan al ras: el autor dibujó cada uno con medio
    sangrado a cada lado, así que puestos uno al lado del otro **queda una ranura
    por la que se ve el fondo**. Sobre fondo oscuro pasa por sombra entre casas y
    sobre el pasto de una parcela canta.

    La medición es literalmente esa: se pegan las piezas con solapa 0, 1, 2… y se
    busca la más chica que no deje **fondo encerrado** en la costura — píxeles
    transparentes con dibujo a la izquierda Y a la derecha en la misma fila. El
    aire de arriba del techo o el de los costados no cuenta, porque no está
    encerrado: eso es cielo, y tiene que seguir siendo cielo.
    """
    alto = max(izq.shape[0], der.shape[0])

    def agujeros(s):
        ancho = izq.shape[1] + der.shape[1] - s * CELL
        lienzo = np.zeros((alto, ancho), bool)
        lienzo[alto - izq.shape[0] :, : izq.shape[1]] |= izq[:, :, 3] > OPACO
        x = izq.shape[1] - s * CELL
        lienzo[alto - der.shape[0] :, x : x + der.shape[1]] |= der[:, :, 3] > OPACO
        # Fondo ENCERRADO: entre el primer y el último pixel dibujado de la fila.
        idx = np.arange(ancho)
        n = 0
        a, b = max(x - CELL, 0), min(x + CELL, ancho)
        for fila in lienzo:
            if not fila.any():
                continue
            i0, i1 = idx[fila][0], idx[fila][-1]
            hueco = ~fila[i0 : i1 + 1]
            if not hueco.any():
                continue
            xs = idx[i0 : i1 + 1][hueco]
            n += int(((xs >= a) & (xs < b)).sum())
        return n

    for s in range(maximo + 1):
        if agujeros(s) == 0:
            return s
    return min(range(maximo + 1), key=agujeros)


# ---------------------------------------------------------------------------
# 1. Dónde vive cada pieza dentro de las páginas del atlas
# ---------------------------------------------------------------------------


def paginas_del_tema():
    out = {}
    for f in sorted(os.listdir(PAGINAS)):
        if re.fullmatch(r"additional_houses_\d+\.png", f):
            out[f"art/pack/{f}"] = png(os.path.join(PAGINAS, f))
    if not out:
        raise SystemExit(
            "no hay páginas art/pack/additional_houses_*.png — corré antes "
            "scripts/import-pack-masivo.py"
        )
    return out


def ubicar(nombre, paginas):
    """`(png, origen_px, arr)` de un single del pack dentro del atlas.

    Se planta si no lo encuentra o si el empaquetador lo dejó desalineado de la
    grilla: un rect de pieza se dice en CELDAS, así que media celda de
    corrimiento no se puede escribir y saldría un recorte movido.
    """
    arr = single(nombre)
    for ruta, pag in paginas.items():
        hit = buscar_exacto(pag, arr)
        if hit is None:
            continue
        x, y = hit
        if x % CELL or y % CELL:
            raise SystemExit(
                f"{nombre} quedó en {ruta} en el pixel ({x},{y}), que no cae en la "
                "grilla de 32: el rect del manifest no lo puede decir"
            )
        return ruta, (x, y), arr
    raise SystemExit(
        f"{nombre} no aparece en ninguna página de additional_houses: "
        "¿cambió el pack o el empaquetado?"
    )


# ---------------------------------------------------------------------------
# 2. Las puertas animadas: de qué pieza es cada hoja y en qué pixel va
# ---------------------------------------------------------------------------


class Puerta:
    """Una tira de puerta ya medida, con las piezas donde calza."""

    def __init__(self, nombre, arr, w, h, cuadros, abierta, donde):
        self.nombre = nombre
        self.arr = arr
        self.w = w
        self.h = h
        self.cuadros = cuadros
        self.abierta = abierta
        self.donde = donde  # nombre de pieza -> (x, y) en px del recorte
        self.rect = None    # dónde quedó en `casas_puertas.png`


def medir_puertas(piezas):
    """Las hojas de puerta, con su ancho de cuadro y dónde calza cada una.

    `piezas` es `{nombre: array}`. Las que no calzan en ninguna se listan y no se
    publican: una puerta que no se sabe dónde va sería un sprite flotando.
    """
    out = []
    for nombre in HOJAS_DE_PUERTA:
        ruta = os.path.join(ANIMADAS, f"{nombre}_32x32.png")
        if not os.path.exists(ruta):
            print(f"  · {nombre}: no está en el pack")
            continue
        arr = png(ruta)
        celdas, alto = arr.shape[1] // CELL, arr.shape[0] // CELL
        ancho = ancho_de_cuadro(arr)
        if ancho is None:
            print(f"  · {nombre}: {celdas} celdas, no las divide ningún ancho de cuadro")
            continue
        cuadro0 = arr[:, : ancho * CELL]
        donde = {n: h for n, a in piezas.items() if (h := buscar_opaco(a, cuadro0))}
        if not donde:
            print(f"  · {nombre}: no calza en ninguna casa del tema — no se publica")
            continue
        n = celdas // ancho
        abierta = cuadro_abierto(arr, ancho, n)
        out.append(Puerta(nombre, arr, ancho, alto, n, abierta, donde))
        print(
            f"  · {nombre}: {n} cuadros de {ancho}×{alto} celdas, abierta en el "
            f"{abierta} — calza en {', '.join(sorted(donde))}"
        )
    return out


def cuadro_abierto(arr, ancho, n):
    """**En qué cuadro la puerta está abierta del todo.**

    Lo obvio —"el cuadro más distinto del cerrado"— NO sirve: un batiente a medio
    abrir cruza el vano con su hoja y cambia más píxeles que uno abierto del todo,
    que es un agujero oscuro. Medido en `Door_3`, del cuadro 2 al 12 la diferencia
    contra el cerrado es plana (±8 %), así que el máximo lo decidía el ruido.

    Lo que sí se puede medir es la FORMA de la tira: estas hojas son de ida y
    vuelta, o sea un palíndromo (el cuadro k se parece al n-k). Si lo es —y se
    comprueba, no se supone—, el vértice está en el medio y ése es el cuadro
    abierto. Si no lo es, la tira es un ciclo de otra clase y se cae al máximo,
    que para eso está.
    """
    paso = ancho * CELL
    cuadro = [arr[:, k * paso : (k + 1) * paso].astype(int) for k in range(n)]
    escala = max(1.0, float(np.abs(cuadro[0] - cuadro[n // 2]).mean()))
    espejo = max(
        float(np.abs(cuadro[k] - cuadro[(n - k) % n]).mean()) for k in range(1, n // 2 + 1)
    )
    if espejo < escala * 0.15:
        return n // 2
    difs = [float(np.abs(c - cuadro[0]).sum()) for c in cuadro]
    return int(np.argmax(difs))


def armar_pagina(puertas, dry_run):
    """Copia las tiras de puerta a `art/pack/casas_puertas.png`, en filas.

    Es lo único que mueve píxeles. Cada tira entra entera y alineada a la grilla,
    así el rect de cada cuadro se dice en celdas y el juego avanza la tira sin
    saber nada de esta página.
    """
    # La página mide lo que mide la tira más ancha: una tira NO se parte.
    ancho_pagina = max(PAGINA_MIN, max(p.arr.shape[1] for p in puertas))
    filas, fila, ancho_fila, alto_fila, alto = [], [], 0, 0, 0
    for p in puertas:
        w = p.arr.shape[1]
        if ancho_fila + w > ancho_pagina and fila:
            filas.append((fila, alto_fila))
            alto += alto_fila
            fila, ancho_fila, alto_fila = [], 0, 0
        fila.append((p, ancho_fila))
        ancho_fila += w
        alto_fila = max(alto_fila, p.arr.shape[0])
    if fila:
        filas.append((fila, alto_fila))
        alto += alto_fila
    pagina = Image.new("RGBA", (ancho_pagina, max(alto, CELL)), (0, 0, 0, 0))
    yy = 0
    for fila, alto_fila in filas:
        for p, x in fila:
            pagina.paste(Image.fromarray(p.arr), (x, yy))
            p.rect = (x // CELL, yy // CELL)
        yy += alto_fila
    if not dry_run:
        pagina.save(os.path.join(ASSETS, PUERTAS_REL))
    return pagina


def poses_del_batiente(p):
    """Las cuatro poses que `building::animate_doors` sabe reproducir.

    `closed` y `open` son un cuadro quieto; `opening` y `closing` son la tira que
    va de uno al otro, dicha con `rect` + `anim` (el manifest avanza los cuadros
    hacia la derecha por defecto, que es como está dibujada la hoja).
    """
    col, row = p.rect

    def cuadro(k, frames=None):
        e = {
            "png": PUERTAS_REL,
            "grid": [CELL, CELL],
            "rect": {"col": col + k * p.w, "row": row, "w": p.w, "h": p.h},
        }
        if frames:
            e["anim"] = {"frames": frames, "fps": FPS_PUERTA}
        return e

    abre = max(p.abierta, 1)
    return {
        "closed": cuadro(0),
        "open": cuadro(p.abierta),
        "opening": cuadro(0, abre),
        "closing": cuadro(p.abierta, max(p.cuadros - p.abierta, 1)),
    }


# ---------------------------------------------------------------------------
# 3. Publicar
# ---------------------------------------------------------------------------


def celdas(origen, arr):
    x, y = origen
    return {
        "col": x // CELL,
        "row": y // CELL,
        "w": arr.shape[1] // CELL,
        "h": arr.shape[0] // CELL,
    }


def pose_pieza(ruta, origen, arr):
    return {"png": ruta, "grid": [CELL, CELL], "rect": celdas(origen, arr)}


def pose_sitio_de_puerta(ruta, origen_pieza, xy_en_pieza, p):
    """Dónde cae la puerta **dentro del recorte**, en píxeles.

    Con `grid: [1, 1]` el rect del manifest se lee en píxeles en vez de en celdas
    (`Pose::grid_or`): es la forma que ya tenía el formato de decir "esta pose vive
    en otra escala", y es lo que hace falta acá porque las puertas del pack no caen
    en la grilla de 32 (la de `Terraced_House_Modular_2` arranca en el píxel 204).

    El `col`/`row` es un **desplazamiento desde la esquina de la pieza**, no una
    posición en la página: así el cliente la ubica sabiendo sólo con qué tamaño
    dibujó la fachada, sin tener que averiguar en qué parte del atlas vive.
    """
    del origen_pieza  # el offset es relativo: la página sólo importa para el png
    return {
        "png": ruta,
        "grid": [1, 1],
        "rect": {
            "col": xy_en_pieza[0],
            "row": xy_en_pieza[1],
            "w": p.w * CELL,
            "h": p.h * CELL,
        },
    }


def colgar_puertas_en_fachadas(sprites, ubic, puerta_de):
    """**La puerta animada, encima de las fachadas ENTERAS que ya reparte el juego.**

    Una casa entera se planta de un sprite (`building::roof_sprite`), así que no
    hay dónde meterle la puerta… salvo que el propio slot diga dónde está. Eso es
    lo que se agrega acá: a TODA entrada del manifest cuyo recorte sea exactamente
    esta casa —las que `import-edificios.py` le repartió a los lotes, con sus
    variantes— se le cuelgan las cuatro poses del batiente y una `puerta_en_sprite`
    que dice, en píxeles de la página, dónde va.

    Se buscan por RECORTE y no por nombre: el importador masivo bautiza automático
    y el de edificios reparte alias, así que la misma casa vive bajo media docena
    de claves (`building/casa_mediana#3`, `building/villa#7`…). Lo único que no
    miente es a qué píxeles apunta cada una.
    """
    tocadas = []
    for nombre, (ruta, origen, arr) in ubic.items():
        pd = puerta_de.get(nombre)
        if not pd:
            continue
        p, xy = pd
        rect = celdas(origen, arr)
        poses = poses_del_batiente(p)
        poses["puerta_en_sprite"] = pose_sitio_de_puerta(ruta, origen, xy, p)
        for clave, e in sprites.items():
            if clave.startswith(SLOT) or e.get("png") != ruta or e.get("rect") != rect:
                continue
            # No se le pisa nada al que ya tenga poses propias (un asset curado a
            # mano en HA Studio): se agregan sólo las que faltan.
            suyas = e.setdefault("poses", {})
            for k, v in poses.items():
                suyas.setdefault(k, v)
            # **Y la pose REPRESENTATIVA, que es la casa entera.**
            #
            # Sin esto, colgarle la puerta a una fachada la ROMPE: `world_sprite`
            # busca una pose representativa (`idle_down`/`center`/`base`) y, si no
            # hay, **se queda con la primera del mapa** — que en un `HashMap` es
            # la que salga. O sea que la casa se dibujaba con el sprite de su
            # propia puerta, y si le tocaba `opening` o `closing` (que llevan
            # animación) `world_sprite` ni siquiera recorta: estira la HOJA
            # ENTERA de puertas sobre el lote. Ése fue el "se ven todos los
            # frames" y el "el exterior no cubre el interior" del reporte.
            suyas.setdefault("base", {"png": ruta, "grid": [CELL, CELL], "rect": rect})
            tocadas.append(clave)
    return tocadas


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    paginas = paginas_del_tema()
    nombres = sorted({n for par in HILERA for n in par} | set(ENTERAS))
    piezas, ubic = {}, {}
    for n in nombres:
        ruta, origen, arr = ubicar(n, paginas)
        piezas[n] = arr
        ubic[n] = (ruta, origen, arr)
    print(f"piezas ubicadas en el atlas: {len(ubic)} ({len(ENTERAS)} casas enteras)")

    print("puertas animadas:")
    puertas = medir_puertas(piezas)
    if not puertas:
        raise SystemExit("ninguna puerta calzó: no se publica un kit sin puerta")
    armar_pagina(puertas, args.dry_run)
    puerta_de = {}
    for p in puertas:
        for n, xy in p.donde.items():
            puerta_de.setdefault(n, (p, xy))

    man = json.load(open(MANIFEST))
    sprites, objects = man["sprites"], man["objects"]

    variantes, nuevos = [], []
    for i, (tramo, remate) in enumerate(HILERA, start=1):
        rt, ot, at = ubic[tramo]
        rr, orr, ar = ubic[remate]
        st = solapa(at, at)
        sr = solapa(at, ar)
        poses = {
            # La representativa: sin ella `world_sprite` se queda con la primera
            # pose del mapa (ver `colgar_puertas_en_fachadas`) y el slot se dibuja
            # con la puerta. Acá es el remate, que es una casa en hilera de un
            # solo módulo.
            "base": pose_pieza(rr, orr, ar),
            "tramo": pose_pieza(rt, ot, at),
            "remate": pose_pieza(rr, orr, ar),
            # **El PASO entre módulos**, en píxeles: lo que hay que avanzar para
            # poner el siguiente. Es el ancho de la pieza menos lo que se pisan
            # (ver `solapa`). Viaja como una pose con `grid: [1, 1]` por el mismo
            # motivo que el sitio de la puerta: un rect del manifest se dice en
            # celdas y esto no cae en la grilla.
            "paso_tramo": {
                "png": rt,
                "grid": [1, 1],
                "rect": {"col": 0, "row": 0, "w": (at.shape[1] - st * CELL), "h": 1},
            },
            "paso_remate": {
                "png": rr,
                "grid": [1, 1],
                "rect": {"col": 0, "row": 0, "w": (ar.shape[1] - sr * CELL), "h": 1},
            },
        }
        print(f"  · diseño {i}: solapa tramo→tramo {st} celda(s), tramo→remate {sr}")
        pt, pr = puerta_de.get(tramo), puerta_de.get(remate)
        # El batiente sale de la puerta que se haya encontrado: la del tramo si
        # la hay, y si no la del remate (son el mismo dibujo en los seis
        # diseños; lo que cambia es dónde la puso el autor).
        p = (pt or pr)[0] if (pt or pr) else None
        if p:
            poses.update(poses_del_batiente(p))
            if pt:
                poses["puerta_en_tramo"] = pose_sitio_de_puerta(rt, ot, pt[1], p)
            if pr:
                poses["puerta_en_remate"] = pose_sitio_de_puerta(rr, orr, pr[1], p)
        clave = f"{SLOT}#{i}"
        # El sprite pelado del slot es el REMATE: una casa en hilera de un solo
        # módulo es una casa, y así el editor y `loading.rs` tienen algo que
        # mostrar sin componer nada.
        sprites[clave] = {
            "png": rr,
            "grid": [CELL, CELL],
            "rect": celdas(orr, ar),
            "layer": "Roof",
            "poses": poses,
        }
        nuevos.append(clave)
        variantes.append({"id": str(i), "label": f"{LABEL} {i}"})
    sprites[SLOT] = dict(sprites[f"{SLOT}#1"])
    nuevos.append(SLOT)
    objects[SLOT] = {"label": LABEL, "variants": variantes}

    tocadas = colgar_puertas_en_fachadas(sprites, ubic, puerta_de)
    print(f"fachadas enteras con puerta animada: {len(tocadas)}")

    if not args.dry_run:
        with open(MANIFEST, "w") as fh:
            json.dump(man, fh, indent=1, ensure_ascii=False, sort_keys=True)
        with open(INDEX, "w") as fh:
            json.dump(
                {"sprites": nuevos, "objects": [SLOT], "poses_en": sorted(set(tocadas))},
                fh,
                indent=1,
            )
    print(f"manifest: {len(nuevos)} claves · página {PUERTAS_REL} · índice {INDEX}")


if __name__ == "__main__":
    main()
