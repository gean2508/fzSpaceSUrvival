#!/usr/bin/env python3
"""**GUIRNALDAS DE LUZ** — el cable con bombitas colgado entre dos postes.

    python3 scripts/dibujar-guirnaldas.py
    python3 scripts/dibujar-guirnaldas.py --dry-run

## Por qué se dibuja en vez de importarse

Se buscó primero, como manda la skill `pixel-art`, y **no está en el pack**:

- en el manifest, en castellano y en inglés: `guirnalda`, `luces`, `bombita`,
  `string_light`, `fairy_light`, `garland`, `banderín` → lo único que aparece es
  `prop/luces_calabaza_*`, que **no son una guirnalda**: son calabazas de
  Halloween con una vela adentro (mirado, no supuesto — `scripts/ver-claves.py`);
- en los PNG crudos de `PRUEBAS-CODIGOS/`: `*Light*`, `*Party*`, `*Christmas*`,
  `*Bunting*` → semáforos, reflectores de estadio, faroles y gorros de fiesta del
  generador de personajes. Cable con bombitas, ninguno.

Y hace falta: es el elemento que más se repite en las capturas de referencia —
las hileras de puntos cálidos cruzando la plaza son las que le dan escala y
profundidad a la noche, porque son la única fuente de luz que está en el AIRE y
no en el piso.

## Qué se dibuja

Un OBJETO completo, no una pieza: dos postes, el cable con su catenaria y las
bombitas colgando. **Entero a propósito** (regla 8 del proyecto): una guirnalda
partida en tramos modulares no se puede sembrar suelta —quedarían cables
empezados en el medio del pasto— y componerla necesitaría un sistema de
colocación que no existe. Un tramo completo con sus dos postes se planta como se
planta un banco.

Cuatro variantes, y su diferencia es sólo el largo del vano y el color de las
bombitas: es lo que evita que una plaza se lea como el mismo objeto repetido.

## El estilo

Todo pasa por `scripts/lib/pixel.py`, o sea por la paleta medida de LimeZu, y
por `verificar()`. Las decisiones de forma que impone el pack y que acá importan:

- **Sin contorno.** El cable es una línea de un tono; su silueta es donde se
  acaba el cable. `contorno()` sólo oscurece el lado sur/este, que es lo que hace
  LimeZu.
- **Sin dithering.** La catenaria se dibuja con `linea()` en escalones enteros;
  un cable "suavizado" es de otro juego.
- **Sombra de contacto al pie de cada poste**, en (58,58,80) a alfa 100. Es lo
  que apoya el objeto en el suelo en vez de dejarlo flotando.
- **El poste se apoya al ras del borde de abajo** del recorte.

La emisión de estas claves NO se escribe acá: la mide `import-emisiones.py`
sobre el dibujo terminado, igual que la de cualquier asset comprado. Por eso el
nombre lleva `luces_`, que es lo que la hace candidata.
"""

import argparse
import sys

sys.path.insert(0, "scripts")

from lib.pixel import CELDA, Lienzo, Pagina, publicar, rampa  # noqa: E402

PAGINA_REL = "art/gen/prop/guirnaldas.png"

#: El poste: madera del pack, la misma familia de marrones con la que están
#: hechos los postes de cerca y los muelles.
MADERA = (122, 88, 58)

#: El cable. Oscuro y tintado (nunca negro neutro — el pack tinta hasta su
#: sombra), para que se lea contra el cielo de noche sin ser una raya key.
CABLE = (58, 58, 80)

#: Los colores de bombita, uno por variante. Salen de las capturas de
#: referencia: la guirnalda de la plaza es ámbar, y las de las calles laterales
#: tiran a papel de arroz (blanco cálido), rojo de farolillo y verde de fiesta.
#: **Todos claros a propósito.** Una bombita ENCENDIDA es lo más brillante del
#: sprite: el vidrio de `prop/farol` mide 0,93 de luminancia y el de una vela del
#: pack 0,88. Las dos primeras versiones usaban un rojo y un verde de "color de
#: farolillo" (0,55 y 0,75) y `import-emisiones.py` las descartaba con razón —
#: **medían como apagadas**, y una guirnalda apagada no ilumina nada. El tinte se
#: sigue leyendo porque el reflejo y la punta bajan a los tonos saturados de la
#: rampa; lo que sube es el cuerpo del vidrio.
BOMBITAS = [
    ("ambar", (255, 216, 150)),
    ("papel", (252, 242, 220)),
    ("rojo", (255, 178, 152)),
    ("verde", (205, 245, 180)),
]

#: Largo del vano en CELDAS, por variante. Cuatro largos distintos es lo que
#: hace que una plaza con cuatro guirnaldas no se lea como cuatro copias.
VANOS = [4, 5, 3, 6]

#: Alto del recorte en celdas. Dos: el poste ocupa la de abajo y el cable cuelga
#: por la de arriba, que es donde tiene que estar para leerse "en el aire".
ALTO_CELDAS = 2

#: Cuánto baja el cable en el medio del vano, en píxeles. Una catenaria plana no
#: se lee como cable colgado; una muy hundida choca con las cabezas.
PANZA = 13

#: Alto del poste en píxeles, desde el suelo. Tres cuartos del recorte: el cable
#: tiene que arrancar por encima del alto de una persona (32 px = 1 m).
POSTE_H = 46

#: Ancho del poste en píxeles. **Tres, no dos.** Con dos el poste desaparecía
#: contra el pasto —comparado con el farol del pack, que tiene cuatro— y la
#: guirnalda se leía como un cable flotando sin de qué colgar.
POSTE_W = 3


def catenaria(x0: int, x1: int, y_top: int, panza: int):
    """Los puntos (x, y) del cable entre dos postes.

    Parábola y no coseno hiperbólico: a este tamaño no se distinguen, y una
    parábola se evalúa sin librería. Devuelve un punto por columna, redondeado a
    entero — **el cable tiene que caer en escalones enteros**, porque el pack no
    interpola y un cable anti-aliaseado se ve borroso al lado de todo lo demás.
    """
    span = max(x1 - x0, 1)
    for x in range(x0, x1 + 1):
        t = (x - x0) / span * 2.0 - 1.0  # -1..1
        yield x, y_top + round(panza * (1.0 - t * t))


def dibujar(vano_celdas: int, color_bombita) -> Lienzo:
    w = vano_celdas * CELDA
    h = ALTO_CELDAS * CELDA
    l = Lienzo(w, h)

    madera = rampa(MADERA, 4)
    luz = rampa(color_bombita, 3)

    x_izq, x_der = 3, w - 3 - POSTE_W
    suelo = h - 1
    y_top = suelo - POSTE_H

    # 1) La sombra de contacto de cada poste, PRIMERO: va debajo de todo.
    l.sombra_al_pie(x_izq - 2, POSTE_W + 4)
    l.sombra_al_pie(x_der - 2, POSTE_W + 4)

    # 2) Los dos postes. `volumen` pone el relleno medio, la tapa clara arriba y
    # la columna derecha un tono más oscura: la luz del pack viene de
    # arriba-izquierda (+5 y +4, medido) y esto es exactamente eso.
    for x in (x_izq, x_der):
        l.volumen(x, y_top, POSTE_W, POSTE_H, madera, tapa=2)
        # El nudo de arriba: un ensanche de 1 px a cada lado. Sin él el cable
        # nace del aire al lado del palo; con él se ve ATADO, que es lo que
        # convierte dos palos y una curva en una guirnalda.
        l.rect(x - 1, y_top, POSTE_W + 2, 2, madera[-1])
        l.rect(x - 1, y_top + 2, POSTE_W + 2, 1, madera[1])

    # 3) El cable, colgado de la punta de un poste a la del otro.
    puntos = list(catenaria(x_izq + POSTE_W, x_der, y_top + 1, PANZA))
    for x, y in puntos:
        l.punto(x, y, CABLE)

    # 4) Las bombitas: reparto parejo a lo largo del cable, colgando de él.
    # Cada una son tres píxeles —el casquillo oscuro, el vidrio y el reflejo—
    # porque a 32 px por metro una bombita mide eso y no más. El reflejo va
    # ARRIBA-IZQUIERDA, que es de donde viene la luz en todo el pack.
    # **Dos por celda de vano.** La primera versión ponía una cada dos celdas y
    # con un solo píxel de vidrio: a tamaño de juego eso no se leía como una
    # guirnalda sino como un alambre con mugre. Una bombita del pack (el vidrio
    # de `prop/farol`) mide 3x4 px, y ése es el tamaño que se usa acá.
    cuantas = max(4, vano_celdas * 2)
    for k in range(cuantas):
        # Salteando los extremos: una bombita pegada al poste no se ve.
        t = (k + 1) / (cuantas + 1)
        x, y = puntos[round(t * (len(puntos) - 1))]
        # Casquillo: el punto oscuro del que cuelga. Sin él la bombita parece
        # pegada al cable en vez de colgada.
        l.punto(x, y + 1, CABLE)
        # El vidrio: un bloque de 3x2 con la punta abajo. **Bloque y no cruz.**
        # La primera versión ponía una columna y una fila del mismo tono y salía
        # un signo "+" — que es lo que pasa cuando se dibuja el CONTORNO de una
        # forma de 3 px en vez de su relleno. A este tamaño una lamparita es
        # macizo con un reflejo, nada más.
        # El cuerpo va en el tono SATURADO y sólo el píxel del medio va al tono
        # más claro. Es lo que hace el pack con cualquier vidrio encendido —el de
        # `prop/farol` tiene un único píxel casi blanco rodeado de ámbar— y acá
        # además decide qué COLOR mide `import-emisiones.py`: el tinte sale del
        # anillo de 2 px alrededor del núcleo, así que si el bulbo entero fuera
        # blanco, las cuatro guirnaldas darían la misma luz blanca y las
        # variantes de color no servirían para nada.
        l.rect(x - 1, y + 2, 3, 2, luz[1])
        l.punto(x, y + 2, luz[-1])
        l.punto(x, y + 4, luz[0])

    # 5) Y el escalón oscuro del lado en sombra. NO es un contorno: `contorno()`
    # sólo baja un tono el borde que da al sur y al este (ver su docstring).
    l.contorno(madera[0])
    return l


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    pagina = Pagina()
    sprites, objetos = {}, {}
    for (nombre, color), vano in zip(BOMBITAS, VANOS):
        l = dibujar(vano, color)
        l.verificar()
        col, row = pagina.poner(l.im)
        clave = f"prop/luces_guirnalda_{nombre}"
        sprites[clave] = {
            "png": PAGINA_REL,
            "grid": [CELDA, CELDA],
            "rect": {"col": col, "row": row, "w": vano, "h": ALTO_CELDAS},
        }
        objetos[clave] = f"Guirnalda de luces ({nombre})"
        print(f"  {clave}: vano de {vano} celdas en ({col},{row})")

    publicar(pagina, sprites, objetos, PAGINA_REL, seco=args.dry_run)


if __name__ == "__main__":
    main()
