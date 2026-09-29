#!/usr/bin/env python3
"""**LAS CARNES DE LA GRANJA, DEL PACK.**

    python3 scripts/import-granja-items.py [--dry-run]

Modern Interiors trae comida suelta en sus temas numerados (Kitchen, Grocery
Store). Se miró la lámina de cada tema (skill `pixel-art`, puerta 1) y tres
singles son exactamente lo que la granja necesita como icono:

- `Grocery_Store_Singles_32x32_304` — un bife crudo → **carne de res**.
- `Kitchen_Singles_32x32_215` — una feta de jamón rosado → **carne de cerdo**.
- `Kitchen_Singles_32x32_216` — un corte con hueso → **carne de cordero**.

Lo demás (pollo, conejo, pluma, lana, hueso, huevo, etiqueta) no existe en los
packs y lo dibuja `dibujar-granja.py`.

No se escribe ningún rect a mano: cada single se pega en una hoja de 32 px de
celda y el rect sale de dónde cayó. Si el pack cambia y un single falta, se
planta.
"""

import json
import sys
from pathlib import Path

from PIL import Image

RAIZ = Path(__file__).resolve().parent.parent
TEMAS = RAIZ / "PRUEBAS-CODIGOS/moderninteriors-win (Copy 1)/1_Interiors/32x32/Theme_Sorter_Singles_32x32"
HOJA_REL = "art/pack/granja_items.png"

ELEGIDOS = [
    ("items/beef", "16_Grocery_Store_Singles_32x32/Grocery_Store_Singles_32x32_304.png", "Carne de res"),
    ("items/pork", "12_Kitchen_Singles_32x32/Kitchen_Singles_32x32_215.png", "Carne de cerdo"),
    ("items/mutton", "12_Kitchen_Singles_32x32/Kitchen_Singles_32x32_216.png", "Carne de cordero"),
]


def main():
    seco = "--dry-run" in sys.argv
    hoja = Image.new("RGBA", (32 * len(ELEGIDOS), 32), (0, 0, 0, 0))
    sprites, objetos = {}, {}
    for i, (clave, rel, nombre) in enumerate(ELEGIDOS):
        ruta = TEMAS / rel
        if not ruta.exists():
            raise SystemExit(f"el pack cambió: falta {ruta}")
        im = Image.open(ruta).convert("RGBA")
        if im.size != (32, 32):
            raise SystemExit(f"{rel} mide {im.size}, se esperaba una celda de 32")
        hoja.paste(im, (32 * i, 0))
        sprites[clave] = {"grid": [32, 32], "png": HOJA_REL, "rect": {"col": i, "row": 0, "w": 1, "h": 1}}
        objetos[clave] = nombre
    if seco:
        print(f"[dry-run] {len(sprites)} íconos del pack")
        return 0
    assets = RAIZ / "native/crates/ha-game/assets"
    hoja.save(assets / HOJA_REL)
    man_p = assets / "art/manifest.json"
    man = json.loads(man_p.read_text())
    man["sprites"].update(sprites)
    for k, v in objetos.items():
        man.setdefault("objects", {}).setdefault(k, {})["label"] = v
    man_p.write_text(json.dumps(man, ensure_ascii=False, indent=1))
    print(f"{HOJA_REL}: {len(sprites)} íconos al manifest")
    return 0


if __name__ == "__main__":
    sys.exit(main())
