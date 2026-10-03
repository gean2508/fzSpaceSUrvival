#!/usr/bin/env python3
"""**EL LABORATORIO DE SONIDO**: renderiza todo el audio del juego, lo mide y lo dibuja para poder AUDITARLO sin oído.

    python3 scripts/sfx-lab.py                    # renderiza el spec por defecto y audita
    python3 scripts/sfx-lab.py --json native/assets/audio.json
    python3 scripts/sfx-lab.py --sheet /tmp/sfx.png   # lámina de espectrogramas (la podés abrir con Read)

Por cada efecto/pista mide duración, pico, RMS de la parte activa, factor de cresta, centroide espectral y
continua, y falla si algo rompe el piso de calidad de la skill `audio-sfx`: clipping, silencio, DC, niveles
fuera de banda por categoría, o un sonido «de pitido» (una sola línea espectral y nada más). El espectrograma
deja ver lo que el oído sentiría: transitorio, cuerpo, cola, color del ruido.
"""
import json
import subprocess
import sys
import wave
from pathlib import Path

import numpy as np
from scipy import signal

RAIZ = Path(__file__).resolve().parent.parent
NATIVE = RAIZ / "native"
SALIDA = Path("/tmp/audio-lab")


def renderizar(json_path):
    SALIDA.mkdir(parents=True, exist_ok=True)
    cmd = ["cargo", "run", "-q", "-p", "ha-audio", "--profile", "qa", "--bin", "render_all", "--", str(SALIDA)]
    if json_path:
        cmd.append(str(json_path))
    subprocess.run(cmd, cwd=NATIVE, check=True)


def leer(p):
    with wave.open(str(p)) as w:
        n, rate = w.getnframes(), w.getframerate()
        x = np.frombuffer(w.readframes(n), dtype="<i2").astype(np.float32) / 32768.0
    return rate, x


def categoria(nombre):
    if nombre.startswith("mus_"):
        return "musica"
    if nombre.startswith(("sfx_step", "sfx_drip", "sfx_crystal")):
        return "ambiente"
    if nombre in ("sfx_click", "sfx_bag_open", "sfx_bag_close", "sfx_pickup", "sfx_craft", "sfx_jump", "sfx_discover", "sfx_fizzle", "sfx_goal", "sfx_deny"):
        return "ui"
    return "mundo"


#: banda de RMS (dBFS) por categoría: lo que hace que ningún sonido se pierda ni reviente al lado de otro.
BANDAS = {"ui": (-24.0, -15.0), "mundo": (-21.0, -12.0), "ambiente": (-30.0, -17.0), "musica": (-26.0, -17.0)}


def medir(rate, x):
    pico = float(np.abs(x).max())
    act = x[np.abs(x) > pico * 0.03] if pico > 0 else x
    rms = float(np.sqrt((act**2).mean())) if len(act) else 0.0
    f, t, S = signal.spectrogram(x, rate, nperseg=2048, noverlap=1536)
    pot = S.sum(1)
    centroide = float((f * pot).sum() / max(pot.sum(), 1e-12))
    # «pitido»: casi toda la energía en una sola línea espectral y duración corta
    k = int(pot.argmax())
    pico_esp = float(pot[max(0, k - 1) : k + 2].sum() / max(pot.sum(), 1e-12))
    return {
        "dur": len(x) / rate,
        "pico_db": 20 * np.log10(max(pico, 1e-9)),
        "rms_db": 20 * np.log10(max(rms, 1e-9)),
        "cresta": pico / max(rms, 1e-9),
        "centroide": centroide,
        "dc": float(x.mean()),
        "linea": pico_esp,
    }


def main():
    args = sys.argv[1:]
    json_path = Path(args[args.index("--json") + 1]) if "--json" in args else None
    sheet = Path(args[args.index("--sheet") + 1]) if "--sheet" in args else None
    renderizar(json_path)
    archivos = sorted(SALIDA.glob("*.wav"))
    fallas, filas = [], []
    for p in archivos:
        rate, x = leer(p)
        m = medir(rate, x)
        cat = categoria(p.stem)
        lo, hi = BANDAS[cat]
        malos = []
        if m["pico_db"] > -0.5:
            malos.append("clipping")
        if m["pico_db"] < -30:
            malos.append("casi silencioso")
        if abs(m["dc"]) > 0.01:
            malos.append(f"continua {m['dc']:+.3f}")
        if not lo <= m["rms_db"] <= hi:
            malos.append(f"RMS {m['rms_db']:.1f} dB fuera de {lo:.0f}..{hi:.0f} ({cat})")
        if cat in ("ui", "mundo") and m["linea"] > 0.6 and m["centroide"] > 250 and m["dur"] < 0.5:
            malos.append("es un pitido: una sola línea espectral")
        filas.append((p.stem, cat, m, malos, x, rate))
        if malos:
            fallas.append((p.stem, malos))
    print(f"{'sonido':26s} {'cat':8s} {'dur':>5s} {'pico':>6s} {'rms':>6s} {'cresta':>6s} {'centro':>7s}")
    for nombre, cat, m, malos, _, _ in filas:
        print(f"{nombre:26s} {cat:8s} {m['dur']:5.2f} {m['pico_db']:6.1f} {m['rms_db']:6.1f} {m['cresta']:6.1f} {m['centroide']:7.0f}  {'; '.join(malos)}")
    print(f"\n{len(filas) - len(fallas)}/{len(filas)} pasan el piso")
    if sheet:
        from PIL import Image, ImageDraw

        cols, W, H = 5, 300, 150
        img = Image.new("RGB", (cols * W, ((len(filas) + cols - 1) // cols) * (H + 14)), (20, 22, 30))
        d = ImageDraw.Draw(img)
        for i, (nombre, cat, m, malos, x, rate) in enumerate(filas):
            f, t, S = signal.spectrogram(x, rate, nperseg=512, noverlap=448)
            S = 10 * np.log10(S + 1e-10)
            S = np.clip((S + 90) / 70, 0, 1)
            sel = f < 10000
            im = Image.fromarray((np.flipud(S[sel]) * 255).astype(np.uint8)).resize((W - 4, H - 4), Image.BILINEAR).convert("RGB")
            # paleta caliente simple
            a = np.asarray(im)[..., 0].astype(np.float32) / 255
            col = np.stack([np.clip(a * 2, 0, 1), np.clip(a * 2 - 0.6, 0, 1), np.clip(0.5 - np.abs(a - 0.35), 0, 1) * 1.5], -1)
            x0, y0 = (i % cols) * W, (i // cols) * (H + 14)
            img.paste(Image.fromarray((col * 255).astype(np.uint8)), (x0 + 2, y0 + 12))
            d.text((x0 + 3, y0 + 1), f"{nombre} {m['dur']:.2f}s {m['rms_db']:.0f}dB" + (" !" if malos else ""), fill=(255, 120, 120) if malos else (220, 220, 220))
        img.save(sheet)
        print(f"lámina: {sheet}")
    sys.exit(1 if fallas else 0)


if __name__ == "__main__":
    main()
