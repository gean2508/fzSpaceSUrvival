#!/usr/bin/env python3
"""**EL SLOT CANÓNICO APUNTA AL PIXEL QUE YA ESTÁ.**

    python3 scripts/asignar-del-pack.py [--dry-run]

## El error que esto repara

El 14 ago 2026 se dibujaron 132 piezas para tapar los slots vacíos del catálogo.
La mitad **no hacía falta**: el arte del pack no sólo existía, ya estaba
importado en el manifest — con el nombre traducido al castellano, que es
justamente por qué las búsquedas en inglés no lo encontraban. `vehicle/bus` se
dibujó teniendo `vehicle/colectivo_abajo`; `agua/estrella_mar`, teniendo seis
estrellas de mar en dos tamaños; `furniture/inodoro`, teniendo 180 muebles de
baño de Modern Interiors.

Lo que faltaba nunca fue pixel: era **decir cuál de las 13.413 claves le toca a
cada slot canónico**. Eso es esto.

## Cómo mirar antes de decidir

Dos herramientas, y hay que usarlas *antes* de dibujar:

    python3 scripts/ver-claves.py boya bote muelle    # lo YA importado
    python3 scripts/ver-pack.py "*Yard_Toy*"          # los PNG crudos del pack

Cada entrada de [`ASIGNACIONES`] se eligió mirando la lámina, no por el nombre:
el importador masivo bautiza automático y erra seguido (`prop/pozos_1` se llama
"pozos" y es un charco; `building/juguete_casa_3` es una casa, no un tobogán).

## Qué hace

Copia la entrada de sprite de la clave de origen a la clave del slot. No mueve
pixel ni escribe PNG: el mismo recorte queda accesible por los dos nombres. Si el
usuario le asigna otra cosa desde HA Studio, gana la suya.
"""

import json
import sys
from pathlib import Path

RAIZ = Path(__file__).resolve().parent.parent
MANIFEST = RAIZ / "native/crates/ha-game/assets/art/manifest.json"

#: `slot canónico → (clave que ya está en el manifest, nombre humano)`.
#: **Todas verificadas mirando el recorte**, no el nombre.
ASIGNACIONES = {
    # — agua: el kit de camping y playa estaba entero —
    "agua/boya": ("agua/boya_1", "Boya"),
    "agua/bote": ("agua/bote_abajo_1", "Bote"),
    "agua/muelle": ("agua/camping_muelle_2", "Muelle"),
    "agua/estrella_mar": ("agua/estrella_de_mar_azul_grande", "Estrella de mar"),
    "agua/flotador": ("agua/salvavidas", "Salvavidas"),
    "agua/tronco_flotante": ("tiles/floating_trunk", "Tronco flotante"),
    "fishing/balde": ("agua/balde_chico_azul_1", "Balde"),
    "fishing/spot": ("agua/pesca_1", "Sitio de pesca"),
    # — parcela —
    "prop/parrilla": ("prop/camping_parrilla", "Parrilla"),
    "prop/arenero": ("nature/villa_patio_juguete_arena_caja_1", "Arenero"),
    "prop/hamaca": ("building/villa_patio_juguete_hamaca_1", "Hamacas"),
    "prop/buzon_casa": ("building/buzon_grande_azul", "Buzón"),
    "prop/cantero": ("azotea/cantero", "Cantero"),
    "prop/auto_estacionado": ("vehicle/auto_1", "Auto estacionado"),
    "prop/arco_basket": ("building/basquet_red_1", "Aro de básquet"),
    "prop/tobogan": ("building/patio_juguete_1", "Tobogán"),
    "prop/pileta_borde": ("agua/pileta_30", "Borde de pileta"),
    "prop/tendedero": ("anim/clothes_hanging_1", "Tendedero"),
    "fence/seto": ("nature/arbusto_5", "Seto"),
    # Las cercas son 378 claves de autotile del pack; acá va la pieza de tramo
    # de cada familia, que es lo que el slot canónico pide.
    "fence/madera": ("fence/cerca_5", "Cerca de madera"),
    "fence/alambre": ("fence/cerca_11", "Cerca de alambre"),
    "fence/piedra": ("fence/cerca_abajo_medio", "Muro de piedra"),
    # — naturaleza —
    "nature/tocon": ("nature/tocon_1", "Tocón"),
    "nature/pasto": ("nature/pasto_5", "Matas de pasto"),
    "nature/flores_rojas": ("nature/arbusto_con_flores_1", "Flores"),
    # — vehículos y estructuras —
    "vehicle/bus": ("vehicle/colectivo_abajo", "Colectivo"),
    "vehicle/tanque": ("building/tanque_arma_marron_derecha_costado_1", "Tanque"),
    "structure/boulder": ("nature/piedra_1", "Peñasco"),
    "structure/monument": ("nature/estatua_angel", "Monumento"),
    "decor/mine_hole": ("building/mina_1", "Boca de mina"),
    # — muebles: Modern Interiors entero estaba importado —
    "furniture/inodoro": ("furniture/bano_31", "Inodoro"),
    "furniture/banera": ("furniture/bano_156", "Bañera"),
    "furniture/lampara": ("furniture/living_80", "Lámpara de pie"),
    "furniture/planta": ("furniture/living_13", "Planta de maceta"),
    "furniture/ropero": ("furniture/living_41", "Ropero"),
    "furniture/mostrador": ("furniture/living_53", "Mostrador"),
    "furniture/estante": ("building/carpa_estante", "Estante"),
    "furniture/escritorio": ("prop/mesa_1", "Escritorio"),
    # — edificios y fachadas —
    "building/escuela": ("building/escuela_1", "Escuela"),
    "building/garaje": ("building/venta_de_garaje_edificio_puerta", "Garaje"),
    "building/universidad": ("building/hospital", "Universidad"),
    "doorway/fachada_local": ("door/fachada_local", "Fachada de local"),
    "doorway/fachada_portal": ("door/fachada_portal", "Portal de fachada"),
    # — varios que estaban a la vista y no se buscaron —
    "prop/trampolin": ("anim/trampoline", "Trampolín"),
    "ui/lantern_button": ("items/lantern", "Botón de linterna"),
    # — items —
    "items/apple": ("prop/manzanas_1", "Manzana"),
}


def main():
    seco = "--dry-run" in sys.argv
    man = json.loads(MANIFEST.read_text())
    sp = man["sprites"]
    faltan = [o for _, (o, _) in ASIGNACIONES.items() if o not in sp]
    if faltan:
        raise SystemExit(f"claves de origen que no están en el manifest: {faltan}")
    for slot, (origen, label) in ASIGNACIONES.items():
        sp[slot] = json.loads(json.dumps(sp[origen]))
        man.setdefault("objects", {}).setdefault(slot, {})["label"] = label
    if seco:
        print(f"[dry-run] {len(ASIGNACIONES)} slots apuntarían a arte del pack")
        return 0
    MANIFEST.write_text(json.dumps(man, ensure_ascii=False, indent=1))
    print(f"{len(ASIGNACIONES)} slots canónicos apuntando a arte del pack ya importado")
    return 0


if __name__ == "__main__":
    sys.exit(main())
