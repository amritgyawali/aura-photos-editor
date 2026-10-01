"""Turn JPEGs into the packed-RGB files the by-hand skin and retouch checks read. ADR-0077.

Writes NAME_WxH.rgb (packed 8-bit sRGB, long edge at most 1024) for every .jpg/.png in IN_DIR,
then, after the Rust checks have written their *.rgb outputs, `--png` converts every output in
OUT_DIR back to PNG for viewing.

    python ml/models/skin/to_raw.py photos/ raw/
    AURA_SKIN_PHOTOS=raw cargo test -p aura-vision --test skin_photos -- --ignored
    AURA_SKIN_PHOTOS=raw AURA_RETOUCH_PRESETS=natural,beauty \
        cargo test -p aura-app --test auto_retouch_photos -- --ignored --nocapture
    python ml/models/skin/to_raw.py --png raw/
"""
import argparse
import glob
import os

from PIL import Image


def to_raw(src, dst, edge=1024):
    os.makedirs(dst, exist_ok=True)
    for path in sorted(glob.glob(os.path.join(src, '*.jpg')) + glob.glob(os.path.join(src, '*.png'))):
        image = Image.open(path).convert('RGB')
        if max(image.size) > edge:
            scale = edge / max(image.size)
            image = image.resize((round(image.size[0] * scale), round(image.size[1] * scale)), Image.LANCZOS)
        name = os.path.splitext(os.path.basename(path))[0]
        with open(os.path.join(dst, f'{name}_{image.size[0]}x{image.size[1]}.rgb'), 'wb') as out:
            out.write(image.tobytes())


def to_png(folder):
    for path in sorted(glob.glob(os.path.join(folder, '*.*.rgb'))):
        base = os.path.basename(path)
        stem = base.split('.')[0]
        width, height = map(int, stem.rsplit('_', 1)[1].split('x'))
        with open(path, 'rb') as data:
            Image.frombytes('RGB', (width, height), data.read()).save(path[:-4] + '.png')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--png', action='store_true')
    parser.add_argument('folders', nargs='+')
    args = parser.parse_args()
    if args.png:
        for folder in args.folders:
            to_png(folder)
    else:
        to_raw(args.folders[0], args.folders[1])


if __name__ == '__main__':
    main()
