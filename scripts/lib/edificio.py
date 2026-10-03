#!/usr/bin/env python3
"""**EDIFICIOS A LA MEDIDA DE LA SIM, CON EL PIXEL DEL PACK.**

Por qué existe (3 oct 2026). Las casas «rotas» —el interior que asoma por el costado, la puerta dibujada que
no cae en la celda `Door`, la fachada estirada ×1,4— salen todas de lo mismo: **el pack dibuja cada edificio de un
tamaño fijo y la sim le pide otra huella** (biblia §5, «El lote CRECE», «Interior que asoma»). Estirar un sprite
lo deforma y no estirarlo deja huecos. Dibujar la casa entera a mano tampoco sirve: el juez
(`lib/juez.py`) la deja POR DEBAJO de las casas del pack, que tienen tablas, tejas, alféizares y caños.

La salida es la tercera: **re-componer el pixel del pack a la medida exacta**, como hace un pixel-artista cuando
le piden la misma casa una tabla más ancha.

* **Columnas gemelas.** Medido sobre `Terraced_House_Side_View_Base_1`: 160 de sus 320 columnas son idénticas a
  la de al lado (una tabla del revestimiento mide más de 1 px), y el techo `Side_View_Roof_*` es de filas que se
  repiten. Repetir una de esas líneas agranda una tabla; quitarla la achica: **no se ve**, y no se inventa un solo
  color. `redimensionar` reparte el cambio entre las tiras de líneas iguales en proporción a su largo, así las
  tablas anchas absorben y los filos de 2 px de un marco casi no se tocan.
* **Franjas protegidas.** La puerta y las ventanas no se tocan: `proteger_x=aberturas(fachada)` las detecta por
  el vidrio. Achicar tiene piso (`ancho_minimo`); agrandar, no.
* **La puerta va donde dice la sim.** `mover_franja` saca la franja de la puerta y re-compone lo de los dos
  lados al ancho que haga falta para que caiga en la columna de `city::puerta_del_frente`.

Todo lo que sale de acá son píxeles del pack: ni colores nuevos, ni anti-alias, ni escala fraccionaria. Por eso
**no pasa por `Lienzo.verificar`** (que es para lo dibujado): pasa por `juez.juzgar` contra la pieza original,
que tiene que dar `A LA ALTURA` o `SUPERA`.
"""

from __future__ import annotations

import numpy as np
from PIL import Image

CELDA = 32


def _arr(im) -> np.ndarray:
    if isinstance(im, Image.Image):
        return np.asarray(im.convert("RGBA")).copy()
    return np.asarray(im).copy()


def gemelas(a: np.ndarray, eje: int = 1) -> np.ndarray:
    """Índices i tales que la línea i es idéntica a la i+1 (columnas con eje=1, filas con eje=0)."""
    if eje == 1:
        return np.nonzero((a[:, :-1] == a[:, 1:]).all(axis=(0, 2)))[0]
    return np.nonzero((a[:-1] == a[1:]).all(axis=(1, 2)))[0]


def _tiras(a: np.ndarray, eje: int, prohibido: np.ndarray) -> list[tuple[int, int]]:
    """Tiras [inicio, largo) de líneas idénticas consecutivas que no tocan lo protegido."""
    gem = np.zeros(a.shape[eje], bool)
    gem[gemelas(a, eje)] = True
    tiras, i, n = [], 0, a.shape[eje]
    while i < n:
        j = i
        while j < n - 1 and gem[j]:
            j += 1
        largo = j - i + 1
        if largo >= 2 and not prohibido[i : j + 1].any():
            tiras.append((i, largo))
        i = j + 1
    return tiras


def _cambiar(a: np.ndarray, objetivo: int, eje: int, proteger) -> np.ndarray:
    """Lleva el eje a `objetivo` repartiendo el cambio entre las tiras de líneas iguales, **en proporción a su
    largo** (al cuadrado): una tabla de 9 px absorbe más que el filo de 2 px de un marco, que es lo que mira el ojo.

    (La primera versión duplicaba gemelas de a una por pasada y, al crecer mucho, volvía a elegir las mismas: el
    marco de 2 px de un panel terminaba de 6 y se leía como tres pilares. Lo cazó la lámina, no un número.)"""
    largo = a.shape[eje]
    delta = objetivo - largo
    if delta == 0:
        return a
    prohibido = np.zeros(largo, bool)
    for x0, x1 in proteger:
        prohibido[max(0, x0) : min(largo, x1)] = True
    tiras = _tiras(a, eje, prohibido)
    if not tiras:
        raise SystemExit("edificio: la pieza no tiene líneas gemelas fuera de lo protegido — no se puede re-medir sin inventar pixel")
    total = sum(l for _, l in tiras)
    if delta < 0 and -delta > total - len(tiras):
        raise SystemExit(f"edificio: no entra en {objetivo} px; lo mínimo sin tocar aberturas es {largo - (total - len(tiras))}")
    # reparto por mayor resto, pesado por largo²: un caño de 3 px casi no crece y un paño de 12 absorbe (con
    # peso lineal el caño de bajada engordaba a la vista en una fachada de 16 celdas). Tope: ninguna tira baja de 1.
    peso = [l * l for _, l in tiras]
    cuota = [abs(delta) * w / sum(peso) for w in peso]
    n = [int(c) for c in cuota]
    if delta < 0:
        n = [min(k, l - 1) for k, (_, l) in zip(n, tiras)]
    resto = abs(delta) - sum(n)
    orden = sorted(range(len(tiras)), key=lambda k: -(cuota[k] - int(cuota[k])) - tiras[k][1] * 1e-6)
    k = 0
    while resto > 0:
        t = orden[k % len(orden)]
        if delta > 0 or n[t] < tiras[t][1] - 1:
            n[t] += 1
            resto -= 1
        k += 1
    trozos, cursor = [], 0
    for (ini, l), cambio in zip(tiras, n):
        trozos.append(np.take(a, range(cursor, ini), axis=eje))
        tira = np.take(a, range(ini, ini + l), axis=eje)
        nuevo = l + cambio if delta > 0 else l - cambio
        # la tira es de líneas iguales: re-medirla es repetir su primera línea
        trozos.append(np.repeat(np.take(tira, [0], axis=eje), nuevo, axis=eje))
        cursor = ini + l
    trozos.append(np.take(a, range(cursor, largo), axis=eje))
    return np.concatenate(trozos, axis=eje)


def redimensionar(im, ancho: int | None = None, alto: int | None = None, proteger_x=(), proteger_y=()) -> np.ndarray:
    """La pieza a `ancho` x `alto` px duplicando/quitando líneas gemelas. Lo protegido no se toca."""
    a = _arr(im)
    if ancho is not None and ancho != a.shape[1]:
        a = _cambiar(a, ancho, 1, proteger_x)
    if alto is not None and alto != a.shape[0]:
        a = _cambiar(a, alto, 0, proteger_y)
    return a


def mover_franja(im, x0: int, x1: int, destino: int, proteger=()) -> np.ndarray:
    """La franja vertical [x0, x1) termina empezando en `destino`, sin cambiar el ancho total.

    Se re-mide lo de la izquierda a `destino` px y lo de la derecha a lo que sobre. Es cómo la puerta dibujada
    se lleva a la celda de la puerta de la sim."""
    a = _arr(im)
    W = a.shape[1]
    izq, franja, der = a[:, :x0], a[:, x0:x1], a[:, x1:]
    p_izq = [(p0, p1) for p0, p1 in proteger if p1 <= x0]
    p_der = [(p0 - x1, p1 - x1) for p0, p1 in proteger if p0 >= x1]
    izq = redimensionar(izq, ancho=destino, proteger_x=p_izq) if destino != x0 else izq
    resto = W - destino - (x1 - x0)
    der = redimensionar(der, ancho=resto, proteger_x=p_der) if resto != der.shape[1] else der
    return np.concatenate([izq, franja, der], axis=1)


def apilar(arriba, abajo, solape: int = 0) -> np.ndarray:
    """Techo sobre fachada (mismo ancho). `solape` filas del techo caen sobre la fachada (el alero)."""
    t, f = _arr(arriba), _arr(abajo)
    if t.shape[1] != f.shape[1]:
        raise SystemExit(f"edificio: techo {t.shape[1]} px y fachada {f.shape[1]} px — re-medí antes de apilar")
    H = t.shape[0] + f.shape[0] - solape
    out = np.zeros((H, t.shape[1], 4), np.uint8)
    out[t.shape[0] - solape :] = f
    techo = Image.fromarray(out)
    techo.alpha_composite(Image.fromarray(t), (0, 0))
    return np.asarray(techo).copy()


def recortar_opaco(a: np.ndarray) -> np.ndarray:
    ys, xs = np.nonzero(a[..., 3] > 0)
    return a[ys.min() : ys.max() + 1, xs.min() : xs.max() + 1]


def a_celdas(a: np.ndarray) -> np.ndarray:
    """Rellena con transparente hasta múltiplos de 32 (abajo y a la derecha): la grilla del editor."""
    H, W = a.shape[:2]
    h2, w2 = -(-H // CELDA) * CELDA, -(-W // CELDA) * CELDA
    out = np.zeros((h2, w2, 4), np.uint8)
    out[h2 - H :, :W] = a  # apoyada al ras abajo
    return out


def a_imagen(a: np.ndarray) -> Image.Image:
    return Image.fromarray(np.ascontiguousarray(a), "RGBA")


def aberturas(im, margen: int = 5, min_px: int = 6) -> list[tuple[int, int]]:
    """Franjas x [x0, x1) de puertas y ventanas, para protegerlas al re-medir.

    Se detectan por el VIDRIO. Medido en `Terraced_House_Side_View_Base_2`: el vidrio del pack es celeste con el VERDE
    por encima del rojo (g−r 18-25) y el revestimiento gris-azulado no (g−r 3-6) — «azul > rojo» solo agarra la pared
    más `margen` px de marco a cada lado. Una puerta del pack lleva su mirilla o su paño vidriado, así que cae
    con la misma regla. Las franjas que se tocan se funden."""
    a = _arr(im).astype(np.int32)
    r, g, b, al = a[..., 0], a[..., 1], a[..., 2], a[..., 3]
    lum = 0.299 * r + 0.587 * g + 0.114 * b
    vidrio = (al > 200) & (g > r + 14) & (b > g + 5) & (lum > 100)
    cols = vidrio.sum(0) >= 2
    franjas, x = [], 0
    W = a.shape[1]
    while x < W:
        if cols[x]:
            x0 = x
            while x < W and cols[x]:
                x += 1
            if x - x0 >= min_px:
                franjas.append([max(0, x0 - margen), min(W, x + margen)])
        x += 1
    fundidas: list[list[int]] = []
    for f in franjas:
        if fundidas and f[0] <= fundidas[-1][1]:
            fundidas[-1][1] = max(fundidas[-1][1], f[1])
        else:
            fundidas.append(f)
    return [tuple(f) for f in fundidas]


def ancho_minimo(im, proteger=()) -> int:
    """El ancho más chico al que se puede llevar la pieza sin tocar lo protegido ni inventar pixel.

    Agrandar siempre se puede (una gemela se duplica las veces que haga falta); achicar tiene piso: cada gemela
    quitada deja de serlo. Es lo que un importador mira para elegir, de las fachadas del pack, una que entre en la
    huella — en vez de estirar una que no entra (el ×1,4 de `building::AGRANDE_MAX`)."""
    a = _arr(im)
    pro = np.zeros(a.shape[1], bool)
    for p0, p1 in proteger:
        pro[max(0, p0) : min(a.shape[1], p1)] = True
    # una tira de k líneas iguales se puede bajar a 1
    return a.shape[1] - sum(l - 1 for _, l in _tiras(a, 1, pro))


# ── la fachada a la medida: módulos del pack uno al lado del otro ──────────────────────────────────────────────

#: Cuánto se deja re-medir un módulo solo. Medido con el juez sobre `Terraced_House_Side_View_Base_2` (10
#: celdas): a 8 y a 13 da A LA ALTURA; a 16 queda POR DEBAJO (plano 0,30 contra 0,19 del pack: un paño liso
#: enorme). Más allá de esto se agrega otro módulo, como hace el pack con sus casas adosadas.
ESTIRA_MAX = 1.3

# (Se probó, y se descartó, un compositor por camino óptimo en el grafo de columnas: las fachadas del pack no
#  tienen bahías que se repitan exactas —coinciden al 77 % como mucho— y el camino más barato terminaba
#  repitiendo UNA columna decenas de veces. Ver biblia §8, «La casa a la medida».)


def _recortar_lados(a: np.ndarray, izq: bool, der: bool) -> np.ndarray:
    """Saca las columnas casi transparentes (sombra, filo) del lado que se va a pegar con otro módulo."""
    opac = (a[..., 3] > 200).mean(0)
    lim = 0.6 * float(np.median(opac[opac > 0]))
    x0, x1 = 0, a.shape[1]
    while izq and x0 < x1 and opac[x0] < lim:
        x0 += 1
    while der and x1 > x0 and opac[x1 - 1] < lim:
        x1 -= 1
    return a[:, x0:x1]


def _alinear_alto(mods: list[np.ndarray]) -> list[np.ndarray]:
    H = max(m.shape[0] for m in mods)
    out = []
    for m in mods:
        if m.shape[0] < H:  # apoyados al ras abajo
            m = np.concatenate([np.zeros((H - m.shape[0], m.shape[1], 4), np.uint8), m], axis=0)
        out.append(m)
    return out


def _repartir(partes: list[np.ndarray], ancho: int) -> list[int]:
    """El ancho de cada módulo: en proporción a su ancho natural, sin salirse de [mínimo, ×ESTIRA_MAX]."""
    lo = [ancho_minimo(p, aberturas(p)) for p in partes]
    hi = [int(p.shape[1] * ESTIRA_MAX) for p in partes]
    nat = sum(p.shape[1] for p in partes)
    metas = [min(max(round(p.shape[1] * ancho / nat), l), h) for p, l, h in zip(partes, lo, hi)]
    k = 0
    while sum(metas) != ancho and k < 10 * ancho:
        i = k % len(metas)
        if sum(metas) < ancho and metas[i] < hi[i]:
            metas[i] += 1
        elif sum(metas) > ancho and metas[i] > lo[i]:
            metas[i] -= 1
        k += 1
    return metas


def fachada_a_medida(modulos: list, ancho: int, max_modulos: int = 4, juzgar: bool = True) -> tuple[np.ndarray, list[int]]:
    """**Una fachada de exactamente `ancho` px con módulos del pack**, con las aberturas intactas. Devuelve
    (imagen, índices de los módulos usados).

    Prueba las combinaciones (con repetición, hasta `max_modulos`, sin dos iguales pegadas si hay otra) de la
    más cercana a su ancho natural a la más lejana, y **cada una pasa por el juez** (`lib/juez.py`) contra los
    módulos originales: devuelve la primera que da `A LA ALTURA` o `SUPERA`. Se planta si ninguna: mejor un error
    —y que el importador elija otra familia de fachada— que una casa estirada (el ×1,4 que deja asomar el
    interior) o un paño liso que se nota hecho a máquina."""
    import itertools

    from lib.juez import juzgar as juez

    base = [_arr(m) for m in modulos]
    candidatos = []
    for n in range(1, max_modulos + 1):
        for combo in itertools.product(range(len(base)), repeat=n):
            if n > 1 and len(base) > 1 and any(combo[k] == combo[k + 1] for k in range(n - 1)):
                continue
            partes = [_recortar_lados(base[c], k > 0, k < n - 1) for k, c in enumerate(combo)]
            nat = sum(p.shape[1] for p in partes)
            lo = sum(ancho_minimo(p, aberturas(p)) for p in partes)
            hi = sum(int(p.shape[1] * ESTIRA_MAX) for p in partes)
            if lo <= ancho <= hi:
                candidatos.append((abs(ancho - nat) / nat + 0.02 * n, combo, partes))
    candidatos.sort(key=lambda c: c[0])
    motivos = []
    for _, combo, partes in candidatos[:8]:
        metas = _repartir(partes, ancho)
        hechas = [redimensionar(p, ancho=m, proteger_x=aberturas(p)) for p, m in zip(partes, metas)]
        out = np.concatenate(_alinear_alto(hechas), axis=1)
        if not juzgar:
            return out, list(combo)
        r = juez(out, [base[c] for c in sorted(set(combo))], escala=False)
        if r["veredicto"] != "POR DEBAJO":
            return out, list(combo)
        motivos.append(f"{list(combo)}: {r['faltas'][0]}")
    raise SystemExit(
        f"edificio: ninguna combinación de {len(base)} módulo(s) da {ancho} px a la altura del pack"
        + ("" if motivos else f" (ninguna entra re-midiendo ≤ ×{ESTIRA_MAX})")
        + "".join(f"\n  {m}" for m in motivos)
    )
