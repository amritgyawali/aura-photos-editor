"""Download before/after pairs from the MIT-Adobe FiveK dataset for profile fitting.

The "before" is the camera RAW (DNG) exactly as it came off the sensor; the "after" is a
professional retoucher's finished rendition of the same frame (16-bit ProPhoto TIFF).
AURA's own RAW decoder renders the DNG, so the parameters `tools/profile-fit` recovers are
parameters AURA's renderer honours - the fit copies the retoucher's settings in AURA's own
units rather than guessing at Lightroom's.

Only files listed in the dataset's `filesAdobeMIT.txt` are used. The dataset is under a
research licence (see https://data.csail.mit.edu/graphics/fivek/legal/); no image is
committed to this repository or bundled with the product - only the aggregate parameter
statistics measured from them, which are numbers, not copies of the images.

The TIFF is converted here, once, to an 8-bit sRGB PNG at a 1024 px long edge and then
deleted, so the working set stays small:

    ProPhoto RGB (gamma 1.8, D50) -> XYZ D50 -> Bradford -> XYZ D65 -> linear sRGB -> sRGB

Usage:
    python ml/profiles/fetch_fivek_pairs.py --out D:/aura-data/fivek --expert c --count 32
"""

from __future__ import annotations

import argparse
import pathlib
import sys
import urllib.parse
import urllib.request

import numpy as np
import tifffile
from PIL import Image

BASE = "https://data.csail.mit.edu/graphics/fivek"
LONG_EDGE = 1024

# ProPhoto RGB (ROMM) to XYZ D50, then Bradford D50 -> D65, then XYZ D65 -> linear sRGB.
PROPHOTO_TO_XYZ_D50 = np.array(
    [
        [0.7976749, 0.1351917, 0.0313534],
        [0.2880402, 0.7118741, 0.0000857],
        [0.0000000, 0.0000000, 0.8252100],
    ]
)
BRADFORD_D50_TO_D65 = np.array(
    [
        [0.9555766, -0.0230393, 0.0631636],
        [-0.0282895, 1.0099416, 0.0210077],
        [0.0122982, -0.0204830, 1.3299098],
    ]
)
XYZ_D65_TO_SRGB = np.array(
    [
        [3.2404542, -1.5371385, -0.4985314],
        [-0.9692660, 1.8760108, 0.0415560],
        [0.0556434, -0.2040259, 1.0572252],
    ]
)
PROPHOTO_TO_SRGB = XYZ_D65_TO_SRGB @ BRADFORD_D50_TO_D65 @ PROPHOTO_TO_XYZ_D50


def prophoto_to_srgb8(pixels: np.ndarray) -> np.ndarray:
    """A 16-bit ProPhoto array (H, W, 3) to an 8-bit sRGB array."""
    encoded = pixels.astype(np.float64) / 65535.0
    linear = np.where(encoded < 1.0 / 32.0, encoded / 16.0, encoded ** 1.8)
    srgb = np.clip(linear @ PROPHOTO_TO_SRGB.T, 0.0, 1.0)
    gamma = np.where(srgb <= 0.0031308, srgb * 12.92, 1.055 * srgb ** (1 / 2.4) - 0.055)
    return np.clip(np.round(gamma * 255.0), 0, 255).astype(np.uint8)


def fetch(url: str, path: pathlib.Path) -> None:
    if path.exists() and path.stat().st_size > 0:
        return
    partial = path.with_suffix(path.suffix + ".part")
    with urllib.request.urlopen(url, timeout=600) as response, open(partial, "wb") as out:
        while chunk := response.read(1 << 20):
            out.write(chunk)
    partial.replace(path)


def convert(tiff: pathlib.Path, png: pathlib.Path) -> None:
    pixels = tifffile.imread(tiff)
    if pixels.ndim != 3 or pixels.shape[2] < 3:
        raise ValueError(f"unexpected TIFF shape {pixels.shape}")
    pixels = pixels[:, :, :3]
    if pixels.dtype != np.uint16:
        pixels = (pixels.astype(np.float64) * (65535.0 / np.iinfo(pixels.dtype).max)).astype(np.uint16)
    image = Image.fromarray(prophoto_to_srgb8(pixels))
    scale = LONG_EDGE / max(image.size)
    if scale < 1.0:
        image = image.resize((round(image.width * scale), round(image.height * scale)), Image.LANCZOS)
    image.save(png)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", required=True, type=pathlib.Path)
    parser.add_argument("--expert", default="c", choices=list("abcde"))
    parser.add_argument("--count", default=32, type=int)
    parser.add_argument("--offset", default=0, type=int, help="shift the deterministic stride")
    args = parser.parse_args()

    args.out.mkdir(parents=True, exist_ok=True)
    listing = args.out / "filesAdobeMIT.txt"
    fetch(f"{BASE}/legal/filesAdobeMIT.txt", listing)
    names = [line.strip() for line in listing.read_text().splitlines() if line.strip()]
    # A deterministic stride across the whole list: the files are grouped by photographer
    # and by shoot, so the first N would all be one wedding.
    stride = max(1, len(names) // args.count)
    chosen = [names[(i * stride + args.offset) % len(names)] for i in range(args.count)]

    expert_dir = args.out / f"expert_{args.expert}"
    raw_dir = args.out / "dng"
    expert_dir.mkdir(exist_ok=True)
    raw_dir.mkdir(exist_ok=True)
    done = 0
    for index, name in enumerate(chosen, 1):
        png = expert_dir / f"{name}.png"
        dng = raw_dir / f"{name}.dng"
        try:
            fetch(f"{BASE}/img/dng/{urllib.parse.quote(name)}.dng", dng)
            if not png.exists():
                tiff = expert_dir / f"{name}.tif"
                fetch(f"{BASE}/img/tiff16_{args.expert}/{urllib.parse.quote(name)}.tif", tiff)
                convert(tiff, png)
                tiff.unlink()
            done += 1
            print(f"[{index}/{len(chosen)}] {name}", flush=True)
        except Exception as error:  # noqa: BLE001 - one bad file must not end the run
            print(f"[{index}/{len(chosen)}] {name} skipped: {error}", file=sys.stderr, flush=True)
    print(f"{done} pairs ready in {args.out}")
    return 0 if done else 1


if __name__ == "__main__":
    sys.exit(main())
