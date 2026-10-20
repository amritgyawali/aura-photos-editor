"""Create review-only crops/maps from the native RAW integration test's PPMs.

Requires Pillow and NumPy. Never changes source photographs or native renders.
Usage: python scripts/review-polished-wedding.py EVIDENCE_FOLDER
"""
from pathlib import Path
from html import escape
import json
import sys
import numpy as np
from PIL import Image, ImageDraw, ImageFont

root = Path(sys.argv[1]).resolve()
results = json.loads((root / 'results.json').read_text(encoding='utf-8'))
font = ImageFont.truetype('C:/Windows/Fonts/arial.ttf', 18)
cards = []
for row in results:
    name = row['photo']
    recipe = json.loads((root / f'{name}-recipe.json').read_text(encoding='utf-8'))
    report = recipe['studio_portrait_auto_v1']
    with Image.open(root / f'{name}-before.ppm') as original, Image.open(root / f'{name}-after.ppm') as revised:
        a = np.asarray(original).astype(np.int16)
        b = np.asarray(revised).astype(np.int16)
        changed = np.max(np.abs(a-b), axis=2) >= 2
        overlay = b.astype(np.float32)
        overlay[changed] = overlay[changed]*0.4 + np.array([20, 225, 180])*0.6
        overlay = Image.fromarray(overlay.astype(np.uint8))
        overlay.thumbnail((1600, 1600))
        overlay.save(root / f'{name}-changes.jpg', quality=96)
        full = revised.copy()
        full.thumbnail((1600, 1600))
        full.save(root / f'{name}-review.jpg', quality=97)
        faces = []
        for index, face in enumerate(report['faces'], 1):
            left, top, right, bottom = face['bounds']
            pad = .025
            box = (max(0, int((left-pad)*original.width)), max(0, int((top-pad)*original.height)),
                   min(original.width, int((right+pad)*original.width)), min(original.height, int((bottom+pad)*original.height)))
            before = original.crop(box)
            after = revised.crop(box)
            before.thumbnail((600, 780))
            after.thumbnail((600, 780))
            pair = Image.new('RGB', (before.width*2, before.height+32), (24, 24, 24))
            pair.paste(before, (0, 32))
            pair.paste(after, (before.width, 32))
            ImageDraw.Draw(pair).text((6, 5), f'{name} F{index:02} | Before skin retouch / Polished wedding', font=font, fill='white')
            filename = f'{name}-face-{index:02}.jpg'
            pair.save(root / filename, quality=98)
            faces.append(f'<figure><figcaption>Detected face {index}</figcaption><img loading="lazy" src="{filename}"></figure>')
        cards.append(f'''<section><h2>{escape(name)}</h2><p>{row['dimensions'][0]} × {row['dimensions'][1]} native RAW render · {row['faces']} detected faces · {row['operations']} editable operations.</p>
<div class="overview"><img src="{name}-review.jpg"><img src="{name}-changes.jpg"></div>
<p>Teal marks actual pixel changes of at least 2 levels out of 255. Face and body corrections are included. Detector targets can include printed portraits; inspect them separately.</p>
<a href="{name}-recipe.json">Saved native recipe and decisions</a><div class="faces">{''.join(faces)}</div></section>''')
html = '''<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>AURA polished wedding validation</title>
<style>body{margin:32px auto;padding:0 24px;max-width:1500px;background:#141816;color:#eef3ef;font:16px/1.55 system-ui}a{color:#93e6be}section{margin:48px 0;border-top:1px solid #526258}img{max-width:100%;height:auto}figure{margin:24px 0}.overview{display:grid;grid-template-columns:1fr 1fr;gap:20px}.overview img{max-height:800px;object-fit:contain}p{color:#c6d0ca}@media(max-width:700px){.overview{grid-template-columns:1fr}}</style>
<h1>Polished wedding · native RAW validation</h1><p>Matching-frame before/after skin retouch comparisons. Lighting and crop settings are held constant for these checks. All source hashes are unchanged. These are test renders in isolated catalogs; existing delivered photos were not re-edited.</p>
<p>Review the bindi, lashes, nose, jaw/neck transitions, pore texture, hands and jewellery. No paired Evoto exports were used, so these images do not demonstrate an exact Evoto match.</p>'''+''.join(cards)
(root/'Review.html').write_text(html, encoding='utf-8')
print(f'Prepared native review: {len(results)} photos, {sum(x["faces"] for x in results)} detected face crops.')
