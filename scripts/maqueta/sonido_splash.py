#!/usr/bin/env python3
"""El sonido de la splash, sintetizado — nada de archivos comprados.

La referencia es el tecleo de Grok: pops cortos, blandos y con la altura subiendo.
Acá cada pop es una MANCHA que aterriza, así que caen en los mismos tiempos que la
animación (una cada 0,135 s, ocho en total) y al final entra un acorde suave
mientras aparece el logo.

Se sintetiza igual que el resto del audio del juego (`scripts/generate-synth-audio.mjs`),
así que en el APK no pesa un archivo: son cuatro osciladores y una envolvente.
"""

import math
import struct
import sys
import wave
from pathlib import Path

SR = 44100
DUR = 2.6


def env_exp(n, ataque, caida):
    """Envolvente percusiva: sube en `ataque` s y cae exponencialmente."""
    out = []
    na = max(1, int(ataque * SR))
    for i in range(n):
        if i < na:
            out.append(i / na)
        else:
            out.append(math.exp(-(i - na) / (caida * SR)))
    return out


def pop(buf, t0, f0, vol=0.5):
    """Una gota: seno con caída de altura + un chasquido cortito de ruido."""
    dur = 0.22
    n = int(dur * SR)
    e = env_exp(n, 0.003, 0.055)
    fase = 0.0
    semilla = int(t0 * 1000) or 1
    for i in range(n):
        k = i / n
        # la altura cae de 1,8× a 1×: eso es lo que lo hace "gota" y no "beep"
        f = f0 * (1.0 + 0.8 * math.exp(-k * 14))
        fase += 2 * math.pi * f / SR
        s = math.sin(fase) * 0.85 + math.sin(fase * 2) * 0.12
        if i < int(0.008 * SR):          # chasquido del impacto
            semilla = (semilla * 1103515245 + 12345) & 0x7FFFFFFF
            s += ((semilla / 0x7FFFFFFF) * 2 - 1) * 0.55 * (1 - i / (0.008 * SR))
        j = int(t0 * SR) + i
        if j < len(buf):
            buf[j] += s * e[i] * vol


def acorde(buf, t0, base=196.0, vol=0.26):
    """El colchón del final: tríada suave que entra con el logo."""
    dur = 1.3
    n = int(dur * SR)
    e = env_exp(n, 0.09, 0.42)
    for mult, peso in ((1.0, 1.0), (1.5, 0.7), (2.0, 0.5), (3.0, 0.22)):
        fase = 0.0
        for i in range(n):
            fase += 2 * math.pi * base * mult / SR
            j = int(t0 * SR) + i
            if j < len(buf):
                buf[j] += math.sin(fase) * e[i] * vol * peso


def main(destino):
    n = int(DUR * SR)
    buf = [0.0] * n
    # ocho manchas, una cada 0,135 s — los mismos tiempos que la animación
    escala = [329.6, 392.0, 440.0, 493.9, 587.3, 659.3, 784.0, 880.0]
    for i, f in enumerate(escala):
        pop(buf, 0.06 + i * 0.135, f, 0.52 - i * 0.02)
    acorde(buf, 1.16)

    pico = max(abs(x) for x in buf) or 1.0
    with wave.open(str(destino), "w") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(b"".join(
            struct.pack("<h", int(max(-1.0, min(1.0, x / pico * 0.92)) * 32767)) for x in buf))
    print(destino, round(DUR, 2), "s")


if __name__ == "__main__":
    main(Path(sys.argv[1] if len(sys.argv) > 1 else ".") / "10-splash-sonido.wav")
