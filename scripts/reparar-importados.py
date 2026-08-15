#!/usr/bin/env python3
"""**Reparar lo que el import masivo dejó mal medido.**

    python3 scripts/reparar-importados.py --dry-run
    python3 scripts/reparar-importados.py

## Qué estaba mal

`import-pack-masivo.py` metió 12.000 sprites del pack con una constante inventada
a ojo, `ESCALA_TILE = 1.5`: cada celda de 32 px del pack pasó a medir 1,5 m de
mundo. Pero la escala del juego **ya estaba fijada por el personaje**, que es el
único objeto cuyo tamaño real conocemos:

    marco del personaje (LimeZu)   32 × 64 px   = 1 × 2 celdas
    dibujado en el juego                          0,95 × 1,90 m   (`scene.rs`)
    ⇒ una celda del pack                          0,95 m

O sea que todo lo importado se dibuja **un 58 % más grande de lo que el artista
lo dibujó**. Se ve enseguida en cualquier captura de calle: bancos de tres metros
de largo, tachos de basura más altos que el jugador, latas de gaseosa de metro y
medio. No era una decisión de estilo — es una constante que nadie midió.

Y ése era **uno** de los tres defectos que traía la importación masiva. Los tres
se arreglan acá, y los tres son medibles:

    --solo escala     el tamaño. Recalcula, DESDE EL MANIFEST, los tres sitios
                      donde el 1,5 quedó horneado:
                        ha-shared/src/pack_siembra.rs      metros
                        ha-shared/src/interior_siembra.rs  metros (los muebles)
                        ha-shared/src/placeables.rs        huella en tiles

    --solo asentar    la POSICIÓN. El empaquetador pega cada sprite arriba-
                      izquierda de un bloque redondeado a celdas, así que 8.553
                      quedaron con aire ABAJO — y como el mundo los planta con el
                      pivote al pie, ese aire es el objeto flotando. Se mueve el
                      pixel dentro de su bloque: abajo del todo y centrado.

    --solo baldosas   la CLASIFICACIÓN. 29 entradas de las tablas de siembra son
                      autotiles de terreno o props con el suelo pegado detrás:
                      cuadrados planos sobre el pasto. Un recorte de una celda al
                      100 % de opacidad no es un objeto, es superficie.

Sin `--solo` corre los tres. Es **idempotente**: nada se escala ni se mueve dos
veces, porque todo se recalcula desde el pixel y desde el manifest. Correrlo dos
veces da lo mismo que correrlo una — la segunda no cambia nada.

## Lo que NO toca

- Las tablas curadas a mano de `worldgen.rs` (`NATURE_SPECIES`, `PROP_SPECIES`,
  `WATER_DECOR`, `SHORE_DECOR`): ésas las midió una persona mirando la pantalla.
- Lo que embaldosa, lo que se compone en piezas que encajan y lo que tiene varios
  cuadros por bloque (ver `INTOCABLES`): ahí mover un pixel abre una costura.

## Cómo se verifica

`HA_CITYTEST=1 HA_AUTOPLAY=1 ./target/debug/ha-game` deja tres capturas de una
ciudad generada en `$HA_SHOTDIR`. Es la prueba: los tres defectos se ven en la
primera, a pie en la calle.
"""

import argparse
import collections
import json
import os
import re

import numpy as np
from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MANIFEST = os.path.join(ROOT, "native/crates/ha-game/assets/art/manifest.json")
#: Las DOS tablas generadas que llevan tamaños en metros. `interior_siembra.rs`
#: es la de los muebles de adentro y se había quedado afuera de la primera
#: reparación: los interiores siguieron un rato con los muebles a escala 1,5
#: mientras la calle ya estaba corregida, o sea camas de metro y medio de alto.
SIEMBRAS = [
    os.path.join(ROOT, "native/crates/ha-shared/src/pack_siembra.rs"),
    os.path.join(ROOT, "native/crates/ha-shared/src/interior_siembra.rs"),
]
PLACEABLES = os.path.join(ROOT, "native/crates/ha-shared/src/placeables.rs")

#: Metros que mide una celda de 32 px del pack. NO se elige: sale de dividir el
#: alto al que se dibuja el personaje (1,90 m) por las celdas que ocupa su marco
#: (2). Ver el docstring.
METROS_POR_CELDA = 0.95

#: La constante vieja, para poder reconocer lo que todavía no se reparó.
VIEJA = 1.5


def celdas_por_clave():
    """clave lógica → (ancho, alto) en celdas, según su recorte en el manifest."""
    man = json.load(open(MANIFEST))
    out = {}
    for clave, spr in man["sprites"].items():
        r = spr.get("rect") or spr.get("strip")
        if not r:
            poses = spr.get("poses")
            if poses:
                p = next(iter(poses.values()))
                r = p.get("rect")
        if not r:
            continue
        out[clave] = (max(1, int(r["w"])), max(1, int(r["h"])))
    return out


# ---------------------------------------------------------------------------
# pack_siembra.rs — `size: [ancho, alto]` en metros
# ---------------------------------------------------------------------------

LINEA_SIEMBRA = re.compile(
    r'(?P<pre>ExtraDecor \{ key: "(?P<clave>[^"]+)", size: \[)'
    r"(?P<w>[0-9.]+), (?P<h>[0-9.]+)"
    r"(?P<post>\], weight: \d+ \},)"
)


def reparar_siembra(celdas, dry_run):
    cambios = [0]
    sin_celdas = set()

    def sub(m):
        clave = m.group("clave")
        cw, ch = celdas.get(clave, (None, None))
        if cw is None:
            # Sin recorte en el manifest no se puede medir: se deduce de la propia
            # línea, que se escribió como celdas × 1.5.
            cw = max(1, round(float(m.group("w")) / VIEJA))
            ch = max(1, round(float(m.group("h")) / VIEJA))
            sin_celdas.add(clave)
        w, h = cw * METROS_POR_CELDA, ch * METROS_POR_CELDA
        nuevo = f"{m.group('pre')}{w:.2f}, {h:.2f}{m.group('post')}"
        if nuevo != m.group(0):
            cambios[0] += 1
        return nuevo

    for ruta in SIEMBRAS:
        nuevo = LINEA_SIEMBRA.sub(sub, open(ruta).read())
        if not dry_run:
            open(ruta, "w").write(nuevo)
    return cambios[0], len(sin_celdas)


# ---------------------------------------------------------------------------
# pack_siembra.rs — echar las BALDOSAS, que no son objetos
# ---------------------------------------------------------------------------

#: Por encima de esta cobertura de pixel opaco, un recorte de UNA celda no puede
#: ser un objeto suelto. Medido, no elegido: los 15 casos que pasan este filtro
#: se miraron uno por uno y los 15 son superficie.
COBERTURA_BALDOSA = 0.98

#: Lo mismo para los recortes de VARIAS celdas: **ni un solo pixel transparente**.
#:
#: La regla miraba sólo el 1×1, así que las superficies grandes seguían pasando:
#: el entablado del muelle (2×1), la ladera de arena modular (3×1), el suelo de
#: obra, el panel de pared de la cárcel, la puerta del faro con su muro rojo
#: pegado atrás. Son el mismo cuadrado plano, más grande — lo que define a una
#: superficie es que no tenga silueta, y eso no depende del tamaño.
#:
#: El corte es más duro que el del 1×1 y no por prudencia: **se midió**. En un
#: recorte grande el aire de un borde pesa muy poco, así que con 0,98 empezaban a
#: caer objetos de verdad (un banco de jardín de 3×1 da 0,9987). Los que hay que
#: echar dan exactamente 1,0000: están dibujados para embaldosar y llegan al
#: borde de la celda por definición. La línea está donde la puso el pack.
COBERTURA_BALDOSA_GRANDE = 0.999


def es_baldosa(clave, man, cache):
    """**¿Este slot es una superficie disfrazada de objeto?**

    Un objeto suelto tiene silueta: se recorta contra el fondo y por eso se puede
    plantar sobre cualquier suelo. Un recorte de UNA celda con el pixel al 100 %
    no tiene silueta — es una baldosa, o un objeto con el suelo pegado detrás.

    Las dos cosas se ven igual de mal en el juego y por el mismo motivo: al
    plantarlas queda un CUADRADO PLANO sobre el pasto. El pack trae 32 de estos
    metidos en las tablas de siembra (`pasto_3`, `pasto_17`, `pasto_agua_5`,
    `empedrado`, `obra_suelo_3`…, todos autotiles de terreno que el importador
    clasificó como `nature`/`prop`), y son los cuadrados verdes que se ven en
    cualquier captura de pueblo.

    El suelo del juego NO se dibuja con sprites: es la malla del chunk con la
    hoja `suelo.png` (`art2d::SUELO_PAGINA`). Una baldosa que llega acá no es que
    esté mal puesta — es que no tenía que estar.
    """
    spr = man["sprites"].get(clave)
    if not spr or not spr.get("png") or not spr.get("rect"):
        return False
    r = spr["rect"]
    ruta = os.path.join(ART, spr["png"])
    if ruta not in cache:
        if not os.path.exists(ruta):
            return False
        cache[ruta] = np.asarray(Image.open(ruta).convert("RGBA"))[:, :, 3] > OPACO
    a = cache[ruta]
    gw, gh = spr.get("grid", [32, 32])
    x0, y0 = r["col"] * gw, r["row"] * gh
    x1, y1 = x0 + r["w"] * gw, y0 + r["h"] * gh
    if y1 > a.shape[0] or x1 > a.shape[1]:
        return False
    # **Se mide sobre el BLOQUE DE CELDAS, no sobre el dibujo apretado.** Es la
    # diferencia entera: un tacho de basura tiene la silueta llena de pixel —su
    # caja apretada da 1,00— y lo que lo delata como objeto es el AIRE que le
    # queda dentro de la celda. Una superficie llega al borde de la celda porque
    # está dibujada para embaldosar, y por eso no lo tiene.
    una_celda = r["w"] == 1 and r["h"] == 1
    corte = COBERTURA_BALDOSA if una_celda else COBERTURA_BALDOSA_GRANDE
    return float(a[y0:y1, x0:x1].mean()) >= corte


def echar_baldosas(dry_run):
    man = json.load(open(os.path.join(ART, "art/manifest.json")))
    cache = {}
    fuera = []
    for ruta in SIEMBRAS:
        salida = []
        for linea in open(ruta).read().splitlines(keepends=True):
            m = re.search(r'key: "([^"]+)"', linea)
            if m and es_baldosa(m.group(1), man, cache):
                fuera.append(m.group(1))
                continue
            salida.append(linea)
        if not dry_run:
            open(ruta, "w").write("".join(salida))
    return fuera


# ---------------------------------------------------------------------------
# placeables.rs — huella en tiles, sólo dentro del bloque PACK
# ---------------------------------------------------------------------------

LINEA_PLACEABLE = re.compile(
    r'(?P<pre>p\("[^"]+", "(?:[^"\\]|\\.)*", "(?P<clave>[^"]+)", '
    r"PlaceCat::\w+, Material::\w+, )"
    r"(?P<w>\d+), (?P<h>\d+)"
    r"(?P<post>\),)"
)


def reparar_placeables(celdas, dry_run):
    txt = open(PLACEABLES).read()
    ini = txt.index("// === PACK:START ===")
    fin = txt.index("// === PACK:END ===")
    cuerpo = txt[ini:fin]
    cambios = [0]

    def sub(m):
        cw, ch = celdas.get(m.group("clave"), (None, None))
        if cw is None:
            return m.group(0)
        # La huella es en TILES enteros: lo que ocupa en la grilla de construcción.
        w = max(1, round(cw * METROS_POR_CELDA))
        h = max(1, round(ch * METROS_POR_CELDA))
        nuevo = f"{m.group('pre')}{w}, {h}{m.group('post')}"
        if nuevo != m.group(0):
            cambios[0] += 1
        return nuevo

    nuevo = LINEA_PLACEABLE.sub(sub, cuerpo)
    if not dry_run:
        open(PLACEABLES, "w").write(txt[:ini] + nuevo + txt[fin:])
    return cambios[0]


# ---------------------------------------------------------------------------
# Las páginas del atlas — asentar el dibujo en su bloque
# ---------------------------------------------------------------------------

ART = os.path.join(ROOT, "native/crates/ha-game/assets")
OPACO = 16

#: Namespaces que NO se tocan porque su pixel tiene que quedar donde está:
#: lo que embaldosa (suelo, pared, techo, cerca), lo que se compone en piezas que
#: encajan entre sí (fachadas, marcos, escaleras) y lo que no vive en el mundo
#: (HUD, partículas). Mover el dibujo dentro del bloque en cualquiera de éstos
#: abre una costura visible entre baldosas contiguas.
INTOCABLES = {
    "wall", "window", "door", "floor", "tiles", "road", "roof", "ceiling",
    "stairs", "doorway", "railing", "room", "interior", "ui", "particles",
    "npc", "player", "items", "machines", "structure", "fence",
}


def asentar_paginas(dry_run):
    """**Que el dibujo toque el piso de su bloque.**

    El empaquetador pega cada sprite en la esquina de ARRIBA-IZQUIERDA de un
    bloque redondeado a celdas enteras. Un objeto de 40×56 px termina en un bloque
    de 2×2 celdas (64×64) con 24 px de aire ABAJO y 24 a la derecha. Como el mundo
    planta estos sprites con el pivote al pie (`world2d`: el borde de abajo del
    quad es donde el objeto pisa), ese aire se traduce en objetos **flotando**
    sobre su propia celda y corridos hacia la izquierda. Son 8.553 sprites, y es
    lo que se ve como "los props están mal puestos".

    El arreglo mueve el pixel DENTRO de su bloque: abajo del todo y centrado. El
    bloque no cambia de tamaño ni de sitio, así que no hay que reempaquetar nada
    ni tocar un solo rect del manifest — y el aire que queda pasa a estar ARRIBA,
    que para algo plantado en el suelo es lo correcto: es aire sobre la cabeza.
    """
    man = json.load(open(os.path.join(ART, "art/manifest.json")))
    por_pagina = collections.defaultdict(list)
    saltados = collections.Counter()
    for clave, spr in man["sprites"].items():
        if "strip" in spr or "poses" in spr:
            # Varios cuadros por bloque: el rect describe UNO y los demás son sus
            # vecinos. Mover el primero lo desalinea del resto de la tira.
            saltados["animado"] += 1
            continue
        if not spr.get("rect") or not spr.get("png"):
            saltados["sin recorte"] += 1
            continue
        if clave.split("/")[0] in INTOCABLES:
            saltados["intocable"] += 1
            continue
        por_pagina[spr["png"]].append((clave, spr))

    movidos = paginas_tocadas = solapados = 0
    for png, items in sorted(por_pagina.items()):
        ruta = os.path.join(ART, png)
        if not os.path.exists(ruta):
            continue
        im = Image.open(ruta).convert("RGBA")
        W, H = im.size
        alfa = np.asarray(im)[:, :, 3] > OPACO
        # **Un bloque por RECT, no por clave.** 1.176 claves comparten recorte con
        # otra (el mismo dibujo entró con dos nombres: `prop/x` y `nature/x`), y
        # tratarlas como bloques distintos haría mover el mismo pixel dos veces.
        # Lo que sí sería peligroso es que dos rects DISTINTOS se pisaran —mover
        # uno escribiría encima del otro—, así que eso se comprueba y se salta.
        bloques = {}
        for clave, spr in items:
            gw, gh = spr.get("grid", [32, 32])
            r = spr["rect"]
            x0, y0 = r["col"] * gw, r["row"] * gh
            x1, y1 = x0 + r["w"] * gw, y0 + r["h"] * gh
            if x1 > W or y1 > H:
                continue
            bloques[(x0, y0, x1, y1)] = clave
        ocupadas = np.zeros((H // 32 + 1, W // 32 + 1), np.int32)
        for (x0, y0, x1, y1) in bloques:
            ocupadas[y0 // 32:y1 // 32, x0 // 32:x1 // 32] += 1
        tocada = False
        for (x0, y0, x1, y1), clave in bloques.items():
            if (ocupadas[y0 // 32:y1 // 32, x0 // 32:x1 // 32] > 1).any():
                solapados += 1
                continue
            sub = alfa[y0:y1, x0:x1]
            if not sub.any():
                continue
            ys, xs = np.nonzero(sub)
            ix0, ix1, iy0, iy1 = int(xs.min()), int(xs.max()) + 1, int(ys.min()), int(ys.max()) + 1
            bw, bh = x1 - x0, y1 - y0
            dy = bh - iy1                      # al piso del bloque
            dx = ((bw - ix1) - ix0) // 2       # centrado
            if dy == 0 and dx == 0:
                continue
            tinta = im.crop((x0 + ix0, y0 + iy0, x0 + ix1, y0 + iy1))
            im.paste(Image.new("RGBA", (bw, bh), (0, 0, 0, 0)), (x0, y0))
            im.paste(tinta, (x0 + ix0 + dx, y0 + iy0 + dy))
            movidos += 1
            tocada = True
        if tocada:
            paginas_tocadas += 1
            if not dry_run:
                im.save(ruta, optimize=True)
    return movidos, paginas_tocadas, saltados, solapados


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument(
        "--solo", choices=("escala", "asentar", "baldosas"), help="un paso nada más"
    )
    args = ap.parse_args()

    if args.solo in (None, "baldosas"):
        fuera = echar_baldosas(args.dry_run)
        print(f"pack_siembra.rs: {len(fuera)} baldosas echadas de la siembra")
        for k in fuera:
            print(f"       - {k}")

    if args.solo in (None, "escala"):
        celdas = celdas_por_clave()
        print(f"{len(celdas)} recortes medidos en el manifest")
        n, sin = reparar_siembra(celdas, args.dry_run)
        print(f"pack_siembra.rs: {n} tamaños recalculados ({sin} claves sin recorte, deducidas)")
        n = reparar_placeables(celdas, args.dry_run)
        print(f"placeables.rs:   {n} huellas recalculadas")

    if args.solo in (None, "asentar"):
        movidos, pags, saltados, solapados = asentar_paginas(args.dry_run)
        print(f"atlas: {movidos} sprites asentados en {pags} páginas")
        print(f"       saltados {dict(saltados)}; bloques solapados {solapados}")

    if args.dry_run:
        print("(dry-run: no se escribió nada)")


if __name__ == "__main__":
    main()
