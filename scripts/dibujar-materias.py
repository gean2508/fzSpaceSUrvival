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
    from lib.iconos import bulto, cubos, esquirla, gota, ladrillo, lingote, polvo, ramitas

    # Dibujados con `lib/volumen.py` (cúpula iluminada + relieve de la roca REAL del pack) y el contorno de
    # Modern Interiors: ya no son siluetas reteñidas de un ícono, sino cada uno con su forma.
    lista = [
        # ── crudos ──────────────────────────────────────────────────────────
        ("clay", "Arcilla", bulto((176, 108, 74), 1, 9.5, 7.5, False, None, 0, 1.7, 0.0).im),
        ("coal", "Carbón", bulto((54, 52, 66), 2, 8.5, 7, True, (150, 150, 170), 3).im),
        ("copper_ore", "Mena de cobre", bulto((120, 120, 134), 3, 9, 7.5, True, (205, 112, 60), 6).im),
        ("tin_ore", "Mena de estaño", bulto((120, 120, 134), 4, 9, 7.5, True, (222, 228, 238), 6).im),
        ("iron_ore", "Mena de hierro", bulto((110, 96, 96), 5, 9, 7.5, True, (160, 84, 58), 6).im),
        ("limestone", "Caliza", bulto((224, 214, 188), 11, 10, 6.8, True, None, 0, 1.3, 0.45).im),
        ("resin", "Resina", gota((216, 150, 54)).im),
        ("salt", "Sal", cubos((236, 236, 244)).im),
        # ── derivados, uno por clase ─────────────────────────────────────────
        ("mat_metal", "Metal", lingote((156, 168, 184)).im),
        ("mat_aleacion", "Aleación", lingote((192, 124, 62)).im),
        ("mat_ceramica", "Cerámica", ladrillo((176, 86, 66)).im),
        ("mat_vidrio", "Vidrio", esquirla((186, 226, 236)).im),
        ("mat_aglutinante", "Aglutinante", polvo((228, 220, 200)).im),
        ("mat_compuesto", "Compuesto", polvo((190, 154, 104), 4).im),
        ("mat_combustible", "Carbón vegetal", ramitas((40, 38, 50)).im),
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
