"""Exercise a running AURA desktop's real import, renderer, history and PNG export.

Requires: pip install playwright pillow
Start the debug desktop with WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9223.
Run: python scripts/test-portrait-studio.py --photos PATH --output PATH
Creates a separate collection; never modifies the source photographs.
"""
import argparse
import base64
import hashlib
import json
import time
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageOps, ImageStat
from playwright.sync_api import sync_playwright


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--photos', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--endpoint', default='http://127.0.0.1:9223')
    args = parser.parse_args()
    paths = sorted(args.photos.resolve().glob('*.jpg'))
    if len(paths) != 5:
        raise ValueError('Provide exactly five JPEG portraits in the test folder')
    original_hashes = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    results = []
    report = {'status': 'running', 'photos': results}

    with sync_playwright() as playwright:
        browser = playwright.chromium.connect_over_cdp(args.endpoint)
        page = next(p for c in browser.contexts for p in c.pages if 'tauri' in p.url or 'localhost' in p.url)
        page.set_default_timeout(60_000)
        errors = []
        page.on('pageerror', lambda error: errors.append(str(error)))

        def invoke(command, data=None):
            return page.evaluate('''async ([command, args]) => {
                try { return await window.__TAURI_INTERNALS__.invoke(command, args); }
                catch (error) { throw new Error(command + ': ' + JSON.stringify(error)); }
            }''', [command, data or {}])

        def call(command, data):
            return invoke(command, {'input': data})

        def rendered(photo, name):
            payload = call('render_image', {'photoId': photo, 'level': 'full', 'screen': None, 'colourSpace': 'srgb', 'purpose': 'export'})
            rgb = base64.b64decode(payload['rgbBase64'], validate=True)
            assert len(rgb) == payload['width'] * payload['height'] * 3
            image = Image.frombytes('RGB', (payload['width'], payload['height']), rgb)
            image.save(output / name)
            return image, hashlib.sha256(rgb).hexdigest()

        try:
            name = f'Five real portraits {time.strftime("%Y-%m-%d %H-%M-%S")}'
            project = call('create_project', {'name': name, 'coupleNames': None, 'eventDate': None})['id']
            report['projectId'] = project
            page.evaluate('''async () => {
                window.__portraitEvents = [];
                const handler = window.__TAURI_INTERNALS__.transformCallback(event => window.__portraitEvents.push(event.payload));
                await window.__TAURI_INTERNALS__.invoke('plugin:event|listen', {event: 'ingest', target: {kind: 'Any'}, handler});
            }''')
            call('start_ingest', {'projectId': project, 'roots': [str(p) for p in paths]})
            deadline = time.monotonic() + 90
            photos = []
            while len(photos) < 5 and time.monotonic() < deadline:
                photos = call('list_images', {'projectId': project, 'offset': 0, 'limit': 10, 'orderBy': 'timeline'})
                if len(photos) < 5:
                    page.wait_for_timeout(500)
            assert len(photos) == 5, f'Expected five imported portraits, got {len(photos)}'
            page.wait_for_function("window.__portraitEvents.some(event => event.kind === 'finished')")
            events = page.evaluate('window.__portraitEvents')
            assert not any(event['kind'] == 'warning' for event in events), events
            assert next(event for event in events if event['kind'] == 'finished')['inserted'] == 5
            report['importCompletionEvent'] = True

            for photo in photos:
                photo_id, stem = photo['id'], Path(photo['fileName']).stem
                before, before_hash = rendered(photo_id, f'{stem}-before.png')
                recipe = call('enhance_photo', {'photoId': photo_id})
                repeated = call('enhance_photo', {'photoId': photo_id})
                assert recipe['recipeHash'] == repeated['recipeHash'], 'Auto enhancement compounded'
                after, after_hash = rendered(photo_id, f'{stem}-after.png')
                assert before.size == after.size
                if before_hash == after_hash:
                    # A balanced photo may legitimately need no correction.
                    assert all(p['value'] == 0 for p in recipe['params'] if p['path'] in
                               ['global.exposure', 'global.contrast', 'global.highlights', 'global.shadows'])
                exposure = next(p['value'] for p in recipe['params'] if p['path'] == 'global.exposure')
                manual = min(4, float(exposure) + 0.5)
                call('set_param', {'projectId': project, 'photoId': photo_id, 'path': 'global.exposure', 'value': manual, 'label': 'Portrait test manual exposure'})
                call('history_step', {'projectId': project, 'photoId': photo_id, 'action': 'undo'})
                undone = call('image_recipe', {'photoId': photo_id})
                assert next(p['value'] for p in undone['params'] if p['path'] == 'global.exposure') == exposure
                call('history_step', {'projectId': project, 'photoId': photo_id, 'action': 'redo'})
                protected = call('enhance_photo', {'photoId': photo_id})
                param = next(p for p in protected['params'] if p['path'] == 'global.exposure')
                assert abs(param['value'] - manual) < 0.00001 and param['protected']
                call('history_step', {'projectId': project, 'photoId': photo_id, 'action': 'undo'})
                assert call('image_history', {'photoId': photo_id})['canRedo']
                call('set_param', {'projectId': project, 'photoId': photo_id, 'path': 'global.exposure', 'value': manual + 0.1, 'label': 'Portrait test new edit branch'})
                assert not call('image_history', {'photoId': photo_id})['canRedo']
                call('history_step', {'projectId': project, 'photoId': photo_id, 'action': 'reset_original'})
                call('enhance_photo', {'photoId': photo_id})
                _, reset_hash = rendered(photo_id, f'{stem}-after.png')
                assert reset_hash == after_hash, 'Reset and re-enhance changed the deterministic result'
                change = sum(ImageStat.Stat(ImageChops.difference(before, after)).mean) / 3
                results.append({'file': photo['fileName'], 'dimensions': before.size, 'meanChannelChange': round(change, 3), 'autoRepeatable': True, 'manualProtected': True, 'undoRedoPassed': True, 'newEditDiscardsRedo': True})
                print(f'PASS: {photo["fileName"]} - enhance, repeat, undo, redo, manual protection', flush=True)

            exported = call('export_run', {
                'projectId': project, 'destination': str(output / 'exports'), 'destinationKind': 'folder',
                'copyright': None, 'contact': None, 'creator': None, 'keywords': [],
                'stripGps': True, 'stripCameraSerial': True, 'verify': True,
                'sets': [{'name': 'portraits', 'imageIds': [p['id'] for p in photos], 'format': 'png', 'quality': 95,
                          'colour': 'srgb', 'bitDepth': 8, 'resize': 'full', 'sharpen': 'none', 'naming': '{seq}-edited', 'sidecar': False}],
            })
            assert exported['written'] == 5 and exported['verified'] == 5 and exported['corrupt'] == 0 and exported['renderFailed'] == 0
            assert exported['manifestSealed']
            report['export'] = exported
            # Use the export ledger: templates can use fallbacks, and an existing
            # destination can legitimately cause collision-safe filename suffixes.
            exports = invoke('export_files', {'projectId': project})
            assert len(exports) == 5
            for photo in photos:
                source = args.photos.resolve() / photo['fileName']
                entry = next(item for item in exports if item['imageId'] == photo['id'])
                dest = output / 'exports' / entry['path']
                assert entry['verified'] and entry['hash']
                with Image.open(dest) as actual, Image.open(output / f'{source.stem}-after.png') as expected:
                    assert actual.size == expected.size
                    assert actual.convert('RGB').tobytes() == expected.tobytes(), 'Export differs from the full renderer'
                assert hashlib.sha256(source.read_bytes()).hexdigest() == original_hashes[source.name], 'Original changed'
            report['originalsUnchanged'] = True
            report['exportFiles'] = exports

            # The UI and export use the same real native collection; no IPC mocks.
            page.reload()
            page.get_by_role('button', name=f'{name} 5', exact=True).click()
            page.get_by_role('button', name='Auto edit One click, start to finish', exact=True).click()
            auto = page.get_by_role('button', name='Auto enhance photo', exact=True)
            from playwright.sync_api import expect
            expect(auto).to_be_enabled(timeout=60_000)
            auto.click()
            expect(auto).to_be_enabled(timeout=60_000)
            expect(page.get_by_role('figure', name='Edited photo RGB histogram')).to_be_visible()
            page.get_by_role('button', name='Compare', exact=True).click()
            divider = page.get_by_role('slider', name='Before and after divider')
            divider.press('ArrowLeft')
            expect(divider).to_have_value('49')
            divider.press('ArrowRight')
            expect(divider).to_have_value('50')
            page.locator('.photo-studio').screenshot(path=str(output / 'editor-essentials.png'))
            page.get_by_role('button', name='Advanced', exact=True).click()
            expect(page.get_by_text('Calibration', exact=True)).to_be_visible()
            page.locator('.photo-studio').screenshot(path=str(output / 'editor-advanced.png'))
            page.get_by_role('button', name='Essentials', exact=True).click()
            options = page.get_by_role('listbox', name='Filmstrip').get_by_role('option')
            expect(options).to_have_count(5)
            expect(options.first.locator('img')).to_be_visible()
            options.first.focus()
            options.first.press('ArrowRight')
            expect(options.nth(1)).to_have_attribute('aria-selected', 'true')
            expect(auto).to_be_enabled(timeout=60_000)
            report['ui'] = {'autoEnhance': True, 'histogram': True, 'compare': True, 'essentialsAndAdvanced': True, 'thumbnailNavigation': True}
            assert not errors, errors
            report['status'] = 'passed'
        except Exception as error:
            report['status'] = 'failed'
            report['error'] = str(error)
            page.screenshot(path=str(output / 'failure.png'))
            raise
        finally:
            (output / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')

    sheet = Image.new('RGB', (1500, 470), '#24232b')
    draw = ImageDraw.Draw(sheet)
    for index, source in enumerate(paths):
        for half, suffix in enumerate(['before', 'after']):
            with Image.open(output / f'{source.stem}-{suffix}.png') as frame:
                tile = ImageOps.contain(frame, (146, 400))
                x = index * 300 + half * 150
                sheet.paste(tile, (x + (146 - tile.width) // 2, 35 + (400 - tile.height) // 2))
                draw.text((x + 8, 12), suffix.upper(), fill='#e5d4ff')
        draw.text((index * 300 + 8, 444), source.stem, fill='white')
    sheet.save(output / 'before-after.jpg', quality=95)
    print(f'PASS: 5 full-resolution exports verified; originals unchanged. Results: {output}', flush=True)


if __name__ == '__main__':
    main()
