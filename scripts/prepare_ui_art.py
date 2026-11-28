#!/usr/bin/env python3
"""Prepare the HUD's painted art from its generated masters.

Masters live in the gitignored ``assets/ui/source/``; the prepared WebPs in
``assets/ui/`` and the generated CSS beside the styles that use them are
tracked, so the build never runs this script. Rerun it after replacing a
master:

    python3 scripts/prepare_ui_art.py           # needs Pillow and NumPy

Each master is opaque art on pure black. The black around it becomes
transparent (anti-aliased edges are un-blended from black) and the art is
cropped to itself. Then:

- The unit frame stays whole: the live bars are drawn on top of it, each
  sized to the inside of its painted trough, so this measures the troughs,
  the name plaque and the round socket, as fractions of the art
  (``unit-frame-art.css``). It comes in two, both as painted: with the
  round portrait socket (``unit-frame.webp``) and without it
  (``unit-frame-bare.webp``).
- The cast bar frame is a 9-slice border image: its gold ends and edges keep
  their shape while its middle stretches to any bar length. This measures
  the slices, which end where the dark trough inside begins
  (``cast-bar-art.css``); the cast fill sits in that trough.
- The window frame is a 9-slice border too, its middle cleared and its
  corner slices big enough to keep the gold corner ornaments whole
  (``web/src/styles/window-art.css``); windows and dialogs wear it.
"""

from collections import deque
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "assets/ui/source"
OUT = ROOT / "assets/ui"
STYLES = ROOT / "web/src/features/unit-frames"
QUALITY = 90
BACKGROUND = 8  # max channel value that is the black ground
EDGE = 2  # px band around the ground un-blended from black

UNIT_FRAME = {
    # The frame with its round portrait socket, and the same frame without
    # it: one canvas, the body in the same place in both.
    "master": "unit-frame.png",
    "bare": "unit-frame-bare.png",
    # Shown at 336 CSS px wide (272 in lists) with the portrait: 2.5x for
    # sharp high-DPI screens. The bare frame keeps the same pixel density.
    "width": 840,
    # The troughs: a seed inside each (master pixels) and the brightness that
    # bounds its dark inside.
    "parts": {"health": ((630, 192), 30), "resource": ((630, 296), 30)},
    # Painted parts whose stone is as bright as their bevels, measured by hand
    # from brightness profiles of each master (x0, y0, x1, y1): the plaque
    # is drawn a little shorter in the portrait frame.
    "boxes": {
        "master": {"plaque": (396, 64, 872, 120)},
        "bare": {"plaque": (372, 64, 888, 120)},
    },
    # The portrait's dark socket, inside its ring (portrait frame only).
    "socket": (75, 125, 265, 325),
}
CAST_BAR = {
    "master": "cast-bar.png",
    # Its slices are drawn a few CSS px thick: half size keeps them sharp.
    "width": 800,
    "trough": ((800, 250), 30),
}


def flood(lum, seeds, limit):
    """Pixels darker than `limit` connected to any seed (4-neighbour)."""
    h, w = lum.shape
    mask = np.zeros((h, w), bool)
    queue = deque()
    for x, y in seeds:
        if lum[y, x] <= limit and not mask[y, x]:
            mask[y, x] = True
            queue.append((x, y))
    while queue:
        x, y = queue.popleft()
        for nx, ny in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)):
            if 0 <= nx < w and 0 <= ny < h and not mask[ny, nx] and lum[ny, nx] <= limit:
                mask[ny, nx] = True
                queue.append((nx, ny))
    return mask


def grow(mask, steps):
    out = mask.copy()
    for _ in range(steps):
        g = out.copy()
        g[1:] |= out[:-1]
        g[:-1] |= out[1:]
        g[:, 1:] |= out[:, :-1]
        g[:, :-1] |= out[:, 1:]
        out = g
    return out


def bbox(mask):
    ys, xs = np.nonzero(mask)
    return xs.min(), ys.min(), xs.max() + 1, ys.max() + 1


def cut_out(name):
    """The master on a transparent ground: (rgb, lum, alpha)."""
    rgb = np.asarray(Image.open(SOURCE / name).convert("RGB"), dtype=np.float64)
    lum = rgb.max(axis=2)
    h, w = lum.shape
    border = [(x, 0) for x in range(w)] + [(x, h - 1) for x in range(w)]
    border += [(0, y) for y in range(h)] + [(w - 1, y) for y in range(h)]
    ground = flood(lum, border, BACKGROUND)
    alpha = np.where(ground, 0.0, 1.0)
    # Anti-aliased edge pixels were blended over black: recover their alpha
    # from brightness against their opaque neighbours, and un-blend colour.
    band = grow(ground, EDGE) & ~ground
    # The opaque neighbours: a thin ring just inside the edge band.
    inner = grow(ground, EDGE + 3) & ~grow(ground, EDGE)
    ref = max(np.percentile(lum[inner], 25), 1.0)
    alpha[band] = np.clip(lum[band] / ref, 0, 1)
    color = rgb.copy()
    soft = band & (alpha > 0)
    color[soft] = np.clip(rgb[soft] / alpha[soft, None], 0, 255)
    return color, lum, alpha


def part_box(lum, seed, limit, name):
    part = flood(lum, [seed], limit)
    if part.sum() > 0.25 * lum.size:
        raise SystemExit(f"{name}: the fill leaked; check its seed or limit")
    return bbox(part)


def save(color, alpha, crop, width, out):
    x0, y0, x1, y1 = crop
    rgba = np.dstack([color, alpha * 255])[y0:y1, x0:x1]
    height = round(width * (y1 - y0) / (x1 - x0))
    image = Image.fromarray(np.round(rgba).astype(np.uint8), "RGBA").resize(
        (width, height), Image.LANCZOS
    )
    image.save(out, "WEBP", quality=QUALITY, method=6, alpha_quality=100)
    print(f"{out.relative_to(ROOT)}: {width}x{height}, {out.stat().st_size:,} bytes")
    return width / (x1 - x0)


def write_css(path, lines):
    banner = [
        "/* Generated by scripts/prepare_ui_art.py from the art's master.",
        "   Do not edit. */",
    ]
    path.write_text("\n".join(banner + lines) + "\n")
    print(f"{path.relative_to(ROOT)}")


def unit_frame():
    spec = UNIT_FRAME
    frames = {}
    for name in ("master", "bare"):
        color, lum, alpha = cut_out(spec[name])
        boxes = {n: part_box(lum, seed, limit, n) for n, (seed, limit) in spec["parts"].items()}
        frames[name] = (color, alpha, bbox(alpha > 0), {**boxes, **spec["boxes"][name]})
    color, alpha, full, boxes = frames["master"]
    save(color, alpha, full, spec["width"], OUT / "unit-frame.webp")
    bare_color, bare_alpha, bare, bare_boxes = frames["bare"]
    share = (bare[2] - bare[0]) / (full[2] - full[0])
    save(bare_color, bare_alpha, bare, round(spec["width"] * share), OUT / "unit-frame-bare.webp")

    def pct(value, total):
        return f"{100 * value / total:.3f}%"

    def block(selector, crop, parts, share):
        x0, y0, x1, y1 = crop
        cw, ch = x1 - x0, y1 - y0
        share_text = f"{share:.4f}".rstrip("0").rstrip(".")
        lines = [f"{selector} {{", f"  --art-ratio: {cw} / {ch};", f"  --art-share: {share_text};"]
        for name, (bx0, by0, bx1, by1) in parts.items():
            lines += [
                f"  --{name}-x: {pct(bx0 - x0, cw)};",
                f"  --{name}-y: {pct(by0 - y0, ch)};",
                f"  --{name}-w: {pct(bx1 - bx0, cw)};",
                f"  --{name}-h: {pct(by1 - by0, ch)};",
            ]
        return lines + ["}"]

    lines = [
        "/* Where the unit frame art's parts sit, as fractions of the art: with",
        "   the portrait, and without it (`.bare`, narrower by --art-share). */",
    ]
    lines += block(".unit-frame", full, {**boxes, "socket": spec["socket"]}, 1)
    lines += block(".unit-frame.bare", bare, bare_boxes, share)
    write_css(STYLES / "unit-frame-art.css", lines)


def cast_bar():
    spec = CAST_BAR
    color, lum, alpha = cut_out(spec["master"])
    seed, limit = spec["trough"]
    tx0, ty0, tx1, ty1 = part_box(lum, seed, limit, "cast trough")
    crop = bbox(alpha > 0)
    x0, y0, x1, y1 = crop
    scale = save(color, alpha, crop, spec["width"], OUT / "cast-bar.webp")
    # The slices, in the prepared image's pixels: top, right, bottom, left.
    top, right = round((ty0 - y0) * scale), round((x1 - tx1) * scale)
    bottom, left = round((y1 - ty1) * scale), round((tx0 - x0) * scale)
    height = round(spec["width"] * (y1 - y0) / (x1 - x0))
    lines = [
        "/* The cast bar frame's 9-slice: where its dark trough begins, in the",
        "   prepared image's pixels, and the image's height. A bar of height H",
        "   draws each edge at (edge / height) x H. */",
        ":root {",
        f"  --cast-art-slice: {top} {right} {bottom} {left};",
        f"  --cast-art-height: {height};",
        f"  --cast-art-top: {top};",
        f"  --cast-art-right: {right};",
        f"  --cast-art-bottom: {bottom};",
        f"  --cast-art-left: {left};",
        "}",
    ]
    write_css(STYLES / "cast-bar-art.css", lines)


WINDOW_FRAME = {
    "master": "window-frame.png",
    # A 9-slice border: its gold corner ornaments reach `corner` master px in
    # from the frame's outer edge, its stone band is `stone` px thick, and
    # its middle (pure black in the master) is cleared.
    "corner": 176,
    "stone": 66,
    "centre": (512, 512),
    # Its stone is drawn about 11 CSS px thick: twice that for sharp screens.
    "width": 360,
}


def window_frame():
    spec = WINDOW_FRAME
    color, lum, alpha = cut_out(spec["master"])
    alpha[flood(lum, [spec["centre"]], BACKGROUND)] = 0.0
    crop = bbox(alpha > 0)
    scale = save(color, alpha, crop, spec["width"], OUT / "window-frame.webp")
    lines = [
        "/* The window frame's 9-slice, in the prepared image's pixels. Its",
        "   corner slices (ornaments included) and its stone band's thickness,",
        "   then: a frame drawn with stone T px thick uses border widths of",
        "   slice / stone x T. */",
        ":root {",
        f"  --window-art-slice: {round(spec['corner'] * scale)};",
        f"  --window-art-stone: {spec['stone'] * scale:.1f};",
        "}",
    ]
    write_css(ROOT / "web/src/styles/window-art.css", lines)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    unit_frame()
    cast_bar()
    window_frame()


if __name__ == "__main__":
    main()
