"""Build the photo set used to qualify one-click automatic editing (ADR-0076).

Downloads public Pexels photographs, copies the earlier capability-audit inputs when
available, and derives controlled variants (colour casts, exposure errors, noise, haze,
monochrome) from them. Originals are never modified. Output: .work-checks/auto-v3/inputs
and a manifest recording where every file came from and what it is meant to exercise.

Usage: python scripts/prepare-auto-edit-fixtures.py [--audit-inputs PATH]
"""
import argparse
import hashlib
import json
import random
import shutil
import urllib.request
from pathlib import Path

from PIL import Image, ImageEnhance, ImageFilter

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.work-checks/auto-v3/inputs'
PEXELS = {
    '1036623': ('portrait-pink-backdrop.jpg', 'studio portrait, coloured backdrop'),
    '1181686': ('portrait-smile-teeth.jpg', 'smiling portrait with visible teeth'),
    '1308881': ('portrait-outdoor-flowers.jpg', 'outdoor portrait, foliage'),
    '1640777': ('food-flatlay.jpg', 'food flat lay, bright'),
    '1845534': ('portrait-glasses.jpg', 'portrait with eyeglasses'),
    '2253275': ('pet-dog-indoor.jpg', 'animal, indoor'),
    '3184405': ('group-office.jpg', 'group of people, indoor'),
    '415829': ('portrait-yellow-backdrop.jpg', 'studio portrait, saturated backdrop'),
    '5325104': ('two-people-meeting.jpg', 'two people, indoor, window light'),
    '842711': ('landscape-sunset-hills.jpg', 'landscape at golden hour'),
}


def fetch(photo_id, name):
    target = OUT / name
    if target.exists():
        return target
    url = f'https://images.pexels.com/photos/{photo_id}/pexels-photo-{photo_id}.jpeg?auto=compress&w=1200'
    with urllib.request.urlopen(url, timeout=60) as response:
        target.write_bytes(response.read())
    return target


def derive(source, name, transform, purpose, manifest):
    with Image.open(source) as image:
        out = transform(image.convert('RGB'))
        out.save(OUT / name, quality=92)
    manifest.append({'file': name, 'kind': 'derived', 'from': source.name, 'purpose': purpose})


def cast(r, g, b):
    def apply(image):
        channels = image.split()
        return Image.merge('RGB', [c.point(lambda v, k=k: min(255, int(v * k))) for c, k in zip(channels, (r, g, b))])
    return apply


def exposure(gain):
    return lambda image: image.point(lambda v: min(255, int(255 * ((v / 255) ** 2.2 * gain) ** (1 / 2.2))))


def noisy(image):
    dark = exposure(0.25)(image)
    rng = random.Random(7)
    pixels = dark.load()
    for y in range(dark.height):
        for x in range(dark.width):
            n = int(rng.gauss(0, 7))
            r, g, b = pixels[x, y]
            pixels[x, y] = tuple(max(0, min(255, v + n + int(rng.gauss(0, 3)))) for v in (r, g, b))
    return dark


def hazy(image):
    veil = Image.new('RGB', image.size, (196, 200, 208))
    return Image.blend(image, veil, 0.42)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--audit-inputs', type=Path)
    args = parser.parse_args()
    OUT.mkdir(parents=True, exist_ok=True)
    manifest = []
    for photo_id, (name, purpose) in PEXELS.items():
        path = fetch(photo_id, name)
        manifest.append({'file': name, 'kind': 'real photograph', 'source': f'https://www.pexels.com/photo/{photo_id}/', 'purpose': purpose})
    if args.audit_inputs and args.audit_inputs.exists():
        for path in sorted(args.audit_inputs.iterdir()):
            if path.is_file():
                shutil.copy2(path, OUT / path.name)
                manifest.append({'file': path.name, 'kind': 'earlier audit input', 'purpose': 'format or content coverage'})
    base = OUT / 'portrait-smile-teeth.jpg'
    derive(base, 'variant-tungsten-cast.jpg', cast(1.12, 0.96, 0.7), 'strong warm (tungsten) cast on a portrait', manifest)
    derive(OUT / 'landscape-sunset-hills.jpg', 'variant-blue-cast.jpg', cast(0.78, 0.95, 1.18), 'cool cast on a landscape', manifest)
    derive(OUT / 'group-office.jpg', 'variant-underexposed.jpg', exposure(0.3), 'about -1.7 EV underexposure, group', manifest)
    derive(OUT / 'food-flatlay.jpg', 'variant-overexposed.jpg', exposure(2.0), 'about +1 EV overexposure with clipping', manifest)
    derive(OUT / 'pet-dog-indoor.jpg', 'variant-noisy-lowlight.jpg', noisy, 'dark, high-noise frame', manifest)
    derive(OUT / 'landscape-sunset-hills.jpg', 'variant-hazy.jpg', hazy, 'haze veil over a landscape', manifest)
    derive(OUT / 'portrait-glasses.jpg', 'variant-monochrome.jpg', lambda i: i.convert('L').convert('RGB'), 'black-and-white portrait', manifest)
    derive(OUT / 'portrait-yellow-backdrop.jpg', 'variant-soft.jpg', lambda i: i.filter(ImageFilter.GaussianBlur(1.6)), 'soft / slightly out-of-focus portrait', manifest)
    derive(OUT / 'portrait-outdoor-flowers.jpg', 'variant-flat.jpg', lambda i: ImageEnhance.Contrast(i).enhance(0.55), 'flat, low-contrast', manifest)
    for entry in manifest:
        entry['sha256'] = hashlib.sha256((OUT / entry['file']).read_bytes()).hexdigest()
    (OUT.parent / 'input-manifest.json').write_text(json.dumps(manifest, indent=2), encoding='utf-8')
    print(len(manifest), 'files in', OUT)


if __name__ == '__main__':
    main()
