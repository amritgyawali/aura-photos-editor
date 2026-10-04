"""Check native retouch on five real JPEGs through a running AURA debug desktop.

Requires playwright and pillow; WebView2 CDP enabled on the desktop process.
Creates a separate collection. Source photographs are never edited.
"""
import argparse
import base64
import hashlib
import json
import time
from pathlib import Path

from PIL import Image
from playwright.sync_api import expect, sync_playwright


def digest(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--photos', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--endpoint', default='http://127.0.0.1:9223')
    args = parser.parse_args()
    originals = args.photos.resolve()
    paths = sorted(originals.glob('*.jpg'))
    if len(paths) != 5:
        raise ValueError('Provide exactly five JPEG portraits')
    hashes = {p.name: digest(p.read_bytes()) for p in paths}
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    report = {'status': 'running', 'photos': []}
    name = f'Native retouch validation {time.strftime("%Y-%m-%d %H-%M-%S")}'

    with sync_playwright() as playwright:
        browser = playwright.chromium.connect_over_cdp(args.endpoint)
        page = next(p for c in browser.contexts for p in c.pages
                    if 'tauri' in p.url or 'localhost' in p.url)
        page.set_default_timeout(60_000)

        def invoke(command, data=None):
            return page.evaluate('''async ([command, args]) => {
                try { return await window.__TAURI_INTERNALS__.invoke(command, args); }
                catch (error) { throw new Error(command + ': ' + JSON.stringify(error)); }
            }''', [command, data or {}])

        def full(photo):
            result = invoke('render_image', {'input': {
                'photoId': photo, 'level': 'full', 'purpose': 'export'}})
            raw = base64.b64decode(result['rgbBase64'], validate=True)
            return Image.frombytes('RGB', (result['width'], result['height']), raw)

        try:
            project = invoke('create_project', {'input': {
                'name': name, 'coupleNames': None, 'eventDate': None}})['id']
            report['projectId'] = project
            invoke('start_ingest', {'input': {
                'projectId': project, 'roots': [str(p) for p in paths]}})
            photos = []
            deadline = time.monotonic() + 90
            while len(photos) != 5 and time.monotonic() < deadline:
                photos = invoke('list_images', {'input': {
                    'projectId': project, 'offset': 0, 'limit': 10, 'orderBy': 'timeline'}})
                time.sleep(0.5)
            assert len(photos) == 5, 'Ingest did not produce five photographs'

            def edit(photo, action, edits=None):
                return invoke('native_retouch_edit', {'input': {
                    'projectId': project, 'photoId': photo, 'action': action,
                    'edits': edits or [], 'id': None}})

            # These manually selected face regions match the five documented Pexels fixtures.
            # Other inputs use a central selection: this test does not detect faces.
            regions = {
                'portrait-774909.jpg': [.46, .43, .16, .17],
                'portrait-2379004.jpg': [.52, .35, .12, .14],
                'portrait-1239291.jpg': [.46, .49, .08, .16],
                'portrait-220453.jpg': [.50, .42, .17, .18],
                'portrait-8386841.jpg': [.48, .47, .10, .12],
            }
            for row in photos:
                photo, filename = row['id'], row['fileName']
                before = full(photo)
                before.save(output / f'{filename}-before.png')
                draft = {
                    'id': 'draft', 'enabled': True,
                    'region': regions.get(filename, [.5, .45, .15, .18]),
                    'source': None, 'amount': .25, 'feather': .8, 'radius': .003,
                    'texture': 1, 'tone': .4, 'warmth': 0, 'tint': 0,
                }
                saved = edit(photo, 'append', [
                    dict(draft, tool='frequency'),
                    dict(draft, tool='micro_dodge_burn', amount=.18),
                    dict(draft, tool='mattify', amount=.2),
                ])
                assert len(saved) == 3 and len({e['id'] for e in saved}) == 3
                after = full(photo)
                after.save(output / f'{filename}-after.png')
                a, b = before.tobytes(), after.tobytes()
                assert a != b, f'No pixel effect: {filename}'
                invoke('history_step', {'input': {
                    'projectId': project, 'photoId': photo, 'action': 'undo'}})
                assert edit(photo, 'list') == [] and full(photo).tobytes() == a
                invoke('history_step', {'input': {
                    'projectId': project, 'photoId': photo, 'action': 'redo'}})
                assert full(photo).tobytes() == b
                preview = invoke('native_retouch_preview', {
                    'projectId': project, 'photoId': photo, 'before': False})
                assert 'studio_retouch' in preview['stagesRun']
                report['photos'].append({
                    'id': photo, 'file': filename, 'dimensions': list(after.size),
                    'changedChannels': sum(x != y for x, y in zip(a, b)),
                    'afterSha256': digest(b), 'operationIds': [e['id'] for e in saved],
                    'undoRedoPassed': True,
                })
                print(f'PASS {filename}: rendering and undo/redo', flush=True)
            try:
                edit(photos[0]['id'], 'append', [dict(draft, tool='clone', source=None)])
            except Exception as error:
                assert 'source' in str(error).lower(), str(error)
            else:
                raise AssertionError('Missing clone source was accepted')

            result = invoke('export_run', {'input': {
                'projectId': project, 'destination': str(output / 'exports'),
                'destinationKind': 'folder', 'copyright': None, 'contact': None,
                'creator': None, 'keywords': [], 'stripGps': True,
                'stripCameraSerial': True, 'verify': True,
                'sets': [{'name': 'retouch', 'imageIds': [p['id'] for p in photos],
                          'format': 'png', 'quality': 95, 'colour': 'srgb',
                          'bitDepth': 8, 'resize': 'full', 'sharpen': 'none',
                          'naming': '{original}', 'sidecar': True}],
            }})
            assert result['written'] == 5 and result['verified'] == 5, result
            assert result['corrupt'] == 0 and result['renderFailed'] == 0, result
            files = invoke('export_files', {'projectId': project})
            assert len(files) == 5
            for file in files:
                item = next(p for p in report['photos'] if p['id'] == file['imageId'])
                with Image.open(output / 'exports' / file['path']) as image:
                    assert digest(image.convert('RGB').tobytes()) == item['afterSha256']
            report['export'] = result
            report['exactFullRendererMatch'] = True
            assert hashes == {p.name: digest(p.read_bytes()) for p in paths}
            report['originalsUnchanged'] = True

            page.reload()
            page.get_by_role('button', name=f'{name} 5', exact=True).click()
            page.get_by_role('button', name='Auto edit One click, start to finish', exact=True).click()
            page.get_by_role('button', name='Retouch', exact=True).click()
            expect(page.get_by_alt_text('Retouched photograph')).to_be_visible()
            expect(page.get_by_role('button', name='Apply retouch', exact=True)).to_be_enabled()
            page.get_by_label('Tool', exact=True).select_option('frequency')
            page.screenshot(path=str(output / 'native-retouch-workspace.png'), full_page=True)
            page.get_by_role('button', name='Show before retouch', exact=True).click()
            expect(page.get_by_alt_text('Before native retouch')).to_be_visible()
            page.get_by_role('button', name='Show retouched', exact=True).click()
            page.get_by_role('button', name='Discard draft', exact=True).click()
            page.get_by_role('button', name='Back to Develop', exact=True).click()
            expect(page.get_by_role('button', name='Retouch', exact=True)).to_be_enabled()
            report['ui'] = {'workspaceMounted': True, 'beforeAfter': True, 'developReturn': True}
            report['status'] = 'passed'
            print('PASS: five native retouches, exports, original hashes, and desktop UI', flush=True)
        except Exception as error:
            report['status'] = 'failed'
            report['error'] = str(error)
            raise
        finally:
            (output / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')


if __name__ == '__main__':
    main()
