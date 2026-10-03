#!/usr/bin/env python3
"""**DIBUJAR PIXEL ART QUE NO SE DISTINGA DEL PACK.**

Es la librería que usa todo script que genere un PNG para el juego. No es un
motor de dibujo: es un **corsé**. Cada primitiva pasa el color por la paleta
medida del pack y [`Lienzo.verificar`] se planta si la pieza rompe alguna de las
reglas que se midieron sobre el arte de LimeZu.

## De dónde salen las reglas (ninguna se eligió a ojo)

Medidas sobre 250-400 singles de 32x32 de Modern Exteriors y sobre las 168
páginas del pack que el juego dibuja hoy (75,9 millones de píxeles):

| regla | medición |
|---|---|
| grilla de 32x32 | la del pack; mezclar 16 está prohibido (biblia §8) |
| **9 colores** por sprite (p90 16, máx 30) | mediana de 300 singles |
| **sin dithering** | 0,0 % de píxeles en damero — mediana Y p90 |
| **sin contorno negro** | 237 de 250 sprites tienen el contorno de VARIOS colores: el borde es el tono oscuro del propio material, no una línea key |
| contorno 18 de luminancia por debajo del interior | media de 250 sprites |
| **paso de 12 de luminancia** entre tonos vecinos | mediana (p25 6, p75 18) |
| el tono más oscuro va **tintado**, nunca gris | 246 de 250 sprites |
| saturación mediana **0,28** | 250 sprites; el pack es apagado, no vivo |
| luz **suave** de arriba-izquierda: +5 arriba, +4 izquierda | media de 400 sprites |
| sombra de contacto en **(58,58,80) a alfa 100/255** | 104 de 400 sprites la traen; es EL color, no un negro |
| el objeto se apoya **al ras del borde de abajo** | mediana de 0 filas vacías debajo |
| ocupa ~48 % de la celda | mediana (p10 14 %, p90 100 %) |

La paleta es `paleta_limezu.json`: los **501 colores que cubren el 95 % de los
píxeles** que el juego dibuja. No es una lámina de referencia — es lo que está en
pantalla. Se regenera con `python3 scripts/lib/pixel.py --paleta`.

## Cómo se usa

```python
from lib.pixel import Lienzo, rampa

l = Lienzo(32, 32)
r = rampa((150, 110, 70), 4)          # 4 tonos del pack cerca de ese marrón
l.caja(8, 14, 16, 18, r)              # relleno + tapa clara + contorno propio
l.sombra_al_pie(8, 24)
l.verificar()                          # se planta si rompe una regla
l.guardar("art/gen/items/wood.png")
```
"""

from __future__ import annotations

import colorsys
import json
import sys
from pathlib import Path

from PIL import Image

RAIZ = Path(__file__).resolve().parent.parent.parent
PALETA_JSON = Path(__file__).resolve().parent / "paleta_limezu.json"

#: Lado de la celda del pack. Mezclar 16 rompe la grilla del editor.
CELDA = 32

#: **La sombra de contacto del pack.** No es negro ni un gris: es este azul
#: violáceo desaturado, y siempre a alfa 100 de 255. Medido en 104 de 400
#: singles: 3.136 píxeles de este color y ninguno de otro.
SOMBRA = (58, 58, 80)
SOMBRA_ALFA = 100

#: Paso de luminancia entre dos tonos vecinos de una rampa. Mediana medida.
PASO_LUZ = 12.0

#: Techo de colores por sprite de 32x32. El máximo observado en 300 singles fue
#: 30; la mediana, 9. Pasarse no es "más detalle", es dejar de parecer del pack.
COLORES_MAX = 30

#: Cuánto más oscuro que su interior va el contorno, en luminancia. Medido: 18.
#: **No es una línea negra** — es el tono oscuro del propio material.
CONTORNO_DELTA = 18.0


def luz(c) -> float:
    """Luminancia perceptual de un color."""
    return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]


def _cargar_paleta() -> list[tuple[int, int, int]]:
    d = json.loads(PALETA_JSON.read_text())
    return [tuple(c) for c in d["nucleo95"]]


PALETA = _cargar_paleta()


def _dist(a, b) -> float:
    """Distancia de color con el verde pesando como lo ve el ojo."""
    dr, dg, db = a[0] - b[0], a[1] - b[1], a[2] - b[2]
    return 2.0 * dr * dr + 4.0 * dg * dg + 3.0 * db * db


def ajustar(c) -> tuple[int, int, int]:
    """**El color más cercano DEL PACK.** Es la puerta por la que pasa todo:
    ningún píxel que escriba esta librería puede tener un color que el pack no
    haya usado nunca."""
    c = tuple(int(max(0, min(255, q))) for q in c[:3])
    if c in _CACHE_AJUSTE:
        return _CACHE_AJUSTE[c]
    mejor = min(PALETA, key=lambda p: _dist(p, c))
    _CACHE_AJUSTE[c] = mejor
    return mejor


_CACHE_AJUSTE: dict = {}


def rampa(base, n: int = 4) -> list[tuple[int, int, int]]:
    """**n tonos del pack alrededor de `base`, de oscuro a claro.**

    No interpola en RGB: pide a la paleta el color más parecido a `base` en cada
    escalón de luminancia, así que los tonos salen con el desvío de tono que el
    pack le da a sus sombras (el tono oscuro TINTADO, medido en 246 de 250
    sprites) y no un gris multiplicado.

    Los escalones van de [`PASO_LUZ`] en [`PASO_LUZ`], que es la mediana medida
    entre dos tonos vecinos de un sprite del pack.
    """
    base = ajustar(base)
    l0 = luz(base)
    h, s, _ = colorsys.rgb_to_hsv(*[q / 255 for q in base])
    fuera = []
    for i in range(n):
        # el índice n-1 es el propio `base`; los anteriores bajan de a PASO_LUZ
        objetivo = l0 - PASO_LUZ * (n - 1 - i)
        objetivo = max(14.0, min(246.0, objetivo))
        v = max(0.04, min(1.0, objetivo / max(l0, 1.0))) * _v(base)
        pedido = colorsys.hsv_to_rgb(h, s, min(1.0, v))
        cand = tuple(round(q * 255) for q in pedido)
        # entre los colores del pack, el más cercano EN TONO que caiga cerca de
        # la luminancia pedida
        elegido = min(
            PALETA,
            key=lambda p: _dist(p, cand) + 60.0 * abs(luz(p) - objetivo),
        )
        fuera.append(elegido)
    # sin repetidos: si la paleta no tiene un tono ahí, se corre al siguiente
    limpio = []
    for c in fuera:
        if c not in limpio:
            limpio.append(c)
    while len(limpio) < n:
        limpio.append(limpio[-1])
    return sorted(limpio, key=luz)


def _v(c) -> float:
    return colorsys.rgb_to_hsv(*[q / 255 for q in c])[2]


class Lienzo:
    """Un PNG en construcción. Todo color pasa por [`ajustar`]."""

    def __init__(self, ancho: int = CELDA, alto: int = CELDA):
        self.w, self.h = ancho, alto
        self.im = Image.new("RGBA", (ancho, alto), (0, 0, 0, 0))
        self.px = self.im.load()

    # — primitivas ————————————————————————————————————————————————

    def punto(self, x: int, y: int, c, alfa: int = 255):
        if 0 <= x < self.w and 0 <= y < self.h:
            self.px[x, y] = (*ajustar(c), alfa)

    def rect(self, x: int, y: int, w: int, h: int, c, alfa: int = 255):
        for j in range(y, y + h):
            for i in range(x, x + w):
                self.punto(i, j, c, alfa)

    def linea(self, x0: int, y0: int, x1: int, y1: int, c, alfa: int = 255):
        n = max(abs(x1 - x0), abs(y1 - y0)) or 1
        for k in range(n + 1):
            self.punto(round(x0 + (x1 - x0) * k / n), round(y0 + (y1 - y0) * k / n), c, alfa)

    def elipse(self, cx: float, cy: float, rx: float, ry: float, c, alfa: int = 255):
        for j in range(self.h):
            for i in range(self.w):
                if ((i + 0.5 - cx) / rx) ** 2 + ((j + 0.5 - cy) / ry) ** 2 <= 1.0:
                    self.punto(i, j, c, alfa)

    def banda(self, x0, y0, x1, y1, grosor: float, c, alfa: int = 255):
        """**Una línea GRUESA y maciza**, no varias de un píxel.

        Existe porque el guardián de dithering cazó el error opuesto: tres líneas
        diagonales de 1 px con tonos distintos hacen que cada píxel tenga arriba
        y abajo el mismo color ajeno, o sea un damero — que es exactamente lo que
        el pack no hace. Un palo del pack es una banda de un tono con su filo
        claro al lado, y eso es esto.
        """
        import math

        dx, dy = x1 - x0, y1 - y0
        n = math.hypot(dx, dy) or 1.0
        px, py = -dy / n * grosor / 2, dx / n * grosor / 2
        self.poligono(
            [(x0 + px, y0 + py), (x1 + px, y1 + py), (x1 - px, y1 - py), (x0 - px, y0 - py)],
            c,
            alfa,
        )

    def poligono(self, puntos, c, alfa: int = 255):
        """Relleno por regla par-impar, sin anti-alias (el pack no tiene)."""
        ys = [p[1] for p in puntos]
        for j in range(int(min(ys)), int(max(ys)) + 1):
            cortes = []
            for k in range(len(puntos)):
                (x0, y0), (x1, y1) = puntos[k], puntos[(k + 1) % len(puntos)]
                if (y0 <= j < y1) or (y1 <= j < y0):
                    cortes.append(x0 + (j - y0) * (x1 - x0) / (y1 - y0))
            cortes.sort()
            for a, b in zip(cortes[0::2], cortes[1::2]):
                for i in range(int(round(a)), int(round(b)) + 1):
                    self.punto(i, j, c, alfa)

    # — el idioma del pack ————————————————————————————————————————

    def volumen(self, x: int, y: int, w: int, h: int, tonos, tapa: int = 1):
        """**Un cuerpo con el sombreado del pack**: relleno medio, `tapa` filas
        claras arriba (la luz viene de arriba, +5 medido) y la columna de la
        derecha un tono más oscura (+4 a favor de la izquierda). Sin degradado:
        el pack cambia de tono de golpe, no interpola."""
        medio = tonos[len(tonos) // 2]
        claro = tonos[-1]
        oscuro = tonos[max(0, len(tonos) // 2 - 1)]
        self.rect(x, y, w, h, medio)
        if tapa:
            self.rect(x, y, w, min(tapa, h), claro)
        self.rect(x + w - 1, y, 1, h, oscuro)

    def contorno(self, tono_oscuro=None):
        """**EL PACK NO CONTORNEA. Esto oscurece el lado en sombra, nada más.**

        Ésta es la función que más veces se escribió mal, y las dos veces por
        confiar en una medida promediada:

        1. Primera versión: pintar TODO el perímetro de un solo tono. Es una línea
           key, el sprite lee como calcomanía y se distingue del pack a un metro.
        2. Segunda: bajarle 18 de luminancia a cada píxel del borde, porque
           `mean(interior) - mean(contorno)` daba 18 en 250 singles. **Ese 18 es un
           artefacto**: los bordes caen más seguido en las partes bajas y laterales,
           que ya son las oscuras del objeto. Medido como corresponde —cada píxel
           de borde contra su vecino INTERIOR, 26.327 pares— la mediana del salto
           es **0**, y en el **94 %** de los pares el borde es exactamente el mismo
           color que el interior.

        O sea que LimeZu **no dibuja contorno**: la silueta es donde se termina el
        material. Lo que sí hace es tener el lado en sombra más oscuro, porque la
        luz le viene de arriba-izquierda (+5 arriba, +4 izquierda, medido). Eso es
        lo único que hace esto ahora: un escalón de rampa más oscuro en el borde de
        ABAJO y de la DERECHA, y el de arriba-izquierda intacto.

        `tono_oscuro` es el tono al que tirar; sin él se busca en la paleta el
        color más parecido un escalón por debajo.
        """
        respaldo = ajustar(tono_oscuro) if tono_oscuro is not None else None
        marca = []
        for j in range(self.h):
            for i in range(self.w):
                if self.px[i, j][3] < 255:
                    continue
                # sólo el borde que da al SUR o al ESTE: el lado en sombra
                for a, b in ((i + 1, j), (i, j + 1)):
                    if not (0 <= a < self.w and 0 <= b < self.h) or self.px[a, b][3] == 0:
                        marca.append((i, j))
                        break
        for i, j in marca:
            c = self.px[i, j][:3]
            objetivo = luz(c) - PASO_LUZ
            if objetivo < 16.0:
                if respaldo is not None:
                    self.px[i, j] = (*respaldo, 255)
                continue
            elegido = min(
                PALETA,
                key=lambda p: _dist(p, c) + 90.0 * abs(luz(p) - objetivo),
            )
            self.px[i, j] = (*elegido, 255)

    def sombra_al_pie(self, x: int, w: int, y: int | None = None, alto: int = 3):
        """La sombra de contacto del pack: elipse achatada bajo el objeto, en
        [`SOMBRA`] a alfa [`SOMBRA_ALFA`]. Va DEBAJO de lo ya dibujado."""
        y = self.h - alto if y is None else y
        base = Image.new("RGBA", (self.w, self.h), (0, 0, 0, 0))
        b = base.load()
        cx, cy, rx, ry = x + w / 2, y + alto / 2, w / 2, alto / 2
        for j in range(self.h):
            for i in range(self.w):
                if ((i + 0.5 - cx) / max(rx, 0.5)) ** 2 + ((j + 0.5 - cy) / max(ry, 0.5)) ** 2 <= 1:
                    b[i, j] = (*SOMBRA, SOMBRA_ALFA)
        base.alpha_composite(self.im)
        self.im = base
        self.px = self.im.load()

    # — el guardián ————————————————————————————————————————————————

    def verificar(self, apoyado: bool = True, max_colores: int = COLORES_MAX):
        """Se planta si la pieza no se puede confundir con una del pack.

        Cada regla es una de las medidas del encabezado del módulo. No es
        cosmético: un sprite con 60 colores, con dithering o con contorno negro
        se ve de otro juego apenas cae al lado de un asset de LimeZu.
        """
        d = list(self.im.getdata())
        op = [p for p in d if p[3] > 0]
        if not op:
            raise SystemExit("el lienzo quedó vacío")
        cs = {p[:3] for p in op}
        if len(cs) > max_colores:
            raise SystemExit(f"{len(cs)} colores y el pack no pasa de {max_colores} (mediana 9)")
        fuera = [c for c in cs if c not in set(PALETA)]
        if fuera:
            raise SystemExit(f"{len(fuera)} colores fuera de la paleta del pack, p.ej. {fuera[:3]}")
        alfas = {p[3] for p in op} - {255, SOMBRA_ALFA}
        if alfas:
            raise SystemExit(f"alfas {sorted(alfas)}: el pack sólo usa 255 y {SOMBRA_ALFA} (sombra)")
        # negro: el tono más oscuro va tintado (246 de 250 sprites)
        for c in cs:
            if max(c) < 30 and max(c) - min(c) <= 6:
                raise SystemExit(f"{c} es negro neutro; el pack tinta hasta su sombra")
        if self._dithering() > 0.02:
            raise SystemExit("hay damero (dithering) y el pack no usa: medido 0,0 %")
        if apoyado:
            ys = [j for j in range(self.h) for i in range(self.w) if self.px[i, j][3] > 0]
            if self.h - 1 - max(ys) > 2:
                raise SystemExit(
                    f"queda{'n' if self.h-1-max(ys)>1 else ''} {self.h-1-max(ys)} filas vacías "
                    "abajo: el objeto del pack se apoya al ras"
                )

    def _dithering(self) -> float:
        d = t = 0
        for j in range(1, self.h - 1):
            for i in range(1, self.w - 1):
                if self.px[i, j][3] != 255:
                    continue
                t += 1
                a, b = self.px[i - 1, j], self.px[i + 1, j]
                c, e = self.px[i, j - 1], self.px[i, j + 1]
                if (
                    a[3] == b[3] == c[3] == e[3] == 255
                    and a[:3] == b[:3] != self.px[i, j][:3]
                    and c[:3] == e[:3] != self.px[i, j][:3]
                ):
                    d += 1
        return d / max(t, 1)

    def guardar(self, ruta):
        ruta = Path(ruta)
        ruta.parent.mkdir(parents=True, exist_ok=True)
        self.im.save(ruta)
        return ruta


class Pagina:
    """**Una sola hoja para todo lo dibujado**, como hacen los importadores del
    pack: 32 columnas de ancho y las filas que hagan falta. Un atlas es un draw
    call; 142 PNGs sueltos son 142 texturas."""

    COLS = 32

    def __init__(self):
        self.im = Image.new("RGBA", (self.COLS * CELDA, CELDA), (0, 0, 0, 0))
        self.col = 0
        self.fila = 0
        self.alto_fila = 0

    def poner(self, im: Image.Image) -> tuple[int, int]:
        w, h = im.width // CELDA, im.height // CELDA
        if self.col + w > self.COLS:
            self.fila += self.alto_fila
            self.col = 0
            self.alto_fila = 0
        self.alto_fila = max(self.alto_fila, h)
        alto_px = (self.fila + self.alto_fila) * CELDA
        if alto_px > self.im.height:
            nueva = Image.new("RGBA", (self.im.width, alto_px), (0, 0, 0, 0))
            nueva.paste(self.im, (0, 0))
            self.im = nueva
        c, r = self.col, self.fila
        self.im.paste(im, (c * CELDA, r * CELDA))
        self.col += w
        return c, r


def publicar(pagina: Pagina, sprites: dict, objetos: dict, png_rel: str, seco: bool = False):
    """Escribe la hoja y mete sus claves en `art/manifest.json`.

    Respeta lo que ya está: sólo toca las claves que se le pasan, así que un
    slot con arte curado en el editor no se pisa nunca. Es el mismo contrato que
    `import-acantilado.py`.
    """
    assets = RAIZ / "native/crates/ha-game/assets"
    manifest = assets / "art/manifest.json"
    if seco:
        print(f"[dry-run] {len(sprites)} claves, hoja {pagina.im.size}")
        return
    destino = assets / png_rel
    destino.parent.mkdir(parents=True, exist_ok=True)
    pagina.im.save(destino)
    man = json.loads(manifest.read_text())
    man.setdefault("sprites", {}).update(sprites)
    man.setdefault("objects", {})
    for k, label in objetos.items():
        man["objects"].setdefault(k, {})["label"] = label
    manifest.write_text(json.dumps(man, ensure_ascii=False, indent=1))
    print(f"{png_rel}: {pagina.im.width}x{pagina.im.height} px · {len(sprites)} claves al manifest")


def _regenerar_paleta():
    """`python3 scripts/lib/pixel.py --paleta` — vuelve a medir la paleta desde
    las páginas del pack que el juego dibuja hoy."""
    import glob
    from collections import Counter

    cnt: Counter = Counter()
    paginas = sorted(glob.glob(str(RAIZ / "native/crates/ha-game/assets/art/pack/*.png")))
    for f in paginas:
        for p in Image.open(f).convert("RGBA").getdata():
            if p[3] > 200:
                cnt[p[:3]] += 1
    tot = sum(cnt.values())
    acum, nucleo = 0, []
    for c, n in cnt.most_common():
        nucleo.append(c)
        acum += n
        if acum / tot > 0.95:
            break
    PALETA_JSON.write_text(
        json.dumps(
            {
                "fuente": "colores de art/pack/*.png — lo que el juego dibuja hoy",
                "paginas": len(paginas),
                "colores_totales": len(cnt),
                "nucleo95": [list(c) for c in nucleo],
            },
            indent=1,
        )
    )
    print(f"{len(paginas)} páginas · {len(cnt)} colores · núcleo del 95 %: {len(nucleo)}")


if __name__ == "__main__":
    if "--paleta" in sys.argv:
        _regenerar_paleta()
    else:
        print(f"paleta: {len(PALETA)} colores · celda {CELDA}px · sombra {SOMBRA}@{SOMBRA_ALFA}")
