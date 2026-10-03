#!/usr/bin/env python3
"""**LO QUE EL JUGADOR TIENE EN LA MANO MIENTRAS HACE ALGO** — capas que van encima de la hoja del
personaje, cuadro a cuadro, sincronizadas con su fila de animación.

La hoja del generador de LimeZu ya trae el cuerpo de 19 animaciones (quieto, caminar, celular, leer,
juntar, golpear, disparar…), pero hay tres cosas que no trae y el juego necesita:

* **El libro** de la fila 7 (leer): el pack lo vende como CAPA aparte (`Books/`), no en el cuerpo. El
  chargen no la compone, así que se importa tal cual (`art/pack/jugador_libro.png`). Esto NO se dibuja.
* **Comer y beber**: el pack no tiene. Se dibujan acá sobre la pose de «manos en el pecho» de la fila 7
  (la de leer, sin libro): la comida sube hasta la boca, pierde un mordisco y baja; la taza sube, se
  inclina (se le ve el culo) y baja. 12 cuadros = dos ciclos de los 6 del cuerpo.
* **Las ondas al nadar**: el cuerpo se recorta a la cintura en el juego (`pose_jugador.rs`) y esto es el
  anillo de agua que lo rodea, 4 cuadros.

Todo se dibuja a la grilla REAL del personaje: el pack de 32x32 es arte de 16 px escalado x2 (cada píxel
del cuerpo es un bloque de 2x2 — medido), así que se pinta en 16 y se duplica con vecino más cercano. Un
objeto pintado a 1 px de 32 se vería más fino que la mano que lo sostiene.

Posiciones MEDIDAS sobre `Bodies/Body_32x32_01.png` fila 7 (en la escala de 16): las manos cruzan el pecho
en la fila 22, el mentón termina en la 21, la cabeza sube 1 px en los cuadros 2, 3, 8 y 9 (el libro del
pack no se mueve: las manos quedan quietas). El contorno es (58,58,80), el mismo del libro del pack.

    python3 scripts/dibujar-acciones-jugador.py           # escribe PNGs + manifest
    python3 scripts/dibujar-acciones-jugador.py --ver     # además deja una lámina de control
"""
from __future__ import annotations

import sys
from pathlib import Path

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib.pixel import Lienzo, Pagina, ajustar, publicar, rampa  # noqa: E402

RAIZ = Path(__file__).resolve().parent.parent
PACK = RAIZ / "PRUEBAS-CODIGOS/Character_Generator"
ASSETS = RAIZ / "native/crates/ha-game/assets"

CW, CH = 32, 64          # celda del personaje
FILA_LEER = 7            # fila de la hoja con las manos en el pecho
CUADROS = 12             # dos ciclos de los 6 del cuerpo
FPS_MANO = 8.0           # comer/beber: 1,5 s por bocado doble
TINTA = (58, 58, 80)     # contorno del libro del pack

#: Subida de la cabeza por cuadro (escala 16), medida: el bbox del cuerpo arranca 2 px más arriba.
BOB = [0, 0, 1, 1, 0, 0, 0, 0, 1, 1, 0, 0]


def _x2(l: Lienzo) -> Image.Image:
    return l.im.resize((l.w * 2, l.h * 2), Image.NEAREST)


def _contorno(l: Lienzo):
    """Contorno de 1 px (de 16) por fuera de la silueta, como el libro y el celular del pack."""
    t = ajustar(TINTA)
    marca = []
    for j in range(l.h):
        for i in range(l.w):
            if l.px[i, j][3]:
                continue
            if any(0 <= a < l.w and 0 <= b < l.h and l.px[a, b][3] == 255
                   for a, b in ((i + 1, j), (i - 1, j), (i, j + 1), (i, j - 1))):
                marca.append((i, j))
    for i, j in marca:
        l.px[i, j] = (*t, 255)


# ── comida: un pancito relleno (lee como «comida» sea lo que sea que comiste) ────────────────────

PAN = rampa((196, 134, 70), 5)       # corteza, de oscuro a claro
MIGA = rampa((236, 212, 160), 3)
HOJA = rampa((112, 168, 72), 3)      # la lechuga que asoma: es lo que dice «sándwich» a 6 px


def _pancito(l: Lienzo, x: int, y: int, mordidas: int):
    """6x4 (escala 16) con la esquina de arriba a la derecha comida `mordidas` veces."""
    # cuerpo: tapa clara arriba-izquierda, base oscura abajo-derecha (luz del pack)
    forma = [
        "  1221",
        "122233",
        "4hhhh4",
        " 3443 ",
    ]
    tono = {"1": PAN[4], "2": PAN[3], "3": PAN[2], "4": PAN[1], "h": HOJA[1]}
    for j, fila in enumerate(forma):
        for i, ch in enumerate(fila):
            if ch != " ":
                l.punto(x + i, y + j, tono[ch])
    l.punto(x + 1, y + 2, HOJA[2])   # brillo de la hoja del lado de la luz
    # mordidas: se come de arriba a la derecha hacia el centro; la miga queda a la vista
    bocados = [[(5, 0), (5, 1), (4, 0)], [(4, 1), (3, 0), (5, 2)], [(3, 1), (2, 0), (4, 2)]]
    for k in range(min(mordidas, len(bocados))):
        for (i, j) in bocados[k]:
            l.px[x + i, y + j] = (0, 0, 0, 0)
    if mordidas:
        # borde de la mordida: la miga clara donde se cortó
        for (i, j) in bocados[min(mordidas, len(bocados)) - 1]:
            for a, b in ((i - 1, j), (i, j + 1)):
                if 0 <= x + a < l.w and l.px[x + a, y + b][3]:
                    l.punto(x + a, y + b, MIGA[2] if b == y else MIGA[1])


def comer() -> list[Image.Image]:
    """Sube, muerde, baja — dos veces. El cuadro 3 y el 9 son el mordisco (la cabeza sube a buscarlo)."""
    # altura del pan (fila de arriba, escala 16) en cada cuadro del ciclo de 6
    alto = [23, 21, 19, 19, 21, 23]
    out = []
    for f in range(CUADROS):
        ciclo, k = divmod(f, 6)
        mord = ciclo + (1 if k >= 3 else 0)
        l = Lienzo(CW // 2, CH // 2)
        y = alto[k] - BOB[f]
        _pancito(l, 5, y, mord)
        _contorno(l)
        if k == 3:  # migas que caen del mordisco
            l.punto(11, y + 2, MIGA[1])
            l.punto(12, y + 4, PAN[2])
        if k == 4:
            l.punto(12, y + 5, MIGA[1])
        l.verificar(apoyado=False)
        out.append(_x2(l))
    return out


# ── beber: una taza de barro con el líquido a la vista ───────────────────────────────────────────

TAZA = rampa((196, 108, 86), 5)
LIQ = rampa((92, 146, 196), 3)


def _taza(l: Lienzo, x: int, y: int, inclinada: bool, nivel: int):
    """4x5 + asa. Derecha: se ve la boca con el líquido. Inclinada: se ve el culo (y el asa arriba)."""
    if not inclinada:
        forma = [
            "1LL2 ",
            "12234",
            "12234",
            "1223 ",
            " 33  ",
        ]
    else:
        forma = [
            " 11  ",
            "12234",
            "12234",
            "22334",
            " 44  ",
        ]
    tono = {"1": TAZA[4], "2": TAZA[3], "3": TAZA[2], "4": TAZA[1], "L": LIQ[2 if nivel else 1]}
    for j, fila in enumerate(forma):
        for i, ch in enumerate(fila):
            if ch == " ":
                continue
            # el asa (columna 4) se dibuja hueca: sólo su borde
            if i == 4 and j == 2:
                continue
            l.punto(x + i, y + j, tono[ch])
    if not inclinada:
        l.punto(x + 1, y + 1, TAZA[4])        # brillo en el labio, del lado de la luz
    else:
        l.punto(x + 1, y + 4, TAZA[2])        # la base: el anillo del culo


def beber() -> list[Image.Image]:
    alto = [23, 21, 18, 18, 21, 23]
    out = []
    for f in range(CUADROS):
        ciclo, k = divmod(f, 6)
        l = Lienzo(CW // 2, CH // 2)
        y = alto[k] - BOB[f]
        _taza(l, 6, y, inclinada=k in (2, 3), nivel=1 - ciclo)
        _contorno(l)
        l.verificar(apoyado=False)
        out.append(_x2(l))
    return out


# ── nadar: el anillo de agua alrededor de la cintura ─────────────────────────────────────────────

AGUA = rampa((150, 210, 226), 4)
ONDA_W, ONDA_H = 24, 8   # escala 16 → 48x16


def ondas() -> list[Image.Image]:
    """Dos anillos en perspectiva 3/4 que se abren, con huecos que giran (no un óvalo liso)."""
    import math

    out = []
    for f in range(4):
        l = Lienzo(ONDA_W, ONDA_H)
        for anillo, (rx, ry, tono) in enumerate(((6.5 + f * 0.9, 1.6 + f * 0.25, AGUA[3]),
                                                  (9.5 + f * 0.9, 2.4 + f * 0.25, AGUA[2]))):
            cx, cy = ONDA_W / 2, ONDA_H / 2
            pasos = 64
            for s in range(pasos):
                a = s / pasos * math.tau
                # huecos: dos tramos apagados que rotan con el cuadro → se lee movimiento
                fase = (a + f * 0.6 + anillo * 1.3) % math.tau
                if fase < 0.7 or 3.1 < fase < 3.6:
                    continue
                x = int(round(cx + rx * math.cos(a) - 0.5))
                y = int(round(cy + ry * math.sin(a) - 0.5))
                # la mitad de arriba del anillo queda detrás del cuerpo: más oscura
                c = tono if math.sin(a) > 0 else AGUA[1]
                l.punto(x, y, c)
        # espuma: un brillo blanco que salta de lado en lado
        bx = (5, 17, 8, 15)[f]
        l.punto(bx, ONDA_H // 2 + 1, (240, 248, 255))
        l.verificar(apoyado=False)
        out.append(_x2(l))
    return out


# ── el libro del pack (no se dibuja: se importa) ─────────────────────────────────────────────────


def libro() -> Image.Image:
    hoja = Image.open(PACK / "Books/32x32/Book_32x32_01.png").convert("RGBA")
    if hoja.size != (1792, 1312):
        raise SystemExit(f"Book_32x32_01.png mide {hoja.size}: el pack cambió, revisar la fila {FILA_LEER}")
    tira = hoja.crop((0, FILA_LEER * CH, CUADROS * CW, (FILA_LEER + 1) * CH))
    if tira.getbbox() is None:
        raise SystemExit("la fila del libro vino vacía")
    return tira


def _tira(cuadros: list[Image.Image], w: int, h: int) -> Image.Image:
    t = Image.new("RGBA", (w * len(cuadros), h), (0, 0, 0, 0))
    for i, c in enumerate(cuadros):
        t.alpha_composite(c, (i * w, 0))
    return t


def _pose(fila: int, cuadros: int, fps: float) -> dict:
    return {"anim": {"fps": fps, "frames": cuadros}, "dir": "Right",
            "rect": {"col": 0, "h": 1, "row": fila, "w": 1}}


def _lamina(mano: Image.Image, onda: Image.Image, libro_t: Image.Image, destino: Path):
    """Cada capa encima del cuerpo real del pack, ampliada x4, sobre pasto: así se mira."""
    cuerpo = Image.open(PACK / "Bodies/32x32/Body_32x32_01.png").convert("RGBA")
    ropa = sorted((PACK / "Outfits/32x32").glob("*.png"))
    traje = Image.open(ropa[0]).convert("RGBA") if ropa else None
    filas = 4
    L = Image.new("RGBA", (CUADROS * CW, CH * filas), (96, 128, 72, 255))
    for f in range(CUADROS):
        base = cuerpo.crop((f % 6 * CW, FILA_LEER * CH, (f % 6 + 1) * CW, (FILA_LEER + 1) * CH))
        if traje is not None and traje.size == cuerpo.size:
            base.alpha_composite(traje.crop((f % 6 * CW, FILA_LEER * CH, (f % 6 + 1) * CW, (FILA_LEER + 1) * CH)))
        for r in range(2):
            c = base.copy()
            c.alpha_composite(mano.crop((f * CW, r * CH, (f + 1) * CW, (r + 1) * CH)))
            L.alpha_composite(c, (f * CW, r * CH))
        c = cuerpo.crop((f * CW, FILA_LEER * CH, (f + 1) * CW, (FILA_LEER + 1) * CH))
        c.alpha_composite(libro_t.crop((f * CW, 0, (f + 1) * CW, CH)))
        L.alpha_composite(c, (f * CW, 2 * CH))
    for f in range(4):
        andar = cuerpo.crop((f * CW, 2 * CH, (f + 1) * CW, 2 * CH + 40))
        x0 = f * 3 * CW
        L.paste((70, 120, 170, 255), (x0, 3 * CH, x0 + 3 * CW, 4 * CH))
        L.alpha_composite(andar, (x0 + CW, 3 * CH + 8))
        L.alpha_composite(onda.crop((f * 48, 0, (f + 1) * 48, 16)), (x0 + CW - 8, 3 * CH + 8 + 40 - 8))
    L.resize((L.width * 4, L.height * 4), Image.NEAREST).save(destino)
    print(f"lámina: {destino}")


def main():
    ver = "--ver" in sys.argv
    mano = Image.new("RGBA", (CUADROS * CW, 2 * CH), (0, 0, 0, 0))
    mano.alpha_composite(_tira(comer(), CW, CH), (0, 0))
    mano.alpha_composite(_tira(beber(), CW, CH), (0, CH))
    onda = _tira(ondas(), ONDA_W * 2, ONDA_H * 2)
    libro_t = libro()

    for im, rel, sprites, objetos in (
        (mano, "art/gen/jugador/en_mano.png",
         {"player/en_mano": {"grid": [CW, CH], "png": "art/gen/jugador/en_mano.png",
                             "rect": {"col": 0, "h": 1, "row": 0, "w": 1},
                             "poses": {"comer": _pose(0, CUADROS, FPS_MANO), "beber": _pose(1, CUADROS, FPS_MANO)}}},
         {"player/en_mano": "Lo que el jugador come o bebe"}),
        (onda, "art/gen/jugador/ondas.png",
         {"player/ondas": {"grid": [ONDA_W * 2, ONDA_H * 2], "png": "art/gen/jugador/ondas.png",
                           "rect": {"col": 0, "h": 1, "row": 0, "w": 1},
                           "poses": {"nadar": _pose(0, 4, 6.0)}}},
         {"player/ondas": "Ondas del jugador al nadar"}),
        (libro_t, "art/pack/jugador_libro.png",
         {"player/libro": {"grid": [CW, CH], "png": "art/pack/jugador_libro.png",
                           "rect": {"col": 0, "h": 1, "row": 0, "w": 1},
                           "poses": {"leer": _pose(0, CUADROS, 6.0)}}},
         {"player/libro": "Libro en la mano del jugador"}),
    ):
        p = Pagina()
        p.im = im
        publicar(p, sprites, objetos, rel)
    if ver:
        _lamina(mano, onda, libro_t, Path(sys.argv[sys.argv.index("--ver") + 1]) if len(sys.argv) > sys.argv.index("--ver") + 1 else Path("/tmp/acciones-jugador.png"))


if __name__ == "__main__":
    main()
