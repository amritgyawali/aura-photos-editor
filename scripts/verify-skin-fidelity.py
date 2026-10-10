"""Verify automatic retouch in a running native desktop, without editing input pixels.

python scripts/verify-skin-fidelity.py --output OUTPUT --endpoint http://127.0.0.1:9339 PHOTO...
Uses a new test collection. --export checks full-size PNGs against native full renders.
Pillow, NumPy and Playwright are required. Comparisons measure retouch separately from grade.
"""
import argparse
import base64
import hashlib
import json
import time
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw
from playwright.sync_api import sync_playwright


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('photos', nargs='+', type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--endpoint', default='http://127.0.0.1:9339')
    parser.add_argument('--export', action='store_true')
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    sources = {p.name: p.resolve() for p in args.photos}
    assert len(sources) == len(args.photos), 'Use distinct filenames'
    digest = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
    hashes = {name: digest(p) for name, p in sources.items()}
    report = {'originalHashes': hashes, 'photos': [], 'status': 'running'}

    def save_report():
        (out / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')

    with sync_playwright() as pw:
        browser = pw.chromium.connect_over_cdp(args.endpoint)
        page = next(p for c in browser.contexts for p in c.pages if 'tauri' in p.url or 'localhost' in p.url)
        page.set_default_timeout(300000)

        def call(command, **input):
            return page.evaluate('async ([c,a])=>{try{return await window.__TAURI_INTERNALS__.invoke(c,{input:a})}catch(e){throw new Error(JSON.stringify(e))}}', [command, input])

        def image(dto):
            return Image.frombytes('RGB', (dto['width'], dto['height']), base64.b64decode(dto['rgbBase64']))

        def preview(photo, before=False):
            return image(page.evaluate('async ([p,ph,b])=>await window.__TAURI_INTERNALS__.invoke("native_retouch_preview",{projectId:p,photoId:ph,before:b})', [project, photo, before]))

        project = call('create_project', name='Skin fidelity ' + out.name + ' ' + time.strftime('%H-%M-%S'), coupleNames=None, eventDate=None)['id']
        report['projectId'] = project
        call('start_ingest', projectId=project, roots=[str(p) for p in sources.values()])
        deadline = time.monotonic() + 180
        rows = []
        while len(rows) < len(sources) and time.monotonic() < deadline:
            rows = call('list_images', projectId=project, offset=0, limit=100, orderBy='timeline')
            time.sleep(.3)
        assert len(rows) == len(sources), 'Import did not finish'
        for row in rows:
            photo, name = row['id'], row['fileName']
            started = time.monotonic()
            # Exercise the exact one-click path used for new photographs.
            auto = call('enhance_photo', photoId=photo)
            auto_body = json.loads(auto['body'])
            assert call('enhance_photo', photoId=photo)['recipeHash'] == auto['recipeHash'], 'One-click repeat changed the recipe'
            record = {'id': photo, 'file': name, 'oneClickRepeat': True,
                      'global': auto_body['global'], 'scene': auto_body['studio_portrait_auto_v1'].get('scene')}
            automatic = preview(photo)
            automatic.save(out / f'{Path(name).stem}-one-click.png')
            original = Image.open(sources[name]).convert('RGB').resize(automatic.size, Image.Resampling.LANCZOS)
            auto_pair = Image.new('RGB', (automatic.width * 2, automatic.height))
            auto_pair.paste(original); auto_pair.paste(automatic, (automatic.width, 0))
            auto_pair.save(out / f'{Path(name).stem}-one-click-comparison.jpg', quality=95)
            # Isolate retouch so its colour fidelity is not confused with exposure/white balance.
            call('history_step', projectId=project, photoId=photo, action='reset_original')
            options = {'scope': 'face_and_body', 'adaptive': True}
            dto = call('auto_retouch', projectId=project, photoId=photo, options=options, **{'global': False})
            body = json.loads(dto['body'])
            analysis = body['studio_portrait_auto_v1']
            edits = body.get('studio_retouch_v1', [])
            before, after = preview(photo, True), preview(photo)
            before.save(out / f'{Path(name).stem}-before.png')
            after.save(out / f'{Path(name).stem}-after.png')
            record.update(recipeHash=dto['recipeHash'], analysis=analysis, operations=len(edits))
            (out / f'{Path(name).stem}-recipe.json').write_text(json.dumps(body, indent=2), encoding='utf-8')
            signatures = [e.get('expert', {}).get('adjusted') for e in analysis['assessments'] if e.get('expert')]
            record['adaptedSettings'] = signatures
            for edit in edits:
                if edit['tool'] != 'skin_smooth' or '-body-' in edit['id']:
                    continue
                sel = call('native_retouch_selection_preview', projectId=project, photoId=photo, edit=edit, replaceId=edit['id'])
                mask = np.asarray(image(sel).convert('L')) >= 180
                a, b = np.asarray(before) / 255., np.asarray(after) / 255.
                if mask.sum() < 20:
                    continue
                decode = lambda v: np.where(v <= .04045, v / 12.92, ((v + .055) / 1.055) ** 2.4)
                la, lb = decode(a), decode(b)
                ca = la / np.maximum(la.sum(axis=2, keepdims=True), 1e-6)
                cb = lb / np.maximum(lb.sum(axis=2, keepdims=True), 1e-6)
                drift = (cb - ca)[mask]
                record.setdefault('skinMeasurements', []).append({
                    'operation': edit['id'], 'pixels': int(mask.sum()),
                    'meanChromaDrift': float(np.linalg.norm(drift.mean(axis=0))),
                    'medianChromaChange': float(np.median(np.linalg.norm(drift, axis=1))),
                    'meanEncodedLumaDrift': float(((b - a) @ np.array([.2126, .7152, .0722]))[mask].mean()),
                })
            pair = Image.new('RGB', (after.width * 2, after.height + 24), 'white')
            pair.paste(before, (0, 24)); pair.paste(after, (after.width, 24))
            ImageDraw.Draw(pair).text((5, 4), name + '    Original / automatic retouch', fill='black')
            pair.save(out / f'{Path(name).stem}-comparison.jpg', quality=95)
            for i, face in enumerate(analysis['faces']):
                l, t, r, b = face['bounds']
                box = (int(l * after.width), int(t * after.height), int(r * after.width), int(b * after.height))
                first, second = before.crop(box), after.crop(box)
                detail = Image.new('RGB', (first.width * 2, first.height))
                detail.paste(first); detail.paste(second, (first.width, 0))
                detail.save(out / f'{Path(name).stem}-face-{i + 1}.png')
            assert call('auto_retouch', projectId=project, photoId=photo, options=options, **{'global': False})['recipeHash'] == dto['recipeHash'], 'Retouch repeat changed the recipe'
            call('history_step', projectId=project, photoId=photo, action='reset_original')
            assert preview(photo).tobytes() == before.tobytes(), 'Reset did not restore original'
            call('history_step', projectId=project, photoId=photo, action='undo')
            assert call('image_recipe', photoId=photo)['recipeHash'] == dto['recipeHash']
            call('history_step', projectId=project, photoId=photo, action='redo')
            assert preview(photo).tobytes() == before.tobytes()
            call('history_step', projectId=project, photoId=photo, action='undo')
            assert preview(photo).tobytes() == after.tobytes(), 'Undo did not restore saved retouch'
            record.update(retouchRepeat=True, resetUndoRedo=True, seconds=round(time.monotonic() - started, 2))
            report['photos'].append(record)
            save_report()
            print(name, 'faces', analysis['detectedFaces'], 'operations', len(edits), 'skin', record.get('skinMeasurements', []), flush=True)
        if args.export:
            exported = call('export_run', projectId=project, destination=str(out / 'exports'), destinationKind='folder',
                            copyright=None, contact=None, creator=None, keywords=[], stripGps=True, stripCameraSerial=True, verify=True,
                            sets=[{'name': 'skin-fidelity', 'imageIds': [r['id'] for r in rows], 'format': 'png', 'quality': 95,
                                   'colour': 'srgb', 'bitDepth': 8, 'resize': 'full', 'sharpen': 'none', 'naming': '{original}', 'sidecar': False}])
            assert exported['written'] == exported['verified'] == len(rows)
            assert exported['corrupt'] == exported['renderFailed'] == 0
            report['export'] = exported
            files = page.evaluate('async p=>await window.__TAURI_INTERNALS__.invoke("export_files",{projectId:p})', project)
            for row in report['photos']:
                delivered = next(f for f in files if f['imageId'] == row['id'])
                png = Image.open(out / 'exports' / delivered['path']).convert('RGB')
                original = Image.open(sources[row['file']])
                assert png.size == original.size, 'Export used preview dimensions'
                full = image(call('render_image', photoId=row['id'], level='full', purpose='export', colourSpace='srgb'))
                assert png.tobytes() == full.tobytes(), 'Export differs from native full-size render'
                call('history_step', projectId=project, photoId=row['id'], action='reset_original')
                full_before = image(call('render_image', photoId=row['id'], level='full', purpose='export', colourSpace='srgb'))
                call('history_step', projectId=project, photoId=row['id'], action='undo')
                assert call('image_recipe', photoId=row['id'])['recipeHash'] == row['recipeHash']
                first, second = np.asarray(full_before), np.asarray(full)
                yy, xx = np.mgrid[0:full.height, 0:full.width]
                protected = np.zeros((full.height, full.width), dtype=bool)
                for i, face in enumerate(row['analysis']['faces']):
                    eyes = np.asarray(face['landmarks'][:2]) * np.array([full.width, full.height])
                    radius = np.linalg.norm(eyes[0] - eyes[1]) * .09
                    for eye in eyes:
                        protected |= (xx - eye[0]) ** 2 + (yy - eye[1]) ** 2 <= radius ** 2
                    l, t, r, b = face['bounds']
                    box = (int(l * full.width), int(t * full.height), int(r * full.width), int(b * full.height))
                    a, b = full_before.crop(box), full.crop(box)
                    pair = Image.new('RGB', (a.width * 2, a.height))
                    pair.paste(a); pair.paste(b, (a.width, 0))
                    pair.save(out / f"{Path(row['file']).stem}-full-face-{i + 1}.png")
                assert np.array_equal(first[protected], second[protected]), 'Protected eye detail changed at full resolution'
                row['protectedEyePixelsUnchanged'] = int(protected.sum())
                row.update(fullExportMatches=True, exportDimensions=list(png.size))
                save_report()
        # Exercise manual protection on a body operation carrying the face-exclusion matte.
        candidate = next((r for r in report['photos'] if r['file'] != 'female-acne-unsplash.jpg'
                          and any('-body-' in e['id'] for e in json.loads((out / f"{Path(r['file']).stem}-recipe.json").read_text()).get('studio_retouch_v1', []))), None)
        if candidate:
            photo = candidate['id']
            ops = call('native_retouch_edit', projectId=project, photoId=photo, action='list', edits=[], id=None)
            chosen = next(e for e in ops if '-body-' in e['id'] and e['tool'] == 'skin_smooth')
            original_id = chosen['id']
            chosen.update(amount=.27, enabled=False)
            saved = call('native_retouch_edit', projectId=project, photoId=photo, action='update', edits=[chosen], id=None)
            manual = next(e for e in saved if e['id'] == 'manual-' + original_id)
            mask = call('native_retouch_selection_preview', projectId=project, photoId=photo,
                        edit={**manual, 'enabled': True}, replaceId=manual['id'])
            for _ in range(2):
                call('auto_retouch', projectId=project, photoId=photo, options={'scope': 'face_and_body', 'adaptive': True}, **{'global': False})
                actual = call('native_retouch_edit', projectId=project, photoId=photo, action='list', edits=[], id=None)
                assert next(e for e in actual if e['id'] == manual['id']) == manual
                assert not any(e['id'] == original_id for e in actual)
                again = call('native_retouch_selection_preview', projectId=project, photoId=photo,
                             edit={**manual, 'enabled': True}, replaceId=manual['id'])
                assert again['rgbBase64'] == mask['rgbBase64'], 'Manual body mask changed'
            report['manualBodyMaskPreserved'] = candidate['file']
        assert hashes == {name: digest(p) for name, p in sources.items()}, 'An original changed'
        report.update(status='passed', originalsUnchanged=True)
        save_report()
        print('PASS native import, repeat, reset/Undo/Redo, originals and requested exports', flush=True)


if __name__ == '__main__':
    main()
