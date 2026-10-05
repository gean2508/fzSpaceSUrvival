#!/usr/bin/env python3
"""**LOS VEHÍCULOS QUE SE MANEJAN: una clave por modelo, cuatro poses de rumbo.**

    python3 scripts/import-vehiculos.py
    python3 scripts/import-vehiculos.py --dry-run

## Por qué existe

El importador masivo dejó los autos del pack como claves sueltas por DIRECCIÓN
(`vehicle/auto_abajo_1`, `vehicle/auto_arriba_14`…) agrupando colores como
variantes, y de los 26 modelos de `10_Vehicles_Singles` entraron cuatro. Eso
sirve para estacionar uno de adorno, no para manejarlo: un auto que se maneja
necesita **las cuatro vistas del MISMO auto** —el rojo que doblás tiene que
seguir siendo rojo— y pedirlas por rumbo.

Este script publica `vehicle/<modelo>` con poses `abajo`/`arriba`/`izquierda`/
`derecha` (y `*_luz` para la ambulancia: el segundo cuadro del pack es la sirena
encendida). El juego (`ha-game/src/vehiculos.rs`) elige la pose por el rumbo.

## Los rects se MIDEN

Cada pieza del pack trae margen transparente distinto según el modelo (el
compacto 64×128 empieza en y=12, el SUV 96×128 en x=14). Se recorta a la caja de
píxeles opacos (`getbbox`) y el rect se publica en píxeles (`grid [1,1]`): el
juego centra el sprite en el vehículo, y con margen el auto quedaba corrido
respecto de su propia colisión. Si el pack cambia de tamaño una pieza, el rect
cambia solo; si DESAPARECE una, el script se planta (`falta`).

## De dónde sale cada cosa

| clave | piezas | qué es |
|---|---|---|
| `vehicle/coche_1..26` | `Car_{Dir}_N` | 4 carrocerías × 6-7 colores (el 11 es el taxi) |
| `vehicle/colectivo_1..6` | `Bus_{Dir}_N` | colectivos urbanos |
| `vehicle/casa_rodante` | `Camper_{Dir}_1` | la casa rodante |
| `vehicle/ambulancia` | `Ambulance_{Dir}_1/2` | con sirena (`*_luz`) |
| `vehicle/lancha` | `Boat_1_{Dir}_1` | la lancha (agua) |
| `vehicle/camion` | `Delivery_Truck` (hoja animada, celda 288 px) | camión de reparto |
| `vehicle/basura`, `vehicle/bomberos` | claves ya medidas del manifest | camión de basura y autobomba |

⚠ `vehicle/camion_*` del importador masivo NO es un camión: es la autobomba sin
escalera de la hoja de bomberos. El de reparto sale de `Delivery_Truck`.

Moto y bici no están en el pack: las dibuja `scripts/dibujar-vehiculos.py`.
"""

import argparse
import json
import os

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACK = os.path.join(ROOT, "PRUEBAS-CODIGOS")
SINGLES = os.path.join(
    PACK,
    "modernexteriors-win/Modern_Exteriors_32x32/ME_Theme_Sorter_32x32",
    "10_Vehicles_Singles_32x32",
)
PREFIJO = "ME_Singles_Vehicles_32x32_"
ASSETS = os.path.join(ROOT, "native/crates/ha-game/assets")
MANIFEST = os.path.join(ASSETS, "art/manifest.json")
PAGINA_REL = "art/pack/vehiculos.png"
INDEX = os.path.join(ROOT, "scripts/vehiculos-import-index.json")

#: Ancho de la hoja. 2048 entra en cualquier GPU de celular (el Mali-G52 acepta 8192).
ANCHO = 2048
#: Aire entre piezas: el filtrado no puede mezclar el borde de un auto con el vecino.
AIRE = 2

DIRS = {"Down": "abajo", "Up": "arriba", "Left": "izquierda", "Right": "derecha"}

#: Nombres por carrocería, medidos mirando la hoja de contacto (el número del
#: archivo agrupa por forma: 1-6 hatchback, 7-13 sedán, 14-20 SUV, 21-26 compacto).
CARROCERIA = [(range(1, 7), "Hatchback"), (range(7, 14), "Sedán"), (range(14, 21), "SUV"), (range(21, 27), "Compacto")]

#: Camiones que ya están medidos en el manifest (los importó el masivo desde las
#: hojas de bomberos y de props): se copia el recorte, no se re-mide a ojo.
CAMIONES = {
    "basura": ("Camión de basura", "vehicle/camion_de_basura_naranja"),
    "bomberos": ("Autobomba", "vehicle/bomberos_camion_escalera_de_mano"),
}


def falta(ruta):
    raise SystemExit(f"falta la pieza del pack: {ruta} — ¿cambió el pack?")


def single(nombre):
    ruta = os.path.join(SINGLES, PREFIJO + nombre + ".png")
    if not os.path.exists(ruta):
        falta(ruta)
    return recortar(Image.open(ruta).convert("RGBA"))


def recortar(im):
    caja = im.getbbox()
    if caja is None:
        raise SystemExit("pieza vacía")
    return im.crop(caja)


class Hoja:
    """Empaquetado por estantes: cada pieza a la derecha de la anterior, y fila
    nueva cuando no entra. Devuelve el rect en píxeles."""

    def __init__(self):
        self.piezas = []
        self.x = AIRE
        self.y = AIRE
        self.alto_fila = 0

    def poner(self, im):
        w, h = im.size
        if self.x + w + AIRE > ANCHO:
            self.x = AIRE
            self.y += self.alto_fila + AIRE
            self.alto_fila = 0
        rect = {"col": self.x, "row": self.y, "w": w, "h": h}
        self.piezas.append((im, self.x, self.y))
        self.x += w + AIRE
        self.alto_fila = max(self.alto_fila, h)
        return rect

    def imagen(self):
        alto = self.y + self.alto_fila + AIRE
        alto = (alto + 31) // 32 * 32
        im = Image.new("RGBA", (ANCHO, alto), (0, 0, 0, 0))
        for p, x, y in self.piezas:
            im.paste(p, (x, y))
        return im


def modelos_del_pack():
    """`[(clave, etiqueta, {pose: Image})]` de las piezas sueltas."""
    out = []
    for n in range(1, 27):
        forma = next(nombre for r, nombre in CARROCERIA if n in r)
        etiqueta = "Taxi (manejable)" if n == 11 else f"Coche {forma.lower()} {n}"
        out.append((f"coche_{n}", etiqueta, {v: single(f"Car_{k}_{n}") for k, v in DIRS.items()}))
    for n in range(1, 7):
        out.append((f"colectivo_{n}", f"Colectivo {n}", {v: single(f"Bus_{k}_{n}") for k, v in DIRS.items()}))
    out.append(("casa_rodante", "Casa rodante", {v: single(f"Camper_{k}_1") for k, v in DIRS.items()}))
    amb = {v: single(f"Ambulance_{k}_1") for k, v in DIRS.items()}
    amb.update({f"{v}_luz": single(f"Ambulance_{k}_2") for k, v in DIRS.items()})
    out.append(("ambulancia", "Ambulancia", amb))
    out.append(("lancha", "Lancha", {v: single(f"Boat_1_{k}_1") for k, v in DIRS.items()}))
    return out


def camiones(man):
    """Los camiones ya medidos: se recortan de su hoja con el rect publicado."""
    out = []
    hoja = man["sprites"]
    for clave, (etiqueta, base) in CAMIONES.items():
        poses = {}
        for v in DIRS.values():
            e = hoja.get(f"{base}_{v}")
            if e is None or "rect" not in e:
                falta(f"{base}_{v} en el manifest")
            gw, gh = e.get("grid", [32, 32])
            r = e["rect"]
            src = Image.open(os.path.join(ASSETS, e["png"])).convert("RGBA")
            caja = (r["col"] * gw, r["row"] * gh, (r["col"] + r["w"]) * gw, (r["row"] + r["h"]) * gh)
            poses[v] = recortar(src.crop(caja))
        out.append((clave, etiqueta, poses))
    return out


ANIMADOS = os.path.join(PACK, "modernexteriors-win/Modern_Exteriors_32x32/Animated_32x32/Vehicles_32x32")
#: Celda de la hoja `Delivery_Truck` (el pack lo escribe en la propia hoja: «TILE
#: SIZE 144X144» a 16 px, o sea 288 a 32). Fila 0: un cuadro por rumbo.
CELDA_REPARTO = 288
REPARTO_COLUMNAS = {"derecha": 0, "arriba": 1, "izquierda": 2, "abajo": 3}


def camion_de_reparto():
    ruta = os.path.join(ANIMADOS, "Delivery_Truck_32x32.png")
    if not os.path.exists(ruta):
        falta(ruta)
    src = Image.open(ruta).convert("RGBA")
    poses = {}
    for pose, col in REPARTO_COLUMNAS.items():
        c = CELDA_REPARTO
        im = recortar(src.crop((col * c, 0, (col + 1) * c, c)))
        w, h = im.size
        # Lo que TIENE que medir: de costado más largo que alto, de frente al revés.
        acostado = pose in ("derecha", "izquierda")
        if acostado != (w > h):
            raise SystemExit(f"Delivery_Truck: la pose {pose} mide {w}x{h} — ¿cambió el orden de la hoja?")
        poses[pose] = im
    return [("camion", "Camión de reparto", poses)]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    man = json.load(open(MANIFEST))
    todos = modelos_del_pack() + camion_de_reparto() + camiones(man)

    # Las piezas grandes primero: los estantes quedan más parejos.
    hoja = Hoja()
    sprites = {}
    objetos = {}
    for clave, etiqueta, poses in todos:
        rects = {}
        for pose, im in sorted(poses.items(), key=lambda kv: -kv[1].size[1]):
            rects[pose] = {"rect": hoja.poner(im)}
        # La pose de reposo (`rect` de la entrada) es la de frente: es la que
        # muestra el editor y el catálogo.
        sprites[f"vehicle/{clave}"] = {
            "png": PAGINA_REL,
            "grid": [1, 1],
            "rect": rects["abajo"]["rect"],
            "poses": rects,
        }
        objetos[f"vehicle/{clave}"] = etiqueta
        tam = {p: f"{im.size[0]}x{im.size[1]}" for p, im in poses.items() if "_" not in p}
        print(f"vehicle/{clave}: {etiqueta} · {tam}")

    im = hoja.imagen()
    if args.dry_run:
        print(f"\n[dry-run] hoja {im.size[0]}x{im.size[1]} px, {len(sprites)} vehículos")
        return
    im.save(os.path.join(ASSETS, PAGINA_REL), optimize=True)
    print(f"\n{PAGINA_REL}: {im.size[0]}x{im.size[1]} px")

    s = man.setdefault("sprites", {})
    if os.path.exists(INDEX):
        for k in json.load(open(INDEX)).get("sprites", []):
            s.pop(k, None)
    s.update(sprites)
    objs = man.setdefault("objects", {})
    for clave, etiqueta in objetos.items():
        objs.setdefault(clave, {})["label"] = etiqueta
    with open(MANIFEST, "w") as f:
        json.dump(man, f, indent=1, ensure_ascii=False, sort_keys=True)
    with open(INDEX, "w") as f:
        json.dump({"sprites": sorted(sprites), "pagina": PAGINA_REL}, f, indent=1, ensure_ascii=False)
    print(f"manifest: {len(sprites)} vehículos")


if __name__ == "__main__":
    main()
