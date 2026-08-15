#!/usr/bin/env python3
"""**MIRAR EL PACK ANTES DE DIBUJAR NADA.**

    python3 scripts/ver-pack.py "*Yard_Toy*"            # lámina de contacto
    python3 scripts/ver-pack.py "*Toilet*" --max 60

Existe por un error concreto: el 14 ago 2026 se dibujaron 132 piezas "porque los
singles de Modern Interiors están numerados y mirarlos uno a uno cuesta horas".
Cuesta un minuto — esto arma una lámina con el nombre debajo de cada recorte y se
mira de un vistazo. La regla 0 de la skill `pixel-art` (buscar en el pack antes
que nada) no se puede cumplir sin ver el pack.

Busca en los tres packs a la vez y ordena por nombre. `--max` corta (por defecto
120, que es lo que entra legible en una lámina).
"""

import sys
from fnmatch import fnmatch
from pathlib import Path

from PIL import Image, ImageDraw

RAIZ = Path(__file__).resolve().parent.parent
PACKS = RAIZ / "PRUEBAS-CODIGOS"
SALIDA = Path(
    "/tmp/claude-1000/-home-fzion-Documents-mods-phaser-arena/"
    "c5b6a2ce-15de-4761-b31b-4fd9fd419911/scratchpad/pack"
)


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    patron = args[0] if args else "*"
    zoom = 3
    maximo = 120
    if "--max" in sys.argv:
        maximo = int(sys.argv[sys.argv.index("--max") + 1])
    if "--zoom" in sys.argv:
        zoom = int(sys.argv[sys.argv.index("--zoom") + 1])
    nombre = args[1] if len(args) > 1 else "lamina"

    fs = sorted(f for f in PACKS.rglob("*.png") if fnmatch(f.name, patron))
    print(f"{len(fs)} archivos coinciden con {patron!r}")
    if not fs:
        return 1
    fs = fs[:maximo]

    ims = []
    for f in fs:
        try:
            im = Image.open(f).convert("RGBA")
        except Exception:
            continue
        if im.width > 32 * 6 or im.height > 32 * 6:
            im.thumbnail((32 * 6, 32 * 6), Image.NEAREST)
        ims.append((f.stem, im))

    cols = 10
    cw = max(i.width for _, i in ims) * zoom + 10
    ch = max(i.height for _, i in ims) * zoom + 26
    filas = (len(ims) + cols - 1) // cols
    hoja = Image.new("RGBA", (cw * cols, ch * filas), (34, 34, 42, 255))
    d = ImageDraw.Draw(hoja)
    for k, (n, im) in enumerate(ims):
        r, c = divmod(k, cols)
        z = im.resize((im.width * zoom, im.height * zoom), Image.NEAREST)
        hoja.alpha_composite(z, (c * cw + 5, r * ch + 20))
        corto = n.replace("ME_Singles_", "").replace("_32x32_", "·").replace("_Singles", "")
        d.text((c * cw + 3, r * ch + 4), f"{k}:{corto[:30]}", fill=(235, 235, 245, 255))
    SALIDA.mkdir(parents=True, exist_ok=True)
    out = SALIDA / f"{nombre}.png"
    hoja.save(out)
    print(out)
    for k, (n, _) in enumerate(ims):
        print(f"  {k:3} {n}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
