#!/usr/bin/env python3
"""**Corrige a qué lote le toca cada dibujo** (5 oct 2026) sobre el manifest ya importado.

    python3 scripts/reasignar-lotes.py

Va DESPUÉS de `import-completos.py` / `import-edificios.py` (cuando corras la cadena de importadores en tu máquina, los
cambios de `import-completos.py::SLOTS` ya hacen esto solos y este script no encuentra nada que mover). Existe para dejar el
manifest actual bien sin la hoja cruda. Idempotente.

* Los caserones con torre de reloj (`Victorian_House_4/5`) NO son casas: pasan de `casa_country`/`casa_grande` a
  `building/ayuntamiento`.
* `building/granero` tenía por arte el escaparate de una tienda: copia los galpones de `building/deposito`.
"""
import json, os

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MANIFEST = os.path.join(ROOT, "native/crates/ha-game/assets/art/manifest.json")


def rect_de(e):
    b = e["poses"]["base"] if "poses" in e and "base" in e["poses"] else e
    return b.get("rect")


def main():
    man = json.load(open(MANIFEST))
    sp, ob = man["sprites"], man["objects"]
    movidos = 0
    # 1) ayuntamiento = los caserones (el rect coincide con `completo/additional_houses_victorian_house_4|5`).
    torres = [sp[k] for k in ("completo/additional_houses_victorian_house_4", "completo/additional_houses_victorian_house_5") if k in sp]
    rects = [rect_de(t) for t in torres]
    if torres and "building/ayuntamiento" not in sp:
        nuevos = []
        for lote in ("building/casa_country", "building/casa_grande"):
            for k in [k for k in list(sp) if k == lote or k.startswith(lote + "#")]:
                if rect_de(sp[k]) in rects:
                    nuevos.append(sp.pop(k))
                    movidos += 1
        if not nuevos:
            nuevos = [json.loads(json.dumps(t)) for t in torres]
        for i, e in enumerate(nuevos[:2]):
            sp["building/ayuntamiento" + ("" if i == 0 else f"#{i}")] = e
        ob["building/ayuntamiento"] = {"label": "Ayuntamiento",
                                       "variants": [{"id": "1", "label": "Ayuntamiento 1"}]}
    # 2) granero = galpones del depósito.
    dep = [k for k in sp if k == "building/deposito" or k.startswith("building/deposito#")]
    if dep:
        for k in [k for k in list(sp) if k == "building/granero" or k.startswith("building/granero#")]:
            del sp[k]
        for k in dep:
            sp[k.replace("building/deposito", "building/granero")] = json.loads(json.dumps(sp[k]))
        movidos += len(dep)
    print(f"{movidos} entradas reasignadas")
    json.dump(man, open(MANIFEST, "w"), indent=1, ensure_ascii=False)


main()
