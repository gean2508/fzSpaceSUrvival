#!/usr/bin/env python3
"""**LA LÁMINA DE REFERENCIA DE CALIDAD**: todo lo dibujado a mano que cumple el listón del 2 oct 2026.

    python3 scripts/galeria-arte.py [salida.png]

Antes de dibujar CUALQUIER pieza nueva, mirá esta lámina (`Read`): tu pieza tiene que poder ponerse al lado de
estas sin que se note la diferencia de oficio. Está hecha con `lib/volumen.py` + `lib/iconos.py` (ver la skill
`pixel-art` §0.0). Imprime la ruta de la imagen.
"""
import json
import sys
from pathlib import Path

from PIL import Image, ImageDraw

RAIZ = Path(__file__).resolve().parent.parent
ASSETS = RAIZ / "native/crates/ha-game/assets"
HOJAS = ["art/gen/comida.png", "art/gen/minerales.png", "art/gen/materias.png", "art/gen/mundo2.png", "art/gen/ui2.png", "art/gen/granja.png"]
ESCALA = 4


def main():
    salida = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("/tmp/galeria-arte.png")
    man = json.loads((ASSETS / "art/manifest.json").read_text())
    piezas = []
    for clave, e in sorted(man["sprites"].items()):
        if e.get("png") in HOJAS and "rect" in e and e["rect"]["w"] == 1 and e["rect"]["h"] == 1 and "tierra" not in clave:
            hoja = Image.open(ASSETS / e["png"]).convert("RGBA")
            r = e["rect"]
            piezas.append((clave, hoja.crop((r["col"] * 32, r["row"] * 32, (r["col"] + 1) * 32, (r["row"] + 1) * 32))))
    cols = 10
    celda = 32 * ESCALA + 6
    filas = (len(piezas) + cols - 1) // cols
    img = Image.new("RGBA", (cols * celda, filas * (celda + 12)), (96, 104, 96, 255))
    d = ImageDraw.Draw(img)
    for i, (clave, im) in enumerate(piezas):
        x, y = (i % cols) * celda, (i // cols) * (celda + 12)
        img.alpha_composite(im.resize((32 * ESCALA, 32 * ESCALA), Image.NEAREST), (x + 3, y + 3))
        d.text((x + 3, y + 32 * ESCALA + 3), clave.split("/")[-1][:18], fill=(235, 235, 235, 255))
    img.convert("RGB").save(salida)
    print(f"{len(piezas)} piezas → {salida}")


if __name__ == "__main__":
    main()
