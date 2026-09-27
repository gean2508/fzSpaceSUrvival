#!/usr/bin/env python3
"""**Las casas del mapa, con la fachada entera del pack — y una distinta por lote.**

    python3 scripts/import-edificios.py --dry-run   # qué fachadas le tocaron a cada lote
    python3 scripts/import-edificios.py             # escribe el manifest

## Por qué

`ha-shared::city::LOT_PROGRAMS` declara qué es cada parcela de una ciudad
(`building/casa_chica`, `building/hospital`, …) y `ha-game::building` dibuja esa
clave como un sprite **encima** de la casa que se **transparenta al entrar** — el
"entrás por la puerta y ves el interior sin cortes". Lo que faltaba es que esas
claves tuvieran pixel: sin arte, cada casa del mundo se dibuja con el cuadrado
marrón de respaldo.

El pack SÍ las tiene, y de sobra: `24_Additional_Houses` trae 215 singles y
`7_Villas` otros 194.

## Cómo elige, sin man-in-the-loop

Tres filtros, en este orden:

1. **La PÁGINA manda** (`PAGINAS_DEL_LOTE`). Cada fachada del pack vive en la
   página de atlas de SU tema (`art/pack/additional_houses_3.png`,
   `art/pack/school_1.png`), y esa página la escribe el importador masivo con el
   nombre de la carpeta del pack. Una casa sólo puede recibir una fachada de
   casa; una escuela, una de escuela.

   **Antes esto se filtraba por el PREFIJO de la clave** (`building/casas_…`,
   `building/escuela_…`) y se rompió sin hacer ruido: cuando el importador masivo
   se volvió idempotente dejó de anteponer el tema, así que `casas_` pasó a
   emparejar UNA clave de 215 y `villas_` ocho de 194. Con un solo candidato,
   `casa_chica`, `casa_mediana`, `casa_grande` y `casa_country` recibían las
   cuatro **la misma villa**, y veinte lotes más se quedaban sin fachada. Un
   nombre es una convención y se le escapa a cualquiera; la página sale de la
   carpeta del pack y no depende de cómo se bautizó nada.

2. **Y dentro de la página, el TAMAÑO.** Gana la fachada cuya huella (celdas ×
   `ESCALA_TILE`) más se parece a la del lote, midiendo lado y proporción. El
   nombre no entra: el importador masivo bautiza automático y llama `casa_*`
   hasta a una maceta de 1×1.

3. **Y no una: VARIAS.** Un barrio donde todas las casas son la misma casa se lee
   como un error, no como un barrio — y es el 60 % de la ciudad. Así que cada
   slot se lleva hasta `VARIANTES_MAX` fachadas distintas como variantes
   (`building/casa_chica#2`, `#3`, …) y el cliente elige una por lote con
   `variant_key_seeded` sembrado en la esquina de la parcela. No cuesta un slot
   más ni un byte de mundo guardado.

Se descarta lo que no puede ser un edificio: menos de `MIN_CELDAS`, con menos del
`OPACIDAD_MIN` de su recorte dibujado (un cartel suelto, una antena), o sin
apoyar en el piso (`BASE_MIN`).

No copia píxeles: la entrada del manifest apunta al mismo `png` y `rect` que el
pack ya tiene. Anota lo suyo en `scripts/edificios-import-index.json`.
"""

import argparse
import json
import os
import re

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ASSETS = os.path.join(ROOT, "native/crates/ha-game/assets")
MANIFEST = os.path.join(ASSETS, "art/manifest.json")
CITY_RS = os.path.join(ROOT, "native/crates/ha-shared/src/city.rs")
INDEX = os.path.join(ROOT, "scripts/edificios-import-index.json")

#: **Metros de mundo que mide una celda de 32 px del pack.** Es el mismo número
#: que `ha-game::art2d::ESCALA_ARTE` y que `import-pack-masivo.py::ESCALA_TILE`:
#: sale de dividir la altura del personaje (1,90 m) por las 2 celdas que mide su
#: marco en el pack. Este script decía 1,5 —el valor viejo, el que se había
#: elegido a ojo— así que medía cada fachada un 58 % más grande de lo que el
#: juego la dibuja y elegía siempre edificios demasiado chicos para el lote. Lo
#: cuida el test `todos_los_importadores_usan_la_misma_escala`.
ESCALA_TILE = 0.95
#: Mínimo para que un recorte pueda ser un edificio, en celdas.
#:
#: El ancho es el que hace el trabajo: en este pack no hay ningún edificio de
#: menos de ocho celdas de frente, y lo que sí hay de 5–6 son composiciones de
#: jardín (`casa_sombrilla_composicion`: sombrilla + mesa) y trozos verticales de
#: casa (`casa_victoriana_7` es la TORRE del reloj, 6×16). Los dos se colaban al
#: barrio como si fueran casas.
MIN_CELDAS = (8, 5)
#: Fracción mínima del recorte que tiene que estar dibujada. Una fachada cubre
#: casi todo su rectángulo; un cartel, una antena o una composición de jardín
#: (sombrilla + mesa + cantero) dejan medio recorte en aire.
OPACIDAD_MIN = 0.55
#: Y además tiene que estar APOYADA: su fila de abajo, dibujada en buena parte.
#: Separa un edificio —que se planta en el suelo— de un conjunto de muebles
#: sueltos que casualmente llena su caja. Flojo a propósito: una fachada en 3/4
#: tiene sombra y aire en las puntas de su fila de abajo.
#:
#: **El pie se mide contra el ANCHO DEL PROPIO DIBUJO, no contra el rectángulo.**
#: Medirlo como "relleno de la última fila del rect" descartaba 23 de las 32 casas
#: del pack — `casa_victoriana_1`, las seis `casa_en_terrazas_*`, `casa_embrujada`,
#: la `casa` de 23×14 — y por dos motivos a la vez: el recorte deja una franja
#: transparente abajo (el aire de la sombra proyectada), y una fachada en 3/4 casi
#: nunca ocupa TODO el ancho de su caja al ras del piso (hay un cantero, un
#: sendero, un farol que baja más que la pared). Con nueve candidatos para cuatro
#: slots residenciales el barrio salía fotocopiado por falta de material, no por
#: falta de arte.
#:
#: Lo que sí separa un edificio de un montón de muebles sueltos es que el edificio
#: sigue siendo ANCHO abajo: su quinto inferior mide casi lo mismo que su fila más
#: ancha. Medido sobre el pack: las 12 casas dan 0,66–1,00 y el toldo suelto 0,07,
#: el cartel 0,33.
BASE_MIN = 0.55
#: **La otra manera de ser un edificio: llenar la caja.**
#:
#: `BASE_MIN` mide el PIE, y hay fachadas del pack que apoyan sobre un porche
#: angosto: el cuartel de bomberos entero (16×15, con su torre a la izquierda y su
#: escalinata saliente) da 0,39, porque su quinto inferior es la escalera y nada
#: más. Con el corte en 0,55 quedaba afuera, `18_Fire_Station` se quedaba sin
#: un solo candidato y `building/bomberos` seguía apuntando a un placeholder
#: dibujado a mano de 4×3 celdas.
#:
#: Lo que sí lo separa de un toldo o un cartel es que un edificio **ocupa el
#: ancho de su recorte**: su fila más ancha mide casi tanto como el rect. Medido
#: sobre el pack:
#:
#:     cuartel de bomberos   apoya 0,39   llena 0,98   ← entra
#:     escuela / villa       apoya 0,77   llena 0,96
#:     cartel embrujado      apoya 0,33   llena 0,56   ← no
#:     toldo                 apoya 0,07   llena 0,86   ← no (le falta pie)
#:
#: Así que se acepta por cualquiera de los dos caminos: pie ancho, **o** caja
#: llena con algo de pie ([`BASE_MIN_LLENA`]).
LLENA_MIN = 0.90
#: El pie mínimo cuando la caja está llena. Menos que esto ya no es un edificio:
#: es algo colgado (el toldo da 0,07 y el cartel 0,33).
BASE_MIN_LLENA = 0.35
#: Qué fracción de la altura dibujada es "el pie". Un quinto: menos es el filo con
#: su antialias, más se come el cuerpo.
BASE_BANDA = 0.20
#: **Se probó y NO se usa: el contraste techo/pared.** La idea era descartar los
#: recortes de techo suelto mirando si el tercio de arriba y el de abajo son de
#: colores distintos (una fachada tiene cubierta arriba y muro abajo). Funciona
#: para las casas y ARRUINA los edificios cívicos: en este pack la escuela, la
#: comisaría y la estación están dibujadas casi todas de techo, así que el filtro
#: las dejaba a todas sin fachada. Queda anotado para no volver a intentarlo.

#: **Cuántas fachadas distintas se le cuelgan a un slot.** La primera es la clave
#: pelada (la que cae por defecto y la que precarga `loading.rs`); el resto son
#: variantes. Cuesta cero: son entradas de manifest apuntando a píxeles que YA
#: están en la página, así que el tope no compra ni memoria ni draw calls — sólo
#: decide cuánto del pack comprado llega al mundo.
#:
#: Estaba en doce y **doce era el tope, no el material**: `casa_mediana`,
#: `casa_grande` y `villa` cortaban exactamente en 12, o sea que el pack tenía más
#: casas que calzan igual de bien y quedaban en el disco. La barrera que sí importa
#: es `BANDA_DE_CALCE`, que ya deja afuera lo que no le corresponde a este talle:
#: subir el tope no mete casas peores, mete las que ya habían pasado el filtro.
VARIANTES_MAX = 48
#: Cuánto puede alejarse la PROPORCIÓN de una fachada de la de la huella.
#:
#: Ya no es un límite de deformación —el cliente escala uniforme y apoya el dibujo
#: en el frente, así que nada se estira (`building::roof_layout`)—, es un filtro de
#: sensatez: una fachada tres veces más alta que ancha metida en una parcela
#: apaisada le tapa media manzana a los vecinos. Flojo (±90 %) a propósito, porque
#: una fachada en 3/4 es SIEMPRE más alta que su huella: el dibujo incluye la
#: pared de frente, que en el suelo no ocupa nada.
PROPORCION_MAX = 0.9
#: Cuánto peor que la mejor puede calzar una variante y seguir entrando. Es lo que
#: mantiene separadas a la casa chica y a la grande cuando comparten pool.
BANDA_DE_CALCE = 0.55

#: **Páginas de atlas del pack de cada programa de lote.** Es la única decisión
#: escrita a mano de todo el pipeline, y es la que no puede salir de un hash: qué
#: clase de edificio es cada parcela. Los nombres son los de las carpetas del
#: pack, que es como los bautiza `import-pack-masivo.py::empaquetar`.
PAGINAS_DEL_LOTE = {
    # Residencial: las 215 casas sueltas y las 194 villas, juntas. Que un barrio
    # mezcle chalet y casa de dos plantas es exactamente lo que se quiere.
    "building/casa_chica": ["additional_houses", "villas"],
    "building/casa_mediana": ["additional_houses", "villas"],
    "building/casa_grande": ["additional_houses", "villas"],
    "building/casa_country": ["additional_houses", "villas"],
    # **La villa sale de `7_Villas` y de NINGÚN otro lado.** Con
    # `additional_houses` de respaldo, el lote más grande del barrio se llevaba
    # `casa_embrujada` —la casa encantada de Halloween, 19×22— porque la villa que
    # el pack dibujó mide 9×13 y la parcela pedía 19 tiles de frente. Un barrio
    # residencial sembrado de casas embrujadas no es variedad, es otra cosa.
    #
    # Acá se le saca el respaldo. La parcela NO se toca: desde el 24 ago 2026
    # ningún importador achica un lote (ver `reportar_huellas`). Las casas grandes
    # y raras del pack no se pierden: son las de `casa_grande` y `casa_country`,
    # que ya las reciben.
    "building/villa": ["villas"],
    "building/duplex": ["floor_modular_building", "generic_building"],
    "building/garaje": ["garage_sales"],
    # Comercio.
    "building/tienda": ["shopping_center_and_markets", "grocery_store"],
    "building/mercado": ["shopping_center_and_markets", "grocery_store"],
    "building/shopping": ["shopping_center_and_markets"],
    "building/gasolinera": ["vehicles", "worksite"],
    # Centro. **`condominium` NO va acá aunque el nombre invite**: en este pack es
    # un tema de Modern INTERIORS (77 sprites, todos `furniture/`), no la torre de
    # departamentos. Apuntado ahí, la oficina de la ciudad recibía un escritorio.
    "building/oficina": ["office", "generic_building"],
    "building/torre": ["generic_building", "office"],
    "building/hotel": ["hotel_and_hospital"],
    "building/hospital": ["hotel_and_hospital"],
    # Cívico.
    "building/universidad": ["school"],
    "building/escuela": ["school"],
    "building/comisaria": ["police_station"],
    "building/bomberos": ["fire_station"],
    "building/correo": ["post_office"],
    "building/estacion_tren": ["subway_and_train_station"],
    # Industria y campo. `worksite` tiene cuatro edificios contados y ninguno
    # grande (el resto de la obra son vallas y props), así que la nave y el
    # depósito salen de la base militar, que es donde el pack dibujó los galpones.
    "building/deposito": ["military_base", "worksite"],
    "building/granero": ["garden", "worksite"],
    "building/nave_industrial": ["military_base", "worksite"],
    "building/base_militar": ["military_base"],
    # **El natatorio y el balneario NO se declaran acá.** No son una fachada: el
    # pack los dibujó como ESCENAS —48 piezas de pileta, 110 de agua de playa— y
    # se componen pieza por pieza (`ha-shared::escenas`), no se estiran de un
    # sprite. Ver `import-escenas.py`.
}


def escala_del_juego():
    """`ESCALA_ARTE` tal como la declara el cliente. Si alguien la mueve allá y
    no acá, este script para en seco en vez de reimportar todo el pack a otra
    escala en silencio (que es exactamente lo que pasó con el 1,5)."""
    rs = os.path.join(ROOT, "native/crates/ha-game/src/art2d.rs")
    m = re.search(r"pub const ESCALA_ARTE: f32 = ([0-9.]+);", open(rs, encoding="utf8").read())
    if not m:
        raise SystemExit("no pude leer ESCALA_ARTE de art2d.rs")
    return float(m.group(1))


def programas_de_lote():
    """`clave -> (ancho, fondo)` de la HUELLA EDIFICADA, en tiles.

    **No es el tamaño del lote.** El lote incluye el jardín, la entrada de auto y
    la pileta; la fachada se dibuja sobre el rectángulo construido, que es lo que
    devuelve `ha-shared::city::roofed_rect`. Comparar la fachada contra la parcela
    entera hacía elegir edificios el doble de grandes que el hueco donde iban a
    entrar: para un lote residencial de 16×12 la casa mide 8×6, no 16×12.

    La cuenta NO se copia acá: se lee de `city.rs`, que es donde vive
    (`LotProgram::huella`, la misma que usan `residential_tile` y `roofed_rect`).
    Duplicar el 55 % era garantía de que un día alguien lo tocara de un lado
    solo y la fachada se eligiera para una huella que no existe.
    """
    txt = open(CITY_RS, encoding="utf8").read()
    m = re.search(r"const CASA_DEL_LOTE: \(i64, i64\) = \((\d+), (\d+)\);", txt)
    if not m:
        raise SystemExit("no pude leer CASA_DEL_LOTE de city.rs")
    pw, ph = int(m.group(1)), int(m.group(2))
    out = {}
    for clave, w, h, distrito in re.findall(
        r'LotProgram \{ key: "([^"]+)", cells: \((\d+), (\d+)\), district: District::(\w+)',
        txt,
    ):
        w, h = int(w), int(h)
        if distrito == "Residential":
            out[clave] = (
                min(max(w * pw // 100, 6), max(w - 4, 6)),
                min(max(h * ph // 100, 5), max(h - 5, 5)),
            )
        else:
            out[clave] = (max(w - 2, 1), max(h - 2, 1))
    if not out:
        raise SystemExit("no pude leer LOT_PROGRAMS de city.rs")
    return out


def pagina_de(entrada):
    """El tema del pack de un sprite: el nombre de su página sin el número.

    `art/pack/additional_houses_3.png` → `additional_houses`. Es el dato que
    `import-pack-masivo.py` saca de la CARPETA del pack, así que no depende de
    cómo se haya bautizado la clave.
    """
    png = entrada.get("png") or ""
    return re.sub(r"_\d+\.png$", "", os.path.basename(png)).replace(".png", "")


#: Carpetas de clave que pueden ser una fachada. Hace falta porque una PÁGINA de
#: atlas mezcla lo de afuera con lo de adentro cuando el pack repite el nombre del
#: tema: `office` trae los 65 edificios de `16_Office` (exteriores) **y** los 21
#: muebles del `Office` de Modern Interiors. Sin este filtro, la oficina de la
#: ciudad se dibujaba con `furniture/oficina_2` — un escritorio de 18×17 tiles.
CARPETAS = ("building/", "edificio/")

#: **Lo que el pack marcó como PIEZA de una casa, no como la casa.** Es la regla 8
#: aplicada a las fachadas: media docena de recortes de `24_Additional_Houses` son
#: el techo suelto, la baranda del balcón o el toldo, dibujados aparte para
#: componerlos encima del cuerpo. Un techo suelto llena su rectángulo y apoya
#: perfecto, así que pasa todos los filtros de pixel — y salía elegido como la
#: mejor fachada de `building/casa_chica`: una chapa gris rayada en medio del
#: barrio.
#:
#: Las palabras salen del pack (el importador masivo traduce el nombre del
#: archivo: `..._Roof_Side` → `casa_techo_..._costado`), igual que
#: `import-pack-masivo.py::MARCAS_MODULARES`. No se miden porque no hay nada que
#: medir: un techo BIEN dibujado es indistinguible de una fachada bien dibujada
#: mirando el alfa; lo que lo delata es para qué lo dibujó el autor, y eso está en
#: el nombre.
MARCAS_DE_PIEZA = tuple(
    re.compile(p)
    for p in (
        r"(?:^|_)techo(?:_|$)",
        r"(?:^|_)pieza(?:_|$)",
        r"(?:^|_)puerta(?:_|$)",
        r"(?:^|_)ventana(?:_|$)",
        r"(?:^|_)pared(?:_|$)",
        r"(?:^|_)baranda(?:_|$)",
        r"(?:^|_)balcon(?:_|$)",
        r"(?:^|_)toldo(?:_|$)",
        r"(?:^|_)cartel(?:_|$)",
        r"(?:^|_)poste(?:_|$)",
        r"(?:^|_)maceta(?:_|$)",
        r"(?:^|_)escalera(?:_|$)",
        r"(?:^|_)entrada(?:_|$)",
        r"(?:^|_)modular(?:_|$)",
        # **`costado` es la casa VISTA DE LADO**, y son las dos mitades de un kit
        # aparte: `Terraced_House_Side_View_Base` es la pared y
        # `..._Side_View_Roof` el techo, dibujados para apilarse uno sobre otro.
        # El empaquetador del pack dejó las dos bases pegadas en un recorte de
        # 10×12 que apoya, llena su caja y pasa todos los filtros de pixel — y así
        # `building/casa_chica`, la casa más común del mapa, era DOS PAREDES
        # apiladas sin un techo. Lo compone `import-casas.py`; suelto no va.
        r"(?:^|_)costado(?:_|$)",
        # `capa` es como el importador masivo tradujo `Layer`: media cosa por
        # definición (el invernadero venía en tres capas apiladas).
        r"(?:^|_)capa(?:_|$)",
        # `interior` es el relleno de un edificio, no el edificio: la carpa de la
        # base militar traía su interior negro como recorte aparte y ganaba el
        # lote del depósito por tamaño.
        r"(?:^|_)interior(?:_|$)",
        r"(?:^|_)riel(?:_|$)",
    )
)

#: **Lo que el pack dibujó entero, apoya, llena su caja… y NO es un edificio.**
#:
#: `MARCAS_DE_PIEZA` caza MEDIA cosa (un techo suelto, una baranda). Esto caza
#: cosas ENTERAS que pasan todos los filtros de pixel —son anchas abajo, opacas y
#: del tamaño justo— y que aun así no pueden hacer de fachada de un lote. Medido
#: sobre el pack, en el orden en que aparecieron:
#:
#: - **la carpa** (`carpa_marron`, `carpa_negra`, 12×13 en la base militar) ganaba
#:   los dos lotes de industria: el depósito y la NAVE INDUSTRIAL de la ciudad se
#:   dibujaban con una carpa de lona;
#: - **las canchas** (`cancha_de_basquet_*`, `futbol_cancha_*`, 12-14 celdas en la
#:   hoja de la escuela) competían por el lote de la escuela y de la universidad:
#:   una cancha pintada en el piso haciendo de edificio;
#: - **la torre de radio** (`radio_torre*`, 8×21) es una antena con su base, no un
#:   galpón, y por alta y angosta le ganaba a los galpones de verdad.
#:
#: Ninguna de las tres es un error del pack ni del importador masivo: las tres
#: entran al juego como lo que son (props y equipamiento de parcela). Lo que no
#: pueden es ser la fachada de una parcela.
NO_SON_FACHADA = tuple(
    re.compile(p)
    for p in (
        r"(?:^|_)carpa(?:_|$)",
        r"(?:^|_)cancha(?:_|$)",
        r"(?:^|_)canchas(?:_|$)",
        r"(?:^|_)radio_torre",
        r"(?:^|_)pileta(?:_|$)",
        r"(?:^|_)piscina(?:_|$)",
        r"(?:^|_)parrilla(?:_|$)",
    )
)


def es_pieza(clave):
    """¿El pack dibujó esto como PARTE de un edificio, o como algo que no es un
    edificio? Ver [`MARCAS_DE_PIEZA`] y [`NO_SON_FACHADA`]."""
    fam = clave.split("/", 1)[-1]
    return any(rx.search(fam) for rx in MARCAS_DE_PIEZA + NO_SON_FACHADA)


def apoya(pix, ancho, alto):
    """**¿Sigue siendo ancho abajo?** El pie del dibujo contra su fila más ancha.

    1 = el quinto inferior mide lo mismo que la parte más ancha (una pared que
    llega al piso). 0 = abajo no hay nada (un toldo, un cartel colgado). Ver
    [`BASE_MIN`] para las mediciones sobre el pack.
    """
    filas = [sum(1 for p in pix[y * ancho : (y + 1) * ancho] if p > 0) for y in range(alto)]
    dibujadas = [y for y, f in enumerate(filas) if f > 0]
    if not dibujadas:
        return 0.0
    y0, y1 = dibujadas[0], dibujadas[-1]
    banda = max(1, int((y1 - y0 + 1) * BASE_BANDA))
    pie = sum(filas[y1 - banda + 1 : y1 + 1]) / banda
    return pie / max(filas)


def llena(pix, ancho, alto):
    """**¿Ocupa el ancho de su recorte?** La fila más ancha contra el rect.

    1 = hay una fila que va de borde a borde (una fachada). 0,5 = el dibujo vive
    en la mitad de su caja (un cartel colgado, una composición de jardín). Ver
    [`LLENA_MIN`] para las mediciones sobre el pack.
    """
    filas = [sum(1 for p in pix[y * ancho : (y + 1) * ancho] if p > 0) for y in range(alto)]
    return max(filas) / ancho if ancho else 0.0


def candidatos(sprites, paginas, cache, propias=()):
    """Los recortes de esas páginas del pack que pueden ser un edificio.

    `propias` son las claves que escribió ESTE script en la corrida anterior: son
    alias que apuntan a un recorte del pack, no arte nuevo. Si entran al sorteo,
    la segunda corrida elige su propia salida de la primera y el importador deja
    de ser idempotente — `building/casa_chica` terminaba eligiéndose a sí misma.
    """
    out = []
    for clave, e in sprites.items():
        if "#" in clave or clave in propias:
            continue
        if not clave.startswith(CARPETAS) or es_pieza(clave):
            continue
        if pagina_de(e) not in paginas:
            continue
        r = e.get("rect")
        if not r or r["w"] < MIN_CELDAS[0] or r["h"] < MIN_CELDAS[1]:
            continue
        png = e.get("png")
        ruta = os.path.join(ASSETS, png or "")
        if not png or not os.path.exists(ruta):
            continue
        if png not in cache:
            cache[png] = Image.open(ruta).convert("RGBA")
        gw, gh = e.get("grid", [32, 32])
        im = cache[png].crop(
            (r["col"] * gw, r["row"] * gh, (r["col"] + r["w"]) * gw, (r["row"] + r["h"]) * gh)
        )
        alfa = im.getchannel("A")
        pix = list(alfa.getdata())
        ancho, alto = im.size
        opaco = sum(1 for p in pix if p > 0) / len(pix)
        if opaco < OPACIDAD_MIN:
            continue
        pie = apoya(pix, ancho, alto)
        if pie < BASE_MIN and not (pie >= BASE_MIN_LLENA and llena(pix, ancho, alto) >= LLENA_MIN):
            continue
        out.append({"clave": clave, "w": r["w"], "h": r["h"], "opaco": opaco, "entrada": e})
    return out


#: Cuánto pesa el FONDO frente al ancho al elegir una fachada.
#:
#: El cliente dibuja la fachada a su propia proporción, escalada por el ANCHO de
#: la huella y apoyada en el frente (`building::roof_layout`): el alto no deforma
#: nada, sólo decide cuánto sobresale el edificio por arriba. Así que el ancho es
#: la medida que tiene que calzar y el fondo apenas orienta — pesa, pero poco,
#: para que una torre de 24 tiles de alto no caiga en una parcela de chalet.
PESO_FONDO = 0.35


def calce(cand, objetivo):
    """Cuánto se aleja esta fachada de la huella edificada (0 = calza justo)."""
    cw, ch = cand["w"] * ESCALA_TILE, cand["h"] * ESCALA_TILE
    ow, oh = objetivo
    return abs(cw - ow) / ow + PESO_FONDO * abs(ch - oh) / oh


def deformacion(cand, objetivo):
    """Cuánto se aleja la PROPORCIÓN de la fachada de la de la huella.

    Ya no deforma nada (el dibujo se escala uniforme), pero sigue sirviendo de
    filtro de sensatez: una fachada tres veces más alta que ancha metida en una
    parcela apaisada tapa media manzana de vecinos.
    """
    cw, ch = cand["w"] * ESCALA_TILE, cand["h"] * ESCALA_TILE
    ow, oh = objetivo
    return abs((cw / ch) - (ow / oh)) / (ow / oh)


def elegir(cand, objetivo):
    """Las fachadas de este lote, la mejor primero y sin repetir dibujo.

    Se descarta lo que se deformaría demasiado al estirarlo al lote
    (`PROPORCION_MAX`) — pero si eso deja el slot vacío se afloja el filtro antes
    que dejarlo sin pixel: una casa un poco estirada se lee como una casa; un
    cuadrado marrón no se lee como nada.
    """
    sanos = [c for c in cand if deformacion(c, objetivo) <= PROPORCION_MAX]
    pool = sorted(sanos or cand, key=lambda c: calce(c, objetivo))
    # **Y no cualquiera que calce: sólo las que calzan PARECIDO a la mejor.**
    # Sin esto, `casa_chica` y `casa_grande` se llevaban las mismas siete casas en
    # otro orden —los dos lotes tienen proporción parecida— y la diferencia entre
    # una casa chica y una grande dejaba de verse. Con la banda, cada slot se
    # queda con el tramo de tamaño que de verdad le corresponde.
    if pool:
        tope = calce(pool[0], objetivo) + BANDA_DE_CALCE
        pool = [c for c in pool if calce(c, objetivo) <= tope]
    vistos, out = set(), []
    for c in pool:
        e = c["entrada"]
        r = e["rect"]
        firma = (e.get("png"), r["col"], r["row"], r["w"], r["h"])
        if firma in vistos:
            continue
        vistos.add(firma)
        out.append(c)
        if len(out) >= VARIANTES_MAX:
            break
    return out


#: Dónde vive el arte que salió del pack comprado. Lo dibujado a mano va en
#: `art/gen/` (skill `pixel-art`) y lo curado desde HA Studio en `art/edited/`.
DEL_PACK = "art/pack/"


def es_arte_del_pack(clave, sprites, mias):
    """¿La clave del lote YA es un sprite del pack por sí misma?

    Media docena de programas de lote apuntan directo al nombre que les puso el
    importador masivo (`building/reloj_torre`, `building/escuela`) porque el pack
    los dibujó enteros y con nombre propio. Ésos no necesitan fachada: ya SON la
    fachada, y pisarlos sería reemplazar el dibujo exacto por uno elegido por
    parecido.

    **"Que exista" no alcanza: tiene que venir del PACK.** Preguntando sólo si la
    clave está en el manifest, `building/bomberos` y `building/base_militar`
    contestaban que sí —y apuntaban a un placeholder DIBUJADO A MANO de 4×3
    celdas en `art/gen/mundo.png`, o sea un cuartel de bomberos de 4×3—. Como
    "ya eran arte", este script los saltaba y no les buscaba nunca la fachada de
    verdad, que el pack sí tiene (`fire_station` trae el cuartel entero de 16×15).
    Un placeholder es exactamente lo que hay que reemplazar cuando aparece el arte
    comprado; no es arte curado que haya que proteger.
    """
    e = sprites.get(clave)
    if e is None or clave in mias:
        return False
    png = e.get("png") if isinstance(e, dict) else e
    return isinstance(png, str) and png.startswith(DEL_PACK)


#: Cuánto puede agrandar el cliente una fachada sobre su propio pixel. Es
#: `ha-game::building::AGRANDE_MAX`, y se lee de ahí para que no haya dos copias.
def agrande_max():
    rs = os.path.join(ROOT, "native/crates/ha-game/src/building.rs")
    m = re.search(r"AGRANDE_MAX: f32 = ([0-9.]+);", open(rs, encoding="utf8").read())
    if not m:
        raise SystemExit("no pude leer AGRANDE_MAX de building.rs")
    return float(m.group(1))


def lotes_compuestos(sprites):
    """**Las parcelas cuya fachada se ARMA, y a las que por eso no se les toca el lote.**

    Ningún lote se achica nunca (ver [`reportar_huellas`]). Esto sigue haciendo
    falta para el INFORME: a un lote que compone su fachada no tiene sentido
    avisarle que "el arte no le llega", porque su fachada se arma del ancho que
    haga falta — el aviso es sólo para los que dependen de un dibujo único.

    Son dos familias, y las dos se detectan solas:

    1. **Las del kit modular** (`city::LOTES_MODULARES`): el ancho de la parcela
       está declarado para dar un número entero de módulos de 9 tiles.
    2. **La casa en hilera**: se reconoce por el ARTE, no por una lista — un slot
       que trae las poses `tramo` y `remate` es un kit
       (`ha-game::building::kit_de_hilera`). Achicar su parcela de 46 a 15 tiles,
       que es lo que pedía su dibujo suelto, convertía una hilera de cuatro
       viviendas en una sola.
    """
    txt = open(CITY_RS, encoding="utf8").read()
    m = re.search(r"pub const LOTES_MODULARES: &\[\(&str, u8, u8\)\] = &\[(.*?)\];", txt, re.S)
    out = set(re.findall(r'\("([^"]+)"', m.group(1))) if m else set()
    for clave, e in sprites.items():
        poses = e.get("poses") if isinstance(e, dict) else None
        if poses and "tramo" in poses and "remate" in poses:
            out.add(clave.split("#")[0])
    return out


def reportar_huellas(elegidas, objetivo, compuestos, _dry_run):
    """**Informe: qué lotes tienen un arte más chico que su parcela. NO toca nada.**

    Antes esto se llamaba `ajustar_huellas` y reescribía el `cells` de los
    `LotProgram` de `city.rs` para achicar el edificio hasta lo que su fachada
    tapaba. **Eso está prohibido desde el 24 ago 2026, por decisión del usuario**:

        "deja de estarme achicando edificios. Si te ocupa mil tiles, que ocupe
        los putos mil. Los assets están ahí, los usás del tamaño que son."

    O sea: el tamaño de un edificio es una decisión de diseño y la toma él, no un
    importador midiendo píxeles. Si una universidad ocupa 44×32, ocupa 44×32; que
    el arte del pack sea más chico es un problema de ARTE —hay que componer,
    repetir o dibujar más fachada— y no una excusa para encoger el edificio.

    Lo que sí sigue siendo útil es la MEDIDA, así que se imprime: es la lista de
    los lotes a los que les falta dibujo, o sea el trabajo de arte pendiente.
    """
    tope = agrande_max()
    print("\nHuellas contra el arte que les tocó (en tiles) — INFORME, no se toca nada:")
    faltantes = []
    for clave, ms in sorted(elegidas.items()):
        if clave in compuestos:
            print(f"  {clave:28s} su fachada se COMPONE — cubre el lote que haga falta")
            continue
        ow, oh = objetivo[clave]
        aw, ah = round(ms[0]["w"] * ESCALA_TILE), round(ms[0]["h"] * ESCALA_TILE)
        if aw * tope >= ow - 0.5:
            continue
        print(f"  {clave:28s} huella {ow}x{oh} → el arte mide {aw}x{ah}: NO LA TAPA")
        faltantes.append((clave, (ow, oh), (aw, ah)))
    if faltantes:
        print(
            f"\n  ⚠ {len(faltantes)} lote(s) con menos dibujo que parcela. El lote NO se achica:\n"
            "    hay que darles fachada (componer/repetir/dibujar), no encogerles el edificio."
        )
    return faltantes


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    juego = escala_del_juego()
    if abs(juego - ESCALA_TILE) > 1e-4:
        raise SystemExit(
            f"ESCALA_TILE={ESCALA_TILE} pero el juego dibuja con ESCALA_ARTE={juego}: "
            "las fachadas entrarían de otro tamaño que el resto del pack"
        )

    with open(MANIFEST) as fh:
        man = json.load(fh)
    sprites = man["sprites"]
    objetos = man.setdefault("objects", {})
    lotes = programas_de_lote()

    previo = {}
    if os.path.exists(INDEX):
        with open(INDEX) as fh:
            previo = json.load(fh)
    mias = {k.split("#")[0] for k in previo.get("sprites", [])}

    # **Todo lo que declara página, tenga lote o no.** `slots.rs` declara slots
    # curados (`building/mercado`, `building/comisaria`, `building/correo`,
    # `building/gasolinera`) que hoy NINGUNA parcela usa —la ciudad apunta a
    # `mercado_chico_1`, `comisaria_chica`, `correo_edificio`— pero que siguen en
    # el catálogo y los sigue mirando `ningun_slot_se_queda_sin_pixel`. Si este
    # script sólo recorre `LOT_PROGRAMS`, esos cuatro se quedan sin pixel en
    # cuanto la ciudad cambia de clave. Sin lote no hay huella que medir, así que
    # se les busca la fachada contra un edificio de talle medio.
    HUELLA_SIN_LOTE = (16, 12)
    objetivo = {k: HUELLA_SIN_LOTE for k in PAGINAS_DEL_LOTE}
    objetivo.update(lotes)

    cache = {}
    elegidas, sin_arte, propias = {}, [], []
    for clave, (ow, oh) in sorted(objetivo.items()):
        if not clave.startswith("building/"):
            continue  # `lot/plaza`, `lot/estacionamiento`: no llevan edificio
        if es_arte_del_pack(clave, sprites, mias):
            propias.append(clave)
            continue
        paginas = PAGINAS_DEL_LOTE.get(clave)
        if not paginas:
            sin_arte.append((clave, "sin páginas declaradas"))
            continue
        cand = candidatos(sprites, set(paginas), cache, mias)
        if not cand:
            sin_arte.append((clave, f"ningún edificio en {paginas}"))
            continue
        mejores = elegir(cand, (ow, oh))
        elegidas[clave] = mejores
        m = mejores[0]
        print(
            f"  {clave:28s} lote {ow:2d}x{oh:2d} → "
            f"{m['w'] * ESCALA_TILE:4.0f}x{m['h'] * ESCALA_TILE:4.0f} tiles "
            f"({len(mejores):2d} fachadas, calce {calce(m, (ow, oh)):.2f})  {m['clave'][:44]}"
        )
    for clave in propias:
        print(f"  {clave:28s} ya ES arte del pack — no se le busca fachada")
    for clave, motivo in sin_arte:
        print(f"  {clave:28s} SIN FACHADA — {motivo}")

    #: La parcela se ajusta al ARTE, no al revés: se estira el lote hasta que la
    #: casa entre entera en vez de aplastar el pixel (decisión tomada con el
    #: usuario). Lo que importa es el ANCHO — el alto lo resuelve el apoyo en el
    #: frente (`building::roof_layout`).
    # **Y los lotes que YA son arte del pack también se miden.** No se les elige
    # fachada —ya la tienen, con nombre propio— pero su parcela sí puede estar
    # declarada mucho más grande que el dibujo, y entonces pasa lo mismo que con
    # una fachada chica: el edificio queda de mota en un solar pelado. Medido:
    # `building/hangar` pedía 28 tiles de frente para un dibujo que tapa 15,
    # `building/universidad` 42 para 23 y `building/correo_edificio` 18 para 11.
    # Hasta ahora este paso ni los miraba, así que esos huecos no los veía nadie.
    for clave in propias:
        e = sprites.get(clave)
        r = e.get("rect") if isinstance(e, dict) else None
        if r:
            elegidas.setdefault(clave, [{"clave": clave, "w": r["w"], "h": r["h"], "entrada": e}])

    reportar_huellas(elegidas, objetivo, lotes_compuestos(sprites), args.dry_run)

    if args.dry_run:
        return

    # Se borra LO SUYO de la corrida anterior y nada más: el arte curado a mano
    # en HA Studio y lo que escriben los otros importadores no se toca.
    for k in previo.get("sprites", []):
        sprites.pop(k, None)
    for k in previo.get("objects", []):
        objetos.pop(k, None)

    nuevos, nuevos_obj, origen = [], [], {}
    for clave, ms in elegidas.items():
        if clave in propias:
            continue  # ya ES arte del pack: sólo se midió para ajustarle el lote
        # Apunta al MISMO png+rect del pack: no se copia ni un pixel.
        sprites[clave] = dict(ms[0]["entrada"])
        nuevos.append(clave)
        origen[clave] = [m["clave"] for m in ms]
        if len(ms) > 1:
            etiqueta = clave.rsplit("/", 1)[-1].replace("_", " ").capitalize()
            variantes = []
            for i, m in enumerate(ms, start=1):
                vid = str(i)
                sprites[f"{clave}#{vid}"] = dict(m["entrada"])
                nuevos.append(f"{clave}#{vid}")
                variantes.append({"id": vid, "label": f"{etiqueta} {i}"})
            objetos[clave] = {"label": etiqueta, "variants": variantes}
            nuevos_obj.append(clave)

    with open(MANIFEST, "w") as fh:
        json.dump(man, fh, indent=1, ensure_ascii=False, sort_keys=True)
    with open(INDEX, "w") as fh:
        json.dump(
            {"sprites": nuevos, "objects": nuevos_obj, "origen": origen},
            fh,
            indent=1,
            ensure_ascii=False,
        )
    print(
        f"\nmanifest: {len(elegidas)} lotes con fachada del pack, "
        f"{len(nuevos)} entradas ({len(nuevos_obj)} con variantes)"
    )


if __name__ == "__main__":
    main()
