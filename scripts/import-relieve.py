#!/usr/bin/env python3
"""**LA RAMPA DEL RELIEVE: el paso pelado por donde se sube la terraza.**

    python3 scripts/import-relieve.py
    python3 scripts/import-relieve.py --dry-run

## Por qué existe

El relieve entero se dibujaba con `procedural_art.rs` —cuadrados grises con una
cara marrón— al lado de un pack de doce mil dibujos. La queja fue literal:
*"está prohibido seguir usando colores base; poné los assets de Llano y, cuando
sea marrón, los de Arena"*.

La PARED y el BORDILLO se los lleva `import-acantilado.py`, que arma el
acantilado alto con el kit modular del pack. Acá queda la **rampa**, que no es
una pared: es el paso, y por el paso no hay meseta ni labio, hay tierra pisada.

Ese pixel sale de los autotiles de terreno del pack
(`Autotiles_32x32/Game_Maker_Studio_Autotiles_Singles_32x32`), 26 hojas de 12x4
celdas. Dos son las que se pidieron por nombre:

| hoja | fondo medido | qué es |
|---|---|---|
| `..._1` | (170,143,90) tierra apagada | **el Llano** → `slope/pasto` |
| `..._2` | (202,136,84) arena naranja | **la Arena** → `slope/arena` |

De cada una se toma su celda de FONDO PURO —el terreno sin pasto encima—, que es
exactamente el suelo desnudo de un paso de montaña.

## Nada de rects a ojo

El número de hoja no dice nada (son archivos numerados), así que cada familia
declara el color que su celda de fondo tiene que medir: si LimeZu reordena el
pack, el script se planta en vez de importar la cancha de tenis como si fuera el
llano. La maquinaria que puntúa las trece formas de autotile sigue acá porque es
la que VERIFICA que la hoja sea de transición: una hoja lisa no llega a
[`MINIMO`] y falla.

Anota lo suyo en `scripts/relieve-import-index.json`, así una segunda corrida
reemplaza sus claves sin tocar el resto del manifest.
"""

import json
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
PACK = ROOT / "PRUEBAS-CODIGOS/modernexteriors-win/Modern_Exteriors_32x32"
AUTOTILES = PACK / "Autotiles_32x32/Game_Maker_Studio_Autotiles_Singles_32x32"
HOJA_PREF = "Game_Maker_Studio_Autotiles_Singles_32x32_"

ASSETS = ROOT / "native/crates/ha-game/assets"
MANIFEST = ASSETS / "art/manifest.json"
PAGINA_REL = "art/pack/relieve.png"
INDEX = ROOT / "scripts/relieve-import-index.json"

CELL = 32
#: Las hojas del pack son de 12x4 celdas: el autotile de 47 formas empaquetado.
HOJA_COLS, HOJA_FILAS = 12, 4
#: Ancho de la página de salida, en celdas. 32 celdas = 1024 px.
PAGINA_COLS = 32

# ---------------------------------------------------------------------------
# Las dos familias
# ---------------------------------------------------------------------------

#: `material -> (nº de hoja, color de fondo esperado)`.
#:
#: El número de hoja no alcanza como documentación —son archivos numerados sin
#: nombre—, así que va con el color que tiene que medir su celda de fondo puro.
#: Si LimeZu reordena el pack, el script se planta en vez de importar la cancha
#: de tenis en lugar del llano.
FAMILIAS = {
    "pasto": (1, (170, 143, 90)),
    "arena": (2, (202, 136, 84)),
}

#: El verde del pasto, que es el terreno ALTO de las dos hojas. Se comprueba
#: igual que el fondo: las dos familias tienen que compartirlo, si no la meseta
#: de la playa y la del prado saldrían de dos verdes distintos.
VERDE = (71, 151, 87)

# ---------------------------------------------------------------------------
# Medir: qué forma de autotile es cada celda
# ---------------------------------------------------------------------------

#: `forma -> (n, e, s, w, ne, nw, se, sw)`, donde 1 = por ese lado se CAE.
#:
#: Es la misma convención de `world2d::autotile_shape`: el nombre mira hacia el
#: terreno más bajo. `edge_s` es la cara que da a la terraza de abajo.
FORMAS = {
    "center": (0, 0, 0, 0, 0, 0, 0, 0),
    "edge_n": (1, 0, 0, 0, 0, 0, 0, 0),
    "edge_e": (0, 1, 0, 0, 0, 0, 0, 0),
    "edge_s": (0, 0, 1, 0, 0, 0, 0, 0),
    "edge_w": (0, 0, 0, 1, 0, 0, 0, 0),
    "corner_ne": (1, 1, 0, 0, 0, 0, 0, 0),
    "corner_nw": (1, 0, 0, 1, 0, 0, 0, 0),
    "corner_se": (0, 1, 1, 0, 0, 0, 0, 0),
    "corner_sw": (0, 0, 1, 1, 0, 0, 0, 0),
    "inner_ne": (0, 0, 0, 0, 1, 0, 0, 0),
    "inner_nw": (0, 0, 0, 0, 0, 1, 0, 0),
    "inner_se": (0, 0, 0, 0, 0, 0, 1, 0),
    "inner_sw": (0, 0, 0, 0, 0, 0, 0, 1),
}

#: Píxeles de duda a cada lado de una línea de corte. La orilla entre los dos
#: terrenos es una curva dentada de unos cuatro píxeles: puntuarla sería
#: puntuar el dibujo, no la forma. Sólo se ignora alrededor de los cortes que la
#: forma TIENE — para `center`, que no tiene ninguno, se puntúa la celda entera
#: (si no, las celdas con una franja de tierra por el medio pasaban por centro).
BORDE = 5

#: Puntaje mínimo para dar una forma por buena. Medido sobre las dos hojas
#: elegidas: la peor es `corner_ne` con 0,95 y ninguna otra baja de 0,98. Una
#: hoja que no sea de transición (la arena lisa, por ejemplo) no pasa de 0,84.
MINIMO = 0.90


def mascara(forma):
    """La máscara ideal de una forma: `True` = terreno alto, `None` = no cuenta."""
    n, e, s, w, ne, nw, se, sw = FORMAS[forma]
    h = CELL / 2
    corta_x = any((e, w, ne, nw, se, sw))
    corta_y = any((n, s, ne, nw, se, sw))
    m = [[None] * CELL for _ in range(CELL)]
    for y in range(CELL):
        for x in range(CELL):
            if corta_x and abs(x - h) < BORDE:
                continue
            if corta_y and abs(y - h) < BORDE:
                continue
            norte, oeste = y < h, x < h
            bajo = (
                (n and norte)
                or (s and not norte)
                or (w and oeste)
                or (e and not oeste)
                or (ne and norte and not oeste)
                or (nw and norte and oeste)
                or (se and not norte and not oeste)
                or (sw and not norte and oeste)
            )
            m[y][x] = not bajo
    return m


MASCARAS = {f: mascara(f) for f in FORMAS}


def leer_hoja(numero):
    ruta = AUTOTILES / f"{HOJA_PREF}{numero}.png"
    if not ruta.exists():
        raise SystemExit(f"falta la hoja de autotile {ruta.name}")
    im = Image.open(ruta).convert("RGBA")
    esperado = (HOJA_COLS * CELL, HOJA_FILAS * CELL)
    if im.size != esperado:
        raise SystemExit(f"{ruta.name} mide {im.size}, se esperaba {esperado}")
    return im


def dominante(tile):
    """El color más repetido de una celda (ignorando lo transparente)."""
    cuenta = {}
    for p in tile.getdata():
        if p[3] > 0:
            cuenta[p[:3]] = cuenta.get(p[:3], 0) + 1
    return max(cuenta, key=cuenta.get) if cuenta else None


def celda(im, c, r):
    return im.crop((c * CELL, r * CELL, (c + 1) * CELL, (r + 1) * CELL))


def formas_de(numero, fondo_esperado):
    """`{forma: (col, row)}` de una hoja del pack, **medido**.

    El clasificador de píxel es el mismo truco que usa `import-agua.py`: el
    terreno de ABAJO es exactamente la paleta de la celda de fondo puro —el pack
    la pinta lisa— y todo lo demás (pasto, labio, sombra) es terreno alto.
    """
    im = leer_hoja(numero)
    # (1,0) es el fondo puro y (7,3) el más lleno de terreno alto: son las dos
    # celdas fijas de esta plantilla, y de ahí salen los colores a verificar.
    fondo = dominante(celda(im, 1, 0))
    alto = dominante(celda(im, 7, 3))
    if fondo != fondo_esperado:
        raise SystemExit(
            f"hoja {numero}: el fondo mide {fondo} y se esperaba {fondo_esperado} — "
            "el pack se reordenó, revisá FAMILIAS antes de importar"
        )
    if alto != VERDE:
        raise SystemExit(f"hoja {numero}: el terreno alto mide {alto} y no el verde {VERDE}")
    paleta = {p for p in celda(im, 1, 0).getdata() if p[3] > 0}

    mejor = {}
    for r in range(HOJA_FILAS):
        for c in range(HOJA_COLS):
            px = celda(im, c, r).load()
            for forma, m in MASCARAS.items():
                ok = tot = 0
                for y in range(CELL):
                    for x in range(CELL):
                        if m[y][x] is None:
                            continue
                        tot += 1
                        p = px[x, y]
                        if (p[3] > 0 and p not in paleta) == m[y][x]:
                            ok += 1
                punto = ok / tot
                if forma not in mejor or punto > mejor[forma][0]:
                    mejor[forma] = (punto, c, r)

    flojas = {f: round(p, 3) for f, (p, _, _) in mejor.items() if p < MINIMO}
    if flojas:
        raise SystemExit(f"hoja {numero}: estas formas no cierran, {flojas}")
    return im, paleta, {f: (c, r) for f, (_, c, r) in mejor.items()}


# ---------------------------------------------------------------------------
# La sombra que separa una pared de un bordillo
# ---------------------------------------------------------------------------

#: Hasta dónde llega la sombra de la pared sobre el terreno de abajo, en píxeles.
SOMBRA_PX = 7
#: Cuánto oscurece al pie del muro. 1.0 = nada.
SOMBRA_K = 0.55


def con_sombra(tile, paleta):
    """La misma celda, con la sombra del muro cayendo sobre el terreno de abajo.

    Es lo único que distingue `cliff/*` de `ledge/*`, y sale del propio dibujo:
    se marcan los píxeles del terreno BAJO que están a menos de [`SOMBRA_PX`] del
    alto y se los oscurece con una rampa. Una pared de 2,5 m proyecta sombra; un
    escalón de prado, no.
    """
    out = tile.copy()
    px = out.load()
    alto = [[px[x, y][3] > 0 and px[x, y] not in paleta for x in range(CELL)] for y in range(CELL)]
    for y in range(CELL):
        for x in range(CELL):
            if alto[y][x] or px[x, y][3] == 0:
                continue
            # Distancia (Chebyshev, que basta a esta escala) al terreno alto más
            # cercano. Sin vecino alto cerca, no hay sombra que proyectar.
            d = None
            for r in range(1, SOMBRA_PX + 1):
                if any(
                    0 <= x + dx < CELL and 0 <= y + dy < CELL and alto[y + dy][x + dx]
                    for dy in range(-r, r + 1)
                    for dx in (-r, r)
                ) or any(
                    0 <= x + dx < CELL and 0 <= y + dy < CELL and alto[y + dy][x + dx]
                    for dx in range(-r, r + 1)
                    for dy in (-r, r)
                ):
                    d = r
                    break
            if d is None:
                continue
            t = (d - 1) / SOMBRA_PX
            k = SOMBRA_K + (1.0 - SOMBRA_K) * t
            p = px[x, y]
            px[x, y] = (int(p[0] * k), int(p[1] * k), int(p[2] * k), p[3])
    return out


# ---------------------------------------------------------------------------
# Armar la hoja de salida
# ---------------------------------------------------------------------------


class Hoja:
    """Empaquetador por filas de celdas de 1x1. Sin tiras: acá nada se anima."""

    def __init__(self):
        self.im = Image.new("RGBA", (PAGINA_COLS * CELL, CELL), (0, 0, 0, 0))
        self.col = 0
        self.fila = 0

    def poner(self, tile):
        if self.col >= PAGINA_COLS:
            self.col = 0
            self.fila += 1
            nueva = Image.new("RGBA", (self.im.width, (self.fila + 1) * CELL), (0, 0, 0, 0))
            nueva.paste(self.im, (0, 0))
            self.im = nueva
        c, r = self.col, self.fila
        self.im.paste(tile, (c * CELL, r * CELL))
        self.col += 1
        return c, r


def main():
    seco = "--dry-run" in sys.argv
    pagina = Hoja()
    sprites = {}

    for material, (numero, fondo) in FAMILIAS.items():
        im, _paleta, _elegidas = formas_de(numero, fondo)
        # **La rampa es el terreno pelado.** Por el paso no hay meseta ni labio:
        # hay tierra pisada, que es el fondo puro de la propia familia. Una sola
        # pose alcanza — `ArtLibrary::tile_sprite` cae a `center` cuando la forma
        # que se le pide no existe.
        col, row = pagina.poner(celda(im, 1, 0))
        sprites[f"slope/{material}"] = {
            "png": PAGINA_REL,
            "grid": [CELL, CELL],
            "poses": {"center": {"rect": {"col": col, "row": row, "w": 1, "h": 1}}},
        }
        print(f"slope/{material}: el terreno pelado de la hoja {numero}")

    if seco:
        print(f"\n[dry-run] hoja {pagina.im.size[0]}x{pagina.im.size[1]} px, {len(sprites)} claves")
        return

    (ASSETS / "art/pack").mkdir(parents=True, exist_ok=True)
    pagina.im.save(ASSETS / PAGINA_REL, optimize=True)
    print(f"\n{PAGINA_REL}: {pagina.im.size[0]}x{pagina.im.size[1]} px")

    man = json.loads(MANIFEST.read_text())
    hoja_sprites = man.setdefault("sprites", {})
    if INDEX.exists():
        for k in json.loads(INDEX.read_text()).get("sprites", []):
            hoja_sprites.pop(k, None)
    hoja_sprites.update(sprites)

    # Nombre humano en el editor: la lista se lee "Acantilado con pasto", no
    # `cliff/pasto`. **Las etiquetas se copian de `ha-assets/src/slots.rs`**, que
    # es la fuente de verdad (biblia §8): dos nombres para la misma ranura es
    # exactamente cómo el editor termina mostrando dos cosas distintas.
    objetos = man.setdefault("objects", {})
    etiquetas = {
        "slope/pasto": "Rampa con pasto",
        "slope/arena": "Rampa de arena",
    }
    for slot, etiqueta in etiquetas.items():
        objetos.setdefault(slot, {})["label"] = etiqueta

    MANIFEST.write_text(json.dumps(man, indent=1, ensure_ascii=False, sort_keys=True))
    INDEX.write_text(json.dumps({"sprites": sorted(sprites), "pagina": PAGINA_REL}, indent=1, ensure_ascii=False))
    print(f"manifest: {len(sprites)} claves de relieve")


if __name__ == "__main__":
    main()
