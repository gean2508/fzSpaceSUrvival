#!/usr/bin/env python3
"""**Lo que se PINTA sobre el asfalto y todavía no estaba: la bahía de BUS y el
cajón de estacionamiento.**

    python3 scripts/import-marcas-calzada.py --dry-run   # qué encontró y dónde
    python3 scripts/import-marcas-calzada.py             # escribe el manifest

## Por qué

La calzada ya lleva su línea divisoria y su senda peatonal
(`scripts/import-calles.py`), pero el pack trae dos marcas más que el juego nunca
usó y que son justo las que hacen que una calle se lea como una calle de ciudad:

- **La bahía de BUS** — el recuadro amarillo con `BUS` escrito adentro, donde el
  colectivo se sale del carril para parar. El pack la dibujó en dos versiones: la
  vertical (para una calle norte-sur) y la horizontal.
- **El cajón de estacionamiento** — la `P` con sus líneas blancas, en batería
  (perpendicular al cordón) y en fila (paralelo).

Las dos van **sobre la CALZADA**, no sobre la vereda: son pintura vial. Es lo que
quedó anotado pendiente en biblia §13.

## Cómo se recortan, sin escribir un solo número a ojo

Los rectángulos se **MIDEN** (regla 7): no hay una tabla de celdas copiada de
mirar la hoja.

1. Se buscan las manchas de AMARILLO VIAL de la hoja (`amarillo`, un color que en
   este pack sólo usa la bahía) y se agrupan por conexidad. Cada mancha grande es
   el marco de una bahía; su caja, ajustada a la grilla de 32 px, es la bahía.
2. Para el estacionamiento se busca el glifo `P` —una mancha clara del tamaño de
   una letra— y desde ahí se crece hasta las LÍNEAS BLANCAS que delimitan el
   cajón. El cajón es lo que queda entre línea y línea.
3. De cada rectángulo se publica **una pose por tile** (`ns_0_0`, `ns_1_0`, …),
   porque así es como el mundo lo dibuja: una calcomanía por celda de calzada,
   igual que la línea y la senda.

## Calcomanía, no losa

Igual que `road/linea`, cada tile se recorta **contra el asfalto liso de su
familia**: lo que es asfalto se va a alfa y queda sólo la pintura. Sin esto, la
bahía traería su propio gris y taparía la variante de asfalto de esas dieciocho
celdas — el mismo parche rectangular que ya se había reportado con la línea.

Anota lo suyo en `scripts/marcas-calzada-import-index.json`, así una segunda
corrida reemplaza lo suyo y nada más.
"""

import argparse
import json
import os
import re
from collections import deque

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
HOJA = os.path.join(
    ROOT,
    "PRUEBAS-CODIGOS/modernexteriors-win/Modern_Exteriors_32x32/ME_Theme_Sorter_32x32",
    "2_City_Terrains_32x32.png",
)
SINGLES = os.path.join(
    ROOT,
    "PRUEBAS-CODIGOS/modernexteriors-win/Modern_Exteriors_32x32/ME_Theme_Sorter_32x32",
    "2_City_Terrains_Singles_32x32",
)
ART = os.path.join(ROOT, "native/crates/ha-game/assets/art")
MANIFEST = os.path.join(ART, "manifest.json")
PAGINA_REL = "art/pack/marcas.png"
ROADS_RS = os.path.join(ROOT, "native/crates/ha-shared/src/roads.rs")
INDEX = os.path.join(ROOT, "scripts/marcas-calzada-import-index.json")
CELL = 32

#: Los slots. La bahía y el cajón son dos redes distintas —una la usa el
#: colectivo y el otro el que estaciona— así que son dos slots, no dos poses.
SLOT_BAHIA = "road/bahia_bus"
SLOT_CAJON = "road/cajon"

#: Grosor máximo, en píxeles, de una línea PINTADA. Más gruesa que esto y no es
#: pintura: es la baldosa de la vereda. Medido sobre la hoja: las líneas de cajón
#: miden 1–2 px y la franja de vereda que se colaba medía 12.
GRUESO_LINEA = 3

#: Píxeles de margen alrededor de la caja de pintura. La sombra de contacto que el
#: pack dibuja al pie de una línea cae justo afuera del trazo y sin esto se
#: perdía: la pintura quedaba pegada al asfalto como una calcomanía de papel.
MARGEN_PINTURA = 2


def amarillo(p):
    """El amarillo vial del pack: naranja saturado, sin nada de azul."""
    r, g, b, a = p
    return a > 128 and r > 170 and g > 120 and b < 90 and r - b > 90


def claro(p):
    """Blanco de pintura: los tres canales altos y parejos."""
    r, g, b, a = p
    return a > 128 and min(r, g, b) > 150


def manchas(px, x0, y0, x1, y1, test):
    """Componentes conexas de los píxeles que pasan `test`, por caja."""
    dentro = set()
    for y in range(y0, y1):
        for x in range(x0, x1):
            if test(px[x, y]):
                dentro.add((x, y))
    vistos, out = set(), []
    for p in dentro:
        if p in vistos:
            continue
        cola, comp = deque([p]), []
        vistos.add(p)
        while cola:
            x, y = cola.popleft()
            comp.append((x, y))
            for dx in (-1, 0, 1):
                for dy in (-1, 0, 1):
                    n = (x + dx, y + dy)
                    if n in dentro and n not in vistos:
                        vistos.add(n)
                        cola.append(n)
        xs = [q[0] for q in comp]
        ys = [q[1] for q in comp]
        out.append(((min(xs), min(ys), max(xs) + 1, max(ys) + 1), len(comp)))
    out.sort(key=lambda t: -t[1])
    return out


def a_celdas(caja):
    """La caja de píxeles, ajustada hacia afuera a la grilla de 32."""
    x0, y0, x1, y1 = caja
    return (x0 // CELL, y0 // CELL, -(-x1 // CELL), -(-y1 // CELL))


def bahias(px, ancho, alto):
    """Las bahías de BUS: las manchas grandes de amarillo, por orientación.

    Devuelve `{"ns": (col,row,w,h), "ew": (col,row,w,h)}` en celdas. La
    orientación sale de la CAJA (más alta que ancha = calle norte-sur), no de un
    nombre: el pack no numera estas piezas.
    """
    out = {}
    for caja, n in manchas(px, 0, 0, ancho, alto, amarillo):
        if n < 600:
            continue  # las letras sueltas y los bordes de un carril
        c0, r0, c1, r1 = a_celdas(caja)
        w, h = c1 - c0, r1 - r0
        if w < 3 or h < 3:
            continue
        eje = "ns" if h > w else "ew"
        if eje in out:
            continue  # el pack repite la misma bahía por familia de vereda
        out[eje] = ((c0, r0, w, h), caja)
    return out


def franja_v(px, c, r):
    """Desplazamiento de la LÍNEA blanca vertical de esta celda, si la hay.

    "Línea" quiere decir una columna de píxeles claros de arriba abajo de la
    celda: es lo que separa un cajón del de al lado y lo que cierra el bloque
    contra la calzada. Buscarla así —y no por brillo medio— es lo que la
    distingue de la baldosa de la vereda, que también es clara pero no forma una
    columna de un solo píxel de ancho.
    """
    for x in range(c * CELL, (c + 1) * CELL):
        if all(claro(px[x, y]) for y in range(r * CELL, (r + 1) * CELL)):
            return x - c * CELL
    return None


def franja_h(px, c, r):
    """Ídem, la línea horizontal."""
    for y in range(r * CELL, (r + 1) * CELL):
        if all(claro(px[x, y]) for x in range(c * CELL, (c + 1) * CELL)):
            return y - r * CELL
    return None


def glifos_p(px, ancho, alto):
    """Las celdas donde el pack escribió una `P` sobre el asfalto.

    Una `P` es una mancha clara del tamaño de una letra; lo que la separa de
    cualquier otro detalle claro del pack es que tiene asfalto —oscuro y opaco—
    arriba o abajo. Encuentra 45 en la hoja: las tres familias de vereda × dos
    bancos × sus cajones.
    """

    def asfalto(c, r):
        """Asfalto de verdad: oscuro, opaco y **GRIS**.

        Lo de gris no es adorno. La misma hoja trae, más abajo, las zonas rayadas
        de VERDE (carril de bici, dársena de carga) con sus propios glifos —un
        auto, una bici, el símbolo de accesibilidad— del mismo tamaño que una `P`.
        Sin pedir gris, el detector se traía esos bancos como si fueran cajones de
        estacionamiento.
        """
        if c < 0 or r < 0 or c >= ancho // CELL or r >= alto // CELL:
            return False
        opaco = gris = 0
        for y in range(r * CELL, (r + 1) * CELL, 2):
            for x in range(c * CELL, (c + 1) * CELL, 2):
                p = px[x, y]
                if p[3] < 128:
                    continue
                opaco += 1
                if max(p[0], p[1], p[2]) < 150 and max(p[:3]) - min(p[:3]) < 26:
                    gris += 1
        return opaco > 180 and gris / max(opaco, 1) > 0.6

    out = set()
    for caja, n in manchas(px, 0, 0, ancho, alto, claro):
        x0, y0, x1, y1 = caja
        if not (200 <= n <= 900 and 12 <= x1 - x0 <= 30 and 18 <= y1 - y0 <= 40):
            continue
        c, r = (x0 + x1) // 2 // CELL, (y0 + y1) // 2 // CELL
        if asfalto(c, r + 1) or asfalto(c, r - 1):
            out.add((c, r))
    return sorted(out)


def cajones(px, ancho, alto, fondo_px, ruido):
    """Los cajones de estacionamiento, medidos por el `P` y por las líneas.

    Dos medidas, cada una donde es fiable:

    - **El lado del banco**, del PASO entre dos `P` vecinas. Tres `P` seguidas en
      la misma fila cada tres celdas dicen que el cajón mide tres de ancho, sin
      tener que encontrar ninguna línea. Es el lado por el que se repite el
      cajón.
    - **El lado del fondo**, de las LÍNEAS blancas que cierran el banco contra la
      calzada. Se busca la primera fila (o columna) del banco que tenga una línea
      de lado a lado; ésa es la que lo cierra, y entra al recorte porque la línea
      está dibujada sobre ella.

    Se toma UNO de cada orientación: el pack repite el mismo banco una vez por
    familia de vereda y son el mismo dibujo.
    """
    cols, rows = ancho // CELL, alto // CELL
    ps = glifos_p(px, ancho, alto)
    por_fila, por_col = {}, {}
    for c, r in ps:
        por_fila.setdefault(r, []).append(c)
        por_col.setdefault(c, []).append(r)

    def paso(vals):
        vals = sorted(vals)
        difs = [b - a for a, b in zip(vals, vals[1:]) if 2 <= b - a <= 8]
        return min(difs) if difs else None

    out = {}
    # BATERÍA: varias `P` seguidas en la misma fila. El cajón es tan ancho como el
    # paso y tan hondo como el banco.
    for r, cs in sorted(por_fila.items()):
        w = paso(cs)
        if w is None:
            continue
        c0 = min(cs) - (w - 1) // 2
        r0 = r
        while r0 > 0 and not any(franja_h(px, c, r0) is not None for c in range(c0, c0 + w)):
            r0 -= 1
        r1 = r
        while r1 + 1 < rows and not any(
            franja_h(px, c, r1) is not None for c in range(c0, c0 + w)
        ):
            r1 += 1
        h = r1 - r0 + 1
        if 2 <= h <= 8 and "ns" not in out:
            out["ns"] = ((c0, r0, w, h), caja_de_pintura(px, c0, r0, w, h, fondo_px, ruido))
    # FILA (estacionamiento paralelo al cordón): varias `P` en la misma columna.
    for c, rs in sorted(por_col.items()):
        h = paso(rs)
        if h is None:
            continue
        r0 = min(rs) - (h - 1) // 2
        c0 = c
        while c0 > 0 and not any(franja_v(px, c0, r) is not None for r in range(r0, r0 + h)):
            c0 -= 1
        c1 = c
        while c1 + 1 < cols and not any(
            franja_v(px, c1, r) is not None for r in range(r0, r0 + h)
        ):
            c1 += 1
        w = c1 - c0 + 1
        if 2 <= w <= 8 and "ew" not in out:
            out["ew"] = ((c0, r0, w, h), caja_de_pintura(px, c0, r0, w, h, fondo_px, ruido))
    return out


def caja_de_pintura(px, c0, r0, w, h, fondo_px, ruido):
    """La caja de PÍXELES de las líneas blancas de un banco de cajones.

    Hace falta porque el rectángulo de CELDAS es más grande que el dibujo: el
    banco arranca a media celda de su primera fila, y la mitad de arriba de esa
    celda es la VEREDA. Recortada tal cual, cada bahía y cada cajón se llevaban
    puesta una franja de baldosa que después aparecía flotando en mitad de la
    calzada.

    La caja son las líneas que cierran el banco (las mismas que lo midieron), no
    todo lo claro que haya adentro.
    """
    px0, py0 = c0 * CELL, r0 * CELL
    px1, py1 = (c0 + w) * CELL, (r0 + h) * CELL

    def difiere(a, b):
        return max(abs(a[i] - b[i]) for i in range(3)) > ruido

    def no_es_calzada(pixeles):
        """¿Esta fila (o columna) NO es asfalto pintado, sino otra cosa entera?

        Una fila de calzada es asfalto salvo en los pocos píxeles donde pasa un
        trazo pintado. Una fila de VEREDA, o la del CORDÓN que la levanta, no
        coincide con el asfalto en ningún píxel. Ésa es la regla, y caza las dos
        con la misma medida: la banda clara de la baldosa y la banda oscura y
        lisa del cordón, que si se colaban terminaban dibujadas por segunda vez
        en mitad de la calzada (el juego ya pone su propio cordón).
        """
        n = sum(1 for p in pixeles if difiere(px[p], fondo_px[p[0] % CELL, p[1] % CELL]))
        return n > len(pixeles) * 0.8

    # Se pela desde el borde: mientras la primera fila (o columna) del rectángulo
    # sea una banda clara ENTERA, es vereda y no marca. Se para en cuanto aparece
    # asfalto, así que el trazo pintado que está a un par de píxeles del borde no
    # se toca. El tope es para no comerse la marca si algo sale mal.
    tope = GRUESO_LINEA * 6
    while py0 < py1 - tope and no_es_calzada([(x, py0) for x in range(px0, px1)]):
        py0 += 1
    while py1 > py0 + tope and no_es_calzada([(x, py1 - 1) for x in range(px0, px1)]):
        py1 -= 1
    while px0 < px1 - tope and no_es_calzada([(px0, y) for y in range(py0, py1)]):
        px0 += 1
    while px1 > px0 + tope and no_es_calzada([(px1 - 1, y) for y in range(py0, py1)]):
        px1 -= 1
    return (px0, py0, px1, py1)


def asfalto_liso():
    """La losa de asfalto lisa del pack: el fondo contra el que se recorta.

    Es la misma que usa `import-calles.py` para el cordón, y por el mismo motivo:
    la pintura está dibujada ENCIMA de esta losa pixel por pixel, así que borrar
    lo idéntico deja exactamente la pintura, con su antialias y su sombra.
    """
    mejor = None
    for f in sorted(os.listdir(SINGLES)):
        if "Asphalt" not in f:
            continue
        im = Image.open(os.path.join(SINGLES, f)).convert("RGBA")
        if im.size != (CELL, CELL):
            continue
        px = im.load()
        vals = [sum(px[x, y][:3]) / 3 for y in range(CELL) for x in range(CELL)]
        var = max(vals) - min(vals)
        if mejor is None or var < mejor[0]:
            mejor = (var, im)
    if mejor is None:
        raise SystemExit("no encontré la losa de asfalto lisa en el pack")
    return mejor[1]


def enmascarar(tile, x0, y0, caja):
    """Deja en alfa todo lo que cae fuera de la caja de la marca.

    `caja` es la caja de PÍXELES del dibujo (el marco amarillo de la bahía, las
    líneas del cajón), medida sobre la hoja; `x0, y0` es dónde empieza esta celda
    en la hoja. Un par de píxeles de margen para que no se coma la sombra de
    contacto que el pack pinta al pie de la línea.
    """
    cx0, cy0, cx1, cy1 = caja
    out = tile.copy()
    dst = out.load()
    for y in range(CELL):
        for x in range(CELL):
            gx, gy = x0 + x, y0 + y
            if not (cx0 - MARGEN_PINTURA <= gx < cx1 + MARGEN_PINTURA
                    and cy0 - MARGEN_PINTURA <= gy < cy1 + MARGEN_PINTURA):
                dst[x, y] = (0, 0, 0, 0)
    return out


def recortar(tile, fondo):
    """Deja la PINTURA; el asfalto se va a alfa.

    El fondo no se adivina: la pieza está dibujada sobre la losa lisa, así que
    todo lo que coincide con ella exactamente es asfalto. Lo que no coincide es
    lo pintado — la raya blanca, el marco amarillo, la letra y su sombra.

    Como la hoja tiene RUIDO en el asfalto (el pack le pone grano), no alcanza con
    comparar igualdad exacta: se compara contra el grano tolerando la amplitud del
    propio ruido, medida sobre la losa.
    """
    a, b = tile.load(), fondo.load()
    ruido = ruido_de(fondo)
    out = Image.new("RGBA", (CELL, CELL), (0, 0, 0, 0))
    dst = out.load()
    for y in range(CELL):
        for x in range(CELL):
            p = a[x, y]
            if p[3] == 0:
                continue
            q = b[x, y]
            if max(abs(p[i] - q[i]) for i in range(3)) > ruido:
                dst[x, y] = p
    return out


def ruido_de(fondo):
    """Cuánto varía el GRANO del asfalto, en niveles.

    El pack le pinta ruido al asfalto, así que "igual al fondo" no puede medirse
    por igualdad exacta: la tolerancia es la amplitud del propio grano, medida
    sobre la losa lisa en vez de elegida.
    """
    b = fondo.load()
    vals = [sum(b[x, y][:3]) / 3 for y in range(CELL) for x in range(CELL)]
    return (max(vals) - min(vals)) / 2 + 6


def medidas_de_roads():
    """Cuánto mide cada marca **según el juego** (`roads.rs`), en tiles.

    Las dos mitades tienen que decir lo mismo: el cliente reserva `w × h` celdas
    de calzada y este script publica `w × h` poses. Si se separan, media bahía
    queda sin dibujar y la otra media se pinta sobre el carril.
    """
    txt = open(ROADS_RS, encoding="utf8").read()
    out = {}
    for nombre, w, h in re.findall(
        r"pub const (BAHIA_NS|BAHIA_EW|CAJON_NS|CAJON_EW): \(i64, i64\) = \((\d+), (\d+)\);", txt
    ):
        out[nombre.lower()] = (int(w), int(h))
    if len(out) != 4:
        raise SystemExit(
            "roads.rs tiene que declarar BAHIA_NS/BAHIA_EW/CAJON_NS/CAJON_EW; "
            f"encontré {sorted(out)}"
        )
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    hoja = Image.open(HOJA).convert("RGBA")
    px = hoja.load()
    W, H = hoja.size
    # El asfalto liso hace falta ANTES de detectar: es contra él que se decide
    # qué franja del borde es vereda o cordón y cuál es pintura.
    fondo = asfalto_liso()
    fondo_px, ruido = fondo.load(), ruido_de(fondo)

    encontradas = {
        f"bahia_{k}": v for k, v in bahias(px, W, H).items()
    }
    encontradas.update({f"cajon_{k}": v for k, v in cajones(px, W, H, fondo_px, ruido).items()})

    esperado = medidas_de_roads()
    for nombre, ((c0, r0, w, h), caja) in sorted(encontradas.items()):
        marca = "  " if esperado.get(nombre) == (w, h) else "≠ "
        print(f"{marca}{nombre:10s} hoja col {c0:3d} row {r0:3d}  {w}x{h} tiles  pintura px {caja}")
    faltan = [k for k in esperado if k not in encontradas]
    if faltan:
        raise SystemExit(f"no encontré en la hoja: {faltan}")
    malas = {
        k: (encontradas[k][0][2], encontradas[k][0][3])
        for k in esperado
        if (encontradas[k][0][2], encontradas[k][0][3]) != esperado[k]
    }
    if malas:
        raise SystemExit(
            "el pack mide distinto de lo que declara roads.rs "
            f"(medido {malas}, declarado { {k: esperado[k] for k in malas} }): "
            "actualizá las constantes, no el recorte"
        )

    if args.dry_run:
        return

    # Una sola página chica: son ~70 tiles de 32 px.
    tiles = []
    for nombre, ((c0, r0, w, h), caja) in sorted(encontradas.items()):
        slot = SLOT_BAHIA if nombre.startswith("bahia") else SLOT_CAJON
        eje = nombre.split("_")[1]
        for j in range(h):
            for i in range(w):
                x, y = (c0 + i) * CELL, (r0 + j) * CELL
                celda = hoja.crop((x, y, x + CELL, y + CELL))
                # Fuera de la caja de la pintura no hay marca: hay vereda, cordón
                # o la calzada de al lado. Se va a alfa antes de recortar contra
                # el asfalto, no después, para que el borde del dibujo sea el
                # borde del dibujo y no el de la celda.
                tiles.append((slot, f"{eje}_{i}_{j}", recortar(enmascarar(celda, x, y, caja), fondo)))

    lado = 1
    while lado * lado < len(tiles):
        lado += 1
    pagina = Image.new("RGBA", (lado * CELL, lado * CELL), (0, 0, 0, 0))
    poses = {}
    for n, (slot, pose, im) in enumerate(tiles):
        c, r = n % lado, n // lado
        pagina.paste(im, (c * CELL, r * CELL))
        poses.setdefault(slot, {})[pose] = {"col": c, "row": r, "w": 1, "h": 1}
    os.makedirs(os.path.dirname(os.path.join(ART, "..", PAGINA_REL)), exist_ok=True)
    pagina.save(os.path.join(ROOT, "native/crates/ha-game/assets", PAGINA_REL))

    with open(MANIFEST) as fh:
        man = json.load(fh)
    previo = {}
    if os.path.exists(INDEX):
        with open(INDEX) as fh:
            previo = json.load(fh)
    for k in previo.get("sprites", []):
        man["sprites"].pop(k, None)

    nuevos = []
    for slot, ps in poses.items():
        # La entrada base apunta a la primera pose: quien pida el slot sin pose
        # (el editor, la precarga) ve pintura y no un hueco.
        primera = min(ps)
        man["sprites"][slot] = {
            "grid": [CELL, CELL],
            # Capa Floor: es pintura sobre el piso, no algo que se levante. Sin
            # esto el Y-sort la trataría como un objeto y el jugador le pasaría
            # por debajo.
            "layer": "Floor",
            "png": PAGINA_REL,
            "rect": dict(ps[primera]),
            # Mismo formato que `road/linea`: la pose envuelve su `rect`.
            "poses": {p: {"rect": dict(r)} for p, r in sorted(ps.items())},
        }
        nuevos.append(slot)

    with open(MANIFEST, "w") as fh:
        json.dump(man, fh, indent=1, ensure_ascii=False, sort_keys=True)
    with open(INDEX, "w") as fh:
        json.dump({"sprites": nuevos, "medidas": {k: list(v) for k, v in esperado.items()}}, fh, indent=1)
    print(f"\n{PAGINA_REL}: {len(tiles)} tiles de pintura, {len(nuevos)} slots")


if __name__ == "__main__":
    main()
