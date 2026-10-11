"""Run a public before image through native AURA and compare its paired reference.

Use an isolated catalogue/WebView. Marketing images are a reference, not a
lossless-source benchmark or a claim of complete feature/quality equivalence.
The source images and labelled comparison remain local, outside application assets.
"""
import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

from PIL import Image, ImageDraw


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('before', type=Path)
    parser.add_argument('reference', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--collection', required=True)
    parser.add_argument('--port', type=int, default=9363)
    args = parser.parse_args()
    before_hash, reference_hash = sha(args.before), sha(args.reference)
    before = Image.open(args.before).convert('RGB')
    reference = Image.open(args.reference).convert('RGB')
    if before.size != reference.size:
        raise SystemExit('The paired images must have identical dimensions; no resizing is applied.')
    args.output.mkdir(parents=True, exist_ok=True)
    source = args.output / 'reference-before.png'
    # Lossless format conversion only: the importer does not accept WebP.
    before.save(source)
    assert Image.open(source).convert('RGB').tobytes() == before.tobytes()
    scripts = Path(__file__).resolve().parent
    common = ['--port', str(args.port), '--collection', args.collection]
    native = args.output / 'native'
    full = args.output / 'native-full'
    subprocess.run([sys.executable, str(scripts / 'verify-professional-retouch.py'),
                    str(source), str(native), '--retouch-only', '--detailed-skin', *common], check=True)
    subprocess.run([sys.executable, str(scripts / 'verify-saved-retouch-export.py'),
                    str(source), str(full), *common], check=True)
    aura = Image.open(full / 'after-full.png').convert('RGB')
    assert aura.size == before.size
    assert sha(args.before) == before_hash and sha(args.reference) == reference_hash
    width, height = before.size
    comparison = Image.new('RGB', (3 * width, height + 40), '#17191c')
    draw = ImageDraw.Draw(comparison)
    for index, (label, frame) in enumerate([
        ('Public before image', before),
        ('AURA native detailed skin repair', aura),
        ('Published Evoto after image', reference),
    ]):
        draw.text((index * width + 12, 12), label, fill='white')
        comparison.paste(frame, (index * width, 40))
    comparison.save(args.output / 'comparison.png')
    recipe = json.loads((native / 'recipe.json').read_text(encoding='utf8'))
    body = json.loads(recipe['body'])
    faces = body.get('studio_portrait_auto_v1', {}).get('faces', [])
    for index, face in enumerate(faces):
        left, top, right, bottom = face['bounds']
        box = (int(left * width), int(top * height), int(right * width), int(bottom * height))
        crops = [frame.crop(box) for frame in (before, aura, reference)]
        crop_width, crop_height = crops[0].size
        detail = Image.new('RGB', (3 * crop_width, crop_height + 40), '#17191c')
        draw = ImageDraw.Draw(detail)
        for column, (label, crop) in enumerate(zip(('Original', 'AURA', 'Published Evoto'), crops)):
            draw.text((column * crop_width + 12, 12), label, fill='white')
            detail.paste(crop, (column * crop_width, 40))
        detail.save(args.output / f'face-{index + 1}-comparison.png')
    report = {
        'before_sha256': before_hash, 'reference_sha256': reference_hash,
        'dimensions': list(before.size), 'comparison_ready': True, 'detected_faces': len(faces),
        'native_verification': json.loads((full / 'verification.json').read_text(encoding='utf8')),
        'complete_removal_established': False,
        'full_feature_parity_established': False,
        'superior_quality_established': False,
        'limitations': [
            'Published marketing exports are not lossless camera originals or matched editing settings.',
            'Export integrity and protected regions do not measure complete blemish removal.',
            'Visual judgement and an independently annotated, varied dataset are still required.',
        ],
    }
    (args.output / 'comparison-verification.json').write_text(json.dumps(report, indent=2), encoding='utf8')
    print('Native paired comparison exported; quality claims require separate assessment.', flush=True)


if __name__ == '__main__':
    main()
