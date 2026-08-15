#!/usr/bin/env python3
"""**El semáforo entero, armado como el pack manda.**

    python3 scripts/import-semaforos.py

## Por qué existe este script

El semáforo del pack **no es un sprite: es un kit**. Son ocho archivos y sólo
cuatro de ellos son un objeto:

| archivo | qué es | ¿va solo? |
|---|---|---|
| `Traffic_Lights_Off_Frontal_2` | el poste de frente, lámparas APAGADAS | sí |
| `Traffic_Lights_Back_2` | el mismo poste de espaldas | sí |
| `Traffic_Lights_Left_2` | de perfil, la caja asoma a la izquierda | sí |
| `Traffic_Lights_Right_2` | de perfil, la caja asoma a la derecha | sí |
| `Traffic_Lights_Colours_Mod_2` | **`Mod` = MODULAR**: sólo las tres lámparas | NO |
| `..._Red_Mod_2` / `..._Yellow_Mod_2` / `..._Green_Mod_2` | una lámpara prendida | NO |

Las cuatro `Mod` son CALCOMANÍAS: se dibujan ENCIMA del poste frontal, alineadas
al borde de arriba, y es lo que prende el semáforo. Solas son una caja de
lámparas flotando en el aire — que es exactamente lo que se veía en la esquina.

Y las que sí van solas no alcanzan sueltas: el poste frontal viene apagado. Un
cruce con los cuatro semáforos en gris no se lee como un cruce.

Así que este script hace lo único que faltaba: **componer**. Toma el poste
frontal, le pega la calcomanía de colores encima y de ahí sale
`road/semaforo_frente`, la cara viva. Las otras tres caras entran tal cual.

## Lo que produce

`art/pack/semaforos.png` — cuatro celdas de 1×5 tiles, una por rumbo — y cuatro
entradas del manifest. Las consume `ha_shared::worldgen::SEMAFOROS`, que le
asigna una a cada esquina del cruce según a qué brazo detiene
(`roads::semaforo_at`).

Anota lo suyo en `scripts/semaforos-import-index.json`, así una segunda corrida
reemplaza sus claves sin tocar el resto del manifest.
"""

import json
import os

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACK = os.path.join(
    ROOT,
    "PRUEBAS-CODIGOS/modernexteriors-win/Modern_Exteriors_32x32/ME_Theme_Sorter_32x32",
    "3_City_Props_Singles_32x32",
)
ASSETS = os.path.join(ROOT, "native/crates/ha-game/assets")
MANIFEST = os.path.join(ASSETS, "art/manifest.json")
PAGINA_REL = "art/pack/semaforos.png"
INDEX = os.path.join(ROOT, "scripts/semaforos-import-index.json")
CELL = 32
PREFIJO = "ME_Singles_City_Props_32x32_Traffic_Lights_"

#: El semáforo alto del pack mide 1×5 tiles. La familia `_1` es la versión corta
#: (1×4) y no se usa: cuatro esquinas con dos alturas distintas se ven como un
#: error, no como variedad.
ANCHO, ALTO = 1, 5

#: **Las cuatro caras, y cuál lleva calcomanía.** La clave es el slot del juego;
#: el valor es `(poste, encima)`. Sólo la de frente se compone: es la única cara
#: desde la que se ven las lámparas, así que es la única que puede estar prendida.
CARAS = {
    "road/semaforo_frente": ("Off_Frontal_2", "Colours_Mod_2"),
    "road/semaforo_atras": ("Back_2", None),
    "road/semaforo_izquierda": ("Left_2", None),
    "road/semaforo_derecha": ("Right_2", None),
}


def pieza(nombre):
    ruta = os.path.join(PACK, f"{PREFIJO}{nombre}.png")
    if not os.path.exists(ruta):
        raise SystemExit(f"falta la pieza del pack: {ruta}")
    return Image.open(ruta).convert("RGBA")


def armar(poste, encima):
    """El poste con su calcomanía puesta, o el poste tal cual si no lleva.

    La calcomanía va **alineada arriba** y no centrada: las `Mod` del pack son el
    recorte de la caja de lámparas desde el borde superior del poste hacia abajo,
    así que el (0,0) de las dos imágenes es el mismo punto del dibujo. Alinear de
    cualquier otra forma corre las lámparas dentro de la caja.
    """
    base = pieza(poste)
    if base.size != (ANCHO * CELL, ALTO * CELL):
        raise SystemExit(f"{poste} mide {base.size}, se esperaba {ANCHO}×{ALTO} tiles")
    if encima is None:
        return base
    calco = pieza(encima)
    if calco.width != base.width or calco.height > base.height:
        raise SystemExit(f"{encima} ({calco.size}) no calza sobre {poste} ({base.size})")
    out = base.copy()
    out.alpha_composite(calco, (0, 0))
    return out


def main():
    caras = {clave: armar(*fuente) for clave, fuente in CARAS.items()}

    pagina = Image.new("RGBA", (len(caras) * ANCHO * CELL, ALTO * CELL), (0, 0, 0, 0))
    sitio = {}
    for i, (clave, im) in enumerate(caras.items()):
        pagina.paste(im, (i * ANCHO * CELL, 0))
        sitio[clave] = i * ANCHO
    os.makedirs(os.path.join(ASSETS, "art/pack"), exist_ok=True)
    pagina.save(os.path.join(ASSETS, PAGINA_REL), optimize=True)
    print(f"{PAGINA_REL}: {pagina.size[0]}x{pagina.size[1]} px, {len(caras)} caras")

    with open(MANIFEST) as fh:
        man = json.load(fh)
    sprites = man.setdefault("sprites", {})
    if os.path.exists(INDEX):
        with open(INDEX) as fh:
            for k in json.load(fh).get("sprites", []):
                sprites.pop(k, None)

    nuevos = []
    for clave, col in sitio.items():
        sprites[clave] = {
            "png": PAGINA_REL,
            "grid": [CELL, CELL],
            "rect": {"col": col, "row": 0, "w": ANCHO, "h": ALTO},
        }
        nuevos.append(clave)

    with open(MANIFEST, "w") as fh:
        json.dump(man, fh, indent=1, ensure_ascii=False, sort_keys=True)
    with open(INDEX, "w") as fh:
        json.dump({"sprites": sorted(nuevos), "pagina": PAGINA_REL}, fh, indent=1)
    print(f"manifest: {len(nuevos)} claves de semáforo")


if __name__ == "__main__":
    main()
