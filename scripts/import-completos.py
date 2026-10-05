#!/usr/bin/env python3
"""**LOS EDIFICIOS ENTEROS DEL PACK, TAL CUAL LOS DIBUJÓ LimeZu — uno por archivo, con su nombre de verdad.**

    python3 scripts/import-completos.py --dry-run     # qué le toca a cada lote
    python3 scripts/import-completos.py               # empaqueta + escribe el manifest

## Por qué existe

El importador masivo corta las HOJAS de atlas por islas de píxel, bautiza lo que sale con un nombre automático y
agrupa por nombre: cuando dos objetos distintos terminan con el mismo nombre castellano uno pasa a ser
«variante» del otro. El resultado, en el mapa: una casa entera con el módulo que venía pegado en la hoja al lado, o
una **universidad que es el hospital** (`building/universidad` apuntaba al mismo recorte que `building/hospital`),
o una oficina, una torre y un dúplex con el mismo dibujo.

El pack trae además cada pieza SUELTA en su carpeta `*_Singles_32x32`: **un archivo = un objeto entero**, y el
nombre del archivo dice qué es (`Country_House`, `Palace_Example_1`, `School_1`). Este script lee esas carpetas
—no las hojas—, copia cada edificio ENTERO a `art/pack/completos_N.png` y publica:

* `completo/<tema>_<nombre>` — una clave por edificio, con el nombre del archivo del pack (para mirarlas en HA Studio).
* Los slots de lote (`building/casa_chica`, `…/universidad`, …) con sus variantes `#1…#n` apuntando a ésas.

Las piezas modulares (`*_Modular_*`, `Banister`, `Canopy`, `Side_View`, `Roof`…) **no se usan**: el sistema de
construcción ya resuelve lo modular, y esas piezas por sí solas son un techo o una baranda.

**Orden de la cadena**: `import-pack-masivo → import-casas → import-edificios → import-completos →
medir-puertas → import-puertas-edificios` (este va último entre los de fachada: manda sobre los slots que nombra).
"""

import argparse
import glob
import json
import os
import re

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ASSETS = os.path.join(ROOT, "native/crates/ha-game/assets")
MANIFEST = os.path.join(ASSETS, "art/manifest.json")
SORTER = os.path.join(ROOT, "PRUEBAS-CODIGOS/modernexteriors-win/Modern_Exteriors_32x32/ME_Theme_Sorter_32x32")
INDEX = os.path.join(ROOT, "scripts/completos-import-index.json")
CELDA = 32
ANCHO_PAGINA = 2048
ALTO_MAX = 2048

#: slot de lote → [(carpeta de la hoja, nombre EXACTO del archivo sin prefijo ni «_32x32.png»), …]
A = "24_Additional_Houses_Singles_32x32"
V = "7_Villas_Singles_32x32"
G = "4_Generic_Building_Singles_32x32"
SLOTS = {
    # **Pocas variantes por lote, a propósito**: cada edificio entero son 1-4 MB de textura en una GPU móvil con la RAM
    # justa (el Redmi mataba procesos con 49 MB de fachadas). El barrio varía igual: la variante se sortea por parcela.
    "building/casa_chica": [(A, "Post_Apocalyptic_House_1"), (A, "Terraced_House_1"), (A, "Terraced_House_3")],
    "building/casa_mediana": [(A, "One_Story_House"), (A, "Japanese_House")],
    "building/casa_grande": [(A, "Country_House")],
    "building/casa_country": [(A, "Modern_House")],
    # **Los caserones con torre de reloj NO son casas**: son el ayuntamiento (lote cívico propio), no una vivienda de barrio.
    "building/ayuntamiento": [(A, "Victorian_House_4"), (A, "Victorian_House_5")],
    "building/villa": [(V, "Villa_1"), (V, "Villa_3")],
    "building/duplex": [(G, "Condo_4_38")],
    "building/oficina": [("16_Office_Singles_32x32", "Example_1"), ("16_Office_Singles_32x32", "Example_2")],
    "building/torre": [(G, "Condo_6")],
    "building/hospital": [("12_Hotel_and_Hospital_Singles_32x32", "Hospital_1")],
    "building/universidad": [("17_Garden_Singles_32x32", "Palace_Example_1")],
    "building/escuela": [("13_School_Singles_32x32", "School_1")],
    "building/comisaria": [("15_Police_Station_Singles_32x32", "Police_Station_1")],
    "building/tienda": [(G, "Hardware_Store")],
    "building/deposito": [("23_MIlitary_Base_Singles_32x32", f"Dormitory_{i}") for i in (1, 4)],
    # El granero tenía por arte el escaparate de una tienda: lleva los mismos galpones de madera que el depósito.
    "building/granero": [("23_MIlitary_Base_Singles_32x32", f"Dormitory_{i}") for i in (1, 4)],
    "building/correo_edificio": [("22_Post_Office_Singles_32x32", "Building_1")],
}


def archivo(carpeta, nombre):
    # Dos convenciones de nombre en el pack: `<tema>_<Nombre>_32x32.png` y `ME_Singles_<tema>_32x32_<Nombre>.png`.
    # El nombre tiene que terminar el archivo y arrancar tras un «_»: `Country_House` NO es `Country_House_No_Banisters`.
    g = [
        f for f in glob.glob(os.path.join(SORTER, carpeta, "*.png"))
        if os.path.basename(f).endswith(f"_{nombre}_32x32.png") or os.path.basename(f).endswith(f"_32x32_{nombre}.png")
    ]
    if len(g) != 1:
        raise SystemExit(f"{carpeta}/{nombre}: se esperaba UN archivo, hay {len(g)}: {g}")
    return g[0]


def slug(carpeta, nombre):
    tema = re.sub(r"^\d+_", "", carpeta).replace("_Singles_32x32", "").lower()
    return f"completo/{tema}_{nombre.lower()}"


def empaquetar(imagenes):
    """Estantes de 1024 px de ancho; página nueva al pasar de ALTO_MAX. Devuelve páginas y {clave: (pagina, col, fila)}."""
    paginas, pos = [], {}
    x = y = alto_fila = 0
    cur = Image.new("RGBA", (ANCHO_PAGINA, ALTO_MAX), (0, 0, 0, 0))
    usado = 0
    for clave, im in imagenes:
        w, h = im.size
        if x + w > ANCHO_PAGINA:
            x, y, alto_fila = 0, y + alto_fila, 0
        if y + h > ALTO_MAX:
            paginas.append(cur.crop((0, 0, ANCHO_PAGINA, max(usado, CELDA))))
            cur = Image.new("RGBA", (ANCHO_PAGINA, ALTO_MAX), (0, 0, 0, 0))
            x = y = alto_fila = usado = 0
        cur.paste(im, (x, y))
        pos[clave] = (len(paginas), x // CELDA, y // CELDA)
        alto_fila = max(alto_fila, h)
        usado = max(usado, y + h)
        x += w
    paginas.append(cur.crop((0, 0, ANCHO_PAGINA, max(usado, CELDA))))
    return paginas, pos


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    unicos = {}
    for slot, lista in SLOTS.items():
        for carpeta, nombre in lista:
            f = archivo(carpeta, nombre)
            im = Image.open(f).convert("RGBA")
            if im.width % CELDA or im.height % CELDA:
                raise SystemExit(f"{nombre}: {im.size} no es múltiplo de la celda")
            unicos[slug(carpeta, nombre)] = im
    for slot, lista in SLOTS.items():
        print(f"  {slot:28s} " + ", ".join(f"{n} {unicos[slug(c, n)].width // CELDA}x{unicos[slug(c, n)].height // CELDA}" for c, n in lista))
    if args.dry_run:
        return

    orden = []
    for lista in SLOTS.values():
        for c, n in lista:
            if (slug(c, n), unicos[slug(c, n)]) not in orden:
                orden.append((slug(c, n), unicos[slug(c, n)]))
    # Estantes por ALTO decreciente: el desperdicio de un estante es lo que sobra bajo los más bajos.
    orden.sort(key=lambda kv: (-kv[1].height, kv[0]))
    paginas, pos = empaquetar(orden)
    with open(MANIFEST) as fh:
        man = json.load(fh)
    sprites, objetos, origen = man["sprites"], man.setdefault("objects", {}), man.setdefault("origen", {})
    previo = json.load(open(INDEX)) if os.path.exists(INDEX) else {"sprites": [], "objects": [], "paginas": []}
    for k in previo["sprites"]:
        sprites.pop(k, None)
    for k in previo["objects"]:
        objetos.pop(k, None)
    for p in previo["paginas"]:
        origen.pop(p, None)
        ruta = os.path.join(ASSETS, p)
        if os.path.exists(ruta):
            os.remove(ruta)

    nombres_pag = []
    for i, pg in enumerate(paginas):
        rel = f"art/pack/completos_{i}.png"
        pg.save(os.path.join(ASSETS, rel), optimize=True)
        origen[rel] = "exterior"
        nombres_pag.append(rel)

    def entrada(clave):
        p, c, f = pos[clave]
        im = unicos[clave]
        return {"grid": [CELDA, CELDA], "png": nombres_pag[p], "rect": {"col": c, "row": f, "w": im.width // CELDA, "h": im.height // CELDA}}

    nuevos, nuevos_obj = [], []
    for clave in unicos:
        sprites[clave] = entrada(clave)
        nuevos.append(clave)
    for slot, lista in SLOTS.items():
        for k in [k for k in sprites if k.startswith(slot + "#")]:
            del sprites[k]                                    # las variantes viejas eran recortes de hoja: afuera
        claves = [slug(c, n) for c, n in lista]
        sprites[slot] = entrada(claves[0])
        nuevos.append(slot)
        if len(claves) > 1:
            etiqueta = slot.rsplit("/", 1)[-1].replace("_", " ").capitalize()
            variantes = []
            for i, k in enumerate(claves, start=1):
                sprites[f"{slot}#{i}"] = entrada(k)
                nuevos.append(f"{slot}#{i}")
                variantes.append({"id": str(i), "label": f"{etiqueta} {i}"})
            objetos[slot] = {"label": etiqueta, "variants": variantes}
            nuevos_obj.append(slot)
    with open(MANIFEST, "w") as fh:
        json.dump(man, fh, indent=1, ensure_ascii=False, sort_keys=True)
    with open(INDEX, "w") as fh:
        json.dump({"sprites": nuevos, "objects": nuevos_obj, "paginas": nombres_pag}, fh, indent=1, ensure_ascii=False)
    print(f"\n{len(unicos)} edificios enteros en {len(paginas)} páginas, {len(SLOTS)} slots de lote")


if __name__ == "__main__":
    main()
