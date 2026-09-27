#!/usr/bin/env python3
"""**LO QUE LE FALTABA PIXEL AL MUNDO** — los slots que no son ni item ni UI.

    python3 scripts/dibujar-mundo.py [--dry-run]

**Acá quedó SÓLO lo que no existe en ningún pack ni en el manifest.** La primera
versión de este archivo dibujaba 81 piezas; 44 de ellas ya estaban —el arte del
pack no sólo existía, estaba importado, con el nombre traducido al castellano—.
Las asigna `asignar-del-pack.py` y de acá se borraron. Lo que sobrevive es
ciencia ficción (máquinas, cohete, minerales), fauna de granja, fondos de cielo y
un puñado de piezas urbanas que LimeZu no dibujó.

El orden de la skill `pixel-art` se aplica slot por slot, y acá se ven las cuatro
puertas funcionando:

1. **Asignar lo que YA está** ([`ASIGNACIONES`]): `npc/colonist`, `npc/aldeano` y
   `npc/comerciante` no se dibujan — el juego ya tiene veinte hojas de personaje
   del pack (`art/npc/char_*.png`) y lo único que faltaba era decir cuál le toca
   a cada oficio. `player/astronaut` es alias histórico de `player/character`
   (lo dice `slots.rs`), así que apunta al mismo sitio.
2. **Derivar** — lo hicieron `atlas-suelo.py` (regolito, basalto) y
   `import-acantilado.py` (`cliff/roca`), que es de donde salieron tres de estos
   slots sin tocar este archivo.
3. **Adaptar** — no aplica acá.
4. **Dibujar**: todo lo demás, con `lib/pixel.py`.

**Un objeto del mundo NO es un icono.** A diferencia de `dibujar-items.py`, acá
cada pieza se apoya al ras de su celda de abajo y lleva la sombra de contacto del
pack ((58,58,80) a alfa 100) — es lo que la pega al suelo en vista 3/4. Lo
verifica `verificar(apoyado=True)`.
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from lib.pixel import CELDA, Lienzo, Pagina, publicar, rampa  # noqa: E402

PAGINA_REL = "art/gen/mundo.png"

# — materiales —
MADERA = (146, 104, 62)
MADERA_OSC = (110, 78, 48)
PIEDRA = (140, 140, 148)
ROCA_LUNA = (168, 168, 176)
ROCA_MARTE = (176, 108, 78)
HIERRO = (176, 182, 190)
OXIDO = (150, 92, 58)
ORO = (226, 178, 58)
METAL = (150, 158, 172)
VIDRIO = (150, 200, 220)
HOJA = (92, 148, 84)
HOJA_OSC = (62, 112, 66)
ROJO = (196, 62, 54)
AZUL = (72, 116, 196)
NARANJA = (226, 146, 52)
BLANCO = (230, 234, 240)
AGUA = (86, 150, 178)
ARENA = (222, 190, 128)
TELA = (198, 88, 76)
GRIS = (150, 150, 160)
NEGRO_AZUL = (58, 58, 80)
CESPED = (108, 168, 96)
LADRILLO = (170, 96, 80)

#: **Lo que NO se dibuja: se asigna.** `clave → (png ya existente, grid, label)`.
ASIGNACIONES = {
    "npc/colonist": ("art/npc/char_05.png", [32, 64], "Colono"),
    "npc/aldeano": ("art/npc/char_11.png", [32, 64], "Aldeano"),
    "npc/comerciante": ("art/npc/char_17.png", [32, 64], "Comerciante"),
    "player/astronaut": ("art/player/character.png", [32, 64], "Jugador con traje espacial"),
}


# — helpers de forma ————————————————————————————————————————————


def piedra(l, cx, cy, rx, ry, tonos):
    l.elipse(cx, cy, rx, ry, tonos[len(tonos) // 2])
    l.elipse(cx - rx * 0.3, cy - ry * 0.35, rx * 0.55, ry * 0.5, tonos[-1])
    l.elipse(cx + rx * 0.4, cy + ry * 0.3, rx * 0.35, ry * 0.3, tonos[1])


def veta(l, cx, cy, tonos, color):
    """Las pepitas de un mineral dentro de la roca."""
    v = rampa(color, 3)
    for dx, dy, r in [(-3, -2, 2), (2, 1, 2), (-1, 4, 1), (4, -3, 1)]:
        l.elipse(cx + dx, cy + dy, r, r, v[len(v) // 2])
        l.punto(int(cx + dx - 1), int(cy + dy - 1), v[-1])


def poste(l, x, y0, y1, tonos, ancho=3):
    l.rect(x, y0, ancho, y1 - y0, tonos[len(tonos) // 2])
    l.rect(x, y0, 1, y1 - y0, tonos[-1])
    l.rect(x + ancho - 1, y0, 1, y1 - y0, tonos[1])


def caja(l, x, y, w, h, tonos, tapa=3):
    l.rect(x, y, w, h, tonos[len(tonos) // 2])
    l.rect(x, y, w, tapa, tonos[-1])
    l.rect(x + w - 2, y, 2, h, tonos[1])


def techo(l, x, y, w, h, tonos):
    """Un techo MACIZO que se angosta un cuarto de su ancho, con alero.

    La primera versión angostaba `j*w/(2h)` por fila: con un edificio de 128 px
    de ancho eso deja la fila de arriba en 12 px, así que en pantalla el techo no
    era un volumen sino una V oscura flotando sobre la fachada."""
    medio, claro, oscuro = tonos[len(tonos) // 2], tonos[-1], tonos[1]
    retiro = max(1, w // 8)
    for j in range(h):
        k = retiro * j // max(h - 1, 1)
        l.rect(x + k, y + j, w - 2 * k, 1, claro if j < max(2, h // 3) else medio)
    # el alero: la última fila sobresale y va oscura, que es lo que separa el
    # techo de la pared cuando los dos son del mismo marrón
    l.rect(x, y + h - 2, w, 2, oscuro)


# — nature ——————————————————————————————————————————————————————


def _tronco(l):
    m = rampa(MADERA, 4)
    i = rampa((196, 164, 118), 3)
    l.sombra_al_pie(4, 24)
    l.rect(4, 17, 24, 11, m[len(m) // 2])
    l.rect(4, 17, 24, 3, m[-1])
    l.elipse(27, 22, 3, 6, i[len(i) // 2])
    l.contorno(m[0])


def _cactus(l):
    """Saguaro: tronco grueso hasta el suelo y dos brazos que salen y SUBEN. La
    primera versión los dejaba horizontales y el resultado era una cruz verde —
    la silueta es lo único que identifica a un cactus a 32 px."""
    v = rampa((86, 140, 88), 4)
    medio, claro, oscuro = v[len(v) // 2], v[-1], v[1]
    l.sombra_al_pie(10, 12)
    l.rect(13, 6, 8, 24, medio)          # tronco
    l.rect(13, 6, 2, 24, claro)
    l.rect(19, 6, 2, 24, oscuro)
    l.elipse(17, 6, 4, 3, claro)         # corona redondeada
    # brazo izquierdo: sale a los 20 y sube hasta los 12
    l.rect(7, 18, 7, 4, medio)
    l.rect(7, 11, 4, 11, medio)
    l.rect(7, 11, 2, 11, claro)
    l.elipse(8, 11, 2, 2, claro)
    # brazo derecho, más corto y más alto (los cactus no son simétricos)
    l.rect(20, 21, 6, 4, medio)
    l.rect(22, 15, 4, 10, medio)
    l.rect(22, 15, 2, 10, claro)
    l.elipse(23, 15, 2, 2, claro)
    l.contorno(v[0])


# — decor ———————————————————————————————————————————————————————


def roca(color):
    def f(l):
        t = rampa(color, 4)
        l.sombra_al_pie(6, 20)
        piedra(l, 16, 21, 11, 8, t)
        l.contorno(t[0])
    return f


def mineral(color):
    def f(l):
        t = rampa(PIEDRA, 4)
        l.sombra_al_pie(6, 20)
        piedra(l, 16, 21, 11, 8, t)
        veta(l, 16, 21, t, color)
        l.contorno(t[0])
    return f


def gema_suelo(color):
    def f(l):
        t = rampa(color, 4)
        l.sombra_al_pie(10, 12)
        l.poligono([(16, 8), (23, 18), (16, 29), (9, 18)], t[len(t) // 2])
        l.poligono([(16, 8), (23, 18), (16, 19), (9, 18)], t[-1])
        l.poligono([(16, 29), (23, 18), (16, 19)], t[1])
        l.contorno(t[0])
    return f


def _cockpit_seat(l):
    t = rampa(METAL, 4)
    c = rampa(TELA, 4)
    l.sombra_al_pie(8, 16)
    l.rect(10, 6, 12, 16, c[len(c) // 2])
    l.rect(10, 6, 12, 3, c[-1])
    l.rect(8, 20, 16, 6, t[len(t) // 2])
    l.rect(8, 20, 16, 2, t[-1])
    l.rect(11, 26, 3, 4, t[1])
    l.rect(18, 26, 3, 4, t[1])
    l.contorno(t[0])


def _fuel_source(l):
    t = rampa(METAL, 4)
    n = rampa(NARANJA, 3)
    l.sombra_al_pie(6, 20)
    l.rect(8, 8, 16, 20, t[len(t) // 2])
    l.rect(8, 8, 16, 3, t[-1])
    l.rect(21, 8, 3, 20, t[1])
    l.rect(11, 14, 10, 6, n[len(n) // 2])
    l.rect(11, 14, 10, 2, n[-1])
    l.contorno(t[0])


# — machines ————————————————————————————————————————————————————


def extractor(color, marca):
    def f(l):
        t = rampa(METAL, 4)
        c = rampa(color, 3)
        l.sombra_al_pie(4, 24)
        caja(l, 6, 12, 20, 18, t)
        l.rect(9, 16, 14, 8, c[len(c) // 2])
        l.rect(9, 16, 14, 2, c[-1])
        marca(l)
        l.contorno(t[0])
    return f


def _marca_hacha(l):
    m = rampa(MADERA, 3)
    l.banda(12, 22, 20, 17, 3, m[len(m) // 2])


def _marca_pico(l):
    g = rampa(GRIS, 3)
    l.banda(11, 21, 21, 18, 3, g[-1])


def _launch_pad(l):
    t = rampa(METAL, 4)
    n = rampa(NARANJA, 3)
    l.elipse(16, 22, 14, 8, t[len(t) // 2])
    l.elipse(16, 21, 11, 6, t[-1])
    l.elipse(16, 21, 6, 3, n[len(n) // 2])
    for dx in (-12, -6, 6, 12):
        l.rect(16 + dx, 26, 3, 4, t[1])
    l.contorno(t[0])


def _regolith_drill(l):
    t = rampa(METAL, 4)
    g = rampa(GRIS, 4)
    l.sombra_al_pie(6, 20)
    caja(l, 7, 14, 18, 16, t)
    l.poligono([(16, 2), (20, 14), (12, 14)], g[len(g) // 2])
    l.poligono([(16, 2), (18, 9), (14, 9)], g[-1])
    l.contorno(t[0])


def _habitat(l):
    t = rampa(BLANCO, 4)
    v = rampa(VIDRIO, 3)
    l.sombra_al_pie(2, 28)
    l.elipse(16, 26, 15, 15, t[len(t) // 2])
    l.rect(1, 26, 30, 5, t[len(t) // 2])
    l.elipse(13, 20, 7, 6, t[-1])
    l.elipse(16, 24, 5, 5, v[len(v) // 2])
    l.contorno(t[0])


# — agua ————————————————————————————————————————————————————————


def _junco(l):
    v = rampa((110, 150, 80), 4)
    p = rampa((150, 118, 72), 3)
    for x, h in [(9, 20), (14, 26), (20, 22), (24, 17)]:
        l.rect(x, 31 - h, 2, h, v[len(v) // 2])
        l.rect(x, 31 - h, 1, h, v[-1])
        l.elipse(x + 1, 31 - h, 2, 3, p[len(p) // 2])
    l.contorno(v[0])


def _fishing_cana(l):
    m = rampa(MADERA, 4)
    h = rampa(HIERRO, 3)
    l.banda(4, 30, 26, 4, 3, m[len(m) // 2])
    l.banda(3, 30, 25, 4, 1, m[-1])
    l.rect(8, 20, 5, 5, h[len(h) // 2])
    l.linea(26, 5, 28, 18, h[-1])
    l.contorno(m[0])


# — parcela ——————————————————————————————————————————————————————


def cancha(color, marca_y):
    def f(l):
        c = rampa(color, 4)
        b = rampa(BLANCO, 3)
        l.rect(0, 4, 32, 26, c[len(c) // 2])
        l.rect(0, 4, 32, 2, c[-1])
        l.rect(1, marca_y, 30, 1, b[-1])
        l.rect(1, 5, 1, 24, b[-1])
        l.rect(30, 5, 1, 24, b[-1])
        l.contorno(c[0])
    return f


def _cesped_cortado(l):
    v = rampa(CESPED, 4)
    l.rect(0, 0, 32, 32, v[len(v) // 2])
    for j in range(0, 32, 8):
        l.rect(0, j, 32, 4, v[-1])
    l.verificar_nada = True


# — furniture ————————————————————————————————————————————————————


def _biblioteca(l):
    m = rampa(MADERA, 4)
    libros = [ROJO, AZUL, HOJA, NARANJA]
    l.sombra_al_pie(3, 26)
    l.rect(3, 2, 26, 28, m[len(m) // 2])
    l.rect(3, 2, 26, 2, m[-1])
    for j, y in enumerate((6, 16)):
        for i, x in enumerate(range(6, 26, 5)):
            c = rampa(libros[(i + j) % 4], 3)
            l.rect(x, y, 4, 8, c[len(c) // 2])
            l.rect(x, y, 4, 2, c[-1])
    l.contorno(m[0])


# — vehicle / building / structure / varios ————————————————————————


def vehiculo(color, largo, ventana=True):
    def f(l):
        c = rampa(color, 4)
        v = rampa(VIDRIO, 3)
        n = rampa((58, 58, 70), 3)
        l.sombra_al_pie(1, 30, alto=4)
        l.rect(1, 32 - largo, 30, largo - 4, c[len(c) // 2])
        l.rect(1, 32 - largo, 30, 3, c[-1])
        l.rect(27, 32 - largo, 4, largo - 4, c[1])
        if ventana:
            l.rect(5, 32 - largo + 4, 22, 6, v[len(v) // 2])
        l.rect(3, 27, 7, 4, n[len(n) // 2])
        l.rect(22, 27, 7, 4, n[len(n) // 2])
        l.contorno(c[0])
    return f


def edificio(color, filas_ventanas, ancho=4, alto=4):
    """Un edificio de `ancho`x`alto` celdas: cuerpo, techo y filas de ventanas.
    La silueta es lo que lo identifica a distancia — la fachada fina la pone el
    pack cuando alguien le asigne una."""
    def f(l):
        c = rampa(color, 4)
        v = rampa(VIDRIO, 3)
        t = rampa((92, 72, 64), 4)
        W, H = l.w, l.h
        l.sombra_al_pie(2, W - 4, alto=4)
        l.rect(2, 10, W - 4, H - 14, c[len(c) // 2])
        l.rect(2, 10, W - 4, 4, c[-1])
        l.rect(W - 6, 10, 4, H - 14, c[1])
        techo(l, 0, 0, W, 11, t)
        for j in range(filas_ventanas):
            y = 18 + j * 14
            for x in range(7, W - 8, 12):
                l.rect(x, y, 7, 8, v[len(v) // 2])
                l.rect(x, y, 7, 2, v[-1])
        # la puerta, siempre al medio y al ras del suelo
        l.rect(W // 2 - 5, H - 14, 10, 14, t[len(t) // 2])
        l.rect(W // 2 - 5, H - 14, 10, 3, t[-1])
        l.contorno(c[0])
    return f


def _pozo(l):
    p = rampa(PIEDRA, 4)
    m = rampa(MADERA, 4)
    a = rampa((44, 60, 78), 3)
    l.sombra_al_pie(3, 26, alto=3)
    l.elipse(16, 24, 13, 7, p[len(p) // 2])
    l.elipse(16, 23, 10, 5, a[0])
    poste(l, 5, 4, 24, m)
    poste(l, 24, 4, 24, m)
    techo(l, 1, 0, 30, 8, m)
    l.contorno(p[0])


def _chest(l):
    m = rampa(MADERA, 4)
    h = rampa(ORO, 3)
    l.sombra_al_pie(4, 24)
    l.rect(4, 14, 24, 15, m[len(m) // 2])
    for j in range(6):
        k = j * 2
        l.rect(4 + k // 2, 8 + j, 24 - k, 1, m[-1] if j < 3 else m[len(m) // 2])
    l.rect(4, 18, 24, 3, h[len(h) // 2])
    l.rect(14, 17, 4, 6, h[-1])
    l.contorno(m[0])


def _room_large(l):
    p = rampa((198, 186, 168), 4)
    t = rampa((92, 72, 64), 4)
    v = rampa(VIDRIO, 3)
    W, H = l.w, l.h
    l.sombra_al_pie(2, W - 4, alto=4)
    l.rect(2, 12, W - 4, H - 16, p[len(p) // 2])
    l.rect(2, 12, W - 4, 4, p[-1])
    techo(l, 0, 0, W, 13, t)
    for x in range(8, W - 10, 16):
        l.rect(x, 22, 9, 9, v[len(v) // 2])
        l.rect(x, 22, 9, 2, v[-1])
    l.rect(W // 2 - 6, H - 18, 12, 18, t[len(t) // 2])
    l.contorno(p[0])


def _rocket(l):
    t = rampa(BLANCO, 4)
    r = rampa(ROJO, 4)
    v = rampa(VIDRIO, 3)
    f_ = rampa(METAL, 3)
    W, H = l.w, l.h
    cx = W // 2
    l.sombra_al_pie(cx - 14, 28, alto=4)
    l.poligono([(cx, 2), (cx + 11, 34), (cx - 11, 34)], t[len(t) // 2])
    l.poligono([(cx, 2), (cx + 5, 22), (cx, 22)], t[1])
    l.rect(cx - 11, 34, 22, H - 44, t[len(t) // 2])
    l.rect(cx - 11, 34, 22, 3, t[-1])
    l.rect(cx + 6, 34, 5, H - 44, t[1])
    l.elipse(cx, 30, 6, 6, v[len(v) // 2])
    l.elipse(cx - 2, 28, 3, 3, v[-1])
    l.rect(cx - 11, H - 22, 22, 5, r[len(r) // 2])
    l.poligono([(cx - 11, H - 18), (cx - 11, H - 4), (cx - 20, H - 2)], r[len(r) // 2])
    l.poligono([(cx + 11, H - 18), (cx + 11, H - 4), (cx + 20, H - 2)], r[1])
    l.poligono([(cx - 9, H - 10), (cx + 9, H - 10), (cx + 12, H - 1), (cx - 12, H - 1)], f_[len(f_) // 2])
    l.contorno(t[0])


def _subsuelo(l):
    t = rampa((92, 68, 50), 4)
    l.rect(0, 0, 32, 32, t[len(t) // 2])
    for x, y in [(4, 6), (18, 3), (11, 15), (25, 12), (6, 24), (20, 26), (28, 20)]:
        l.elipse(x, y, 3, 2, t[1])
        l.elipse(x - 1, y - 1, 1, 1, t[-1])


def fachada(color):
    def f(l):
        c = rampa(color, 4)
        v = rampa(VIDRIO, 3)
        t = rampa((92, 72, 64), 4)
        l.rect(0, 0, 32, 32, c[len(c) // 2])
        l.rect(0, 0, 32, 4, c[-1])
        l.rect(4, 8, 24, 12, v[len(v) // 2])
        l.rect(4, 8, 24, 3, v[-1])
        l.rect(10, 20, 12, 12, t[len(t) // 2])
        l.rect(10, 20, 12, 3, t[-1])
    return f


def cielo(arriba, abajo, estrellas=0):
    """Un fondo de cielo: dos bandas y, si se pide, estrellas. Va sin contorno ni
    sombra — es fondo, no objeto — y por eso `verificar(apoyado=False)`."""
    def f(l):
        a = rampa(arriba, 4)
        b = rampa(abajo, 4)
        W, H = l.w, l.h
        l.rect(0, 0, W, H // 2, a[len(a) // 2])
        l.rect(0, H // 2, W, H - H // 2, b[len(b) // 2])
        l.rect(0, H // 2 - 2, W, 4, a[1] if luz_menor(a, b) else b[-1])
        if estrellas:
            c = rampa(BLANCO, 3)
            paso = max(3, (W * H) // max(estrellas, 1))
            k = 0
            for y in range(1, H, 5):
                for x in range((y * 7) % 11, W, 9):
                    if k % 2 == 0:
                        l.punto(x, y, c[-1])
                    k += 1
    return f


def luz_menor(a, b):
    from lib.pixel import luz as _l

    return _l(a[len(a) // 2]) < _l(b[len(b) // 2])


# — fauna ————————————————————————————————————————————————————————


def animal(cuerpo_c, alto, patas_c=None, mancha=None, cresta=None):
    """Un animal de granja en vista 3/4, de perfil: cuerpo, cabeza, cuatro patas
    y su sombra. Se distinguen por SILUETA y color — a 32 px un cerdo y una oveja
    sólo se diferencian por eso."""
    def f(l):
        c = rampa(cuerpo_c, 4)
        p = rampa(patas_c or cuerpo_c, 4)
        base = 30
        l.sombra_al_pie(5, 22, alto=3)
        l.elipse(15, base - alto // 2 - 3, 10, alto // 2 + 1, c[len(c) // 2])
        l.elipse(13, base - alto - 1, 8, alto // 2 - 1, c[-1])
        l.elipse(25, base - alto - 2, 5, 5, c[len(c) // 2])   # cabeza
        l.elipse(24, base - alto - 4, 3, 2, c[-1])
        for x in (8, 13, 19, 23):
            l.rect(x, base - 5, 3, 5, p[1])
        if mancha:
            mm = rampa(mancha, 3)
            l.elipse(13, base - alto + 1, 4, 3, mm[len(mm) // 2])
            l.elipse(20, base - alto // 2 - 4, 3, 2, mm[len(mm) // 2])
        if cresta:
            cc = rampa(cresta, 3)
            l.rect(24, base - alto - 8, 4, 3, cc[len(cc) // 2])
        l.contorno(c[0])
    return f


# — el catálogo ————————————————————————————————————————————————————

#: `clave → (label, receta, ancho_celdas, alto_celdas, apoyado)`
RECETAS = {
    # nature
    "nature/tronco": ("Tronco caído", _tronco, 1, 1, True),
    "nature/cactus": ("Cactus", _cactus, 1, 1, True),
    # decor
    "decor/rock_moon": ("Roca lunar", roca(ROCA_LUNA), 1, 1, True),
    "decor/rock_mars": ("Roca marciana", roca(ROCA_MARTE), 1, 1, True),
    "decor/ore_iron": ("Veta de hierro", mineral(OXIDO), 1, 1, True),
    "decor/ore_gold": ("Veta de oro", mineral(ORO), 1, 1, True),
    "decor/gem_red": ("Gema roja", gema_suelo(ROJO), 1, 1, True),
    "decor/gem_blue": ("Gema azul", gema_suelo(AZUL), 1, 1, True),
    "decor/cockpit_seat": ("Asiento de cabina", _cockpit_seat, 1, 1, True),
    "decor/fuel_source": ("Depósito de combustible", _fuel_source, 1, 1, True),
    # machines
    "machines/wood_extractor": ("Extractor de madera", extractor(MADERA, _marca_hacha), 1, 1, True),
    "machines/stone_extractor": ("Extractor de piedra", extractor(PIEDRA, _marca_pico), 1, 1, True),
    "machines/launch_pad": ("Plataforma de lanzamiento", _launch_pad, 1, 1, True),
    "machines/regolith_drill": ("Perforadora de regolito", _regolith_drill, 1, 1, True),
    "machines/habitat": ("Hábitat", _habitat, 1, 1, True),
    # agua
    "agua/junco": ("Juncos", _junco, 1, 1, True),
    "fishing/caña": ("Caña de pescar", _fishing_cana, 1, 1, True),
    # parcela
    "prop/cancha_basket": ("Cancha de básquet", cancha((186, 118, 78), 17), 1, 1, True),
    "prop/cancha_tenis": ("Cancha de tenis", cancha((78, 128, 96), 17), 1, 1, True),
    "prop/cesped_cortado": ("Césped cortado", _cesped_cortado, 1, 1, True),
    "prop/pozo": ("Pozo", _pozo, 1, 1, True),
    "props/chest": ("Cofre", _chest, 1, 1, True),
    # furniture
    "furniture/biblioteca": ("Biblioteca", _biblioteca, 1, 1, True),
    # vehicle
    "vehicle/taxi": ("Taxi", vehiculo((234, 190, 62), 22), 1, 1, True),
    # structure / build / room / ship
    "room/room_large": ("Cuarto grande", _room_large, 4, 3, True),
    "ship/rocket": ("Cohete", _rocket, 3, 4, True),
    "building/bomberos": ("Cuartel de bomberos", edificio((176, 82, 70), 1), 4, 3, True),
    "building/base_militar": ("Base militar", edificio((116, 124, 96), 1), 4, 3, True),
    "tiles/subsuelo": ("Subsuelo", _subsuelo, 1, 1, False),
    # backgrounds
    "backgrounds/sky_day": ("Cielo de día", cielo((122, 178, 226), (186, 214, 236)), 4, 2, False),
    "backgrounds/sky_night": ("Cielo de noche", cielo((28, 32, 62), (58, 62, 96), 40), 4, 2, False),
    "backgrounds/stars": ("Estrellas", cielo((18, 20, 38), (24, 26, 48), 90), 4, 2, False),
    # fauna — vaca, gallina, oveja, cerdo y perro salen de scripts/fauna3d.py
}


def main():
    seco = "--dry-run" in sys.argv
    pagina = Pagina()
    sprites, objetos = {}, {}
    fallos = []
    for clave, (label, receta, w, h, apoyado) in RECETAS.items():
        l = Lienzo(CELDA * w, CELDA * h)
        receta(l)
        try:
            l.verificar(apoyado=apoyado)
        except SystemExit as e:
            fallos.append((clave, str(e)))
            continue
        col, row = pagina.poner(l.im)
        sprites[clave] = {
            "png": PAGINA_REL,
            "grid": [CELDA, CELDA],
            "rect": {"col": col, "row": row, "w": w, "h": h},
        }
        objetos[clave] = label
    for clave, (png, grid, label) in ASIGNACIONES.items():
        sprites[clave] = {"png": png, "grid": grid, "rect": {"col": 0, "row": 0, "w": 1, "h": 1}}
        objetos[clave] = label
    if fallos:
        print(f"⚠ {len(fallos)} piezas no pasaron el guardián:")
        for k, e in fallos:
            print(f"   {k}: {e}")
    publicar(pagina, sprites, objetos, PAGINA_REL, seco)
    print(f"{len(RECETAS) - len(fallos)} dibujadas + {len(ASIGNACIONES)} asignadas del pack")
    return 1 if fallos else 0


if __name__ == "__main__":
    sys.exit(main())
