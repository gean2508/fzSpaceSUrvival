#!/usr/bin/env python3
"""**Lo que la revisión visual vio, aplicado.**

    python3 scripts/aplicar-revision-visual.py              # escribe
    python3 scripts/aplicar-revision-visual.py --verificar  # sólo mira; 1 si algo se desfasó

## Por qué existe

El importador masivo bautiza por el NOMBRE DE ARCHIVO del pack, y Modern Interiors
viene numerado (`bano_121`, `gimnasio_117`): el nombre no dice qué es. El 29 sep
2026 se miraron las 9.799 claves base del manifest, una por una, en láminas de
100 (`scripts/revision-visual.json` es el resultado). Salió lo que el usuario
venía viendo en el juego:

- **Cosas de pared sembradas en el piso**: espejos del gimnasio, termotanques del
  baño, cuadros del museo, televisores del estudio, carteles del subte.
- **Nombres que mienten**: los `balde_chico_*` son salvavidas, `bano_120` y sus
  hermanos son heladeras, la mitad de `vehicle/` son mesas y sillas del patio de
  comidas, `vehicle/helicoptero_1` es un teléfono público y `sombra/int_1..19`
  son las LETRAS de un cartel, no sombras.
- **Piezas de kit como si fueran objetos**: frazadas sin cama, tramos de mesada,
  módulos de sofá, pedazos de vitrina.

## Qué hace

1. **Nombre**: pone el nombre corregido en `objects[clave].label` del manifest,
   en el `("clave", "…")` de `ha-assets/src/slots.rs` y en el menú de
   construcción (`ha-shared/src/placeables.rs`).
2. **Capa**: lo marcado `S` (plano en el suelo) va a la banda del piso
   (`layer: Floor`), así un charco o una rejilla no te tapan los pies.
3. **Siembra**: saca de `pack_siembra.rs` lo marcado `X`, `M` o `P`.
   `interior_siembra.rs` no se toca acá: lo regenera `import-interiores.py`, que
   ya lee esta revisión y rellena la tabla con muebles de verdad.

Idempotente. Lo llama `import-pack-masivo.py` al terminar, porque ese importador
reescribe el manifest, `slots.rs` y `pack_siembra.rs` desde el pack y se llevaría
puesto todo lo de acá.
"""

import json
import re
import sys
from pathlib import Path

RAIZ = Path(__file__).resolve().parent.parent
REVISION = RAIZ / "scripts/revision-visual.json"
MANIFEST = RAIZ / "native/crates/ha-game/assets/art/manifest.json"
SLOTS = RAIZ / "native/crates/ha-assets/src/slots.rs"
PLACEABLES = RAIZ / "native/crates/ha-shared/src/placeables.rs"
PACK_SIEMBRA = RAIZ / "native/crates/ha-shared/src/pack_siembra.rs"

#: Lo que no sale sorteado en la calle, el parque o la playa. `S` sí puede: una
#: rejilla o una tapa de alcantarilla en la vereda es exactamente lo que va ahí.
NO_SE_SIEMBRA_AFUERA = set("XMP")


def cargar():
    return json.loads(REVISION.read_text())["claves"]


def flags(rev, clave):
    return set(rev.get(clave, {}).get("flags", ""))


def aplicar_manifest(rev, man):
    cambios = 0
    objetos = man.setdefault("objects", {})
    sprites = man["sprites"]
    for clave, d in rev.items():
        if clave not in sprites:
            continue
        label = d.get("label")
        if label and objetos.get(clave, {}).get("label") != label:
            objetos.setdefault(clave, {})["label"] = label
            cambios += 1
        if "S" in d.get("flags", "") and "P" not in d.get("flags", ""):
            for k in [clave] + [k for k in sprites if k.startswith(clave + "#")]:
                if "layer" not in sprites[k]:
                    sprites[k]["layer"] = "Floor"
                    cambios += 1
    return cambios


def aplicar_slots(rev, texto):
    cambios = 0

    def cambiar(m):
        nonlocal cambios
        clave, viejo = m.group(1), m.group(2)
        nuevo = rev.get(clave, {}).get("label")
        if not nuevo or nuevo == viejo:
            return m.group(0)
        cambios += 1
        return f'("{clave}", "{nuevo}")'

    texto = re.sub(r'\("([a-z0-9_]+/[^"#]+)", "([^"]*)"\)', cambiar, texto)
    return texto, cambios


def aplicar_colocables(rev, texto):
    """El nombre del menú de construcción: `p("id", "Nombre", "clave", …)`."""
    cambios = 0

    def cambiar(m):
        nonlocal cambios
        nuevo = rev.get(m.group(3), {}).get("label")
        if not nuevo or nuevo == m.group(2):
            return m.group(0)
        cambios += 1
        return f'p("{m.group(1)}", "{nuevo}", "{m.group(3)}"'

    texto = re.sub(r'p\("([^"]+)", "([^"]*)", "([^"]+)"', cambiar, texto)
    return texto, cambios


def aplicar_siembra(rev, texto):
    fuera = []
    lineas = []
    for ln in texto.splitlines(keepends=True):
        m = re.search(r'key: "([^"]+)"', ln)
        if m and flags(rev, m.group(1)) & NO_SE_SIEMBRA_AFUERA:
            fuera.append(m.group(1))
            continue
        lineas.append(ln)
    return "".join(lineas), fuera


def main():
    verificar = "--verificar" in sys.argv
    rev = cargar()
    man = json.loads(MANIFEST.read_text())
    c_man = aplicar_manifest(rev, man)
    slots, c_slots = aplicar_slots(rev, SLOTS.read_text())
    colocables, c_col = aplicar_colocables(rev, PLACEABLES.read_text())
    c_slots += c_col
    siembra, fuera = aplicar_siembra(rev, PACK_SIEMBRA.read_text())
    if verificar:
        if c_man or c_slots or fuera:
            print(
                f"desfasado: manifest {c_man} · slots.rs {c_slots} · "
                f"siembra {len(fuera)} ({', '.join(fuera[:8])})"
            )
            return 1
        print(f"revisión visual al día ({len(rev)} claves)")
        return 0
    MANIFEST.write_text(json.dumps(man, ensure_ascii=False, indent=1))
    SLOTS.write_text(slots)
    PLACEABLES.write_text(colocables)
    PACK_SIEMBRA.write_text(siembra)
    print(f"manifest: {c_man} cambios · slots.rs + colocables: {c_slots} nombres · siembra: {len(fuera)} fuera")
    for k in fuera:
        print(f"  fuera de la siembra: {k} ({rev[k].get('label') or ''} {rev[k].get('flags')})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
