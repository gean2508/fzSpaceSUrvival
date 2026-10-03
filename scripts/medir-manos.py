#!/usr/bin/env python3
"""**LAS MANOS DEL PERSONAJE, MEDIDAS: dónde agarra cada cuadro y con qué parte se agarra cada objeto.**

    python3 scripts/medir-manos.py            # escribe art/chargen/manos.json y empalma walk_gun en la hoja por defecto
    python3 scripts/medir-manos.py --vista    # además deja la lámina de control en /tmp/claude-1000/manos.png

## Por qué existe

El ítem equipado flotaba al costado del cuerpo a un offset fijo y el arma a 0,62 m del pecho: nadie lo agarraba
(pedido del 3 oct 2026: «se ve horrible que el jugador dispare y su mano no se vea agarrando el arma»). Para pegar
un objeto a la mano hay que saber DÓNDE está la mano en cada cuadro de cada animación, y eso se mide, no se escribe.

## Cómo se mide la mano

Las capas del Character Generator de LimeZu comparten esqueleto: el cuerpo (`cuerpo/*.png`) dibuja la piel entera y
la ropa (`ropa/*.png`, 132 conjuntos) la tapa. **Una mano es piel que casi ninguna ropa tapa**: se cuenta, píxel por
píxel, qué fracción de los conjuntos lo cubre; lo que cubre menos del 15 % y está por debajo del mentón (y ≥ 49) es
mano. Así salen solas las dos manos de cada cuadro, en las 9 pieles (y se exige que coincidan entre ellas).

De las dos manos se elige la que agarra: la derecha del personaje (a la izquierda de la pantalla mirando abajo, a la
derecha mirando arriba) y, de perfil, la de adelante. Mirando arriba las manos quedan detrás del cuerpo: el objeto va
DETRÁS (`detras`).

## Caminar con el arma (`walk_gun`)

El pack trae «apuntar» (`gun_idle`, fila 17) y «disparar» (`shoot`, 18) con las dos manos en el arma, pero no
«caminar apuntando». Se arma con el mismo arte: torso de `gun_idle` (y < 56) sobre las piernas de `walk` (y ≥ 56),
en las celdas libres de la fila 17 (columnas 24..47, vacías en las 432 capas: se verifica). El juego lo empalma al
componer la hoja del jugador (`chargen.rs`); este script hace lo mismo con la hoja por defecto
(`art/player/character.png`).

## El agarre de cada objeto

* **Armas** (`items/gun_*`): del propio diseño (`lib/armas_diseno.py`): la empuñadura colgante de madera/polímero
  más atrasada; si el arma no tiene (escopeta), la muñeca de la culata, detrás del cajón de mecanismos.
* **Largos** (hacha, pico, espada, antorcha…): el eje principal del dibujo es largo (> 2,2 veces su ancho): se
  agarran a un 18 % del extremo de abajo-izquierda (el mango).
* **Chicos** (comida, piedras, frascos, baldes…): desde abajo, al centro.
* **Colgantes** (el farol): de la manija, arriba al centro.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw
from scipy import ndimage

RAIZ = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib.armas import OX, OY  # noqa: E402
from lib.armas_diseno import ARMAS  # noqa: E402

ASSETS = RAIZ / "native/crates/ha-game/assets"
INDICE = ASSETS / "art/chargen/index.json"
SALIDA = ASSETS / "art/chargen/manos.json"
HOJA_DEFECTO = ASSETS / "art/player/character.png"
MANIFEST = ASSETS / "art/manifest.json"

CW, CH = 32, 64
# Animaciones donde se sostiene algo: (fila, cuadros por dirección). Las cuatro direcciones van R, U, L, D.
ANIMS = {
    "still": (0, 1), "idle": (1, 6), "walk": (2, 6), "hit": (13, 6), "punch": (14, 6), "stab": (15, 12),
    "grab_gun": (16, 4), "gun_idle": (17, 6), "shoot": (18, 3),
}
# Filas de la lámina de manos (`art/chargen/manos/<piel>.png`): una por animación, en este orden.
FILAS_MANOS = list(ANIMS)
ARMADAS = {"gun_idle", "shoot"}
WALK_GUN_FILA, WALK_GUN_COL0, CORTE = 17, 24, 56
UMBRAL_ROPA = 0.15
MENTON = 49


def rgba(p: Path) -> np.ndarray:
    return np.asarray(Image.open(p).convert("RGBA"))


def cobertura_de_ropa(idx) -> np.ndarray:
    filas = 20 * CH
    cnt = np.zeros((filas, 56 * CW), np.int32)
    for o in idx["outfits"]:
        cnt += rgba(ASSETS / o["png"])[:filas, :, 3] > 0
    return cnt / len(idx["outfits"])


def manos_de_celda(cuerpo, frac, fila, col):
    y0, x0 = fila * CH, col * CW
    piel = cuerpo[y0:y0 + CH, x0:x0 + CW, 3] > 0
    m = piel & (frac[y0:y0 + CH, x0:x0 + CW] < UMBRAL_ROPA)
    m[:MENTON] = False
    lab, n = ndimage.label(m, structure=np.ones((3, 3)))
    comps = []
    for i in range(1, n + 1):
        ys, xs = np.nonzero(lab == i)
        if len(xs) < 3:
            continue
        comps.append((float(xs.mean()), float(ys.mean()), int(xs.min()), int(ys.min()), int(xs.max()), int(ys.max())))
    return comps


def elegir(comps, d, armada):
    """El agarre de una celda: dónde cae la mano que sostiene el objeto."""
    if not comps:
        return None
    if armada and d in (1, 3):
        # De frente o de espaldas las dos manos van juntas sobre el arma: el agarre es el punto medio.
        gx = sum(k[0] for k in comps) / len(comps)
        gy = sum(k[1] for k in comps) / len(comps)
        return {"ancla": [round(gx), round(gy)]}
    if armada:
        # De perfil, el arma se empuña con la mano de ATRÁS (la del gatillo): la más cercana al cuerpo.
        c = max(comps, key=lambda c: c[0]) if d == 2 else min(comps, key=lambda c: c[0])
        return {"ancla": [round(c[0]), round(c[1])]}
    # R y U: la de más a la derecha en pantalla; L y D: la de más a la izquierda.
    c = min(comps, key=(lambda c: -c[0]) if d in (0, 1) else (lambda c: c[0]))
    return {"ancla": [round(c[0]), round(c[1])]}


def lamina_de_manos(cuerpo, frac):
    """Sólo los píxeles de las manos (más su contorno), de UNA piel, en las filas de `FILAS_MANOS`."""
    out = np.zeros((len(FILAS_MANOS) * CH, 56 * CW, 4), np.uint8)
    for i, (anim, (fila, n)) in enumerate(ANIMS.items()):
        for d in range(4):
            for f in range(n):
                col = d * n + f
                y0, x0 = fila * CH, col * CW
                alfa = cuerpo[y0:y0 + CH, x0:x0 + CW, 3] > 0
                libre = alfa & (frac[y0:y0 + CH, x0:x0 + CW] < UMBRAL_ROPA)
                libre[:MENTON] = False
                lab, k = ndimage.label(libre, structure=np.ones((3, 3)))
                m = np.zeros_like(libre)
                for j in range(1, k + 1):
                    if (lab == j).sum() >= 3:
                        m |= lab == j
                # El contorno oscuro de la mano también es del cuerpo y casi nunca lo tapa la ropa.
                aro = ndimage.binary_dilation(m, structure=np.ones((3, 3))) & alfa & (frac[y0:y0 + CH, x0:x0 + CW] < 0.5)
                aro[:MENTON - 1] = False
                m |= aro
                celda = cuerpo[y0:y0 + CH, x0:x0 + CW].copy()
                celda[~m] = 0
                out[i * CH:(i + 1) * CH, x0:x0 + CW] = celda
    return out


def medir_personaje(idx):
    frac = cobertura_de_ropa(idx)
    cuerpos = [rgba(ASSETS / b["png"]) for b in idx["bodies"]]
    manos = {}
    peor = 0
    for anim, (fila, n) in ANIMS.items():
        for d in range(4):
            for f in range(n):
                col = d * n + f
                armada = anim in ARMADAS or anim == "grab_gun"
                por_piel = [elegir(manos_de_celda(c, frac, fila, col), d, armada) for c in cuerpos]
                ref = por_piel[0]
                for otra in por_piel[1:]:
                    if (ref is None) != (otra is None):
                        peor = max(peor, 99)
                    elif ref:
                        peor = max(peor, abs(ref["ancla"][0] - otra["ancla"][0]), abs(ref["ancla"][1] - otra["ancla"][1]))
                if ref is None:
                    # Mirando arriba las manos quedan tapadas por la espalda: el objeto va DETRÁS, a la altura
                    # de las manos de los otros perfiles.
                    ref = {"ancla": [CW // 2 + (5 if d == 1 else 0), 51]}
                ref["detras"] = d == 1
                manos[f"{anim}:{col}"] = ref
    if peor > 1:
        sys.exit(f"medir-manos: las pieles no coinciden en la mano (hasta {peor} px): revisar el pack")
    laminas = [lamina_de_manos(c, frac) for c in cuerpos]
    return manos, laminas


def verificar_celdas_libres(idx):
    capas = [b["png"] for b in idx["bodies"]]
    for k in ("eyes", "outfits", "hair", "accessories"):
        capas += [o["png"] for o in idx[k]]
    y0, x0 = WALK_GUN_FILA * CH, WALK_GUN_COL0 * CW
    for p in capas:
        a = rgba(ASSETS / p)[y0:y0 + CH, x0:x0 + 24 * CW, 3]
        if a.any():
            sys.exit(f"medir-manos: {p} dibuja en las celdas de walk_gun (fila 17, col 24..47): el pack cambió")
    return len(capas)


def empalmar_walk_gun(hoja: Image.Image) -> Image.Image:
    """Mismo empalme que `chargen::empalmar_walk_gun`: torso de gun_idle sobre piernas de walk."""
    out = hoja.copy()
    n = ANIMS["gun_idle"][1]
    for d in range(4):
        for f in range(n):
            col = d * n + f
            arma = hoja.crop((col * CW, 17 * CH, col * CW + CW, 17 * CH + CORTE))
            piernas = hoja.crop((col * CW, 2 * CH + CORTE, col * CW + CW, 3 * CH))
            dx = (WALK_GUN_COL0 + col) * CW
            celda = Image.new("RGBA", (CW, CH), (0, 0, 0, 0))
            celda.paste(arma, (0, 0))
            celda.paste(piernas, (0, CORTE))
            out.paste(celda, (dx, WALK_GUN_FILA * CH))
    return out


# --- El agarre de los objetos ---------------------------------------------------------------------------------------


def recorte(m, clave):
    s = m[clave]
    g = s.get("grid", [32, 32])
    r = s.get("rect") or s.get("strip")
    x, y = r["col"] * g[0], r["row"] * g[1]
    w, h = r.get("w", 1) * g[0], r.get("h", 1) * g[1]
    im = Image.open(ASSETS / s["png"]).convert("RGBA").crop((x, y, x + w, y + h))
    if "anim" in s and "strip" in s:
        # Una tira: el agarre se mide en el primer cuadro.
        frames = s["anim"].get("frames", 1)
        if s.get("strip_dir", "Right") == "Right":
            im = im.crop((0, 0, w // frames if r.get("w", 1) % frames == 0 else w, h))
    return im


def agarre_de_arma(aid):
    _nombre, _cal, piezas = ARMAS[aid]
    y_icono = lambda y: OY + 12 + (y - 12) * 1.14  # noqa: E731 — el estirón vertical de `lib.armas._mask`
    colgantes = []
    for p in piezas:
        if p.mat != "F":
            continue
        xs = [q[0] for q in p.puntos]
        ys = [q[1] for q in p.puntos]
        if min(ys) >= 10 and max(ys) - min(ys) >= 7:
            colgantes.append((min(xs), xs, ys))
    if colgantes:
        _x, xs, ys = min(colgantes, key=lambda c: c[0])
        gx, gy = (min(xs) + max(xs)) / 2, min(ys) + 3
    else:
        cajones = [p for p in piezas if p.mat == "B"]
        p = min(cajones, key=lambda p: min(q[0] for q in p.puntos))
        gx = min(q[0] for q in p.puntos) + 3
        gy = max(q[1] for q in p.puntos) + 2
    return [round(OX + gx), round(y_icono(gy))]


def agarre_de_objeto(im: Image.Image, clave: str):
    a = np.asarray(im)[..., 3] > 16
    ys, xs = np.nonzero(a)
    if len(xs) == 0:
        return None
    x0, x1, y0, y1 = int(xs.min()), int(xs.max()), int(ys.min()), int(ys.max())
    if clave == "items/lantern":
        # El farol trae su halo pintado (alfa bajo): la manija es la del cuerpo opaco.
        ys2, xs2 = np.nonzero(np.asarray(im)[..., 3] > 200)
        return {"forma": "colgante", "agarre": [round((int(xs2.min()) + int(xs2.max())) / 2), int(ys2.min()) + 1]}
    pts = np.stack([xs, ys], 1).astype(np.float32)
    c = pts.mean(0)
    cov = np.cov((pts - c).T)
    val, vec = np.linalg.eigh(cov)
    if val[1] > 0 and np.sqrt(val[1] / max(val[0], 1e-3)) > 2.2:
        eje = vec[:, 1]
        t = (pts - c) @ eje
        # El extremo «mango» es el que queda más abajo-izquierda.
        if (-eje[0] + eje[1]) < 0:
            eje, t = -eje, -t
        tmin, tmax = float(t.min()), float(t.max())
        objetivo = c + eje * (tmax - 0.18 * (tmax - tmin))
        i = int(np.argmin(((pts - objetivo) ** 2).sum(1)))
        return {"forma": "largo", "agarre": [int(pts[i, 0]), int(pts[i, 1])]}
    alto = y1 - y0 + 1
    return {"forma": "chico", "agarre": [round((x0 + x1) / 2), y1 - max(2, round(0.2 * alto))]}


def medir_objetos():
    m = json.loads(MANIFEST.read_text(encoding="utf8"))["sprites"]
    objetos = {}
    for clave in sorted(m):
        if not clave.startswith("items/") or "__" in clave or clave == "items/hand":
            continue
        base = clave[len("items/"):]
        if base.startswith("gun_") and base[4:] in ARMAS:
            im = recorte(m, clave)
            objetos[clave] = {"forma": "arma", "agarre": agarre_de_arma(base[4:]), "tam": list(im.size)}
            continue
        try:
            im = recorte(m, clave)
        except (KeyError, FileNotFoundError) as e:
            print(f"  (sin recorte: {clave}: {e})")
            continue
        g = agarre_de_objeto(im, clave)
        if g:
            g["tam"] = list(im.size)
            objetos[clave] = g
    return objetos


def vista(manos, objetos):
    idx = json.loads(INDICE.read_text(encoding="utf8"))
    look = idx["npc_looks"][1]
    hoja = Image.new("RGBA", (56 * CW, 20 * CH), (110, 140, 110, 255))
    for k, kk in [("body", "bodies"), ("eyes", "eyes"), ("outfit", "outfits"), ("hair", "hair"), ("accessory", "accessories")]:
        if look.get(k) is not None:
            hoja.alpha_composite(Image.open(ASSETS / idx[kk][look[k]]["png"]).convert("RGBA"))
    hoja = empalmar_walk_gun(hoja)
    filas = [(1, 0, 24), (2, 0, 24), (17, 0, 24), (17, 24, 48), (18, 0, 12)]
    lam = Image.new("RGBA", (24 * 34, len(filas) * 66), (0, 0, 0, 255))
    for i, (fila, c0, c1) in enumerate(filas):
        for j, col in enumerate(range(c0, c1)):
            t = hoja.crop((col * CW, fila * CH, col * CW + CW, fila * CH + CH))
            nombre = next((a for a, (fi, _) in ANIMS.items() if fi == fila), None)
            cc = col - WALK_GUN_COL0 if col >= WALK_GUN_COL0 else col
            mm = manos.get(f"{'gun_idle' if col >= WALK_GUN_COL0 else nombre}:{cc}")
            if mm:
                d = ImageDraw.Draw(t)
                d.ellipse((mm["ancla"][0] - 1, mm["ancla"][1] - 1, mm["ancla"][0] + 1, mm["ancla"][1] + 1), outline=(255, 0, 255, 255))
            lam.paste(t, (j * 34, i * 66))
    Path("/tmp/claude-1000").mkdir(exist_ok=True)
    lam.resize((lam.width * 2, lam.height * 2), Image.NEAREST).save("/tmp/claude-1000/manos.png")
    m = json.loads(MANIFEST.read_text(encoding="utf8"))["sprites"]
    claves = list(objetos)
    lo = Image.new("RGBA", (12 * 72, ((len(claves) + 11) // 12) * 72), (110, 140, 110, 255))
    for i, k in enumerate(claves):
        im = recorte(m, k)
        im = im.resize((64, 64), Image.NEAREST) if im.size[0] <= 64 else im.resize((64, 64))
        esc = 64 / objetos[k]["tam"][0]
        d = ImageDraw.Draw(im)
        gx, gy = objetos[k]["agarre"]
        d.ellipse((gx * esc - 2, gy * esc - 2, gx * esc + 2, gy * esc + 2), fill=(255, 0, 255, 255))
        lo.alpha_composite(im, ((i % 12) * 72, (i // 12) * 72))
    lo.save("/tmp/claude-1000/agarres.png")
    print("  vista: /tmp/claude-1000/manos.png y /tmp/claude-1000/agarres.png")


def main():
    idx = json.loads(INDICE.read_text(encoding="utf8"))
    n = verificar_celdas_libres(idx)
    print(f"  celdas de walk_gun libres en las {n} capas")
    manos, laminas = medir_personaje(idx)
    destino = ASSETS / "art/chargen/manos"
    destino.mkdir(exist_ok=True)
    for i, lam in enumerate(laminas):
        Image.fromarray(lam, "RGBA").save(destino / f"{i:03d}.png", optimize=True)
    objetos = medir_objetos()
    salida = {
        "celda": [CW, CH],
        "walk_gun": {"fila": WALK_GUN_FILA, "col0": WALK_GUN_COL0, "corte": CORTE},
        "filas": {a: i for i, a in enumerate(FILAS_MANOS)},
        "manos": manos,
        "objetos": objetos,
    }
    SALIDA.write_text(json.dumps(salida, ensure_ascii=False, separators=(",", ":"), sort_keys=True) + "\n", encoding="utf8")
    print(f"  {len(manos)} cuadros con mano y {len(objetos)} objetos con agarre → {SALIDA.relative_to(RAIZ)}")
    # La hoja por defecto (partidas sin `look`) también camina con el arma.
    hoja = Image.open(HOJA_DEFECTO).convert("RGBA")
    libre = not np.asarray(hoja)[WALK_GUN_FILA * CH:(WALK_GUN_FILA + 1) * CH, WALK_GUN_COL0 * CW:(WALK_GUN_COL0 + 24) * CW, 3].any()
    nueva = empalmar_walk_gun(hoja)
    if libre or np.asarray(nueva).tobytes() != np.asarray(hoja).tobytes():
        nueva.save(HOJA_DEFECTO, optimize=True)
        print(f"  walk_gun empalmado en {HOJA_DEFECTO.relative_to(RAIZ)}")
    if "--vista" in sys.argv:
        vista(manos, objetos)


if __name__ == "__main__":
    main()
