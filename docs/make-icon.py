#!/usr/bin/env python3
"""Render the roses app icon: the "all caught up" ASCII rose on a dark ground.

The icon is not used by the app — it exists to link to the project from a web
page — but it is generated from the same art and palette the TUI draws, so it
stays truthful to the program if either changes.

    python3 docs/make-icon.py

writes `roses-icon.png` (rounded, for the web) and `roses-icon-square.png`
(full-bleed; iOS/PWA apply their own mask) next to this script, both 1024x1024.

Requires Pillow and macOS's Menlo; no other dependency, and nothing in the Rust
build depends on it.
"""

import math
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont, ImageFilter

# Kept in sync with ART in draw_caught_up() (src/tui.rs).
ART = [
    "  .---.  ",
    " / .-. \\ ",
    "| ( @ ) |",
    " \\ `-' / ",
    "  `---'  ",
    "   \\|/   ",
    "    |    ",
]
# The first rows are the bloom (rose gradient); the rest are the stem/leaf.
PETAL_ROWS = 5

# Kept in sync with src/theme.rs.
ROSE_LIGHT = (0xF2, 0xA9, 0xC4)
ROSE_DEEP = (0xB0, 0x39, 0x5B)
LEAF = (0x6B, 0x9E, 0x78)

BG_INNER = (0x24, 0x16, 0x1E)
BG_OUTER = (0x0C, 0x08, 0x0B)

# Menlo Bold: at icon sizes the regular weight's hairlines wash out below ~96px.
FONT_PATH = "/System/Library/Fonts/Menlo.ttc"
FONT_INDEX = 1
# Menlo's advance width is 0.6021 em, so this is the size at which one glyph
# fills one cell of the character grid.
MENLO_ADVANCE = 0.6021

# A terminal cell is about 1:2; squaring it up a little lets the bloom read as
# round and fill more of a square canvas.
CELL_ASPECT = 1.35
# Art size and placement as fractions of the canvas: a small rose parked in the
# top-right quadrant. The centre is high enough that the stem stops short of the
# horizontal midline, so the whole rose stays inside that quadrant.
ART_FRAC = 0.36
CENTER = (0.72, 0.27)

# Apple's continuous-corner radius, as a fraction of the icon's side.
CORNER_FRAC = 0.2237
# Render this many times oversized, then downsample — cheap antialiasing for
# both the glyph edges and the corner curve.
SUPERSAMPLE = 4


def lerp(a, b, t):
    """Component-wise blend, mirroring theme::lerp."""
    return tuple(round(x + (y - x) * t) for x, y in zip(a, b))


def row_color(row):
    if row < PETAL_ROWS:
        return lerp(ROSE_LIGHT, ROSE_DEEP, row / (PETAL_ROWS - 1))
    return LEAF


def squircle_mask(size, radius_frac=CORNER_FRAC, n=5.0):
    """A superellipse mask — closer to Apple's continuous corners than the
    circular arcs of a plain rounded rectangle."""
    mask = Image.new("L", (size, size), 0)
    draw = ImageDraw.Draw(mask)
    r = size * radius_frac
    # Everything but the four corner squares is straight edge.
    draw.rectangle([r, 0, size - r, size], fill=255)
    draw.rectangle([0, r, size, size - r], fill=255)
    for cx, cy, sx, sy in (
        (r, r, -1, -1),
        (size - r, r, 1, -1),
        (r, size - r, -1, 1),
        (size - r, size - r, 1, 1),
    ):
        # Trace one quarter of |x/r|^n + |y/r|^n = 1 and fill back to the centre.
        steps = 256
        points = []
        for i in range(steps + 1):
            ang = (i / steps) * math.pi / 2
            points.append(
                (
                    cx + sx * r * abs(math.cos(ang)) ** (2 / n),
                    cy + sy * r * abs(math.sin(ang)) ** (2 / n),
                )
            )
        points.append((cx, cy))
        draw.polygon(points, fill=255)
    return mask


def radial_background(size, inner, outer):
    """A soft radial gradient, built small and upscaled so it stays smooth
    without a per-pixel loop at full resolution."""
    n = 96
    small = Image.new("RGB", (n, n))
    px = small.load()
    c = (n - 1) / 2
    longest = math.hypot(c, c)
    for y in range(n):
        for x in range(n):
            d = math.hypot(x - c, y - c) / longest
            px[x, y] = lerp(inner, outer, min(1.0, d**0.9))
    return small.resize((size, size), Image.LANCZOS)


def render(
    path,
    size=1024,
    rounded=True,
    art_frac=ART_FRAC,
    center=CENTER,
    cell_aspect=CELL_ASPECT,
    glow=True,
):
    s = size * SUPERSAMPLE
    cols = max(len(row) for row in ART)

    cell_w = (s * art_frac) / cols
    cell_h = cell_w * cell_aspect
    art_w, art_h = cell_w * cols, cell_h * len(ART)
    left = s * center[0] - art_w / 2
    top = s * center[1] - art_h / 2

    font = ImageFont.truetype(FONT_PATH, int(cell_w / MENLO_ADVANCE), index=FONT_INDEX)

    art = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    draw = ImageDraw.Draw(art)
    for r, line in enumerate(ART):
        # Sit the baseline low in the cell so '.' and '_' land at the bottom of
        # their row, the way they do in a terminal.
        y = top + r * cell_h + cell_h * 0.80
        for c, ch in enumerate(line):
            if ch == " ":
                continue
            x = left + c * cell_w + cell_w / 2
            draw.text((x, y), ch, font=font, fill=row_color(r) + (255,), anchor="ms")

    canvas = radial_background(s, BG_INNER, BG_OUTER).convert("RGBA")
    if glow:
        # A rose halo keeps the glyphs from reading as scratches on black.
        halo = art.filter(ImageFilter.GaussianBlur(radius=cell_w * 0.55))
        halo.putalpha(halo.getchannel("A").point(lambda a: int(a * 0.55)))
        canvas = Image.alpha_composite(canvas, halo)
    canvas = Image.alpha_composite(canvas, art)

    if rounded:
        canvas.putalpha(squircle_mask(s))

    canvas.resize((size, size), Image.LANCZOS).save(path)
    return path


def main():
    here = Path(__file__).resolve().parent
    for name, rounded in (("roses-icon.png", True), ("roses-icon-square.png", False)):
        print("wrote", render(here / name, rounded=rounded))


if __name__ == "__main__":
    main()
