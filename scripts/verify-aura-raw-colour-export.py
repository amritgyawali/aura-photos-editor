"""Match an Internet reference to a supplied native RAW and verify the actual delivery."""
import argparse
import base64
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
from importlib.util import spec_from_file_location, module_from_spec

ROOT = Path(__file__).resolve().parents[1]
spec = spec_from_file_location('export_audit', ROOT / 'scripts/verify-aura-exported-colour.py')
audit = module_from_spec(spec)
spec.loader.exec_module(audit)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--port', type=int, default=9772)
    args = parser.parse_args()
    out = args.output.resolve()
    if out.exists():
        parser.error('Use a fresh evidence directory')
    (out / 'catalog').mkdir(parents=True)
    source = ROOT / 'output/aura-comprehensive-sidecars-retry-final-2026-10-22'
    earlier = audit.read(source / 'results.json')
    assert earlier['completed'] and earlier['originalsUnchanged']
    photo = next(p for p in earlier['photos'] if p['file'] == '989A2865.CR3')
    pid = photo['photoId']
    raw = Path('H:/AURA EDIT/989A2865.CR3')
    reference = ROOT / 'output/aura-internet-reference-2026-10-22/warm-cinematic.jpg'
    with sqlite3.connect(source / 'catalog/catalog.sqlite') as original:
        with sqlite3.connect(out / 'catalog/catalog.sqlite') as copied:
            original.backup(copied)
    report = dict(raw=str(raw), reference=str(reference), rawBeforeSha256=audit.digest(raw),
                  referenceSha256=audit.digest(reference), attempts=[], exports=[], pageErrors=[],
                  executableSha256=audit.digest(ROOT / 'target/desktop-launch/debug/aura-desktop.exe'),
                  scriptSha256=audit.digest(Path(__file__)))
    def persist():
        (out / 'results.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    def preview(pixels, name):
        image = Image.fromarray(pixels)
        image.thumbnail((1200, 1200), Image.Resampling.LANCZOS)
        image.save(out / name)
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
            page.set_default_timeout(600000)
            page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
            page.on('pageerror', lambda error: report['pageErrors'].append(str(error)))
            def call(command, **values):
                return page.evaluate('''async ([command,input])=>{
                    try{return await window.__TAURI_INTERNALS__.invoke(command,{input})}
                    catch(e){throw Error(JSON.stringify(e))}
                }''', [command, values])
            def recipe():
                return call('image_recipe', photoId=pid)
            def render():
                frame = call('render_image', photoId=pid, level='full', purpose='export')
                return np.frombuffer(base64.b64decode(frame['rgbBase64']), dtype=np.uint8).reshape(frame['height'], frame['width'], 3)
            collection = None
            # Resolve the imported photo's collection without assuming a report field or UI selection.
            for project in call('list_projects'):
                if any(p['id'] == pid for p in call('list_images', projectId=project['id'], offset=0, limit=240, orderBy=None)):
                    collection = project['id']
                    break
            assert collection
            baseline = recipe()
            before = render()
            report['nativeDimensions'] = dict(width=before.shape[1], height=before.shape[0])
            assert max(before.shape[:2]) >= 5000, 'This must be a native RAW decode, not its preview'
            preview(before, 'before-preview.png')
            del before
            ref = call('import_color_reference', path=str(reference), cancelId=str(uuid.uuid4()))
            prepared = call('prepare_color_match', photoId=pid, referenceId=ref['id'], cancelId=str(uuid.uuid4()))
            assert recipe()['recipeHash'] == baseline['recipeHash']
            report['analysis'] = audit.clean(prepared)
            print('PASS native RAW analysis; attempting guarded strengths', flush=True)
            for amount in [1, .5, .25, .1, 0]:
                controls = dict(prepared['controls'], amount=amount)
                old = recipe()['recipeHash']
                try:
                    matched = call('apply_color_match', photoId=pid, sessionId=prepared['sessionId'], controls=controls, cancelId=str(uuid.uuid4()))
                except Exception as error:
                    assert 'highlight or skin-colour check' in str(error) or 'farther from the reference' in str(error)
                    assert recipe()['recipeHash'] == old
                    report['attempts'].append(dict(amount=amount, accepted=False, error=str(error)))
                    persist()
                    continue
                report['attempts'].append(dict(amount=amount, accepted=True))
                break
            else:
                raise AssertionError('No removable native match')
            assert matched['current'] and matched['fullResolutionChecked']
            report['match'] = audit.clean(matched)
            expected = render()
            preview(expected, 'matched-preview.png')
            final_recipe = recipe()
            (out / 'matched.recipe.json').write_text(final_recipe['body'], encoding='utf-8')
            for key in ['geometry', 'retouch', 'masks']:
                assert json.loads(final_recipe['body']).get(key) == json.loads(baseline['body']).get(key)
            report['exportStatus'] = call('export_run', projectId=collection, destination=str(out / 'delivery'), destinationKind='folder',
                copyright=None, contact=None, creator=None, keywords=[], stripGps=True, stripCameraSerial=True, verify=True,
                sets=[dict(name=fmt, imageIds=[pid], format=fmt, quality=95, colour='srgb', bitDepth=8, resize='full',
                           sharpen='none', naming='{original}', sidecar=True) for fmt in ['jpeg', 'png']])
            manifest = audit.read(out / 'delivery/aura-delivery-manifest.json')
            assert manifest['verified'] and manifest['file_count'] == 2 and report['exportStatus']['sidecars'] == 2
            ref_rgb, info = load_srgb(reference)
            ref_descriptor = descriptor(ref_rgb)
            report['referenceInfo'] = info
            report['referenceAppearance'] = public_descriptor(ref_descriptor)
            del ref_rgb
            for entry in manifest['files']:
                path = out / 'delivery' / entry['path']
                with Image.open(path) as image:
                    image.verify()
                rgb, info = load_srgb(path)
                assert rgb.shape == expected.shape
                # Every PNG pixel is compared. JPEG Lab metrics use a declared 4-pixel grid
                # to bound memory for the full camera image; no resize is performed.
                exact = np.array_equal(rgb, expected)
                error = aligned_error(rgb if exact else rgb[::4, ::4], expected if exact else expected[::4, ::4])
                measured = descriptor(rgb)
                assert path.with_suffix('.xmp').read_text(encoding='utf-8') == final_recipe['body']
                report['exports'].append(dict(file=str(path), sha256=audit.digest(path), info=info,
                    versusRender=error, allPixelsCompared=path.suffix == '.png', jpegSamplingStride=4 if path.suffix == '.jpg' else None,
                    appearance=public_descriptor(measured), referenceDistributionGap=distribution_gap(measured, ref_descriptor),
                    sidecarMatchesRecipe=True))
                if path.suffix == '.png':
                    assert exact, 'Native lossless PNG differs from the export render'
                else:
                    preview(rgb, 'exported-jpeg-preview.png')
                    assert error['meanDeltaE76'] < 2 and error['p95DeltaE76'] < 5
                persist()
            assert recipe()['recipeHash'] == final_recipe['recipeHash']
            report['rawAfterSha256'] = audit.digest(raw)
            report['originalsUnchanged'] = report['rawAfterSha256'] == report['rawBeforeSha256'] and audit.digest(reference) == report['referenceSha256']
            report['completed'] = report['originalsUnchanged'] and not report['pageErrors']
            assert report['completed']
            print('PASS full-resolution RAW colour match, JPEG/PNG export and source hashes', flush=True)
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
