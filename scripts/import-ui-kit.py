#!/usr/bin/env python3
"""**EL KIT DE INTERFAZ: los marcos de 9 partes y los botones-icono del pack.**

    python3 scripts/import-ui-kit.py
    python3 scripts/import-ui-kit.py --dry-run

## Por qué existe

La interfaz nueva (la propuesta del 15 ago 2026, `Interfaz de Ki`) está armada
enteramente con **Modern UI Style 1**, el marrón: el panel grande, el hueco de
una casilla, el botón, el campo de texto y la canaleta del scroll. El manifest
ya tenía `ui/panel` y `ui/slot_empty`, pero los dos apuntan al MISMO recorte
(col 3, fila 8) y su borde se adivinaba con la heurística de "una celda de la
grilla" — con lo cual el panel del menú salía con un borde de 32 px y las
casillas de 130 px quedaban todas marco y nada de centro.

Las piezas de esta hoja **no están alineadas a la grilla de 32**: el panel
empieza en el pixel 116 y mide 56 × 58. Por eso se publican en una página propia
con `grid: [1, 1]` —o sea, rects en píxeles— y con su **borde de 9 partes
declarado** (`slice`), que es lo que `art2d::ui_frame_node` necesita para
estirar el centro sin deformar las esquinas.

## De dónde salen los números

Los recortes y sus bordes se midieron sobre la hoja ampliada cuando se compuso
el mockup (`uikit.py::PIEZAS`), y son los que produjeron las imágenes que el
usuario aprobó. Para que eso no se vuelva un número a ojo el día que el pack
cambie, el script **verifica el SHA-256 de la hoja** antes de recortar: si el
pack se actualiza, se planta y hay que volver a medir (regla 7).

## Qué NO hace

No toca `ui/panel` ni `ui/slot_empty` viejos: los sigue usando el HUD y no se
rompe nada. Las claves nuevas van con el prefijo `ui/marco_*` y `ui/ic_*`.
"""

import argparse
import hashlib
import json
import os

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
HOJA = os.path.join(
    ROOT, "PRUEBAS-CODIGOS/modernuserinterface-win/32x32/Modern_UI_Style_1_32x32.png"
)
SHA_ESPERADO = "377cc56789fc83a3508f165b93416f8c0876b12dd8f3d966273ab8e5235e187d"
ASSETS = os.path.join(ROOT, "native/crates/ha-game/assets")
MANIFEST = os.path.join(ASSETS, "art/manifest.json")
PAGINA_REL = "art/pack/ui_kit.png"
INDEX = os.path.join(ROOT, "scripts/ui-kit-import-index.json")

# — los marcos de 9 partes: (caja en la hoja, borde px, clave, etiqueta) —
MARCOS = [
    ((116, 338, 172, 396), 11, "ui/marco_panel", "Panel de madera"),
    ((24, 502, 72, 552), 9, "ui/marco_liso", "Panel liso"),
    ((12, 688, 52, 728), 8, "ui/marco_slot", "Casilla"),
    ((64, 704, 96, 736), 7, "ui/marco_slot_chico", "Casilla chica"),
    ((26, 736, 70, 768), 8, "ui/marco_boton", "Botón"),
    ((122, 738, 166, 768), 8, "ui/marco_boton_ancho", "Botón ancho"),
    ((142, 98, 218, 126), 6, "ui/marco_campo", "Campo de texto"),
    ((122, 416, 166, 448), 2, "ui/marco_hueco", "Canaleta"),
]

# — botones-icono: el bloque de 32×32 que arranca en (768, 32) —
def _btn(col, fila):
    x, y = 768 + col * 32, 32 + fila * 32
    return (x, y, x + 32, y + 32)


ICONOS = [
    (_btn(2, 0), "ui/ic_grilla", "Icono · grilla"),
    (_btn(6, 0), "ui/ic_casa", "Icono · casa"),
    (_btn(0, 2), "ui/ic_reloj", "Icono · reloj"),
    (_btn(4, 0), "ui/ic_persona", "Icono · persona"),
    (_btn(7, 0), "ui/ic_lista", "Icono · lista"),
    (_btn(5, 0), "ui/ic_engranaje", "Icono · engranaje"),
    (_btn(8, 3), "ui/ic_controles", "Icono · controles"),
    (_btn(6, 1), "ui/ic_bajar", "Icono · descargar"),
    (_btn(1, 3), "ui/ic_interrogacion", "Icono · pregunta"),
    (_btn(5, 1), "ui/ic_flecha_izq", "Icono · flecha izquierda"),
    (_btn(4, 1), "ui/ic_flecha_der", "Icono · flecha derecha"),
    (_btn(7, 1), "ui/ic_flecha_arriba", "Icono · flecha arriba"),
    (_btn(0, 1), "ui/ic_tilde", "Icono · tilde"),
    (_btn(8, 0), "ui/ic_papelera", "Icono · papelera"),
    (_btn(4, 4), "ui/ic_lapiz", "Icono · lápiz"),
    (_btn(1, 1), "ui/ic_equis", "Icono · equis"),
    ((452, 294, 478, 314), "ui/ic_lupa", "Icono · lupa"),
    ((336, 204, 374, 238), "ui/ic_mochila", "Icono · mochila"),
]

GUTTER = 2


class Hoja:
    """Empaqueta recortes en una tira de filas, con calle de 2 px."""

    def __init__(self, ancho=512):
        self.ancho = ancho
        self.piezas = []          # (x, y, imagen)
        self.x = GUTTER
        self.y = GUTTER
        self.alto_fila = 0

    def poner(self, im):
        w, h = im.size
        if self.x + w + GUTTER > self.ancho:
            self.x = GUTTER
            self.y += self.alto_fila + GUTTER
            self.alto_fila = 0
        x, y = self.x, self.y
        self.piezas.append((x, y, im))
        self.x += w + GUTTER
        self.alto_fila = max(self.alto_fila, h)
        return x, y

    def imagen(self):
        alto = self.y + self.alto_fila + GUTTER
        out = Image.new("RGBA", (self.ancho, alto), (0, 0, 0, 0))
        for x, y, im in self.piezas:
            out.alpha_composite(im, (x, y))
        return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    if not os.path.exists(HOJA):
        raise SystemExit(f"falta la hoja del pack: {HOJA}")
    sha = hashlib.sha256(open(HOJA, "rb").read()).hexdigest()
    if sha != SHA_ESPERADO:
        raise SystemExit(
            "la hoja del pack CAMBIÓ: los recortes de MARCOS/ICONOS se midieron "
            f"sobre {SHA_ESPERADO[:12]} y ahora es {sha[:12]}. Hay que volver a "
            "medirlos antes de publicar (regla 7: la geometría se mide)."
        )
    ui = Image.open(HOJA).convert("RGBA")

    pagina = Hoja()
    sprites = {}
    objetos = {}

    for caja, borde, clave, etiqueta in MARCOS:
        im = ui.crop(caja)
        w, h = im.size
        if borde * 2 >= min(w, h):
            raise SystemExit(f"{clave}: borde {borde} no entra en {w}x{h}")
        x, y = pagina.poner(im)
        sprites[clave] = {
            "png": PAGINA_REL,
            "grid": [1, 1],
            "rect": {"col": x, "row": y, "w": w, "h": h},
            "slice": {"left": borde, "right": borde, "top": borde, "bottom": borde},
        }
        objetos[clave] = etiqueta
        print(f"{clave}: {w}x{h} px · borde {borde}")

    for caja, clave, etiqueta in ICONOS:
        im = ui.crop(caja)
        w, h = im.size
        x, y = pagina.poner(im)
        sprites[clave] = {
            "png": PAGINA_REL,
            "grid": [1, 1],
            "rect": {"col": x, "row": y, "w": w, "h": h},
        }
        objetos[clave] = etiqueta

    im = pagina.imagen()
    if args.dry_run:
        print(f"\n[dry-run] hoja {im.size[0]}x{im.size[1]} px, {len(sprites)} claves")
        return

    os.makedirs(os.path.join(ASSETS, "art/pack"), exist_ok=True)
    im.save(os.path.join(ASSETS, PAGINA_REL), optimize=True)
    print(f"\n{PAGINA_REL}: {im.size[0]}x{im.size[1]} px")

    man = json.load(open(MANIFEST))
    hoja = man.setdefault("sprites", {})
    if os.path.exists(INDEX):
        for k in json.load(open(INDEX)).get("sprites", []):
            hoja.pop(k, None)
    hoja.update(sprites)
    objs = man.setdefault("objects", {})
    for clave, etiqueta in objetos.items():
        objs.setdefault(clave, {})["label"] = etiqueta
    with open(MANIFEST, "w") as f:
        json.dump(man, f, indent=1, ensure_ascii=False, sort_keys=True)
    with open(INDEX, "w") as f:
        json.dump({"sprites": sorted(sprites), "pagina": PAGINA_REL}, f, indent=1,
                  ensure_ascii=False)
    print(f"manifest: {len(sprites)} claves del kit de interfaz")


if __name__ == "__main__":
    main()
