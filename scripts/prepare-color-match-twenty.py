"""Download 20 additional targets and six distinct reference sources for AURA QA.

Photographs remain attributed and ungraded here. The native assessment creates
declared reference edits separately. Requires Pillow and numpy.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
import importlib.util
import io
import json
import math
from pathlib import Path
from urllib.request import Request, urlopen

import numpy as np
from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('dataset_helpers', ROOT / 'scripts/prepare-unique-color-match-dataset.py')
helpers = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helpers)

TARGETS = [
    (33380315, 'quang vinh', 'Woman on beach at night'),
    (16646551, 'HONG SON', 'Illuminated coastal city'),
    (18688314, 'Anastasiya Badun', 'Rustic cafe and red flowers'),
    (15069828, 'evgenii', 'Man on beach at night'),
    (30154480, 'mitbg000', 'Urban beach at dusk'),
    (29209958, 'Sergei Starostin', 'Red flowers in snow'),
    (11008002, 'Aleksey Kuprikov', 'Mossy forest waterfall'),
    (3455934, 'Oliver Halls', 'Autumn forest waterfall'),
    (26651573, 'Imthiyaz Syed', 'Desert dunes and blue sky'),
    (37164686, 'Aysegul Aytoren', 'Senior man outdoors'),
    (7808457, 'Syed Qaarif Andrabi', 'Bearded man in dramatic shadows'),
    (7322073, 'Alena Darmel', 'Thoughtful grey-haired senior'),
    (17000433, 'Jhosmayra Palomino', 'Monochrome senior portrait'),
    (37461742, 'Gizem Gokce', 'Bright fruit bowl'),
    (20699589, 'Daniel Duarte', 'Mixed fruit bowl in dark cafe'),
    (36701322, 'Becca Correia', 'Woman in white corset in studio'),
    (32972578, 'Dicky Agustian', 'Dim studio fashion portrait'),
    (20788937, 'Phuc Lai', 'Dramatic red studio portrait'),
    (20756240, 'Emiliano Vittoriosi', 'Studio portrait with natural hair'),
    (31714366, 'Jonathan Goncalves', 'Woman in pastel pink dress'),
]
REFERENCES = [
    (13822354, 'Jonathan', 'Senior man with flat cap'),
    (12779583, 'Lukas Feldmann', 'Green forest stream'),
    (1652312, 'Jane T D.', 'Assorted fruit bowls'),
    (33202132, 'Gabriela Brasiliano', 'Smiling woman in white top'),
    (34704802, 'syd.trgt', 'Cafe with striped awning'),
    (23891642, 'Andrea Musto', 'Brunette studio portrait'),
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT / 'output/aura-twenty-color-match-2026-10-24/dataset')
    args = parser.parse_args()
    root = args.output.resolve()
    for folder in ('sources', 'targets', 'references'):
        (root / folder).mkdir(parents=True, exist_ok=True)

    def download(entry):
        role, index, (photo_id, author, description) = entry
        url = f'https://images.pexels.com/photos/{photo_id}/pexels-photo-{photo_id}.jpeg?auto=compress&cs=tinysrgb&w=1280&h=1280&fit=max'
        source = root / 'sources' / f'{photo_id}.jpg'
        try:
            if not source.exists():
                with urlopen(Request(url, headers={'User-Agent': 'AURA-photo-review/1.0'}), timeout=60) as response:
                    data = response.read(20 * 1024 * 1024 + 1)
                if len(data) > 20 * 1024 * 1024:
                    raise ValueError('Image exceeded download bound')
                with Image.open(io.BytesIO(data)) as photo:
                    photo.verify()
                source.write_bytes(data)
            image = helpers.rgb_open(source)
            if max(image.size) > 1280:
                raise ValueError(f'Unexpected CDN dimensions: {image.size}')
            image.thumbnail((960, 960), Image.Resampling.LANCZOS)
            target = root / ('targets' if role == 'target' else 'references') / f'{index:02}-{photo_id}.jpg'
            image.save(target, quality=95, subsampling=0)
            item = dict(role=role, index=index, photoId=str(photo_id), author=author, description=description,
                        sourcePage=f'https://www.pexels.com/photo/{photo_id}/', downloadUrl=url,
                        source=helpers.details(source, root), input=helpers.details(target, root))
            print(json.dumps({'downloaded': photo_id, 'role': role}), flush=True)
            return item, None
        except Exception as error:
            failure = dict(photoId=str(photo_id), url=url, error=str(error), retried=False)
            print(json.dumps({'downloadFailed': failure}), flush=True)
            return None, failure

    entries = [(role, i + 1, photo) for role, photos in [('target', TARGETS), ('reference', REFERENCES)] for i, photo in enumerate(photos)]
    with ThreadPoolExecutor(max_workers=4) as pool:
        downloaded = list(pool.map(download, entries))
    images = [item for item, _ in downloaded if item]
    failures = [error for _, error in downloaded if error]
    old_root = ROOT / 'output/aura-unique-color-match-2026-10-24/dataset'
    old_manifest = json.loads((old_root / 'manifest.json').read_text(encoding='utf-8'))
    assert not {x['photoId'] for x in images} & {x['photoId'] for x in old_manifest['images']}
    candidates = [(root, x, False) for x in images] + [(old_root, x, True) for x in old_manifest['images']]
    grids = {item['photoId']: helpers.gray_grid(base / item['input']['path']) for base, item, _ in candidates}
    pairs = []
    for i, (_, first, old_a) in enumerate(candidates):
        for _, second, old_b in candidates[i + 1:]:
            if old_a and old_b:
                continue
            a, b = first['input'], second['input']
            dh, ph = helpers.hamming(a['dHash'], b['dHash']), helpers.hamming(a['pHash'], b['pHash'])
            ssim = helpers.block_ssim(grids[first['photoId']], grids[second['photoId']])
            same = a['sha256'] == b['sha256'] or a['pixelSha256'] == b['pixelSha256']
            pairs.append(dict(first=first['photoId'], second=second['photoId'], againstPriorDataset=old_b,
                              exactDuplicate=same, dHashDistance=dh, pHashDistance=ph, ssim=ssim,
                              needsReview=same or dh <= 8 or ph <= 8 or ssim >= .9))
    manifest = dict(schemaVersion=1, createdUtc=datetime.now(timezone.utc).isoformat(), images=images,
                    rejectedDownloads=failures, normalization='Preserved official CDN source <=1280px; oriented sRGB input <=960px JPEG95, no chroma subsampling.',
                    license='https://www.pexels.com/license/', duplicateChecks=dict(pairCount=len(pairs),
                    exactDuplicates=sum(p['exactDuplicate'] for p in pairs), flaggedPairs=[p for p in pairs if p['needsReview']],
                    pairwise=pairs, visualInspection={'status': 'pending'}))
    (root / 'manifest.json').write_text(json.dumps(manifest, indent=2), encoding='utf-8')
    font = ImageFont.truetype('C:/Windows/Fonts/arial.ttf', 15)
    sheet = Image.new('RGB', (1200, 45 + math.ceil(len(images) / 4) * 275), '#16161b')
    draw = ImageDraw.Draw(sheet)
    draw.text((12, 12), 'AURA - 20 NEW targets + 6 separate reference sources', font=font, fill='white')
    for i, item in enumerate(images):
        x, y = i % 4 * 300, 45 + i // 4 * 275
        photo = helpers.rgb_open(root / item['input']['path'])
        photo.thumbnail((285, 220))
        sheet.paste(photo, (x + (300 - photo.width) // 2, y))
        draw.text((x + 8, y + 223), f'{item["role"]} {item["index"]:02} - {item["photoId"]}', font=font, fill='#ffe0a0')
        draw.text((x + 8, y + 245), item['description'], font=font, fill='white')
    sheet.save(root / 'contact-sheet.jpg', quality=93)
    credits = ['# Twenty new targets and six separate reference sources', '', '[Pexels licence](https://www.pexels.com/license/). Sources are unchanged; edited references are labelled separately.', '']
    credits += [f'- {x["role"]} {x["index"]}: [{x["description"]}]({x["sourcePage"]}) - {x["author"]}, Pexels {x["photoId"]}.' for x in images]
    (root / 'ATTRIBUTION.md').write_text('\n'.join(credits) + '\n', encoding='utf-8')
    print(json.dumps({'downloaded': len(images), 'failed': len(failures), 'duplicateFlags': len(manifest['duplicateChecks']['flaggedPairs']), 'output': str(root)}), flush=True)
    assert len(images) == 26 and not failures and not manifest['duplicateChecks']['exactDuplicates']


if __name__ == '__main__':
    main()
