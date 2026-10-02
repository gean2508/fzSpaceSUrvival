#!/usr/bin/env python3
"""**LA ROPA QUE SE ENCUENTRA EN EL MUNDO, DEL PACK.**

    python3 scripts/import-ropa.py [--dry-run]

El creador de personajes tiene 33 estilos de ropa (`ROPA 01..33`) y varias gorras. Para
que se puedan ENCONTRAR (ropero, cadáveres) hace falta un ícono por prenda — y Modern
Interiors los trae: `21_Clothing_Store_Singles_32x32` tiene las prendas dobladas
(13..45) en todos los colores, tres gorras (233..235) y calzado (150..156). Se miró la
lámina numerada (skill `pixel-art`, puerta 1) y se eligió así:

- `ropa_01..ropa_33`  → single 12+n (13..45), una prenda por estilo del creador.
- `gorra_roja` 234 · `gorra_azul` 233 · `gorro_gris` 235 → GORRA / GORRA DE POLICIA / GORRO DE LANA.
- `botas_1..7` → singles 150..156 (calzado; sólo protege, no cambia el look).

Nada se dibuja: son los píxeles del pack. El rect sale de dónde cayó cada celda; el
nombre en castellano sale del color medio del ícono. También escribe
`native/crates/ha-assets/src/ropa_items.rs` (ids + nombres) que `slots.rs` suma al
catálogo, así falla el test de etiquetas si algo queda sin nombre.
"""

import colorsys
import json
import sys
from pathlib import Path

from PIL import Image

RAIZ = Path(__file__).resolve().parent.parent
TEMA = RAIZ / "PRUEBAS-CODIGOS/moderninteriors-win (Copy 1)/1_Interiors/32x32/Theme_Sorter_Singles_32x32/21_Clothing_Store_Singles_32x32"
HOJA_REL = "art/pack/ropa_items.png"


def single(n: int) -> Image.Image:
    p = TEMA / f"Clothing_Store_Singles_32x32_{n}.png"
    if not p.exists():
        raise SystemExit(f"el pack cambió: falta {p}")
    im = Image.open(p).convert("RGBA")
    if im.size != (32, 32):
        raise SystemExit(f"{p.name} mide {im.size}, se esperaba 32x32")
    return im


def color_de(im: Image.Image) -> str:
    """El nombre del color dominante, sin contar el contorno oscuro ni el blanco de los cuellos."""
    px = [p for p in im.getdata() if p[3] > 128 and 70 < (0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2]) < 235]
    if not px:
        return "gris"
    sat = [(colorsys.rgb_to_hsv(p[0] / 255, p[1] / 255, p[2] / 255), p) for p in px]
    vivos = [(h, s, v) for (h, s, v), _ in sat if s > 0.28]
    if len(vivos) < len(px) * 0.25:
        medio = sum(v for _, _, v in [s[0] for s in sat]) / len(sat)
        return "blanca" if medio > 0.8 else ("gris" if medio > 0.4 else "negra")
    h = sorted(x[0] for x in vivos)[len(vivos) // 2] * 360
    for tope, nombre in [(15, "roja"), (40, "naranja"), (70, "amarilla"), (165, "verde"), (200, "turquesa"), (255, "azul"), (290, "violeta"), (335, "rosa"), (361, "roja")]:
        if h < tope:
            return nombre
    return "gris"


def main():
    seco = "--dry-run" in sys.argv
    elegidos = []  # (id, single, rotulo)
    for n in range(1, 34):
        elegidos.append((f"ropa_{n:02d}", 12 + n, "Ropa"))
    elegidos += [("gorra_roja", 234, "Gorra"), ("gorra_azul", 233, "Gorra"), ("gorro_gris", 235, "Gorro")]
    for k, n in enumerate(range(150, 157), 1):
        elegidos.append((f"botas_{k}", n, "Calzado"))
    # Las 4 prendas FABRICABLES se dibujaron a mano y el crítico las midió demasiado lisas
    # (35-52 % de píxeles planos contra 9 % del pack): el arte real gana, se reemplazan.
    ALIAS = [("leather_vest", 29, "Chaleco de cuero"), ("wool_coat", 19, "Abrigo de lana"),
             ("leather_cap", 235, "Gorro de cuero"), ("leather_boots", 150, "Botas de cuero")]
    cols = 12
    elegidos += [(a, n, "Alias") for a, n, _ in ALIAS]
    filas = (len(elegidos) + cols - 1) // cols
    hoja = Image.new("RGBA", (32 * cols, 32 * filas), (0, 0, 0, 0))
    sprites, objetos, ids = {}, {}, []
    cuenta = {}
    for i, (iid, n, rot) in enumerate(elegidos):
        im = single(n)
        c, r = i % cols, i // cols
        hoja.paste(im, (32 * c, 32 * r))
        color = color_de(im)
        cuenta[(rot, color)] = cuenta.get((rot, color), 0) + 1
        nombre = f"{rot} {color}" if rot != "Gorro" else "Gorro de lana gris"
        if rot == "Gorra":
            nombre = "Gorra " + ("roja" if "roja" in iid else "azul")
        if rot == "Ropa":
            nombre = f"Ropa {int(iid[-2:]):02d} ({color})"
        if rot == "Calzado":
            nombre = f"Calzado {iid[-1]} ({color})"
        if rot == "Alias":
            nombre = next(x[2] for x in ALIAS if x[0] == iid)
            ids_alias = True
        clave = f"items/{iid}"
        sprites[clave] = {"grid": [32, 32], "png": HOJA_REL, "rect": {"col": c, "row": r, "w": 1, "h": 1}}
        objetos[clave] = nombre
        if rot != "Alias":  # los alias ya están en slots.rs (ITEM_IDS)
            ids.append((iid, nombre))
    if seco:
        print(f"[dry-run] {len(ids)} prendas")
        for iid, nom in ids:
            print(f"  {iid:12s} {nom}")
        return 0
    assets = RAIZ / "native/crates/ha-game/assets"
    hoja.save(assets / HOJA_REL)
    man_p = assets / "art/manifest.json"
    man = json.loads(man_p.read_text())
    man["sprites"].update(sprites)
    for k, v in objetos.items():
        man.setdefault("objects", {}).setdefault(k, {})["label"] = v
    man_p.write_text(json.dumps(man, ensure_ascii=False, indent=1))
    rs = ["// GENERADO por scripts/import-ropa.py — no editar a mano.", "pub const ROPA_IDS: &[(&str, &str)] = &["]
    rs += [f'    ("{i}", "{n}"),' for i, n in ids]
    rs.append("];")
    (RAIZ / "native/crates/ha-assets/src/ropa_items.rs").write_text("\n".join(rs) + "\n")
    print(f"{HOJA_REL}: {len(ids)} prendas al manifest + ropa_items.rs")
    return 0


if __name__ == "__main__":
    sys.exit(main())
