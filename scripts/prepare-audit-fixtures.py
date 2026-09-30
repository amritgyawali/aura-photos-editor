"""Create an isolated audit corpus. Originals are copied, never overwritten.

Pillow transformations below create explicitly labelled decoder/stress fixtures;
the actual editing under test is performed by AURA, not by Pillow.
"""
import hashlib
import json
import shutil
import subprocess
from pathlib import Path

from PIL import Image, ImageEnhance

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.work-checks/full-audit'
INPUT = OUT / 'inputs'
INPUT.mkdir(parents=True, exist_ok=True)
manifest = []


def record(path, kind, source):
    manifest.append(dict(file=path.name, kind=kind, source=source,
                         sha256=hashlib.sha256(path.read_bytes()).hexdigest()))


for photo in sorted((ROOT / '.work-checks/portrait-review/originals').glob('*.jpg')):
    target = INPUT / photo.name
    shutil.copy2(photo, target)
    record(target, 'real portrait', 'https://www.pexels.com/photo/' + photo.stem.split('-')[-1] + '/')

for name, ident in [('landscape', 23531520), ('cat-veterinary', 6816859), ('product-sneaker', 1161538)]:
    target = INPUT / (name + '.jpg')
    subprocess.run(['curl.exe', '-sSL', '--fail', '-A', 'Mozilla/5.0',
                    f'https://images.pexels.com/photos/{ident}/pexels-photo-{ident}.jpeg?w=1200',
                    '-o', str(target)], check=True)
    with Image.open(target) as check:
        check.verify()
    record(target, 'real photograph', f'https://www.pexels.com/photo/{ident}/')

with Image.open(INPUT / 'portrait-1239291.jpg') as original:
    photo = original.convert('RGB')
    photo.thumbnail((500, 500))
    variants = [('test-opaque.png', photo), ('test-rgb.tif', photo),
                ('test-gray.jpg', photo.convert('L')), ('test-cmyk.jpg', photo.convert('CMYK')),
                ('test-webp.webp', photo), ('test-bmp.bmp', photo), ('test-gif.gif', photo),
                ('test-dark.jpg', ImageEnhance.Brightness(photo).enhance(.2)),
                ('test-bright.jpg', ImageEnhance.Brightness(photo).enhance(2.5))]
    rgba = photo.convert('RGBA')
    rgba.putalpha(80)
    variants.append(('test-alpha.png', rgba))
    variants.append(('test-gray16.tif', photo.convert('L').point(lambda v: v * 257, 'I').convert('I;16')))
    variants.extend([('test-black.png', Image.new('RGB', (320, 240))), ('test-one-pixel.png', Image.new('RGB', (1, 1), 'white'))])
    for name, pixels in variants:
        target = INPUT / name
        pixels.save(target)
        record(target, 'synthetic format/stress fixture', 'Derived from portrait-1239291.jpg or generated flat field')
    target = INPUT / 'test-exif6.jpg'
    exif = Image.Exif()
    exif[274] = 6
    photo.save(target, exif=exif)
    record(target, 'EXIF orientation fixture', 'Portrait pixels plus orientation=6')
    target = INPUT / 'test-corrupt.jpg'
    target.write_bytes(b'\xff\xd8not a valid jpeg')
    record(target, 'deliberately corrupt fixture', 'Generated invalid JPEG')
    left = Image.open(INPUT / 'portrait-220453.jpg').convert('RGB')
    left.thumbnail((500, 500))
    group = Image.new('RGB', (photo.width + left.width, 500), '#808080')
    group.paste(photo, (0, 0))
    group.paste(left, (photo.width, 0))
    target = INPUT / 'test-two-portraits.jpg'
    group.save(target, quality=95)
    record(target, 'composed multi-face fixture, not a real group photo', 'Two supplied Pexels portrait photographs')

(OUT / 'input-manifest.json').write_text(json.dumps(manifest, indent=2), encoding='utf-8')
print(f'Prepared {len(manifest)} audit inputs in {INPUT}')
