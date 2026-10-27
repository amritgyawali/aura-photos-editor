"""Download attributed, distinct Pexels photographs for AURA's colour-match review.

No photo is synthesised, graded, cropped, or reused as another target. Denied URLs
are recorded without retries or alternate access paths. Requires Pillow and numpy.
"""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import math
import struct
from datetime import datetime, timezone
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

import numpy as np
from PIL import Image, ImageDraw, ImageFont, ImageOps

PHOTOS = [
    ("reference", 0, 34923319, "Joelle Khaled", "Romantic wedding couple"),
    ("target", 1, 19403517, "terence b", "Outdoor woman"),
    ("target", 2, 30267960, "César O'neill", "Woman in nature"),
    ("target", 3, 36646537, "abubakar mamman", "Woman in a garden"),
    ("target", 4, 32747251, "Zulfugar Karimov", "Outdoor woman"),
    ("target", 5, 33271173, "Anil Sharma", "Indoor man"),
    ("target", 6, 31705625, "Wolrider YURTSEVEN", "Warm indoor man"),
    ("target", 7, 30482418, "claytons gallary", "Formal man"),
    ("target", 8, 20479992, "Alexander Mass", "Wedding ceremony"),
    ("target", 9, 23531520, "Willian Justen de Vasconcellos", "Mountain lake"),
    ("target", 10, 12655016, "Erik Mclean", "Night city"),
]


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def rgb_open(path: Path) -> Image.Image:
    with Image.open(path) as image:
        return ImageOps.exif_transpose(image).convert("RGB")


def difference_hash(image: Image.Image) -> str:
    gray = np.asarray(image.convert("L").resize((9, 8), Image.Resampling.LANCZOS))
    bits = gray[:, 1:] > gray[:, :-1]
    return f"{int(''.join('1' if value else '0' for value in bits.ravel()), 2):016x}"


def perceptual_hash(image: Image.Image) -> str:
    gray = np.asarray(image.convert("L").resize((32, 32), Image.Resampling.LANCZOS), dtype=np.float64)
    positions = np.arange(32, dtype=np.float64)
    frequencies = np.arange(8, dtype=np.float64)[:, None]
    cosine = np.cos(math.pi / 32 * (positions + 0.5) * frequencies)
    dct = cosine @ gray @ cosine.T
    coefficients = dct.ravel()
    bits = coefficients > np.median(coefficients[1:])
    bits[0] = False  # Exclude mean luminance from the duplicate comparison.
    return f"{int(''.join('1' if value else '0' for value in bits), 2):016x}"


def details(path: Path, root: Path) -> dict:
    image = rgb_open(path)
    pixels = struct.pack(">II", image.width, image.height) + image.tobytes()
    return {
        "path": path.relative_to(root).as_posix(),
        "width": image.width,
        "height": image.height,
        "sha256": digest(path.read_bytes()),
        "pixelSha256": digest(pixels),
        "dHash": difference_hash(image),
        "pHash": perceptual_hash(image),
    }


def hamming(left: str, right: str) -> int:
    return (int(left, 16) ^ int(right, 16)).bit_count()


def gray_grid(path: Path) -> np.ndarray:
    return np.asarray(rgb_open(path).convert("L").resize((128, 128), Image.Resampling.LANCZOS), dtype=np.float64) / 255


def block_ssim(left: np.ndarray, right: np.ndarray) -> float:
    # Mean SSIM over aligned non-overlapping 8x8 windows; a duplicate screen,
    # not a colour-match quality score or a judgement of unrelated compositions.
    a = left.reshape(16, 8, 16, 8).transpose(0, 2, 1, 3).reshape(-1, 64)
    b = right.reshape(16, 8, 16, 8).transpose(0, 2, 1, 3).reshape(-1, 64)
    mean_a, mean_b = a.mean(axis=1), b.mean(axis=1)
    var_a, var_b = a.var(axis=1), b.var(axis=1)
    covariance = ((a - mean_a[:, None]) * (b - mean_b[:, None])).mean(axis=1)
    score = ((2 * mean_a * mean_b + 0.01**2) * (2 * covariance + 0.03**2)) / (
        (mean_a**2 + mean_b**2 + 0.01**2) * (var_a + var_b + 0.03**2))
    return float(score.mean())


def contact_sheet(images: list[dict], root: Path) -> None:
    cell_width, cell_height, columns = 340, 330, 4
    rows = math.ceil(len(images) / columns)
    sheet = Image.new("RGB", (columns * cell_width, 52 + rows * cell_height), "#11151b")
    draw = ImageDraw.Draw(sheet)
    font = ImageFont.truetype("C:/Windows/Fonts/arial.ttf", 16)
    small = ImageFont.truetype("C:/Windows/Fonts/arial.ttf", 13)
    draw.text((16, 14), "AURA — 1 reference + 10 independently photographed targets (Pexels)", font=font, fill="white")
    for position, item in enumerate(images):
        x, y = (position % columns) * cell_width, 52 + (position // columns) * cell_height
        image = rgb_open(root / item["input"]["path"])
        image.thumbnail((cell_width - 16, 245), Image.Resampling.LANCZOS)
        sheet.paste(image, (x + (cell_width - image.width) // 2, y + (245 - image.height) // 2))
        heading = "REFERENCE" if item["role"] == "reference" else f"TARGET {item['index']:02}"
        draw.text((x + 8, y + 250), f"{heading} · Pexels {item['photoId']}", font=font, fill="#ffd990")
        draw.text((x + 8, y + 274), item["description"], font=small, fill="white")
        draw.text((x + 8, y + 294), f"Photo: {item['author']}", font=small, fill="#ccd4df")
    sheet.save(root / "contact-sheet.jpg", quality=95, subsampling=0)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path("output/aura-unique-color-match-2026-10-24/dataset"))
    args = parser.parse_args()
    root = args.output.resolve()
    (root / "sources").mkdir(parents=True, exist_ok=True)
    (root / "targets").mkdir(exist_ok=True)
    images, failures = [], []
    for role, index, photo_id, author, description in PHOTOS:
        download_url = f"https://images.pexels.com/photos/{photo_id}/pexels-photo-{photo_id}.jpeg?auto=compress&cs=tinysrgb&w=1280&h=1280&fit=max"
        source = root / "sources" / f"{role}-{photo_id}.jpg"
        if not source.exists():
            request = Request(download_url, headers={"User-Agent": "AURA-photo-review/1.0"})
            try:
                with urlopen(request, timeout=60) as response:
                    data = response.read(20 * 1024 * 1024 + 1)
                if len(data) > 20 * 1024 * 1024:
                    raise ValueError("Download exceeded the 20 MB bound")
                with Image.open(io.BytesIO(data)) as image:
                    image.verify()
                source.write_bytes(data)
            except (HTTPError, URLError, ValueError, OSError) as cause:
                failure = {"role": role, "photoId": str(photo_id), "url": download_url, "error": str(cause), "retried": False}
                failures.append(failure)
                print(json.dumps({"downloadRefused": failure}), flush=True)
                continue
        image = rgb_open(source)
        if max(image.size) > 1280:
            # Preserve the downloaded bytes; report an unexpected CDN size instead of
            # disguising a larger download as the requested bounded source.
            failures.append({"role": role, "photoId": str(photo_id), "url": download_url, "error": f"CDN size exceeds1280: {image.size}", "retried": False})
            continue
        image.thumbnail((960, 960), Image.Resampling.LANCZOS)
        target = root / ("reference.jpg" if role == "reference" else f"targets/{index:02}-{photo_id}.jpg")
        image.save(target, quality=95, subsampling=0)
        item = {
            "role": role, "index": index, "photoId": str(photo_id), "author": author, "description": description,
            "sourcePage": f"https://www.pexels.com/photo/{photo_id}/", "downloadUrl": download_url,
            "source": details(source, root), "input": details(target, root),
        }
        images.append(item)
        print(json.dumps({"downloaded": item["photoId"], "role": role, "input": item["input"]}), flush=True)

    pairwise = []
    grids = {item["photoId"]: gray_grid(root / item["input"]["path"]) for item in images}
    for first_index, first in enumerate(images):
        for second in images[first_index + 1:]:
            a, b = first["input"], second["input"]
            dh, ph = hamming(a["dHash"], b["dHash"]), hamming(a["pHash"], b["pHash"])
            ga, gb = grids[first["photoId"]], grids[second["photoId"]]
            ssim = block_ssim(ga, gb)
            pairwise.append({"firstPhotoId": first["photoId"], "secondPhotoId": second["photoId"],
                "sameFile": a["sha256"] == b["sha256"], "sameNormalizedPixels": a["pixelSha256"] == b["pixelSha256"],
                "dHashDistance": dh, "pHashDistance": ph, "gray128Rmse": round(float(np.sqrt(np.mean((ga - gb)**2))), 6),
                "gray128BlockSsim": round(ssim, 6), "needsDuplicateReview": dh <= 8 or ph <= 8 or ssim >= 0.9})
    manifest = {
        "schemaVersion": 1, "createdUtc": datetime.now(timezone.utc).isoformat(),
        "purpose": "One reference plus ten genuinely separate source photographs. No paired, synthetic, graded or duplicated targets.",
        "licence": {"provider": "Pexels", "url": "https://www.pexels.com/license/", "attribution": "Individual photographer and source page recorded for each photo."},
        "normalization": {"download": "Official Pexels CDN, sRGB, fit=max, width1280 height1280; downloaded bytes preserved.",
            "input": "EXIF orientation applied; RGB; aspect preserved; long edge at most960; JPEG quality95, no chroma subsampling.",
            "pixelSha256": "SHA256 of big-endian uint32 width,height followed by oriented decoded RGB8 pixel bytes.",
            "dHash": "64-bit horizontal9x8 grayscale difference hash, Lanczos resize.",
            "pHash": "64-bit low-frequency8x8 DCT of32x32 grayscale; median excluding DC; DC bit cleared.",
            "ssim": "Mean SSIM of nonoverlapping8x8 grayscale blocks after128x128 Lanczos resize, C1=.01²,C2=.03²; duplicate screening only."},
        "expectedImages": 11, "downloadedImages": len(images), "images": images,
        "duplicateChecks": {"pairCount": len(pairwise), "exactDuplicates": sum(pair["sameFile"] or pair["sameNormalizedPixels"] for pair in pairwise),
            "flaggedPairs": [pair for pair in pairwise if pair["needsDuplicateReview"]], "pairwise": pairwise,
            "visualInspection": "Pending human/assistant inspection of contact-sheet.jpg; hashes do not prove semantic distinctness."},
        "rejectedDownloads": failures,
    }
    (root / "manifest.json").write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    if images:
        contact_sheet(images, root)
    lines = ["# Attributed Pexels dataset", "", "Licence: https://www.pexels.com/license/", "", "No images are before/after pairs; each role has a separate photo ID.", ""]
    for item in images:
        label = "Reference" if item["role"] == "reference" else f"Target {item['index']:02}"
        lines.append(f"- {label}: {item['description']} — {item['author']}, Pexels {item['photoId']}: {item['sourcePage']}")
    (root / "ATTRIBUTION.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(json.dumps({"manifest": str(root / "manifest.json"), "images": len(images), "failures": len(failures), "duplicateFlags": len(manifest["duplicateChecks"]["flaggedPairs"])}), flush=True)
    return 0 if len(images) == 11 and not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())
