#!/usr/bin/env python3
"""**AUDITA TODO LO DIBUJADO CONTRA EL LISTÓN** (`lib/volumen.py::medir_calidad`).

    python3 scripts/auditar-arte.py            # tabla por hoja de art/gen
    python3 scripts/auditar-arte.py --falla    # sólo lo que no pasa

Recorre las hojas de `art/gen/*.png` por celda de 32x32 y mide tonos con peso, dirección de luz y forma de
plantilla. Sirve para dos cosas: encontrar lo que sigue siendo «de Paint» y confirmar que una pieza nueva
está al nivel de las rehechas el 2 oct 2026. Los terrenos (losas que llenan la celda) se saltean.
"""
import json
import sys
from pathlib import Path

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib.volumen import medir_calidad, pasa_liston  # noqa: E402

RAIZ = Path(__file__).resolve().parent.parent
ASSETS = RAIZ / "native/crates/ha-game/assets"


def main():
    solo_fallas = "--falla" in sys.argv
    man = json.loads((ASSETS / "art/manifest.json").read_text())
    total = bien = 0
    for clave, e in sorted(man["sprites"].items()):
        png = e.get("png", "")
        if "/gen/" not in png or "rect" not in e or "suelo" in png or e["rect"]["w"] != 1 or e["rect"]["h"] != 1:
            continue
        g = e.get("grid", [32, 32])
        r = e["rect"]
        hoja = Image.open(ASSETS / png)
        celda = hoja.crop((r["col"] * g[0], r["row"] * g[1], (r["col"] + 1) * g[0], (r["row"] + 1) * g[1]))
        m = medir_calidad(celda)
        if m["n"] < 30:
            continue
        total += 1
        ok = not pasa_liston(m)
        bien += ok
        if not ok or not solo_fallas:
            print(f"{'OK ' if ok else 'MAL'} {clave:34s} tonos {m['tonos']:2d} luz {m['luz']:+5.1f} llena {m['llena']:.2f} sim {m['sim']:.2f}  {png}")
    print(f"\n{bien}/{total} piezas pasan el listón")


if __name__ == "__main__":
    main()
