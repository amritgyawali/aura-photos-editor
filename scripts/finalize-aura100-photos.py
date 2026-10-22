"""Export saved qualification edits with the rebuilt desktop, without re-ingesting RAWs."""
import base64
import hashlib
import html
import json
import os
from pathlib import Path
import subprocess
import time
import urllib.request
import uuid

from PIL import Image
from playwright.sync_api import sync_playwright

from importlib.util import module_from_spec, spec_from_file_location

ROOT = Path(__file__).resolve().parents[1]
spec = spec_from_file_location('photo_audit', ROOT / 'scripts/audit-aura100-photos.py')
audit = module_from_spec(spec)
spec.loader.exec_module(audit)
BASE = ROOT / 'output/aura100-photo-baseline'
OUT = ROOT / 'output/aura100-final'
OUT.mkdir(parents=True, exist_ok=True)
baseline = json.loads((BASE / 'results.json').read_text(encoding='utf-8'))
result_path = OUT / 'results.json'
report = json.loads(result_path.read_text()) if result_path.exists() else {'photos': [], 'pageErrors': []}
report.update(projectId=baseline['projectId'], inputManifest=baseline['inputManifest'], completed=False)
exe = ROOT / 'target/desktop-launch/debug/aura-desktop.exe'
report['executableSha256'] = audit.digest(exe)
port = 9385
env = dict(os.environ, AURA_TEST_CATALOG=str(BASE / 'catalog/catalog.sqlite'),
           WEBVIEW2_USER_DATA_FOLDER=str(BASE / 'webview-final'),
           WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-port={port}', AURA_IMAGE_WORKERS='1')
startup = subprocess.STARTUPINFO()
startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
startup.wShowWindow = 0
proc = subprocess.Popen([str(exe)], cwd=ROOT, env=env, startupinfo=startup)


def persist():
    result_path.write_text(json.dumps(report, indent=2), encoding='utf-8')


def verify_exports(folder):
    entries = []
    for path in sorted(folder.rglob('*.jpg')):
        with Image.open(path) as image:
            image.verify()
        with Image.open(path) as image:
            entries.append(dict(file=str(path), width=image.width, height=image.height, sha256=audit.digest(path)))
    return entries


try:
    for _ in range(120):
        try:
            urllib.request.urlopen(f'http://127.0.0.1:{port}/json/version', timeout=1)
            break
        except OSError:
            time.sleep(1)
    else:
        raise RuntimeError('Rebuilt AURA did not start')
    with sync_playwright() as pw:
        page = pw.chromium.connect_over_cdp(f'http://127.0.0.1:{port}').contexts[0].pages[0]
        page.set_default_timeout(3600000)
        page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
        page.on('pageerror', lambda error: report['pageErrors'].append(str(error)))

        def call(command, **values):
            return page.evaluate('async ([command,input]) => {try{return await window.__TAURI_INTERNALS__.invoke(command,{input})}catch(error){throw new Error(JSON.stringify(error))}}', [command, values])

        def export(photo_id, folder):
            return call('export_run', projectId=baseline['projectId'], destination=str(folder), destinationKind='folder',
                        copyright=None, contact=None, creator=None, keywords=[], stripGps=True, stripCameraSerial=True,
                        verify=True, sets=[dict(name='edited', imageIds=[photo_id], format='jpeg', quality=95,
                        colour='srgb', bitDepth=8, resize='full', sharpen='none', naming='{original}', sidecar=True)])

        for original in baseline['photos']:
            prior = next((p for p in report['photos'] if p['photoId'] == original['photoId']), None)
            if prior and prior.get('passed') and all(Path(p['file']).is_file() and audit.digest(Path(p['file'])) == p['sha256'] for p in prior['exports']):
                print('RESUME', original['file'], flush=True)
                continue
            if prior:
                report['photos'].remove(prior)
            started = time.monotonic()
            item = dict(file=original['file'], photoId=original['photoId'])
            report['photos'].append(item)
            try:
                recipe = call('image_recipe', photoId=item['photoId'])
                assert recipe['recipeHash'] == original['recipeHash'], 'Saved recipe changed since qualification'
                (OUT / (item['file'] + '.recipe.json')).write_text(recipe['body'], encoding='utf-8')
                item['recipeHash'] = recipe['recipeHash']
                frame = call('render_image', photoId=item['photoId'], level='screen', screen=[1600, 1600], purpose='export')
                pixels = base64.b64decode(frame['rgbBase64'])
                item['previewSha256'] = hashlib.sha256(pixels).hexdigest()
                assert item['previewSha256'] == original['after']['pixelSha256'], 'Rebuilt preview differs from qualification'
                Image.frombytes('RGB', (frame['width'], frame['height']), pixels).save(OUT / (item['file'] + '-after.jpg'), quality=95)
                folder = OUT / 'full-resolution' / Path(item['file']).stem
                item['exportResult'] = export(item['photoId'], folder)
                item['exports'] = verify_exports(folder)
                assert len(item['exports']) == 1, 'One verified JPEG expected'
                assert max(item['exports'][0]['width'], item['exports'][0]['height']) > 1600, 'Full resolution expected'
                item['passed'] = True
                print('PASS FULL EXPORT', item['file'], item['exports'][0]['width'], item['exports'][0]['height'], round(time.monotonic()-started, 1), flush=True)
            except Exception as error:
                item.update(passed=False, error=str(error))
                print('FAIL', item['file'], str(error)[:300], flush=True)
            item['seconds'] = round(time.monotonic()-started, 3)
            persist()

        # A separate, labelled display-reference experiment; no claim of native Evoto parity.
        reference = ROOT / '.work-checks/evoto-next/runtime-color/Evoto-989A2698-watermarked-preview.png'
        target = next(p for p in baseline['photos'] if p['file'] == '989A2698.CR3')
        if reference.exists() and not report.get('displayReferenceExperiment', {}).get('completed'):
            experiment = {'reference': str(reference), 'referenceSha256': audit.digest(reference),
                          'scope': 'Watermarked resampled screenshot guidance; no native Evoto export or parity score.'}
            report['displayReferenceExperiment'] = experiment
            try:
                imported = call('import_color_reference', path=str(reference), cancelId=str(uuid.uuid4()))
                prepared = call('prepare_color_match', photoId=target['photoId'], referenceId=imported['id'], cancelId=str(uuid.uuid4()))
                experiment['prepare'] = prepared
                applied = call('apply_color_match', photoId=target['photoId'], sessionId=prepared['sessionId'],
                               controls=dict(amount=.65, tone=.65, color=.65, whiteBalance=.65), cancelId=str(uuid.uuid4()))
                experiment['apply'] = applied
                if applied['changed']:
                    frame = call('render_image', photoId=target['photoId'], level='screen', screen=[1600,1600], purpose='export')
                    image_path = OUT / '989A2698-screenshot-guided-approximation.jpg'
                    Image.frombytes('RGB', (frame['width'], frame['height']), base64.b64decode(frame['rgbBase64'])).save(image_path, quality=95)
                    experiment['preview'] = str(image_path)
                    recipe = call('image_recipe', photoId=target['photoId'])
                    (OUT / '989A2698-screenshot-guided-approximation.recipe.json').write_text(recipe['body'], encoding='utf-8')
                    experiment['export'] = export(target['photoId'], OUT / 'screenshot-guided-approximation')
                    experiment['exports'] = verify_exports(OUT / 'screenshot-guided-approximation')
                experiment['completed'] = True
            except Exception as error:
                experiment['error'] = str(error)
            finally:
                if call('image_recipe', photoId=target['photoId'])['recipeHash'] != target['recipeHash']:
                    call('history_step', projectId=baseline['projectId'], photoId=target['photoId'], action='undo')
                experiment['baselineRestored'] = call('image_recipe', photoId=target['photoId'])['recipeHash'] == target['recipeHash']
                assert experiment['baselineRestored'], 'Reference experiment must restore the baseline'
            print('DISPLAY REFERENCE', experiment.get('completed', False), flush=True)
            persist()

        report['originalsUnchanged'] = all(audit.digest(Path(item['file'])) == item['sha256'] for item in report['inputManifest'])
        report['completed'] = len(report['photos']) == 27 and all(p.get('passed') for p in report['photos']) and report['originalsUnchanged']
        cards = []
        for item in report['photos']:
            filename = html.escape(item['file'])
            before = '../aura100-photo-baseline/' + item['file'] + '-before.jpg'
            after = item['file'] + '-after.jpg'
            exported = item.get('exports', [{}])[0].get('file', '')
            target_link = Path(exported).relative_to(OUT).as_posix() if exported else after
            cards.append(f'<article><h2>{filename}</h2><div><figure><img loading="lazy" src="{before}"><figcaption>Before</figcaption></figure><figure><a href="{target_link}"><img loading="lazy" src="{after}"></a><figcaption>AURA edit · click for full resolution</figcaption></figure></div></article>')
        (OUT / 'index.html').write_text('<!doctype html><meta charset="utf-8"><title>AURA photo qualification</title><style>body{background:#15161a;color:#eee;font:16px system-ui;margin:24px}article{margin:32px 0}article>div{display:flex;gap:16px}figure{margin:0;flex:1}img{width:100%;max-height:75vh;object-fit:contain;background:#222}figcaption{text-align:center}h2{font-size:18px}a{color:#9cf}</style><h1>AURA · 27 RAW edits</h1><p>Original files preserved. JPEG exports use the saved automatic edits. Creative lab tools remain explicitly selected. Evoto exact output matching is unverified.</p>' + ''.join(cards), encoding='utf-8')
except Exception as error:
    report['fatalError'] = str(error)
    raise
finally:
    persist()
    proc.terminate()
    proc.wait(timeout=30)
