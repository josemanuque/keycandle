#!/usr/bin/env python3
"""Draw the Keycandle icon (a keycap lifted on a ring of light, with a light line and a "K" legend)
in a light and a dark variant, and write every format the three systems need:

  icon-light.ico / icon-dark.ico     Windows (16-256 px)
  icon-light.icns / icon-dark.icns   macOS
  png/<variant>-<size>.png           Linux (and the README)

Requires Pillow and numpy:  python make_icon.py
"""
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

HERE = Path(__file__).resolve().parent
SIZES = [16, 24, 32, 48, 64, 128, 256, 512, 1024]

# Ring and line gradients (from the app's accent colours)
RING = ["#7C8CFF", "#C084FC", "#FF7AB6", "#FFB84D", "#4DE1FF", "#7C8CFF"]
LINE = ["#7C8CFF", "#C084FC"]

VARIANTS = {
    "light": dict(legend="#7C8597", tile="#E9ECF1", tile_edge="#D3D8E0", rim="#FFFFFF", cap_top="#F7F8FA", cap_bottom="#DADEE5", lip="#CDD2DA"),
    "dark": dict(legend="#B9C1CE", tile="#1E222A", tile_edge="#14171C", rim="#3A414D", cap_top="#323844", cap_bottom="#1F242C", lip="#171B21"),
}


def rgb(hex_colour):
    h = hex_colour.lstrip("#")
    return np.array([int(h[i:i + 2], 16) / 255 for i in (0, 2, 4)])


def round_rect(x, y, half, radius):
    """Signed distance to a centred rounded square (normalised units)."""
    qx, qy = np.abs(x) - half + radius, np.abs(y) - half + radius
    outside = np.hypot(np.maximum(qx, 0), np.maximum(qy, 0))
    return outside + np.minimum(np.maximum(qx, qy), 0) - radius


def coverage(dist, px):
    return np.clip(0.5 - dist / px, 0, 1)


def gradient(stops, t):
    """Colours along evenly spaced stops for t in 0..1 (array)."""
    cols = np.array([rgb(c) for c in stops])
    pos = t * (len(stops) - 1)
    i = np.clip(np.floor(pos).astype(int), 0, len(stops) - 2)
    f = (pos - i)[..., None]
    return cols[i] * (1 - f) + cols[i + 1] * f


def over(dst, colour, alpha):
    """Composite a colour layer (H,W,3) or (3,) with alpha (H,W) over an RGBA canvas."""
    a = alpha[..., None]
    out_a = a + dst[..., 3:] * (1 - a)
    safe = np.where(out_a > 0, out_a, 1)
    dst[..., :3] = (colour * a + dst[..., :3] * dst[..., 3:] * (1 - a)) / safe
    dst[..., 3:] = out_a


LEGEND_FONT = "C:/Windows/Fonts/seguisb.ttf"  # Segoe UI Semibold; any bold sans works


def draw(size, variant, legend=None):
    v = VARIANTS[variant]
    small = size <= 32
    ss = 8 if small else 2  # supersampling
    n = size * ss
    px = 2 / n
    y, x = np.mgrid[0:n, 0:n]
    x = (x + 0.5) / n * 2 - 1
    y = (y + 0.5) / n * 2 - 1
    img = np.zeros((n, n, 4))

    # Tile (full bleed at small sizes; a margin for the soft shadow when large)
    tile_half = 0.98 if small else 0.86
    tile_r = tile_half * 0.42
    if not small:
        shadow = coverage(round_rect(x, y - 0.035, tile_half, tile_r), px * 30) * 0.28
        over(img, np.zeros(3), shadow)
    d_tile = round_rect(x, y, tile_half, tile_r)
    over(img, rgb(v["tile"]), coverage(d_tile, px))
    # a faint lit edge on top-left, darker bottom-right (neumorphic rim)
    rim = coverage(np.abs(d_tile + px * 3) - px * 3, px) * 0.6
    shade = np.clip((x + y) * 0.5 + 0.5, 0, 1)[..., None]
    over(img, rgb(v["rim"]) * (1 - shade) + rgb(v["tile_edge"]) * shade, rim if not small else rim * 0)

    # Ring of light (conic gradient), with a glow at large sizes
    ring_half = 0.62 if small else 0.50
    ring_r = ring_half * 0.30
    angle = (np.arctan2(y, x) / (2 * np.pi) + 0.5 + 0.08) % 1
    ring_col = gradient(RING, angle)
    d_ring = round_rect(x, y, ring_half, ring_r)
    if not small:
        glow = Image.fromarray((coverage(d_ring, px) * 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(n * 0.05))
        over(img, ring_col, np.asarray(glow) / 255 * 0.75)
    over(img, ring_col, coverage(d_ring, px))

    # Keycap: raised, top-lit gradient, a lip at the bottom
    cap_half = ring_half - (0.10 if small else 0.065)
    cap_r = cap_half * 0.28
    d_cap = round_rect(x, y, cap_half, cap_r)
    over(img, rgb(v["lip"]), coverage(d_cap, px))
    top_y = y + (0.06 if small else 0.045)
    d_top = round_rect(x, top_y + 0.0, cap_half - (0.0 if small else 0.012), cap_r)
    d_top = np.maximum(d_top, d_cap)
    t = np.clip((y + cap_half) / (2 * cap_half), 0, 1)[..., None]
    over(img, rgb(v["cap_top"]) * (1 - t) + rgb(v["cap_bottom"]) * t, coverage(d_top, px))

    # The light line on the cap (like the keycap's homing bar)
    line_w, line_h = cap_half * (0.62 if small else 0.52), max(cap_half * (0.13 if small else 0.085), 1.6 / size)
    line_y = -cap_half * (0.40 if small else 0.46)  # near the top of the cap, like a keycap's legend bar
    lx, ly = x / line_w, (y - line_y)
    d_line = np.maximum(np.abs(x) - line_w + line_h, 0)
    d_line = np.hypot(d_line, ly) - line_h
    line_col = gradient(LINE, np.clip(lx * 0.5 + 0.5, 0, 1))
    if not small:
        lg = Image.fromarray((coverage(d_line, px) * 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(n * 0.02))
        over(img, line_col, np.asarray(lg) / 255 * 0.6)
    over(img, line_col, coverage(d_line, px))

    # Optional keycap legend under the light line, like a printed key.
    if legend:
        layer = Image.new("L", (n, n), 0)
        font = ImageFont.truetype(LEGEND_FONT, int(n * cap_half * 0.62))
        d = ImageDraw.Draw(layer)
        box = d.textbbox((0, 0), legend, font=font)
        w, h = box[2] - box[0], box[3] - box[1]
        cy = (cap_half * 0.18 + 1) / 2 * n  # a little below the cap's centre
        d.text(((n - w) / 2 - box[0], cy - h / 2 - box[1]), legend, font=font, fill=255)
        over(img, rgb(v["legend"]), np.asarray(layer) / 255)

    out = Image.fromarray((np.clip(img, 0, 1) * 255).astype(np.uint8), "RGBA")
    return out.resize((size, size), Image.LANCZOS)


def main():
    (HERE / "png").mkdir(exist_ok=True)
    for variant in VARIANTS:
        # The "K" legend only where it can be read: from 48 px up (smaller sizes keep just the light line).
        images = {s: draw(s, variant, "K" if s >= 48 else None) for s in SIZES}
        for s, im in images.items():
            im.save(HERE / "png" / f"{variant}-{s}.png")
        ico_sizes = [16, 24, 32, 48, 64, 128, 256]
        images[256].save(HERE / f"icon-{variant}.ico", sizes=[(s, s) for s in ico_sizes],
                         append_images=[images[s] for s in ico_sizes if s != 256])
        images[1024].save(HERE / f"icon-{variant}.icns")
        print(f"{variant}: done")


if __name__ == "__main__":
    main()
