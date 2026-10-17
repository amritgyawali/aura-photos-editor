"""Measure only the last saved blemish brush; restore its exact recipe in finally."""
import argparse
import base64
import json
from pathlib import Path

import numpy as np
from PIL import Image
from playwright.sync_api import sync_playwright

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('output', type=Path)
parser.add_argument('--port', type=int, default=9359)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
with sync_playwright() as p:
    page = p.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}').contexts[0].pages[0]
    def call(command, value):
        return page.evaluate('([c,v])=>window.__TAURI_INTERNALS__.invoke(c,v)', [command, value])
    project = next(row for row in call('list_projects', {})
                   if row['name'] == 'Professional retouch verification')
    [photo] = call('list_images', {'input': {'projectId': project['id'], 'offset': 0, 'limit': 10, 'orderBy': None}})
    value = {'projectId': project['id'], 'photoId': photo['id']}
    saved = call('image_recipe', {'input': {'photoId': photo['id']}})
    edit = json.loads(saved['body'])['studio_retouch_v1'][-1]
    assert edit['tool'] == 'acne_clear' and edit['mask']['strokes']
    def pixels():
        frame = call('native_retouch_preview', {**value, 'before': False, 'quality': 'full'})
        return np.frombuffer(base64.b64decode(frame['rgbBase64']), dtype=np.uint8).reshape(frame['height'], frame['width'], 3)
    after = pixels()
    undone = False
    try:
        call('history_step', {'input': {**value, 'action': 'undo'}})
        undone = True
        before = pixels()
    finally:
        if undone:
            call('history_step', {'input': {**value, 'action': 'redo'}})
        assert call('image_recipe', {'input': {'photoId': photo['id']}})['recipeHash'] == saved['recipeHash']
    assert before.shape == after.shape
    h, w, _ = after.shape
    selected = np.zeros((h, w), dtype=bool)
    for stroke in edit['mask']['strokes']:
        assert not stroke.get('erase', False) and len(stroke['points']) == 1
        x, y, _ = stroke['points'][0]
        x, y = x * w, y * h
        radius = stroke['radius'] * min(w, h) + 2
        l, t, r, b = max(0, int(x-radius)), max(0, int(y-radius)), min(w, int(x+radius)+1), min(h, int(y+radius)+1)
        yy, xx = np.ogrid[t:b, l:r]
        selected[t:b, l:r] |= (xx-x)**2 + (yy-y)**2 <= radius**2
    difference = np.abs(after.astype(np.int16) - before.astype(np.int16)).max(axis=2)
    assert not difference[~selected].any(), 'Brush changed pixels outside its painted footprint'
    Image.fromarray(before).save(args.output / 'before-brush.png')
    Image.fromarray(after).save(args.output / 'after-brush.png')
    result = {'outside_brush_unchanged': True, 'recipe_restored': True,
              'changed_pixels': int(np.count_nonzero(difference)),
              'max_channel_change': int(difference.max()), 'dimensions': [w, h]}
    (args.output / 'results.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps(result), flush=True)
