#!/usr/bin/env python3
"""Las cinco pestañas del creador, recortadas al panel: ninguna se sale."""
import sys
from pathlib import Path
from PIL import Image, ImageDraw
sys.path.insert(0, str(Path(__file__).resolve().parent))
from mock_menu import crear_personaje, CAPAS
from parallax import Fondo
from uikit import texto
import personaje as PJ

CAJA = (1140, 130, 2400, 1030)          # el panel entero, con aire
ELEG = {"piel": 5, "ojos": 3, "ropa": 12, "pelo": 10, "accesorio": 6}   # números de ESTILO

if __name__ == "__main__":
    out = Path(sys.argv[1] if len(sys.argv) > 1 else ".")
    noche = Fondo("noche", 11)
    fondo = noche.cuadro(0, 20)
    recortes = []
    for et, capa in CAPAS:
        im = crear_personaje(fondo, capa, ELEG[capa]).convert("RGB").crop(CAJA)
        recortes.append((et, im))
    cols, esc = 3, 700
    ah = esc * recortes[0][1].height // recortes[0][1].width
    filas = (len(recortes) + cols - 1) // cols
    hoja = Image.new("RGB", (cols * (esc + 20) + 20, filas * (ah + 60) + 20), (26, 24, 34))
    d = ImageDraw.Draw(hoja)
    for i, (et, im) in enumerate(recortes):
        x = 20 + (i % cols) * (esc + 20)
        y = 20 + (i // cols) * (ah + 60) + 44
        hoja.paste(im.resize((esc, ah), Image.LANCZOS), (x, y))
        texto(d, (x + esc // 2, y - 24), et, 30, fill=(210, 200, 220),
              anchor="mm", sombra=False)
    hoja.save(out / "06d-pestanas-del-creador.png")
    print("listo")
