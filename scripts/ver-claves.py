#!/usr/bin/env python3
"""**MIRAR CLAVES QUE YA ESTÁN EN EL MANIFEST.**

    python3 scripts/ver-claves.py boya bote muelle           # por subcadena
    python3 scripts/ver-claves.py --exacto agua/boya_1 agua/bote

Hermano de `ver-pack.py`, pero sobre el arte YA IMPORTADO. Existe por el mismo
error y uno peor: el 14 ago 2026 se dibujaron piezas que no sólo estaban en el
pack sino que **ya estaban importadas en el manifest** con nombre en castellano
(`agua/estrella_de_mar_azul_grande`, `prop/pozos_1`, `anim/trampoline`…). Antes
de dibujar hay que mirar acá, no sólo en `PRUEBAS-CODIGOS/`.
"""

import json
import sys
from pathlib import Path

from PIL import Image, ImageDraw

RAIZ = Path(__file__).resolve().parent.parent
ASSETS = RAIZ / "native/crates/ha-game/assets"
SALIDA = Path(
    "/tmp/claude-1000/-home-fzion-Documents-mods-phaser-arena/"
    "c5b6a2ce-15de-4761-b31b-4fd9fd419911/scratchpad/pack"
)
_cache: dict = {}


def recorte(man, clave):
    """El pixel de una clave del manifest, con su primera pose si tiene poses."""
    e = man["sprites"][clave]
    if not isinstance(e, dict) or "png" not in e:
        return None
    gw, gh = e.get("grid", [32, 32])
    r = e.get("rect") or e.get("strip")
    if r is None:
        poses = e.get("poses") or {}
        if not poses:
            return None
        p = poses.get("center") or next(iter(poses.values()))
        r = p["rect"]
        gw, gh = p.get("grid", (gw, gh))
    ruta = ASSETS / e["png"]
    if not ruta.exists():
        return None
    if ruta not in _cache:
        _cache[ruta] = Image.open(ruta).convert("RGBA")
    im = _cache[ruta]
    return im.crop(
        (r["col"] * gw, r["row"] * gh, (r["col"] + r["w"]) * gw, (r["row"] + r["h"]) * gh)
    )


def main():
    man = json.loads((ASSETS / "art/manifest.json").read_text())
    exacto = "--exacto" in sys.argv
    nombre = "claves"
    if "--out" in sys.argv:
        nombre = sys.argv[sys.argv.index("--out") + 1]
    terminos = [
        a
        for i, a in enumerate(sys.argv[1:], 1)
        if not a.startswith("--") and sys.argv[i - 1] != "--out"
    ]
    if exacto:
        claves = [t for t in terminos if t in man["sprites"]]
    else:
        claves = [k for k in sorted(man["sprites"]) if any(t in k.lower() for t in terminos)]
    print(f"{len(claves)} claves")
    claves = claves[:120]
    ims = []
    for k in claves:
        im = recorte(man, k)
        if im is None or im.width == 0:
            continue
        if im.width > 32 * 5 or im.height > 32 * 5:
            im.thumbnail((32 * 5, 32 * 5), Image.NEAREST)
        ims.append((k, im))
    if not ims:
        print("ninguna dibujable")
        return 1
    Z, COLS = 3, 10
    cw = max(i.width for _, i in ims) * Z + 10
    ch = max(i.height for _, i in ims) * Z + 24
    filas = (len(ims) + COLS - 1) // COLS
    hoja = Image.new("RGBA", (cw * COLS, ch * filas), (34, 34, 42, 255))
    d = ImageDraw.Draw(hoja)
    for i, (k, im) in enumerate(ims):
        r, c = divmod(i, COLS)
        hoja.alpha_composite(im.resize((im.width * Z, im.height * Z), Image.NEAREST), (c * cw + 5, r * ch + 18))
        d.text((c * cw + 3, r * ch + 3), k[:34], fill=(235, 235, 245, 255))
    SALIDA.mkdir(parents=True, exist_ok=True)
    out = SALIDA / f"{nombre}.png"
    hoja.save(out)
    print(out)
    for k, _ in ims:
        print("  ", k)
    return 0


if __name__ == "__main__":
    sys.exit(main())
