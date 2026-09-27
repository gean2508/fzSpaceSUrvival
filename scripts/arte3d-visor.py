#!/usr/bin/env python3
"""Arma `arte3d/visor/`: PNG del juego al lado de su `.glb`, para mirarlos juntos.

```sh
python3 scripts/arte3d-visor.py            # muestra por categoría (liviano)
python3 scripts/arte3d-visor.py --todo     # todos los modelos (sólo local)
cd arte3d/visor && python3 -m http.server  # y abrir http://localhost:8000
```

El personaje sale armado en capas (cuerpo + ojos + ropa + pelo), como lo
compone el juego, para ver que las piezas separadas encajan.
"""

import argparse
import json
import shutil
from pathlib import Path

RAIZ = Path(__file__).resolve().parent.parent
A3 = RAIZ / "arte3d"
VISOR = A3 / "visor"

POR_CATEGORIA = 8
NOMBRES = {
    "nature": "Naturaleza", "prop": "Calle", "furniture": "Muebles", "vehicle": "Vehículos",
    "building": "Edificios", "npc": "Personajes y fauna", "fence": "Cercas", "anim": "Animados",
    "decor": "Decoración", "cartel": "Carteles", "agua": "Agua", "floor": "Suelo",
}
# Primero lo que más se ve en el juego; el resto se completa en orden.
PREFERIDOS = [
    "nature/arbol_1", "nature/arbol_4", "npc/fauna_cow", "npc/fauna_dog", "npc/fauna_chicken",
    "npc/fauna_sheep", "npc/fauna_pig", "npc/fauna_crow", "building/casa_hilera",
]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--todo", action="store_true")
    a = ap.parse_args()

    dest = VISOR / "m"
    shutil.rmtree(dest, ignore_errors=True)
    dest.mkdir(parents=True)
    lista, cuenta = [], {}

    def agregar(cat, nombre, clave, png, glbs, camara="oblicua"):
        n = len(lista)
        shutil.copyfile(png, dest / f"{n}.png")
        rutas = []
        for i, g in enumerate(glbs):
            shutil.copyfile(g, dest / f"{n}_{i}.glb")
            rutas.append(f"m/{n}_{i}.glb")
        lista.append({"cat": cat, "nombre": nombre, "clave": clave,
                      "png": f"m/{n}.png", "glb": rutas, "camara": camara})

    # personaje en capas
    per = A3 / "modelos/personajes"
    capas = [("cuerpo", "000"), ("ojos", "000"), ("ropa", "000"), ("pelo", "000")]
    if all((per / c / f"{n}.glb").exists() for c, n in capas):
        from PIL import Image
        base = None
        for c, n in capas:
            im = Image.open(A3 / "fuente/personajes" / c / f"{n}.png").convert("RGBA")
            base = im if base is None else Image.alpha_composite(base, im)
        tmp = VISOR / "_personaje.png"
        base.save(tmp)
        agregar("Personajes y fauna", "Personaje en capas", "chargen: cuerpo+ojos+ropa+pelo",
                tmp, [per / c / f"{n}.glb" for c, n in capas])
        tmp.unlink()

    # la fauna nueva: modelo de primitivas renderizado con cámara inclinada
    for g in sorted((A3 / "modelos/fauna").glob("*.glb")):
        png = A3 / "fuente/objetos/npc" / f"fauna_{g.stem}" / "base.png"
        if png.exists():
            agregar("Animales", {"cow": "Vaca", "chicken": "Gallina", "sheep": "Oveja", "pig": "Cerdo",
                                 "dog": "Perro"}.get(g.stem, g.stem), f"npc/fauna_{g.stem}", png, [g],
                    camara="inclinada")

    modelos = sorted(p for p in (A3 / "modelos").rglob("*.glb")
                     if "personajes" not in p.parts and "fauna" not in p.parts)
    orden = {k: i for i, k in enumerate(PREFERIDOS)}

    def objeto(p):
        return str(p.parent.relative_to(A3 / "modelos"))

    modelos.sort(key=lambda p: (orden.get(objeto(p), 999), objeto(p), p.name))
    vistos = set()
    for g in modelos:
        obj = objeto(g)
        cat = obj.split("/")[0]
        if cat not in NOMBRES and not a.todo:
            continue
        if not a.todo and (obj in vistos or cuenta.get(cat, 0) >= POR_CATEGORIA):
            continue
        png = A3 / "fuente/objetos" / obj / (g.stem + ".png")
        if not png.exists():
            continue
        # no mostrar las poses sueltas de una casa (puertas, techo) como objetos
        if not a.todo and "__" in g.stem and not g.stem.endswith("__base"):
            continue
        vistos.add(obj)
        cuenta[cat] = cuenta.get(cat, 0) + 1
        agregar(NOMBRES.get(cat, cat), obj.split("/")[-1].replace("_", " "), obj, png, [g])

    (VISOR / "lista.json").write_text(json.dumps(lista, ensure_ascii=False, indent=0))
    peso = sum(f.stat().st_size for f in dest.iterdir())
    print(f"{len(lista)} assets en el visor, {peso / 1e6:.1f} MB")


if __name__ == "__main__":
    main()
