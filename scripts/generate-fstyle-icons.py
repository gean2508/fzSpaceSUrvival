#!/usr/bin/env python3
"""Generate fStyle app icons (Blood Dragon neon on void purple)."""
from __future__ import annotations

import math
import os
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]
MOBILE = ROOT / "apps" / "mobile" / "assets"
DESKTOP = ROOT / "apps" / "desktop" / "src-tauri" / "icons"

VOID = (10, 0, 20)
VOID_DEEP = (5, 0, 10)
PINK = (255, 0, 110)
CYAN = (0, 245, 255)
PURPLE = (123, 47, 247)
GREEN = (57, 255, 20)


def hex_rgb(h: str) -> tuple[int, int, int]:
    h = h.lstrip("#")
    return tuple(int(h[i : i + 2], 16) for i in (0, 2, 4))


def draw_grid(draw: ImageDraw.ImageDraw, size: int, step: int, color: tuple[int, int, int, int]):
    for x in range(0, size, step):
        draw.line([(x, 0), (x, size)], fill=color, width=1)
    for y in range(0, size, step):
        draw.line([(0, y), (size, y)], fill=color, width=1)


def draw_dragon_skull(draw: ImageDraw.ImageDraw, cx: int, cy: int, scale: float):
    s = scale
    # stylized skull / dragon visor (geometric fStyle mark)
    pts = [
        (cx - 80 * s, cy - 20 * s),
        (cx - 40 * s, cy - 70 * s),
        (cx, cy - 85 * s),
        (cx + 40 * s, cy - 70 * s),
        (cx + 80 * s, cy - 20 * s),
        (cx + 70 * s, cy + 40 * s),
        (cx + 30 * s, cy + 70 * s),
        (cx, cy + 55 * s),
        (cx - 30 * s, cy + 70 * s),
        (cx - 70 * s, cy + 40 * s),
    ]
    draw.polygon(pts, fill=PINK + (255,), outline=CYAN)
    # eyes
    draw.ellipse(
        [cx - 45 * s, cy - 35 * s, cx - 15 * s, cy - 5 * s], fill=CYAN + (255,)
    )
    draw.ellipse(
        [cx + 15 * s, cy - 35 * s, cx + 45 * s, cy - 5 * s], fill=CYAN + (255,)
    )
    # horn spikes
    draw.polygon(
        [
            (cx - 55 * s, cy - 55 * s),
            (cx - 75 * s, cy - 95 * s),
            (cx - 35 * s, cy - 65 * s),
        ],
        fill=PURPLE + (255,),
    )
    draw.polygon(
        [
            (cx + 55 * s, cy - 55 * s),
            (cx + 75 * s, cy - 95 * s),
            (cx + 35 * s, cy - 65 * s),
        ],
        fill=PURPLE + (255,),
    )


def render_icon(size: int, *, fg_only: bool = False, mono: bool = False) -> Image.Image:
    if fg_only:
        img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    else:
        img = Image.new("RGBA", (size, size), VOID + (255,))
    draw = ImageDraw.Draw(img)
    if not fg_only:
        draw_grid(draw, size, max(16, size // 32), CYAN + (40,))
        # horizon glow band
        for i in range(size // 3):
            alpha = int(90 * (1 - i / (size / 3)))
            draw.rectangle(
                [0, size - i - 1, size, size - i],
                fill=PINK + (alpha,),
            )
    scale = size / 512
    cx, cy = size // 2, int(size * 0.46)
    if mono:
        draw_dragon_skull_mono(draw, cx, cy, scale)
    else:
        draw_dragon_skull(draw, cx, cy, scale)
    # fStyle micro label on full icons only
    if size >= 256 and not fg_only and not mono:
        try:
            font = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", max(14, size // 28))
        except OSError:
            font = ImageFont.load_default()
        draw.text((size * 0.08, size * 0.82), "fStyle", fill=GREEN + (220,), font=font)
    return img


def draw_dragon_skull_mono(draw: ImageDraw.ImageDraw, cx: int, cy: int, scale: float):
    s = scale
    pts = [
        (cx - 80 * s, cy - 20 * s),
        (cx - 40 * s, cy - 70 * s),
        (cx, cy - 85 * s),
        (cx + 40 * s, cy - 70 * s),
        (cx + 80 * s, cy - 20 * s),
        (cx + 70 * s, cy + 40 * s),
        (cx + 30 * s, cy + 70 * s),
        (cx, cy + 55 * s),
        (cx - 30 * s, cy + 70 * s),
        (cx - 70 * s, cy + 40 * s),
    ]
    draw.polygon(pts, fill=(230, 230, 230, 255))


def save_all():
    MOBILE.mkdir(parents=True, exist_ok=True)
    DESKTOP.mkdir(parents=True, exist_ok=True)

    sizes = {
        MOBILE / "icon.png": 1024,
        MOBILE / "android-icon-foreground.png": 1024,
        MOBILE / "splash-icon.png": 512,
        MOBILE / "favicon.png": 48,
        DESKTOP / "icon.png": 512,
        DESKTOP / "128x128.png": 128,
        DESKTOP / "128x128@2x.png": 256,
        DESKTOP / "32x32.png": 32,
        DESKTOP / "64x64.png": 64,
    }

    fg = render_icon(1024, fg_only=True)
    fg.save(MOBILE / "android-icon-foreground.png")

    mono = render_icon(1024, mono=True)
    mono.save(MOBILE / "android-icon-monochrome.png")

    bg = Image.new("RGBA", (1024, 1024), VOID + (255,))
    bg_draw = ImageDraw.Draw(bg)
    draw_grid(bg_draw, 1024, 32, CYAN + (55,))
    bg.save(MOBILE / "android-icon-background.png")

    for path, sz in sizes.items():
        if path.name == "android-icon-foreground.png":
            continue
        img = render_icon(sz)
        img.save(path)
        print(f"✓ {path.relative_to(ROOT)} ({sz}px)")

    print("\nfStyle icons ready — run prebuild / tauri build to bake into apps.")


if __name__ == "__main__":
    save_all()
