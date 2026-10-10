#!/usr/bin/env python3
"""iNilla's app icon, drawn from code so it can be regenerated (needs Pillow).

    python3 ios/tools/make-icons.py      # writes ios/icons/*.png

A gold "N" inside a ring on a deep night-blue field. The PNGs are committed; CI never
runs this.
"""
import math
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

OUT = Path(__file__).resolve().parent.parent / "icons"
SIZES = {
    "AppIcon60x60@2x.png": 120,
    "AppIcon60x60@3x.png": 180,
    "AppIcon76x76@2x.png": 152,
    "AppIcon83.5x83.5@2x.png": 167,
    "AppIcon1024.png": 1024,
}

GOLD = (212, 175, 55)
GOLD_HI = (250, 222, 130)


def draw(size: int = 1024) -> Image.Image:
    s = size
    img = Image.new("RGB", (s, s))
    px = img.load()
    # Radial night-blue field.
    for y in range(s):
        for x in range(s):
            d = math.hypot(x - s * 0.5, y - s * 0.42) / (s * 0.75)
            t = max(0.0, min(1.0, d))
            r = int(28 * (1 - t) + 6 * t)
            g = int(34 * (1 - t) + 7 * t)
            b = int(64 * (1 - t) + 14 * t)
            px[x, y] = (r, g, b)
    glow = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    gd = ImageDraw.Draw(glow)
    c = s / 2
    ring_r = s * 0.36
    w = int(s * 0.035)
    gd.ellipse([c - ring_r, c - ring_r, c + ring_r, c + ring_r], outline=GOLD + (255,), width=w)
    # The letter: two staves and the diagonal between them, an "N" with the rune's weight.
    lw = s * 0.07
    left, right = c - s * 0.15, c + s * 0.15
    top, bot = c - s * 0.24, c + s * 0.24
    gold = GOLD + (255,)
    gd.rectangle([left - lw / 2, top, left + lw / 2, bot], fill=gold)
    gd.rectangle([right - lw / 2, top, right + lw / 2, bot], fill=gold)
    gd.polygon(
        [(left - lw / 2, top), (left + lw / 2, top), (right + lw / 2, bot), (right - lw / 2, bot)],
        fill=gold,
    )
    # Small diamond — a gem on the ring's crown.
    dr = s * 0.045
    gy = c - ring_r
    gd.polygon([(c, gy - dr), (c + dr, gy), (c, gy + dr), (c - dr, gy)], fill=GOLD_HI + (255,))
    blurred = glow.filter(ImageFilter.GaussianBlur(radius=s * 0.02))
    base = img.convert("RGBA")
    base = Image.alpha_composite(base, Image.eval(blurred, lambda v: int(v * 0.6)))
    base = Image.alpha_composite(base, glow)
    return base.convert("RGB")


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    master = draw(1024)
    for name, size in SIZES.items():
        master.resize((size, size), Image.LANCZOS).save(OUT / name, optimize=True)
        print("wrote", OUT / name)


if __name__ == "__main__":
    main()
