"""Import, save native color/proportion tools, inspect boundaries, undo/redo and export.

AURA renders every image. This harness only drives controls and reads its output.
Use an isolated catalog and WebView profile, never the photographer's working catalog.
"""
import argparse
import base64
import hashlib
import json
import shutil
import time
from pathlib import Path

import numpy as np
from PIL import Image
from playwright.sync_api import expect, sync_playwright

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('source', type=Path)
parser.add_argument('output', type=Path)
parser.add_argument('--port', type=int, default=9362)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
incoming = args.output / 'incoming'
incoming.mkdir(exist_ok=True)
original_hash = hashlib.sha256(args.source.read_bytes()).hexdigest()
copy = incoming / args.source.name
shutil.copyfile(args.source, copy)
report = {'source': str(args.source), 'original_hash': original_hash, 'tools': [], 'page_errors': []}

with sync_playwright() as p:
    page = p.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}').contexts[0].pages[0]
    expect.set_options(timeout=300000)
    page.on('pageerror', lambda error: report['page_errors'].append(str(error)))
    def call(command, value=None):
        return page.evaluate('([c,v])=>window.__TAURI_INTERNALS__.invoke(c,v)', [command, value or {}])
    def recipe():
        return call('image_recipe', {'input': {'photoId': photo['id']}})
    collection = 'Color and proportions ' + time.strftime('%H%M%S') + ' verification'
    page.get_by_label('New collection', exact=True).fill(collection)
    page.get_by_role('button', name='Create', exact=True).click()
    page.locator('.studio-nav').get_by_role('button', name='Photos Browse your collection', exact=True).click()
    page.get_by_text('Enter folders manually', exact=True).click()
    page.get_by_label('Card or folder', exact=True).fill(str(incoming.resolve()))
    page.get_by_role('button', name='Add folder', exact=True).click()
    page.get_by_role('button', name='Start import', exact=True).click()
    expect(page.get_by_role('button', name='Retouch', exact=True)).to_be_enabled()
    project = next(v for v in call('list_projects') if v['name'] == collection)
    [photo] = call('list_images', {'input': {'projectId': project['id'], 'offset': 0, 'limit': 10, 'orderBy': None}})
    value = {'projectId': project['id'], 'photoId': photo['id']}
    page.get_by_role('button', name='Retouch', exact=True).click()
    def pixels():
        frame = call('native_retouch_preview', {**value, 'before': False, 'quality': 'full'})
        return np.frombuffer(base64.b64decode(frame['rgbBase64']), dtype=np.uint8).reshape(frame['height'], frame['width'], 3).copy()
    before = pixels()
    report['dimensions'] = [before.shape[1], before.shape[0]]
    initial = recipe()['recipeHash']
    for tool, region in [('colorize', [.23,.48,.035,.19]), ('background_color', [.08,.5,.065,.35]), ('reshape', [.65,.75,.15,.15])]:
        button = page.get_by_role('button', name='Apply retouch', exact=True)
        expect(page.get_by_label('Tool', exact=True)).to_be_enabled()
        page.get_by_label('Tool', exact=True).select_option(tool)
        for label, amount in zip(['Center X (%)','Center Y (%)','Horizontal radius (%)','Vertical radius (%)'], region):
            page.get_by_label(label, exact=True).fill(str(amount*100))
        if tool != 'reshape':
            page.get_by_label('Target color', exact=True).fill('#6d3b9e' if tool == 'colorize' else '#d7e7ec')
        else:
            # Keyboard range input is the browser-supported input path in WebView2.
            slider = page.get_by_label('Local width', exact=True)
            slider.focus()
            slider.press('Home')
            for _ in range(65):
                slider.press('ArrowRight')
        expect(button).to_be_enabled()
        previous = recipe()['recipeHash']
        button.click()
        expect(page.get_by_label('Tool', exact=True)).to_be_enabled()
        saved = recipe()
        assert saved['recipeHash'] != previous
        edit = json.loads(saved['body'])['studio_retouch_v1'][-1]
        assert edit['tool'] == tool and np.allclose(edit['region'], region)
        after = pixels()
        assert before.shape == after.shape
        h,w,_ = before.shape
        yy,xx = np.ogrid[:h,:w]
        inside = (((xx+.5)/w-region[0])/region[2])**2 + (((yy+.5)/h-region[1])/region[3])**2 < 1
        changed = np.any(before != after, axis=2)
        assert not changed[~inside].any(), f'{tool} changed pixels outside its selection'
        assert changed.any(), f'{tool} made no visible change'
        Image.fromarray(before).save(args.output / (tool + '-before.png'))
        Image.fromarray(after).save(args.output / (tool + '-after.png'))
        call('history_step', {'input': {**value, 'action': 'undo'}})
        assert recipe()['recipeHash'] == previous
        assert np.array_equal(pixels(), before)
        call('history_step', {'input': {**value, 'action': 'redo'}})
        assert recipe()['recipeHash'] == saved['recipeHash']
        assert np.array_equal(pixels(), after)
        report['tools'].append({'tool': tool, 'changed_pixels': int(changed.sum()), 'outside_unchanged': True, 'undo_redo_exact': True, 'edit': edit})
        # UI history controls refresh the saved recipe and previews too.
        page.get_by_role('button', name='Undo', exact=True).click()
        expect(page.get_by_role('button', name='Redo', exact=True)).to_be_enabled()
        page.get_by_role('button', name='Redo', exact=True).click()
        expect(page.get_by_label('Tool', exact=True)).to_be_enabled()
        page.screenshot(path=str(args.output / (tool + '-ui.png')))
        before = after
        print('PASS ' + tool, flush=True)
    report['initial_recipe_hash'] = initial
    report['final_recipe_hash'] = recipe()['recipeHash']
    (args.output/'tool-verification.json').write_text(json.dumps(report, indent=2), encoding='utf8')
    page.get_by_role('button', name='Back to Develop', exact=True).click()
    page.locator('.studio-nav').get_by_role('button', name='Export Ready to share', exact=True).click()
    page.get_by_test_id('destination').fill(str((args.output / 'export').resolve()))
    page.get_by_test_id('verify').check()
    page.get_by_test_id('preview-names').click()
    page.get_by_test_id('run').click()
    manifest_path = args.output / 'export/aura-delivery-manifest.json'
    deadline = time.monotonic() + 300
    while not manifest_path.exists():
        assert time.monotonic() < deadline, 'No native export manifest'
        time.sleep(.25)
    manifest = json.loads(manifest_path.read_text())
    assert manifest['verified'] and manifest['file_count'] == 1
    export_root = (args.output/'export').resolve()
    exported_path = (export_root / manifest['files'][0]['path']).resolve()
    assert exported_path.is_relative_to(export_root), 'Export path leaves the destination'
    with Image.open(exported_path) as exported:
        exported.load()
        assert list(exported.size) == report['dimensions']
    assert hashlib.sha256(args.source.read_bytes()).hexdigest() == original_hash
    assert hashlib.sha256(copy.read_bytes()).hexdigest() == original_hash
    assert not report['page_errors']
    report.update(original_unchanged=True, imported_copy_unchanged=True, manifest=manifest,
                  recipe_unchanged_by_export=recipe()['recipeHash'] == report['final_recipe_hash'])
    assert report['recipe_unchanged_by_export']
    page.screenshot(path=str(args.output/'export-ui.png'))
    (args.output/'verification.json').write_text(json.dumps(report, indent=2), encoding='utf8')
    print(json.dumps({'tools':[v['tool'] for v in report['tools']], 'native_export_verified':True,'original_unchanged':True}), flush=True)
