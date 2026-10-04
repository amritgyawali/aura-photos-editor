"""Verify painted retouch, disposable previews and delivery on five real portraits.

Requires playwright and pillow, plus a running desktop with WebView2 CDP enabled.
Usage: python scripts/test-precision-retouch.py --photos PATH --output PATH
"""
import argparse
import base64
import hashlib
import json
import time
from pathlib import Path

from PIL import Image, ImageChops
from playwright.sync_api import sync_playwright, expect


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--photos', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--endpoint', default='http://127.0.0.1:9223')
    parser.add_argument('--resume', action='store_true', help='Resume a saved report after a desktop restart')
    args = parser.parse_args()
    paths = sorted(args.photos.resolve().glob('*.jpg'))
    if len(paths) != 5:
        raise ValueError('Provide exactly five JPEG portraits')
    digest = lambda value: hashlib.sha256(value).hexdigest()
    originals = {p.name: digest(p.read_bytes()) for p in paths}
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    report = json.loads((output/'results.json').read_text(encoding='utf-8')) if args.resume else {'photos': []}
    report['status'] = 'running'
    report.pop('error', None)
    expected_originals = report.setdefault('originalHashes', originals)
    assert originals == expected_originals, 'Source files changed since the earlier run'
    name = report.get('collectionName', f'Precision retouch {time.strftime("%Y-%m-%d %H-%M-%S")}')
    with sync_playwright() as playwright:
        browser = playwright.chromium.connect_over_cdp(args.endpoint)
        page = next(p for c in browser.contexts for p in c.pages if 'tauri' in p.url or 'localhost' in p.url)
        page.set_default_timeout(60_000)

        def invoke(command, data=None):
            return page.evaluate('''async ([command, args]) => {
                try { return await window.__TAURI_INTERNALS__.invoke(command, args); }
                catch (error) { throw new Error(command + ': ' + JSON.stringify(error)); }
            }''', [command, data or {}])

        def full(photo):
            r = invoke('render_image', {'input': {'photoId': photo, 'level': 'full', 'purpose': 'export'}})
            return Image.frombytes('RGB', (r['width'], r['height']), base64.b64decode(r['rgbBase64'], validate=True))

        try:
            project = report.get('projectId') or invoke('create_project', {'input': {'name': name, 'coupleNames': None, 'eventDate': None}})['id']
            report.update(projectId=project, collectionName=name)
            invoke('start_ingest', {'input': {'projectId': project, 'roots': [str(p) for p in paths]}})
            photos = []
            deadline = time.monotonic() + 90
            while len(photos) != 5 and time.monotonic() < deadline:
                photos = invoke('list_images', {'input': {'projectId': project, 'offset': 0, 'limit': 10, 'orderBy': 'timeline'}})
                time.sleep(.5)
            assert len(photos) == 5

            def edit(photo, action, edits=None, id=None):
                return invoke('native_retouch_edit', {'input': {'projectId': project, 'photoId': photo, 'action': action, 'edits': edits or [], 'id': id}})

            centers = {'portrait-1239291.jpg': [.46,.43], 'portrait-220453.jpg': [.50,.30],
                       'portrait-2379004.jpg': [.52,.30], 'portrait-774909.jpg': [.46,.34],
                       'portrait-8386841.jpg': [.48,.41]}
            for row in photos:
                photo, filename = row['id'], row['fileName']
                previous = next((p for p in report['photos'] if p['id'] == photo), None)
                if previous:
                    assert edit(photo, 'list')[0]['id'] == previous['operationId']
                    assert digest(full(photo).tobytes()) == previous['afterSha256']
                    previous['restartPersistence'] = True
                    print(f'PASS {filename}: persisted mask and identical pixels after restart', flush=True)
                    continue
                before = full(photo)
                before.save(output / f'{filename}-before.png')
                x, y = centers.get(filename, [.5,.4])
                draft = {'id': 'draft', 'tool': 'dodge', 'enabled': True, 'region': [x,y,.05,.05],
                         'source': None, 'amount': .2, 'feather': 0, 'radius': .003,
                         'texture': 1, 'tone': .4, 'warmth': 0, 'tint': 0,
                         'mask': {'strokes': [
                             {'erase': False, 'radius': .02, 'opacity': .7, 'points': [[x-.025,y,1],[x+.025,y,1]]},
                             {'erase': True, 'radius': .008, 'opacity': 1, 'points': [[x,y,1]]},
                         ]}}
                history = invoke('image_history', {'input': {'photoId': photo}})
                trial = invoke('native_retouch_draft_preview', {'input': {
                    'projectId': project, 'photoId': photo, 'edit': draft, 'replaceId': None}})
                assert edit(photo, 'list') == []
                assert invoke('image_history', {'input': {'photoId': photo}}) == history
                saved = edit(photo, 'append', [draft])
                preview = invoke('native_retouch_preview', {'projectId': project, 'photoId': photo, 'before': False})
                assert preview['rgbBase64'] == trial['rgbBase64'], 'Saved output differs from the draft preview'
                after = full(photo)
                after.save(output / f'{filename}-after.png')
                bounds = ImageChops.difference(before, after).getbbox()
                assert bounds, filename
                w, h = after.size
                radius = min(w, h) * .02
                assert bounds[0] >= int((x-.025)*w-radius)-1 and bounds[2] <= int((x+.025)*w+radius)+2
                assert bounds[1] >= int(y*h-radius)-1 and bounds[3] <= int(y*h+radius)+2
                assert after.getpixel((int(x*w), int(y*h))) == before.getpixel((int(x*w), int(y*h))), 'Erased center changed'
                invoke('history_step', {'input': {'projectId': project, 'photoId': photo, 'action': 'undo'}})
                assert full(photo).tobytes() == before.tobytes()
                invoke('history_step', {'input': {'projectId': project, 'photoId': photo, 'action': 'redo'}})
                assert full(photo).tobytes() == after.tobytes()
                report['photos'].append({'id': photo, 'file': filename, 'operationId': saved[0]['id'],
                                        'afterSha256': digest(after.tobytes()), 'changedBounds': bounds,
                                        'previewDidNotSave': True, 'undoRedo': True, 'erasureProtected': True})
                print(f'PASS {filename}: draft isolation, painted effect, erasure, undo/redo', flush=True)
                (output/'results.json').write_text(json.dumps(report,indent=2),encoding='utf-8')

            photo = photos[0]['id']
            original = edit(photo, 'list')[0]
            duplicate = edit(photo, 'duplicate', id=original['id'])
            assert len(duplicate) == 2 and duplicate[0]['id'] != duplicate[1]['id']
            copied = duplicate[1]['id']
            assert edit(photo, 'earlier', id=copied)[0]['id'] == copied
            assert edit(photo, 'later', id=copied)[1]['id'] == copied
            assert len(edit(photo, 'remove', id=copied)) == 1
            history = invoke('image_history', {'input': {'photoId': photo}})
            invoke('native_retouch_draft_preview', {'input': {'projectId': project, 'photoId': photo,
                'edit': dict(original, amount=.8), 'replaceId': original['id']}})
            assert edit(photo, 'list') == [original]
            assert invoke('image_history', {'input': {'photoId': photo}}) == history
            report['operationStack'] = {'duplicate': True, 'reorder': True, 'replacementPreviewDoesNotSave': True}

            result = invoke('export_run', {'input': {'projectId': project, 'destination': str(output/'exports'),
                'destinationKind': 'folder', 'copyright': None, 'contact': None, 'creator': None, 'keywords': [],
                'stripGps': True, 'stripCameraSerial': True, 'verify': True,
                'sets': [{'name': 'precision', 'imageIds': [p['id'] for p in photos], 'format': 'png', 'quality': 95,
                          'colour': 'srgb', 'bitDepth': 8, 'resize': 'full', 'sharpen': 'none', 'naming': '{original}', 'sidecar': True}]}})
            assert result['written'] == 5 and result['verified'] == 5 and result['corrupt'] == 0
            files = invoke('export_files', {'projectId': project})
            assert len(files) == 5
            for file in files:
                expected = next(p for p in report['photos'] if p['id'] == file['imageId'])
                with Image.open(output/'exports'/file['path']) as image:
                    assert digest(image.convert('RGB').tobytes()) == expected['afterSha256']
            assert originals == {p.name: digest(p.read_bytes()) for p in paths}
            report.update(export=result, originalsUnchanged=True, exactExportMatch=True)

            page.reload()
            page.get_by_role('button', name=f'{name} 5', exact=True).click()
            page.get_by_role('button', name='Auto edit One click, start to finish', exact=True).click()
            page.get_by_role('button', name='Retouch', exact=True).click()
            expect(page.get_by_role('button', name='Apply retouch', exact=True)).to_be_enabled()
            page.get_by_label('Tool', exact=True).select_option('dodge')
            page.get_by_role('button', name='Brush (B)', exact=True).click()
            page.get_by_role('button', name='Zoom in', exact=True).click()
            surface = page.get_by_label('Retouch image interaction')
            surface.scroll_into_view_if_needed()
            bounds = surface.bounding_box()
            assert bounds
            x, y = bounds['x']+bounds['width']*.48, bounds['y']+bounds['height']*.45
            page.mouse.move(x,y); page.mouse.down(); page.mouse.move(x+30,y+15,steps=10); page.mouse.up()
            expect(page.get_by_role('button',name='Undo brush stroke',exact=True)).to_be_enabled()
            page.get_by_label('Preview unsaved changes').check()
            expect(page.get_by_text('Unsaved preview. Apply to keep this change.',exact=True)).to_be_visible(timeout=90_000)
            assert len(edit(photo,'list')) == 1, 'The UI preview saved a draft'
            page.screenshot(path=str(output/'precision-retouch-workspace.png'),full_page=True)
            page.get_by_role('button',name='Discard draft',exact=True).click()
            page.get_by_role('button',name='Back to Develop',exact=True).click()
            report.update(status='passed', desktopUI=True)
            print('PASS five native exports, operation stack, original hashes and real brush UI',flush=True)
        except Exception as error:
            report.update(status='failed',error=str(error))
            raise
        finally:
            (output/'results.json').write_text(json.dumps(report,indent=2),encoding='utf-8')


if __name__ == '__main__':
    main()
