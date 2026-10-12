#!/usr/bin/env python3
"""Inserta lo que generó `import-pack-objects.py` entre los marcadores
`// === PACK:START ===` y `// === PACK:END ===` de `slots.rs` y `placeables.rs`.

    python3 scripts/import-pack-objects.py scripts/pack-objects.tsv
    python3 scripts/apply-pack-tables.py

Reemplaza el bloque ENTERO: lo que edites ahí dentro se pierde. Lo curado a mano
va fuera de los marcadores.
"""
import sys
N='/home/fzion/Documents/mods/phaser-arena/native'
def put(path, gen):
    s=open(path).read()
    a=s.index('// === PACK:START ==='); b=s.index('// === PACK:END ===')
    body=open(gen).read().rstrip()+'\n    '
    s=s[:a]+'// === PACK:START ===\n'+body+s[b:]
    open(path,'w').write(s)
put(f'{N}/crates/ha-assets/src/slots.rs', '/tmp/gen_slots.txt')
put(f'{N}/crates/ha-shared/src/placeables.rs', '/tmp/gen_places.txt')
print("insertado")
