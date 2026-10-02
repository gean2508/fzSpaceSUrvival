#!/usr/bin/env python3
"""**EL MACIZO DEL SUBSUELO, CON LA ROCA DEL PACK Y NO UN COLOR PLANO.**

    python3 scripts/dibujar-subsuelo.py            # la hoja del suelo y el manifest
    python3 scripts/dibujar-subsuelo.py --preview out.png

Reclamo del usuario (2 oct 2026): *«cuando cavo parece que acabo de romper el juego… las
texturas se ven como si faltara un cuadrado de algo»*. Lo que se veía: el macizo era un
color plano casi negro (`tiles/subsuelo` no tenía losa en la hoja del suelo y caía al color
del tile), y la galería, un piso claro recortado en la nada. No había pared, no había roca.

Acá el macizo sale de la **roca empedrada que el pack ya dibuja** (`tiles/roca`, el adoquín
gris de Modern Exteriors), llevada a una rampa fría y oscura de la paleta del pack
(`lib/pixel.py::rampa`) conservando SU estructura —las lajas y las juntas—, de modo que se
lee como roca de verdad y no como tinta. Las cuatro variantes son la misma losa girada y
espejada (el adoquín es continuo), así que el macizo no repite un patrón evidente.

Cada bioma subterráneo (`BIOMAS`) arma su propia rampa — piedra gris azulada, arcilla
rojiza, cristal violeta, musgo verdoso— con la MISMA estructura de lajas: la identidad
del bioma es el color y las vetas, no una textura distinta que haya que dibujar de cero.

Las losas van a celdas libres de `art/pack/suelo.png` (la hoja que lee la malla del
suelo) como `tiles/subsuelo` y `tiles/subsuelo#m1..#m3`, y se corren desde
`atlas-suelo.py` al final, porque ese reescribe la hoja.
"""

import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib.pixel import ajustar, luz, rampa  # noqa: E402

RAIZ = Path(__file__).resolve().parent.parent
ASSETS = RAIZ / "native/crates/ha-game/assets"
MANIFEST = ASSETS / "art/manifest.json"
HOJA = ASSETS / "art/pack/suelo.png"
GEN = ASSETS / "art/gen/suelo"
CELL = 32

#: Los biomas subterráneos y el color base de su roca (la rampa sale de la paleta del pack).
#: El primero (`piedra`) es el del macizo de siempre: `tiles/subsuelo`.
BIOMAS = {
    "piedra": (92, 90, 108),
    "arcilla": (118, 84, 78),
    "cristal": (96, 84, 124),
    "musgo": (78, 98, 86),
    "hierro": (104, 88, 80),
}


def sacar_roca(man, hoja) -> Image.Image:
    """La losa de roca empedrada del pack (`tiles/roca`), tal como está en la hoja."""
    e = man["sprites"]["tiles/roca"]["rect"]
    return hoja.crop((e["col"] * CELL, e["row"] * CELL, (e["col"] + 1) * CELL, (e["row"] + 1) * CELL)).convert("RGBA")


def llevar_a_rampa(roca: Image.Image, base) -> Image.Image:
    """Mismos relieves (juntas y lajas), otro material: la luminancia de la roca decide qué
    tono de la rampa le toca a cada píxel. La roca queda DENTRO de un rango oscuro: es el
    macizo, lo que rodea a la galería, y lo que se mira es la galería."""
    tonos = rampa(base, 6)
    lum = np.array([[luz(roca.getpixel((x, y))[:3]) for x in range(CELL)] for y in range(CELL)], dtype=np.float32)
    lo, hi = float(np.percentile(lum, 3)), float(np.percentile(lum, 97))
    out = Image.new("RGBA", (CELL, CELL), (0, 0, 0, 255))
    for y in range(CELL):
        for x in range(CELL):
            t = (lum[y, x] - lo) / max(hi - lo, 1.0)
            i = int(round(max(0.0, min(1.0, t)) * (len(tonos) - 1)))
            out.putpixel((x, y), (*tonos[i], 255))
    return out


def variantes(im: Image.Image) -> list[Image.Image]:
    """La misma losa girada y espejada: cuatro caras de un empedrado continuo."""
    return [
        im,
        im.transpose(Image.FLIP_LEFT_RIGHT),
        im.transpose(Image.ROTATE_180),
        im.transpose(Image.FLIP_TOP_BOTTOM),
    ]


def celdas_libres(man, hoja):
    usadas = set()
    for e in man["sprites"].values():
        if e.get("png") == "art/pack/suelo.png" and "rect" in e:
            r = e["rect"]
            for dc in range(r["w"]):
                for dr in range(r["h"]):
                    usadas.add((r["col"] + dc, r["row"] + dr))
    for row in range(hoja.height // CELL):
        for col in range(hoja.width // CELL):
            if (col, row) in usadas:
                continue
            if hoja.crop((col * CELL, row * CELL, (col + 1) * CELL, (row + 1) * CELL)).getbbox() is None:
                yield col, row


def main():
    prev = sys.argv[sys.argv.index("--preview") + 1] if "--preview" in sys.argv else None
    man = json.loads(MANIFEST.read_text())
    hoja = Image.open(HOJA).convert("RGBA")
    GEN.mkdir(parents=True, exist_ok=True)
    roca = sacar_roca(man, hoja)
    previas = {k: e["rect"] for k, e in man["sprites"].items() if k.startswith("tiles/subsuelo") and e.get("png") == "art/pack/suelo.png"}
    libres = celdas_libres(man, hoja)
    lamina = []
    for bioma, base in BIOMAS.items():
        clave_base = "tiles/subsuelo" if bioma == "piedra" else f"tiles/subsuelo_{bioma}"
        losas = variantes(llevar_a_rampa(roca, base))
        lamina.append(losas)
        obj = man["objects"].setdefault(clave_base, {"label": f"Subsuelo: {bioma}", "variants": []})
        ids = {v["id"] for v in obj["variants"]}
        for i, im in enumerate(losas):
            clave = clave_base if i == 0 else f"{clave_base}#m{i}"
            im.save(GEN / f"subsuelo_{bioma}_{i}.png")
            if clave in previas:
                col, row = previas[clave]["col"], previas[clave]["row"]
            else:
                col, row = next(libres)
            hoja.paste(im, (col * CELL, row * CELL))
            man["sprites"][clave] = {
                "grid": [CELL, CELL],
                "layer": "Floor",
                "png": "art/pack/suelo.png",
                "rect": {"col": col, "row": row, "w": 1, "h": 1},
            }
            if i and f"m{i}" not in ids:
                obj["variants"].append({"id": f"m{i}", "label": f"{bioma} {i}"})
        print(f"{clave_base}: 4 losas en rampa {base}")
    hoja.save(HOJA, optimize=True)
    MANIFEST.write_text(json.dumps(man, ensure_ascii=False, indent=1, sort_keys=True))
    if prev:
        w = CELL * 4
        sheet = Image.new("RGBA", (w, CELL * len(lamina)), (0, 0, 0, 255))
        for r, fila in enumerate(lamina):
            for c, im in enumerate(fila):
                sheet.paste(im, (c * CELL, r * CELL))
        sheet.resize((sheet.width * 4, sheet.height * 4), Image.NEAREST).convert("RGB").save(prev)


if __name__ == "__main__":
    main()
