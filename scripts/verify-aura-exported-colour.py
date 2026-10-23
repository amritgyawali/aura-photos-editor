"""Measure delivered files, tune existing AURA match strengths and export verified copies."""
import argparse
import base64
import copy
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import time
import urllib.request
import uuid

import numpy as np
from PIL import Image
from playwright.sync_api import sync_playwright

from aura_colour_metrics import aligned_error, descriptor, distribution_gap, load_srgb, public_descriptor

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def read(path):
    return json.loads(path.read_text(encoding='utf-8'))


def clean(report):
    value = copy.deepcopy(report)
    if value.get('reference'):
        value['reference'].pop('thumbnail', None)
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, default=ROOT / 'output/aura-internet-reference-2026-10-22/ui-qualified')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--port', type=int, default=9771)
    args = parser.parse_args()
    source, out = args.source.resolve(), args.output.resolve()
    if out.exists():
        parser.error('Use a fresh output directory to retain previous evidence')
    (out / 'catalog').mkdir(parents=True)
    earlier = read(source / 'results.json')
    assert earlier['completed'] and earlier['originalsUnchanged']
    with sqlite3.connect(source / 'catalog/qualified.sqlite') as original:
        with sqlite3.connect(out / 'catalog/catalog.sqlite') as destination:
            original.backup(destination)
    sources = [Path(r['file']) for r in earlier['downloadedReferences']]
    sources += [source / 'incoming' / p['fileName'] for p in earlier['photos']]
    hashes = {str(path): digest(path) for path in sources}
    report = dict(existingExports=[], matches=[], pageErrors=[], sources=hashes,
                  sourceRun=str(source), executableSha256=digest(ROOT / 'target/desktop-launch/debug/aura-desktop.exe'),
                  scriptSha256=digest(Path(__file__)), metricsSha256=digest(ROOT / 'scripts/aura_colour_metrics.py'))

    def persist():
        (out / 'results.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')

    refs = {}
    for reference in earlier['downloadedReferences']:
        rgb, info = load_srgb(reference['file'])
        description = descriptor(rgb)
        refs[reference['name']] = (reference, description)
        delivery_root = source / ('export-' + reference['name'])
        manifest = read(delivery_root / 'aura-delivery-manifest.json')
        for photo, file in zip(earlier['photos'], manifest['files']):
            frame = source / f'{reference["name"]}-{photo["fileName"]}-matched.png'
            exported = delivery_root / file['path']
            expected, _ = load_srgb(frame)
            actual, export_info = load_srgb(exported)
            measured = descriptor(actual)
            row = dict(reference=reference, target=photo['fileName'], file=str(exported), referenceInfo=info,
                       exportInfo=export_info, exportVersusSavedRender=aligned_error(actual, expected),
                       exportedAppearance=public_descriptor(measured), referenceAppearance=public_descriptor(description),
                       referenceDistributionGap=distribution_gap(measured, description), fileSha256=digest(exported))
            report['existingExports'].append(row)
            print('MEASURED previous export', reference['name'], photo['fileName'],
                  'mean CIE76 vs render', round(row['exportVersusSavedRender']['meanDeltaE76'], 4), flush=True)
            persist()

    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    process = subprocess.Popen([str(ROOT / 'target/desktop-launch/debug/aura-desktop.exe')], cwd=ROOT,
        startupinfo=startup, env=dict(os.environ, AURA_TEST_CATALOG=str(out / 'catalog/catalog.sqlite'),
        AURA_IMAGE_WORKERS='1', WEBVIEW2_USER_DATA_FOLDER=str(out / 'webview'),
        WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-port={args.port}'))
    try:
        for _ in range(120):
            try:
                urllib.request.urlopen(f'http://127.0.0.1:{args.port}/json/version', timeout=1)
                break
            except OSError:
                time.sleep(1)
        else:
            raise TimeoutError('Desktop did not start')
        with sync_playwright() as pw:
            page = pw.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}').contexts[0].pages[0]
            page.set_default_timeout(180000)
            page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
            page.on('pageerror', lambda error: report['pageErrors'].append(str(error)))

            def call(command, **values):
                if command == 'color_match_status':
                    return page.evaluate('async p=>window.__TAURI_INTERNALS__.invoke("color_match_status",{photoId:p})', values['photoId'])
                return page.evaluate('''async ([command,input])=>{
                    try{return await window.__TAURI_INTERNALS__.invoke(command,{input})}
                    catch(e){throw Error(JSON.stringify(e))}
                }''', [command, values])

            def recipe(pid):
                return call('image_recipe', photoId=pid)

            def render(pid, path):
                frame = call('render_image', photoId=pid, level='full', purpose='export')
                rgb = np.frombuffer(base64.b64decode(frame['rgbBase64']), dtype=np.uint8).reshape(frame['height'], frame['width'], 3)
                Image.fromarray(rgb).save(path)
                return rgb

            imported = call('list_color_references')
            defaults = dict(amount=1, tone=1, color=1, whiteBalance=1,
                            detail=dict.fromkeys(['exposure', 'contrast', 'highlights', 'shadows', 'whites', 'blacks',
                                                 'curves', 'temperature', 'tint', 'vibrance', 'saturation', 'hue'], 1))
            for reference in earlier['downloadedReferences']:
                name = reference['name']
                ref_id = next(r['id'] for r in imported if r['name'] == Path(reference['file']).name)
                for photo in earlier['photos']:
                    pid = photo['id']
                    saved = call('color_match_status', photoId=pid)
                    assert saved['current']
                    call('apply_color_match', photoId=pid, sessionId=saved['sessionId'], controls=dict(saved['controls'], amount=0), cancelId=str(uuid.uuid4()))
                    before_path = out / f'{name}-{photo["fileName"]}-before.png'
                    before_pixels = render(pid, before_path)
                    old_baseline, _ = load_srgb(source / (photo['fileName'] + '-before.png'))
                    assert np.array_equal(before_pixels, old_baseline), 'The fresh match must start from the same original correction'
                    baseline = recipe(pid)
                    analysis = call('prepare_color_match', photoId=pid, referenceId=ref_id, cancelId=str(uuid.uuid4()))
                    assert recipe(pid)['recipeHash'] == baseline['recipeHash']
                    row = dict(reference=reference, target=photo['fileName'], photoId=pid, attempts=[], analysis=clean(analysis), before=str(before_path))
                    report['matches'].append(row)

                    def apply(controls):
                        old_hash, old_history = recipe(pid)['recipeHash'], call('image_history', photoId=pid)
                        try:
                            result = call('apply_color_match', photoId=pid, sessionId=analysis['sessionId'],
                                          controls=controls, cancelId=str(uuid.uuid4()))
                        except Exception as error:
                            assert 'highlight or skin-colour check' in str(error) or 'farther from the reference' in str(error), str(error)
                            assert recipe(pid)['recipeHash'] == old_hash and call('image_history', photoId=pid) == old_history
                            row['attempts'].append(dict(controls=copy.deepcopy(controls), accepted=False, error=str(error), noEditSaved=True))
                            persist()
                            return None
                        assert result['fullResolutionChecked'] and result['current']
                        row['attempts'].append(dict(controls=copy.deepcopy(controls), accepted=True, match=clean(result)))
                        persist()
                        return result

                    best_controls = copy.deepcopy(defaults)
                    best = apply(best_controls)
                    assert best is not None
                    row['default'] = clean(best)
                    # Bounded coordinate search through the existing AURA controls. All
                    # candidates use the same immutable analysis baseline and native guards.
                    for key in ['tone', 'color', 'whiteBalance', 'amount']:
                        for value in [.5, 1.5, 2]:
                            candidate = copy.deepcopy(best_controls)
                            candidate[key] = value
                            result = apply(candidate)
                            if result and result['afterDistance'] + .0001 < best['afterDistance']:
                                best, best_controls = result, candidate
                        print('CHECKED', name, photo['fileName'], key, 'best descriptor', round(best['afterDistance'], 4), flush=True)
                    final = apply(best_controls)
                    assert final and final['afterDistance'] <= row['default']['afterDistance'] + .0001
                    row.update(best=clean(final), bestControls=best_controls)
                    rendered_path = out / f'{name}-{photo["fileName"]}-render.png'
                    expected = render(pid, rendered_path)
                    row['render'] = str(rendered_path)
                    final_recipe = recipe(pid)
                    for key in ['geometry', 'retouch', 'masks']:
                        assert json.loads(final_recipe['body']).get(key) == json.loads(baseline['body']).get(key)
                    (out / f'{name}-{photo["fileName"]}.recipe.json').write_text(final_recipe['body'], encoding='utf-8')
                    destination = out / f'export-{name}-{Path(photo["fileName"]).stem}'
                    status = call('export_run', projectId=earlier['projectId'], destination=str(destination), destinationKind='folder',
                        copyright=None, contact=None, creator=None, keywords=[], stripGps=True, stripCameraSerial=True, verify=True,
                        sets=[dict(name=fmt, imageIds=[pid], format=fmt, quality=95, colour='srgb', bitDepth=8,
                                   resize='full', sharpen='none', naming='{original}', sidecar=True) for fmt in ['jpeg', 'png']])
                    assert status['written'] == 2 and status['sidecars'] == 2, status
                    manifest = read(destination / 'aura-delivery-manifest.json')
                    assert manifest['verified'] and manifest['file_count'] == 2
                    assert recipe(pid)['recipeHash'] == final_recipe['recipeHash'], 'Export changed the edit'
                    row['exports'] = []
                    for entry in manifest['files']:
                        path = destination / entry['path']
                        with Image.open(path) as image:
                            image.verify()
                        pixels, info = load_srgb(path)
                        errors = aligned_error(pixels, expected)
                        measured = descriptor(pixels)
                        sidecar = path.with_suffix('.xmp')
                        assert sidecar.read_text(encoding='utf-8') == final_recipe['body']
                        item = dict(file=str(path), sha256=digest(path), info=info, versusRender=errors,
                                    referenceDistributionGap=distribution_gap(measured, refs[name][1]),
                                    exportedAppearance=public_descriptor(measured), sidecarMatchesRecipe=True)
                        row['exports'].append(item)
                        persist()
                        if path.suffix == '.png':
                            assert errors['exactPixels'], 'Lossless sRGB PNG differs from the native export render'
                        else:
                            assert errors['meanDeltaE76'] < 2 and errors['p95DeltaE76'] < 5, 'JPEG export exceeded the declared comparison tolerances'
                    row['passed'] = True
                    print('PASS exported JPEG/PNG', name, photo['fileName'], 'default', row['default']['afterDistance'], 'selected', final['afterDistance'], flush=True)
                    persist()
            with sqlite3.connect(out / 'catalog/catalog.sqlite') as current:
                with sqlite3.connect(out / 'catalog/qualified.sqlite') as qualified:
                    current.backup(qualified)
            report['originalsUnchanged'] = all(digest(path) == value for path, value in hashes.items())
            report['completed'] = all(row.get('passed') for row in report['matches']) and len(report['matches']) == 4 and report['originalsUnchanged'] and not report['pageErrors']
            assert report['completed']
            persist()
    except Exception as error:
        report['fatalError'] = str(error)
        raise
    finally:
        persist()
        if process.poll() is None:
            process.terminate()
        process.wait(timeout=30)


if __name__ == '__main__':
    main()
