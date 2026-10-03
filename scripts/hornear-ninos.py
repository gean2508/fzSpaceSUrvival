#!/usr/bin/env python3
"""**LAS SKINS DE NIÑOS: seis hojas compactas `art/npc/char_21..26.png` con las capas de niños del Character Generator.**

    python3 scripts/hornear-ninos.py [--hoja]       # --hoja: sólo deja la lámina de control

Hasta ahora los hijos de la sociedad eran adultos achicados con `escala_por_edad`. El pack trae el cuerpo, los ojos, la
ropa y los peinados de niños (`Bodies_kids`, `Eyes_kids`, `Outfits_kids`, `Hairstyles_kids`): hojas de 24 columnas × 4
filas de 32×64 — que NO tienen el mismo layout que las de adultos (56×20). Acá se apilan en el orden del generador
(cuerpo + ojos + ropa + peinado) y se pasan a la hoja COMPACTA del aldeano (`chargen_index::NPC_ANIMS`, 48 columnas):

    still     ← fila 0 del niño (4 poses fijas, R U L D)
    idle      ← fila 1 (6 cuadros × 4 direcciones)
    walk      ← fila 2 (6 × 4)
    sleep     ← fila 3 (6, una sola pose)
    sit_floor / pick_up / punch ← no existen en el niño: se arman con los cuadros de `idle` (quieto en el sitio).

Eyes y Outfits miden 768×192 (sin la fila de poses fijas): van corridos una fila (64 px) para alinear con el cuerpo.
"""

import sys
from pathlib import Path

from PIL import Image

RAIZ = Path(__file__).resolve().parent.parent
GEN = RAIZ / "PRUEBAS-CODIGOS/Character_Generator"
SALIDA = RAIZ / "native/crates/ha-game/assets/art/npc"
CELDA = (32, 64)
COLS = 48
FILAS = ["still", "idle", "walk", "sleep", "sit_floor", "pick_up", "punch"]

#: (cuerpo, ojos, ropa, (estilo, color) de peinado o None)
COMBOS = [
    (1, 1, 1, (1, 1)),
    (2, 2, 2, (2, 3)),
    (3, 3, 3, (3, 2)),
    (4, 4, 4, (4, 5)),
    (1, 5, 5, (5, 4)),
    (2, 6, 6, None),          # pijama de rana: no lleva peinado
]


def capa(carpeta, nombre):
    return Image.open(GEN / carpeta / "32x32" / nombre).convert("RGBA")


def armar(c):
    cuerpo, ojos, ropa, pelo = c
    hoja = capa("Bodies_kids", f"Body_{cuerpo}_kid_32x32.png")
    ojos_im = capa("Eyes_kids", f"Eyes_kids_32x32_{ojos}.png")
    ropa_im = [p for p in sorted((GEN / "Outfits_kids/32x32").glob(f"Outfit_kid_{ropa}_*.png"))]
    out = hoja.copy()
    out.alpha_composite(ojos_im, (0, 64))
    out.alpha_composite(Image.open(ropa_im[0]).convert("RGBA"), (0, 64))
    if pelo:
        out.alpha_composite(capa("Hairstyles_kids", f"Hairstyle_kid_{pelo[0]}_32x32_{pelo[1]}.png"))
    return out


def celdas(im, fila, desde, n):
    """`n` celdas consecutivas de la fila `fila` empezando en la columna `desde`."""
    return [im.crop(((desde + i) * CELDA[0], fila * CELDA[1], (desde + i + 1) * CELDA[0], (fila + 1) * CELDA[1])) for i in range(n)]


def compactar(kid):
    out = Image.new("RGBA", (COLS * CELDA[0], len(FILAS) * CELDA[1]), (0, 0, 0, 0))

    def tira(fila_nombre, cuadros):
        y = FILAS.index(fila_nombre) * CELDA[1]
        for i, c in enumerate(cuadros):
            out.alpha_composite(c, (i * CELDA[0], y))

    tira("still", celdas(kid, 0, 0, 4))
    tira("idle", celdas(kid, 1, 0, 24))
    tira("walk", celdas(kid, 2, 0, 24))
    tira("sleep", celdas(kid, 3, 0, 6))
    idle = [celdas(kid, 1, d * 6, 6) for d in range(4)]              # R U L D, 6 cuadros cada una
    tira("sit_floor", idle[0] + idle[2])                              # perfil derecho + izquierdo
    tira("pick_up", sum((idle[d] + idle[d] for d in range(4)), []))   # 12 por dirección
    tira("punch", sum(idle, []))                                      # 6 por dirección
    return out


def main():
    solo_hoja = "--hoja" in sys.argv
    kids = [armar(c) for c in COMBOS]
    if not solo_hoja:
        for i, k in enumerate(kids):
            compactar(k).save(SALIDA / f"char_{21 + i}.png", optimize=True)
    lam = Image.new("RGBA", (24 * 32 * 1, len(kids) * 64 * 2), (70, 90, 70, 255))
    for i, k in enumerate(kids):
        lam.alpha_composite(k.crop((0, 64, 24 * 32, 128)), (0, i * 128))
        lam.alpha_composite(k.crop((0, 128, 24 * 32, 192)), (0, i * 128 + 64))
    lam.convert("RGB").save("/tmp/claude-1000/ninos-hoja.png")
    print(f"{len(kids)} skins de niño → char_21..{20 + len(kids)}.png {'(sólo lámina)' if solo_hoja else ''}")


if __name__ == "__main__":
    main()
