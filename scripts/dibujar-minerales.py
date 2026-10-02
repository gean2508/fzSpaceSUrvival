#!/usr/bin/env python3
"""**MENAS Y GEMAS QUE SE VEAN COMO DEL PACK** (reemplazan las elipses de `dibujar-mundo.py`).

    python3 scripts/dibujar-minerales.py --preview out.png   # sólo mira
    python3 scripts/dibujar-minerales.py                      # escribe art/gen/minerales.png + manifest

La roca de cada veta sale de `lib/volumen.py` (silueta irregular + cúpula iluminada desde arriba-izquierda +
la textura REAL de la roca del pack como relieve), y el mineral va incrustado: granos con tono claro
arriba-izquierda y oscuro abajo-derecha, o cristales facetados con su cara de luz y su cara de sombra.
"""
import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib.pixel import CELDA, Lienzo, Pagina, luz, publicar, rampa  # noqa: E402
from lib.volumen import blob, esculpir, motas  # noqa: E402

RAIZ = Path(__file__).resolve().parent.parent
ASSETS = RAIZ / "native/crates/ha-game/assets"
CELL = 32


def textura_de_roca() -> np.ndarray:
    man = json.loads((ASSETS / "art/manifest.json").read_text())
    r = man["sprites"]["tiles/roca"]["rect"]
    hoja = Image.open(ASSETS / "art/pack/suelo.png").convert("RGB")
    t = hoja.crop((r["col"] * CELL, r["row"] * CELL, (r["col"] + 1) * CELL, (r["row"] + 1) * CELL))
    a = np.asarray(t).astype(np.float32)
    return (a * [0.299, 0.587, 0.114]).sum(-1)


ROCA = rampa((120, 120, 134), 5)


def roca_con(semilla: int, rx=10.5, ry=9.5, cx=16.0, cy=21.0):
    """La piedra de la veta: irregular, con volumen y el grano de la roca del pack."""
    l = Lienzo()
    mask = blob(CELL, CELL, cx, cy, rx, ry, semilla, rugosidad=0.22)
    tex = np.tile(textura_de_roca(), (2, 2))[:CELL, :CELL]
    l.sombra_al_pie(int(cx - rx + 1), int(2 * rx - 2), y=int(cy + ry - 2), alto=3)
    esculpir(l, mask, ROCA, textura=tex, fuerza_textura=0.6, base_oscura=0.2, relieve=1.5)
    return l, mask


def mena(color, semilla, n=5, brillo=False):
    def f():
        l, mask = roca_con(semilla)
        motas(l, mask, color, n, semilla * 7 + 1, tam=(1, 1))
        l.contorno()
        l.verificar()
        return l
    return f


def cristal(l, base, x, y, w, h, tilt=0):
    """Un prisma facetado: cara izquierda clara, derecha oscura, punta con brillo."""
    c = rampa(base, 5)
    cuerpo_izq = [(x, y + h), (x + w * 0.5, y + h - 1), (x + w * 0.5 + tilt, y + 2), (x + tilt * 0.5, y + 5)]
    cuerpo_der = [(x + w * 0.5, y + h - 1), (x + w, y + h), (x + w - tilt * 0.3, y + 5), (x + w * 0.5 + tilt, y + 2)]
    punta = [(x + tilt * 0.5, y + 5), (x + w * 0.5 + tilt, y + 2), (x + w - tilt * 0.3, y + 5), (x + w * 0.5 + tilt * 0.8, y - 1)]
    l.poligono(cuerpo_izq, c[3])
    l.poligono(cuerpo_der, c[1])
    l.poligono(punta, c[4])
    # filo claro de la arista central y brillo puntual
    l.linea(int(x + w * 0.5), int(y + h - 2), int(x + w * 0.5 + tilt), int(y + 3), c[4])
    l.punto(int(x + w * 0.5 + tilt * 0.7), int(y + 1), (255, 255, 255))


def gema(base, semilla):
    def f():
        l, mask = roca_con(semilla, rx=10.0, ry=6.0, cy=24.0)
        cristal(l, base, 7, 9, 8, 15, 1)
        cristal(l, base, 15, 5, 9, 19, -1)
        cristal(l, base, 11, 14, 7, 12, 0)
        l.contorno()
        l.verificar()
        return l
    return f


PIEZAS = {
    "decor/ore_iron": mena((176, 98, 62), 3, 6),
    "decor/ore_gold": mena((236, 190, 70), 5, 5),
    "decor/gem_blue": gema((66, 120, 214), 8),
    "decor/gem_red": gema((206, 64, 74), 9),
}


def main():
    prev = sys.argv[sys.argv.index("--preview") + 1] if "--preview" in sys.argv else None
    ims = {k: f().im for k, f in PIEZAS.items()}
    if prev:
        man = json.loads((ASSETS / "art/manifest.json").read_text())
        hoja_r = Image.open(ASSETS / "art/decor/rock.png").convert("RGBA")
        vecinos = [hoja_r.crop((0, 0, CELL, CELL))]
        ims = {**{f"pack{i}": v for i, v in enumerate(vecinos)}, **ims}
        sh = Image.new("RGBA", (CELL * len(ims) * 6 + 10 * len(ims), CELL * 6), (52, 70, 56, 255))
        for i, im in enumerate(ims.values()):
            sh.alpha_composite(im.resize((CELL * 6, CELL * 6), Image.NEAREST), (i * (CELL * 6 + 10), 0))
        sh.convert("RGB").save(prev)
        return
    # Los ítems de mochila que son estos mismos materiales (contorno de Interiors).
    from lib.iconos import bulto, lingote, polvo, silueta

    def cristal_icono(base, semilla):
        l = Lienzo()
        cristal(l, base, 10, 6, 8, 17, 1)
        cristal(l, base, 16, 10, 7, 14, -1)
        silueta(l)
        l.verificar(apoyado=False)
        return l.im

    ims_items = {
        "items/gem_blue": cristal_icono((66, 120, 214), 1),
        "items/gem_red": cristal_icono((206, 64, 74), 2),
        "items/iron_ingot": lingote((176, 182, 190)).im,
        "items/gold_ingot": lingote((226, 178, 58)).im,
        "items/stone": bulto((130, 130, 144), 21, 9, 7.5, True, None, 0).im,
        "items/dirt": bulto((140, 100, 66), 22, 9.5, 7.5, False, None, 0, 1.7, 0.0).im,
        "items/sand": polvo((222, 200, 150), 5).im,
    }
    ims.update(ims_items)
    HOJA_REL = "art/gen/minerales.png"
    pag = Pagina()
    sprites, objetos = {}, {}
    etiquetas = {"decor/ore_iron": "Veta de hierro", "decor/ore_gold": "Veta de oro", "decor/gem_blue": "Gema azul", "decor/gem_red": "Gema roja",
                 "items/gem_blue": "Gema azul", "items/gem_red": "Gema roja", "items/iron_ingot": "Lingote de hierro", "items/gold_ingot": "Lingote de oro",
                 "items/stone": "Piedra", "items/dirt": "Tierra", "items/sand": "Arena"}
    for clave, im in ims.items():
        c, r = pag.poner(im)
        sprites[clave] = {"grid": [CELDA, CELDA], "png": HOJA_REL, "rect": {"col": c, "row": r, "w": 1, "h": 1}}
        objetos[clave] = etiquetas[clave]
    publicar(pag, sprites, objetos, HOJA_REL)


if __name__ == "__main__":
    main()
