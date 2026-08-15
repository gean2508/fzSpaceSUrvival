#!/usr/bin/env python3
"""**EL ACANTILADO DE VERDAD: pared alta, del kit modular del pack.**

    python3 scripts/import-acantilado.py
    python3 scripts/import-acantilado.py --dry-run

## Por qué existe

`import-relieve.py` sacó el relieve de los cuadrados grises poniéndole la
transición de terreno del pack (pasto sobre llano). Sirvió para el color, pero no
para la LECTURA: una transición de terreno es plana, y lo que se pidió es una
**pared** — meseta arriba, roca cayendo, falda al pie —, alta y modular.

Ese kit existe y es uno solo en todo Modern Exteriors: el **Sand Mountain** del
tema 21 (playa). Se buscó en serio antes de dar el pack por incompleto: los 6.142
recortes sueltos de los 24 temas por familia, los 26 autotiles, y comparando la
lámina de referencia contra cada hoja del pack. Las únicas familias de relieve
son `Sand_Mountain`, `Mound_1/2` (montículos de un tile) y `Wall_1/2/3` (muros de
ladrillo de jardín). **No hay gemelo de pasto**, así que la copia de llano se
TIÑE — decisión del usuario.

## LAS TRES FILAS QUE HAY QUE ACERTAR (y que estaban mal)

La meseta chica (`Base_Single_Small_Sand_Mountain`, 3×5 celdas) tiene **dos
bordes distintos**, y confundirlos fue el bug que se ve en las capturas:

```
  fila 0   CRESTA: el canto LEJANO, el que se ve desde el aire mirando al norte
  fila 1   meseta (arena limpia, con el filo lateral en las columnas 0 y 2)
  fila 2   meseta + el labio de la pared asomando por abajo   ← BORDE SUPERIOR
  fila 3   la PARED de cantos rodados                          ← CUERPO
  fila 4   la pared + la falda naranja al pie                  ← BORDE INFERIOR
```

La versión vieja armaba las caras que miran al SUR con `fila 0 + filas 3-4`: le
pegaba la cresta lejana encima de la pared. En el juego eso salía como una franja
marrón oscura flotando sobre la meseta, y como el teñido a pasto sólo alcanzaba a
la arena (que en esa columna era una tira de cuatro píxeles), el acantilado de
pasto y el de arena se veían **iguales**. Ahora cada cara pide su fila:

- mira al SUR → filas 2,3,4 (labio + cuerpo + pie). Es la única cara que se ve
  entera en 3/4 y por eso es la que lleva la pared.
- mira al NORTE → fila 0 sola. Es el canto de enfrente: una celda, sin pared.
- mira al ESTE/OESTE → fila 1 sola (meseta con su filo). De una cara que mira al
  este, en 3/4, sólo se ve el filo. **Una celda, no tres**: el costado modular
  se pegaba debajo y una corrida norte-sur lo repetía cada metro (ver
  [`CARA_LADO`] en `pieza`).
- esquina CÓNCAVA → el ENLACE curvo del pack, una celda ([`CANTO_CURVO`]): los
  cuatro `Top_Small_Sand_Mountain_Big_*` traen la roca doblando la esquina, que
  es lo que empalma un tramo de muro con el siguiente. Antes iba el canto recto
  ([`CARA_CANTO`], que queda de respaldo) y el perímetro salía a trozos.

**Cada recorte en su sitio y ENTERO.** Se probó vaciarle el suelo dibujado a las
piezas para que no desentonaran con el pasto, y poner cuerpo de pared en todas
las caras para que ninguna quedara fina: las dos cosas dejaron el muro hecho
bloques de roca pelada. El pack ya resolvió cómo se ve un acantilado. Si el color
no pega, se TIÑE con la losa que está al lado ([`LOSA_DEL_MATERIAL`]) o se corta
una variante por clima ([`PASTOS_DE_CLIMA`]); no se le borra la mitad al dibujo.

## Qué produce

`art/pack/acantilado.png` y las claves `cliff/*`, `ledge/*` y `slope/*` de los
cinco materiales de [`MATERIALES`] más los cuatro climas de [`PASTOS_DE_CLIMA`]. Cada forma es de **1 celda de ancho**: la
primera fila es la celda del propio acantilado y las de abajo CUELGAN hacia el
sur sobre la terraza baja (ver `world2d::spawn_chunk_tile_art`).

Además de las trece formas del autotile, `cliff/*` trae las dos **mejillas de
rampa** (`rampa_o` / `rampa_e`): la celda de pared que va pegada a un paso, con
el espolón de roca que el kit dibuja al costado de la escalinata. Sin ellas la
rampa se abre como un agujero cuadrado en la pared.

`slope/*` es la escalinata en sí: las rayas por las que se sube.

| cara | hoja | qué se corta |
|---|---|---|
| baja al SUR | `Four_Sand_Mountains_1_Bottom` | columnas 1-2, filas 1-3 |
| baja al NORTE | `Four_Sand_Mountains_1_Top` | la muesca vista desde el aire, 1 celda |
| baja al ESTE/OESTE | la meseta chica | fila 1 de la columna del filo, **1 celda** |

**Las hojas `_Left` y `_Right` NO entran, y esta vez está medido.** Entraron el
9 ago 2026 con el argumento de que sus columnas del medio son "tres celdas
repetibles (roca, rayas, filo)". Miradas las hojas, no lo son: la banda de roca
cruza las 4×4 **en diagonal**, así que cualquier columna cortada es un trozo de
esa diagonal —un palito de roca rodeado de meseta— que no empalma con la celda
de al lado. Sembrado a lo largo de una corrida eso es un PEINE de barritas
verticales idénticas cada metro, que es lo que el usuario tachó en la captura
del campo. De un paso que baja de lado, en 3/4, se ve el filo de la meseta; la
escalinata la enseña la cara sur, que es la única que da a la cámara.

Eso deja un hueco CONOCIDO, no un olvido: el pack sí dibujó el paso lateral y el
juego no lo enseña. La salida que no se probó es dejar de cortarlo por celda y
colocar la hoja ENTERA una vez por paso —una pieza de kit, regla 8, como el
semáforo—. Ver [`RAMPA_OESTE`].

## El bordillo

`ledge/*` es el mismo dibujo con UNA fila de pared: es el escalón que se trepa
(`relief::climbable_edge`), y tiene que distinguirse de un vistazo del muro que
hay que rodear.
"""

import json
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
PACK = ROOT / "PRUEBAS-CODIGOS/modernexteriors-win/Modern_Exteriors_32x32"
SINGLES = PACK / "Modern_Exteriors_Complete_Singles_32x32"
PREF = "21_Beach_32x32_"

ASSETS = ROOT / "native/crates/ha-game/assets"
MANIFEST = ASSETS / "art/manifest.json"
PAGINA_REL = "art/pack/acantilado.png"
INDEX = ROOT / "scripts/acantilado-import-index.json"

CELL = 32
PAGINA_COLS = 32

#: Alto de una pieza de acantilado que mira al sur, en celdas: el labio + la
#: pared + el pie. **Tres es lo que mide EL RECORTE DEL PACK**, y nada más.
#:
#: Este comentario decía que tres era también lo que mide el desnivel, porque
#: "en este mundo no existe la pared de dos escalones": eso fue cierto mientras
#: todas las terrazas medían lo mismo y las 1.276 caras que miran al sur bajaban
#: exactamente un escalón. **Ya no lo es.** `relief::SALTO_DE_TERRAZA` reparte la
#: escalera de 1 a 10 escalones según la altura, y del lado del juego
#: `world2d::pared_apilada` APILA: labio + N cuerpos + pie. Por eso el kit se
#: corta en tres poses sueltas (`labio`, `cuerpo`, `pie`) además del autotile —
#: son las que se repiten para llegar a los diez pisos.
ALTO = 3
#: Alto de un bordillo: el mismo dibujo sin el cuerpo. Se trepa.
ALTO_BORDILLO = 2

# ---------------------------------------------------------------------------
# Medir el kit antes de recortarlo
# ---------------------------------------------------------------------------

#: La meseta chica entera. 3 celdas de ancho por 5 de alto. La `Vers_2` es la
#: MISMA pared con musgo entre los cantos: es la que le toca al pasto y al
#: bosque, y es arte del pack — no un tinte.
BASE = "Base_Single_Small_Sand_Mountain_Vers_1"
BASE_MUSGO = "Base_Single_Small_Sand_Mountain_Vers_2"
#: La fila de pared repetible, 3×1. Es PURA ROCA, sin un píxel de arena: por eso
#: sirve de referencia para saber qué colores NO se tiñen (ver [`mapa_de_tinte`]).
#:
#: **Y sus tres celdas son también los CUERPOS DE VARIANTE del muro.** Durante un
#: tiempo esta hoja entró sólo como paleta de referencia, y mientras tanto el
#: muro apilaba nueve veces la misma celda (`Base` columna 1, fila 3): en pantalla
#: eso es una cuadrícula de gofre, que es lo que se lee como "relleno" y no como
#: roca. La suposición escrita era que *"el pack no tiene variantes de cuerpo, una
#: sola por familia"*, y medida es falsa: las tres celdas de este tramo (más las
#: tres de la `Vers_2`) son cuatro dibujos distintos con la misma paleta y la
#: misma altura de cantos — exactamente lo que hace falta para que las filas no se
#: repitan. Lo verifica [`cuerpos_distintos`], que se planta si el pack las
#: colapsa.
TRAMO = "Middle_Modular_Single_Small_Sand_Mountain_Vers_1"
TRAMO_MUSGO = "Middle_Modular_Single_Small_Sand_Mountain_Vers_2"
#: Los costados, 1×1 cada uno: la pared vista DE CANTO. Son casi transparentes y
#: eso es lo correcto — en 3/4, de una cara que mira al este sólo se ve el filo.
LADO = {"edge_w": "Sand_Mountain_Small_Left_Side_Modular_1",
        "edge_e": "Sand_Mountain_Small_Right_Side_Modular_1"}
#: La escalinata que baja al sur, 4×4: columna 0 = la pared con el espolón
#: izquierdo, columnas 1-2 = las rayas por las que se sube, columna 3 = la pared
#: con el espolón derecho.
RAMPA = "Four_Sand_Mountains_1_Bottom"
#: La muesca de arriba: el paso visto desde el aire, 4×3.
RAMPA_NORTE = "Four_Sand_Mountains_1_Top"
#: **Los pasos que bajan al OESTE y al ESTE**, 4×4 cada uno.
#:
#: ⚠ **HOY NO SE DIBUJAN**, y conviene saberlo antes de creer que el pack no los
#: trae: los trae, se ven en su propia lámina de ejemplo
#: (`Example_Four_Sand_Mountains`, las dos bandas de peldaños que bajan de lado).
#: Entraron el 9 ago 2026 cortados por columnas y volvieron a salir el mismo día:
#: la banda de roca cruza la hoja **en diagonal**, así que cualquier columna
#: cortada es un trozo de esa diagonal y sembrada a lo largo de una corrida da un
#: PEINE de barritas verticales cada metro. Lo único que quedó en pie de ellas es
#: su PALETA (alimentan el conjunto `suelo` de [`main`]).
#:
#: O sea que un paso que baja al este se dibuja hoy con una celda de filo de
#: meseta y sin peldaños. La salida que NO se probó es dejar de cortarlas por
#: celda: la hoja es una composición de 4×4 —una pieza de kit, regla 8— así que
#: le tocaría entrar ENTERA y colocarse una vez por paso, como el semáforo, en
#: vez de embaldosarse.
RAMPA_OESTE = "Four_Sand_Mountains_1_Left"
RAMPA_ESTE = "Four_Sand_Mountains_1_Right"

#: Filas de [`BASE`], medidas: qué es cada una. Ver el docstring del módulo.
FILA_CRESTA = 0
FILA_MESETA = 1
FILA_LABIO = 2
FILA_CUERPO = 3
FILA_PIE = 4

#: Filas de [`RAMPA`]: la 0 es meseta limpia (ya la dibuja el suelo del chunk) y
#: de la 1 a la 3 es la bajada, que es lo que cuelga.
RAMPA_FILAS = (1, 2, 3)

#: La paleta de la MESETA (la arena de arriba), de oscura a clara. Sale de medir
#: la celda central de la meseta; se declara para que el script falle si el pack
#: cambia, no para elegirla a ojo.
ARENA_ESPERADA = (230, 174, 85)
#: La paleta de la PARED (los cantos rodados). Ésta sólo se tiñe en los mundos
#: donde la roca NO es roca terrestre (la nieve, el regolito, el basalto).
ROCA_ESPERADA = (121, 86, 83)

# ---------------------------------------------------------------------------
# Los materiales
# ---------------------------------------------------------------------------

#: **Un material por bioma, y todos con arte de verdad.**
#:
#: `world2d::cliff_material` reparte la cara del relieve por bioma y hasta ahora
#: sólo dos de los cinco tenían pixel: la sierra nevada y la Luna caían al
#: respaldo por código, un cuadrado liso. Cada entrada es
#: `(material, rampa_de_suelo, rampa_de_roca, base_con_musgo)`:
#:
#: - `rampa_de_suelo`: a qué colores va la ARENA de la meseta, de oscuro a claro.
#:   `None` = no se tiñe (la arena se queda como la dibujó LimeZu).
#: - `rampa_de_roca`: ídem para los cantos rodados. `None` = **la roca es roca**,
#:   igual en el prado que en el médano. Sólo la tiñen los mundos donde el
#:   material de la pared es otra cosa (hielo, regolito, basalto).
#: - `base_con_musgo`: usar la `Vers_2` del kit, la que trae musgo entre las
#:   piedras. Es del pack, no un tinte, y es lo que hace que un acantilado de
#:   bosque no se vea como uno de playa repintado.
#: **DE QUÉ LOSA SACA EL COLOR CADA MATERIAL.**
#:
#: El tinte no se elige a ojo: se mide del tile de suelo que el usuario ya asignó
#: en HA Studio, que es justamente el que va a quedar pegado a la pieza. `None` o
#: losa sin curar = ese material se queda con la rampa escrita a mano de
#: [`MATERIALES`].
LOSA_DEL_MATERIAL = {
    "pasto": "tiles/grass",
    "pasto_bosque": "tiles/pasto_bosque",
    "pasto_seco": "tiles/pasto_seco",
    "pasto_pantano": "tiles/pasto_pantano",
    "pasto_sierra": "tiles/pasto_sierra",
    "arena": "tiles/sand",
    "nieve": "tiles/ice",
    "regolito": "tiles/regolith",
    "basalto": "tiles/basalt",
}

#: **UN ACANTILADO POR CLIMA DE PASTO.**
#:
#: El mundo tiene cinco pastos —el de referencia más los cuatro que reparte
#: `world2d::pasto_de_bioma`— y el relieve tenía uno solo. La pieza del kit trae
#: su meseta dibujada, así que con un único `cliff/pasto` la franja de arriba
#: salía de otro verde que el pasto de al lado: medido en la captura del campo,
#: **(86,165,92) contra un pasto de sabana de (135,147,89)**. Ahí no falla el
#: recorte, falla el color, y el color se arregla tiñendo cada variante con SU
#: losa — no vaciándole el suelo a la pieza, que fue el intento anterior y dejó
#: el muro hecho bloques de roca pelada.
#:
#: Todos usan la base con musgo (`Vers_2`), que es la que el pack dibujó para
#: terreno vivo.
PASTOS_DE_CLIMA = [
    ("pasto_bosque", "de bosque"),
    ("pasto_seco", "de sabana"),
    ("pasto_pantano", "de pantano"),
    ("pasto_sierra", "de sierra"),
]

MATERIALES = [
    ("arena", None, None, False),
    ("pasto", [(55, 133, 78), (71, 151, 87), (83, 166, 93), (121, 161, 92)], None, True),
    (
        "nieve",
        [(150, 170, 196), (196, 212, 230), (228, 238, 248), (250, 252, 255)],
        [(86, 96, 116), (120, 134, 158), (158, 174, 196), (206, 218, 234)],
        False,
    ),
    (
        "regolito",
        [(88, 84, 80), (126, 121, 114), (160, 155, 147), (196, 191, 183)],
        [(58, 55, 52), (92, 88, 84), (128, 123, 117), (170, 165, 158)],
        False,
    ),
    (
        "basalto",
        [(44, 40, 48), (72, 64, 76), (104, 94, 108), (140, 128, 144)],
        [(28, 25, 32), (56, 50, 62), (88, 80, 96), (126, 116, 132)],
        False,
    ),
    # **LA ROCA PELADA: el respaldo genérico, que no tenía pixel.**
    #
    # `Tile::Cliff` declara `cliff/roca` como su clave de respaldo
    # (`topdown.rs::art_key`) y `cliff_art_key` sólo se le adelanta cuando el
    # bioma tiene material propio. O sea que cualquier cara que caiga fuera de
    # los cinco biomas —y la primera que se dibuja antes de que el bioma
    # responda— se quedaba sin arte, en color plano, siendo la clave más
    # nombrada del relieve.
    #
    # No hace falta dibujar nada: es el MISMO kit con la meseta llevada al gris
    # de `Tile::Rock` (0.52, 0.50, 0.47 → 133,128,120, el color que el juego ya
    # declara) y la roca sin teñir, que es roca en todas partes.
    (
        "roca",
        [(96, 92, 86), (118, 114, 107), (140, 135, 127), (170, 165, 156)],
        None,
        False,
    ),
]


def abrir_libre(nombre, celdas=None):
    """Un recorte del pack por su nombre COMPLETO (los de ciudad no son del tema 21)."""
    ruta = SINGLES / f"{nombre}.png"
    if not ruta.exists():
        raise SystemExit(f"falta la pieza {ruta.name} del pack")
    im = Image.open(ruta).convert("RGBA")
    if celdas is not None:
        esperado = (celdas[0] * CELL, celdas[1] * CELL)
        if im.size != esperado:
            raise SystemExit(f"{ruta.name} mide {im.size}, se esperaba {esperado}")
    return im


def abrir(nombre, celdas):
    return abrir_libre(f"{PREF}{nombre}", celdas)


def dominante(im, box):
    cuenta = {}
    for p in im.crop(box).getdata():
        if p[3] > 0:
            cuenta[p[:3]] = cuenta.get(p[:3], 0) + 1
    return max(cuenta, key=cuenta.get) if cuenta else None


def luminancia(c):
    return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]


def paleta(im, box=None):
    src = im.crop(box) if box else im
    return {p[:3] for p in src.getdata() if p[3] > 0}


def celda(im, c, r, w=1, h=1):
    return im.crop((c * CELL, r * CELL, (c + w) * CELL, (r + h) * CELL))


def columna(im, c, filas):
    """Las `filas` de la columna `c`, pegadas una debajo de otra."""
    out = Image.new("RGBA", (CELL, len(filas) * CELL), (0, 0, 0, 0))
    for i, r in enumerate(filas):
        out.paste(celda(im, c, r), (0, i * CELL))
    return out


def ventana(im, x0, filas):
    """Igual que [`columna`] pero cortando una celda de ancho **en píxeles**,
    no en la grilla: sirve para sacar un trozo que cae a caballo de dos celdas.

    Lo pide la escalinata, cuyo dibujo repetible no coincide con la grilla — ver
    [`piezas_de_rampa`]."""
    out = Image.new("RGBA", (CELL, len(filas) * CELL), (0, 0, 0, 0))
    for i, r in enumerate(filas):
        out.paste(im.crop((x0, r * CELL, x0 + CELL, (r + 1) * CELL)), (0, i * CELL))
    return out


# ---------------------------------------------------------------------------
# El teñido
# ---------------------------------------------------------------------------


def cuerpos_distintos(base, tramo):
    """**MEDIR QUE LAS VARIANTES DE CUERPO SEAN DE VERDAD DISTINTAS.**

    El muro reparte sus filas entre `cuerpo` (la del kit compuesto) y las celdas
    de [`TRAMO`]. Si el pack cambiara y esas celdas fueran copias, el reparto no
    arreglaría nada y nadie se enteraría: la pared seguiría siendo el gofre de
    antes con más claves en el manifiesto. Así que se comprueba acá, contando
    píxeles distintos entre pares.

    El umbral es bajo a propósito (**64 píxeles de 1.024, un 6 %**): no se pide
    que sean dibujos ajenos —son la misma roca del mismo kit— sino que la mancha
    de cantos no caiga en el mismo sitio.
    """
    minimo = 64
    piezas = [celda(base, 1, FILA_CUERPO)] + [
        celda(tramo, i, 0) for i in range(tramo.width // CELL)
    ]
    for i, a in enumerate(piezas):
        for j, b in enumerate(piezas[i + 1 :], start=i + 1):
            d = sum(1 for p, q in zip(a.getdata(), b.getdata()) if p != q)
            if d < minimo:
                raise SystemExit(
                    f"los cuerpos {i} y {j} se diferencian en {d} píxeles (mínimo {minimo}): "
                    "el pack colapsó las variantes y el muro volvería a ser una sola baldosa"
                )


def mapa_de_tinte(origen, destino):
    """`{color de origen: color de destino}`, por orden de LUMINANCIA.

    No se elige a ojo qué verde le toca a cada arena: se ordenan las dos rampas
    de oscuro a claro y se reparte proporcionalmente. Así el degradado de la
    meseta —que es lo que le da volumen— sobrevive al cambio de color en vez de
    aplastarse a un color plano.
    """
    orden = sorted(origen, key=luminancia)
    fin = sorted(destino, key=luminancia)
    mapa = {}
    for i, c in enumerate(orden):
        t = i / max(len(orden) - 1, 1)
        j = t * (len(fin) - 1)
        a, b = fin[int(j)], fin[min(int(j) + 1, len(fin) - 1)]
        f = j - int(j)
        mapa[c] = tuple(int(a[k] + (b[k] - a[k]) * f) for k in range(3))
    return mapa


def mapa_por_losa(origen, referencia, destino):
    """`{color del kit: color teñido}` **anclado** en el tono dominante.

    La rampa de cuatro paradas repartía los tonos del kit por su posición dentro
    de la lista, así que el color final dependía de cuántos tonos tuviera el
    recorte: la meseta terminaba más clara o más oscura que la losa de al lado
    aunque el verde fuera el correcto. Acá el tono DOMINANTE de la meseta
    (`referencia`) va exactamente al color de la losa y el resto lo sigue por
    luminancia, así que la parte plana de la pieza es indistinguible del pasto
    que tiene pegado y el filo y la sombra conservan su relieve.
    """
    base = max(luminancia(referencia), 1.0)
    mapa = {}
    for c in origen:
        f = luminancia(c) / base
        mapa[c] = tuple(min(255, max(0, int(destino[k] * f))) for k in range(3))
    return mapa


def color_de_la_losa(clave):
    """El color del tile de suelo `clave`, o `None` si no está curado.

    El pack pinta sus losas de suelo de un color plano (medido: `tiles/grass` son
    1.024 píxeles de (71,151,87) y ni uno más), así que alcanza con el dominante.
    """
    man = json.loads(MANIFEST.read_text())
    sp = man.get("sprites", {}).get(clave)
    if not sp or "rect" not in sp:
        return None
    png = ASSETS / sp["png"]
    if not png.exists():
        return None
    gw, gh = sp.get("grid", [CELL, CELL])
    r = sp["rect"]
    im = Image.open(png).convert("RGBA")
    box = (r["col"] * gw, r["row"] * gh, (r["col"] + r["w"]) * gw, (r["row"] + r["h"]) * gh)
    return dominante(im, box)


#: **LA FALDA DE ARENA: las "líneas blancas" que se colaron en cada tinte.**
#:
#: El kit dibuja, entre la meseta y los cantos rodados, una orilla clara de arena
#: —la falda por la que el médano se derrama sobre la roca—. Esos dos tonos
#: viven también DENTRO de la pared, como brillo de los cantos, así que
#: [`mapa_de_tinte`] los clasificó como ROCA y la roca no se tiñe: el acantilado
#: de pasto salía con una línea crema cruzándole el canto y otra bajándole por el
#: filo. Medido sobre `cliff/pasto` contra `cliff/arena`: de los 3.072 píxeles de
#: `edge_s`, **616 quedaban sin teñir y todos son estos dos tonos** (312 + 304);
#: en `labio`, los 148 píxeles sin teñir son éstos y nada más.
#:
#: No se pueden teñir a lo bruto —los mismos tonos son el brillo de la roca, y
#: pintarlos de verde deja la pared con manchas de musgo—, así que se tiñen
#: **sólo donde tocan el suelo**: ver [`tenir`], que crece desde los píxeles ya
#: teñidos. Así la orilla toma el color del tile vecino (que es lo que se pidió:
#: *"la textura que deberían tener es la de los tiles que ocupan"*) y el brillo
#: de los cantos, que no toca la meseta, se queda como lo dibujó LimeZu.
FALDA_DE_ARENA = {(200, 155, 132), (161, 121, 108)}


def falda_a_roca(piedra):
    """`{tono de falda: tono de ROCA}` — lo que queda de la orilla cuando no toca suelo.

    **El contagio desde el suelo no alcanzaba, y está medido.** El flood-fill de
    [`tenir`] sólo repinta la falda que TOCA un píxel de terreno ya teñido; la
    que vive metida entre los cantos rodados —el brillo de la piedra, según el
    comentario de [`FALDA_DE_ARENA`]— se quedaba con el crema de la playa. Sobre
    la hoja publicada: **el 8,74 % de los píxeles de `cliff/pasto` seguían siendo
    exactamente esos dos tonos** (3.008 de 34.428), y el 3,02 % de `ledge/pasto`.
    En pantalla eso es lo que el usuario ve como *"líneas blancas"* cruzando cada
    acantilado de pasto: no son de la nieve ni del sol, son la arena del kit de
    playa del que salió el recorte.

    La salida no puede ser pintarlas de verde (serían manchas de musgo sobre la
    roca) ni dejarlas: se las manda **a la roca**, al tono de piedra de
    luminancia más parecida que no sea falda. Así el canto conserva su brillo
    —sigue habiendo un tono claro donde el kit puso uno— pero es brillo DE ROCA y
    no una tira de médano.

    `nieve`, `regolito` y `basalto` no lo necesitan (tiñen la piedra entera, y
    medido les quedaba 0 %), y `arena` es el kit original: ahí la orilla es el
    médano derramándose sobre la pared y no se toca.
    """
    tonos = sorted(piedra - FALDA_DE_ARENA, key=luminancia)
    if not tonos:
        return {}
    return {c: min(tonos, key=lambda t: abs(luminancia(t) - luminancia(c))) for c in FALDA_DE_ARENA}


def tenir(im, mapa, mapa_suelo=None):
    """Aplica `mapa` color a color, y arrastra la falda de arena con el suelo.

    `mapa_suelo` es el trozo de `mapa` que corresponde al TERRENO. Si se pasa,
    los píxeles de [`FALDA_DE_ARENA`] pegados (4-vecinos) a un píxel de suelo ya
    teñido se tiñen **con el color de suelo** —no con el de roca—, en cadena.
    Sin él, el teñido es el de siempre.
    """
    if not mapa:
        return im
    out = im.copy()
    px = out.load()
    # El color de ANTES de teñir, que es el que sabe qué era cada píxel: un tono
    # de falda puede coincidir con el tinte de otra cosa una vez pintado.
    crudo = {}
    frontera = []
    for y in range(out.height):
        for x in range(out.width):
            p = px[x, y]
            if p[3] == 0:
                continue
            crudo[(x, y)] = p[:3]
            v = mapa.get(p[:3])
            if v:
                px[x, y] = (v[0], v[1], v[2], p[3])
            if mapa_suelo and p[:3] in mapa_suelo:
                frontera.append((x, y))
    if not mapa_suelo:
        return out
    # La orilla se contagia desde el suelo hacia adentro, y sólo por sus dos tonos:
    # el brillo de los cantos rodados no toca la meseta y no se entera.
    visto = set(frontera)
    while frontera:
        n = frontera.pop()
        for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            v = (n[0] + dx, n[1] + dy)
            if v in visto or crudo.get(v) not in FALDA_DE_ARENA:
                continue
            visto.add(v)
            c = color_de_falda(mapa_suelo, crudo[v])
            if c:
                px[v] = (c[0], c[1], c[2], px[v][3])
            frontera.append(v)
    return out


def color_de_falda(mapa_suelo, color):
    """A qué color va un tono de falda: al del tono de SUELO de luminancia más
    parecida. Así la orilla sigue el degradado del tinte —conserva su volumen— en
    vez de aplastarse a un verde plano."""
    if not mapa_suelo:
        return None
    l = luminancia(color)
    return mapa_suelo[min(mapa_suelo, key=lambda c: abs(luminancia(c) - l))]


# ---------------------------------------------------------------------------
# Armar las trece formas
# ---------------------------------------------------------------------------

#: Qué columna de la meseta chica le toca a cada forma. El nombre de la forma
#: dice hacia dónde se CAE (`world2d::autotile_shape`), así que una que cae al
#: oeste usa la columna izquierda del kit — la que trae el canto redondeado.
COLUMNA = {
    "edge_s": 1,
    "edge_n": 1,
    "edge_e": 2,
    "edge_w": 0,
    "corner_se": 2,
    "corner_sw": 0,
    "corner_ne": 2,
    "corner_nw": 0,
    "center": 1,
    # Las esquinas cóncavas usan la columna del lado hacia el que cae la
    # diagonal, igual que las convexas: la del este trae el canto derecho.
    "inner_ne": 2,
    "inner_nw": 0,
    "inner_se": 2,
    "inner_sw": 0,
}

#: Las formas que muestran la PARED de frente: las que caen hacia el SUR, que en
#: 3/4 es la única cara que se ve entera. Son las únicas que CUELGAN: el labio en
#: la celda del canto y el cuerpo y el pie tapando la terraza de abajo.
CARA_SUR = {"edge_s", "corner_se", "corner_sw", "center"}

#: Las formas que muestran la PARED de frente: las que caen hacia el SUR, que en
#: 3/4 es la única cara que se ve entera. Llevan labio + cuerpo + pie.
CARA_SUR = {"edge_s", "corner_se", "corner_sw", "center"}
#: Las que sólo enseñan el canto de enfrente: una celda y nada más. El desnivel
#: cae en dirección contraria a la cámara.
CARA_NORTE = {"edge_n", "corner_ne", "corner_nw"}
#: Las que se ven de canto: meseta con su filo y el costado modular colgando.
CARA_LADO = {"edge_e", "edge_w"}

#: **LAS ESQUINAS CÓNCAVAS: el 48 % del muro que no se dibujaba.**
#:
#: Una celda alta cuyos cuatro vecinos ortogonales están a su nivel pero con una
#: DIAGONAL más abajo no es acantilado —no se puede caer de ella caminando— así
#: que `relief::relief_cell` la marca llana y nadie le pedía pieza. Y es la mitad
#: del perímetro: medido sobre la Tierra (test `medir_acantilados`), 1.522
#: esquinas cóncavas contra 3.154 caras. En el mundo se veía como una línea de
#: puntos —muro, hueco, muro, hueco— bajando en diagonal por cada meseta.
#:
#: Lo que va ahí es el CANTO, que es lo que el kit dibuja para un borde visto
#: desde arriba: la cresta cuando la diagonal cae al norte, el labio cuando cae
#: al sur. **No** una pared colgando: la celda de abajo es terreno alto por el
#: que se camina y taparla sería poner roca sobre la meseta.
#: **Y el canto de una esquina cóncava es CURVO, no recto** (9 ago 2026).
#:
#: Poner ahí la cresta o el labio rectos tapó el agujero pero dejó el perímetro
#: hecho de trozos que no empalman: volcado con `medir_acantilados::piezas_del_muro`,
#: un borde de meseta baja en diagonal y alterna `M?w9W?w?` — pared de tres
#: celdas, canto de una, pared, canto. Con los cantos rectos eso en pantalla es
#: un gusano de roca partido, que es lo que se pidió arreglar: *"en la parte de
#: atrás tenés que cerrar toda la meseta, perfilar todo con enlace"*.
#:
#: El pack dibujó el enlace y no se estaba usando: cuatro trims de 2×1 celdas con
#: la roca CURVÁNDOSE alrededor de la esquina. De cada uno sirve una sola celda
#: —la que tiene la curva; la otra es tramo recto que ya cubren `edge_*`—, y la
#: que sirve cambia de lado según la hoja.
CANTO_CURVO = {
    # (single, columna). La diagonal que cae manda: `inner_ne` es la esquina cuya
    # diagonal noreste está más abajo, así que la roca envuelve arriba a la
    # derecha.
    "inner_ne": ("Top_Small_Sand_Mountain_Big_Right_Up", 0),
    "inner_nw": ("Top_Small_Sand_Mountain_Big_Left_Up", 1),
    "inner_se": ("Top_Small_Sand_Mountain_Big_Right_Down", 1),
    "inner_sw": ("Top_Small_Sand_Mountain_Big_Left_Down", 0),
}

#: Respaldo recto de [`CANTO_CURVO`], por si el kit cambia y una hoja no está.
CARA_CANTO = {
    "inner_ne": FILA_CRESTA,
    "inner_nw": FILA_CRESTA,
    "inner_se": FILA_LABIO,
    "inner_sw": FILA_LABIO,
}

def pieza(base, lados, cantos, forma, alto):
    """La columna de una forma: `1 x n` celdas, transparente donde no hay nada.

    La fila 0 del resultado es SIEMPRE la celda del propio acantilado (la que se
    camina); de ahí para abajo cuelga lo que tape la terraza de abajo.
    """
    col = COLUMNA[forma]
    if forma in CARA_CANTO:
        # Esquina cóncava: sólo el canto, una celda. El del pack va CURVADO
        # ([`CANTO_CURVO`]) y es el que empalma un tramo de muro con el
        # siguiente; el recto de [`CARA_CANTO`] queda de respaldo.
        if forma in cantos:
            return cantos[forma]
        return celda(base, col, CARA_CANTO[forma])
    if forma in CARA_NORTE:
        # El canto de enfrente: una celda, sin pared. Es lo que se ve del borde
        # lejano de una meseta mirando al norte.
        return celda(base, col, FILA_CRESTA)
    if forma in CARA_LADO:
        # **UNA CELDA, NO TRES.** El costado modular se pegaba `alto - 1` veces
        # debajo del filo, y una cara que mira al este o al oeste no es un tramo
        # aislado: es una corrida de norte a sur de decenas de celdas, cada una
        # colgando su copia sobre la de abajo. En pantalla eso es una TIRA de
        # roca vertical con el mismo dibujo repetido cada metro —el "se está
        # repitiendo la textura y se ve horrible" de la captura—, y además cae
        # sobre terreno que está a SU MISMO nivel: la pared de una corrida
        # norte-sur baja hacia el costado, no hacia el sur, así que ahí no hay
        # nada que tapar.
        #
        # Lo que se ve en 3/4 de una cara de canto es el FILO de la meseta, que
        # es justo lo que el kit dibujó en `FILA_MESETA` de las columnas 0 y 2.
        # Una celda embaldosa sin delatar la repetición; la pared entera la
        # sigue enseñando el extremo sur de la corrida, que es `CARA_SUR`.
        return celda(base, col, FILA_MESETA)
    # Cara sur: labio + cuerpo + pie. El bordillo (alto 2) se saltea el cuerpo —
    # es exactamente un escalón, sin ladera.
    filas = [FILA_LABIO, FILA_CUERPO, FILA_PIE] if alto >= 3 else [FILA_LABIO, FILA_PIE]
    return columna(base, col, filas[:alto])


def piezas_de_rampa(base, rampa, rampa_norte, rampa_oeste, rampa_este, alto):
    """Las piezas de la escalinata y las dos mejillas que la enmarcan.

    Devuelve `(mejillas, escalones)`:

    - `mejillas` son celdas de PARED (van en `cliff/*`): las columnas 0 y 3 del
      kit, que traen el espolón de roca contra el que muere el paso.
    - `escalones` son las rayas (van en `slope/*`): las columnas 1 y 2, que son
      dos dibujos distintos de lo mismo y se reparten por celda para que un paso
      ancho no repita la misma raya.

    **Cada cara pide SU kit.** El pack dibujó cuatro pasos, uno por dirección
    ([`RAMPA`], [`RAMPA_NORTE`], [`RAMPA_OESTE`], [`RAMPA_ESTE`]), y hasta ahora
    entraba sólo el del sur: un paso que bajaba al este salía con los escalones
    de frente, vistos desde una cara que en 3/4 se ve de canto. Ahora `edge_w` y
    `edge_e` salen de sus propias hojas — las mismas tres filas (roca, rayas,
    filo), pero dibujadas en diagonal, que es como se ve bajar un camino de lado.
    """
    filas = RAMPA_FILAS[:alto] if alto < len(RAMPA_FILAS) else RAMPA_FILAS
    mejillas = {
        "rampa_o": columna(rampa, 0, filas),
        "rampa_e": columna(rampa, 3, filas),
        # Y las mismas, fila a fila, para el espolón que tiene que acompañar a un
        # paso de diez pisos. Sin ellas el paso alto muere contra una pared lisa.
        "rampa_o_labio": columna(rampa, 0, (filas[0],)),
        "rampa_o_cuerpo": columna(rampa, 0, (filas[len(filas) // 2],)),
        "rampa_o_pie": columna(rampa, 0, (filas[-1],)),
        "rampa_e_labio": columna(rampa, 3, (filas[0],)),
        "rampa_e_cuerpo": columna(rampa, 3, (filas[len(filas) // 2],)),
        "rampa_e_pie": columna(rampa, 3, (filas[-1],)),
    }
    # **LOS ESCALONES SE CORTAN DONDE NO HAY ROCA, no por celda.**
    #
    # Cortados por celda (columnas 1 y 2) cada escalón se lleva pegado el filo de
    # roca del espolón vecino: medido sobre la hoja, la roca ocupa los píxeles
    # 0-37 y 90-127, o sea los 6 primeros de la columna 1 y los 6 últimos de la
    # 2. Un paso de una celda no lo delata, pero los pasos se ensancharon (biblia
    # §5, `el-paso-se-cruza-no-se-enhebra`) y hoy miden cuatro y cinco celdas:
    # las cuatro dibujaban su tira de roca y en pantalla eso es el PEINE de
    # barritas verticales cada metro que el usuario tachó en la captura. Las
    # únicas rocas de un paso son sus dos mejillas, y ésas ya van aparte.
    #
    # La ventana de 38 a 89 es escalón puro. Se cortan dos, corridas entre sí,
    # para que un paso ancho no repita el mismo peldaño celda a celda.
    paso_a = ventana(rampa, 40, filas)
    paso_b = ventana(rampa, 56, filas)
    # **LAS TRES FILAS SUELTAS DEL PASO: lo que se apila cuando la pared es
    # alta.**
    #
    # El kit dibuja la escalinata a la altura de UNA pared del pack (tres filas:
    # el labio de arriba, los peldaños y el filo del pie). Desde que el relieve
    # apila (`relief::SALTO_DE_TERRAZA`), un paso puede tener que bajar diez
    # pisos, y estirar esas tres filas dejaba una franja de meseta lisa de once
    # tiles cortando el muro — la queja del usuario: *"odio cómo usas las
    # rampas"*.
    #
    # La fila del MEDIO es la repetible: son peldaños y nada más. Es la misma
    # regla modular que la cascada — el asset trae su versión más alta y de ahí
    # se saca la pieza que se repite.
    escalones = {
        "labio": ventana(rampa, 40, (filas[0],)),
        "cuerpo": ventana(rampa, 40, (filas[len(filas) // 2],)),
        "pie": ventana(rampa, 40, (filas[-1],)),
        "center": paso_a,
        "edge_s": paso_a,
        "corner_se": paso_b,
        "corner_sw": paso_a,
        "inner_se": paso_b,
        "inner_sw": paso_a,
        "inner_ne": paso_b,
        "inner_nw": paso_a,
        # **EL PASO QUE BAJA DE LADO: UNA CELDA, y del kit de la meseta.**
        #
        # Estuvo cortado de `Four_Sand_Mountains_1_Right/Left` con el argumento
        # de que sus columnas del medio son "tres celdas repetibles". Miradas las
        # hojas (`scratchpad/z-rampas.png`), no lo son: **la banda de roca cruza
        # las 4×4 en DIAGONAL**, así que cualquier columna que se corte es un
        # trozo de esa diagonal —un palito de roca con la meseta alrededor— y no
        # empalma con la celda de al lado. Sembrado celda a celda a lo largo de
        # una corrida, eso es el PEINE de barritas verticales repetidas cada
        # metro que se ve en la captura del campo.
        #
        # De un paso que baja al este o al oeste, en 3/4, se ve el FILO de la
        # meseta y nada más — la escalinata la enseña la cara sur, que es la
        # única que da a la cámara. Misma celda y mismo criterio que la cara de
        # canto de un acantilado (`pieza`, [`CARA_LADO`]), para que un paso y la
        # pared que lo flanquea no se dibujen con dos ideas distintas.
        "edge_e": celda(base, 2, FILA_MESETA),
        "edge_w": celda(base, 0, FILA_MESETA),
        # El paso que baja al norte es la MUESCA vista desde el aire: una celda.
        "edge_n": celda(rampa_norte, 1, 0),
        "corner_ne": celda(rampa_norte, 3, 0),
        "corner_nw": celda(rampa_norte, 0, 0),
    }
    return mejillas, escalones


class Hoja:
    """Empaquetador por columnas de una celda de ancho."""

    def __init__(self):
        self.im = Image.new("RGBA", (PAGINA_COLS * CELL, CELL), (0, 0, 0, 0))
        self.col = 0
        self.fila = 0
        self.alto_fila = 0

    def poner(self, tile):
        h = tile.height // CELL
        if self.col >= PAGINA_COLS:
            self.fila += self.alto_fila
            self.col = 0
            self.alto_fila = 0
        self.alto_fila = max(self.alto_fila, h)
        alto_px = (self.fila + self.alto_fila) * CELL
        if alto_px > self.im.height:
            nueva = Image.new("RGBA", (self.im.width, alto_px), (0, 0, 0, 0))
            nueva.paste(self.im, (0, 0))
            self.im = nueva
        c, r = self.col, self.fila
        self.im.paste(tile, (c * CELL, r * CELL))
        self.col += 1
        return c, r


# ---------------------------------------------------------------------------
# El acantilado de CIUDAD: lo que le cuelga la gente
# ---------------------------------------------------------------------------

#: Alto de la PARED, en celdas: lo que cuelga del canto hacia abajo. Es
#: [`ALTO`] menos la celda del propio acantilado.
ALTO_PARED = ALTO - 1

#: **Lo que la gente le clava a un acantilado que tiene una ciudad al lado.**
#:
#: Un desnivel en mitad del bosque es geología y no lleva nada encima; uno que
#: cae al borde de un pueblo lleva la escalera por la que se baja, el caño que
#: desagua y la reja del sumidero. Es la misma pared, y lo que la cuenta es lo
#: que le colgaron — por eso esto se siembra por `settlement_covering` y no por
#: bioma (`world2d::spawn_chunk_cliff_urbano`).
#:
#: Cada entrada es `(clave, [(recorte_del_pack, filas_a_quedarse)])`. Las filas
#: se cuentan **desde abajo**: un caño de 4 celdas se apoya en el pie de la
#: pared y lo que sobra por arriba se descarta, que es como se ve un desagüe
#: asomando de una ladera. `None` = la pieza entera.
URBANO = [
    (
        "cliff/escalera",
        "Escalera de acantilado",
        [
            ("ME_Singles_Garden_32x32_Small_Stairs", None),
            ("ME_Singles_City_Props_32x32_Broken_Ladder_1", None),
            ("ME_Singles_Garden_32x32_Medium_Stairs", ALTO_PARED),
            ("ME_Singles_Garden_32x32_Big_Stairs", ALTO_PARED),
        ],
    ),
    (
        "cliff/tubo",
        "Caño de acantilado",
        [
            ("ME_Singles_Generic_Building_32x32_Condo_9_Pipe_1", ALTO_PARED),
            ("ME_Singles_Generic_Building_32x32_Condo_9_Pipe_2", ALTO_PARED),
        ],
    ),
    (
        "cliff/reja",
        "Reja al pie del acantilado",
        [
            ("ME_Singles_City_Props_32x32_Grate_1", None),
            ("ME_Singles_City_Props_32x32_Grate_2", None),
            ("ME_Singles_City_Props_32x32_Grate_3", None),
            ("ME_Singles_City_Props_32x32_Manhole_2", None),
        ],
    ),
]


def recorte_de_abajo(im, filas):
    """Las `filas` celdas de ABAJO del recorte. `None` = entero."""
    if filas is None:
        return im
    alto = filas * CELL
    if im.height <= alto:
        return im
    return im.crop((0, im.height - alto, im.width, im.height))


ETIQUETAS = {
    "cliff": "Acantilado",
    "ledge": "Bordillo",
    "slope": "Rampa",
}
NOMBRE = {
    "arena": "de arena",
    "pasto_bosque": "de bosque",
    "pasto_seco": "de sabana",
    "pasto_pantano": "de pantano",
    "pasto_sierra": "de sierra",
    "pasto": "con pasto",
    "nieve": "nevado",
    "regolito": "de regolito (Luna)",
    "basalto": "de basalto",
    "roca": "de roca",
}


def main():
    seco = "--dry-run" in sys.argv
    base_v1 = abrir(BASE, (3, 5))
    base_v2 = abrir(BASE_MUSGO, (3, 5))
    tramo_v1 = abrir(TRAMO, (3, 1))
    tramo_v2 = abrir(TRAMO_MUSGO, (3, 1))
    lados = {forma: abrir(n, (1, 1)) for forma, n in LADO.items()}
    rampa = abrir(RAMPA, (4, 4))
    rampa_norte = abrir(RAMPA_NORTE, (4, 3))
    rampa_oeste = abrir(RAMPA_OESTE, (4, 4))
    rampa_este = abrir(RAMPA_ESTE, (4, 4))
    # Los cuatro enlaces curvos del perímetro ([`CANTO_CURVO`]): de cada hoja de
    # 2x1 se guarda la celda que trae la curva.
    cantos_kit = {
        forma: celda(abrir(hoja, (2, 1)), col, 0)
        for forma, (hoja, col) in CANTO_CURVO.items()
    }

    # **Verificar antes de recortar.** Las filas de la meseta chica se dan por
    # sabidas más arriba; acá se comprueba que sigan siendo lo que dicen ser.
    arena = dominante(base_v1, (CELL, CELL, 2 * CELL, 3 * CELL))
    roca = dominante(base_v1, (CELL, FILA_CUERPO * CELL, 2 * CELL, (FILA_CUERPO + 1) * CELL))
    if arena != ARENA_ESPERADA:
        raise SystemExit(f"la meseta mide {arena} y se esperaba {ARENA_ESPERADA}: el kit cambió")
    if roca != ROCA_ESPERADA:
        raise SystemExit(f"la pared mide {roca} y se esperaba {ROCA_ESPERADA}: el kit cambió")
    cuerpos_distintos(base_v1, tramo_v1)
    cuerpos_distintos(base_v2, tramo_v2)

    # **QUÉ ES ARENA Y QUÉ ES ROCA.** La referencia de "roca" es el tramo
    # repetible, que es pared pura sin un píxel de arena. Todo color del kit que
    # NO esté ahí es suelo: la meseta, su filo naranja y la falda del pie.
    #
    # Antes se restaba la paleta de las FILAS de pared, y como el filo naranja
    # también aparece ahí quedaba fuera del teñido: el acantilado de pasto salía
    # con el contorno de arena de la playa, que es justo lo que se marcó en las
    # capturas.
    piedra = paleta(tramo_v1) | paleta(tramo_v2)
    suelo = (
        paleta(rampa)
        | set().union(*(paleta(t) for t in cantos_kit.values()))
        | paleta(rampa_oeste)
        | paleta(rampa_este)
        | paleta(base_v1)
        | paleta(base_v2)
    ) - piedra
    print(f"kit: {len(suelo)} tonos de suelo, {len(piedra)} de roca")

    pagina = Hoja()
    sprites = {}
    objetos = {}
    # Los cinco materiales de siempre más una variante por clima de pasto: el
    # clima sólo cambia el COLOR del suelo de la pieza, así que se cuela en la
    # misma tabla con la base con musgo (ver [`PASTOS_DE_CLIMA`]).
    todos = list(MATERIALES) + [(n, None, None, True) for n, _ in PASTOS_DE_CLIMA]
    for material, rampa_suelo, rampa_roca, musgo in todos:
        # **El color del suelo se MIDE de la losa, no se elige a ojo**, y se ancla
        # en el tono dominante de la meseta para que la parte plana de la pieza
        # sea indistinguible del pasto que tiene pegado ([`mapa_por_losa`]).
        losa = color_de_la_losa(LOSA_DEL_MATERIAL.get(material, ""))
        mapa = {}
        # El mapa del SUELO va aparte además de dentro de `mapa`: es el que se
        # le pasa a la falda de arena, que tiene que seguir al terreno y no a la
        # roca (ver [`FALDA_DE_ARENA`]).
        if losa:
            mapa_suelo = mapa_por_losa(suelo, ARENA_ESPERADA, losa)
        elif rampa_suelo:
            mapa_suelo = mapa_de_tinte(suelo, rampa_suelo)
        else:
            mapa_suelo = {}
        mapa.update(mapa_suelo)
        # **La arena se queda con su falda.** Es el kit ORIGINAL: ahí la orilla
        # clara no es una línea fuera de lugar, es el médano derramándose sobre
        # la roca, y es lo único que el usuario pidió no tocar.
        if material == "arena":
            mapa_suelo = {}
        if rampa_roca:
            mapa.update(mapa_de_tinte(piedra, rampa_roca))
        elif material != "arena":
            # La orilla de playa que no toca el suelo se va a la roca ([`falda_a_roca`]).
            # Va ANTES del contagio: `tenir` compara contra el color CRUDO, así que
            # la falda pegada al terreno igual termina del color del terreno.
            mapa.update(falda_a_roca(piedra))
        b = tenir(base_v2 if musgo else base_v1, mapa, mapa_suelo)
        # El tramo repetible del MISMO kit que la base: si la base lleva musgo, el
        # cuerpo de variante también, o el muro alternaría piedra pelada con
        # piedra con musgo fila por medio.
        tramo_t = tenir(tramo_v2 if musgo else tramo_v1, mapa, mapa_suelo)
        l = {k: tenir(v, mapa, mapa_suelo) for k, v in lados.items()}
        r = tenir(rampa, mapa, mapa_suelo)
        # **LA ESCALINATA NO SE TIÑE DE PASTO.** Es el mismo criterio que ya
        # valía para lo que la ciudad le cuelga a la pared —*"una escalera de
        # madera es de madera en el prado y en el médano"*— y hacía falta acá por
        # una razón que se ve en la captura: los peldaños del pack están dibujados
        # en los tonos de la MESETA, así que teñirlos con `mapa_suelo` los pasaba
        # a verde sobre verde y el paso quedaba en una franja lisa que no se lee
        # como nada. Es lo que el usuario tachó: *"odio cómo usas las rampas"*.
        #
        # Sin el tinte de suelo el peldaño conserva la tierra del recorte y el
        # paso se lee de un vistazo: un camino cavado en la ladera. La roca SÍ se
        # tiñe (`mapa`), que es lo que lo mantiene del color de su propio muro.
        r_escalon = tenir(rampa, mapa)
        rn = tenir(rampa_norte, mapa, mapa_suelo)
        ro = tenir(rampa_oeste, mapa, mapa_suelo)
        re_ = tenir(rampa_este, mapa, mapa_suelo)
        ck = {k: tenir(v, mapa, mapa_suelo) for k, v in cantos_kit.items()}
        for familia, alto in (("cliff", ALTO), ("ledge", ALTO_BORDILLO)):
            mejillas, _ = piezas_de_rampa(b, r, rn, ro, re_, alto)
            poses = {}
            for forma in sorted(COLUMNA):
                # El alto se lee del RECORTE, no de una tabla paralela: cada
                # familia de forma mide lo suyo —la que cae al sur cuelga sus tres
                # celdas, la de canto mide una— y tener el número escrito dos
                # veces terminó como termina siempre. Declararlas de tres cuando
                # ya medían una hacía que el sprite leyera dos filas de más de la
                # hoja y la pared saliera con la pieza de al lado pegada abajo.
                tile = pieza(b, l, ck, forma, alto)
                col, row = pagina.poner(tile)
                poses[forma] = {
                    "rect": {"col": col, "row": row, "w": 1, "h": tile.height // CELL}
                }
            if familia == "cliff":
                # **LAS TRES FILAS SUELTAS: lo que se apila cuando el desnivel es
                # de más de un escalón.**
                #
                # Hoy no se apila nunca —medido sobre la Tierra, las 7.832 caras
                # bajan exactamente UN escalón, ninguna dos, porque el campo de
                # altura no cambia lo suficiente en un tile— pero el alto dejó de
                # estar escrito a mano: `world2d::spawn_chunk_tile_art` cuenta el
                # desnivel real y mete una fila de `cuerpo` por cada escalón de
                # más. Si algún día el relieve apila, la pared apila con él.
                for nombre, fila in (("labio", FILA_LABIO), ("cuerpo", FILA_CUERPO), ("pie", FILA_PIE)):
                    col, row = pagina.poner(celda(b, 1, fila))
                    poses[nombre] = {"rect": {"col": col, "row": row, "w": 1, "h": 1}}
                # **Y LOS CUERPOS DE VARIANTE, que son lo que evita el gofre.**
                #
                # Un farallón de diez pisos apila nueve filas de `cuerpo`. Con una
                # sola celda eso es la MISMA baldosa repetida 9 × el ancho de la
                # corrida, y a esa escala el ojo ya no ve cantos rodados: ve una
                # cuadrícula. El pack tiene con qué —las tres celdas de [`TRAMO`],
                # que es la fila que el propio kit dibujó para repetirse— así que
                # el muro reparte entre las cuatro por celda
                # (`world2d::pared_apilada`).
                #
                # Se numeran desde el 2 porque `cuerpo` a secas sigue siendo la
                # del kit compuesto: quien no sepa de variantes dibuja lo de antes.
                for i in range(tramo_t.width // CELL):
                    col, row = pagina.poner(celda(tramo_t, i, 0))
                    poses[f"cuerpo_{i + 2}"] = {"rect": {"col": col, "row": row, "w": 1, "h": 1}}
                # Las mejillas viven en `cliff/*` porque son PARED: la celda de
                # acantilado pegada al paso, no el paso.
                for nombre, tile in mejillas.items():
                    col, row = pagina.poner(tile)
                    poses[nombre] = {"rect": {"col": col, "row": row, "w": 1, "h": tile.height // CELL}}
            slot = f"{familia}/{material}"
            sprites[slot] = {"png": PAGINA_REL, "grid": [CELL, CELL], "poses": poses}
            objetos[slot] = f"{ETIQUETAS[familia]} {NOMBRE[material]}"
            print(f"{slot}: {len(poses)} poses")

        # La escalinata: sólo a la altura del acantilado (un bordillo se trepa,
        # no necesita paso). Va con la rampa SIN teñir de suelo — ver `r_escalon`.
        _, escalones = piezas_de_rampa(b, r_escalon, rn, ro, re_, ALTO)
        poses = {}
        for forma, tile in sorted(escalones.items()):
            col, row = pagina.poner(tile)
            poses[forma] = {"rect": {"col": col, "row": row, "w": 1, "h": tile.height // CELL}}
        slot = f"slope/{material}"
        sprites[slot] = {"png": PAGINA_REL, "grid": [CELL, CELL], "poses": poses}
        objetos[slot] = f"{ETIQUETAS['slope']} {NOMBRE[material]}"
        print(f"{slot}: {len(poses)} poses")

    # Lo que le cuelga la ciudad a la pared. No se tiñe: una escalera de madera
    # es de madera en el prado y en el médano, igual que la roca.
    for slot, etiqueta, piezas in URBANO:
        for i, (nombre, filas) in enumerate(piezas):
            entera = recorte_de_abajo(abrir_libre(nombre), filas)
            if entera.width != CELL:
                raise SystemExit(f"{nombre} mide {entera.width // CELL} celdas de ancho, se esperaba 1")
            # La pose se llama `alta` y no `center` porque el juego la pide por
            # nombre: es LA pieza, la que se apoya en el pie de la pared y asoma
            # lo que le sobre (`world2d::spawn_chunk_cliff_urbano`).
            col, row = pagina.poner(entera)
            clave = slot if i == 0 else f"{slot}#{i + 1}"
            sprites[clave] = {
                "png": PAGINA_REL,
                "grid": [CELL, CELL],
                "poses": {"alta": {"rect": {"col": col, "row": row, "w": 1, "h": entera.height // CELL}}},
            }
            objetos[clave] = etiqueta
        print(f"{slot}: {len(piezas)} variantes")

    if seco:
        print(f"\n[dry-run] hoja {pagina.im.size[0]}x{pagina.im.size[1]} px, {len(sprites)} claves")
        return

    (ASSETS / "art/pack").mkdir(parents=True, exist_ok=True)
    pagina.im.save(ASSETS / PAGINA_REL, optimize=True)
    print(f"\n{PAGINA_REL}: {pagina.im.size[0]}x{pagina.im.size[1]} px")

    man = json.loads(MANIFEST.read_text())
    hoja = man.setdefault("sprites", {})
    if INDEX.exists():
        for k in json.loads(INDEX.read_text()).get("sprites", []):
            hoja.pop(k, None)
    hoja.update(sprites)
    objs = man.setdefault("objects", {})
    for slot, etiqueta in objetos.items():
        objs.setdefault(slot, {})["label"] = etiqueta
    MANIFEST.write_text(json.dumps(man, indent=1, ensure_ascii=False, sort_keys=True))
    INDEX.write_text(json.dumps({"sprites": sorted(sprites), "pagina": PAGINA_REL}, indent=1, ensure_ascii=False))
    print(f"manifest: {len(sprites)} claves de relieve")


if __name__ == "__main__":
    main()
