"""**De qué pack salió cada página de atlas** — exterior o interior.

    from lib.origen import marcar_origen
    marcar_origen(man, paginas_escritas, "exterior")

## Por qué existe

El juego mezcla dos packs de LimeZu que dibujan cosas parecidas con los mismos
nombres: `Modern Exteriors` (la calle, las casas, los edificios) y
`Modern Interiors` (los muebles, las góndolas, los mostradores). Los dos traen
temas llamados `Office`, `Hospital`, `Condominium`, `Ice_Cream_Shop`, y los dos
usan las palabras `store`, `shop`, `house`, `wall`, `plant` en el nombre de sus
archivos. Sin un dato explícito de origen, cualquier filtro por NOMBRE se
equivoca — y se equivocó: `building/heladeria_1` (el mostrador de una heladería
de interiores, 2×3 celdas) llegó a ser una parcela de la ciudad, y las góndolas
del almacén y las perchas de la tienda de ropa quedaron en la categoría
"Edificio" del menú de construir.

El origen no se puede deducir después: es un dato del IMPORTADOR, que es el único
que sabe qué carpeta del pack leyó. Así que cada importador lo declara sobre las
páginas que escribe, y queda publicado en `art/manifest.json` bajo `origen`:

    "origen": { "art/pack/villas_1.png": "exterior",
                "art/pack/clothing_store_0.png": "interior" }

Del otro lado, `ha-game::art2d` lo lee y los tests se plantan si algo de afuera
—un lote de la ciudad, una tabla de siembra, un slot `building/*`— termina
apuntando a un dibujo de adentro.

## Mixto no es un valor, es un error

Una página con dibujos de los dos packs no se puede filtrar, así que se marca
`"mixto"` y el test la caza. Nunca se resuelve por mayoría.
"""

EXTERIOR = "exterior"
INTERIOR = "interior"
#: Ni de afuera ni de adentro: iconos, tipografía, cursores. No se filtra.
UI = "ui"


def marcar_origen(man, paginas, origen):
    """Anota el origen de estas páginas en el manifest, en su lugar.

    `paginas` son rutas relativas tal como van en el manifest
    (`art/pack/villas_1.png`) o nombres pelados de página (`villas_1`), que se
    normalizan. Devuelve la lista de claves escritas para que el importador la
    guarde en su índice y en la corrida siguiente pise **lo suyo**.
    """
    assert origen in (EXTERIOR, INTERIOR, UI), origen
    tabla = man.setdefault("origen", {})
    escritas = []
    for p in paginas:
        clave = p if p.startswith("art/") else f"art/pack/{p}.png"
        if not clave.endswith(".png"):
            clave += ".png"
        previo = tabla.get(clave)
        tabla[clave] = origen if previo in (None, origen) else "mixto"
        escritas.append(clave)
    return sorted(set(escritas))


def olvidar_origen(man, claves):
    """Borra las anotaciones de una corrida anterior. Va antes de `marcar_origen`."""
    tabla = man.setdefault("origen", {})
    for k in claves or ():
        tabla.pop(k, None)
