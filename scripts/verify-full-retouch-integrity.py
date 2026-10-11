"""Check full-resolution native pixels in an isolated retouch test collection.

Temporarily clear its test stack to render a native baseline, then undo its diagnostic
history entries in a finally block. Original files are never written.
"""
import argparse
import base64
import hashlib
import json
from pathlib import Path

import numpy as np
from PIL import Image
from playwright.sync_api import sync_playwright
from retouch_evidence import is_compact_spot, only_local_feather_changes, only_local_spot_changes, coarse_region_change

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('source', type=Path)
parser.add_argument('output', type=Path)
parser.add_argument('--port', type=int, default=9359)
parser.add_argument('--collection', default='Professional retouch verification')
parser.add_argument('--photo-name', help='Select one photo in an isolated batch verification collection')
parser.add_argument('--protected-rect', type=float, nargs=4)
parser.add_argument('--allow-structure-feather', action='store_true')
parser.add_argument('--allow-structure-spots', action='store_true')
parser.add_argument('--opening-core', type=float, nargs=4, action='append',
                    help='independently inspected anatomical opening rectangle; supply both openings instead of landmark guesses')
args = parser.parse_args()
assert args.collection.endswith('verification'), 'Use an isolated verification collection'
args.output.mkdir(parents=True, exist_ok=True)
original_hash = hashlib.sha256(args.source.read_bytes()).hexdigest()
with sync_playwright() as p:
    page = p.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}').contexts[0].pages[0]

    def invoke(name, values):
        return page.evaluate('([name,values])=>window.__TAURI_INTERNALS__.invoke(name,values)', [name, values])

    project = next(v for v in invoke('list_projects', {}) if v['name'] == args.collection)
    photos = invoke('list_images', {'input': {'projectId': project['id'], 'offset': 0, 'limit': 50, 'orderBy': None}})
    [photo] = [photo for photo in photos if not args.photo_name or photo['fileName'] == args.photo_name]
    photo_id = photo['id']
    recipe = invoke('image_recipe', {'input': {'photoId': photo_id}})
    body = json.loads(recipe['body'])
    assert body['studio_retouch_v1'], 'No saved test retouch to validate'

    def render():
        frame = invoke('render_image', {'input': {'photoId': photo_id, 'level': 'full', 'purpose': 'export'}})
        return np.frombuffer(base64.b64decode(frame['rgbBase64']), dtype=np.uint8).reshape(frame['height'], frame['width'], 3)

    print('Rendering full native retouch', flush=True)
    after = render()
    undo_count = 0
    broad = None
    try:
        invoke('native_retouch_edit', {'input': {'projectId': project['id'], 'photoId': photo_id, 'action': 'clear', 'edits': [], 'id': None}})
        undo_count += 1
        print('Rendering full native baseline', flush=True)
        before = render()
        if args.allow_structure_spots:
            wide = [e for e in body['studio_retouch_v1'] if not is_compact_spot(e)]
            if wide:
                invoke('native_retouch_edit', {'input': {'projectId': project['id'], 'photoId': photo_id, 'action': 'append', 'edits': wide, 'id': None}})
                undo_count += 1
            print('Rendering broad operations without compact patches', flush=True)
            broad = render()
    finally:
        for _ in range(undo_count):
            invoke('history_step', {'input': {'projectId': project['id'], 'photoId': photo_id, 'action': 'undo'}})
        restored = invoke('image_recipe', {'input': {'photoId': photo_id}})
        assert restored['recipeHash'] == recipe['recipeHash'], 'Test recipe did not restore exactly'

    assert before.shape == after.shape
    Image.fromarray(before).save(args.output/'native-full-before.png')
    Image.fromarray(after).save(args.output/'native-full-after.png')
    h, w, _ = after.shape
    face = body['studio_portrait_auto_v1']['faces'][0]
    a, b, nose, mouth_a, mouth_b = np.array(face['landmarks']) * [w, h]
    d = np.linalg.norm(b-a)
    u = (b-a)/d
    v = np.array([-u[1], u[0]])
    if np.dot((mouth_a+mouth_b-a-b)*.5, v) < 0:
        v = -v
    yy, xx = np.mgrid[:h, :w]
    difference = np.max(np.abs(after.astype(np.int16)-before.astype(np.int16)), axis=2)
    metrics = {}
    for name, point, rx, ry, shift in [('left_eye', a, .30, .19, .035), ('right_eye', b, .30, .19, .035),
            ('left_nostril', nose-.18*d*u, .075, .04, .12), ('right_nostril', nose+.18*d*u, .075, .04, .12)]:
        if args.opening_core and 'nostril' in name:
            continue
        dx, dy = xx+.5-point[0], yy+.5-point[1]
        across = (dx*u[0]+dy*u[1])/d
        down = (dx*v[0]+dy*v[1])/d-shift
        core = (across/rx)**2+(down/ry)**2 <= 1
        delta = difference[core]
        metrics[name] = {'pixels': int(core.sum()), 'max_channel_change': int(delta.max())}
        assert delta.max() == 0, metrics[name]
    if args.opening_core:
        assert len(args.opening_core) == 2, 'Inspect both anatomical openings'
        for side, (l,t,r,b) in enumerate(args.opening_core):
            assert 0 <= l < r <= 1 and 0 <= t < b <= 1
            delta = difference[int(t*h):int(b*h),int(l*w):int(r*w)]
            metrics[f'anatomical_opening_{side}'] = {'bounds':[l,t,r,b], 'pixels':int(delta.size), 'max_channel_change':int(delta.max())}
            assert delta.max() == 0, metrics[f'anatomical_opening_{side}']
    if args.protected_rect:
        l, t, r, b = args.protected_rect
        assert 0 <= l < r <= 1 and 0 <= t < b <= 1
        region = difference[int(t*h):int(b*h), int(l*w):int(r*w)]
        metrics['structure_region'] = {'pixels': int(region.size), 'max_channel_change': int(region.max())}
        if args.allow_structure_spots:
            ys, xs = np.nonzero(region)
            points = list(zip(xs+int(l*w), ys+int(t*h)))
            assert only_local_spot_changes(points, body['studio_retouch_v1'], w, h)
            area = (slice(int(t*h), int(b*h)), slice(int(l*w), int(r*w)))
            assert np.array_equal(before[area], broad[area]), 'Broad processing changes the structure region'
            contour = coarse_region_change(before, after, [l, t, r, b], d)
            assert contour['max_relative_coarse_change'] < .01, contour
            metrics['structure_region'].update(contour, changed_pixels=len(points), broad_pixels_unchanged=True, changes_limited_to_saved_compact_spots=True)
        elif args.allow_structure_feather:
            ys, xs = np.nonzero(region)
            points = list(zip(xs+int(l*w), ys+int(t*h)))
            assert only_local_feather_changes(points, body['studio_retouch_v1'], w, h), 'A repair core or unlocalized change reaches the structure region'
            metrics['structure_region'].update(changed_pixels=len(points), changes_limited_to_neighboring_repair_feather=True)
        else:
            assert region.max() == 0, metrics['structure_region']
    assert (difference > 0).any(), 'No skin pixels changed'
    assert hashlib.sha256(args.source.read_bytes()).hexdigest() == original_hash
    result = {'passed': True, 'dimensions': [w, h], 'original_unchanged': True,
              'restored_recipe_hash': restored['recipeHash'], 'protected_regions': metrics,
              'changed_pixels': int((difference > 0).sum()),
              'native_after_rgb_sha256': hashlib.sha256(after.tobytes()).hexdigest()}
    (args.output/'verification.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps(result), flush=True)
