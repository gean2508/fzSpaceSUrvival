#!/usr/bin/env python3
"""**LA GRANJA** — la tierra arada, los cultivos maduros y sus ítems.

    python3 scripts/dibujar-granja.py [--dry-run]

## Qué se buscó antes de dibujar (skill `pixel-art`, puerta 0-3)

- **Brotes chicos y medianos: SÍ están en el pack** (`nature/brote_chico_*`,
  `nature/brote_mediano_*`, `nature/tierra_22/24` — brotecitos con la raíz
  naranja de una zanahoria). Esas etapas NO se dibujan: las apunta
  `asignar-del-pack.py` a los slots `cultivo/<planta>_1/_2`.
- **Lo que no está**: tierra arada con surcos, trigo maduro, zanahoria lista para
  arrancar y una calabaza de huerta. Las del pack (`prop/calabaza_*`) son todas
  de Halloween — tienen cara tallada — y una calabaza con cara creciendo en la
  huerta se lee como decorado, no como cosecha.
- Ítems: semillas, trigo, zanahoria y calabaza no existen como icono en los
  tres packs ni en las 13.413 claves del manifest (buscado en inglés y en
  castellano: `trigo`, `wheat`, `semilla`, `seed`, `zanahoria`, `carrot`).
- El corazón del celo de los animales tampoco.

Todo con `lib/pixel.py` (paleta del pack, sin contorno, sin dithering).
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from lib.pixel import CELDA, Lienzo, Pagina, publicar, rampa  # noqa: E402

PAGINA_REL = "art/gen/granja.png"

TIERRA = (126, 90, 60)
TIERRA_MOJADA = (104, 72, 50)
TRIGO = (214, 176, 92)
HOJA = (92, 150, 76)
ZANAHORIA = (222, 124, 48)
CALABAZA = (220, 132, 52)
TALLO = (120, 150, 70)
SEMILLA = (200, 170, 110)
SEMILLA_CALABAZA = (226, 214, 170)
CORAZON = (210, 70, 80)


def tierra_arada(base):
    """Una losa de tierra removida: el relleno medio y tres surcos, cada uno una
    loma clara arriba y su canaleta oscura abajo (la luz viene de arriba). Llena
    la celda entera — es suelo, no un objeto."""
    def f(l):
        t = rampa(base, 4)
        l.rect(0, 0, CELDA, CELDA, t[1])
        for y in (2, 12, 22):
            l.rect(1, y, CELDA - 2, 6, t[2])
            l.rect(1, y, CELDA - 2, 2, t[3])
            l.rect(1, y + 6, CELDA - 2, 2, t[0])
    return f, False


def sembrado(l):
    """Semillas asomando en los surcos: puntitos claros de a dos (un grano y su
    sombra), sobre fondo transparente — va encima de la tierra arada."""
    t = rampa(SEMILLA, 3)
    for y in (6, 16, 26):
        for x in (6, 15, 24):
            l.rect(x, y, 2, 1, t[2])
            l.rect(x, y + 1, 2, 1, t[0])


def trigo_maduro(l):
    """Una mata de espigas doradas: cinco tallos rectos y la espiga gorda arriba
    de cada uno. Apoyada al ras, sin sombra (va sobre la tierra arada)."""
    t = rampa(TRIGO, 4)
    tallo = rampa((170, 150, 80), 3)
    for x, top in ((6, 9), (11, 5), (16, 3), (21, 6), (26, 10)):
        l.rect(x, top + 8, 2, 32 - top - 8, tallo[1])
        l.rect(x - 1, top, 4, 9, t[2])
        l.rect(x - 1, top, 2, 9, t[3])
        l.rect(x + 2, top + 1, 1, 8, t[1])
    l.contorno(t[0])


def zanahoria_madura(l):
    """Tres manojos de hojas y el hombro naranja de cada raíz asomando de la
    tierra: es la señal de "lista" que se ve desde lejos."""
    h = rampa(HOJA, 4)
    z = rampa(ZANAHORIA, 3)
    for cx in (8, 16, 24):
        l.elipse(cx, 29, 4, 3, z[1])
        l.rect(cx - 2, 26, 4, 2, z[2])
        l.poligono([(cx - 1, 26), (cx - 6, 14), (cx - 3, 12), (cx, 24)], h[1])
        l.poligono([(cx, 26), (cx, 10), (cx + 2, 10), (cx + 2, 26)], h[2])
        l.poligono([(cx + 1, 26), (cx + 6, 15), (cx + 4, 13), (cx, 24)], h[1])
        l.rect(cx, 10, 1, 6, h[3])
    l.contorno(h[0])


def calabaza(l, cx=16, cy=24, rx=11, ry=7, hojas=True):
    """Calabaza de huerta: cuerpo achatado con dos gajos marcados (bandas
    oscuras verticales), la luz arriba-izquierda, el cabito verde."""
    t = rampa(CALABAZA, 4)
    v = rampa(TALLO, 3)
    if hojas:
        l.elipse(cx - 9, cy - 3, 5, 3, v[1])
        l.elipse(cx + 10, cy - 4, 5, 3, v[1])
    l.elipse(cx, cy, rx, ry, t[1])
    l.elipse(cx - 3, cy - 2, rx - 5, ry - 3, t[2])
    l.elipse(cx - 5, cy - 3, 3, 2, t[3])
    for dx in (-5, 4):
        l.rect(cx + dx, cy - ry + 2, 1, ry * 2 - 3, t[0])
    l.rect(cx - 1, cy - ry - 3, 3, 4, v[1])
    l.rect(cx - 1, cy - ry - 3, 1, 4, v[2])


def calabaza_madura(l):
    calabaza(l)
    l.contorno(rampa(CALABAZA, 4)[0])


# — íconos (centrados, no apoyados) ——————————————————————————————————


def icono_semillas(color):
    def f(l):
        t = rampa(color, 3)
        for cx, cy in ((11, 12), (19, 10), (15, 18), (22, 19), (9, 21), (17, 25)):
            l.elipse(cx, cy, 3, 2, t[1])
            l.rect(cx - 2, cy - 1, 2, 1, t[2])
        l.contorno(t[0])
    return f, True


def icono_trigo(l):
    t = rampa(TRIGO, 4)
    tallo = rampa((170, 150, 80), 3)
    # atado: tres espigas en abanico con un lazo
    l.banda(16, 28, 9, 6, 3, tallo[1])
    l.banda(16, 28, 16, 4, 3, tallo[1])
    l.banda(16, 28, 23, 6, 3, tallo[1])
    for x0, y0 in ((9, 4), (16, 2), (23, 4)):
        l.elipse(x0, y0 + 5, 3, 6, t[2])
        l.elipse(x0 - 1, y0 + 4, 1.5, 4, t[3])
    l.rect(12, 19, 9, 3, rampa((170, 110, 70), 3)[1])
    l.contorno(t[0])


def icono_zanahoria(l):
    z = rampa(ZANAHORIA, 4)
    h = rampa(HOJA, 3)
    l.poligono([(9, 12), (15, 9), (25, 27), (23, 28)], z[2])
    l.poligono([(9, 12), (12, 10), (21, 26), (23, 28)], z[3])
    for x0, y0 in ((6, 3), (10, 2), (13, 5)):
        l.banda(x0, y0, 11, 11, 2, h[1])
    l.contorno(z[0])


def icono_calabaza(l):
    calabaza(l, cx=16, cy=18, rx=11, ry=8, hojas=False)
    l.contorno(rampa(CALABAZA, 4)[0])


def corazon(l):
    t = rampa(CORAZON, 3)
    l.elipse(12, 12, 5, 5, t[1])
    l.elipse(20, 12, 5, 5, t[1])
    l.poligono([(7, 13), (25, 13), (16, 24)], t[1])
    l.elipse(11, 11, 2, 2, t[2])
    l.contorno(t[0])


# — la granja animal (29 sep 2026): lo que el pack no trae ————————————————


def icono_pollo(l):
    """Pata de pollo cruda: el muslo rosado y el hueso blanco asomando."""
    c = rampa((226, 170, 150), 4)
    h = rampa((236, 230, 214), 3)
    l.elipse(13, 13, 8, 7, c[2])
    l.elipse(11, 11, 4, 3, c[3])
    l.banda(17, 18, 25, 26, 3, h[1])
    l.elipse(26, 26, 2.5, 2.5, h[2])
    l.elipse(24, 28, 2, 2, h[2])
    l.contorno(c[0])


def icono_conejo(l):
    """Corte de conejo: un lomo largo y pálido."""
    c = rampa((214, 150, 140), 4)
    l.elipse(16, 17, 11, 6, c[2])
    l.elipse(13, 15, 6, 3, c[3])
    l.rect(8, 19, 16, 1, c[1])
    l.contorno(c[0])


def icono_pluma(l):
    t = rampa((236, 232, 222), 4)
    canon = rampa((200, 186, 160), 3)
    l.poligono([(9, 26), (20, 6), (25, 9), (13, 27)], t[2])
    l.poligono([(9, 26), (20, 6), (21, 8), (11, 26)], t[3])
    l.banda(7, 28, 22, 8, 1.5, canon[1])
    l.contorno(t[0])


def icono_lana(l):
    t = rampa((236, 232, 218), 4)
    for cx, cy, r in ((12, 18, 7), (20, 17, 7), (16, 12, 6), (16, 21, 7)):
        l.elipse(cx, cy, r, r * 0.85, t[2])
    for cx, cy in ((12, 15), (18, 10), (21, 15)):
        l.elipse(cx, cy, 3, 2, t[3])
    l.contorno(t[0])


def icono_hueso(l):
    t = rampa((232, 226, 206), 4)
    l.banda(9, 23, 23, 9, 4, t[2])
    for cx, cy in ((7, 23), (9, 26), (23, 6), (26, 9)):
        l.elipse(cx, cy, 3, 3, t[2])
    l.banda(9, 22, 22, 9, 1.5, t[3])
    l.contorno(t[0])


def icono_huevo(l):
    t = rampa((238, 226, 200), 4)
    l.elipse(16, 17, 7, 9, t[2])
    l.elipse(14, 13, 3, 4, t[3])
    l.contorno(t[0])


def icono_etiqueta(l):
    """La etiqueta de nombre: cartón con ojal y el hilo."""
    t = rampa((214, 186, 130), 4)
    hilo = rampa((150, 104, 66), 3)
    l.poligono([(8, 14), (13, 9), (26, 9), (26, 23), (13, 23)], t[2])
    l.rect(13, 10, 12, 2, t[3])
    l.elipse(12, 16, 1.5, 1.5, hilo[0])
    l.banda(4, 8, 11, 15, 1.5, hilo[1])
    for y in (15, 18):
        l.rect(16, y, 8, 1, t[1])
    l.contorno(t[0])


PIEZAS = {
    # clave: (receta, apoyado, nombre)
    "cultivo/tierra_arada": (*tierra_arada(TIERRA), "Tierra arada"),
    "cultivo/tierra_regada": (*tierra_arada(TIERRA_MOJADA), "Tierra arada (húmeda)"),
    "cultivo/sembrado": (sembrado, False, "Semillas sembradas"),
    "cultivo/trigo_3": (trigo_maduro, True, "Trigo maduro"),
    "cultivo/zanahoria_3": (zanahoria_madura, True, "Zanahorias listas"),
    "cultivo/calabaza_3": (calabaza_madura, True, "Calabaza madura"),
    "items/wheat_seeds": (*icono_semillas(SEMILLA), "Semillas de trigo"),
    "items/pumpkin_seeds": (*icono_semillas(SEMILLA_CALABAZA), "Semillas de calabaza"),
    "items/wheat": (icono_trigo, False, "Trigo"),
    "items/carrot": (icono_zanahoria, False, "Zanahoria"),
    "items/pumpkin": (icono_calabaza, False, "Calabaza"),
    "ui/corazon_celo": (corazon, False, "Corazón (animal en celo)"),
    "items/chicken": (icono_pollo, False, "Pollo crudo"),
    "items/rabbit_meat": (icono_conejo, False, "Carne de conejo"),
    "items/feather": (icono_pluma, False, "Pluma"),
    "items/wool": (icono_lana, False, "Lana"),
    "items/bone": (icono_hueso, False, "Hueso"),
    "items/egg": (icono_huevo, False, "Huevo"),
    "items/name_tag": (icono_etiqueta, False, "Etiqueta"),
}


#: Etapas de brote que el pack SÍ trae — no se dibujan, se apuntan. Elegidas
#: mirando la lámina: el trigo es un pastito que se endereza, la zanahoria son
#: los brotecitos con raíz naranja de `tierra_*`, la calabaza una guía de hojas.
#: (Viven acá y no en `asignar-del-pack.py` porque son del mismo sistema que lo
#: dibujado: si se re-dibuja la granja, se re-apuntan juntas.)
DEL_PACK = {
    "cultivo/trigo_1": ("nature/brote_chico_4", "Trigo (brote)"),
    "cultivo/trigo_2": ("nature/brote_mediano_2", "Trigo (creciendo)"),
    "cultivo/zanahoria_1": ("nature/tierra_22", "Zanahoria (brote)"),
    "cultivo/zanahoria_2": ("nature/tierra_24", "Zanahoria (creciendo)"),
    "cultivo/calabaza_1": ("nature/brote_chico_1", "Calabaza (brote)"),
    "cultivo/calabaza_2": ("nature/brote_chico_5", "Calabaza (creciendo)"),
}


def apuntar_del_pack(seco):
    import json

    from lib.pixel import RAIZ

    ruta = RAIZ / "native/crates/ha-game/assets/art/manifest.json"
    man = json.loads(ruta.read_text())
    sp = man["sprites"]
    faltan = [o for o, _ in DEL_PACK.values() if o not in sp]
    if faltan:
        raise SystemExit(f"el pack cambió: faltan {faltan}")
    if seco:
        return
    for slot, (origen, nombre) in DEL_PACK.items():
        sp[slot] = json.loads(json.dumps(sp[origen]))
        man.setdefault("objects", {}).setdefault(slot, {})["label"] = nombre
    ruta.write_text(json.dumps(man, ensure_ascii=False, indent=1))


def main():
    seco = "--dry-run" in sys.argv
    pag = Pagina()
    sprites, objetos = {}, {}
    for clave, (receta, apoyado, nombre) in PIEZAS.items():
        l = Lienzo()
        receta(l)
        # la tierra arada es una losa de suelo: llena la celda y no se "apoya"
        l.verificar(apoyado=apoyado and clave.startswith("cultivo/") and "tierra" not in clave)
        c, r = pag.poner(l.im)
        sprites[clave] = {"grid": [CELDA, CELDA], "png": PAGINA_REL, "rect": {"col": c, "row": r, "w": 1, "h": 1}}
        objetos[clave] = nombre
    publicar(pag, sprites, objetos, PAGINA_REL, seco)
    apuntar_del_pack(seco)
    return 0


if __name__ == "__main__":
    sys.exit(main())
