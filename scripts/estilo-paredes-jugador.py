#!/usr/bin/env python3
"""Las paredes que construye el jugador (`wall/wood`, `wall/stone`) dejan de usar el andamio naranja de
`construccion.png` y toman las caras del paño de interior del pack (`wall/int_madera_1`, `wall/int_gris_1`).

Se DERIVA del arte ya publicado en el manifest (nada de rects a ojo): cada pose del jugador apunta a la
pose equivalente de la familia de interior. Idempotente: se puede correr después de cualquier importador.

    front  -> front         left -> end_w        center -> front
    right  -> end_e         back -> center       espalda -> center
"""
import json, sys, copy
RUTA = "native/crates/ha-game/assets/art/manifest.json"
MAPA = {"front": "front", "left": "end_w", "right": "end_e", "center": "front", "back": "center", "espalda": "center"}
PARES = {"wall/wood": "wall/int_madera_1", "wall/stone": "wall/int_gris_1"}

m = json.load(open(RUTA))
spr = m["sprites"]
for destino, origen in PARES.items():
    o, d = spr[origen], spr[destino]
    poses = {p: copy.deepcopy(o["poses"][src]) for p, src in MAPA.items() if src in o["poses"]}
    d["png"] = o["png"]
    d["grid"] = o["grid"]
    d["poses"] = poses
    d["rect"] = copy.deepcopy(o["poses"]["front"]["rect"])
    print("ok", destino, "<-", origen, sorted(poses))
json.dump(m, open(RUTA, "w"), indent=1, ensure_ascii=False)
