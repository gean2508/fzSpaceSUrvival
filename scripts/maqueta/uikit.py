"""Kit de composición para los mockups de UI de Ki.

Todo sale de arte REAL: los marcos y botones del pack Modern UI (Style 1, el
marrón, que es el que ya usa el menú), los ítems del manifest del juego y la
fuente que el juego embarca. Nada dibujado a ojo.
"""

import json
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

RAIZ = Path("/home/fzion/Documents/mods/phaser-arena")
ASSETS = RAIZ / "native/crates/ha-game/assets"
UI_SHEET = RAIZ / "PRUEBAS-CODIGOS/modernuserinterface-win/32x32/Modern_UI_Style_1_32x32.png"
FUENTE = ASSETS / "fonts/ui.ttf"

_manifest = json.loads((ASSETS / "art/manifest.json").read_text())
SPRITES = _manifest["sprites"]
_hoja_cache = {}
_ui = Image.open(UI_SHEET).convert("RGBA")

# — paleta medida sobre el pack Modern UI Style 1 —
MARCO_CLARO = (204, 150, 100)
MARCO = (176, 118, 82)
FONDO_PANEL = (203, 180, 148)
FONDO_HUECO = (140, 116, 96)
TINTA = (58, 58, 80)
TEXTO = (247, 240, 226)
TEXTO_OSCURO = (58, 58, 80)
ACENTO = (238, 196, 84)
VERDE = (120, 176, 96)
ROJO = (206, 74, 62)
AZUL = (86, 134, 206)


def fuente(px):
    return ImageFont.truetype(str(FUENTE), px)


# ── piezas medidas de la hoja ─────────────────────────────────────────────────
# (caja en la hoja, borde) — el borde se midió mirando la pieza ampliada.
PIEZAS = {
    "panel": ((116, 338, 172, 396), 11),
    "panel_liso": ((24, 502, 72, 552), 9),
    "slot": ((12, 688, 52, 728), 8),
    "slot_chico": ((64, 704, 96, 736), 7),
    "boton": ((26, 736, 70, 768), 8),
    "boton_ancho": ((122, 738, 166, 768), 8),
    "campo": ((142, 98, 218, 126), 6),
    "hueco": ((122, 416, 166, 448), 2),
}


def pieza(nombre):
    caja, borde = PIEZAS[nombre]
    return _ui.crop(caja), borde


def nine(nombre, w_u, h_u, escala=1):
    """9-slice en unidades de pixel-art, escalado NEAREST al final."""
    src, b = pieza(nombre)
    sw, sh = src.size
    w_u = max(w_u, 2 * b + 1)
    h_u = max(h_u, 2 * b + 1)
    out = Image.new("RGBA", (w_u, h_u), (0, 0, 0, 0))
    cw, ch = w_u - 2 * b, h_u - 2 * b
    mw, mh = sw - 2 * b, sh - 2 * b
    # esquinas
    out.paste(src.crop((0, 0, b, b)), (0, 0))
    out.paste(src.crop((sw - b, 0, sw, b)), (w_u - b, 0))
    out.paste(src.crop((0, sh - b, b, sh)), (0, h_u - b))
    out.paste(src.crop((sw - b, sh - b, sw, sh)), (w_u - b, h_u - b))
    # bordes
    top = src.crop((b, 0, sw - b, b)).resize((cw, b), Image.NEAREST)
    bot = src.crop((b, sh - b, sw - b, sh)).resize((cw, b), Image.NEAREST)
    izq = src.crop((0, b, b, sh - b)).resize((b, ch), Image.NEAREST)
    der = src.crop((sw - b, b, sw, sh - b)).resize((b, ch), Image.NEAREST)
    out.paste(top, (b, 0))
    out.paste(bot, (b, h_u - b))
    out.paste(izq, (0, b))
    out.paste(der, (w_u - b, b))
    # centro
    cen = src.crop((b, b, sw - b, sh - b)).resize((cw, ch), Image.NEAREST)
    out.paste(cen, (b, b))
    if escala != 1:
        out = out.resize((w_u * escala, h_u * escala), Image.NEAREST)
    return out


def marco(dst, nombre, x, y, w, h, escala):
    """Pega un 9-slice en px de pantalla (w,h múltiplos de `escala`)."""
    img = nine(nombre, w // escala, h // escala, escala)
    dst.alpha_composite(img, (x, y))
    return img


# ── sprites del juego ────────────────────────────────────────────────────────
def hoja(rel):
    if rel not in _hoja_cache:
        _hoja_cache[rel] = Image.open(ASSETS / rel).convert("RGBA")
    return _hoja_cache[rel]


def sprite(clave):
    """Recorte del manifest -> RGBA. La geometría sale del manifest, no del ojo."""
    if clave not in SPRITES:
        raise KeyError(f"clave sin arte en el manifest: {clave}")
    e = SPRITES[clave]
    im = hoja(e["png"])
    r = e.get("rect") or (e.get("poses", {}).get("center", {}) or {}).get("rect")
    if "grid" not in e or r is None:
        return im
    gw, gh = e["grid"]
    x0, y0 = r["col"] * gw, r["row"] * gh
    return im.crop((x0, y0, x0 + r["w"] * gw, y0 + r["h"] * gh))


def icono_en(dst, clave, cx, cy, lado):
    """Dibuja el sprite encajado (contain) en un cuadrado de `lado`, centrado."""
    if clave not in SPRITES:
        return False
    im = sprite(clave)
    caja = im.getbbox() or (0, 0, im.width, im.height)
    im = im.crop(caja)
    k = min(lado / im.width, lado / im.height)
    n = max(1, int(k)) if k >= 1 else k
    w, h = max(1, int(im.width * n)), max(1, int(im.height * n))
    im = im.resize((w, h), Image.NEAREST)
    dst.alpha_composite(im, (int(cx - w / 2), int(cy - h / 2)))
    return True


def recorte_ui(dst, caja, x, y, escala):
    im = _ui.crop(caja)
    im = im.resize((im.width * escala, im.height * escala), Image.NEAREST)
    dst.alpha_composite(im, (x, y))
    return im.size


def texto(d, xy, s, px, fill=TEXTO, anchor="la", sombra=True):
    f = fuente(px)
    if sombra:
        d.text((xy[0] + max(2, px // 12), xy[1] + max(2, px // 12)), s, font=f,
               fill=(40, 32, 40, 190), anchor=anchor)
    d.text(xy, s, font=f, fill=fill, anchor=anchor)
