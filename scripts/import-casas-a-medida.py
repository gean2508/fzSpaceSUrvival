#!/usr/bin/env python3
"""**Cada casa del pack, re-medida a la huella EXACTA de su programa.**

    python3 scripts/import-casas-a-medida.py --dry-run     # informa, no escribe
    python3 scripts/import-casas-a-medida.py               # publica
    python3 scripts/import-casas-a-medida.py --lamina /tmp/medida.png

## Por qué (3 oct 2026)

Las casas «rotas» —el interior asomando por un costado, la puerta pintada en una celda y la `Door` de la sim en
otra— salían de que el pack dibuja cada casa de UN tamaño y la ciudad le pide otro: `casa_chica` declara 17x14
tiles y su dibujo mide 13x10 celdas (12,4x9,5 tiles con `ESCALA_ARTE` = 0,95), así que `building::roof_layout` la
estiraba hasta 1,38 a lo ancho y 1,47 a lo alto. Estirar un pixel art lo deforma; no estirarlo deja huecos.

Este importador hace lo que haría el artista: **la misma casa, con una tabla más**. Usa `scripts/lib/edificio.py`
(repite o quita columnas/filas que ya son idénticas a su vecina —tablas, tejas—, con la PUERTA protegida) y la
lleva al número de celdas que, por `ESCALA_ARTE`, mide exactamente la huella. Después la pasa por el **juez**
(`lib/juez.py`, escala libre) contra el dibujo original: si queda POR DEBAJO, esa variante no se toca.

## Qué escribe

- `art/pack/casas_a_medida.png`: las casas re-medidas, en una sola hoja (un lote de dibujo, §10).
- En el manifest, **reemplaza el arte de cada variante** (`building/casa_chica#2`, …) por la re-medida y le lleva
  la puerta: `puerta_en_sprite` se corre a donde quedó la puerta (las poses del batiente no cambian: se dibujan
  en ese offset). Guarda el recorte de partida en `"origen"`, así correrlo dos veces da lo mismo (idempotente)
  y la casa original se puede recuperar.
- No toca `casa_hilera`: ésa ya se arma del ancho exacto con el kit `tramo`/`remate` (`import-casas.py`).

⚠ Orden: va DESPUÉS de `import-completos.py` → `medir-puertas.py` → `import-puertas-edificios.py` (re-correr
`import-completos` le borra el trabajo; volvé a correr éste). Ver biblia §8, «La casa a la medida».
"""
from __future__ import annotations

import json
import math
import re
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib.edificio import nueva_posicion, redimensionar  # noqa: E402
from lib.juez import juzgar  # noqa: E402
from lib.pixel import CELDA, Pagina  # noqa: E402

RAIZ = Path(__file__).resolve().parent.parent
ASSETS = RAIZ / "native/crates/ha-game/assets"
MANIFEST = ASSETS / "art/manifest.json"
HOJA = "art/pack/casas_a_medida.png"


def leer_escala_arte() -> float:
    src = (RAIZ / "native/crates/ha-game/src/art2d.rs").read_text()
    m = re.search(r"pub const ESCALA_ARTE: f32 = ([0-9.]+);", src)
    if not m:
        raise SystemExit("no encontré ESCALA_ARTE en art2d.rs: el importador no adivina la escala")
    return float(m.group(1))


def programas_residenciales() -> dict[str, tuple[int, int]]:
    """{clave: (ancho, fondo) en tiles} de los `LotProgram` residenciales de `ha-shared/src/city.rs`."""
    src = (RAIZ / "native/crates/ha-shared/src/city.rs").read_text()
    pat = re.compile(r'LotProgram \{ key: "([^"]+)", cells: \((\d+), (\d+)\), district: District::Residential')
    return {k: (int(w), int(d)) for k, w, d in pat.findall(src)}


def recorte(e: dict) -> tuple[Image.Image, dict]:
    """El dibujo de partida de una entrada del manifest (su `origen` si ya se re-midió)."""
    o = e.get("origen") or {"png": e["png"], "rect": e["rect"], "grid": e.get("grid", [32, 32])}
    g = o.get("grid", [32, 32])
    r = o["rect"]
    hoja = Image.open(ASSETS / o["png"]).convert("RGBA")
    im = hoja.crop((r["col"] * g[0], r["row"] * g[1], (r["col"] + r["w"]) * g[0], (r["row"] + r["h"]) * g[1]))
    return im, o


def puerta_de(e: dict, origen: dict) -> tuple[int, int, int, int] | None:
    """Rect (x, y, w, h) en px de la puerta dentro del dibujo DE PARTIDA."""
    p = (e.get("origen_poses") or {}).get("puerta_en_sprite") or (e.get("poses") or {}).get("puerta_en_sprite")
    if not p:
        return None
    r = p["rect"]
    return r["col"], r["row"], r["w"], r["h"]


def main():
    seco = "--dry-run" in sys.argv
    lamina = Path(sys.argv[sys.argv.index("--lamina") + 1]) if "--lamina" in sys.argv else None
    escala = leer_escala_arte()
    progs = programas_residenciales()
    man = json.loads(MANIFEST.read_text())
    sprites, objetos = man["sprites"], man.get("objects", {})
    pagina = Pagina()
    nuevos: dict = {}
    pares = []  # (original, re-medida) para la lámina
    hechas = rechazadas = 0
    for base, (tw, td) in sorted(progs.items()):
        variantes = [f"{base}#{v['id']}" for v in objetos.get(base, {}).get("variants", [])]
        variantes = [k for k in variantes if k in sprites]
        if not variantes:
            continue
        if any("tramo" in (sprites[k].get("poses") or {}) for k in variantes):
            print(f"{base}: kit tramo/remate (se arma a la medida en el juego) — no se toca")
            continue
        # celdas del pack que miden exactamente la huella con ESCALA_ARTE
        cw, ch_min = round(tw / escala), math.ceil(td / escala)
        for k in variantes:
            e = sprites[k]
            if "rect" not in e:
                continue
            im, origen = recorte(e)
            w0, h0 = im.width // CELDA, im.height // CELDA
            ch = max(h0, ch_min)  # el fondo sólo crece si no tapa; más alto que el dibujo no hace falta
            puerta = puerta_de(e, origen)
            px = [(puerta[0], puerta[0] + puerta[2])] if puerta else []
            py = [(puerta[1], puerta[1] + puerta[3])] if puerta else []
            try:
                a, cols, filas = redimensionar(im, ancho=cw * CELDA, alto=ch * CELDA, proteger_x=px, proteger_y=py, mapas=True)
            except SystemExit as err:
                print(f"  ✗ {k}: {w0}x{h0} → {cw}x{ch} celdas: {err}")
                rechazadas += 1
                continue
            r = juzgar(a, [np.asarray(im)], escala=False)
            estiro = max(cw / w0, ch / h0)
            if r["veredicto"] == "POR DEBAJO":
                print(f"  ✗ {k}: {w0}x{h0} → {cw}x{ch} (×{estiro:.2f}) POR DEBAJO: {r['faltas'][0]}")
                rechazadas += 1
                continue
            col, fila = pagina.poner(Image.fromarray(a))
            entrada = {k2: v for k2, v in e.items() if k2 not in ("png", "rect", "grid", "origen", "origen_poses")}
            entrada.update({"png": HOJA, "grid": [CELDA, CELDA], "rect": {"col": col, "row": fila, "w": cw, "h": ch}, "origen": origen})
            poses = json.loads(json.dumps(e.get("poses") or {}))
            entrada["origen_poses"] = {"puerta_en_sprite": (e.get("origen_poses") or {}).get("puerta_en_sprite") or poses.get("puerta_en_sprite")} if puerta else {}
            if "base" in poses:
                poses["base"] = {"png": HOJA, "grid": [CELDA, CELDA], "rect": dict(entrada["rect"])}
            if puerta:
                q = poses["puerta_en_sprite"]
                q["rect"] = {"col": nueva_posicion(cols, puerta[0]), "row": nueva_posicion(filas, puerta[1]), "w": puerta[2], "h": puerta[3]}
            entrada["poses"] = poses
            nuevos[k] = entrada
            pares.append((k, im, Image.fromarray(a)))
            hechas += 1
            print(f"  ✓ {k}: {w0}x{h0} → {cw}x{ch} celdas (antes ×{max(tw / (w0 * escala), td / (h0 * escala)):.2f} de estiramiento, ahora 1,00) · juez {r['veredicto']}")
    print(f"{hechas} casas a la medida, {rechazadas} se quedan como estaban")
    if lamina and pares:
        S = 1
        alto = sum(max(o.height, n.height) + 20 for _, o, n in pares)
        ancho = max(o.width + n.width for _, o, n in pares) + 48
        L = Image.new("RGBA", (ancho, alto), (104, 140, 92, 255))
        d = ImageDraw.Draw(L)
        y = 0
        for k, o, n in pares:
            h = max(o.height, n.height)
            L.alpha_composite(o, (8, y + h - o.height))
            L.alpha_composite(n, (24 + o.width, y + h - n.height))
            d.text((8, y + h + 4), f"{k}: original → a la medida", fill=(255, 255, 255, 255))
            y += h + 20
        L.convert("RGB").save(lamina)
        print(f"lámina: {lamina}")
    if seco or not nuevos:
        return
    (ASSETS / HOJA).parent.mkdir(parents=True, exist_ok=True)
    pagina.im.save(ASSETS / HOJA)
    sprites.update(nuevos)
    MANIFEST.write_text(json.dumps(man, ensure_ascii=False, indent=1))
    print(f"{HOJA}: {pagina.im.width}x{pagina.im.height} px · {len(nuevos)} variantes re-medidas en el manifest")


if __name__ == "__main__":
    main()
