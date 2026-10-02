#!/usr/bin/env python3
"""**AUDITORÍA: edificios COMPLETOS vs piezas de KIT** (1 oct 2026).

    python3 scripts/auditar-completos-vs-modulares.py [--estricto]

El pack mezcla casas enteras, casas por piezas (kits modulares) y, en la misma página, edificios
enteros de clases distintas. Esta auditoría lee el manifest YA importado y responde tres cosas:

1. ¿Algún lote de la ciudad (`building/<lote>` y sus `#variantes`) se alimenta de una PIEZA de kit?
   (`scripts/edificios-import-index.json::origen` dice de qué claves sale cada lote; se las pasa por
   las mismas marcas que usa el importador.) → error con `--estricto`.
2. ¿Qué edificios COMPLETOS importados no usa ningún lote? (candidatos a asignar, no a olvidar).
3. ¿Cuántas piezas de kit hay por familia y dónde se componen? (informativo.)
"""
import json, re, sys
from pathlib import Path

RAIZ = Path(__file__).resolve().parent.parent
MAN = json.load(open(RAIZ / "native/crates/ha-game/assets/art/manifest.json"))["sprites"]
IDX = json.load(open(RAIZ / "scripts/edificios-import-index.json"))

# Las mismas marcas que `import-edificios.py` (se leen del script para no duplicarlas).
src = open(RAIZ / "scripts/import-edificios.py", encoding="utf8").read()
def bloque(nombre):
    m = re.search(nombre + r" = tuple\(\s*re\.compile\(p\)\s*for p in \((.*?)\n    \)\n\)", src, re.S)
    return [re.search(r'r"(.*?)"', l).group(1) for l in m.group(1).splitlines() if re.search(r'^\s*r"', l)] if m else []
PIEZA = [re.compile(p) for p in bloque("MARCAS_DE_PIEZA") + bloque("NO_SON_FACHADA")]
es_pieza = lambda clave: any(rx.search(clave.split("/", 1)[-1]) for rx in PIEZA)

usadas = set()
malos = []
for lote, fuentes in IDX["origen"].items():
    for f in fuentes:
        usadas.add(f)
        if es_pieza(f):
            malos.append((lote, f))

completos, kits = [], []
for k, e in MAN.items():
    if not k.startswith("building/") or "#" in k or not isinstance(e, dict):
        continue
    r = e.get("rect")
    if not r:
        continue
    (kits if es_pieza(k) else completos).append((k, r["w"], r["h"]))

# Los slots de lote (`building/hospital`…) no son "fuentes": son EL lote. Y lo angosto y alto
# (6x25) es una torre del kit Victoriano, no una casa.
LOTES = set(re.findall(r'LotProgram \{ key: "([^"]+)"', open(RAIZ / "native/crates/ha-shared/src/city.rs", encoding="utf8").read())) | set(IDX["origen"])
sin_lote = [c for c in completos if c[0] not in usadas and c[0] not in LOTES and c[1] >= 5 and c[2] >= 5 and c[2] <= 2.2 * c[1]
            and not re.match(r"building/(casa_chica|casa_mediana|casa_grande|casa_country|villa|duplex|tienda|mercado|shopping)(#|$)", c[0])]
print(f"edificios completos importados: {len(completos)}   piezas de kit: {len(kits)}")
print(f"fuentes de lote que son PIEZAS de kit: {len(malos)}")
for lote, f in malos:
    print("   ✗", lote, "←", f)
print(f"\ncompletos (>=5x5 celdas) que NINGÚN lote usa: {len(sin_lote)}")
for k, w, h in sorted(sin_lote, key=lambda c: -c[1] * c[2])[:60]:
    print(f"   · {k}  {w}x{h}")
if "--estricto" in sys.argv and malos:
    sys.exit(1)
