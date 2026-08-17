#!/usr/bin/env python3
"""**El MURO DE RIBERA: lo que convierte un río en un canal de ciudad.**

    python3 scripts/import-muelle.py --dry-run   # qué pose es cada pieza
    python3 scripts/import-muelle.py             # escribe el manifest

## Por qué

El pack trae, en `1_Terrains_and_Fences`, tres familias `Wall_*` con 58 piezas
entre las tres, y **el juego no usaba ninguna**. Son justo las que hacen el canal
urbano de las referencias de LimeZu: el agua encajonada entre dos muros de
mampostería con su coronamiento de piedra, en vez de un arroyo de orilla de pasto
cruzando el centro.

- **`Wall_1`** (20 piezas de 32×32) es el CORONAMIENTO: la plataforma vista desde
  arriba, con su labio oscuro del lado por donde cae. Es un autotile: cuatro
  bordes, cuatro esquinas, dos pasajes y ocho diagonales.
- **`Wall_2` y `Wall_3`** (19 cada una) son la CARA: el paramento de ladrillo que
  baja al agua, en tres alturas (32, 64 y 128 px).

Este script publica la primera como el autotile `quay/muro`, que es lo que se ve
en cenital. La cara de ladrillo ya está en el manifest desde el importador masivo
(`fence/pared_*`) y se coloca desde el editor.

## Cómo se lee la pose, sin tablas a mano

Igual que el cordón de `import-calles.py`: **del pixel**. Una pieza de
coronamiento tiene el LABIO —una banda oscura— del lado por donde el muro cae, y
alfa transparente donde la plataforma no llega. Los dos juntos son "el borde", y
qué franjas del recorte ocupa dice la pose:

- una sola franja → `edge_n` / `edge_s` / `edge_e` / `edge_w`
- dos adyacentes → `corner_nw`, `corner_ne`, `corner_se`, `corner_sw`
- dos opuestas   → un pasaje de una celda de ancho, que no es pose de autotile

Es la misma convención que `world2d::autotile_shape`: **la pose mira hacia el lado
DISTINTO**, o sea hacia el agua.

## Lo que esta familia no tiene

**Esquinas cóncavas (`inner_*`).** Se buscaron y no están dibujadas: las ocho
piezas 13–20 son diagonales de media celda, no muescas. El cliente cae al
`edge_*` del lado que corresponda, que en un canal se lee bien — un rincón
resuelto con un tramo recto no llama la atención; uno sin dibujar sí.

Anota lo suyo en `scripts/muelle-import-index.json`.
"""

import argparse
import json
import os
import re

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACK = os.path.join(
    ROOT,
    "PRUEBAS-CODIGOS/modernexteriors-win/Modern_Exteriors_32x32/ME_Theme_Sorter_32x32",
    "1_Terrains_and_Fences_Singles_32x32",
)
ART = os.path.join(ROOT, "native/crates/ha-game/assets/art")
MANIFEST = os.path.join(ART, "manifest.json")
PAGINA_REL = "art/pack/muelle.png"
INDEX = os.path.join(ROOT, "scripts/muelle-import-index.json")
CELL = 32

#: El slot. Uno solo: el coronamiento es lo que se ve desde arriba.
SLOT = "quay/muro"

#: Cuánto más oscuro que la mediana de la pieza tiene que ser un píxel para
#: contar como LABIO. Medido sobre `Wall_1`: el labio queda en 0,55–0,65 de la
#: mediana y la sombra suave del coronamiento no baja de 0,80.
LABIO = 0.72
#: Ancho de la franja de borde, en píxeles. Siete es lo que ocupa el labio con su
#: sombra; con menos, una pieza con el labio un pixel adentro deja de contarse.
FRANJA = 7
#: Qué fracción de una franja tiene que estar en el borde para que ese lado
#: cuente como "por acá cae el muro".
FRANJA_MIN = 0.5


def brillo(p):
    return (p[0] + p[1] + p[2]) / 3


def piezas(familia):
    """Los singles de una familia `Wall_N`, en el orden del pack."""
    rx = re.compile(rf"^ME_Singles_Terrains_and_Fences_32x32_Wall_{familia}_(\d+)\.png$")
    out = []
    for f in sorted(os.listdir(PACK)):
        m = rx.match(f)
        if m:
            out.append((int(m.group(1)), os.path.join(PACK, f)))
    return sorted(out)


def forma(im):
    """La pose de autotile de esta pieza de coronamiento, leída del pixel.

    Devuelve `None` para lo que no es una pose: la plataforma llena, el pasaje de
    una celda (dos lados opuestos) y las diagonales.
    """
    if im.size != (CELL, CELL):
        return None
    px = im.load()
    opacos = [(x, y) for y in range(CELL) for x in range(CELL) if px[x, y][3] > 128]
    if not opacos:
        return None
    mediana = sorted(brillo(px[p]) for p in opacos)[len(opacos) // 2]
    labio = {p for p in opacos if brillo(px[p]) < mediana * LABIO}
    vacio = {(x, y) for y in range(CELL) for x in range(CELL) if px[x, y][3] <= 128}
    borde = labio | vacio
    k = FRANJA
    franjas = {
        "n": {(x, y) for y in range(k) for x in range(k, CELL - k)},
        "s": {(x, y) for y in range(CELL - k, CELL) for x in range(k, CELL - k)},
        "w": {(x, y) for x in range(k) for y in range(k, CELL - k)},
        "e": {(x, y) for x in range(CELL - k, CELL) for y in range(k, CELL - k)},
    }
    lados = [d for d, cs in franjas.items() if len(borde & cs) / len(cs) > FRANJA_MIN]
    if len(lados) == 1:
        return f"edge_{lados[0]}"
    if len(lados) == 2:
        for a, b in (("n", "e"), ("n", "w"), ("s", "e"), ("s", "w")):
            if set(lados) == {a, b}:
                return f"corner_{a}{b}"
        # Dos lados opuestos: es el tramo de una celda de ancho, no una esquina.
        return None
    return None


#: Las poses que el cliente pide (`world2d::autotile_shape` menos `center` y
#: menos las cóncavas, que esta familia no trae). Si falta una, no se publica
#: nada: media familia deja huecos en el canal, que se ve peor que ningún muro.
POSES = ("edge_n", "edge_s", "edge_e", "edge_w", "corner_nw", "corner_ne", "corner_se", "corner_sw")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    elegidas, informe = {}, []
    for fam in (1, 2, 3):
        for n, ruta in piezas(fam):
            im = Image.open(ruta).convert("RGBA")
            pose = forma(im)
            informe.append((fam, n, im.size, pose))
            # Gana la PRIMERA pieza que da cada pose: el pack las ordena de la
            # variante lisa a las adornadas, y el muro de un canal quiere ser
            # liso. Las repetidas quedan anotadas en el informe.
            if pose and pose not in elegidas:
                elegidas[pose] = (fam, n, im)

    for fam, n, tam, pose in informe:
        marca = "→" if pose and elegidas.get(pose, (None, None))[1] == n and elegidas[pose][0] == fam else " "
        print(f" {marca} Wall_{fam}_{n:<3} {tam[0]:>3}x{tam[1]:<3} {pose or '-'}")

    faltan = [p for p in POSES if p not in elegidas]
    if faltan:
        raise SystemExit(
            f"a la familia de muro le faltan poses: {faltan}. "
            "Sin ellas el canal quedaría con huecos; no se publica nada."
        )
    print(f"\n{len(POSES)} poses del muro de ribera, de {len(informe)} piezas del pack")
    if args.dry_run:
        return

    lado = 1
    while lado * lado < len(POSES):
        lado += 1
    pagina = Image.new("RGBA", (lado * CELL, lado * CELL), (0, 0, 0, 0))
    poses = {}
    for i, pose in enumerate(POSES):
        c, r = i % lado, i // lado
        pagina.paste(elegidas[pose][2], (c * CELL, r * CELL))
        poses[pose] = {"rect": {"col": c, "row": r, "w": 1, "h": 1}}
    os.makedirs(os.path.join(ART, "pack"), exist_ok=True)
    pagina.save(os.path.join(ROOT, "native/crates/ha-game/assets", PAGINA_REL))

    with open(MANIFEST) as fh:
        man = json.load(fh)
    previo = {}
    if os.path.exists(INDEX):
        with open(INDEX) as fh:
            previo = json.load(fh)
    for k in previo.get("sprites", []):
        man["sprites"].pop(k, None)
    man["sprites"][SLOT] = {
        "grid": [CELL, CELL],
        # El muro se LEVANTA del suelo: Y-sorta como una pared, no como pintura.
        "layer": "Wall",
        "png": PAGINA_REL,
        "rect": dict(poses["edge_s"]["rect"]),
        "poses": poses,
    }
    with open(MANIFEST, "w") as fh:
        json.dump(man, fh, indent=1, ensure_ascii=False, sort_keys=True)
    with open(INDEX, "w") as fh:
        json.dump({"sprites": [SLOT], "poses": sorted(poses)}, fh, indent=1)
    print(f"{PAGINA_REL}: {SLOT} con {len(poses)} poses")


if __name__ == "__main__":
    main()
