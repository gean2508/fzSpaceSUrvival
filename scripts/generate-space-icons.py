#!/usr/bin/env python3
"""Generate Hunger Arena space-themed app icons (fStyle synthwave + rockets)."""
from __future__ import annotations

import math
import random
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parents[1]
NATIVE_RES = ROOT / "native" / "android" / "app" / "src" / "main" / "res"
BRANDING = ROOT / "native" / "branding"
CLIENT_ICON = ROOT / "client" / "public" / "icon.png"

VOID = (10, 0, 20)
VOID_DEEP = (5, 0, 10)
PINK = (255, 42, 109)
CYAN = (0, 245, 255)
PURPLE = (123, 47, 247)
GREEN = (57, 255, 20)
EARTH_BLUE = (110, 180, 255)
MOON_GREY = (180, 185, 195)
ORANGE = (255, 179, 71)

MIPMAP = {
    "mipmap-mdpi": 48,
    "mipmap-hdpi": 72,
    "mipmap-xhdpi": 96,
    "mipmap-xxhdpi": 144,
    "mipmap-xxxhdpi": 192,
}


def lerp(a: float, b: float, t: float) -> float:
    return a + (b - a) * t


def draw_starfield(draw: ImageDraw.ImageDraw, size: int, seed: int = 42):
    rng = random.Random(seed)
    for _ in range(size // 2):
        x = rng.randint(0, size - 1)
        y = rng.randint(0, size - 1)
        r = rng.choice([1, 1, 1, 2])
        alpha = rng.randint(120, 255)
        color = (255, 255, 255, alpha) if rng.random() > 0.15 else CYAN + (alpha,)
        draw.ellipse([x - r, y - r, x + r, y + r], fill=color)


def draw_horizon_grid(draw: ImageDraw.ImageDraw, size: int):
    horizon = int(size * 0.72)
    step = max(8, size // 24)
    for i in range(0, size // 2, step):
        y = horizon + i
        if y >= size:
            break
        alpha = int(180 * (1 - i / (size * 0.35)))
        draw.line([(0, y), (size, y)], fill=CYAN + (max(20, alpha),), width=1)
    vanish_x = size // 2
    for x in range(-size, size * 2, step):
        x0 = vanish_x + (x - vanish_x) // 3
        draw.line([(x, size), (x0, horizon)], fill=CYAN + (55,), width=1)
    # sunset band
    for i in range(size // 5):
        alpha = int(70 * (1 - i / (size / 5)))
        draw.rectangle([0, horizon - i, size, horizon - i + 1], fill=PINK + (alpha,))


def draw_planet(draw: ImageDraw.ImageDraw, cx: int, cy: int, r: int, base: tuple[int, int, int]):
    for i in range(r, 0, -1):
        t = i / r
        shade = tuple(int(c * (0.55 + 0.45 * t)) for c in base)
        draw.ellipse([cx - i, cy - i, cx + i, cy + i], fill=shade + (255,))
    # highlight
    draw.ellipse(
        [cx - r // 2, cy - r // 2, cx - r // 6, cy - r // 6],
        fill=(255, 255, 255, 90),
    )


def draw_rocket(draw: ImageDraw.ImageDraw, cx: int, cy: int, scale: float):
    s = scale
    # exhaust plume
    flame_pts = [
        (cx - 18 * s, cy + 95 * s),
        (cx, cy + 150 * s),
        (cx + 18 * s, cy + 95 * s),
    ]
    draw.polygon(flame_pts, fill=ORANGE + (220,))
    draw.polygon(
        [
            (cx - 10 * s, cy + 95 * s),
            (cx, cy + 125 * s),
            (cx + 10 * s, cy + 95 * s),
        ],
        fill=CYAN + (200,),
    )
    # body
    body = [
        (cx - 28 * s, cy + 90 * s),
        (cx - 22 * s, cy - 70 * s),
        (cx + 22 * s, cy - 70 * s),
        (cx + 28 * s, cy + 90 * s),
    ]
    draw.polygon(body, fill=(230, 235, 245, 255), outline=PINK)
    # nose cone
    nose = [
        (cx - 22 * s, cy - 70 * s),
        (cx, cy - 130 * s),
        (cx + 22 * s, cy - 70 * s),
    ]
    draw.polygon(nose, fill=PINK + (255,), outline=CYAN)
    # window
    wr = 16 * s
    draw.ellipse(
        [cx - wr, cy - 35 * s - wr, cx + wr, cy - 35 * s + wr],
        fill=CYAN + (255,),
        outline=PURPLE,
        width=max(1, int(2 * s)),
    )
    # fins
    for side in (-1, 1):
        fin = [
            (cx + side * 22 * s, cy + 40 * s),
            (cx + side * 55 * s, cy + 95 * s),
            (cx + side * 28 * s, cy + 90 * s),
        ]
        draw.polygon(fin, fill=PURPLE + (255,), outline=CYAN)
    # stripe
    draw.rectangle(
        [cx - 22 * s, cy + 10 * s, cx + 22 * s, cy + 18 * s],
        fill=PINK + (255,),
    )


def draw_orbit_ring(draw: ImageDraw.ImageDraw, cx: int, cy: int, rx: int, ry: int, color):
    bbox = [cx - rx, cy - ry, cx + rx, cy + ry]
    draw.arc(bbox, start=200, end=340, fill=color, width=max(2, rx // 40))


def render_background(size: int) -> Image.Image:
    img = Image.new("RGBA", (size, size), VOID + (255,))
    draw = ImageDraw.Draw(img)
    draw_starfield(draw, size)
    draw_horizon_grid(draw, size)
    return img


def render_foreground(size: int) -> Image.Image:
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)
    scale = size / 512
    cx, cy = size // 2, int(size * 0.42)

    # distant moon (top-right safe zone)
    draw_planet(draw, int(size * 0.78), int(size * 0.18), int(22 * scale), MOON_GREY)
    # distant earth (top-left)
    draw_planet(draw, int(size * 0.2), int(size * 0.22), int(26 * scale), EARTH_BLUE)

    draw_orbit_ring(draw, cx, cy + int(30 * scale), int(95 * scale), int(35 * scale), CYAN + (90,))
    draw_rocket(draw, cx, cy, scale)

    # small satellite
    sx, sy = int(size * 0.68), int(size * 0.55)
    draw.rectangle([sx - 8, sy - 4, sx + 8, sy + 4], fill=(200, 200, 210, 255), outline=CYAN)
    draw.rectangle([sx - 20, sy - 2, sx - 8, sy + 2], fill=CYAN + (200,))
    draw.rectangle([sx + 8, sy - 2, sx + 20, sy + 2], fill=CYAN + (200,))

    return img


def render_poster(size: int = 1024) -> Image.Image:
    bg = render_background(size)
    fg = render_foreground(size)
    poster = Image.alpha_composite(bg, fg)

    draw = ImageDraw.Draw(poster)
    try:
        title_font = ImageFont.truetype(
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", max(18, size // 22)
        )
        sub_font = ImageFont.truetype(
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", max(12, size // 36)
        )
    except OSError:
        title_font = ImageFont.load_default()
        sub_font = title_font

    draw.text((size * 0.06, size * 0.86), "HUNGER ARENA", fill=GREEN + (230,), font=title_font)
    draw.text((size * 0.06, size * 0.92), "EARTH · LUNA · VOID", fill=CYAN + (180,), font=sub_font)

    # subtle vignette
    vignette = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    vdraw = ImageDraw.Draw(vignette)
    margin = int(size * 0.04)
    vdraw.rectangle([0, 0, size, margin], fill=(0, 0, 0, 80))
    vdraw.rectangle([0, size - margin, size, size], fill=(0, 0, 0, 100))
    poster = Image.alpha_composite(poster, vignette)
    return poster


def save_mipmap(icon: Image.Image, folder: str, size: int):
    out_dir = NATIVE_RES / folder
    out_dir.mkdir(parents=True, exist_ok=True)
    resized = icon.resize((size, size), Image.Resampling.LANCZOS)
    resized.save(out_dir / "ic_launcher.png")
    resized.save(out_dir / "ic_launcher_round.png")


def main():
    BRANDING.mkdir(parents=True, exist_ok=True)
    poster = render_poster(1024)
    poster.save(BRANDING / "app-icon-poster.png")
    poster.save(CLIENT_ICON)
    print(f"✓ poster {BRANDING / 'app-icon-poster.png'}")
    print(f"✓ client  {CLIENT_ICON}")

    fg_hi = render_foreground(1024)
    fg_hi.save(NATIVE_RES / "drawable-nodpi" / "ic_launcher_foreground.png")
    print(f"✓ foreground {NATIVE_RES / 'drawable-nodpi' / 'ic_launcher_foreground.png'}")

    for folder, size in MIPMAP.items():
        save_mipmap(poster, folder, size)
        print(f"✓ {folder}/ic_launcher.png ({size}px)")

    print("\nSpace icons ready — rebuild APK to apply.")


if __name__ == "__main__":
    main()
