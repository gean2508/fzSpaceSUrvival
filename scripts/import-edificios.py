#!/usr/bin/env python3
"""**Las casas del mapa, con la fachada entera del pack.**

    python3 scripts/import-edificios.py --dry-run   # qué fachada le tocó a cada lote
    python3 scripts/import-edificios.py             # escribe el manifest

## Por qué

`ha-shared::city::LOT_PROGRAMS` declara qué es cada parcela de una ciudad
(`building/casa_chica`, `building/hospital`, …) y `ha-game::building` ya dibuja
esa clave como un sprite **encima** de la casa que se **transparenta al entrar**
— el "entrás por la puerta y ves el interior sin cortes". Lo que faltaba es que
esas claves tuvieran pixel: las 25 estaban vacías, así que cada casa del mundo
se dibujaba con el cuadrado marrón de respaldo.

El pack SÍ las tiene, y al tamaño justo: la escuela mide 36×34 tiles, el hotel
40×32, la comisaría 32×27, las casas victorianas 27×33.

## Cómo elige, sin man-in-the-loop

Dos filtros, en este orden:

1. **El TEMA manda.** Cada clave del pack lleva su tema adelante
   (`building/casas_…`, `building/escuela_…`, `building/comisaria_…`), y cada
   lote declara de qué tema es. Una casa sólo puede recibir una fachada de casa;
   una escuela, una de escuela. Es la misma regla con la que se amueblan los
   interiores, así que nada queda descolgado.
2. **Y dentro del tema, el TAMAÑO.** Gana la fachada cuya huella (celdas × 1,5
   tiles) más se parece a la del lote, midiendo área y proporción. El nombre no
   entra: el importador masivo bautiza automático y llama `casas_casa_*` hasta a
   una maceta de 1×1.

Se descarta lo que no puede ser un edificio: menos de 5×4 celdas, o con menos del
`OPACIDAD_MIN` de su recorte dibujado (un cartel suelto, una antena).

No copia píxeles: la entrada del manifest apunta al mismo `png` y `rect` que el
pack ya tiene. Anota lo suyo en `scripts/edificios-import-index.json`.
"""

import argparse
import json
import os

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ASSETS = os.path.join(ROOT, "native/crates/ha-game/assets")
MANIFEST = os.path.join(ASSETS, "art/manifest.json")
CITY_RS = os.path.join(ROOT, "native/crates/ha-shared/src/city.rs")
INDEX = os.path.join(ROOT, "scripts/edificios-import-index.json")

#: Una celda de 32 px del pack son 1,5 tiles del juego (ver `art2d::ESCALA_ARTE`).
ESCALA_TILE = 1.5
#: Mínimo para que un recorte pueda ser un edificio, en celdas.
MIN_CELDAS = (5, 4)
#: Fracción mínima del recorte que tiene que estar dibujada. Una fachada cubre
#: casi todo su rectángulo; un cartel, una antena o una composición de jardín
#: (sombrilla + mesa + cantero) dejan medio recorte en aire.
OPACIDAD_MIN = 0.55
#: Y además tiene que estar APOYADA: la fila de abajo del recorte, dibujada en
#: buena parte. Separa un edificio —que se planta en el suelo— de un conjunto de
#: muebles sueltos que casualmente llena su caja. Flojo a propósito: una fachada
#: en 3/4 tiene sombra y aire en las puntas de su fila de abajo.
BASE_MIN = 0.50
#: **Se probó y NO se usa: el contraste techo/pared.** La idea era descartar los
#: recortes de techo suelto mirando si el tercio de arriba y el de abajo son de
#: colores distintos (una fachada tiene cubierta arriba y muro abajo). Funciona
#: para las casas y ARRUINA los edificios cívicos: en este pack la escuela, la
#: comisaría y la estación están dibujadas casi todas de techo, así que el filtro
#: las dejaba a todas sin fachada. Queda anotado para no volver a intentarlo.

#: **Tema del pack de cada programa de lote.** Es la única decisión escrita a
#: mano de todo el pipeline, y es la que no puede salir de un hash: qué clase de
#: edificio es cada parcela. De acá salen la fachada Y (fase siguiente) los
#: muebles de adentro, así que lo de afuera y lo de adentro nunca se despegan.
TEMA_DEL_LOTE = {
    "building/casa_chica": ["casas_", "villas_"],
    "building/casa_mediana": ["casas_", "villas_"],
    "building/casa_grande": ["casas_", "villas_"],
    "building/casa_country": ["villas_", "casas_"],
    "building/villa": ["villas_"],
    "building/duplex": ["casas_", "edificios_"],
    "building/garaje": ["venta_de_garaje_"],
    "building/tienda": ["shopping_y_mercado_", "almacen_"],
    "building/mercado": ["shopping_y_mercado_"],
    "building/shopping": ["shopping_y_mercado_"],
    "building/gasolinera": ["calle_", "obra_"],
    "building/oficina": ["oficina_", "edificios_"],
    "building/torre": ["edificios_", "oficina_"],
    "building/hotel": ["hotel_y_hospital_"],
    "building/hospital": ["hotel_y_hospital_"],
    "building/universidad": ["escuela_"],
    "building/escuela": ["escuela_"],
    "building/comisaria": ["comisaria_"],
    "building/bomberos": ["bomberos_"],
    "building/correo": ["correo_"],
    "building/estacion_tren": ["tren_y_subte_"],
    "building/deposito": ["obra_", "base_militar_"],
    "building/granero": ["jardin_", "obra_"],
    "building/nave_industrial": ["base_militar_", "obra_"],
    "building/base_militar": ["base_militar_"],
}


def programas_de_lote():
    """`clave -> (ancho, alto)` en tiles, leído de `LOT_PROGRAMS` en city.rs."""
    import re

    txt = open(CITY_RS, encoding="utf8").read()
    out = {}
    for clave, w, h in re.findall(
        r'LotProgram \{ key: "([^"]+)", cells: \((\d+), (\d+)\)', txt
    ):
        out[clave] = (int(w), int(h))
    if not out:
        raise SystemExit("no pude leer LOT_PROGRAMS de city.rs")
    return out


def candidatos(sprites, temas):
    """Los recortes del pack de esos temas que pueden ser un edificio."""
    cache = {}
    out = []
    for clave, e in sprites.items():
        if "#" in clave or not clave.startswith("building/"):
            continue
        familia = clave[len("building/") :]
        if not any(familia.startswith(t) for t in temas):
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
        ancho = im.size[0]
        opaco = sum(1 for p in pix if p > 0) / len(pix)
        if opaco < OPACIDAD_MIN:
            continue
        base = pix[-ancho:]
        if sum(1 for p in base if p > 0) / ancho < BASE_MIN:
            continue
        out.append({"clave": clave, "w": r["w"], "h": r["h"], "opaco": opaco, "entrada": e})
    return out


def calce(cand, objetivo):
    """Cuánto se aleja esta fachada del tamaño del lote (0 = calza justo)."""
    cw, ch = cand["w"] * ESCALA_TILE, cand["h"] * ESCALA_TILE
    ow, oh = objetivo
    # Diferencia relativa de cada lado + de la proporción: un edificio del área
    # correcta pero el doble de ancho que largo no sirve.
    lado = abs(cw - ow) / ow + abs(ch - oh) / oh
    prop = abs((cw / ch) - (ow / oh)) / (ow / oh)
    return lado + prop


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    with open(MANIFEST) as fh:
        man = json.load(fh)
    sprites = man["sprites"]
    lotes = programas_de_lote()

    elegidas, sin_arte = {}, []
    for clave, (ow, oh) in sorted(lotes.items()):
        if not clave.startswith("building/"):
            continue  # `lot/plaza`, `lot/estacionamiento`: no llevan edificio
        temas = TEMA_DEL_LOTE.get(clave)
        if not temas:
            sin_arte.append((clave, "sin tema declarado"))
            continue
        cand = candidatos(sprites, temas)
        if not cand:
            sin_arte.append((clave, f"ningún edificio en {temas}"))
            continue
        mejor = min(cand, key=lambda c: calce(c, (ow, oh)))
        elegidas[clave] = mejor
        print(
            f"  {clave:28s} lote {ow:2d}x{oh:2d} → "
            f"{mejor['w'] * ESCALA_TILE:4.0f}x{mejor['h'] * ESCALA_TILE:4.0f} tiles "
            f"(calce {calce(mejor, (ow, oh)):.2f})  {mejor['clave'][:52]}"
        )
    for clave, motivo in sin_arte:
        print(f"  {clave:28s} SIN FACHADA — {motivo}")

    #: El tamaño del lote pasa a ser el del ARTE: se achica la parcela al dibujo
    #: en vez de estirar el pixel (decisión tomada con el usuario).
    print("\nTamaños de lote que habría que ajustar al arte:")
    for clave, m in elegidas.items():
        ow, oh = lotes[clave]
        aw, ah = round(m["w"] * ESCALA_TILE), round(m["h"] * ESCALA_TILE)
        if (aw, ah) != (ow, oh):
            print(f"  {clave:28s} {ow}x{oh} → {aw}x{ah}")

    if args.dry_run:
        return

    previo = {}
    if os.path.exists(INDEX):
        with open(INDEX) as fh:
            previo = json.load(fh)
        for k in previo.get("sprites", []):
            sprites.pop(k, None)

    nuevos = []
    for clave, m in elegidas.items():
        # Apunta al MISMO png+rect del pack: no se copia ni un pixel.
        sprites[clave] = dict(m["entrada"])
        nuevos.append(clave)

    with open(MANIFEST, "w") as fh:
        json.dump(man, fh, indent=1, ensure_ascii=False, sort_keys=True)
    with open(INDEX, "w") as fh:
        json.dump({"sprites": nuevos, "origen": {k: v["clave"] for k, v in elegidas.items()}}, fh, indent=1)
    print(f"\nmanifest: {len(nuevos)} lotes con fachada del pack")


if __name__ == "__main__":
    main()
