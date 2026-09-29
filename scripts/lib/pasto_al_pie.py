"""**EL ÁRBOL SIN SU MATA DE PASTO AL PIE** (pedido del usuario, 28 sep 2026).

Los árboles del pack traen pintada una elipse de pasto debajo del tronco. En el
juego el suelo ya es pasto (o nieve, o arena, o vereda) y esa elipse se ve como
un felpudo verde pegado al árbol: *"elimina la parte redonda de la base, que
sólo quede el tronco"*. La sombra de contacto la pone el juego
(`light2d::attach_ground_shadows`), así que el pie no queda flotando.

Nada se escribe a ojo, todo se MIDE sobre el recorte:

- **la paleta del pasto** es la que se midió en la mitad de abajo de las
  elipses de los 407 árboles del pack (`PASTO`), más el gris azulado de la
  sombra pintada sobre él (`SOMBRA`);
- **dónde está la elipse**: su fila de abajo es la última fila opaca del
  recorte; su centro, la fila más ancha de pasto subiendo desde ahí (el ancho es
  de extremo a extremo, así el tronco en el medio no lo acorta); su techo, el
  espejo del fondo respecto del centro.

Dentro de esa franja, y sólo dentro del ancho de la elipse, se borra lo que es
de la paleta del pasto o de su sombra. El tronco y las raíces son marrones y
quedan. Es idempotente: un recorte sin pasto en su fila de abajo no se toca.
"""

import numpy as np

# Medido: los colores de la mitad inferior de las elipses (112.405 px del
# primero … 729 del último) y los de la mitad superior que casi no aparecen en
# las copas. Ver `medir()` más abajo para regenerarlo.
PASTO = {
    (52, 122, 75), (55, 133, 78), (45, 114, 86), (31, 97, 79),
    (64, 138, 79), (71, 151, 87), (57, 134, 83),
}
SOMBRA = {(58, 58, 80), (61, 61, 82)}
# Cuántas filas por encima del techo calculado puede asomar el borde de la
# elipse (ver `podar`). Medido en `nature/arbol_100`: dos.
MESETA = 4


def _extremos(a, y):
    xs = [x for x in range(a.shape[1]) if a[y, x, 3] > 0 and tuple(int(t) for t in a[y, x, :3]) in PASTO]
    return (xs[0], xs[-1]) if xs else None


def elipse(a):
    """`(techo, centro, fondo, x0, x1)` de la elipse de pasto, o `None`."""
    filas = np.where(a[:, :, 3].max(1) > 0)[0]
    if len(filas) == 0:
        return None
    fondo = int(filas[-1])
    e = _extremos(a, fondo)
    if not e:
        return None
    ancho, centro, x0, x1 = e[1] - e[0], fondo, e[0], e[1]
    y = fondo
    while y > 0 and fondo - y < 48:
        y -= 1
        e = _extremos(a, y)
        if not e or e[1] - e[0] < ancho:
            break
        if e[1] - e[0] > ancho:
            ancho, centro, x0, x1 = e[1] - e[0], y, e[0], e[1]
    # En pixel art las filas van de a pares: el centro real está entre la
    # primera fila más ancha y la que la repite, de ahí el -1.
    techo = max(0, 2 * centro - fondo - 1)
    return techo, centro, fondo, x0, x1


def podar(a):
    """Borra la elipse de un recorte RGBA (array numpy, se modifica en el
    lugar). Devuelve cuántos píxeles borró."""
    el = elipse(a)
    if not el:
        return 0
    techo, _, fondo, x0, x1 = el
    n = 0
    for y in range(techo, fondo + 1):
        for x in range(x0, x1 + 1):
            p = a[y, x]
            if p[3] == 0:
                continue
            c = (int(p[0]), int(p[1]), int(p[2]))
            if c in PASTO or c in SOMBRA:
                a[y, x] = (0, 0, 0, 0)
                n += 1
    # **El techo del espejo queda corto**: la fila más ancha de una elipse de
    # pixel art es una meseta de dos o tres filas y el espejo se calcula desde
    # la primera, así que el borde de arriba de la elipse (una o dos filas de
    # pasto a los costados de la copa) sobrevivía como una rayita verde. Se
    # sube de a una fila borrando sólo el pasto que se apoya en algo ya borrado.
    for y in range(techo - 1, max(-1, techo - 1 - MESETA), -1):
        for x in range(x0, x1 + 1):
            p = a[y, x]
            if p[3] == 0 or a[y + 1, x, 3] != 0:
                continue
            if (int(p[0]), int(p[1]), int(p[2])) in PASTO:
                a[y, x] = (0, 0, 0, 0)
                n += 1
    return n
