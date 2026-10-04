"""Exercise advanced native selections on five real JPEG portraits.

Creates an isolated collection, verifies originals, previews, history and PNG export,
then exercises the desktop gradient and mask controls. Requires Pillow and Playwright.
"""
import argparse
import base64
import hashlib
import json
import math
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
    parser.add_argument('--resume', action='store_true', help='Verify completed photos and resume this isolated test collection')
    args = parser.parse_args()
    paths = sorted(args.photos.resolve().glob('*.jpg'))
    assert len(paths) == 5, 'Provide exactly five portrait JPEGs'
    hashes = {p.name: digest(p.read_bytes()) for p in paths}
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    report = json.loads((output/'results.json').read_text(encoding='utf-8')) if args.resume else {'photos': []}
    assert report.setdefault('originalHashes', hashes) == hashes, 'Originals changed since the earlier run'
    if report.get('status') == 'passed':
        print('This report is already complete. Omit --resume for a new run.')
        return
    name = report.setdefault('collectionName', f'Advanced selection validation {time.strftime("%Y-%m-%d %H-%M-%S")}')
    report.update(status='running')
    report.pop('error', None)
    with sync_playwright() as playwright:
        browser = playwright.chromium.connect_over_cdp(args.endpoint)
        page = next(p for c in browser.contexts for p in c.pages if 'tauri' in p.url or 'localhost' in p.url)
        page.set_default_timeout(60_000)
        expect.set_options(timeout=60_000)

        def invoke(command, data):
            return page.evaluate('''async ([command,args]) => {
                try {return await window.__TAURI_INTERNALS__.invoke(command,args);}
                catch(error) {throw new Error(command+': '+JSON.stringify(error));}
            }''', [command, data])

        def full(photo):
            dto = invoke('render_image', {'input': {'photoId': photo, 'level': 'full', 'purpose': 'export'}})
            return Image.frombytes('RGB', (dto['width'], dto['height']), base64.b64decode(dto['rgbBase64'], validate=True))

        try:
            project = report.get('projectId') or invoke('create_project', {'input': {'name': name, 'coupleNames': None, 'eventDate': None}})['id']
            report['projectId'] = project
            invoke('start_ingest', {'input': {'projectId': project, 'roots': [str(p) for p in paths]}})
            photos = []
            deadline = time.monotonic()+90
            while len(photos) != 5 and time.monotonic() < deadline:
                photos = invoke('list_images', {'input': {'projectId': project, 'offset': 0, 'limit': 10, 'orderBy': 'timeline'}})
                time.sleep(.5)
            assert len(photos) == 5

            def edit(photo, action, edits=None):
                return invoke('native_retouch_edit', {'input': {'projectId': project, 'photoId': photo,
                              'action': action, 'edits': edits or [], 'id': None}})

            for row in photos:
                photo, filename = row['id'], row['fileName']
                previous = next((p for p in report['photos'] if p['id'] == photo), None)
                if previous:
                    assert [e['id'] for e in edit(photo, 'list')] == previous['operationIds']
                    assert digest(full(photo).tobytes()) == previous['afterSha256']
                    print(f'PASS {filename}: persisted operations and pixels after restart', flush=True)
                    continue
                draft = {'id': 'draft', 'enabled': True, 'tool': 'dodge', 'region': [.5,.5,1,1],
                         'source': None, 'amount': .5, 'feather': 0, 'radius': .003,
                         'texture': 1, 'tone': .4, 'warmth': 0, 'tint': 0,
                         'selection': {'inverted': False, 'gradient': {'start': [.35,.5], 'end': [.7,.5]},
                                       'luminance': {'low': -3, 'high': 3, 'softness': 1}}}
                interrupted = edit(photo, 'list')
                if interrupted:
                    assert args.resume and len(interrupted) == 1
                    assert all(interrupted[0][key] == value for key,value in draft.items() if key != 'id'), 'Unexpected saved edits in test collection'
                    invoke('history_step', {'input': {'projectId': project, 'photoId': photo, 'action': 'undo'}})
                    assert edit(photo, 'list') == [], 'Unexpected preceding edit in test collection'
                before = full(photo)
                if interrupted:
                    with Image.open(output/f'{filename}-before.png') as original_render:
                        assert original_render.convert('RGB').tobytes() == before.tobytes()
                before.save(output/f'{filename}-before.png')
                history = invoke('image_history', {'input': {'photoId': photo}})
                for inverted in [False, True]:
                    trial = dict(draft, selection=dict(draft['selection'], inverted=inverted))
                    dto = invoke('native_retouch_selection_preview', {'input': {
                        'projectId': project, 'photoId': photo, 'edit': trial, 'replaceId': None}})
                    raw = base64.b64decode(dto['rgbBase64'], validate=True)
                    assert len(raw) == dto['width']*dto['height']*3
                    assert min(raw) == 0 and max(raw) > 200
                    assert raw[0::3] == raw[1::3] == raw[2::3], 'Mask is not grayscale coverage'
                    Image.frombytes('RGB', (dto['width'],dto['height']),raw).save(output/f'{filename}-mask-{inverted}.png')
                invoke('native_retouch_draft_preview', {'input': {'projectId': project, 'photoId': photo, 'edit': draft, 'replaceId': None}})
                assert edit(photo, 'list') == []
                assert invoke('image_history', {'input': {'photoId': photo}}) == history
                if interrupted:
                    invoke('history_step', {'input': {'projectId': project, 'photoId': photo, 'action': 'redo'}})
                    saved = edit(photo, 'list')
                    assert saved == interrupted
                else:
                    saved = edit(photo, 'append', [draft])
                assert len(saved) == 1 and saved[0]['selection'] == draft['selection']
                after = full(photo)
                after.save(output/f'{filename}-after.png')
                assert before.tobytes() != after.tobytes(), 'No rendered effect'
                protected = (0, 0, math.floor(after.width*.35), after.height)
                assert before.crop(protected).tobytes() == after.crop(protected).tobytes(), 'Protected pixels changed'
                invoke('history_step', {'input': {'projectId': project, 'photoId': photo, 'action': 'undo'}})
                assert edit(photo, 'list') == [] and full(photo).tobytes() == before.tobytes()
                invoke('history_step', {'input': {'projectId': project, 'photoId': photo, 'action': 'redo'}})
                assert full(photo).tobytes() == after.tobytes()
                report['photos'].append({'id': photo, 'file': filename, 'afterSha256': digest(after.tobytes()),
                                         'operationIds': [saved[0]['id']], 'protectedPixelsUnchanged': True,
                                         'previewIsolation': True, 'undoRedoPassed': True})
                print(f'PASS {filename}: masks, protected pixels, undo/redo', flush=True)
            result = invoke('export_run', {'input': {'projectId': project, 'destination': str(output/'exports'),
                'destinationKind': 'folder', 'copyright': None, 'contact': None, 'creator': None, 'keywords': [],
                'stripGps': True, 'stripCameraSerial': True, 'verify': True,
                'sets': [{'name': 'selection', 'imageIds': [p['id'] for p in photos], 'format': 'png', 'quality': 95,
                          'colour': 'srgb', 'bitDepth': 8, 'resize': 'full', 'sharpen': 'none', 'naming': '{original}', 'sidecar': True}]}})
            assert result['written'] == result['verified'] == 5 and result['corrupt'] == result['renderFailed'] == 0
            files = invoke('export_files', {'projectId': project})
            assert len(files) == 5
            for file in files:
                expected = next(p for p in report['photos'] if p['id'] == file['imageId'])
                with Image.open(output/'exports'/file['path']) as image:
                    assert digest(image.convert('RGB').tobytes()) == expected['afterSha256']
            report['export'] = result
            report['exactFullRendererMatch'] = True
            page.reload()
            page.get_by_role('button', name=f'{name} 5', exact=True).click()
            page.get_by_role('button', name='Auto edit One click, start to finish', exact=True).click()
            for row in photos:
                page.get_by_role('option', name=row['fileName'], exact=True).click()
                page.get_by_role('button', name='Retouch', exact=True).click()
                expect(page.get_by_alt_text('Retouched photograph', exact=True)).to_be_visible()
                history = invoke('image_history', {'input': {'photoId': row['id']}})
                page.get_by_label('Tool', exact=True).select_option('dodge')
                page.get_by_role('button', name='Gradient (G)', exact=True).click()
                surface = page.get_by_label('Retouch image interaction', exact=True)
                surface.scroll_into_view_if_needed()
                box = surface.bounding_box()
                page.mouse.move(box['x']+box['width']*.3, box['y']+box['height']*.5)
                page.mouse.down()
                page.mouse.move(box['x']+box['width']*.7, box['y']+box['height']*.5, steps=5)
                page.mouse.up()
                assert abs(float(page.get_by_label('Gradient start X (%)', exact=True).input_value())-30) < 1
                page.get_by_label('Outside shape', exact=True).check()
                page.get_by_label('Limit by brightness', exact=True).check()
                page.get_by_role('button', name='Midtones', exact=True).click()
                page.get_by_role('button', name='Preview selection mask', exact=True).click()
                expect(page.get_by_alt_text('Selection mask', exact=True)).to_be_visible()
                page.screenshot(path=str(output/f"{row['fileName']}-workspace.png"), full_page=True)
                page.get_by_role('button', name='Preview selection mask', exact=True).click()
                expect(page.get_by_alt_text('Retouched photograph', exact=True)).to_be_visible()
                page.get_by_role('button', name='Discard draft', exact=True).click()
                assert invoke('image_history', {'input': {'photoId': row['id']}}) == history
                page.get_by_role('button', name='Back to Develop', exact=True).click()
                print(f"PASS {row['fileName']}: desktop controls and disposable mask", flush=True)
            assert hashes == {p.name: digest(p.read_bytes()) for p in paths}
            report.update(status='passed', originalsUnchanged=True, desktopControlsPassed=True)
        except Exception as error:
            report.update(status='failed', error=str(error))
            raise
        finally:
            (output/'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')


if __name__ == '__main__':
    main()
