#!/usr/bin/env python3
"""**DATASET PARA UN LoRA DE PIXEL ART (no entrena nada: sólo prepara).**

    python3 scripts/armar-dataset-lora.py [--max 3000] [--salida ~/ia-arte/dataset]

Arma pares imagen+descripción con singles del pack para entrenar un LoRA en una GPU
alquilada (biblia §16, "Arte con IA"). Cada single 32x32 (o 32x64…) se amplía ×16 con
vecino más cercano sobre fondo liso → 512 px, el tamaño base de SD 1.5 / SDXL-pixel.
La descripción sale del NOMBRE del archivo (Exteriors) o de la carpeta del tema
(Interiors: `21_Clothing_Store_Singles` → "clothing store item"), más el estilo fijo
`pixel art, LimeZu style, 32x32 sprite, dark outline` (Interiors) o `no outline` (Exteriors),
para que el LoRA aprenda las dos familias por separado.

**LICENCIA (leída el 2 oct 2026, `LICENSE.txt` de Modern Interiors):** se puede editar y
usar el pack en proyectos comerciales o no, se pide crédito a LimeZu, y NO se puede
revender ni distribuir el pack ni versiones editadas. Entrenar un LoRA PRIVADO para hacer
arte de ESTE juego cabe; **el dataset y el LoRA no se publican ni se comparten** (por eso
la salida va fuera del repo, a `~/ia-arte/`). Los PNG generados por el modelo para el
juego se pasan por el corsé (`lib/pixel.py`) y el crítico (`critico-pixel.py`).
"""

import json
import random
import re
import sys
from pathlib import Path

from PIL import Image

RAIZ = Path(__file__).resolve().parent.parent
INT = RAIZ / "PRUEBAS-CODIGOS/moderninteriors-win (Copy 1)/1_Interiors/32x32/Theme_Sorter_Singles_32x32"
EXT = RAIZ / "PRUEBAS-CODIGOS/modernexteriors-win/Modern_Exteriors_32x32"
FONDO = (126, 160, 104)  # el pasto de las láminas: se ve el recorte sin ensuciarlo


def pedir(args, k, defecto):
    return args[args.index(k) + 1] if k in args else defecto


def texto_de_tema(carpeta: str) -> str:
    t = re.sub(r"^\d+_", "", carpeta)
    t = re.sub(r"_?(Black_Shadow_)?S[Ii]ngles.*$", "", t, flags=re.I)
    return t.replace("_", " ").replace("32x32", "").strip().lower() or "furniture"


def texto_de_archivo(nombre: str) -> str:
    n = re.sub(r"^(ME_Singles_|\d+_)", "", nombre)
    n = re.sub(r"_?32x32_?", " ", n)
    n = re.sub(r"\d+", " ", n)
    return " ".join(n.replace("_", " ").split()).lower()


def main():
    args = sys.argv[1:]
    maximo = int(pedir(args, "--max", "3000"))
    salida = Path(pedir(args, "--salida", str(Path.home() / "ia-arte/dataset"))).expanduser()
    salida.mkdir(parents=True, exist_ok=True)
    rng = random.Random(7)
    candidatos = []
    for p in INT.rglob("*.png"):
        candidatos.append((p, "int", texto_de_tema(p.parent.name)))
    for p in EXT.rglob("ME_Singles*.png"):
        candidatos.append((p, "ext", texto_de_archivo(p.stem)))
    rng.shuffle(candidatos)
    meta, n = [], 0
    for p, fam, desc in candidatos:
        if n >= maximo:
            break
        try:
            im = Image.open(p).convert("RGBA")
        except Exception:
            continue
        if not (16 <= im.width <= 64 and 16 <= im.height <= 64) or sum(1 for a in im.getchannel("A").getdata() if a > 128) < 80:
            continue
        lienzo = Image.new("RGBA", (32, 32), (*FONDO, 255))
        lienzo.alpha_composite(im, ((32 - im.width) // 2, 32 - im.height))
        grande = lienzo.convert("RGB").resize((512, 512), Image.NEAREST)
        nombre = f"{n:05d}.png"
        grande.save(salida / nombre)
        estilo = "dark outline" if fam == "int" else "no outline"
        meta.append({"file_name": nombre, "text": f"pixel art, LimeZu style, 32x32 sprite, {estilo}, {desc}"})
        n += 1
    with open(salida / "metadata.jsonl", "w", encoding="utf8") as f:
        for m in meta:
            f.write(json.dumps(m, ensure_ascii=False) + "\n")
    print(f"{n} pares en {salida}  (privado: no publicar, ver licencia)")


if __name__ == "__main__":
    main()
