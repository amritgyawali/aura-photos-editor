"""Make native-scale QA crops from AURA's before/after test renders; never edit deliveries."""
import argparse
import json
from html import escape
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw


def regional_table(path):
    """Optional measured checks; absence stays explicit rather than implying a pass."""
    if not path.exists():
        return '<p>Local contour measurements are not available for this render.</p>'
    data = json.loads(path.read_text(encoding='utf-8'))
    rows = []
    for scale in ('analysis', 'native'):
        dimensions = data.get(f'{scale}Dimensions', [])
        for face in data.get(scale, []):
            for region in face.get('regions', []):
                status = ('Not measured' if not region.get('measured') else
                          'Review required' if region.get('review') else 'Within pilot limits')
                values = [scale, ' x '.join(map(str, dimensions)), face['face'] + 1,
                          region['region'], status, region['newSeamPairs'],
                          f"{region['weakenedEdgePairs']} / {region['sourceEdgePairs']}"]
                rows.append('<tr>' + ''.join(f'<td>{escape(str(v))}</td>' for v in values) + '</tr>')
    return ('<details><summary>Local contour readings at analysis and native resolution</summary>'
            '<p>Approximate landmark regions; neither skin masks nor a photographic-quality score.</p>'
            '<table><thead><tr><th>Scale</th><th>Dimensions</th><th>Face</th><th>Region</th>'
            '<th>Result</th><th>New seams</th><th>Weakened / source edges</th></tr></thead>'
            '<tbody>' + ''.join(rows) + '</tbody></table></details>')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    parser.add_argument('--previous', type=Path)
    args = parser.parse_args()
    root = args.root.resolve()
    records = json.loads((root / 'results.json').read_text(encoding='utf-8'))
    cards, metrics = [], []
    for record in records:
        name = record['photo']
        cards.append(f'<h2>{escape(name)}</h2>' + regional_table(root / f'{name}-regional-quality.json'))
        recipe = json.loads((root / f'{name}-recipe.json').read_text(encoding='utf-8'))
        with Image.open(root / f'{name}-before.ppm') as original, Image.open(root / f'{name}-after.ppm') as final:
            assert original.size == final.size
            previous_path = args.previous / f'{name}-after.ppm' if args.previous else None
            previous = Image.open(previous_path) if previous_path and previous_path.exists() else None
            try:
                for face_index, face in enumerate(recipe['studio_portrait_auto_v1']['faces']):
                    w, h = final.size
                    l, t, r, b = face['bounds']
                    points = face['landmarks']
                    eyes = [(points[0][0] * w, points[0][1] * h), (points[1][0] * w, points[1][1] * h)]
                    regions = {
                        'left-eye': eyes[0], 'right-eye': eyes[1],
                        'nose': (points[2][0] * w, points[2][1] * h),
                        'chin-neck': ((l + r) * w / 2, (b - 0.025 * (b - t)) * h),
                        'forehead': ((l + r) * w / 2, (t + 0.2 * (b - t)) * h),
                    }
                    for region, (cx, cy) in regions.items():
                        size = min(512, w, h)
                        x = max(0, min(w - size, round(cx - size / 2)))
                        y = max(0, min(h - size, round(cy - size / 2)))
                        box = (x, y, x + size, y + size)
                        panels = [('Baseline', original), ('Current', final)]
                        if previous:
                            assert previous.size == final.size
                            panels.insert(1, ('Previous AURA', previous))
                        canvas = Image.new('RGB', (size * len(panels), size + 28), '#202020')
                        draw = ImageDraw.Draw(canvas)
                        for index, (label, image) in enumerate(panels):
                            canvas.paste(image.crop(box), (index * size, 28))
                            draw.text((index * size + 8, 7), label, fill='white')
                        filename = f'{name}-f{face_index}-{region}-native.png'
                        canvas.save(root / filename)
                        a = np.asarray(original.crop(box)).astype(np.int16)
                        z = np.asarray(final.crop(box)).astype(np.int16)
                        delta = np.abs(z - a)
                        item = {'photo': name, 'face': face_index, 'region': region,
                                'box': box, 'changedPixels2': int((delta.max(axis=2) >= 2).sum()),
                                'meanAbsoluteByteChange': float(delta.mean()),
                                'maximumByteChange': int(delta.max())}
                        if previous:
                            old = np.asarray(previous.crop(box)).astype(np.int16)
                            item['changeFromPreviousMean'] = float(np.abs(z - old).mean())
                        metrics.append(item)
                        cards.append(f'<figure><figcaption>{escape(name)} / {region} / native pixels</figcaption><img src="{filename}" loading="lazy"></figure>')
            finally:
                if previous:
                    previous.close()
    (root / 'boundary-crops.json').write_text(json.dumps(metrics, indent=2), encoding='utf-8')
    (root / 'BoundaryReview.html').write_text('''<!doctype html><meta charset="utf-8"><title>AURA boundary review</title>
<style>body{background:#181818;color:#eee;font:16px system-ui;margin:24px}figure{margin:32px 0;overflow:auto}img{max-width:none}figcaption{margin:10px 0}td,th{padding:8px;text-align:left;border-bottom:1px solid #555}details{overflow:auto}</style>
<h1>Native-scale portrait boundary review</h1><p>Read-only QA crops from AURA test renders. Baseline retains the same global colour.
Previous AURA is an earlier AURA render, not an Evoto reference. Pixel-change measurements do not prove artistic quality.</p>'''+''.join(cards), encoding='utf-8')
    print(json.dumps({'photos': len(records), 'nativeCrops': len(metrics), 'review': str(root / 'BoundaryReview.html')}))


if __name__ == '__main__':
    main()
