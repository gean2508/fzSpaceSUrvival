#!/usr/bin/env python3
"""**TODO EL ARTE 2D, PASADO A 3D Y RE-HORNEADO A 2D.**

Pedido del usuario (27 sep 2026): convertir asset por asset a 3D sin perder
diseño ni color, respetando —o mejorando— la iluminación, y sin sumar lag. La
decisión fue **3D pre-renderizado**: el modelo vive fuera del juego y lo que el
juego carga sigue siendo el MISMO PNG, del MISMO tamaño, con los MISMOS rects.
El manifest no se toca, el Redmi no paga nada (regla 1 y 2 del proyecto).

Salidas (todo bajo `arte3d/` en la raíz del repo, fuera del APK):

| carpeta | qué hay |
|---|---|
| `fuente/paginas/`  | la página ORIGINAL de cada PNG, tal cual vino. Es la entrada de este script, así que correrlo dos veces da lo mismo. |
| `fuente/objetos/`  | cada asset recortado, **una carpeta por objeto** y un PNG por variante/pose. Un objeto que en el juego se arma con varios PNG (casa = cuerpo + puertas + techo) queda junto en su carpeta. |
| `relieve/`         | por página: `*_altura.png` (el modelo, 8 bits, 1 nivel = 1/32 px) y `*_normal.png`. |
| `modelos/`         | un `.glb` por asset: malla de relieve con el PNG original embebido, filtro NEAREST. Se abre en Blender o en cualquier visor glTF. (`--modelos`; no se versiona, se regenera) |

Y el horneado se escribe en `native/crates/ha-game/assets/art/…` encima de la
página, sólo donde el modelo cambia algo.

```sh
python3 scripts/arte3d.py                # hornea + fuente + relieve
python3 scripts/arte3d.py --modelos      # además exporta los .glb
python3 scripts/arte3d.py --seco         # mide y no escribe nada
python3 scripts/arte3d.py --deshacer     # devuelve las páginas originales
```

## Qué NO se vuelve volumen

El suelo es plano de verdad: agua, piso, senda, calle, orla, bordes, taludes,
sombras, UI, partículas. Un bisel ahí pinta costuras en cada tile. Y **una pieza
de kit no se bisela contra el borde de su celda**: el borde del rect se trata
como continuación (padding `edge`), así que una cerca, un muro o una copa
cortada a ras siguen empalmando sin raya con su vecina.
"""

from __future__ import annotations

import argparse
import io
import json
import shutil
import sys
from pathlib import Path

import numpy as np
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib import volumen3d as v3  # noqa: E402
from lib import voxel3d as vx  # noqa: E402

RAIZ = Path(__file__).resolve().parent.parent
ASSETS = RAIZ / "native/crates/ha-game/assets"
MANIFEST = ASSETS / "art/manifest.json"
SALIDA = RAIZ / "arte3d"

# Categorías de clave lógica que son SUELO o pantalla: no llevan volumen.
PLANAS = {
    "agua", "floor", "senda", "orla", "empalme", "road", "tiles", "borde",
    "paso", "sombra", "ui", "particles", "zocalo", "azotea", "cliff", "ledge",
    "slope", "backgrounds", "items", "ceiling", "interior", "room", "retrato",
}


# (png, x, y) de una tira animada → tamaño de UN cuadro: el modelo es del primero.
CUADRO: dict = {}


def rects_de(clave: str, spr: dict):
    """(png, x, y, w, h, parte) de cada recorte que usa una entrada."""
    def uno(png, grid, r, anim=None, dir_=None, parte="base"):
        gw, gh = grid
        w, h = r["w"] * gw, r["h"] * gh
        n = (anim or {}).get("frames", 1)
        if n > 1:
            # el CUADRO va en `CUADRO[...]`; el rect cubre la tira entera
            CUADRO[(png, r["col"] * gw, r["row"] * gh)] = (w, h)
            if dir_ in ("Down", "Up"):
                h *= n
            else:
                w *= n
        return (png, r["col"] * gw, r["row"] * gh, w, h, parte)

    png, grid = spr.get("png"), spr.get("grid") or [32, 32]
    if "rect" in spr:
        yield uno(png, grid, spr["rect"])
    if "strip" in spr:
        yield uno(png, grid, spr["strip"], spr.get("anim"), spr.get("strip_dir"))
    for nombre, p in (spr.get("poses") or {}).items():
        if "rect" in p:
            yield uno(p.get("png", png), p.get("grid", grid), p["rect"], p.get("anim"),
                      p.get("dir"), parte=nombre)
    if list(spr) == ["png"]:
        yield (png, None, None, None, None, "base")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--modelos", action="store_true")
    ap.add_argument("--seco", action="store_true")
    ap.add_argument("--deshacer", action="store_true")
    ap.add_argument("--solo-modelos", action="store_true",
                    help="sólo los .glb, desde los recortes de fuente/objetos (en paralelo)")
    ap.add_argument("--personajes", action="store_true",
                    help="sólo las capas del generador de personajes")
    a = ap.parse_args()
    if a.personajes:
        personajes()
        return
    if a.solo_modelos:
        solo_modelos()
        return

    sprites = json.loads(MANIFEST.read_text())["sprites"]
    por_pagina: dict[str, list] = {}
    for clave, spr in sprites.items():
        cat = clave.split("/")[0]
        for png, x, y, w, h, parte in rects_de(clave, spr):
            if not png or png.startswith("art/chargen"):
                continue
            por_pagina.setdefault(png, []).append((clave, cat in PLANAS, x, y, w, h, parte))

    if a.deshacer:
        for png in por_pagina:
            orig = SALIDA / "fuente/paginas" / png
            if orig.exists():
                shutil.copyfile(orig, ASSETS / png)
        print("páginas originales restauradas")
        return

    tot_px = tot_pag = 0
    for png, rects in sorted(por_pagina.items()):
        ruta = ASSETS / png
        orig = SALIDA / "fuente/paginas" / png
        if not orig.exists():
            if not ruta.exists():
                print(f"  falta {png}")
                continue
            orig.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ruta, orig)
        img = np.array(Image.open(orig).convert("RGBA"))
        H, W = img.shape[:2]
        plana = np.zeros((H, W), bool)
        for _, flat, x, y, w, h, _ in rects:
            if flat:
                if x is None:
                    plana[:] = True
                else:
                    plana[y:y + h, x:x + w] = True

        horneada = img.copy()
        alt = np.zeros((H, W), np.float32)
        nor = np.zeros((H, W, 4), np.uint8)
        hecho = np.zeros((H, W), bool)
        cambiados = 0
        for clave, flat, x, y, w, h, parte in rects:
            if x is None:
                x, y, w, h = 0, 0, W, H
            x1, y1 = min(x + w, W), min(y + h, H)
            if flat or x1 <= x or y1 <= y:
                continue
            sub = img[y:y1, x:x1]
            vol = ~plana[y:y1, x:x1] & ~hecho[y:y1, x:x1]
            if not vol.any():
                continue
            # el borde del rect es CONTINUACIÓN, no silueta: pieza de kit sin raya
            p = int(v3.BISEL) + 2
            sub_p = np.pad(sub, ((p, p), (p, p), (0, 0)), mode="edge")
            vol_p = np.pad(vol, p, mode="edge")
            hh, bis = v3.alturas(sub_p, vol_p)
            hh, bis = hh[p:-p, p:-p], bis[p:-p, p:-p]
            out, n = v3.hornear(sub, vol, bis)
            m = vol[..., None]
            horneada[y:y1, x:x1] = np.where(m, out, horneada[y:y1, x:x1])
            alt[y:y1, x:x1] = np.where(vol, hh, alt[y:y1, x:x1])
            solido = (sub[..., 3] == 255) & vol
            nm = v3.mapa_de_normales(v3.normales(hh), solido)
            nor[y:y1, x:x1] = np.where(solido[..., None], nm, nor[y:y1, x:x1])
            hecho[y:y1, x:x1] |= vol
            cambiados += n

        tot_px += cambiados
        tot_pag += cambiados > 0
        if a.seco:
            continue
        if cambiados:
            Image.fromarray(horneada).save(ruta, optimize=True)
        if hecho.any():
            base = SALIDA / "relieve" / png
            base.parent.mkdir(parents=True, exist_ok=True)
            q = np.clip(alt * 32.0, 0, 255).astype(np.uint8)
            Image.fromarray(q).save(base.with_name(base.stem + "_altura.png"), optimize=True)
            Image.fromarray(nor).save(base.with_name(base.stem + "_normal.png"), optimize=True)

        # objetos: fuente recortada + modelo
        for clave, flat, x, y, w, h, parte in rects:
            if x is None:
                x, y, w, h = 0, 0, W, H
            x1, y1 = min(x + w, W), min(y + h, H)
            if x1 <= x or y1 <= y:
                continue
            objeto, _, variante = clave.partition("#")
            nombre = (variante or "base") + ("" if parte == "base" else f"__{parte}")
            dest = SALIDA / "fuente/objetos" / objeto / f"{nombre}.png"
            dest.parent.mkdir(parents=True, exist_ok=True)
            recorte = Image.fromarray(img[y:y1, x:x1])
            recorte.save(dest, optimize=True)
            if a.modelos:
                sub = img[y:y1, x:x1]
                cw, ch = CUADRO.get((png, x, y), (x1 - x, y1 - y))
                sub = sub[:ch, :cw]
                if flat:  # el suelo es una losa: un vóxel de alto
                    col, lleno = vx.reconstruir(sub, D=max(1, sub.shape[0] - 1), hundir=False)
                elif vx.es_organico(clave):
                    col, lleno, _ = vx.reconstruir_organico(sub)
                else:
                    col, lleno = vx.reconstruir(sub)
                vx.exportar_glb(SALIDA / "modelos" / objeto / f"{nombre}.glb", col, lleno)
        print(f"  {png}: {cambiados} px re-iluminados")

    print(f"TOTAL: {tot_px} px en {tot_pag} páginas")


def _un_modelo(t):
    clave, objeto, nombre, flat, cuadro = t
    src = SALIDA / "fuente/objetos" / objeto / f"{nombre}.png"
    dst = SALIDA / "modelos" / objeto / f"{nombre}.glb"
    if not src.exists():
        return 0
    sub = np.array(Image.open(src).convert("RGBA"))
    if cuadro:
        sub = sub[:cuadro[1], :cuadro[0]]
    if not (sub[..., 3] == 255).any():
        return 0
    if flat:
        col, lleno = vx.reconstruir(sub, D=max(1, sub.shape[0] - 1), hundir=False)
    elif vx.es_organico(clave):
        col, lleno, _ = vx.reconstruir_organico(sub)
    else:
        col, lleno = vx.reconstruir(sub)
    return int(vx.exportar_glb(dst, col, lleno))


def solo_modelos():
    """Todos los `.glb` en vóxeles, en paralelo, desde los recortes ya separados."""
    from multiprocessing import Pool
    sprites = json.loads(MANIFEST.read_text())["sprites"]
    tareas = []
    for clave, spr in sprites.items():
        cat = clave.split("/")[0]
        for png, x, y, w, h, parte in rects_de(clave, spr):
            if not png or png.startswith("art/chargen"):
                continue
            objeto, _, variante = clave.partition("#")
            nombre = (variante or "base") + ("" if parte == "base" else f"__{parte}")
            cuadro = CUADRO.get((png, x, y)) if x is not None else None
            tareas.append((clave, objeto, nombre, cat in PLANAS, cuadro))
    with Pool(3) as pool:
        hechos = sum(pool.imap_unordered(_un_modelo, tareas, chunksize=16))
    print(f"modelos: {hechos} .glb en vóxeles")


# Orden de apilado de las capas del personaje, de adentro hacia afuera, y cuánto
# se despega cada una del cuerpo (px). De frente no se nota —z no mueve x ni y—;
# girado, la ropa envuelve al cuerpo, el pelo a la cabeza y el accesorio va arriba.
CAPAS = [("cuerpo", 0.0), ("ojos", 0.35), ("ropa", 0.6), ("pelo", 1.0), ("accesorio", 1.4)]
# Cuadro de frente: animación `still` (fila 0), dirección D (4.ª de R,U,L,D).
CUADRO_FRENTE = (3 * 32, 0, 32, 64)


def personajes():
    """**El personaje en capas, como el generador lo arma.** Cada pieza (cuerpo,
    ojos, ropa, pelo, accesorio) sale como su propio `.glb` en vóxeles, en
    cascarón sobre el cuerpo base: cargados juntos en el mismo origen dan el
    mismo personaje que el juego compone, y se cambian uno por uno."""
    base = ASSETS / "art/chargen"
    x, y, w, h = CUADRO_FRENTE
    cuerpo0 = np.array(Image.open(base / "cuerpo/000.png").convert("RGBA"))[y:y + h, x:x + w]
    _, _, piso = vx.reconstruir_organico(cuerpo0)
    R = int(piso.max()) + 3
    total = 0
    for n_capa, (capa, _) in enumerate(CAPAS):
        for png in sorted((base / capa).glob("*.png")):
            sub = np.array(Image.open(png).convert("RGBA"))[y:y + h, x:x + w]
            if not (sub[..., 3] == 255).any():
                continue
            col, lleno, _ = vx.reconstruir_organico(sub, piso=piso, despegue=min(n_capa, 2) if n_capa else 0, R_fijo=R)
            Image.fromarray(sub).save((SALIDA / "fuente/personajes" / capa).mkdir(parents=True, exist_ok=True)
                                      or SALIDA / "fuente/personajes" / capa / png.name)
            vx.exportar_glb(SALIDA / "modelos/personajes" / capa / (png.stem + ".glb"), col, lleno)
            total += 1
    print(f"personajes: {total} piezas en capas")


if __name__ == "__main__":
    main()
