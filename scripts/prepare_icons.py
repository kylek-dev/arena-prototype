#!/usr/bin/env python3
"""Prepare HUD icons from generated masters.

Masters live in the gitignored ``assets/icons/source/``; the prepared WebP
files in ``assets/icons/spells/`` are tracked and imported by the web UI, so
the build never runs this script. Rerun it after replacing a master:

    python3 scripts/prepare_icons.py            # needs Pillow and NumPy

Each icon is an opaque square tile. The subject is cropped square around its
bright core and downscaled in linear light to 128 px (1x) and 256 px (2x) for
the 64 px action slot. The black ground is lifted to the palette's
``night-1`` ink (a screen blend, so highlights are untouched).
"""

from pathlib import Path
import sys

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "assets/icons/source"
OUT = ROOT / "assets/icons/spells"
SIZES = (128, 256)
NIGHT_1 = (0x0A, 0x11, 0x18)  # docs/palette.json "night-1"
CORE = 0.25  # linear luminance that counts as subject, not glow
MARGIN = 0.03  # fraction of the crop kept around the core on each side
QUALITY = 90

# Icon id -> master file in SOURCE.
ICONS = {
    "pulse": "spell-pulse.jpg",
    "rift-bolt": "spell-riftbolt.jpg",
    "quell": "spell-quell.jpg",
    "mend": "spell-mend.jpg",
}

# Hand-picked square crops (centre x, centre y, side in master pixels) where
# the automatic crop keeps too much: Mend's hands run off the frame and leave
# the healing orb small at slot size.
CROPS = {
    "mend": (1060, 1000, 1500),
}


def to_linear(srgb):
    return np.where(srgb <= 0.04045, srgb / 12.92, ((srgb + 0.055) / 1.055) ** 2.4)


def to_srgb(linear):
    linear = np.clip(linear, 0, 1)
    return np.where(linear <= 0.0031308, linear * 12.92, 1.055 * linear ** (1 / 2.4) - 0.055)


def square_crop(linear):
    """Smallest centred square that holds the bright core plus a margin."""
    h, w, _ = linear.shape
    lum = linear @ np.array([0.2126, 0.7152, 0.0722])
    ys, xs = np.nonzero(lum > CORE)
    if not len(xs):
        sys.exit("no subject found above the core threshold")
    cx, cy = (xs.min() + xs.max()) / 2, (ys.min() + ys.max()) / 2
    side = max(xs.max() - xs.min(), ys.max() - ys.min()) / (1 - 2 * MARGIN)
    side = min(side, w, h)
    x0 = int(round(min(max(cx - side / 2, 0), w - side)))
    y0 = int(round(min(max(cy - side / 2, 0), h - side)))
    s = int(round(side))
    return linear[y0 : y0 + s, x0 : x0 + s]


def resize_linear(linear, size):
    channels = [
        np.asarray(Image.fromarray(linear[..., c].astype(np.float32), "F").resize((size, size), Image.LANCZOS))
        for c in range(3)
    ]
    return np.stack(channels, axis=-1)


def prepare(icon, master):
    srgb = np.asarray(Image.open(SOURCE / master).convert("RGB"), dtype=np.float64) / 255
    if icon in CROPS:
        cx, cy, side = CROPS[icon]
        srgb = srgb[cy - side // 2 : cy + side // 2, cx - side // 2 : cx + side // 2]
        tile = to_linear(srgb)
    else:
        tile = square_crop(to_linear(srgb))
    ink = np.array(NIGHT_1) / 255
    for size in SIZES:
        out = to_srgb(resize_linear(tile, size))
        out = 1 - (1 - out) * (1 - ink)  # lift black to night-1, keep highlights
        path = OUT / f"{icon}-{size}.webp"
        Image.fromarray(np.round(out * 255).astype(np.uint8), "RGB").save(
            path, "WEBP", quality=QUALITY, method=6
        )
        print(f"{path.relative_to(ROOT)}: {path.stat().st_size:,} bytes (crop {tile.shape[0]} px)")


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for icon, master in ICONS.items():
        prepare(icon, master)


if __name__ == "__main__":
    main()
