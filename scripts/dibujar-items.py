#!/usr/bin/env python3
"""**LOS ICONOS DEL INVENTARIO** — los 35 que no existen en ningún pack.

    python3 scripts/dibujar-items.py [--dry-run]

## Por qué se dibujan y no se recortan

Los packs de LimeZu son urbanos y de interiores: tienen comida servida en un
plato y herramientas de jardín *dentro de una escena*, pero no un **icono** de
hacha, de lingote o de gema. Son los slots que agotan las tres puertas del orden
de la skill `pixel-art` (buscar → derivar → adaptar) y llegan a la cuarta.

**Verificado clave por clave**, no supuesto: `items/apple` se dibujó de más y hoy
sale de `prop/manzanas_1`, que ya estaba importado. Lo que queda acá es lo que se
buscó en los tres packs Y en las 13.413 claves del manifest —en inglés y en
castellano, que es como las bautizó el importador— y no apareció.

Y son los que más se ven: la barra de acceso rápido y la mochila están en
pantalla todo el tiempo, así que un slot vacío ahí es un cuadrado gris que el
jugador mira cada segundo.

## Cómo se dibujan

Con `lib/pixel.py`, que hace cumplir el estilo medido del pack: paleta de 501
colores, 9-16 colores por pieza, sin dithering, sin contorno (el pack no
contornea: medido, el borde es el mismo color que su vecino interior en el 94 %
de los pares), luz suave de arriba-izquierda.

**Un icono no se apoya ni tira sombra.** Va centrado en su celda: no es un objeto
del mundo, es una ficha dentro de un casillero — por eso `verificar(apoyado=False)`
y ninguna llamada a `sombra_al_pie`.
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from lib.pixel import CELDA, Lienzo, Pagina, publicar, rampa  # noqa: E402

PAGINA_REL = "art/gen/items.png"

# — colores base de cada material. Se piden a la paleta del pack (`rampa`), así
#   que estos RGB son la INTENCIÓN; el pixel que sale es el del pack más cercano.
MADERA = (146, 104, 62)
PIEDRA = (140, 140, 148)
HIERRO = (176, 182, 190)
ORO = (226, 178, 58)
CUERO = (150, 104, 66)
HOJA = (92, 148, 84)
ROJO = (196, 62, 54)
AZUL = (72, 116, 196)
VERDE = (86, 168, 92)
NARANJA = (226, 146, 52)
CARNE = (188, 96, 92)
PAN = (198, 150, 86)
TIERRA = (150, 116, 78)
ARENA = (222, 190, 128)
NIEVE = (232, 240, 248)
UVA = (128, 92, 176)
MASA = (214, 174, 110)
CALDO = (198, 132, 62)
METAL = (150, 158, 172)


def mango(l, x0, y0, x1, y1, tonos, grosor=4):
    """El palo de una herramienta: una banda maciza del tono medio con su filo
    claro corrido un píxel a la izquierda. La luz del pack es de arriba-izquierda
    (+4 medido) y se aplica de golpe, no en degradado.

    **Bandas, no líneas de un píxel.** Tres líneas paralelas de tonos distintos
    dan un damero y el guardián de `verificar` las rechaza — con razón: el pack
    no dithera nunca."""
    l.banda(x0, y0, x1, y1, grosor, tonos[len(tonos) // 2])
    l.banda(x0 - 1, y0, x1 - 1, y1, max(1, grosor - 2), tonos[-1])


def cabeza_hacha(l, cx, cy, tonos):
    """La cabeza de un hacha: cuña ancha a la izquierda del mango, con el filo
    claro y el ojo (donde entra el palo) del tono del medio."""
    medio, claro, oscuro = tonos[len(tonos) // 2], tonos[-1], tonos[1]
    # el cuerpo, apoyado sobre el mango y abriéndose hacia la izquierda
    l.poligono([(cx + 2, cy - 6), (cx + 2, cy + 6), (cx - 5, cy + 8), (cx - 9, cy), (cx - 5, cy - 8)], medio)
    # el filo: la franja de la izquierda, más clara
    l.poligono([(cx - 5, cy - 8), (cx - 9, cy), (cx - 5, cy + 8), (cx - 6, cy + 5), (cx - 8, cy), (cx - 6, cy - 5)], claro)
    # el ojo, pegado al mango
    l.rect(cx, cy - 4, 3, 9, oscuro)


def cabeza_pico(l, cx, cy, tonos):
    """Dos puntas que salen del ojo, cada una una banda maciza que baja hacia
    afuera — la silueta del pico, sin un solo píxel suelto."""
    medio, claro, oscuro = tonos[len(tonos) // 2], tonos[-1], tonos[1]
    l.banda(cx - 11, cy + 1, cx, cy - 4, 4, medio)
    l.banda(cx, cy - 4, cx + 11, cy + 1, 4, medio)
    l.banda(cx - 11, cy, cx, cy - 5, 2, claro)
    l.rect(cx - 3, cy - 5, 7, 7, medio)
    l.rect(cx - 3, cy - 5, 7, 2, claro)
    l.rect(cx + 2, cy - 5, 2, 7, oscuro)


def fruta(l, cx, cy, r, tonos, tallo=True):
    l.elipse(cx, cy, r, r, tonos[len(tonos) // 2])
    l.elipse(cx - r * 0.3, cy - r * 0.3, r * 0.5, r * 0.45, tonos[-1])
    if tallo:
        l.rect(int(cx), int(cy - r - 2), 1, 3, rampa(MADERA, 3)[0])


def lingote(l, cx, cy, tonos):
    l.poligono([(cx - 9, cy + 5), (cx - 6, cy - 3), (cx + 6, cy - 3), (cx + 9, cy + 5)], tonos[len(tonos) // 2])
    l.poligono([(cx - 6, cy - 3), (cx - 4, cy - 7), (cx + 8, cy - 7), (cx + 6, cy - 3)], tonos[-1])


def gema(l, cx, cy, tonos):
    l.poligono([(cx, cy - 9), (cx + 7, cy - 2), (cx, cy + 9), (cx - 7, cy - 2)], tonos[len(tonos) // 2])
    l.poligono([(cx, cy - 9), (cx + 7, cy - 2), (cx, cy - 1), (cx - 7, cy - 2)], tonos[-1])
    l.poligono([(cx, cy + 9), (cx + 7, cy - 2), (cx, cy - 1)], tonos[max(0, len(tonos) // 2 - 1)])


def frasco(l, cx, cy, liquido):
    vidrio = rampa((198, 214, 220), 3)
    lt = rampa(liquido, 3)
    l.rect(cx - 2, cy - 11, 4, 3, vidrio[-1])          # cuello
    l.elipse(cx, cy, 7, 8, vidrio[len(vidrio) // 2])   # panza
    l.elipse(cx, cy + 1, 5, 6, lt[len(lt) // 2])
    l.elipse(cx - 2, cy - 1, 2, 2, lt[-1])
    l.rect(cx - 3, cy - 13, 6, 3, rampa(MADERA, 3)[1])  # corcho


def monton(l, cx, cy, tonos, n=3):
    """Un montón de material suelto (tierra, arena, nieve, piedras)."""
    l.elipse(cx, cy + 4, 10, 5, tonos[len(tonos) // 2])
    l.elipse(cx - 3, cy, 6, 5, tonos[-1])
    l.elipse(cx + 4, cy + 1, 5, 4, tonos[len(tonos) // 2])
    if n > 2:
        l.elipse(cx, cy - 3, 4, 3, tonos[-1])


# — las recetas ————————————————————————————————————————————————————


def herramienta(cabeza, material):
    """Mango en diagonal de abajo-izquierda a arriba-derecha y la cabeza arriba.
    Es la pose en la que el pack dibuja sus herramientas de jardín: se lee la
    silueta entera sin que nada toque el borde de la celda."""
    def f(l):
        m = rampa(MADERA, 4)
        t = rampa(material, 4)
        mango(l, 8, 28, 19, 11, m)
        cabeza(l, 20, 8, t)
        l.contorno(t[0] if material != MADERA else m[0])
    return f


def _orange(l):
    t = rampa(NARANJA, 4)
    fruta(l, 16, 18, 8, t)
    l.contorno(t[0])


def _grape(l):
    t = rampa(UVA, 4)
    for cx, cy in [(12, 14), (20, 14), (16, 19), (10, 20), (22, 20), (16, 25)]:
        l.elipse(cx, cy, 4, 4, t[len(t) // 2])
        l.elipse(cx - 1, cy - 1, 2, 2, t[-1])
    l.contorno(t[0])


def _bread(l):
    t = rampa(PAN, 4)
    l.elipse(16, 18, 11, 7, t[len(t) // 2])
    l.elipse(16, 16, 10, 5, t[-1])
    for x in (11, 16, 21):
        l.linea(x, 13, x + 2, 20, t[1])
    l.contorno(t[0])


def _meat(l):
    t = rampa(CARNE, 4)
    l.elipse(17, 16, 9, 8, t[len(t) // 2])
    l.elipse(15, 14, 5, 4, t[-1])
    h = rampa((236, 232, 220), 3)
    l.rect(8, 21, 9, 3, h[-1])
    l.contorno(t[0])


def _pie(l):
    t = rampa(MASA, 4)
    l.elipse(16, 20, 12, 7, t[len(t) // 2])
    l.elipse(16, 17, 10, 5, t[-1])
    r = rampa(ROJO, 3)
    l.elipse(16, 17, 7, 3, r[len(r) // 2])
    l.contorno(t[0])


def _soup(l):
    b = rampa((214, 220, 228), 4)
    c = rampa(CALDO, 3)
    l.elipse(16, 20, 12, 7, b[len(b) // 2])
    l.elipse(16, 17, 11, 5, b[-1])
    l.elipse(16, 17, 8, 3, c[len(c) // 2])
    l.contorno(b[0])


def _wood(l):
    t = rampa(MADERA, 4)
    for i, (x, y) in enumerate([(6, 12), (6, 19)]):
        l.rect(x, y, 20, 6, t[len(t) // 2])
        l.rect(x, y, 20, 2, t[-1])
        l.elipse(x + 20, y + 3, 3, 3, t[1])
    l.contorno(t[0])


def _sticks(l):
    t = rampa(MADERA, 4)
    mango(l, 5, 23, 26, 13, t, grosor=4)
    mango(l, 6, 11, 27, 21, t, grosor=4)
    l.contorno(t[0])


def _stone(l):
    t = rampa(PIEDRA, 4)
    monton(l, 16, 15, t)
    l.contorno(t[0])


def _dirt(l):
    t = rampa(TIERRA, 4)
    monton(l, 16, 15, t)
    l.contorno(t[0])


def _sand(l):
    t = rampa(ARENA, 4)
    monton(l, 16, 15, t)
    l.contorno(t[0])


def _snow(l):
    t = rampa(NIEVE, 4)
    monton(l, 16, 15, t)
    l.contorno(t[0])


def _leather(l):
    """Una piel curtida: contorno irregular con las cuatro patas, que es lo que
    la distingue de una tabla (el primer intento era un rectángulo y se leía
    como madera)."""
    t = rampa(CUERO, 4)
    l.poligono(
        [(9, 8), (16, 6), (23, 8), (26, 13), (23, 16), (25, 22), (18, 24),
         (16, 27), (13, 24), (7, 22), (9, 16), (6, 13)],
        t[len(t) // 2],
    )
    l.poligono([(11, 9), (16, 8), (21, 9), (22, 14), (11, 14)], t[-1])
    l.elipse(16, 18, 4, 3, t[1])
    l.contorno(t[0])


def _iron_ingot(l):
    t = rampa(HIERRO, 4)
    lingote(l, 16, 18, t)
    l.contorno(t[0])


def _gold_ingot(l):
    t = rampa(ORO, 4)
    lingote(l, 16, 18, t)
    l.contorno(t[0])


def _gem_red(l):
    t = rampa(ROJO, 4)
    gema(l, 16, 16, t)
    l.contorno(t[0])


def _gem_blue(l):
    t = rampa(AZUL, 4)
    gema(l, 16, 16, t)
    l.contorno(t[0])


def _torch(l):
    m = rampa(MADERA, 4)
    f = rampa((238, 168, 48), 4)
    mango(l, 14, 28, 16, 14, m, grosor=3)
    l.elipse(16, 10, 5, 7, f[len(f) // 2])
    l.elipse(16, 9, 3, 4, f[-1])
    l.contorno(m[0])


def _shield_wood(l):
    m = rampa(MADERA, 4)
    h = rampa(HIERRO, 3)
    l.poligono([(8, 6), (24, 6), (24, 18), (16, 27), (8, 18)], m[len(m) // 2])
    l.poligono([(8, 6), (24, 6), (24, 10), (8, 10)], m[-1])
    l.rect(15, 7, 3, 18, h[len(h) // 2])
    l.contorno(m[0])


def _hand(l):
    t = rampa((226, 178, 140), 4)
    l.rect(11, 14, 10, 12, t[len(t) // 2])
    l.rect(11, 14, 10, 3, t[-1])
    for x in (11, 14, 17):
        l.rect(x, 9, 2, 6, t[len(t) // 2])
    l.rect(20, 15, 3, 6, t[1])
    l.contorno(t[0])


def _jetpack(l):
    t = rampa(METAL, 4)
    f = rampa((238, 150, 48), 3)
    for x in (8, 18):
        l.rect(x, 8, 6, 14, t[len(t) // 2])
        l.rect(x, 8, 6, 3, t[-1])
        l.elipse(x + 3, 24, 3, 3, f[len(f) // 2])
    l.rect(14, 11, 4, 8, t[1])
    l.contorno(t[0])


def _probe(l):
    t = rampa(METAL, 4)
    a = rampa(AZUL, 3)
    l.elipse(16, 16, 6, 6, t[len(t) // 2])
    l.elipse(14, 14, 3, 3, t[-1])
    l.rect(3, 14, 8, 5, a[len(a) // 2])
    l.rect(21, 14, 8, 5, a[len(a) // 2])
    l.contorno(t[0])


def _potion(color):
    def f(l):
        frasco(l, 16, 18, color)
        l.contorno(rampa((198, 214, 220), 3)[0])
    return f


def _part(forma):
    def f(l):
        t = rampa(METAL, 4)
        r = rampa(ROJO, 3)
        forma(l, t, r)
        l.contorno(t[0])
    return f


def _nose(l, t, r):
    l.poligono([(16, 4), (23, 20), (9, 20)], t[len(t) // 2])
    l.poligono([(16, 4), (20, 13), (12, 13)], t[-1])
    l.rect(9, 20, 14, 4, r[len(r) // 2])


def _tank(l, t, r):
    l.rect(10, 7, 12, 18, t[len(t) // 2])
    l.rect(10, 7, 12, 4, t[-1])
    l.rect(19, 7, 3, 18, t[1])
    l.rect(10, 15, 12, 3, r[len(r) // 2])


def _engine(l, t, r):
    l.poligono([(11, 6), (21, 6), (25, 24), (7, 24)], t[len(t) // 2])
    l.rect(11, 6, 10, 3, t[-1])
    l.rect(7, 21, 18, 3, r[len(r) // 2])


def _fin(l, t, r):
    l.poligono([(12, 5), (18, 5), (18, 27), (4, 27)], t[len(t) // 2])
    l.poligono([(12, 5), (18, 5), (18, 12), (12, 12)], t[-1])
    l.rect(18, 5, 4, 22, r[len(r) // 2])


def _lander(l, t, r):
    l.rect(9, 8, 14, 9, t[len(t) // 2])
    l.rect(9, 8, 14, 3, t[-1])
    l.linea(9, 17, 4, 27, t[1])
    l.linea(22, 17, 27, 27, t[1])
    l.rect(3, 26, 5, 3, r[len(r) // 2])
    l.rect(24, 26, 5, 3, r[len(r) // 2])


RECETAS = {
    "items/orange": ("Naranja", _orange),
    "items/grape": ("Uvas", _grape),
    "items/bread": ("Pan", _bread),
    "items/meat": ("Carne", _meat),
    "items/pie": ("Tarta", _pie),
    "items/soup": ("Sopa", _soup),
    "items/wood": ("Madera", _wood),
    "items/sticks": ("Palos", _sticks),
    "items/stone": ("Piedra", _stone),
    "items/dirt": ("Tierra", _dirt),
    "items/sand": ("Arena", _sand),
    "items/snow": ("Nieve", _snow),
    "items/leather": ("Cuero", _leather),
    "items/iron_ingot": ("Lingote de hierro", _iron_ingot),
    "items/gold_ingot": ("Lingote de oro", _gold_ingot),
    "items/gem_red": ("Gema roja", _gem_red),
    "items/gem_blue": ("Gema azul", _gem_blue),
    "items/torch": ("Antorcha", _torch),
    "items/shield_wood": ("Escudo de madera", _shield_wood),
    "items/hand": ("Mano", _hand),
    "items/jetpack": ("Mochila propulsora", _jetpack),
    "items/probe": ("Sonda", _probe),
    "items/potion_blue": ("Poción azul", _potion(AZUL)),
    "items/potion_green": ("Poción verde", _potion(VERDE)),
    "items/axe": ("Hacha", herramienta(cabeza_hacha, HIERRO)),
    "items/wood_axe": ("Hacha de madera", herramienta(cabeza_hacha, MADERA)),
    "items/stone_axe": ("Hacha de piedra", herramienta(cabeza_hacha, PIEDRA)),
    "items/pickaxe": ("Pico", herramienta(cabeza_pico, HIERRO)),
    "items/wood_pickaxe": ("Pico de madera", herramienta(cabeza_pico, MADERA)),
    "items/stone_pickaxe": ("Pico de piedra", herramienta(cabeza_pico, PIEDRA)),
    "items/part_nose": ("Punta de cohete", _part(_nose)),
    "items/part_tank": ("Tanque de cohete", _part(_tank)),
    "items/part_engine": ("Motor de cohete", _part(_engine)),
    "items/part_fin": ("Aleta de cohete", _part(_fin)),
    "items/part_lander": ("Tren de aterrizaje", _part(_lander)),
}


def main():
    seco = "--dry-run" in sys.argv
    pagina = Pagina()
    sprites, objetos = {}, {}
    for clave, (label, receta) in RECETAS.items():
        l = Lienzo(CELDA, CELDA)
        receta(l)
        # Un icono vive centrado en su casillero: ni se apoya ni tira sombra.
        l.verificar(apoyado=False)
        col, row = pagina.poner(l.im)
        sprites[clave] = {
            "png": PAGINA_REL,
            "grid": [CELDA, CELDA],
            "rect": {"col": col, "row": row, "w": 1, "h": 1},
        }
        objetos[clave] = label
    publicar(pagina, sprites, objetos, PAGINA_REL, seco)
    print(f"{len(RECETAS)} iconos dibujados")


if __name__ == "__main__":
    main()
