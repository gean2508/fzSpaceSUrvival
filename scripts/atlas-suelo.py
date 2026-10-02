#!/usr/bin/env python3
"""**El suelo del juego, con el pixel del pack** — una sola hoja para todo.

    python3 scripts/atlas-suelo.py --dry-run   # informa qué losa le tocó a cada tile
    python3 scripts/atlas-suelo.py             # arma la hoja y reescribe el manifest

## Por qué

El suelo del juego se dibuja con la malla del chunk (`world2d::build_chunk_mesh`),
que hasta ahora era **color plano por vértice, sin textura**. Y el camino que sí
dibuja arte de tile (`spawn_chunk_tile_art`) pide `tile_has_art`, que exige poses
o animación: `floor/asfalto` y `floor/vereda` tienen su recorte pero ninguna pose,
así que **nunca se dibujaban**. De ahí que la calle se viera como una banda gris
con la línea de carril flotando encima: la línea es lo único con poses.

Este script arma `art/pack/suelo.png`, una hoja con **una celda por material de
suelo**, y reescribe esas entradas del manifest para que apunten ahí. La malla del
chunk pasa a llevar UVs contra esa hoja: **un solo draw call por chunk, igual que
antes**, pero con el pixel comprado en vez de un color.

## La celda 0,0 es BLANCA a propósito

`ColorMaterial` multiplica textura × color de vértice. La celda blanca deja el
color de vértice intacto, así que **todo lo que no tenga losa asignada se sigue
viendo exactamente como hoy**. Es lo que permite texturizar el suelo sin tocar el
aspecto de lo que todavía no tiene arte.

## Cómo elige la losa: por COLOR, no por nombre

Cada tile del juego declara su color en `ha-shared::topdown::Tile::color()` — ése
es el aspecto que se decidió que tenga. Así que la losa correcta del pack es **la
que más se le parece**: se recorren los singles de los temas de terreno, se
descartan los que no son losa repetible (no opacos enteros, o con dibujo: una
tapa de alcantarilla no es asfalto), y de las que quedan gana la de color medio
más cercano. El nombre del pack no entra en la cuenta — el importador masivo
bautiza automático y erra seguido.
"""

import argparse
import json
from collections import Counter
import os
from glob import glob

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACK = os.path.join(ROOT, "PRUEBAS-CODIGOS")
EXT = os.path.join(PACK, "modernexteriors-win/Modern_Exteriors_32x32/ME_Theme_Sorter_32x32")
ASSETS = os.path.join(ROOT, "native/crates/ha-game/assets")
MANIFEST = os.path.join(ASSETS, "art/manifest.json")
PAGINA_REL = "art/pack/suelo.png"
INDEX = os.path.join(ROOT, "scripts/suelo-import-index.json")
CELL = 32
#: **Geometría FIJA de la hoja.** La malla del chunk normaliza las UVs con estos
#: dos números (`world2d::SUELO_PAGINA_PX`), y no puede leer el tamaño del PNG:
#: la textura la carga Bevy de forma asíncrona y la malla se arma antes. Fijarla
#: acá y compartirla como constante es lo que mantiene a los dos lados de
#: acuerdo — lo cuida el test `la_hoja_del_suelo_mide_lo_que_dice`.
COLS, FILAS = 32, 8
PAGINA_PX = (COLS * CELL, FILAS * CELL)

#: Temas de donde salen las losas de terreno. Son los que el pack dibuja como
#: SUELO; el resto de los temas son objetos.
TEMAS_TERRENO = [
    "1_Terrains_and_Fences_Singles_32x32",
    "2_City_Terrains_Singles_32x32",
    "21_Beach_Singles_32x32",
    "17_Garden_Singles_32x32",
]

#: Hojas de SUELO que no vienen partidas en singles y hay que recorrer celda a
#: celda. La de Modern Interiors es la que trae los pisos de adentro —madera,
#: baldosa, piedra—, que son justo los que el jugador construye.
HOJAS_SUELO = [
    os.path.join(
        PACK,
        "moderninteriors-win (Copy 1)/1_Interiors/32x32/Room_Bulder_subfiles_32x32",
        "Room_Builder_Floors_32x32.png",
    ),
]

#: Color declarado de cada tile en `Tile::color()` (ha-shared/src/topdown.rs).
#: **Es la referencia contra la que se elige la losa**: el color es la decisión de
#: aspecto que ya estaba tomada, la losa sólo la cumple con textura.
#: Si cambia allá, cambia acá — lo cuida `el_suelo_declara_su_color` en Rust.
#:
#: **Sólo materiales de la TIERRA.** El pack es urbano: no tiene regolito lunar,
#: ni basalto de Ío, ni hielo de Europa. Por color, la vereda "calza" con el
#: regolito y el asfalto con el basalto — y eso pondría una acera de ciudad en la
#: Luna. Los cuerpos que el pack no cubre se quedan con su color plano, que es lo
#: que ya tenían y no miente.
#:
#: El segundo campo es **de dónde puede salir** la losa: `"ext"` = terreno de
#: exteriores, `"int"` = pisos de Room_Builder. Hace falta porque el color solo no
#: alcanza: la tierra y la madera son el MISMO marrón, y sin esta atadura el
#: suelo del campo salía con parqué de interior y el piso de madera con un
#: montículo de tierra (pasó en la primera corrida). El material lo decide de
#: dónde viene la losa; el color sólo elige cuál dentro de esa familia.
#:
#: El tercer campo, opcional, es la **FAMILIA del pack**: un trozo del nombre de
#: archivo que la losa tiene que llevar. Hace falta donde el color no distingue lo
#: que el ojo sí: la vereda de una CIUDAD y el sendero de un PARQUE son el mismo
#: gris, así que por color ganaban el adoquinado del jardín y dos losas sueltas de
#: `17_Garden` — y las veredas de todas las ciudades quedaron con piso de parque,
#: sin cordón ni forma de saber dónde termina la calle. Con la familia atada, la
#: vereda sale de `Sidewalk` y el asfalto de `Asphalt`, que son las piezas que el
#: pack dibujó PARA una calle y las únicas que empalman con `road/cordon`.
#:
#: El cuarto campo, opcional, corre la losa elegida hacia otro color: es lo que
#: da un pasto por clima con un pack que dibujó uno solo. Ver [`derivar`].

#: **Los colores que el PACK declara.** No son inventados: son las paletas planas
#: que LimeZu dejó en sus propias hojas (`Grass_3_21` es el khaki de su tierra,
#: `Grass_4_22` el naranja, `Grass_Water_1_22` el celeste de su agua baja). Sirven
#: de destino cuando hay que correr una losa hacia otro clima — ver [`derivar`].
PACK_KHAKI = (170.0, 143.0, 90.0)
PACK_AGUA = (60.0, 163.0, 178.0)
PACK_SETO = (29.4, 100.3, 75.4)

#: **La nieve, que el pack NO dibujó.** No sale de una hoja de LimeZu porque no
#: hay ninguna: buscado, lo único blanco del pack son props de heladería y de
#: navidad. Sale de la rampa con la que `import-acantilado.py::MATERIALES` ya tiñe
#: `cliff/nieve` / `slope/nieve` / `ledge/nieve` — o sea del arte que YA está en
#: el juego—, tomando sus dos tonos de en medio (196,212,230) y (228,238,248).
#:
#: Que el suelo y la pared salgan del mismo sitio es todo el punto: la meseta del
#: acantilado nevado se cura con `LOSA_DEL_MATERIAL["nieve"] = "tiles/ice"`, así
#: que la cumbre y su cara son literalmente el mismo blanco. Y coincide con el
#: color declarado de `Tile::Ice` (0.82, 0.88, 0.93 → 209,224,237), que es lo que
#: comprueba `el_suelo_declara_su_color`.
NIEVE_DEL_KIT = (212.0, 225.0, 239.0)

#: **El regolito y el basalto, por la misma puerta que la nieve.** Los dos tonos
#: de en medio de sus rampas en `import-acantilado.py::MATERIALES` — o sea del
#: arte que YA está en el juego, no de un color elegido acá:
#:
#: - regolito: (126,121,114) y (160,155,147) → (143,138,131)
#: - basalto:  (72,64,76) y (104,94,108)     → (88,79,92)
REGOLITO_DEL_KIT = (143.0, 138.0, 131.0)
BASALTO_DEL_KIT = (88.0, 79.0, 92.0)

#: **La arena DEL MUNDO.** No es un color elegido: es el amarillo que LimeZu usa
#: para la playa, medido en `Sea_Water_Tileset` (la orilla de `tiles/water_arena`)
#: y en la meseta del kit de acantilado (`import-acantilado.py::ARENA_ESPERADA`) —
#: los dos dan (230,174,85). Es lo que hace que la playa, su orilla y el
#: acantilado de arena sean el mismo amarillo y no tres.
ARENA_DEL_PACK = (230.0, 174.0, 85.0)

#: La tierra del pack: el relleno khaki de `Grass_3`, que es la familia de la que
#: sale la ORLA (`scripts/import-suelo-bordes.py`). Relleno y costura del mismo
#: tono es lo único que hace que el borde no se vea pegado.
TIERRA_DEL_PACK = (170.0, 143.0, 90.0)

COLOR_DEL_TILE = {
    # **EL PASTO, UNO POR BIOMA.** El pack trae varias familias de pasto
    # (`Grass_1`, `Grass_3`, `Grass_4`, `Grass_Water_*`, `Grass_Fenced_1`) y a
    # simple vista parecen cuatro pastos distintos. Medidas, no lo son: el
    # RELLENO de todas es el mismo verde (71,148,86 ±2) y lo que cambia entre
    # familias es la TRANSICIÓN — a tierra khaki la 3, a tierra naranja la 4, a
    # agua las `Water`. O sea que las "versiones" del pasto son piezas de kit
    # (regla 8), y sembrar una de esas transiciones como relleno pinta cuñas
    # verdes en cuadrícula por todo el campo. Por eso el filtro de losa
    # (`CUADRANTES_MAX`, `BORDE_MAX`) las echa, y está bien que las eche.
    #
    # Lo que sí hay son tres juegos de PÍXELES de relleno distintos —`Grass_1`,
    # `Grass_Water_*` y `Grass_Fenced_1` tienen las matas puestas en otro lado—,
    # y el color de cada clima se consigue corriendo esa losa hacia un color que
    # el propio pack declara (ver `derivar` y `PACK_*`). Así la sabana se ve seca
    # y el bosque hondo sin pedirle un pixel nuevo a nadie: el día que le
    # asignes un pasto de verdad a uno de estos slots desde HA Studio, entra ése.
    #
    # Qué bioma usa cuál lo decide el juego en `world2d::pasto_de_bioma`.
    "tiles/grass": ((0.36, 0.62, 0.29), "ext", "Grass_1_"),
    "tiles/pasto_bosque": ((0.36, 0.62, 0.29), "ext", "Grass_Fenced_1_", (PACK_SETO, 0.34)),
    # La sabana va a 0,65 del khaki y no a 1,0 a propósito: a fondo la media
    # aterriza en (156,144,90), que ya es TIERRA, y el mundo volvería al baldío
    # del que se venía (§8, "La superficie viva es PASTO"). A 0,65 queda en
    # (135,146,89) —oro seco con las matas todavía puestas—, que es una sabana.
    "tiles/pasto_seco": ((0.36, 0.62, 0.29), "ext", "Grass_1_", (PACK_KHAKI, 0.65)),
    "tiles/pasto_pantano": ((0.36, 0.62, 0.29), "ext", "Grass_Water_", (PACK_AGUA, 0.20)),
    "tiles/pasto_sierra": ((0.36, 0.62, 0.29), "ext", "Grass_1_", (PACK_KHAKI, 0.35)),
    # **LA NIEVE DE LA CUMBRE.** Entra por la misma puerta que la sabana y el
    # pantano: la losa de pasto trasladada a otro color (ver `derivar`). La regla
    # de más arriba —"sólo materiales de la Tierra, el pack no tiene hielo de
    # Europa"— sigue valiendo y ésta no la rompe: desde que `geography::biome_at`
    # decide la nieve por ESCALÓN (`NIVEL_NIEVE`), `tiles/ice` dejó de ser el
    # suelo de una luna helada y pasó a ser **la cumbre de las montañas de este
    # planeta**, o sea un sitio al que se sube caminando. Y sin losa se veía como
    # lo que era: un rectángulo celeste plano en la cima.
    #
    # Las matas del pasto trasladadas al blanco se leen como los grumos de una
    # nieve pisada, que es justo lo que hace falta para que la cumbre no sea una
    # plancha lisa. A peso 1,0 porque acá no hay nada que conservar del verde.
    # **Y el objetivo de color es el VERDE del pack, no el blanco de la nieve.**
    #
    # Con el blanco declarado, la losa más parecida dentro de `Grass_1` era su
    # relleno de TIERRA (Δ171, porque no hay nada claro en la familia) y sobre
    # todo se quedaba con **cero variantes**: `elegir` corta las variantes contra
    # la losa elegida, y las siete losas con matas están a 99 de la tierra. O sea
    # que la cumbre entera se pintaba con un único cuadrado de un solo color, y
    # eso es lo que se ve en `relieve-3-cumbre.png`: una plancha blanca sin un
    # grumo. Apuntando al verde entra el pasto CON sus siete variantes y el
    # traslado las lleva a todas al blanco de golpe — que es lo que este
    # comentario decía que hacía.
    "tiles/ice": ((0.278, 0.592, 0.341), "ext", "Grass_1_", (NIEVE_DEL_KIT, 1.0)),
    # **LA TIERRA Y LA ARENA: las dos que se quedaban sin losa.**
    #
    # Con el color inventado que declaraban (`Tile::Dirt` era (120,84,56) y
    # `Tile::Sand` (199,178,115)) la búsqueda por parecido no encontraba NADA a
    # menos de `DELTA_MAX`, y las dos se quedaban en color plano — el único suelo
    # del mundo sin pixel del pack. Se veía en la playa: la orilla del agua
    # (`tiles/water_arena`) y el acantilado de arena (`cliff/arena`) traen la
    # arena de LimeZu y el relleno de al lado era otro amarillo.
    #
    # La familia atada es `Grass_3`, que es de donde sale también la ORLA
    # (`scripts/import-suelo-bordes.py`): el relleno y su costura son literalmente
    # el mismo pixel, que es la única forma de que el borde no se vea pegado. De
    # esa familia las únicas losas repetibles son sus dos rellenos —la tierra
    # khaki y el pasto—, así que el objetivo de color sólo tiene que elegir entre
    # esos dos.
    #
    # La ARENA es la misma losa corrida al amarillo del pack ([`ARENA_DEL_PACK`]),
    # igual que la nieve se consigue corriendo el pasto al blanco: las piedritas
    # de la tierra trasladadas al amarillo se leen como el grano del médano.
    # La familia es `Grass_1` por el MISMO motivo que la nieve: es la única con
    # variantes (siete losas con matas), y sin ellas un desierto o un campo
    # cavado es un único cuadrado repetido hasta el horizonte. Ancladas por
    # luminancia (ver [`derivar`]) las matas se vuelven terrones y granos, no
    # pasto de otro color, y la media queda clavada en el tono del pack — el
    # mismo con el que la orilla (`import-suelo-bordes.py`) dibuja su costura.
    "tiles/dirt": ((0.278, 0.592, 0.341), "ext", "Grass_1_", (TIERRA_DEL_PACK, 1.0)),
    "tiles/sand": ((0.278, 0.592, 0.341), "ext", "Grass_1_", (ARENA_DEL_PACK, 1.0)),
    "tiles/stone": ((0.46, 0.47, 0.50), "ext"),
    "tiles/roca": ((0.52, 0.50, 0.47), "ext"),
    # **EL REGOLITO Y EL BASALTO: los dos suelos que quedaban planos.**
    #
    # La regla de arriba —"sólo materiales de la Tierra, el pack no trae regolito
    # lunar ni basalto de Ío"— se escribió antes de que existiera [`derivar`], y
    # con `derivar` deja de valer por el mismo motivo por el que dejó de valer
    # para la nieve: no hace falta que el pack DIBUJE regolito, hace falta que
    # dibuje un grano repetible que se pueda anclar al color del regolito. Lo
    # dibujó.
    #
    # El destino no se elige a ojo ni se inventa: son los dos tonos de en medio de
    # la rampa con la que `import-acantilado.py::MATERIALES` ya tiñe
    # `cliff/regolito` y `cliff/basalto`, exactamente como [`NIEVE_DEL_KIT`]. Que
    # el suelo salga de la misma rampa que la pared es todo el punto: en la Luna,
    # el llano y la cara del cráter son el mismo gris y no dos.
    #
    # Lo que había hasta acá era un plano gris de horizonte a horizonte —el color
    # plano "no miente", pero tampoco se lee como suelo—, y es justo el sitio
    # donde más se nota: la Luna y Mercurio no tienen vegetación que rompa el
    # llano, así que el suelo es TODO lo que hay para mirar.
    #
    # ⚠ **ORDEN**: desde que estas dos claves existen, `import-acantilado.py` deja
    # de tintar `cliff/regolito` y `cliff/basalto` con su rampa escrita a mano y
    # pasa a medir la losa (`LOSA_DEL_MATERIAL` → `color_de_la_losa`). Es lo que
    # se quiere —el llano y la cara del cráter salen del mismo pixel— pero
    # significa que este script va PRIMERO. No hay ciclo: el destino de acá sale
    # de la CONSTANTE de la rampa, no del PNG que produce el otro.
    "tiles/regolith": ((0.278, 0.592, 0.341), "ext", "Grass_1_", (REGOLITO_DEL_KIT, 1.0)),
    "tiles/basalt": ((0.278, 0.592, 0.341), "ext", "Grass_1_", (BASALTO_DEL_KIT, 1.0)),
    # SUBSUELO — los dos materiales que llenan la pantalla cuando estás abajo.
    #
    # El MACIZO va contra los terrenos de exteriores y hoy no encuentra losa: el
    # pack es urbano y no trae tierra oscura repetible, así que se queda en su
    # color plano. Está bien que así sea — un macizo sin dibujo se lee como masa,
    # que es lo que es, y el ruido por celda de `tile_render_color` ya le saca lo
    # liso. El día que entre una losa de tierra, la toma sola.
    #
    # El SUELO DE LA GALERÍA sale de los pisos de Modern Interiors (`int`): es un
    # sitio pisado, no campo abierto, y el pedido era justamente ése — que el
    # cuarto excavado se vea con piso de interiores y el resto sea tierra.
    "tiles/subsuelo": ((0.29, 0.21, 0.14), "ext"),
    "floor/cueva": ((0.42, 0.38, 0.33), "int"),
    # La vereda de ciudad. **La familia tiene que ser UNA y la misma que la del
    # cordón** (`import-calles.py::FAMILIA`): la baldosa la dibuja la malla del
    # chunk y el escalón lo dibuja `road/cordon`, así que si acá entraran las seis
    # familias sorteadas por celda, el cordón crema caería sobre una vereda gris
    # azulada cada tres metros. Ése es el contrato entre los dos scripts.
    "floor/vereda": ((0.83, 0.80, 0.77), "ext", "Sidewalk_1_"),
    # **Y una vereda distinta POR CIUDAD.** El pack dibujó seis familias enteras
    # y el juego usaba una: todas las ciudades del mundo tenían la misma acera.
    # Cada `floor/vereda_N` sale de su familia, y `import-calles.py` publica el
    # cordón `road/cordon_N` de esa misma familia — el juego elige la pareja por
    # el centro del asentamiento (`world2d::vereda_de_la_ciudad`), nunca por
    # celda, o el escalón de una caería sobre la baldosa de otra.
    #
    # El objetivo de color sigue siendo el CREMA de la familia 1 a propósito, y
    # no el color medio de cada familia: dentro de una familia lo que se busca es
    # la BALDOSA, y la baldosa es siempre lo más claro que hay ahí (lo demás es
    # su asfalto, 60 puntos más abajo, que `DELTA_VARIANTE` corta seco).
    "floor/vereda_tipo2": ((0.83, 0.80, 0.77), "ext", "Sidewalk_2_"),
    "floor/vereda_tipo3": ((0.83, 0.80, 0.77), "ext", "Sidewalk_3_"),
    "floor/vereda_tipo4": ((0.83, 0.80, 0.77), "ext", "Sidewalk_4_"),
    "floor/vereda_tipo5": ((0.83, 0.80, 0.77), "ext", "Sidewalk_5_"),
    "floor/vereda_tipo6": ((0.83, 0.80, 0.77), "ext", "Sidewalk_6_"),
    "floor/asfalto": ((0.28, 0.27, 0.29), "ext", "Asphalt"),
    # Los pisos que construye el jugador también son SUELO y viven en esta hoja:
    # son los que se ven al entrar a un cuarto y tienen que embaldosar igual que
    # la calle. Salen de los pisos de Modern Interiors (`HOJAS_SUELO`).
    "floor/wood": ((0.58, 0.44, 0.26), "int"),
    "floor/stone": ((0.54, 0.53, 0.50), "int"),
}

#: Cuántas losas guardar por material. Con varias, el suelo deja de repetir el
#: mismo dibujo cada metro (las elige `variant_key_seeded` por celda).
MAX_VARIANTES = 8
#: Desviación máxima de brillo para que una celda sea LOSA y no dibujo.
LOSA_DESV = 24.0
#: Diferencia máxima de color entre los CUATRO CUADRANTES de la celda.
#:
#: Es la prueba que separa una losa repetible de una pieza de BORDE, y hace falta
#: porque el color medio no alcanza: un pasto con una esquina de tierra promedia
#: casi lo mismo que un pasto entero, y sin esto se colaban las esquinas y las
#: orillas del autotile (se vio en la primera corrida). Una losa de relleno se ve
#: igual en sus cuatro cuartos; una de borde tiene uno distinto.
CUADRANTES_MAX = 26.0
#: Distancia máxima al color declarado del tile para aceptar la losa. Si ninguna
#: se acerca, ese tile se queda en la celda blanca (color plano de siempre): más
#: vale el color de antes que una textura que no es la que se pidió.
DELTA_MAX = 28.0
#: Distancia máxima entre una VARIANTE y la losa elegida. Es más ancha que
#: `DELTA_MAX` a propósito: acá no se pregunta "¿es del color pedido?" sino "¿es
#: el mismo material?", y dos baldosas de la misma vereda se separan bastante más
#: que un punto de gris (las lisas de `Sidewalk_1` van de 211 a 214 de brillo, y
#: el pasto del pack se abre 45 entre la mata seca y la húmeda). Sesenta deja
#: pasar esa familia entera y corta seco contra el asfalto, que está a 211.
DELTA_VARIANTE = 60.0
#: Asimetría máxima de sombra entre los cuatro bordes de la celda (ver `medir`).
#: Doce sale de la medición: las losas de relleno del pack quedan entre 0 y 10
#: (el asfalto liso da 0,4; el pasto, 0,0; el cuarto limpio de la vereda, 1,7) y
#: las de borde saltan a más de 30 (la esquina `Sidewalk_1_9` da 32). Es el corte
#: que separaba las dos poblaciones sin tocar ninguna.
BORDE_MAX = 12.0

#: **Cuántos píxeles de OTRO material tolera un relleno.** Ver [`contamina`].
#: Seis sobre 1.024 es 0,6 % de la celda: alcanza para el pixel suelto que un
#: dibujo comparte con otra paleta y no para una costura, que aporta docenas.
CONTAMINA_MAX = 6
#: A partir de esta distancia **de croma** dos colores son materiales distintos.
#: Sale de medir, no de la intuición — `python3 scripts/atlas-suelo.py --calibrar`
#: imprime las dos poblaciones sobre las 371 losas del pack:
#:
#: - el tono más oscuro de la mata de `Grass_Fenced_1_9` (el único pasto con
#:   dibujo que hay) se separa **0,23** de su propio relleno;
#: - la lengüeta de tierra de una esquina interna de `Grass_1` se separa **0,91**;
#: - y de 0,45 para arriba **no queda ni una losa de un solo material**: son
#:   costuras de autotile (`Grass_Water_*`, `Grass_Fenced_*`), muros de seto
#:   (`Grass_Wall_*`), la cumbre de arena de la playa y `Mound_2_6`.
AJENO_MIN = 0.45


def croma(c):
    """El color **sin su brillo**: (r,g,b) dividido por su luminancia.

    Es lo que identifica un MATERIAL. En RGB pelado la sombra de una roca
    (93,87,86) queda a 68 del verde del pasto y a 87 de su propio gris claro
    (148,142,125), así que `contamina` daba a la roca por contaminada de pasto y
    `tiles/roca` volvía al color plano. La sombra de una roca no es otro
    material: es la misma roca con menos luz. Dividiendo por la luminancia el
    brillo se va y queda el tono, que es lo que se quiere comparar.

    Es el mismo criterio con el que [`derivar`] traslada una losa a otro clima.
    """
    y = max(0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2], 1.0)
    return (c[0] / y, c[1] / y, c[2] / y)


def rellenos_del_pack(losas):
    """**Los materiales que el pack declara**, medidos: las celdas de UN color.

    LimeZu pinta el interior de cada terreno con un color plano y sólo dibuja en
    la COSTURA. Así que las celdas monocromas del pack son, literalmente, su
    lista de materiales: el verde del pasto (71,151,87), el khaki de la tierra
    (170,143,90), el naranja de la otra tierra (199,140,89), el celeste del agua
    (60,163,178). Es contra esa lista que [`contamina`] decide si una losa es
    relleno o costura — no contra un umbral escrito a ojo.
    """
    return [l["rgb"] for l in losas if len(set(l["im"].getdata())) == 1]


def contamina(im, rellenos):
    """**¿Esta losa trae píxeles de OTRO material?** Entonces es COSTURA.

    Es la regla 8 del proyecto (¿pieza o objeto?) aplicada al suelo, y hace falta
    porque ninguno de los filtros de arriba la caza. `medir` echa las piezas de
    BORDE —donde media celda es del otro material— por cuadrantes y por sombra,
    pero **una esquina interna del autotile es 98 % pasto**: su color medio, su
    desviación, sus cuatro cuadrantes y sus cuatro bordes dan lo mismo que el
    relleno. Pasa limpia por los tres.

    Eso fue exactamente lo que pasó: las siete "variantes" de `tiles/grass` eran
    las siete esquinas internas de `Grass_1` (`_9`…`_13`, `_17`, `_20`), cada una
    con su lengüeta de tierra naranja en un rincón, y de ahí se propagaron por
    `derivar` a la tierra, la arena, la nieve, el regolito, el basalto y los
    cuatro pastos de bioma — diez materiales sembrando confeti naranja por celda.

    La pregunta correcta no es "¿cuánto varía?" sino **"¿de qué está hecha?"**: se
    mira píxel por píxel contra la lista de rellenos que el pack declara
    ([`rellenos_del_pack`]) y si hay más de `CONTAMINA_MAX` que se parecen más a
    OTRO material que al propio, la losa es una costura y no entra.

    Lo que SÍ sigue pasando: `Grass_Fenced_1_9`, el único pasto con matas del
    pack. Sus dos verdes oscuros (55,133,78) y (45,114,86) están a 22 y 45 de su
    propio relleno y a más de 100 de cualquier otro — son el mismo material.

    **La referencia es el color DOMINANTE, no el medio**, y se compara por
    [`croma`], no por RGB. Las dos cosas hicieron falta y las dos se vieron
    fallar: con el medio, los brillos de la roca (`Others_4`) quedaban más cerca
    del relleno crema del pack que del gris PROMEDIO de la losa; con RGB pelado,
    su sombra quedaba más cerca del VERDE DEL PASTO que de su propio gris. Las
    dos veces el resultado era el mismo, `tiles/roca` sin losa y de vuelta al
    color plano. El dominante es el color del que la losa está hecha, y el croma
    dice de qué material es cada píxel sin que el sombreado se meta en el medio.
    """
    dominante = croma(Counter(im.getdata()).most_common(1)[0][0][:3])
    ajenos = [croma(r) for r in rellenos if dist(croma(r), dominante) > AJENO_MIN]
    if not ajenos:
        return False
    n = 0
    for p in im.getdata():
        c = croma(p[:3])
        d = dist(c, dominante)
        if d > AJENO_MIN and any(dist(c, a) < d for a in ajenos):
            n += 1
            if n > CONTAMINA_MAX:
                return True
    return False


def medir(im):
    px = im.load()
    w, h = im.size
    vals = []
    for y in range(h):
        for x in range(w):
            p = px[x, y]
            if p[3] < 255:
                return None  # una losa de suelo cubre su celda entera
            vals.append(p[:3])
    n = len(vals)
    medio = tuple(sum(v[i] for v in vals) / n for i in range(3))
    brillo = [sum(v) / 3 for v in vals]
    m = sum(brillo) / n
    desv = (sum((b - m) ** 2 for b in brillo) / n) ** 0.5
    # Color medio de cada cuadrante, para descartar las piezas de borde.
    cuad = []
    for qy in (0, h // 2):
        for qx in (0, w // 2):
            vs = [px[x, y][:3] for y in range(qy, qy + h // 2) for x in range(qx, qx + w // 2)]
            k = len(vs)
            cuad.append(tuple(sum(v[i] for v in vs) / k for i in range(3)))
    peor = max(
        sum((a[i] - b[i]) ** 2 for i in range(3)) ** 0.5 for a in cuad for b in cuad
    )
    # **¿Trae la sombra del borde pintada?** Una losa de RELLENO se ve igual por
    # los cuatro lados; una losa de BORDE lleva la sombra de contacto pintada en
    # el lado por el que termina. Se compara cada franja de borde contra el centro
    # y lo que decide es la ASIMETRÍA (el lado más oscuro contra el más claro), no
    # cuánto se oscurece: el asfalto liso del pack baja 33 por sus cuatro lados a
    # la vez —eso es un viñeteado y embaldosa perfecto— mientras que la vereda
    # `Sidewalk_1_9` baja 33 abajo y 1 arriba, que es una esquina con sombra.
    k = max(2, min(w, h) // 8)
    centro = [brillo[y * w + x] for y in range(h // 4, 3 * h // 4) for x in range(w // 4, 3 * w // 4)]
    c = sum(centro) / len(centro)
    franjas = (
        [(x, y) for y in range(k) for x in range(w)],
        [(x, y) for y in range(h - k, h) for x in range(w)],
        [(x, y) for x in range(k) for y in range(h)],
        [(x, y) for x in range(w - k, w) for y in range(h)],
    )
    caidas = [c - sum(brillo[y * w + x] for x, y in f) / len(f) for f in franjas]
    return medio, desv, peor, max(caidas) - min(caidas)


def losas_del_pack():
    """Todas las losas repetibles del pack, con su color medio.

    Dos fuentes: los singles de los temas de terreno (exteriores) y las hojas de
    piso que no vienen partidas (`HOJAS_SUELO`), que se recorren celda a celda.
    """
    out = []
    vistas = {}

    def sumar(im, origen, fuente):
        m = medir(im)
        if m is None or m[1] > LOSA_DESV or m[2] > CUADRANTES_MAX or m[3] > BORDE_MAX:
            return
        h = im.tobytes()
        if h in vistas:
            # El pack repite la misma losa en varios temas: la baldosa se guarda
            # UNA vez, pero se queda con TODOS sus nombres. Sin esto la repetida
            # perdía el nombre de la familia que la reclamaba y el filtro de
            # familia se quedaba sin candidatas: las cuatro baldosas lisas de
            # `Sidewalk_1` (la 25 a la 28) salían deduplicadas contra otro tema,
            # así que a la vereda le quedaba UNA sola losa suya y el resto de sus
            # "variantes" eran el ASFALTO de la misma familia. Tres de cada cuatro
            # celdas de acera se dibujaban con el gris de la calzada — la vereda
            # no se distinguía de la calle en ninguna ciudad del mundo.
            vistas[h]["files"].append(origen)
            return
        losa = {"im": im, "rgb": m[0], "desv": m[1], "files": [origen], "fuente": fuente}
        vistas[h] = losa
        out.append(losa)

    # **Un single grande también trae losa adentro.** Esto sólo miraba los de
    # 32×32 y por eso la vereda se quedaba con la peor baldosa de su familia: la
    # única lisa de 32×32 (`Sidewalk_1_9`) es la ESQUINA de una vereda y trae
    # pintada la sombra del cordón en su borde de abajo y de la derecha. Repetida
    # celda a celda, esa sombra dibujaba una cuadrícula oscura sobre toda la
    # acera. Las losas limpias de esa vereda están dentro de los bloques de 2×2
    # (`Sidewalk_1_25`…`_28`): son el mismo pixel sin la sombra del borde.
    #
    # Así que un single de N×M celdas entra celda a celda, con el nombre del
    # archivo. Los cuartos con sombra no hay que echarlos a mano: `medir` los
    # castiga solo (su desviación de brillo triplica la del cuarto limpio) y el
    # orden de `elegir` los deja debajo.
    for tema in TEMAS_TERRENO:
        for f in sorted(glob(os.path.join(EXT, tema, "*.png"))):
            im = Image.open(f).convert("RGBA")
            w, h = im.size
            # Hasta 2×2 celdas y ni una más. El pack guarda en estas carpetas
            # tanto losas como ESCENAS de ejemplo —un bar de playa de 7×5, un
            # palacio de 25×34—, y recortarles celdas mete trozos de dibujo donde
            # tiene que ir suelo repetible: en la primera prueba la tierra salió
            # de un pedazo de barco y la piedra de una fachada de palacio. Lo que
            # hace falta acá cabe de sobra en 2×2 (los bloques lisos de vereda).
            if w % CELL or h % CELL or w > 2 * CELL or h > 2 * CELL:
                continue
            nombre = os.path.basename(f)
            for row in range(h // CELL):
                for col in range(w // CELL):
                    celda = im.crop((col * CELL, row * CELL, (col + 1) * CELL, (row + 1) * CELL))
                    sumar(celda, nombre, "ext")

    for ruta in HOJAS_SUELO:
        if not os.path.exists(ruta):
            raise SystemExit(f"falta la hoja de pisos: {ruta}")
        hoja = Image.open(ruta).convert("RGBA")
        nombre = os.path.basename(ruta)
        for row in range(hoja.size[1] // CELL):
            for col in range(hoja.size[0] // CELL):
                celda = hoja.crop((col * CELL, row * CELL, (col + 1) * CELL, (row + 1) * CELL))
                sumar(celda, f"{nombre}[{col},{row}]", "int")
    return out


def dist(a, b):
    return sum((a[i] - b[i]) ** 2 for i in range(3)) ** 0.5


def mod_medio(im):
    """Color medio de una losa. Sólo para informar por consola qué salió."""
    px = im.load()
    w, h = im.size
    return [sum(px[x, y][i] for y in range(h) for x in range(w)) / (w * h) for i in range(3)]


def derivar(im, destino, peso):
    """Corre una losa del pack HACIA otro color **sin aplanarle la textura**.

    Es la forma de tener pasto de sabana y pasto de bosque con un pack que sólo
    dibujó UN verde (ver el comentario del pasto en [`COLOR_DEL_TILE`]). La
    alternativa —mezclar cada pixel contra el destino— apaga justo lo que hace
    que el pasto se lea como pasto: a 0,8 de mezcla las matas desaparecen y queda
    un rectángulo de color liso, que es exactamente el baldío del que se venía.

    Así que no se mezcla: se TRASLADA. Se mide el color medio de la losa, se
    calcula el desvío hasta el destino y ese mismo desvío se le suma a todos sus
    píxeles. La media termina donde se pidió y el relieve interno —la mata más
    oscura, el claro más brillante— llega intacto al otro clima: la mata verde de
    la pradera es la mata seca de la sabana, no una mancha.

    `peso` es cuánto del camino se recorre: 1,0 deja la media clavada en
    `destino`, 0,2 apenas insinúa el clima.

    **A peso 1,0 no se traslada: se ANCLA por luminancia.** El traslado suma el
    mismo desvío a todos los píxeles, y eso es lo correcto para correr un verde
    hacia otro verde —la mata sigue siendo una mata más oscura— pero se rompe
    cuando el destino está en la otra punta del espacio de color. Medido en la
    nieve: el desvío del pasto al blanco es (+141,+74,+152), y aplicado al tono
    más oscuro de la mata (52,85,93) da **(193,159,245): violeta**. En pantalla
    eso fue la cumbre salpicada de motas rosas (`relieve-3-cumbre.png`, 14 ago
    2026).

    Anclando por luminancia cada píxel conserva su BRILLO relativo al medio y
    toma el tono del destino, así que la mata sigue siendo un grumo más oscuro —
    de nieve, no de otro color. Es el mismo criterio que
    `import-acantilado.py::mapa_por_losa`, y por eso el suelo y la pared de un
    mismo material salen del mismo sitio.
    """
    px = im.load()
    w, h = im.size
    n = w * h
    medio = [sum(px[x, y][i] for y in range(h) for x in range(w)) / n for i in range(3)]
    if peso >= 0.99:
        base = max(0.2126 * medio[0] + 0.7152 * medio[1] + 0.0722 * medio[2], 1.0)
        out = Image.new("RGBA", (w, h), (0, 0, 0, 0))
        dst = out.load()
        for y in range(h):
            for x in range(w):
                p = px[x, y]
                f = (0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]) / base
                dst[x, y] = tuple(max(0, min(255, round(destino[i] * f))) for i in range(3)) + (p[3],)
        return out
    off = [(destino[i] - medio[i]) * peso for i in range(3)]
    out = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    dst = out.load()
    for y in range(h):
        for x in range(w):
            p = px[x, y]
            dst[x, y] = tuple(max(0, min(255, round(p[i] + off[i]))) for i in range(3)) + (p[3],)
    return out


def elegir(losas, objetivo255, fuente, familia=None, rellenos=()):
    """Las `MAX_VARIANTES` losas de esa FUENTE (y familia) más parecidas al color.

    **Y todas del MISMO material que la primera.** Las variantes están para que
    el suelo no repita el mismo dibujo cada metro, no para mezclar materiales: si
    la segunda no se parece a la primera, cada celda cambia de piso y lo que se
    ve no es variedad, es un damero. Por eso el corte final no es contra el color
    declarado sino contra la losa ELEGIDA — ver [`DELTA_VARIANTE`].
    """
    cand = [l for l in losas if l["fuente"] == fuente]
    if familia:
        cand = [l for l in cand if any(familia in f for f in l["files"])]
    # **Y ninguna que esté hecha de otro material.** Ver [`contamina`]: es lo que
    # separa el relleno de la costura, y sin esto entraban las esquinas internas
    # del autotile como si fueran variantes.
    cand = [l for l in cand if not contamina(l["im"], rellenos)]
    # A igualdad de color gana la MÁS PAREJA: un piso a rayas gruesas tiene el
    # color justo y se lee como una alfombra de circo cuando lo repetís cien
    # veces. La desviación entra con medio peso — desempata, no manda.
    cand.sort(key=lambda l: dist(l["rgb"], objetivo255) + 0.5 * l["desv"])
    cand = [l for l in cand if dist(l["rgb"], cand[0]["rgb"]) <= DELTA_VARIANTE] if cand else []
    return cand[:MAX_VARIANTES]


def calibrar(losas, rellenos):
    """**De dónde sale [`AJENO_MIN`]** — las dos poblaciones, medidas.

    Mide lo mismo que decide [`contamina`]: para cada losa, cuánto se aparta del
    croma de su color dominante el peor de los píxeles **que además se parecen
    más a OTRO relleno del pack**. Un relleno de verdad no tiene ninguno (su
    sombreado es de su propio material); una costura del autotile los tiene y
    saltan lejos. El corte va en el hueco entre las dos poblaciones.

        python3 scripts/atlas-suelo.py --calibrar
    """
    filas = []
    for l in losas:
        dom = croma(Counter(l["im"].getdata()).most_common(1)[0][0][:3])
        ajenos = [croma(r) for r in rellenos if dist(croma(r), dom) > 0.05]
        peor, cuantos = 0.0, 0
        for p in l["im"].getdata():
            c = croma(p[:3])
            d = dist(c, dom)
            if ajenos and any(dist(c, a) < d for a in ajenos):
                peor = max(peor, d)
                cuantos += 1
        filas.append((peor, cuantos, min(l["files"], key=len)))
    filas.sort()
    print(f"\n{'croma ajeno':>12s} {'píxeles':>8s}  losa")
    anterior = 0.0
    for peor, cuantos, nombre in filas:
        if peor == 0.0:
            continue
        marca = "  <-- HUECO" if peor - anterior > 0.15 else ""
        print(f"{peor:12.3f} {cuantos:8d}  {nombre[:58]}{marca}")
        anterior = peor
    limpias = sum(1 for f in filas if f[0] == 0.0)
    print(f"\n{limpias} losas sin un solo píxel ajeno.  AJENO_MIN = {AJENO_MIN}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--preview", metavar="PNG", help="guarda la hoja ampliada")
    ap.add_argument(
        "--calibrar",
        action="store_true",
        help="imprime la separación de croma de cada losa (de dónde sale AJENO_MIN)",
    )
    args = ap.parse_args()

    losas = losas_del_pack()
    print(f"losas repetibles en los temas de terreno: {len(losas)}")
    if not losas:
        raise SystemExit("no encontré ni una losa: ¿cambió la ruta del pack?")
    rellenos = rellenos_del_pack(losas)
    print(f"materiales que el pack declara (celdas de un solo color): {len(rellenos)}")
    if args.calibrar:
        calibrar(losas, rellenos)
        return

    # 1. Elegir por color.
    elegidas = {}
    for clave, spec in COLOR_DEL_TILE.items():
        rgb01, fuente = spec[0], spec[1]
        familia = spec[2] if len(spec) > 2 else None
        derivacion = spec[3] if len(spec) > 3 else None
        objetivo = tuple(c * 255 for c in rgb01)
        cand = elegir(losas, objetivo, fuente, familia, rellenos)
        # La familia ya dice DE QUÉ es la losa; el color sólo ordena dentro. Pedirle
        # además que se parezca al color declarado dejaría a la vereda sin ninguna
        # (el pack la dibuja crema y el color viejo era gris) y la ciudad volvería
        # al color plano. Sin familia sigue mandando el corte de color de siempre.
        if not familia:
            cand = [l for l in cand if dist(l["rgb"], objetivo) <= DELTA_MAX]
        if not cand:
            print(f"  {clave:16s} SIN LOSA parecida (Δ>{DELTA_MAX:.0f}) — se queda en color plano")
            continue
        # El clima del bioma, encima del pixel del pack. Se guarda una COPIA
        # derivada por losa: la original la sigue usando el bioma de referencia.
        if derivacion:
            destino, peso = derivacion
            cand = [dict(l, im=derivar(l["im"], destino, peso)) for l in cand]
        # **El piso de la galería es UNA piedra, no un muestrario**: el pack trae pisos de interior de
        # otro estilo (alfombra, parqué) que por color caen cerca y salían como manchas ocre sueltas en
        # el suelo de la cueva. Sólo se quedan las que se parecen de verdad a la primera.
        if clave == "floor/cueva" and cand:
            import colorsys
            import numpy as np

            def croma90(im):
                px = np.asarray(im.convert("RGB")).reshape(-1, 3) / 255.0
                return float(np.percentile([colorsys.rgb_to_hsv(*p)[1] for p in px], 90))

            c0 = croma90(cand[0]["im"])
            cand = [cand[0]] + [l for l in cand[1:] if abs(croma90(l["im"]) - c0) <= 0.06]
        elegidas[clave] = cand
        mejor = cand[0]
        clima = ""
        if derivacion:
            m2 = mod_medio(mejor["im"])
            clima = f" → clima {tuple(round(c) for c in m2)}"
        print(
            f"  {clave:20s} objetivo={tuple(round(c) for c in objetivo)} "
            f"→ {tuple(round(c) for c in mejor['rgb'])} (Δ{dist(mejor['rgb'], objetivo):.0f}){clima} "
            f"{mejor['files'][0][:46]} +{len(cand) - 1} variantes"
        )

    # 2. Armar la hoja. La celda (0,0) es BLANCA: es la que usan los tiles sin
    #    losa, y deja el color de vértice tal cual.
    items = [("__blanco__", Image.new("RGBA", (CELL, CELL), (255, 255, 255, 255)))]
    for clave, cand in elegidas.items():
        for i, l in enumerate(cand):
            items.append((clave if i == 0 else f"{clave}#s{i}", l["im"]))
    if len(items) > COLS * FILAS:
        raise SystemExit(f"{len(items)} celdas no entran en la hoja de {COLS}x{FILAS}")
    pagina = Image.new("RGBA", PAGINA_PX, (0, 0, 0, 0))
    sitio = {}
    for i, (nombre, im) in enumerate(items):
        col, row = i % COLS, i // COLS
        pagina.paste(im, (col * CELL, row * CELL))
        sitio[nombre] = (col, row)

    print(f"hoja: {pagina.size[0]}x{pagina.size[1]} px, {len(items)} celdas")
    if args.preview:
        fondo = Image.new("RGBA", pagina.size, (28, 28, 38, 255))
        fondo.alpha_composite(pagina)
        fondo.convert("RGB").resize(
            (pagina.size[0] * 4, pagina.size[1] * 4), Image.NEAREST
        ).save(args.preview)
        print(f"vista previa: {args.preview}")
    if args.dry_run:
        return

    os.makedirs(os.path.join(ASSETS, "art/pack"), exist_ok=True)
    pagina.save(os.path.join(ASSETS, PAGINA_REL), optimize=True)

    with open(MANIFEST) as fh:
        man = json.load(fh)
    sprites, objects = man["sprites"], man.setdefault("objects", {})
    # **Esta hoja es la DUEÑA del suelo.** No alcanza con borrar lo de la corrida
    # anterior: los materiales de suelo acumulaban variantes de tres importadores
    # distintos (`#1..` de import-calles, `#p*` de import-construccion, `#s*` de
    # acá), cada una apuntando a otra página. La malla del chunk sólo puede usar
    # las que estén EN esta hoja, así que las de afuera no son variantes: son
    # claves muertas que sólo confunden. Se van todas y quedan las de la hoja.
    #
    # ⚠ **Pero SÓLO de los materiales a los que esta hoja les da losa.** El barrido
    # borraba las claves de las 23 de `COLOR_DEL_TILE` mirara o no si iba a
    # reponerlas, y `tiles/subsuelo` no encuentra losa en el pack (el pack es
    # urbano, no trae tierra oscura repetible). Tenía su fallback DIBUJADO en
    # `art/gen/mundo.png`, este barrido se lo comía y no ponía nada en su lugar:
    # el slot quedaba sin un solo pixel y el test `ningun_slot_se_queda_sin_pixel`
    # se plantaba. Un material que esta hoja no puede servir no es suyo para
    # borrar.
    borradas = 0
    for clave in COLOR_DEL_TILE:
        # Las VARIANTES viejas se van siempre: apuntaban a celdas de la hoja
        # anterior, que este mismo script acaba de reescribir.
        muertas = [x for x in sprites if x.startswith(f"{clave}#")]
        if clave in elegidas:
            muertas.append(clave)
        for k in muertas:
            if sprites.pop(k, None) is not None:
                borradas += 1
        # **Y si se queda sin ninguna, el objeto se va entero** — pero sólo si el
        # objeto lo escribió este script. Un `objects` con `variants: []` es una
        # entrada muerta que igual aparece en el editor y con el nombre feo
        # (`"tiles/grass"`), porque el humano vive en `slots.rs`. El de
        # `tiles/subsuelo` en cambio dice "Subsuelo": ése lo puso otro importador
        # y no es de acá. Desde que `contamina` echa las esquinas de autotile,
        # ocho materiales quedan con una sola losa y caían justo en este agujero.
        if objects.get(clave, {}).get("label") == clave:
            objects.pop(clave, None)
        elif clave in objects:
            objects[clave]["variants"] = []
    if borradas:
        print(f"suelo: -{borradas} claves viejas (variantes fuera de la hoja)")

    nuevos, variantes = [], {}
    for clave, cand in elegidas.items():
        ids = []
        for i in range(len(cand)):
            nombre = clave if i == 0 else f"{clave}#s{i}"
            col, row = sitio[nombre]
            sprites[nombre] = {
                "png": PAGINA_REL,
                "grid": [CELL, CELL],
                "rect": {"col": col, "row": row, "w": 1, "h": 1},
                "layer": "Floor",
            }
            nuevos.append(nombre)
            if i:
                ids.append(f"s{i}")
        if ids:
            obj = objects.setdefault(clave, {"label": clave, "variants": []})
            obj["variants"].extend({"id": i, "label": f"{clave} {i}"} for i in ids)
            variantes[clave] = ids

    with open(MANIFEST, "w") as fh:
        json.dump(man, fh, indent=1, ensure_ascii=False, sort_keys=True)
    with open(INDEX, "w") as fh:
        json.dump({"sprites": nuevos, "variantes": variantes, "pagina": PAGINA_REL}, fh, indent=1)
    print(f"manifest: {len(nuevos)} celdas de suelo apuntando a {PAGINA_REL}")
    # La hoja se acaba de reescribir: las marcas dibujadas de la nieve (el pack
    # no trae nieve) se vuelven a pegar en sus celdas libres.
    if not args.dry_run:
        import subprocess, sys
        subprocess.run([sys.executable, os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                                     "dibujar-marcas-de-suelo.py")], check=True)
        # Y el macizo del subsuelo (roca del pack en rampa fría), que también vive en esta hoja.
        subprocess.run([sys.executable, os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                                     "dibujar-subsuelo.py")], check=True)


if __name__ == "__main__":
    main()
