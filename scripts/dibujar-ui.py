#!/usr/bin/env python3
"""**LOS HUECOS DE LA INTERFAZ.**

    python3 scripts/dibujar-ui.py [--dry-run]

Dos orígenes distintos, y la diferencia importa:

## 1. Las pipas vacías se DERIVAN de la llena, no se buscan

`heart_empty`, `energy_empty` y `hunger_empty` salen de `*_full` apagándolo
contra la paleta del pack. El atlas de Modern UI tiene corazones oscuros, sí —
pero medidos, son de **otro tamaño** (176 px opacos contra 292 del lleno), y una
pipa de HUD tiene que compartir la silueta EXACTA con su llena: si no, la barra
de vida cambia de forma a medida que baja. La derivación garantiza eso y no
inventa un pixel: cada color se ancla por luminancia a un color del pack, igual
que `atlas-suelo.py::derivar`.

## 2. El resto se dibuja

Pestañas, iconos de acción y botones de menú no existen en ningún pack —son de
este juego— así que se dibujan con `lib/pixel.py`, que hace cumplir el estilo
medido. `ui/lantern_button` sí existía (`items/lantern`) y se sacó de acá: lo
asigna `asignar-del-pack.py`.

**Un icono de UI se lee a 32 px o no sirve.** La regla que gobierna estos
dibujos no es el detalle sino el CONTRASTE de silueta: se ven sobre un panel
marrón, chiquitos y en movimiento. Por eso van con forma maciza y un solo
acento de color, no con dibujo fino.
"""

import colorsys
import json
import sys
from pathlib import Path

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))

from lib.pixel import CELDA, Lienzo, Pagina, ajustar, luz, publicar, rampa  # noqa: E402

RAIZ = Path(__file__).resolve().parent.parent
ASSETS = RAIZ / "native/crates/ha-game/assets"
PAGINA_REL = "art/gen/ui.png"

# — la paleta de la interfaz, medida del panel del pack —
MARCO = (156, 120, 84)
FONDO = (86, 66, 52)
CLARO = (226, 214, 196)
ACENTO = (238, 196, 84)
ROJO = (206, 74, 62)
VERDE = (96, 168, 96)
AZUL = (86, 134, 206)
GRIS = (150, 150, 160)
TIERRA = (138, 108, 72)
LUNA = (176, 178, 186)
ESPACIO = (58, 62, 96)


def recorte(clave):
    """El pixel de una clave que YA está en el manifest."""
    m = json.loads((ASSETS / "art/manifest.json").read_text())
    e = m["sprites"][clave]
    gw, gh = e.get("grid", [CELDA, CELDA])
    r = e["rect"] if "rect" in e else e["poses"]["center"]["rect"]
    im = Image.open(ASSETS / e["png"]).convert("RGBA")
    return im.crop((r["col"] * gw, r["row"] * gh, (r["col"] + r["w"]) * gw, (r["row"] + r["h"]) * gh))


def apagar(im, factor=0.45, sat=0.18):
    """**El lleno, apagado.** Conserva la silueta y el relieve interno —cada
    píxel mantiene su brillo RELATIVO— y le baja la saturación y la luz. Es el
    mismo criterio que `atlas-suelo.py::derivar` a peso 1,0: anclar por
    luminancia en vez de multiplicar, que es lo que evita que las sombras se
    vayan de tono."""
    out = Image.new("RGBA", im.size, (0, 0, 0, 0))
    o = out.load()
    p = im.load()
    for y in range(im.height):
        for x in range(im.width):
            c = p[x, y]
            if c[3] == 0:
                continue
            h, s, v = colorsys.rgb_to_hsv(*[q / 255 for q in c[:3]])
            r, g, b = colorsys.hsv_to_rgb(h, s * sat / max(s, 1e-3) * s, v * factor)
            o[x, y] = (*ajustar((r * 255, g * 255, b * 255)), c[3])
    return out


# — recetas dibujadas ————————————————————————————————————————————


def pestana(icono, color):
    """Una pestaña: chapa con el marco del pack y su glifo encima."""
    def f(l):
        m = rampa(MARCO, 4)
        f_ = rampa(FONDO, 4)
        l.rect(2, 4, 28, 24, f_[len(f_) // 2])
        l.rect(2, 4, 28, 3, m[-1])
        l.rect(2, 25, 28, 3, m[1])
        l.rect(2, 4, 3, 24, m[len(m) // 2])
        l.rect(27, 4, 3, 24, m[1])
        icono(l, rampa(color, 4))
        l.contorno(m[0])
    return f


def _glifo_mochila(l, t):
    l.rect(11, 12, 10, 9, t[len(t) // 2])
    l.rect(11, 12, 10, 2, t[-1])
    l.rect(14, 9, 4, 3, t[1])


def _glifo_yunque(l, t):
    l.poligono([(9, 13), (23, 13), (20, 18), (12, 18)], t[len(t) // 2])
    l.rect(13, 18, 6, 4, t[1])
    l.rect(9, 13, 14, 2, t[-1])


def _glifo_martillo(l, t):
    l.rect(10, 11, 12, 5, t[len(t) // 2])
    l.rect(10, 11, 12, 2, t[-1])
    l.rect(14, 16, 4, 7, t[1])


def _glifo_espada(l, t):
    """Espada de punta arriba: hoja, guarda y empuñadura. En vertical y no en
    diagonal porque a 32 px una diagonal de 4 px de ancho se lee como un garabato
    (lo era en la primera pasada)."""
    g = rampa(GRIS, 4)
    l.poligono([(16, 4), (19, 9), (19, 19), (13, 19), (13, 9)], g[len(g) // 2])
    l.poligono([(16, 4), (19, 9), (16, 9)], g[-1])
    l.rect(13, 9, 3, 10, g[-1])
    l.rect(9, 19, 14, 3, t[len(t) // 2])   # guarda
    l.rect(15, 22, 3, 6, t[1])             # empuñadura
    l.rect(14, 27, 5, 2, t[len(t) // 2])


def _glifo_mano(l, t):
    l.rect(12, 13, 8, 9, t[len(t) // 2])
    l.rect(12, 13, 8, 2, t[-1])
    for x in (12, 15, 18):
        l.rect(x, 9, 2, 4, t[len(t) // 2])


def _glifo_pala(l, t):
    l.banda(16, 22, 16, 12, 4, t[len(t) // 2])
    l.poligono([(11, 12), (21, 12), (19, 6), (13, 6)], t[len(t) // 2])
    l.poligono([(13, 6), (19, 6), (18, 9), (14, 9)], t[-1])


def icono_suelto(glifo, color):
    def f(l):
        glifo(l, rampa(color, 4))
        l.contorno(rampa(color, 4)[0])
    return f


def _crosshair(l):
    """Cuatro brazos y un punto. Con `rect` y no con `banda`: una banda de 3 px
    entre dos puntos alineados redondea a bloques corridos, y una mira que no es
    simétrica al píxel se ve rota."""
    t = rampa(CLARO, 4)
    medio, claro = t[len(t) // 2], t[-1]
    l.rect(14, 3, 4, 8, medio)
    l.rect(14, 21, 4, 8, medio)
    l.rect(3, 14, 8, 4, medio)
    l.rect(21, 14, 8, 4, medio)
    l.rect(14, 3, 2, 8, claro)
    l.rect(3, 14, 8, 2, claro)
    l.rect(14, 14, 4, 4, claro)
    l.contorno(t[0])


def _chip(l):
    m = rampa(MARCO, 4)
    a = rampa(ACENTO, 4)
    l.rect(4, 10, 24, 12, m[len(m) // 2])
    l.rect(4, 10, 24, 3, m[-1])
    l.rect(9, 14, 14, 4, a[len(a) // 2])
    l.contorno(m[0])


def boton_mundo(color, marca):
    """Los botones del menú de destino: un disco del color del cuerpo con su
    marca. Se distinguen por COLOR y SILUETA, no por un dibujo fino: en el menú
    se ven a 32 px."""
    def f(l):
        t = rampa(color, 4)
        l.elipse(16, 16, 12, 12, t[len(t) // 2])
        l.elipse(13, 12, 7, 6, t[-1])
        marca(l, t)
        l.contorno(t[0])
    return f


def _marca_tierra(l, t):
    v = rampa(VERDE, 3)
    l.poligono([(9, 14), (16, 12), (20, 16), (14, 19)], v[len(v) // 2])
    l.poligono([(18, 21), (24, 20), (22, 24)], v[len(v) // 2])


def _marca_luna(l, t):
    o = rampa((122, 124, 134), 3)
    for cx, cy, r in [(12, 12, 3), (20, 18, 4), (13, 21, 2)]:
        l.elipse(cx, cy, r, r, o[len(o) // 2])


def _marca_espacio(l, t):
    c = rampa(CLARO, 3)
    for cx, cy in [(10, 10), (22, 12), (14, 20), (23, 22), (18, 8)]:
        l.rect(cx, cy, 2, 2, c[-1])


def _marca_creativo(l, t):
    a = rampa(ACENTO, 3)
    l.banda(10, 22, 21, 10, 4, a[len(a) // 2])
    l.poligono([(20, 8), (24, 12), (20, 13)], a[-1])


def _marca_ajustes(l, t):
    g = rampa(GRIS, 4)
    l.elipse(16, 16, 9, 9, g[len(g) // 2])
    for dx, dy in [(0, -11), (0, 11), (-11, 0), (11, 0), (-8, -8), (8, 8), (-8, 8), (8, -8)]:
        l.rect(16 + dx - 2, 16 + dy - 2, 4, 4, g[len(g) // 2])
    l.elipse(16, 16, 4, 4, rampa(FONDO, 3)[0])


RECETAS = {
    "ui/tab_inventario": ("Pestaña de mochila", pestana(_glifo_mochila, CLARO)),
    "ui/tab_craft": ("Pestaña de fabricar", pestana(_glifo_yunque, CLARO)),
    "ui/tab_construir": ("Pestaña de construir", pestana(_glifo_martillo, CLARO)),
    "ui/icon_atacar": ("Icono de atacar", icono_suelto(_glifo_espada, ROJO)),
    "ui/icon_usar": ("Icono de usar", icono_suelto(_glifo_mano, CLARO)),
    "ui/icon_cavar": ("Icono de cavar", icono_suelto(_glifo_pala, TIERRA)),
    "ui/icon_craft": ("Icono de fabricar", icono_suelto(_glifo_yunque, GRIS)),
    "ui/crosshair": ("Mira", _crosshair),
    "ui/build_chip": ("Ficha de construcción", _chip),
    "ui/menu_tierra": ("Menú: Tierra", boton_mundo(AZUL, _marca_tierra)),
    "ui/menu_luna": ("Menú: Luna", boton_mundo(LUNA, _marca_luna)),
    "ui/menu_espacio": ("Menú: Espacio", boton_mundo(ESPACIO, _marca_espacio)),
    "ui/menu_creativo": ("Menú: Creativo", boton_mundo(VERDE, _marca_creativo)),
    "ui/menu_ajustes": ("Menú: Ajustes", boton_mundo(MARCO, _marca_ajustes)),
}

#: Las que salen de apagar su versión llena (ver el encabezado).
DERIVADAS = {
    "ui/heart_empty": ("Corazón vacío", "ui/heart_full"),
    "ui/energy_empty": ("Energía vacía", "ui/energy_full"),
    "ui/hunger_empty": ("Hambre vacía", "ui/hunger_full"),
}


def main():
    seco = "--dry-run" in sys.argv
    pagina = Pagina()
    sprites, objetos = {}, {}
    for clave, (label, receta) in RECETAS.items():
        l = Lienzo(CELDA, CELDA)
        receta(l)
        l.verificar(apoyado=False)
        col, row = pagina.poner(l.im)
        sprites[clave] = {
            "png": PAGINA_REL,
            "grid": [CELDA, CELDA],
            "rect": {"col": col, "row": row, "w": 1, "h": 1},
        }
        objetos[clave] = label
    for clave, (label, origen) in DERIVADAS.items():
        im = apagar(recorte(origen))
        col, row = pagina.poner(im)
        sprites[clave] = {
            "png": PAGINA_REL,
            "grid": [CELDA, CELDA],
            "rect": {"col": col, "row": row, "w": im.width // CELDA, "h": im.height // CELDA},
        }
        objetos[clave] = label
    publicar(pagina, sprites, objetos, PAGINA_REL, seco)
    print(f"{len(RECETAS)} dibujadas + {len(DERIVADAS)} derivadas del lleno")


if __name__ == "__main__":
    main()
