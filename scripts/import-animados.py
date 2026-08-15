#!/usr/bin/env python3
"""**LO QUE EL PACK DIBUJÓ ANIMADO ENTRA ANIMADO.**

    python3 scripts/import-animados.py
    python3 scripts/import-animados.py --dry-run

## Por qué existe

`import-pack-masivo.py` mete los 11.691 recortes SUELTOS de Modern Exteriors y
Modern Interiors. Los dos packs traen además carpetas de **hojas de animación**
que ese script no mira, y ahí está la vida del mundo: las fuentes, el fuego, el
agua que se mueve, los molinos, las banderas, las gaviotas, las barreras
automáticas, la maquinaria. 842 hojas que estaban compradas y no las veía nadie.

    Modern_Exteriors/Animated_32x32/Animated_sheets_32x32     451
    Modern_Exteriors/Animated_32x32/Animated_Terrains_32x32    10
    Modern_Exteriors/Animated_32x32/Vehicles_32x32             73
    Modern_Interiors/3_Animated_objects/32x32                 310

## Por qué NO entran las 842 enteras

Medido antes de escribir una línea: las hojas suman **536 megapíxeles**, o sea
511 páginas de atlas de 1024×1024 — unos 2 GB de textura. En un teléfono de
3,8 GB eso no es una decisión de gusto, es el OOM que ya se documentó en
`import-pack-masivo.py`. Y no hace falta: una hoja de puerta trae 22 fotogramas
del batiente abriéndose y el bucle de un molino se lee con cuatro.

Así que se recorta por tres reglas, y las tres están medidas, no elegidas a ojo:

1. **[`MAX_FRAMES`] fotogramas por objeto.** Un ciclo se lee con eso; el resto es
   peso. Los fotogramas se toman del principio, que es donde empieza el bucle.
2. **[`TOPE_CELDAS`] celdas de lado por fotograma.** Lo más grande del lote es
   una hoja de 798 fotogramas de composición completa: eso no es un objeto, es
   una escena.
3. **Las PUERTAS no entran acá** ([`es_puerta`]). El juego ya tiene puertas con
   su sistema de poses `open`/`closed` (`building::DoorArt`), y meter 128 hojas
   de batiente como colocables sueltos sería ruido encima de algo que ya
   funciona. Es la regla 8 del proyecto: antes de colocar algo, preguntarse si es
   una PIEZA de un kit que ya existe.

## Cómo se sabe cuántos fotogramas tiene una hoja

No lo dice el nombre ni el tamaño. Se probó lo obvio —"el objeto mide de ancho lo
que la hoja mide de alto"— y falla: el molino es una hoja de 16×8 celdas con
CUATRO molinos, no dos.

Lo que sí funciona es buscar el **período** ([`periodo`]): en una animación, la
parte quieta del dibujo (el marco de la puerta, la torre del molino, el pilón de
la fuente) se repite idéntica cada fotograma. Se prueba cada ancho que divida a
la hoja y se queda el MÁS CHICO cuyos fotogramas se parecen entre sí; un ancho
equivocado desalinea la parte quieta y la diferencia se dispara.

## Qué escribe

- `art/pack/anim_<n>.png` — páginas de atlas de 1024 px de lado. Cada tira entra
  entera y contigua (es lo que pide `strip` del manifest), acostada o de pie
  según cuál de las dos quepa.
- `art/manifest.json` — una entrada por objeto con `strip` (el rect del PRIMER
  fotograma), `frames`, `fps` y `strip_dir`.
- El bloque `// === ANIM:START/END ===` de `ha-assets/src/slots.rs` y de
  `ha-shared/src/placeables.rs`. Lo segundo es lo que hace que el objeto se pueda
  **poner** desde el juego: un slot sin colocable existe en el editor y no lo
  usa nadie, que es exactamente el estado del que venimos.

Todo queda anotado en `scripts/animados-import-index.json`: la corrida siguiente
borra **lo suyo** y nada más, así el arte curado a mano en HA Studio no se pisa
(la lección de `import-pack-masivo-es-idempotente`).
"""

import argparse
import hashlib
import json
import os
import re
import unicodedata
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
PACK = ROOT / "PRUEBAS-CODIGOS"
ASSETS = ROOT / "native/crates/ha-game/assets"
MANIFEST = ASSETS / "art/manifest.json"
PAGES_DIR = ASSETS / "art/pack"
INDEX = ROOT / "scripts/animados-import-index.json"
SLOTS_RS = ROOT / "native/crates/ha-assets/src/slots.rs"
PLACEABLES_RS = ROOT / "native/crates/ha-shared/src/placeables.rs"

CELL = 32
PAGE = 1024

#: De dónde salen las hojas. `(carpeta, prefijo de slot)`.
FUENTES = [
    (PACK / "modernexteriors-win/Modern_Exteriors_32x32/Animated_32x32/Animated_sheets_32x32", "anim"),
    (PACK / "modernexteriors-win/Modern_Exteriors_32x32/Animated_32x32/Animated_Terrains_32x32", "anim"),
    (PACK / "modernexteriors-win/Modern_Exteriors_32x32/Animated_32x32/Vehicles_32x32", "anim"),
    (PACK / "moderninteriors-win (Copy 1)/3_Animated_objects/32x32", "anim"),
]

#: Fotogramas que se guardan de cada hoja. Un ciclo se lee con ocho; de ahí para
#: arriba es peso de textura que nadie mira. Ver el docstring del módulo.
MAX_FRAMES = 8

#: Lado máximo de un FOTOGRAMA, en celdas. Por encima de esto la "hoja animada"
#: no es un objeto sino una escena entera del pack (hay una de 798 fotogramas),
#: y un colocable de más de ocho metros no lo pide nadie.
TOPE_CELDAS = 8

#: Fotogramas por segundo de un bucle del pack. LimeZu anima a este ritmo y es el
#: mismo que ya usan las cascadas y el agua (`import-agua.py`).
FPS = 8.0


def es_puerta(nombre: str) -> bool:
    """¿Es una hoja de PUERTA? Ésas las dibuja `building::DoorArt` con sus poses
    `open`/`closed`, no un colocable suelto — ver el docstring del módulo."""
    return "door" in nombre.lower()


# ---------------------------------------------------------------------------
# El período: cuántos fotogramas tiene la hoja
# ---------------------------------------------------------------------------


def divisores(n: int):
    return [d for d in range(1, n + 1) if n % d == 0]


def periodo(im: Image.Image) -> int:
    """**El ancho de UN fotograma, en celdas.**

    Se prueba cada ancho que divida a la hoja y se mide cuánto se parecen los
    fotogramas que salen. La parte QUIETA de una animación —el marco, la torre,
    el pilón— es idéntica en todos los fotogramas, así que con el ancho correcto
    la diferencia media es chica; con un ancho equivocado la parte quieta cae
    desalineada y la diferencia se dispara.

    Se queda el ancho **más chico** que esté cerca del mejor puntaje: sin eso
    ganaría siempre la hoja entera (un solo "fotograma" se parece a sí mismo).
    """
    cols = im.width // CELL
    if cols <= 1:
        return cols
    px = im.convert("RGBA")
    mejor = None
    puntajes = {}
    for w in divisores(cols):
        if w == cols:
            continue
        n = cols // w
        base = px.crop((0, 0, w * CELL, px.height)).tobytes()
        total = 0
        for k in range(1, n):
            otro = px.crop((k * w * CELL, 0, (k + 1) * w * CELL, px.height)).tobytes()
            total += sum(a != b for a, b in zip(base, otro))
        d = total / max(1, (n - 1) * len(base))
        puntajes[w] = d
        if mejor is None or d < mejor:
            mejor = d
    if not puntajes:
        return cols
    # El más chico que esté dentro del 25 % del mejor. El margen existe porque la
    # parte que SE MUEVE también cuenta en la diferencia: el ancho correcto no da
    # cero, da "poco".
    umbral = max(mejor * 1.25, mejor + 0.02)
    for w in sorted(puntajes):
        if puntajes[w] <= umbral:
            return w
    return cols


# ---------------------------------------------------------------------------
# Nombres
# ---------------------------------------------------------------------------


def limpio(s: str) -> str:
    s = unicodedata.normalize("NFKD", s).encode("ascii", "ignore").decode()
    s = re.sub(r"_?32x32.*$", "", s)
    s = re.sub(r"[^A-Za-z0-9]+", "_", s).strip("_").lower()
    return re.sub(r"_+", "_", s)


def etiqueta(clave: str) -> str:
    """Nombre HUMANO para el editor: "Fuente 1", no `anim/fountain_1`.

    Regla del proyecto (`nombres-humanos-en-editores`): la lista se lee en
    castellano y la clave va chiquita al lado.
    """
    base = clave.split("/", 1)[1]
    for en, es in TRADUCE:
        base = re.sub(rf"\b{en}\b", es, base)
    return base.replace("_", " ").strip().capitalize()


#: Lo justo para que la lista del editor se lea en castellano. Lo que no esté
#: acá pasa tal cual: es preferible "Carousel" a inventar una traducción rara.
TRADUCE = [
    ("fountain", "fuente"), ("water", "agua"), ("fire", "fuego"),
    ("campfire", "fogata"), ("light", "luz"), ("lights", "luces"),
    ("flag", "bandera"), ("wind_mill", "molino"), ("windmill", "molino"),
    ("seagull", "gaviota"), ("barrier", "barrera"), ("floating_rock", "roca_flotante"),
    ("moss", "musgo"), ("tree", "arbol"), ("machine", "maquina"),
    ("fog", "niebla"), ("smoke", "humo"), ("clock", "reloj"),
    ("washing", "lavarropas"), ("carousel", "calesita"), ("chimney", "chimenea"),
    ("curtain", "cortina"), ("fan", "ventilador"), ("cat", "gato"), ("dog", "perro"),
    ("bird", "pajaro"), ("fish", "pez"), ("car", "auto"), ("bus", "colectivo"),
    ("truck", "camion"), ("boat", "bote"), ("train", "tren"), ("bike", "bici"),
]


# ---------------------------------------------------------------------------
# Empaquetado
# ---------------------------------------------------------------------------


class Pagina:
    """Atlas de `PAGE` px de lado con estantes horizontales.

    Cada tira entra ENTERA y contigua, que es lo que pide `strip` del manifest:
    los fotogramas siguientes son bloques pegados al primero.
    """

    def __init__(self):
        self.im = Image.new("RGBA", (PAGE, PAGE), (0, 0, 0, 0))
        self.x = 0
        self.y = 0
        self.alto_fila = 0

    def cabe(self, w, h):
        if w > PAGE or h > PAGE:
            return False
        if self.x + w <= PAGE and self.y + max(self.alto_fila, h) <= PAGE:
            return True
        return self.y + self.alto_fila + h <= PAGE and w <= PAGE

    def poner(self, tira):
        w, h = tira.size
        if self.x + w > PAGE:
            self.y += self.alto_fila
            self.x = 0
            self.alto_fila = 0
        if self.y + h > PAGE:
            return None
        self.im.paste(tira, (self.x, self.y))
        pos = (self.x // CELL, self.y // CELL)
        self.x += w
        self.alto_fila = max(self.alto_fila, h)
        return pos


def tira_de(im, ancho_celdas, frames, vertical):
    """Los `frames` primeros fotogramas, pegados uno tras otro."""
    fw, fh = ancho_celdas * CELL, im.height
    if vertical:
        out = Image.new("RGBA", (fw, fh * frames), (0, 0, 0, 0))
    else:
        out = Image.new("RGBA", (fw * frames, fh), (0, 0, 0, 0))
    for k in range(frames):
        f = im.crop((k * fw, 0, (k + 1) * fw, fh))
        out.paste(f, (0, k * fh) if vertical else (k * fw, 0))
    return out


# ---------------------------------------------------------------------------


def claves_tomadas():
    """Las claves de slot que ya tienen dueño — **menos las nuestras**.

    Un importador que no mira esto le pisa el arte a otro; uno que no se
    descuenta a sí mismo no es idempotente y se rebautiza en cada corrida
    (`import-pack-masivo-es-idempotente`). Las de la corrida anterior están
    anotadas en `animados-import-index.json` justamente para poder soltarlas.
    """
    usadas = set()
    if MANIFEST.exists():
        usadas |= set(json.loads(MANIFEST.read_text()).get("sprites", {}).keys())
    if INDEX.exists():
        usadas -= set(json.loads(INDEX.read_text()).get("sprites", []))
    return usadas


def hojas():
    """Las hojas del pack que pasan el filtro, ya medidas y sin repetir."""
    vistos = set()
    tomadas = claves_tomadas()
    out = []
    for carpeta, pref in FUENTES:
        if not carpeta.exists():
            print(f"  (falta {carpeta})")
            continue
        for ruta in sorted(carpeta.rglob("*.png")):
            nombre = limpio(ruta.stem)
            if not nombre or es_puerta(ruta.stem):
                continue
            try:
                im = Image.open(ruta).convert("RGBA")
            except Exception as e:  # una hoja rota no puede voltear la corrida
                print(f"  ! {ruta.name}: {e}")
                continue
            if im.width % CELL or im.height % CELL:
                continue
            h_celdas = im.height // CELL
            if h_celdas > TOPE_CELDAS:
                continue
            w = periodo(im)
            if w == 0 or w > TOPE_CELDAS:
                continue
            n = im.width // CELL // w
            if n < 2:
                continue
            firma = hashlib.blake2b(im.tobytes(), digest_size=16).hexdigest()
            if firma in vistos:
                continue
            vistos.add(firma)
            # **Una clave, un dueño.** Dos hojas de temas distintos se limpian al
            # mismo nombre (`Beach_..._Water_1` y `Pool_..._Water_1` → `water_1`)
            # y sin esto la segunda pisaba a la primera en silencio: la primera
            # corrida perdió 9 hojas así, y el contador de "sin sitio" decía 0
            # porque el diccionario ya las había tapado.
            clave = f"{pref}/{nombre}"
            if clave in tomadas:
                for i in range(2, 100):
                    if f"{clave}_{i}" not in tomadas:
                        clave = f"{clave}_{i}"
                        break
            tomadas.add(clave)
            out.append((clave, im, w, min(n, MAX_FRAMES)))
    return out


def reemplazar_bloque(ruta, lineas):
    """Escribe el bloque `ANIM:START/END` del archivo, creándolo si no está.

    Se cuelga siempre justo después del `PACK:END` del importador masivo, que ya
    está DENTRO de la tabla — así no hace falta saber cómo se llama la tabla ni
    dónde termina.
    """
    texto = ruta.read_text()
    ini_m = "// === ANIM:START ==="
    fin_m = "// === ANIM:END ==="
    cuerpo = f"{ini_m}\n" + "\n".join(lineas) + f"\n    {fin_m}"
    if ini_m in texto:
        i = texto.index(ini_m)
        j = texto.index(fin_m) + len(fin_m)
        texto = texto[:i] + cuerpo + texto[j:]
    else:
        ancla = "// === PACK:END ==="
        if ancla not in texto:
            raise SystemExit(f"{ruta.name} no trae PACK:END: no sé dónde colgar los animados")
        i = texto.index(ancla) + len(ancla)
        texto = texto[:i] + "\n    " + cuerpo + texto[i:]
    ruta.write_text(texto)


#: A qué cajón del menú de construir va cada cosa, por lo que dice su nombre. Sin
#: acertar en esto los 548 animados caen todos en el mismo cajón y el menú deja de
#: servir. Se mira en ORDEN: la primera marca que aparezca manda.
CATEGORIAS = [
    (("boat", "bote", "ship", "buoy", "fish", "surf", "wave"), "Agua", "Madera"),
    (("car", "bus", "truck", "train", "subway", "bike", "moto", "helicopter", "ambulance",
      "police", "taxi", "van", "tractor"), "Vehiculo", "Metal"),
    (("water", "fountain", "agua", "fuente"), "Agua", "Piedra"),
    (("tree", "arbol", "flower", "grass", "bush", "seagull", "bird", "cat", "dog",
      "rock", "roca", "leaf"), "Naturaleza", "Planta"),
    (("fire", "fuego", "camp", "lamp", "light", "luz", "sign", "traffic", "barrier",
      "clock", "flag"), "Calle", "Metal"),
    (("bed", "tv", "washing", "kitchen", "fridge", "fan", "curtain", "lava", "toilet",
      "shower", "clothes"), "Mueble", "Madera"),
]


def cajon(clave):
    n = clave.split("/", 1)[1]
    for marcas, cat, mat in CATEGORIAS:
        if any(m in n for m in marcas):
            return cat, mat
    return "Estructura", "Piedra"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    lote = hojas()
    print(f"hojas que entran: {len(lote)}")

    paginas = [Pagina()]
    sprites = {}
    objetos = {}
    fuera = 0
    for clave, im, w, frames in lote:
        # Acostada si cabe; si no, de pie. La dirección se declara en el manifest
        # y `art2d::strip_info` avanza los fotogramas por ahí.
        colocada = False
        for vertical in (False, True):
            n = frames
            while n >= 2:
                tira = tira_de(im, w, n, vertical)
                if max(tira.size) > PAGE:
                    n -= 1
                    continue
                pos = paginas[-1].poner(tira)
                if pos is None:
                    paginas.append(Pagina())
                    pos = paginas[-1].poner(tira)
                if pos is None:
                    break
                col, row = pos
                sprites[clave] = {
                    "png": f"art/pack/anim_{len(paginas) - 1}.png",
                    "grid": [CELL, CELL],
                    "strip": {"col": col, "row": row, "w": w, "h": im.height // CELL},
                    "strip_dir": "Down" if vertical else "Right",
                    # `anim` y no `frames`/`fps` sueltos: es el nombre del campo
                    # en `ha_assets::art_manifest::ArtEntry`, y serde ignora en
                    # silencio lo que no reconoce — una tira con los campos mal
                    # puestos entra como sprite quieto y nadie se entera.
                    "anim": {"frames": n, "fps": FPS},
                }
                objetos[clave] = {"label": etiqueta(clave)}
                colocada = True
                break
            if colocada:
                break
        if not colocada:
            fuera += 1

    print(f"colocadas: {len(sprites)} · sin sitio: {fuera} · páginas: {len(paginas)}")
    if args.dry_run:
        print("(dry-run: no se escribió nada)")
        return

    PAGES_DIR.mkdir(parents=True, exist_ok=True)
    for i, p in enumerate(paginas):
        p.im.save(PAGES_DIR / f"anim_{i}.png", optimize=True)

    man = json.loads(MANIFEST.read_text())
    hoja = man.setdefault("sprites", {})
    objs = man.setdefault("objects", {})
    if INDEX.exists():
        for k in json.loads(INDEX.read_text()).get("sprites", []):
            hoja.pop(k, None)
            objs.pop(k, None)
    hoja.update(sprites)
    objs.update(objetos)
    MANIFEST.write_text(json.dumps(man, indent=1, ensure_ascii=False, sort_keys=True))

    reemplazar_bloque(
        SLOTS_RS,
        [f'    ("{k}", "{objetos[k]["label"]}"),' for k in sorted(objetos)],
    )
    # El colocable: `id` sin barras (es la clave del inventario), y el tamaño en
    # tiles sale del PROPIO recorte — regla `El asset se usa como está dibujado`.
    lineas = []
    for k in sorted(objetos):
        cat, mat = cajon(k)
        s = sprites[k]["strip"]
        lineas.append(
            f'    p("{k.replace("/", "_")}", "{objetos[k]["label"]}", "{k}", '
            f"PlaceCat::{cat}, Material::{mat}, {s['w']}, {s['h']}),"
        )
    reemplazar_bloque(PLACEABLES_RS, lineas)
    INDEX.write_text(
        json.dumps({"sprites": sorted(sprites), "paginas": len(paginas)}, indent=1, ensure_ascii=False)
    )
    print(f"manifest: {len(sprites)} claves animadas · slots.rs actualizado")


if __name__ == "__main__":
    main()
