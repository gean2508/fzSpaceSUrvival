#!/usr/bin/env python3
"""**Los slots del manifest que nadie pide**: cuáles son copia y cuáles no.

    python3 scripts/slots-colgados.py            # informa
    python3 scripts/slots-colgados.py --limpiar  # borra SÓLO las copias exactas

## Qué pasó

Cuando `import-pack-masivo.py` regeneró el bloque `PACK:START/END` de `slots.rs`
y `placeables.rs` (3 ago 2026), los ~176 objetos que había curado a mano el
importador viejo perdieron su slot y su entrada de colocable. **El arte se quedó
en el manifest**: claves con PNG, con rect y con huella que ninguna línea de
código nombra. Son las que aparecen como "assets que no se usan".

## Copia no es "se llama parecido"

La mayoría de esas claves son el MISMO dibujo que el pack ya usa con otro
nombre (`agua/boya_rio` es `agua/camping_boya`). Pero el nombre no alcanza para
saberlo —el importador bautiza automático y erra— así que se compara el
**pixel**: se aplica el `rect` de cada clave, se comparan los bytes RGBA con el
mismo tamaño, y además tienen que coincidir animación, huella de colisión, capa,
emisión de luz e interacción. Si algo de eso difiere, NO es copia y se conserva:
borrar un dibujo único porque "se parece" es perderlo.

Lo que queda —el arte único que nadie pide— se lista para engancharlo al juego.
"""

import argparse
import hashlib
import json
import os
import re

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
NATIVE = os.path.join(ROOT, "native")
ASSETS = os.path.join(NATIVE, "crates/ha-game/assets")
MANIFEST = os.path.join(ASSETS, "art/manifest.json")

#: Campos que tienen que coincidir para que dos claves sean la misma pieza. El
#: dibujo puede ser idéntico y la pieza distinta: una con huella de colisión y la
#: otra sin ella no son intercambiables.
#: `grid` NO entra: es cómo se lee el `rect` dentro de la hoja, no una propiedad
#: de la pieza. El arte curado a mano venía en PNG suelto (sin `grid`) y el del
#: pack en celdas de 32 — compararlo declaraba distinto TODO par, que es como se
#: pierde una limpieza entera sin darse cuenta.
CAMPOS = ("anim", "strip", "strip_dir", "footprint", "layer", "emission", "interaccion")


def fuentes_rust():
    for base, dirs, files in os.walk(os.path.join(NATIVE, "crates")):
        if "target" in base:
            continue
        for f in files:
            if f.endswith(".rs"):
                yield os.path.join(base, f)


def claves_pedidas():
    """Las claves de arte que el código nombra, literales o por `format!`."""
    literales, prefijos = set(), set()
    for p in fuentes_rust():
        with open(p, encoding="utf8", errors="ignore") as fh:
            txt = fh.read()
        literales |= set(re.findall(r'"([a-z0-9_]+/[a-z0-9_\-]+)"', txt))
        # `format!("npc/char_{}", n)` → prefijo "npc/char_"
        prefijos |= set(re.findall(r'"([a-z0-9_]+/[a-z0-9_]*?)\{', txt))
    return literales, prefijos


def colgados(sprites):
    literales, prefijos = claves_pedidas()

    def pedida(k):
        return k in literales or any(k.startswith(p) for p in prefijos)

    return sorted(k for k in sprites if "#" not in k and not pedida(k))


class Dibujos:
    def __init__(self):
        self.cache = {}

    def recorte(self, entrada):
        """La imagen que declara la entrada, con su `rect` aplicado."""
        png = entrada.get("png")
        if not png:
            return None
        ruta = os.path.join(ASSETS, png)
        if not os.path.exists(ruta):
            return None
        if png not in self.cache:
            self.cache[png] = Image.open(ruta).convert("RGBA")
        im = self.cache[png]
        r = entrada.get("rect")
        if r:
            gw, gh = entrada.get("grid", [32, 32])
            im = im.crop(
                (r["col"] * gw, r["row"] * gh, (r["col"] + r["w"]) * gw, (r["row"] + r["h"]) * gh)
            )
        return im

    def firma(self, entrada):
        """Bytes exactos del recorte + tamaño. `None` = no se puede comparar."""
        im = self.recorte(entrada)
        if im is None:
            return None
        return hashlib.sha256(im.tobytes()).hexdigest() + f"|{im.size}"

    def firma_dibujo(self, entrada):
        """Igual, pero recortando el aire de alrededor.

        Es la comparación que importa para decir "es el mismo dibujo": el
        importador viejo guardaba cada objeto en su propio PNG ajustado y el
        masivo lo mete en una celda de la hoja, así que el MISMO banco tiene 6 px
        de aire de más en una de las dos. Lo que no se afloja es el tamaño del
        dibujo: si uno mide 30×18 y el otro 32×20, son dos dibujos.
        """
        im = self.recorte(entrada)
        if im is None:
            return None
        bb = im.getbbox()
        if bb is None:
            return None
        im = im.crop(bb)
        return hashlib.sha256(im.tobytes()).hexdigest() + f"|{im.size}"

    def rasgos(self, entrada):
        return tuple(json.dumps(entrada.get(c), sort_keys=True) for c in CAMPOS)


def sanear_recortes(sprites, d):
    """Recortes que declaran más celdas de las que tiene su PNG.

    Salen de un recorte hecho de más en HA Studio (arrastrar una celda de sobra).
    El juego después dibuja el sprite dentro de un cuadro más alto que el dibujo,
    así que el objeto aparece pegado arriba con un hueco debajo — o directamente
    no se ve. Se ajusta al tamaño real de la imagen: no se inventa nada, se
    recorta lo que no existe.
    """
    arreglados = []
    for k, e in sprites.items():
        png = e.get("png")
        if not png:
            continue
        ruta = os.path.join(ASSETS, png)
        if not os.path.exists(ruta):
            continue
        if png not in d.cache:
            d.cache[png] = Image.open(ruta).convert("RGBA")
        W, H = d.cache[png].size
        gw, gh = e.get("grid", [32, 32])
        for campo in ("rect", "strip"):
            r = e.get(campo)
            if not r:
                continue
            w = min(r["w"], max(1, W // gw - r["col"]))
            h = min(r["h"], max(1, H // gh - r["row"]))
            if (w, h) != (r["w"], r["h"]):
                arreglados.append((k, campo, (r["w"], r["h"]), (w, h)))
                r["w"], r["h"] = w, h
    return arreglados


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--limpiar", action="store_true", help="borra las copias exactas del manifest")
    args = ap.parse_args()

    with open(MANIFEST) as fh:
        man = json.load(fh)
    sprites = man["sprites"]
    sueltos = colgados(sprites)
    print(f"claves de arte: {len([k for k in sprites if '#' not in k])} · sin pedir: {len(sueltos)}")

    d = Dibujos()
    fuera = set(sueltos)
    # Índice de lo que SÍ se usa: por recorte exacto y por dibujo recortado.
    exactos, dibujos = {}, {}
    for k, e in sprites.items():
        if "#" in k or k in fuera:
            continue
        r = d.rasgos(e)
        f = d.firma(e)
        if f:
            exactos.setdefault((f, r), k)
        fd = d.firma_dibujo(e)
        if fd:
            dibujos.setdefault((fd, r), k)

    copias, unicos, sin_comparar = [], [], []
    for k in sueltos:
        e = sprites[k]
        r = d.rasgos(e)
        f, fd = d.firma(e), d.firma_dibujo(e)
        if f is None:
            sin_comparar.append(k)
            continue
        gemelo = exactos.get((f, r)) or (dibujos.get((fd, r)) if fd else None)
        if gemelo:
            copias.append((k, gemelo))
        else:
            unicos.append(k)

    print(f"\nCOPIAS ({len(copias)}) — mismo dibujo Y mismos rasgos que una clave usada:")
    for k, g in copias[:12]:
        print(f"  {k}  ==  {g}")
    if len(copias) > 12:
        print(f"  … y {len(copias) - 12} más")
    print(f"\nDIBUJOS ÚNICOS SIN USAR ({len(unicos)}):")
    for k in unicos:
        print(f"  {k}")
    if sin_comparar:
        print(f"\nNO COMPARABLES ({len(sin_comparar)}) — se conservan:")
        for k in sin_comparar:
            print(f"  {k}")

    arreglables = sanear_recortes({k: sprites[k] for k in sprites}, d) if args.limpiar else []
    if not args.limpiar:
        return
    for k, _ in copias:
        sprites.pop(k, None)
        man.get("objects", {}).pop(k, None)
    for k, campo, antes, ahora in arreglables:
        print(f"recorte ajustado: {k}.{campo} {antes} → {ahora} (se salía del PNG)")
    with open(MANIFEST, "w") as fh:
        json.dump(man, fh, indent=1, ensure_ascii=False, sort_keys=True)
    print(f"\nmanifest: -{len(copias)} claves duplicadas (el dibujo sigue vivo en su clave del pack)"
          f" · {len(arreglables)} recortes ajustados al PNG")


if __name__ == "__main__":
    main()
