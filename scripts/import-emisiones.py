#!/usr/bin/env python3
"""**QUIÉN ILUMINA DE NOCHE: la emisión de cada asset, MEDIDA en su dibujo.**

    python3 scripts/import-emisiones.py
    python3 scripts/import-emisiones.py --dry-run
    python3 scripts/import-emisiones.py --informe    # tabla completa de lo medido

## Por qué existe

El motor de luz estaba entero —dos pasadas por píxel, pozo, bulbo, sombra
direccional, tinte de sprites (`ha-game::light2d` + `night_veil.wgsl`)— y el
mundo de noche seguía negro. La causa no era el shader: **de los 14.440 sprites
del manifest, sólo CUATRO declaraban `emission`** (`prop/farol`,
`furniture/campfire`, `items/lantern` y un botón de la UI). O sea que el pueblo
tenía faroles, velas, fogatas, letreros de neón y torres de reflector dibujados,
y ni uno tiraba un fotón. El sistema de luz existía y no tenía a quién iluminar.

Esto lo arregla de raíz y **por medición**, que es la regla 7 del proyecto: la
emisión no se escribe a ojo, se saca de los píxeles que el pack ya pintó
brillando.

## Cómo se mide

Por cada clave candidata (ver [`FAMILIAS`], que sólo decide QUIÉN se mira):

1. Se recorta el sprite de su hoja (`rect`, primer cuadro de `strip`, o la hoja
   entera si no tiene recorte) y se descartan los píxeles transparentes.
2. Se busca el **núcleo caliente**: los píxeles cuya luminancia supera a la vez
   un piso absoluto y el percentil alto del propio recorte. Se agrupan en
   componentes conexas de 4 vecinos y se queda la de mayor "energía"
   (suma de luminancia), que es la llama / el vidrio / el tubo de neón.
3. **Si no hay núcleo, el asset NO emite.** Es el filtro honesto: `prop/farol`
   entra porque tiene un vidrio encendido; `prop/vela_consumida_1` no, porque el
   pack la dibujó apagada. Nadie decide eso por nombre.
4. Del núcleo salen los tres números que el juego necesita:
   - **`color`**: media de luminancia del núcleo MÁS su anillo de 2 px,
     normalizada a máximo 1. El anillo es lo que aporta el ámbar: el centro de
     una llama es casi blanco y sin el halo el charco de luz sale color crema
     (el mismo error que documenta `night_veil.wgsl` en su recalibrado).
   - **`offset`**: el centroide del núcleo respecto al centro del sprite, en
     METROS. Es lo que hace que la luz de un farol salga de su vidrio, a dos
     metros del piso, y no del centro geométrico del poste.
   - **`radius`**: el alcance base de la familia escalado por el tamaño medido
     del núcleo (`sqrt(área)`), acotado. Una vela de 3 píxeles y una hoguera de
     40 no pueden alumbrar lo mismo, y el dato de cuál es cuál está en el
     dibujo.

`intensity`, `flicker` y `day` SÍ son de la familia: no son geometría, son
comportamiento (una llama late, un LED no), y no hay forma de medirlos en un
cuadro quieto. Van declarados y comentados en [`FAMILIAS`].

## Idempotente

Escribe `scripts/emisiones-import-index.json` con las claves que tocó. En la
corrida siguiente **borra la emisión de esas claves y sólo de ésas** antes de
volver a medir, así una re-corrida no acumula ni pisa lo que puso el editor a
mano en otro slot (el bug de `import-pack-masivo`, ver biblia §8).

## Lo que NO hace

- No dibuja ni un píxel. La emisión es un dato, no un asset.
- No toca `prop/farol` ni `furniture/campfire` de forma distinta al resto: si el
  índice dice que fueron suyos los re-mide, y si no, los respeta como estaban.
  (En la primera corrida no son suyos, así que quedan tal cual los dejó el
  editor; el `--forzar` los adopta.)
- No siembra nada. Que haya faroles en la vereda es cosa de `worldgen`.
"""

import argparse
import json
import os
import re
from collections import deque

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ASSETS = os.path.join(ROOT, "native/crates/ha-game/assets")
MANIFEST = os.path.join(ASSETS, "art/manifest.json")
INDEX = os.path.join(ROOT, "scripts/emisiones-import-index.json")

#: Metros de mundo por celda de 32 px. Es el tamaño con el que `worldgen` planta
#: un prop de una celda (`size: [0.95, 0.95]`), así que el offset medido en
#: píxeles se convierte con esto y cae donde el jugador lo ve.
METROS_POR_CELDA = 0.95

#: Piso absoluto de luminancia (0..1) para considerar un píxel "encendido".
#: Medido sobre el pack: el vidrio de `prop/farol` está en 0.93, la llama de
#: `prop/fogata_1` en 0.88, el tubo de un letrero en 0.80.
#:
#: **0.70 y no 0.72, y el motivo es la paleta.** Con 0.72 quedaban afuera dos
#: cosas que sí están encendidas: la llama de `items/torch` (0.702) y cualquier
#: luz ROJA. Lo segundo no es una casualidad: el color más claro y saturado que
#: tiene la paleta medida de LimeZu del lado del rojo es (255,160,160), que mide
#: **0.707** — o sea que un piso en 0.72 no dice "esto está apagado", dice "en
#: este pack no puede existir una luz roja", que es una propiedad del umbral y no
#: del arte. Bajarlo a 0.70 admite el rojo y la antorcha, y NO admite lo que
#: tiene que quedar afuera: la fogata sin encender mide 0.686, la chimenea de
#: ladrillo 0.504 y el letrero pintado 0.429. El margen sigue siendo cómodo.
PISO_LUZ = 0.70

#: …y además tiene que destacar DENTRO de su propio dibujo: el núcleo son los
#: píxeles por encima de esta fracción del máximo del recorte. Sin esto, un
#: sprite claro entero (una pared blanca) daría "núcleo" en toda su superficie.
FRACCION_DEL_MAXIMO = 0.86

#: Área mínima del núcleo, en píxeles. Uno o dos píxeles claros son un brillo
#: especular (el filo de un metal), no una fuente de luz.
NUCLEO_MINIMO = 3

#: Ancho del anillo que se muestrea alrededor del núcleo para sacar el TINTE.
#: El centro de una llama es blanco; el color está en lo que la rodea.
ANILLO_PX = 2

#: **Claves con emisión escrita a mano que el importador ADOPTA igual.**
#:
#: Vacío, y vale la pena decir por qué se probó y se deshizo: se metió
#: `prop/farol` acá para que su radio saliera medido en vez de escrito a ojo, y
#: la medición contestó lo correcto — **`prop/farol` está dibujado APAGADO**
#: (máximo 0,50 de luminancia: es el poste gris con el vidrio sin encender) — así
#: que le quitó la emisión y dejó la calle a oscuras. La conclusión no fue que la
#: medición esté mal sino que el farol del alumbrado tenía que ser un asset que
#: el pack SÍ dibujó encendido; ése es `prop/farol_de_calle_5`
#: (`worldgen::FAROL_DE_CALLE`). La emisión a mano de `prop/farol` se respeta,
#: como la de cualquier slot que curó el editor.
ADOPTAR: set[str] = set()


class Familia:
    """Una clase de emisor: a quién alcanza y cómo se comporta su luz.

    `patron` decide QUIÉN se mide (nunca cuánto). `base`, `intensidad`,
    `parpadeo` y `de_dia` son comportamiento declarado — no se puede medir en un
    cuadro quieto si una luz late.
    """

    def __init__(
        self, nombre, patron, base, intensidad, parpadeo, de_dia=False, tope=None, multifoco=False
    ):
        self.nombre = nombre
        self.patron = re.compile(patron, re.I)
        #: Alcance base en metros, para un núcleo de ~6 px de lado.
        self.base = base
        self.intensidad = intensidad
        self.parpadeo = parpadeo
        self.de_dia = de_dia
        #: `True` = el asset tiene VARIOS focos y el efectivo es el conjunto.
        #: Una guirnalda son doce bombitas y un letrero es un tubo entero: quedarse
        #: con el clúster más energético pone la luz en UNA bombita, o sea
        #: descentrada respecto de lo que el ojo ve encenderse. Con esto, el foco
        #: es el centroide de TODOS los clústeres encendidos y el radio sale de su
        #: extensión — que sigue siendo medir, sólo que la forma medida es otra.
        self.multifoco = multifoco
        #: Techo del alcance: ni la hoguera más grande abre un pozo de media
        #: pantalla (ver `LIGHT_REACH` en `light2d.rs`, que además lo multiplica).
        self.tope = tope if tope is not None else base * 1.8


#: **El orden manda**: la primera familia que casa se queda con la clave.
FAMILIAS = [
    # Apagados explícitos: el pack los dibujó sin llama y no se miran siquiera.
    Familia("apagado", r"apagad|consumida|sin_farol|_off\b", 0, 0, 0),
    # La UI no vive en el mundo: su "farol" es un icono de botón.
    Familia("ui", r"^ui/", 0, 0, 0),
    # Reflectores: torre de obra, estadio, faro, torre de vigilancia. Luz dura,
    # fría, alta y de largo alcance — es la que baña una manzana entera.
    # `tope` bajo a propósito: `LIGHT_REACH` (1.5) multiplica el radio, así que
    # 12 m ya son 18 m de pozo — más de media pantalla a zoom de caminata, que es
    # el "no queda noche contra la cual leer el charco" de `night_veil.wgsl`.
    Familia("reflector", r"estadio_luz|luz_torre|torre_de_vigilancia_luz|faro_techo_luz",
            base=9.0, intensidad=1.15, parpadeo=0.0, tope=12.0),
    # Alumbrado público. El farol es lo que dibuja la calle de noche: casi sin
    # parpadeo (es eléctrico, no una llama).
    #
    # **Base 4,5 y no 7, y sale de mirar la captura.** Con 7 —el número que tenía
    # `prop/farol` escrito a mano— el pozo mide 10,5 m de radio (`LIGHT_REACH` lo
    # multiplica por 1,5), o sea 21 m de punta a punta sobre un encuadre de a pie
    # de 30 m. Con el alumbrado sembrado cada 8 tiles eso son diez pozos
    # superpuestos por celda: `cover` (el producto de `1-s` del shader) llega a
    # cero, la vereda vuelve a su albedo entero teñido de ámbar y la calle de
    # medianoche se ve como un mediodía nublado. Se vio así en
    # `HA_NIGHTTEST` night-1-block.
    #
    # A 4,5 el pozo mide 6,75 m de radio y, con el paso de 14 tiles de
    # `worldgen::PASO_ALUMBRADO`, entre farol y farol **queda oscuridad**. Que
    # haya oscuridad entre los charcos no es un detalle: es la única forma de que
    # un charco se lea como un charco.
    Familia("farol", r"farol|street_lamp|luz_poste|estructura_luz|luz_modular",
            base=4.5, intensidad=1.0, parpadeo=0.05, tope=6.5),
    # Fuego abierto: fogata, hoguera, chimenea encendida. Late fuerte.
    Familia("fuego", r"fogata|campfire|bonfire|chimenea|fireplace|brasero|hoguera",
            base=5.0, intensidad=1.1, parpadeo=0.55),
    # Antorcha en la mano o en la pared.
    Familia("antorcha", r"torch|antorcha", base=5.0, intensidad=1.05, parpadeo=0.6),
    # Farol de camping / linterna: portátil, pozo corto y muy cálido.
    Familia("linterna", r"lantern|linterna|camping_farol", base=4.5, intensidad=1.0, parpadeo=0.35),
    # Vela: la luz más chica que existe. Un palmo de mesa.
    Familia("vela", r"vela|candle", base=2.2, intensidad=0.75, parpadeo=0.65),
    # Neón y carteles: no calientan nada, pero se ven de día y zumban.
    Familia("neon", r"letrero_grafiti|neon|led_luz", base=4.0, intensidad=0.8,
            parpadeo=0.12, de_dia=True),
    # Lámpara de interior (velador, araña, aplique).
    Familia("lampara", r"lampara|^furniture/.*luz", base=4.0, intensidad=0.9, parpadeo=0.04),
    # Lo bioluminiscente del jardín: flores y estatuas que el pack pintó
    # brillando. Pozo mínimo — son adorno, no alumbrado.
    Familia("bioluz", r"^nature/luz_", base=1.6, intensidad=0.5, parpadeo=0.25),
    # La guirnalda: doce bombitas repartidas a lo largo de un cable. Multifoco
    # (ver `Familia.multifoco`) porque su luz no sale de una bombita sino de la
    # hilera entera, y el radio tiene que cubrir el vano.
    # `base` chica: la guirnalda es luz de ADORNO, no alumbrado. Con 5,0 el
    # alcance medido llegaba al tope de 8 m —12 m de pozo con `LIGHT_REACH`— y en
    # la captura de `HA_NIGHTTEST` la plaza salía como una mancha ámbar sin
    # forma, con las guirnaldas fundidas entre sí. A 3,2 se ve la hilera de
    # bombitas encendidas y el charco que tiran debajo, que es lo que hace la
    # referencia.
    Familia("guirnalda", r"luces_guirnalda", base=3.2, intensidad=0.9, parpadeo=0.08,
            tope=4.6, multifoco=True),
    # Lo que brilla por dentro: la calabaza de Halloween con su vela adentro, el
    # hongo encendido. Late como una llama, porque adentro hay una.
    Familia("interior", r"luces_|glowing|calabaza", base=3.0, intensidad=0.85, parpadeo=0.5),
]

#: Quién se mira, antes de clasificar. Es un filtro barato: sin él habría que
#: abrir los 14.440 recortes del manifest para descartar 14.300.
CANDIDATO = re.compile(
    r"(farol|lamp|lantern|linterna|candle|vela|campfire|fogata|bonfire|torch|antorcha"
    r"|chimenea|fireplace|brasero|hoguera|letrero_grafiti|neon|led_luz|luz|luces"
    r"|glowing|brilla|encendid)",
    re.I,
)


def luminancia(p):
    """Luma perceptual 0..1 (Rec. 709), igual que `scripts/lib/pixel.py::luz`."""
    r, g, b = p[0] / 255.0, p[1] / 255.0, p[2] / 255.0
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def recorte_de(entrada, cache):
    """El dibujo de esta clave, en RGBA. `None` si no se puede resolver.

    Resuelve, en este orden: `rect` (recorte estático), `strip` (primer cuadro de
    una tira animada — el cuadro 0 es el que el pack dibuja completo), la pose
    `Light` si la hay, y por último la hoja entera (assets de un PNG propio).
    """
    png = entrada.get("png")
    if not png:
        return None
    ruta = os.path.join(ASSETS, png)
    if not os.path.exists(ruta):
        return None
    if ruta not in cache:
        cache[ruta] = Image.open(ruta).convert("RGBA")
    hoja = cache[ruta]
    cw, ch = entrada.get("grid") or (32, 32)
    caja = entrada.get("rect") or entrada.get("strip")
    if caja:
        x, y = caja["col"] * cw, caja["row"] * ch
        w, h = caja["w"] * cw, caja["h"] * ch
        if x + w > hoja.width or y + h > hoja.height:
            return None
        return hoja.crop((x, y, x + w, y + h))
    return hoja


def nucleo(img, multifoco=False):
    """El grupo de píxeles ENCENDIDOS más energético del recorte.

    Con `multifoco`, en vez del grupo ganador devuelve **la unión de todos** los
    grupos que pasan el umbral: para una guirnalda o un tubo de neón el foco es
    la hilera entera, no la bombita más brillante.

    Devuelve `(pixeles, area, cx, cy)` o `None` si el dibujo no tiene ninguno.
    Componentes conexas de 4 vecinos sobre la máscara de "encendido", y se queda
    la de mayor suma de luminancia: en un farol con dos vidrios gana el vidrio,
    no el reflejo del poste.
    """
    w, h = img.size
    px = img.load()
    lum = [[0.0] * w for _ in range(h)]
    maximo = 0.0
    for y in range(h):
        for x in range(w):
            if px[x, y][3] < 128:
                continue
            v = luminancia(px[x, y])
            lum[y][x] = v
            maximo = max(maximo, v)
    if maximo < PISO_LUZ:
        return None
    umbral = max(PISO_LUZ, maximo * FRACCION_DEL_MAXIMO)

    visto = [[False] * w for _ in range(h)]
    mejor = None
    todos = []
    for y0 in range(h):
        for x0 in range(w):
            if visto[y0][x0] or lum[y0][x0] < umbral:
                continue
            cola, grupo, energia = deque([(x0, y0)]), [], 0.0
            visto[y0][x0] = True
            while cola:
                x, y = cola.popleft()
                grupo.append((x, y))
                energia += lum[y][x]
                for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                    nx, ny = x + dx, y + dy
                    if 0 <= nx < w and 0 <= ny < h and not visto[ny][nx] and lum[ny][nx] >= umbral:
                        visto[ny][nx] = True
                        cola.append((nx, ny))
            todos += grupo
            if mejor is None or energia > mejor[0]:
                mejor = (energia, grupo)
    if mejor is None or len(mejor[1]) < NUCLEO_MINIMO:
        return None
    grupo = todos if multifoco else mejor[1]
    cx = sum(p[0] for p in grupo) / len(grupo) + 0.5
    cy = sum(p[1] for p in grupo) / len(grupo) + 0.5
    return grupo, len(grupo), cx, cy


def tinte_de(img, grupo):
    """El COLOR de la luz: el núcleo más su anillo, pesado por luminancia.

    Normalizado a máximo 1 porque el brillo lo pone `intensity`, no el color: el
    shader trata este vector como matiz puro (ver `hue_of` en `night_veil.wgsl`).
    """
    w, h = img.size
    px = img.load()
    en_grupo = set(grupo)
    muestras = set(grupo)
    for x, y in grupo:
        for dy in range(-ANILLO_PX, ANILLO_PX + 1):
            for dx in range(-ANILLO_PX, ANILLO_PX + 1):
                nx, ny = x + dx, y + dy
                if 0 <= nx < w and 0 <= ny < h and px[nx, ny][3] >= 128:
                    muestras.add((nx, ny))
    acc = [0.0, 0.0, 0.0]
    peso = 0.0
    for x, y in muestras:
        c = px[x, y]
        # El núcleo pesa doble: es la fuente; el anillo aporta el matiz.
        #
        # **Y el COLOR pesa, no sólo el brillo** (28 sep 2026). Pesando sólo por
        # luminancia ganaba el núcleo blanco de la llama y una vela amarilla salía
        # con luz blanca (1,0.99,0.99): el usuario lo vio en las antorchas. La
        # croma (max−min) sube el peso de los píxeles con matiz; un farol blanco no
        # tiene croma en ningún lado y sigue saliendo blanco.
        croma = (max(c[:3]) - min(c[:3])) / 255.0
        p = luminancia(c) * (0.15 + croma) * (2.0 if (x, y) in en_grupo else 1.0)
        acc[0] += c[0] * p
        acc[1] += c[1] * p
        acc[2] += c[2] * p
        peso += p
    if peso <= 0:
        return [1.0, 0.82, 0.45]
    rgb = [v / peso / 255.0 for v in acc]
    mx = max(rgb) or 1.0
    return [round(v / mx, 4) for v in rgb]


#: El ámbar de una llama: lo que emite el fuego aunque su núcleo se dibuje blanco.
LLAMA = [1.0, 0.74, 0.30]
#: Por debajo de esta saturación (0..1) un emisor de fuego se lee como luz blanca.
SATURACION_MINIMA_DE_LLAMA = 0.30


def color_de_llama(fam, rgb):
    """**Una llama emite luz cálida.** Una vela del pack son cuatro píxeles de
    llama sobre una cera blanca: la medida sale casi blanca (1, 0.99, 0.97) y la
    luz de la antorcha se veía blanca (reporte del usuario, 28 sep 2026). Para el
    fuego, la vela y la antorcha, si lo medido no tiene color se toma el ámbar de
    llama; si lo tiene (una vela roja, una brasa), manda lo medido."""
    if fam.nombre not in ("fuego", "antorcha", "vela"):
        return rgb
    mx, mn = max(rgb), min(rgb)
    sat = (mx - mn) / mx if mx > 0 else 0.0
    return LLAMA if sat < SATURACION_MINIMA_DE_LLAMA else rgb


def familia_de(clave):
    for f in FAMILIAS:
        if f.patron.search(clave):
            return f
    return None


def medir(clave, entrada, cache):
    """`(emission, motivo)` — `emission` es `None` si esta clave no ilumina."""
    fam = familia_de(clave)
    if fam is None:
        return None, "sin familia"
    if fam.base <= 0:
        return None, f"familia {fam.nombre}"
    img = recorte_de(entrada, cache)
    if img is None:
        return None, "sin dibujo"
    n = nucleo(img, fam.multifoco)
    if n is None:
        return None, "sin núcleo encendido"
    grupo, area, cx, cy = n
    w, h = img.size
    cw, ch = entrada.get("grid") or (32, 32)
    mpp = METROS_POR_CELDA / cw  # metros por píxel

    # El alcance crece con el LADO del núcleo (√área), no con su área: duplicar
    # el ancho de una llama no cuadruplica lo que alumbra. 6 px de lado = el
    # tamaño de referencia con el que se calibró `base`.
    if fam.multifoco:
        # Multifoco: lo que manda no es cuánta superficie brilla sino CUÁNTO
        # ABARCA la hilera. Doce bombitas de 6 px repartidas a lo largo de cuatro
        # metros iluminan cuatro metros, no medio. El lado es el ancho medido del
        # conjunto, en unidades de celda.
        xs = [q[0] for q in grupo]
        lado = (max(xs) - min(xs) + 1) / (cw / 6.0)
    else:
        lado = area ** 0.5
    radius = fam.base * (0.55 + 0.45 * (lado / 6.0))
    radius = round(min(max(radius, fam.base * 0.4), fam.tope), 2)

    # Y+ es ARRIBA en el mundo, abajo en la imagen.
    off_x = round((cx - w / 2.0) * mpp, 3)
    off_y = round((h / 2.0 - cy) * mpp, 3)

    return {
        "color": color_de_llama(fam, tinte_de(img, grupo)),
        "radius": radius,
        "intensity": fam.intensidad,
        "offset": [off_x, off_y],
        "flicker": fam.parpadeo,
        "day": fam.de_dia,
    }, f"{fam.nombre}: núcleo {area}px en ({cx:.1f},{cy:.1f}) de {w}x{h}"


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--dry-run", action="store_true", help="no escribe el manifest")
    ap.add_argument("--informe", action="store_true", help="lista también lo descartado")
    ap.add_argument(
        "--forzar",
        action="store_true",
        help="re-mide también las claves con emisión puesta a mano (las adopta)",
    )
    args = ap.parse_args()

    man = json.load(open(MANIFEST))
    sprites = man.get("sprites", {})
    previas = set()
    if os.path.exists(INDEX):
        previas = set(json.load(open(INDEX)).get("claves", []))

    # Lo que había antes y NO es nuestro: se respeta (lo puso el editor a mano).
    ajenas = {
        k
        for k, v in sprites.items()
        if isinstance(v, dict) and v.get("emission") and k not in previas and k not in ADOPTAR
    }
    if args.forzar:
        ajenas = set()

    cache = {}
    puestas, descartes = {}, []
    for clave in sorted(sprites):
        entrada = sprites[clave]
        if not isinstance(entrada, dict) or not CANDIDATO.search(clave):
            continue
        if clave in ajenas:
            descartes.append((clave, "emisión a mano, respetada"))
            continue
        em, motivo = medir(clave, entrada, cache)
        if em is None:
            descartes.append((clave, motivo))
            continue
        puestas[clave] = em
        print(f"  {clave:52s} r={em['radius']:5.2f} c={em['color']} off={em['offset']}  [{motivo}]")

    print(f"\n{len(puestas)} emisores medidos · {len(descartes)} descartados")
    if args.informe:
        print("\n-- descartes --")
        for k, m in descartes:
            print(f"  {k:52s} {m}")

    if args.dry_run:
        print("[dry-run] el manifest no se tocó")
        return

    # Idempotencia: se limpia lo NUESTRO de la corrida anterior y se re-escribe.
    for k in previas:
        e = sprites.get(k)
        if isinstance(e, dict):
            e.pop("emission", None)
    for k, em in puestas.items():
        sprites[k]["emission"] = em

    with open(MANIFEST, "w") as f:
        json.dump(man, f, indent=1, ensure_ascii=False, sort_keys=True)
    with open(INDEX, "w") as f:
        json.dump({"claves": sorted(puestas)}, f, indent=1, ensure_ascii=False)
    print(f"manifest: {len(puestas)} claves con emisión")


if __name__ == "__main__":
    main()
