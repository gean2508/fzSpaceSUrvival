#!/usr/bin/env python3
"""Fast painted sky textures (512×256 equirectangular)."""

import math
import os
import struct
import zlib

ROOT = os.path.normpath(
    os.path.join(os.path.dirname(__file__), "..", "..", "native", "crates", "ha-game", "assets", "sky")
)
W, H = 512, 256

STAGES = {
    "sky_night": ((0.08, 0.06, 0.14), (0.14, 0.12, 0.22), (0.18, 0.14, 0.28), -0.35),
    "sky_dawn": ((0.32, 0.28, 0.48), (0.96, 0.66, 0.78), (1.0, 0.72, 0.55), 0.05),
    "sky_morning": ((0.43, 0.72, 0.91), (0.96, 0.78, 0.91), (1.0, 0.86, 0.72), 0.35),
    "sky_noon": ((0.53, 0.78, 0.94), (0.91, 0.94, 1.0), (1.0, 0.95, 0.82), 0.72),
    "sky_afternoon": ((0.42, 0.60, 0.78), (0.94, 0.72, 0.82), (1.0, 0.78, 0.62), 0.42),
    "sky_dusk": ((0.29, 0.18, 0.43), (0.96, 0.55, 0.78), (1.0, 0.42, 0.55), 0.10),
    "sky_twilight": ((0.22, 0.16, 0.34), (0.78, 0.40, 0.58), (0.95, 0.45, 0.62), -0.08),
}


def lerp(a, b, t):
    return a + (b - a) * t


def write_png(path, rgb_rows):
    raw = b"".join(b"\x00" + bytes(row) for row in rgb_rows)

    def chunk(tag, data):
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", W, H, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 6))
        + chunk(b"IEND", b"")
    )
    with open(path, "wb") as f:
        f.write(png)


def build(name, zenith, horizon, warm, sun_elev):
    rows = []
    neon = 0.85 if name == "sky_dusk" else 0.0
    for y in range(H):
        v = y / (H - 1)
        elev = v
        row = []
        for x in range(W):
            t = elev**0.75
            r = lerp(horizon[0], zenith[0], t)
            g = lerp(horizon[1], zenith[1], t)
            b = lerp(horizon[2], zenith[2], t)
            band = math.exp(-((elev - 0.22) ** 2) / 0.018)
            r = lerp(r, warm[0], band * 0.55)
            g = lerp(g, warm[1], band * 0.55)
            b = lerp(b, warm[2], band * 0.45)
            if neon > 0:
                n = math.exp(-((elev - 0.15) ** 2) / 0.008) * neon
                r = lerp(r, 1.0, n * 0.35)
                g = lerp(g, 0.16, n * 0.5)
                b = lerp(b, 0.67, n * 0.45)
            u = x / (W - 1)
            sx, sy = 0.25, sun_elev * 0.5 + 0.5
            dist = math.hypot(u - sx, v - sy)
            glow = max(0.0, 1.0 - dist / 0.18) ** 3
            r = lerp(r, 1.0, glow * 0.85)
            g = lerp(g, 0.92, glow * 0.75)
            b = lerp(b, 0.55, glow * 0.4)
            steps = 6.0
            row.extend(
                [
                    int(max(0, min(255, math.floor(r * steps + 0.02) / steps * 255))),
                    int(max(0, min(255, math.floor(g * steps + 0.02) / steps * 255))),
                    int(max(0, min(255, math.floor(b * steps + 0.02) / steps * 255))),
                ]
            )
        rows.append(row)
    write_png(os.path.join(ROOT, f"{name}.png"), rows)
    print(f"wrote {name}.png")


def main():
    os.makedirs(ROOT, exist_ok=True)
    for name, (z, h, w, s) in STAGES.items():
        build(name, z, h, w, s)
    print(f"done — {len(STAGES)} files in {ROOT}")


if __name__ == "__main__":
    main()
