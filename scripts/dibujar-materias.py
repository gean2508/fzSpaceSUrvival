#!/usr/bin/env python3
"""**ÍCONOS DE LAS MATERIAS (crudos y derivados), DERIVADOS DEL ARTE EN USO.**

    python3 scripts/dibujar-materias.py [--dry-run]

El jugador experimenta con materiales que el mundo reparte (`ha_shared::civ::materia`): arcilla,
carbón, menas, caliza, resina, sal… y lo que sale de combinarlos (metal, aleación, cerámica, vidrio,
aglutinante, compuesto, carbón vegetal). Ninguno tenía ícono. Siguiendo el orden de la skill
`pixel-art` (buscar → DERIVAR de lo que ya está asignado → recién ahí dibujar), cada ícono se obtiene
**reteñiendo un ícono ya aprobado** (la piedra, la tierra, la arena, el lingote, la poción, la gema)
conservando su forma, su contorno y sus escalones de sombra, y poniéndole encima las pintitas que lo
distinguen (la mena de cobre es la piedra con vetas anaranjadas). Todo color pasa por la paleta del
pack (`lib.pixel.ajustar`), así que no hay ningún tono que el pack no use.

Los derivados usan un ícono POR CLASE (`mat_metal`, `mat_aleacion`…): el nombre propio de cada
material sale de su historia («aleación de metal de mena de cobre y …»), no del dibujo.
"""

import json
import sys
from pathlib import Path

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib.pixel import CELDA, RAIZ, Pagina, ajustar, luz, publicar, rampa  # noqa: E402

ITEMS = RAIZ / "native/crates/ha-game/assets/art/gen/items.png"
GRANJA = RAIZ / "native/crates/ha-game/assets/art/gen/granja.png"
HOJA_REL = "art/gen/materias.png"


def celda(png: Path, col: int) -> Image.Image:
    return Image.open(png).convert("RGBA").crop((col * CELDA, 0, (col + 1) * CELDA, CELDA))


FUENTES = {
    "stone": (ITEMS, 8),
    "dirt": (ITEMS, 9),
    "sand": (ITEMS, 10),
    "iron": (ITEMS, 17),
    "gold": (ITEMS, 18),
    "gem": (ITEMS, 20),
    "potion": (ITEMS, 26),
}


def retenir(src: Image.Image, base, pasos: int = 4) -> Image.Image:
    """Mismo dibujo, otro material: cada escalón de luminancia del original pasa al escalón
    equivalente de una rampa del pack alrededor de `base`. El contorno (lo más oscuro) se conserva."""
    px = src.load()
    lums = sorted({round(luz(px[x, y][:3])) for y in range(src.height) for x in range(src.width) if px[x, y][3] > 128})
    if not lums:
        return src
    r = rampa(base, pasos)
    out = src.copy()
    po = out.load()
    lo, hi = lums[0], lums[-1]
    for y in range(src.height):
        for x in range(src.width):
            c = px[x, y]
            if c[3] <= 128:
                continue
            l = luz(c[:3])
            if l <= lo + 6 and l < 90:  # contorno / lo más oscuro: se queda (tintado a la rampa)
                po[x, y] = (*r[0], 255)
                continue
            t = (l - lo) / max(hi - lo, 1)
            po[x, y] = (*r[min(pasos - 1, 1 + int(t * (pasos - 1)))], 255)
    return out


def motas(im: Image.Image, color, posiciones) -> Image.Image:
    """Pintitas de 1-2 píxeles (vetas, cristales) sólo sobre el cuerpo, nunca sobre el contorno."""
    out = im.copy()
    po = out.load()
    c = ajustar(color)
    for (x, y) in posiciones:
        if 0 <= x < CELDA and 0 <= y < CELDA and po[x, y][3] > 128 and luz(po[x, y][:3]) > 70:
            po[x, y] = (*c, 255)
    return out


def ladrillo(src: Image.Image) -> Image.Image:
    """La piedra reteñida en terracota con las juntas de un ladrillo (dos hiladas)."""
    out = retenir(src, (190, 98, 72))
    po = out.load()
    junta = ajustar((120, 60, 48))
    xs = [x for x in range(CELDA) for y in range(CELDA) if po[x, y][3] > 128]
    ys = [y for y in range(CELDA) for x in range(CELDA) if po[x, y][3] > 128]
    if not xs:
        return out
    x0, x1, y0, y1 = min(xs), max(xs), min(ys), max(ys)
    ym = (y0 + y1) // 2
    for x in range(x0 + 1, x1):
        if po[x, ym][3] > 128:
            po[x, ym] = (*junta, 255)
    for (ya, yb, xj) in ((y0 + 1, ym - 1, (x0 + x1) // 2), (ym + 1, y1 - 1, (x0 + x1) // 2 - 4)):
        for y in range(ya, yb + 1):
            if po[xj, y][3] > 128:
                po[xj, y] = (*junta, 255)
    return out


def main():
    seco = "--dry-run" in sys.argv
    f = {k: celda(p, c) for k, (p, c) in FUENTES.items()}
    veta = [(11, 14), (12, 14), (17, 17), (18, 17), (14, 20), (20, 13), (9, 19)]
    lista = [
        # ── crudos ──────────────────────────────────────────────────────────
        ("clay", "Arcilla", retenir(f["dirt"], (176, 108, 74))),
        ("coal", "Carbón", motas(retenir(f["stone"], (54, 52, 66)), (150, 150, 170), [(13, 15), (18, 18)])),
        ("copper_ore", "Mena de cobre", motas(f["stone"], (205, 112, 60), veta)),
        ("tin_ore", "Mena de estaño", motas(f["stone"], (222, 228, 238), veta)),
        ("iron_ore", "Mena de hierro", motas(retenir(f["stone"], (118, 96, 92)), (160, 84, 58), veta)),
        ("limestone", "Caliza", retenir(f["stone"], (224, 214, 188))),
        ("resin", "Resina", retenir(f["gem"], (216, 150, 54))),
        ("salt", "Sal", retenir(f["sand"], (236, 236, 244))),
        # ── derivados, uno por clase ─────────────────────────────────────────
        ("mat_metal", "Metal", retenir(f["iron"], (156, 168, 184))),
        ("mat_aleacion", "Aleación", retenir(f["gold"], (192, 124, 62))),
        ("mat_ceramica", "Cerámica", ladrillo(f["stone"])),
        ("mat_vidrio", "Vidrio", retenir(f["potion"], (186, 226, 236))),
        ("mat_aglutinante", "Aglutinante", retenir(f["sand"], (228, 220, 200))),
        ("mat_compuesto", "Compuesto", retenir(f["dirt"], (190, 154, 104))),
        ("mat_combustible", "Carbón vegetal", motas(retenir(f["stone"], (40, 38, 50)), (120, 120, 140), [(13, 15)])),
    ]
    pag = Pagina()
    sprites, objetos = {}, {}
    for clave, etiqueta, im in lista:
        c, r = pag.poner(im)
        k = f"items/{clave}"
        sprites[k] = {"grid": [CELDA, CELDA], "png": HOJA_REL, "rect": {"col": c, "row": r, "w": 1, "h": 1}}
        objetos[k] = etiqueta
    publicar(pag, sprites, objetos, HOJA_REL, seco)
    if not seco:
        filas = ",\n".join(f'    ("{k.split("/")[1]}", "{v}")' for k, v in objetos.items())
        (RAIZ / "native/crates/ha-assets/src/materias_items.rs").write_text(
            "// GENERADO por scripts/dibujar-materias.py — no editar a mano.\n"
            f"pub const MATERIAS_IDS: &[(&str, &str)] = &[\n{filas},\n];\n"
        )
        print("materias_items.rs escrito")


if __name__ == "__main__":
    main()
