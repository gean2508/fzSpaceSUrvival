#!/usr/bin/env python3
"""**La puerta animada de cada edificio que ya la trae dibujada.**

    python3 scripts/medir-puertas.py               # mide → scripts/puertas-index.json
    python3 scripts/import-puertas-edificios.py --dry-run
    python3 scripts/import-puertas-edificios.py

## Por qué existe

`medir-puertas.py` encontró 283 dibujos de edificio en uso con la puerta del pack
PINTADA en la fachada (la comisaría, el correo, las casas, el búnker…) y dijo, por
píxel, cuál hoja animada es cada una y dónde cae. Hasta acá sólo las casas de
`import-casas.py` tenían esa puerta montada: el resto tenía una puerta que no se
abría nunca.

Este script le cuelga a cada uno de esos slots las poses que ya sabe reproducir
`building::spawn_puerta_de_fachada`:

- `puerta_en_sprite` (y `puerta2_en_sprite`, `puerta3_…` si el edificio tiene
  más de una): dónde va el batiente dentro del recorte, en PÍXELES (`grid: [1,1]`).
- `closed`, `opening`, `open`, `closing` (y `puerta2_closed`…): el batiente.
- `base`: la casa entera, para que la pose representativa siga siendo la casa y
  no su puerta (ver `import-casas.py::colgar_puertas_en_fachadas`).

## Lo que se mide (nada a ojo)

- **El sitio** sale de `puertas-index.json`: el `xy` es donde calza la CAJA opaca
  del cuadro cerrado. El batiente se recorta a la caja que cubren TODOS sus
  cuadros (al abrirse, una hoja puede salirse de la caja del cuadro cerrado), y
  el sitio se corre lo mismo. Si esa caja se saliera del dibujo por arriba o por
  la izquierda (un rect del manifest no admite negativos), se recorta ahí.
- **Varias puertas por edificio**: se toman en orden de parecido y se descarta
  la que se pise con una ya tomada — el medidor a veces propone dos hojas del
  mismo tema para la misma puerta.
- **El cuadro abierto** es el más distinto del cerrado
  (`import-casas.py::cuadro_abierto`). Las hojas largas (la casa embrujada trae
  91 cuadros) se muestrean a lo sumo `MAX_CUADROS` por tramo, conservando la
  duración.

## Qué escribe

- `art/pack/edificios_puertas_N.png` — las tiras, empaquetadas por filas.
- Las poses de arriba en cada slot del manifest (idempotente: lo que agregó la
  corrida anterior se borra antes, según `scripts/puertas-edificios-index.json`).

**Orden**: después de `import-pack-masivo.py`, `import-edificios.py`,
`import-casas.py` y `medir-puertas.py`. No pisa poses que ya tenga un slot.
"""

import argparse
import importlib.util
import json
import os

import numpy as np
from PIL import Image

RAIZ = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ASSETS = os.path.join(RAIZ, "native/crates/ha-game/assets")
MANIFEST = os.path.join(ASSETS, "art/manifest.json")
MEDIDAS = os.path.join(RAIZ, "scripts/puertas-index.json")
INDEX = os.path.join(RAIZ, "scripts/puertas-edificios-index.json")
PAGINA_REL = "art/pack/edificios_puertas_{}.png"
PAGINA_LADO = 2048
MAX_CUADROS = 8
FPS = 24.0
#: Una puerta tiene que durar lo que dura en el pack, pero ni el portón más
#: lento del pack (la persiana del hospital, 30 cuadros) debe dejarte esperando.
MAX_SEGUNDOS = 1.0
OPACO = 16

_spec = importlib.util.spec_from_file_location("casas", os.path.join(RAIZ, "scripts/import-casas.py"))
casas = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(casas)
CELL = casas.CELL


def png(ruta):
    return np.array(Image.open(ruta).convert("RGBA"))


def muestrear(a, b, n):
    """Índices de a..b inclusive, a lo sumo `n` (siempre con los extremos)."""
    if b - a + 1 <= n:
        return list(range(a, b + 1))
    return sorted({round(a + (b - a) * i / (n - 1)) for i in range(n)})


class Hoja:
    def __init__(self, nombre, med):
        arr = png(os.path.join(casas.ANIMADAS, f"{nombre}.png"))
        w, h = med["cuadro_celdas"]
        n = med["cuadros"]
        self.nombre = nombre
        fw, fh = w * CELL, h * CELL
        cuadros = [arr[:fh, i * fw: (i + 1) * fw] for i in range(n)]
        abierta = casas.cuadro_abierto(arr, w, n)
        # Caja que cubren TODOS los cuadros: el batiente abierto puede salirse
        # de la del cerrado.
        op = np.zeros((fh, fw), bool)
        for c in cuadros:
            op |= c[:, :, 3] > OPACO
        ys, xs = np.where(op)
        self.union = (int(xs.min()), int(ys.min()), int(xs.max() + 1), int(ys.max() + 1))
        self.caja0 = med["caja_px"]
        ida = muestrear(0, abierta, MAX_CUADROS)
        vuelta = muestrear(abierta, n - 1, MAX_CUADROS)
        self.orden = ida + vuelta[1:]
        self.pos_abierta = len(ida) - 1
        self.cuadros = cuadros
        # La animación dura lo que en el pack, repartida en los cuadros que quedan.
        self.fps_ida = len(ida) / min(max(abierta, 1) / FPS, MAX_SEGUNDOS)
        self.fps_vuelta = len(vuelta) / min(max(n - 1 - abierta, 1) / FPS, MAX_SEGUNDOS)

    def tira(self, recorte):
        x0, y0, x1, y1 = recorte
        return [self.cuadros[i][y0:y1, x0:x1] for i in self.orden]


def elegir_puertas(incluidas, medidas):
    """Puertas de un edificio sin pisarse: mejor parecido primero."""
    tomadas = []
    for hoja, xy, f in sorted(incluidas, key=lambda t: -t[2]):
        cx, cy, cw, ch = medidas[hoja]["caja_px"]
        r = (xy[0], xy[1], xy[0] + cw, xy[1] + ch)
        if any(r[0] < o[2] and o[0] < r[2] and r[1] < o[3] and o[1] < r[3] for _, _, o in tomadas):
            continue
        tomadas.append((hoja, xy, r))
    return tomadas


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    med = json.load(open(MEDIDAS))
    man = json.load(open(MANIFEST))
    sprites = man["sprites"]

    # Deshacer la corrida anterior.
    if os.path.exists(INDEX):
        viejo = json.load(open(INDEX))
        for clave, poses in viejo.get("poses", {}).items():
            e = sprites.get(clave)
            if e and "poses" in e:
                for p in poses:
                    e["poses"].pop(p, None)
                if not e["poses"]:
                    del e["poses"]

    hojas = {}
    # (clave, [(hoja, recorte_de_la_tira, offset_en_el_dibujo)])
    plan = []
    for clave, v in sorted(med["edificios"].items()):
        if v["estado"] != "con_puerta" or "@" in clave:
            continue
        e = sprites.get(clave)
        if not e or "rect" not in e or not e.get("png"):
            continue
        if any(p.startswith("puerta_en") for p in e.get("poses", {})):
            continue  # ya la montó import-casas.py
        W, H = e["rect"]["w"] * CELL, e["rect"]["h"] * CELL
        puertas = []
        for hoja, xy, _ in elegir_puertas(v["incluidas"], med["puertas"]):
            if hoja not in hojas:
                hojas[hoja] = Hoja(hoja, med["puertas"][hoja])
            hj = hojas[hoja]
            ux0, uy0, ux1, uy1 = hj.union
            cx, cy = hj.caja0[0], hj.caja0[1]
            # esquina del cuadro entero dentro del dibujo, y de ahí la de la unión
            ox, oy = xy[0] - cx + ux0, xy[1] - cy + uy0
            # no se admite salir del dibujo por arriba/izquierda (rect sin signo),
            # ni por abajo/derecha (se dibujaría fuera de la casa)
            if ox < 0:
                ux0 -= ox
                ox = 0
            if oy < 0:
                uy0 -= oy
                oy = 0
            ux1 = min(ux1, ux0 + (W - ox))
            uy1 = min(uy1, uy0 + (H - oy))
            if ux1 - ux0 < 8 or uy1 - uy0 < 8:
                continue
            puertas.append((hj, (ux0, uy0, ux1, uy1), (ox, oy)))
        if puertas:
            plan.append((clave, puertas))

    # Empaquetar las tiras por filas. Una tira por (hoja, recorte) distinto.
    tiras = {}
    for _, puertas in plan:
        for hj, rec, _ in puertas:
            tiras.setdefault((hj.nombre, rec), hj)
    paginas, ubic = [], {}
    x = y = alto_fila = 0
    lienzo = None
    for (nombre, rec), hj in sorted(tiras.items(), key=lambda t: -(t[0][1][3] - t[0][1][1])):
        fw, fh = rec[2] - rec[0], rec[3] - rec[1]
        ancho = fw * len(hj.orden)
        if lienzo is None or x + ancho > PAGINA_LADO:
            x, y = 0, y + alto_fila
            alto_fila = 0
        if lienzo is None or y + fh > PAGINA_LADO:
            lienzo = np.zeros((PAGINA_LADO, PAGINA_LADO, 4), np.uint8)
            paginas.append(lienzo)
            x = y = alto_fila = 0
        for i, c in enumerate(hj.tira(rec)):
            lienzo[y: y + fh, x + i * fw: x + (i + 1) * fw] = c
        ubic[(nombre, rec)] = (len(paginas) - 1, x, y)
        x += ancho
        alto_fila = max(alto_fila, fh)

    agregadas = {}
    for clave, puertas in plan:
        e = sprites[clave]
        suyas = e.setdefault("poses", {})
        nuevas = []
        if "base" not in suyas:
            suyas["base"] = {"png": e["png"], "grid": e.get("grid") or [CELL, CELL], "rect": e["rect"]}
            nuevas.append("base")
        for n, (hj, rec, (ox, oy)) in enumerate(puertas):
            pre = "" if n == 0 else f"puerta{n + 1}_"
            pag, px, py = ubic[(hj.nombre, rec)]
            ruta = PAGINA_REL.format(pag)
            fw, fh = rec[2] - rec[0], rec[3] - rec[1]

            def cuadro(k, frames=None, fps=None):
                d = {"png": ruta, "grid": [1, 1], "rect": {"col": px + k * fw, "row": py, "w": fw, "h": fh}}
                if frames:
                    d["anim"] = {"frames": frames, "fps": round(fps, 2)}
                return d

            a = hj.pos_abierta
            poses = {
                f"{pre}closed": cuadro(0),
                f"{pre}open": cuadro(a),
                f"{pre}opening": cuadro(0, a + 1, hj.fps_ida),
                f"{pre}closing": cuadro(a, len(hj.orden) - a, hj.fps_vuelta),
                ("puerta_en_sprite" if n == 0 else f"puerta{n + 1}_en_sprite"): {
                    "png": ruta, "grid": [1, 1], "rect": {"col": ox, "row": oy, "w": fw, "h": fh}},
            }
            for k, v in poses.items():
                if k not in suyas:
                    suyas[k] = v
                    nuevas.append(k)
        agregadas[clave] = nuevas

    n_puertas = sum(len(p) for _, p in plan)
    print(f"{len(plan)} edificios · {n_puertas} puertas · {len(tiras)} tiras · {len(paginas)} página(s)")
    if args.dry_run:
        return
    for i, p in enumerate(paginas):
        # recortar el alto sobrante de la última página
        filas = np.where(p[:, :, 3].any(1))[0]
        alto = int(filas.max() + 1) if len(filas) else 1
        Image.fromarray(p[:alto]).save(os.path.join(ASSETS, PAGINA_REL.format(i)), optimize=True)
    with open(MANIFEST, "w") as fh:
        json.dump(man, fh, indent=1, ensure_ascii=False, sort_keys=True)
    with open(INDEX, "w") as fh:
        json.dump({"paginas": [PAGINA_REL.format(i) for i in range(len(paginas))], "poses": agregadas},
                  fh, indent=1, ensure_ascii=False)


if __name__ == "__main__":
    main()
