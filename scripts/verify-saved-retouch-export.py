"""Read back a full-resolution lossless export of a saved native retouch.

Run on an isolated catalogue after verify-professional-retouch.py. No recipe or
input pixels are changed. AURA renders/exports; Pillow only reads output and
writes labelled comparison evidence. Landmark checks require visual inspection.
"""
import argparse
import base64
import hashlib
import json
import math
from pathlib import Path

import numpy as np
from PIL import Image
from playwright.sync_api import sync_playwright

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('source', type=Path)
parser.add_argument('output', type=Path)
parser.add_argument('--collection', required=True)
parser.add_argument('--port', type=int, default=9363)
parser.add_argument('--under-eye-finishing', action='store_true', help='check the protected lid cores separately from deliberately corrected under-eye skin')
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
original_hash = hashlib.sha256(args.source.read_bytes()).hexdigest()

def image(dto):
    return Image.frombytes('RGB', (dto['width'], dto['height']), base64.b64decode(dto['rgbBase64']))

with sync_playwright() as p:
    page = p.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}').contexts[0].pages[0]
    page.set_default_timeout(300000)
    def call(command, value=None):
        return page.evaluate('([c,v])=>window.__TAURI_INTERNALS__.invoke(c,v)', [command, value or {}])
    project = next(row for row in call('list_projects') if row['name'] == args.collection)['id']
    [photo] = call('list_images', {'input': {'projectId': project, 'offset': 0, 'limit': 10, 'orderBy': None}})
    value = {'projectId': project, 'photoId': photo['id']}
    recipe = call('image_recipe', {'input': {'photoId': photo['id']}})
    body = json.loads(recipe['body'])
    if args.under_eye_finishing:
        options = body['studio_portrait_auto_v1']['options']
        assert options['eyes'] and options['settings']['darkCircles'] > 0
        assert options['settings']['protectEyeArea'] and options['settings']['protectNoseDetail']
    before = image(call('native_retouch_preview', {**value, 'before': True, 'quality': 'full'}))
    after = image(call('native_retouch_preview', {**value, 'before': False, 'quality': 'full'}))
    assert before.size == after.size == Image.open(args.source).size, 'Retouch was measured at preview resolution'
    before.save(args.output / 'before-full.png')
    after.save(args.output / 'after-full.png')
    destination = args.output / 'lossless-export'
    export = call('export_run', {'input': {'projectId': project, 'destination': str(destination.resolve()), 'destinationKind': 'folder',
        'copyright': None, 'contact': None, 'creator': None, 'keywords': [], 'stripGps': True, 'stripCameraSerial': True, 'verify': True,
        'sets': [{'name': 'full-skin-validation', 'imageIds': [photo['id']], 'format': 'png', 'quality': 95,
                  'colour': 'srgb', 'bitDepth': 8, 'resize': 'full', 'sharpen': 'none', 'naming': '{original}', 'sidecar': False}]}})
    assert export['written'] == export['verified'] == 1 and export['corrupt'] == export['renderFailed'] == 0, export
    files = call('export_files', {'projectId': project})
    delivered = next(row for row in files if row['imageId'] == photo['id'] and row['set'] == 'full-skin-validation')
    output_image = Image.open(destination / delivered['path']).convert('RGB')
    full = image(call('render_image', {'input': {'photoId': photo['id'], 'level': 'full', 'purpose': 'export', 'colourSpace': 'srgb'}}))
    assert output_image.size == full.size == after.size
    assert output_image.tobytes() == full.tobytes(), 'Lossless export differs from the native full render'
    assert after.tobytes() == full.tobytes(), 'Retouch view differs from the delivered frame; inspect geometry/effects'
    a, b = np.asarray(before).astype(np.int16), np.asarray(after).astype(np.int16)
    w, h = after.size
    coverage = call('native_retouch_saved_selection', {'input': {**value, 'operationId': None}})
    outside_selection = None
    if (coverage['width'], coverage['height']) == (w, h):
        mask = np.frombuffer(base64.b64decode(coverage['rgbBase64']), dtype=np.uint8).reshape(h, w, 3)[:, :, 0]
        outside = mask == 0
        assert np.array_equal(a[outside], b[outside]), 'Retouch changed pixels outside the native selection'
        outside_selection = {'unchanged_pixels': int(outside.sum()), 'selected_pixels': int((mask > 0).sum()),
                             'changed_selected_pixels': int(((mask > 0) & np.any(a != b, axis=2)).sum())}
    yy, xx = np.mgrid[:h, :w]
    checks = []
    under_eye_checks = []
    faces = body.get('studio_portrait_auto_v1', {}).get('faces', [])
    for index, face in enumerate(faces):
        left, right, nose, mouth_left, mouth_right = np.asarray(face['landmarks']) * [w, h]
        distance = math.dist(left, right)
        u = (right - left) / distance
        v = np.array([-u[1], u[0]])
        if np.dot((mouth_left + mouth_right - left - right) * .5, v) < 0:
            v = -v
        features = [('left_eye', left, .30, .19, .035), ('right_eye', right, .30, .19, .035)]
        features += [(name, nose + side * distance * u, .075, .04, .12)
                     for name, side in [('left_nostril_landmark', -.18), ('right_nostril_landmark', .18)]]
        for name, point, rx, ry, shift in features:
            dx, dy = xx + .5 - point[0], yy + .5 - point[1]
            across = (dx * u[0] + dy * u[1]) / distance
            down = (dx * v[0] + dy * v[1]) / distance - shift
            mask = (across / rx) ** 2 + (down / ry) ** 2 <= 1
            if args.under_eye_finishing and name.endswith('_eye'):
                # The old socket ellipse includes the skin this explicit preset
                # corrects. Keep it for all skin-only checks; this mode measures
                # the asymmetric, rotation-aware lid core protected by eye_guard.
                lid_down = down + shift + .03
                mask = (across / .26) ** 2 + (lid_down / np.where(lid_down < 0, .24, .095)) ** 2 <= 1
                # Include the measured upper crescent; the lower cheek alone can
                # hide a successful shadow lift or show unrelated tone finishing.
                band = (np.abs(across) < .22) & (down + shift > .09) & (down + shift < .30)
                under_eye_checks.append({'face': index, 'region': name + '_skin_below',
                    'pixels': int(band.sum()), 'changed_pixels': int(np.any(a[band] != b[band], axis=1).sum()),
                    'mean_before_rgb': a[band].mean(axis=0).tolist(), 'mean_after_rgb': b[band].mean(axis=0).tolist()})
                name += '_lid_core'
            change = int(np.max(np.abs(b[mask] - a[mask])))
            checks.append({'face': index, 'region': name, 'pixels': int(mask.sum()), 'max_channel_change': change})
            assert change == 0, checks[-1]
        l, t, r, bottom = face['bounds']
        box = (int(l * w), int(t * h), int(r * w), int(bottom * h))
        first, second = before.crop(box), after.crop(box)
        pair = Image.new('RGB', (first.width * 2, first.height))
        pair.paste(first); pair.paste(second, (first.width, 0))
        pair.save(args.output / f'face-{index + 1}-full-comparison.png')
    pair = Image.new('RGB', (w * 2, h))
    pair.paste(before); pair.paste(after, (w, 0))
    pair.save(args.output / 'full-comparison.jpg', quality=95)
    assert call('image_recipe', {'input': {'photoId': photo['id']}})['recipeHash'] == recipe['recipeHash']
    assert hashlib.sha256(args.source.read_bytes()).hexdigest() == original_hash
    report = {'original_unchanged': True, 'recipe_unchanged': True, 'recipe_hash': recipe['recipeHash'],
              'export_matches_native_full_render': True, 'retouch_preview_matches_export': True, 'dimensions': [w, h],
              'landmark_feature_checks': checks, 'export': export, 'delivered': delivered}
    report['outside_selection'] = outside_selection
    report['under_eye_finishing'] = args.under_eye_finishing
    report['under_eye_skin_checks'] = under_eye_checks
    (args.output / 'verification.json').write_text(json.dumps(report, indent=2), encoding='utf8')
    print(json.dumps({'verified': True, 'dimensions': [w, h], 'unchanged_feature_regions': len(checks), 'original_unchanged': True}), flush=True)
