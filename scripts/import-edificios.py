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
#: variantes. Doce alcanza para que una manzana de ocho parcelas no repita, y
#: cuesta cero: son entradas de manifest apuntando a píxeles que ya están en la
#: página.
VARIANTES_MAX = 12
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
    "building/villa": ["villas", "additional_houses"],
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


def es_pieza(clave):
    """¿El pack dibujó esto como PARTE de un edificio? Ver [`MARCAS_DE_PIEZA`]."""
    fam = clave.split("/", 1)[-1]
    return any(rx.search(fam) for rx in MARCAS_DE_PIEZA)


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
        if apoya(pix, ancho, alto) < BASE_MIN:
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


def es_arte_del_pack(clave, sprites, mias):
    """¿La clave del lote YA es un sprite del pack por sí misma?

    Media docena de programas de lote apuntan directo al nombre que les puso el
    importador masivo (`building/almacen_1`, `building/reloj_torre`) porque el
    pack los dibujó enteros y con nombre propio. Ésos no necesitan fachada: ya
    SON la fachada, y pisarlos sería reemplazar el dibujo exacto por uno elegido
    por parecido.
    """
    return clave in sprites and clave not in mias


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
    print("\nHuellas que habría que ajustar al arte (ancho de la casa, en tiles):")
    for clave, ms in elegidas.items():
        ow, oh = objetivo[clave]
        aw, ah = round(ms[0]["w"] * ESCALA_TILE), round(ms[0]["h"] * ESCALA_TILE)
        if abs(aw - ow) > 1:
            print(f"  {clave:28s} huella {ow}x{oh} → arte {aw}x{ah}")

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
