#!/usr/bin/env python3
"""Importa objetos del pack AL JUEGO, identificados por su NOMBRE de archivo.

    python3 scripts/import-pack-objects.py scripts/pack-objects.tsv
    python3 scripts/import-pack-objects.py scripts/pack-objects.tsv --no-copy

Los packs de exteriores nombran cada objeto (`..._Sleeping_Bag_1.png`), así que
no hace falta adivinar por índice — y sobre todo, no hace falta depender del
orden del filesystem, que NO es determinista y hacía que la misma tabla apuntara
a sprites distintos según la máquina.

Entrada TSV:  tema <TAB> nombre_en_el_pack <TAB> clave <TAB> nombre_castellano <TAB> categoria <TAB> material
Para los interiores (que se llaman `Bedroom_Singles_32x32_137.png`) el
"nombre_en_el_pack" es el número, que sí es único y estable.

Escribe tres cosas:
  - los PNG bajo `ha-game/assets/art/<carpeta>/<clave>.png` + su entrada en
    `manifest.json` (salvo con `--no-copy`),
  - `/tmp/gen_slots.txt` y `/tmp/gen_places.txt`, que `apply.py` mete entre los
    marcadores `// === PACK:START/END ===` de `slots.rs` y `placeables.rs`.

El bloque PACK se regenera ENTERO: no edites a mano lo que hay ahí dentro, se
pierde en la próxima importación. Lo que sí es tuyo es todo lo de FUERA del
bloque — y este script lo respeta (ver `curated_keys`).

**Los tamaños salen del PNG**, no de una estimación: `320x224 px` son `10x7`
tiles. Es lo que destapó que la carpa estaba declarada 2x2 midiendo 4x4 y el
helicóptero 4x6 midiendo 10x7.
"""
import csv, glob, json, os, re, shutil, struct, sys, unicodedata

ROOT = "/home/fzion/Documents/mods/phaser-arena"
ART = f"{ROOT}/native/crates/ha-game/assets/art"
MANIFEST = f"{ART}/manifest.json"
PACK = f"{ROOT}/PRUEBAS-CODIGOS"
I = "moderninteriors-win (Copy 1)/1_Interiors/32x32/Theme_Sorter_Singles_32x32"
E = "modernexteriors-win/Modern_Exteriors_32x32/ME_Theme_Sorter_32x32"

THEMES = {
    "kitchen": f"{I}/12_Kitchen_Singles_32x32",
    "living": f"{I}/2_Living_Room_Singles_32x32",
    "bath": f"{I}/3_Bathroom_Singles_32x32",
    "bedroom": f"{I}/4_Bedroom_Singles_32x32",
    "camping": f"{E}/11_Camping_Singles_32x32",
    "garden": f"{E}/17_Garden_Singles_32x32",
    "city": f"{E}/3_City_Props_Singles_32x32",
    "vehicles": f"{E}/10_Vehicles_Singles_32x32",
    "beach": f"{E}/21_Beach_Singles_32x32",
}
FOLDER = {
    "Mueble": "furniture", "Calle": "prop", "Parcela": "prop",
    "Naturaleza": "nature", "Agua": "agua", "Vehiculo": "vehicle",
    "Edificio": "building", "Estructura": "structure",
}


def png_size(p):
    with open(p, "rb") as f:
        f.read(16)
        return struct.unpack(">II", f.read(8))


def pick(theme, name):
    """El PNG de `name` en `theme`. Determinista: entre los que matchean, el
    primero por orden alfabético del nombre completo."""
    d = os.path.join(PACK, THEMES[theme])
    cands = []
    for f in sorted(glob.glob(os.path.join(d, "*.png"))):
        b = os.path.basename(f).replace(".png", "")
        stem = re.sub(r"^ME_Singles_.*?_32x32_", "", b)
        stem = re.sub(r"^\w+_Singles_32x32_", "", stem)
        if stem == name or re.fullmatch(rf"{re.escape(name)}(_\d+)?", stem) \
           or re.fullmatch(rf"{re.escape(name)}_(Left|Right|Up|Down|Front|Back)(_\d+)?", stem):
            cands.append(f)
    return cands[0] if cands else None


def ascii_label(s):
    return unicodedata.normalize("NFKD", s).encode("ascii", "ignore").decode()


def curated_keys():
    """Claves que YA declara el catálogo a mano, o sea todo lo que está FUERA del
    bloque PACK:START/PACK:END de `slots.rs`.

    El importador tiene que respetarlas: si `vehicle/auto` ya existe curado, este
    pack no lo declara de nuevo — sólo le da su PNG. Sin esta comprobación
    salían claves duplicadas (`prop/carpa`, `vehicle/auto`,
    `vehicle/helicoptero`), el objeto aparecía DOS VECES en la lista del editor y
    quedaban dos entradas de colocación para el mismo arte con tamaños distintos
    (la carpa como 2x2 y como 4x4). El PNG manda: 128x128 px son 4x4 tiles.
    """
    src = open(f"{ROOT}/native/crates/ha-assets/src/slots.rs").read()
    a, b = src.index("// === PACK:START ==="), src.index("// === PACK:END ===")
    outside = src[:a] + src[b:]
    return set(re.findall(r'^\s*\("([^"]+)"', outside, re.M))


def main(tsv, copy=True):
    manifest = json.load(open(MANIFEST))
    curated = curated_keys()
    slots, places, seen, missing, already = [], [], set(), [], []
    with open(tsv) as f:
        for r in csv.reader(f, delimiter="\t"):
            if not r or r[0].startswith("#"):
                continue
            theme, name, key, label, cat, mat = r
            src = pick(theme, name)
            if not src:
                missing.append(f"{theme}:{name}")
                continue
            w, h = png_size(src)
            tw, th = max(1, min(16, round(w / 32))), max(1, min(16, round(h / 32)))
            folder = FOLDER[cat]
            slot = f"{folder}/{key}"
            if slot in seen:
                missing.append(f"clave repetida {slot}")
                continue
            seen.add(slot)
            if copy:
                os.makedirs(os.path.join(ART, folder), exist_ok=True)
                shutil.copyfile(src, os.path.join(ART, folder, f"{key}.png"))
                manifest["sprites"][slot] = {"png": f"art/{folder}/{key}.png"}
            # El objeto ya existe curado a mano: le damos el ARTE, no lo
            # declaramos otra vez. La entrada curada tiene mejor nombre (con
            # tildes) y es la que no se pisa al reimportar.
            if slot in curated:
                already.append(f"{slot} ({tw}x{th} tiles)")
                continue
            slots.append(f'    ("{slot}", "{label}"),')
            places.append(
                f'    p("{folder}_{key}", "{ascii_label(label)}", "{slot}", '
                f"PlaceCat::{cat}, Material::{mat}, {tw}, {th}),"
            )
    if copy:
        json.dump(manifest, open(MANIFEST, "w"), indent=2, ensure_ascii=False)
    open("/tmp/gen_slots.txt", "w").write("\n".join(slots) + "\n")
    open("/tmp/gen_places.txt", "w").write("\n".join(places) + "\n")
    print(f"{len(slots)} declarados nuevos")
    if already:
        print(
            f"YA CURADOS ({len(already)}) — sólo reciben el PNG, no se declaran de nuevo:\n  "
            + "\n  ".join(already)
        )
    if missing:
        print(f"NO ENCONTRADOS ({len(missing)}): " + ", ".join(missing[:25]), file=sys.stderr)


if __name__ == "__main__":
    # `--no-copy` regenera sólo las tablas (los PNG y el manifest ya están).
    main(sys.argv[1], copy="--no-copy" not in sys.argv)
