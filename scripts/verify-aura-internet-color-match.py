"""Exercise the real colour-reference UI with downloaded original-resolution references."""
import argparse
import base64
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import sqlite3
import subprocess
import sys
import threading
import time
import urllib.request

from PIL import Image
from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parents[1]
LABELS = {'amount': 'Match amount', 'tone': 'Tone match', 'color': 'Colour match', 'whiteBalance': 'Warmth match'}
FINE_LABELS = {key: label + ' match' for key, label in [('exposure', 'Exposure'), ('contrast', 'Contrast'),
    ('highlights', 'Highlights'), ('shadows', 'Shadows'), ('whites', 'Whites'), ('blacks', 'Blacks'),
    ('curves', 'Curve'), ('temperature', 'Temperature'), ('tint', 'Tint'), ('vibrance', 'Vibrance'),
    ('saturation', 'Saturation'), ('hue', 'Hue')]}


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--references', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--port', type=int, default=9760)
    args = parser.parse_args()
    out = args.output.resolve()
    if out.exists():
        parser.error('Use a fresh evidence directory')
    (out / 'catalog').mkdir(parents=True)
    (out / 'incoming').mkdir()
    manifest = json.loads((args.references / 'download-manifest.json').read_text())
    references = [Path(item['file']) for item in manifest]
    assert all(digest(path) == item['sha256'] for path, item in zip(references, manifest))
    inputs = [ROOT / f'output/aura-comprehensive-retry-2026-10-22/{name}.CR3-before.jpg' for name in ['989A2698', '989A2865']]
    original_hashes = {str(path): digest(path) for path in [*inputs, *references]}
    for index, path in enumerate(inputs):
        with Image.open(path) as image:
            image = image.convert('RGB')
            image.thumbnail((1024, 1024))
            image.save(out / f'incoming/target-{index}.jpg', quality=98)
    report = dict(steps=[], matches=[], pageErrors=[], downloadedReferences=manifest,
                  executableSha256=digest(ROOT / 'target/desktop-launch/debug/aura-desktop.exe'),
                  scriptSha256=digest(Path(__file__)), selectorSha256=digest(ROOT / 'scripts/select-native-reference.py'))

    def persist():
        (out / 'results.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')

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
            expect.set_options(timeout=180000)
            page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
            page.on('pageerror', lambda error: report['pageErrors'].append(str(error)))
            def call(command, **values):
                if command == 'color_match_status':
                    return page.evaluate('async photoId => window.__TAURI_INTERNALS__.invoke("color_match_status",{photoId})', values['photoId'])
                return page.evaluate('async ([command,input]) => window.__TAURI_INTERNALS__.invoke(command,{input})', [command, values])

            def step(name, action):
                started = time.monotonic()
                try:
                    value = action()
                    report['steps'].append(dict(name=name, passed=True, result=value, seconds=round(time.monotonic() - started, 2)))
                    page.screenshot(path=str(out / f'{len(report["steps"]):02d}.png'))
                    print('PASS', name, flush=True)
                    persist()
                    return value
                except Exception as error:
                    report['steps'].append(dict(name=name, passed=False, error=str(error)))
                    persist()
                    raise

            def nav(name):
                page.locator('.studio-nav').get_by_role('button', name=re.compile('^' + re.escape(name))).click()

            def picker(button, paths, title, cancel=False):
                helper = subprocess.Popen([sys.executable, str(ROOT / 'scripts/select-native-reference.py'),
                    *map(str, paths), '--pid', str(process.pid), '--title', title, *(['--cancel'] if cancel else [])],
                    stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
                # Bound a failed OS-dialog interaction to this owned test process.
                watchdog = threading.Timer(60, lambda: process.terminate() if process.poll() is None else None)
                watchdog.start()
                try:
                    button.click()
                    text = helper.communicate(timeout=40)[0]
                    assert helper.returncode == 0, text
                    return text
                finally:
                    watchdog.cancel()
                    if helper.poll() is None:
                        helper.terminate()
                        helper.wait(timeout=10)

            name = 'Downloaded reference matching verification'
            nav('Start')
            page.get_by_label('New collection', exact=True).fill(name)
            page.get_by_role('button', name='Create', exact=True).click()
            project = next(p for p in call('list_projects') if p['name'] == name)['id']
            nav('Photos')
            page.get_by_text('Enter folders manually', exact=True).click()
            page.get_by_label('Card or folder', exact=True).fill(str(out / 'incoming'))
            page.get_by_role('button', name='Add folder', exact=True).click()
            page.get_by_role('button', name='Start import', exact=True).click()
            nav('Auto edit')
            expect(page.get_by_role('button', name='Retouch', exact=True)).to_be_enabled()
            photos = call('list_images', projectId=project, offset=0, limit=10, orderBy=None)
            assert len(photos) == 2
            photos.sort(key=lambda p: p['fileName'])
            report.update(projectId=project, photos=photos)
            for photo in photos:
                page.get_by_role('option', name=photo['fileName'], exact=True).click()
                editor = page.get_by_role('region', name='Photo editor', exact=True)
                expect(editor.get_by_role('button', name='Auto enhance photo', exact=True)).to_be_enabled()
                editor.get_by_role('button', name='Auto enhance photo', exact=True).click()
                expect(editor.get_by_role('button', name='Auto enhance photo', exact=True)).to_be_enabled()
            step('Create collection, import targets and save initial corrections', lambda: [p['fileName'] for p in photos])
            nav('Match a look')
            expect(page.get_by_role('heading', name='Match a colour reference.', exact=True)).to_be_visible()
            panel = page.get_by_role('region', name='Reference colour matching', exact=True)
            upload = panel.get_by_role('button', name='Upload colour reference', exact=True)
            expect(upload).to_be_enabled()
            step('Single-image matching is directly available in Match a look', lambda: upload.is_visible())

            def cancel_upload():
                before = call('list_color_references')
                picker(upload, [references[0]], 'Choose an edited colour reference', cancel=True)
                expect(upload).to_be_enabled()
                assert call('list_color_references') == before
                return dict(libraryUnchanged=True)
            step('Cancel native upload without importing another file', cancel_upload)
            step('Upload original 4000 by 6000 reference through the native Windows picker', lambda:
                 picker(upload, [references[0]], 'Choose an edited colour reference'))
            expect(panel.get_by_label('Saved colour reference').get_by_role('option', name=references[0].name, exact=True)).to_have_count(1)
            step('Upload several references through the native Windows picker', lambda:
                 picker(panel.get_by_role('button', name='Upload several colour references', exact=True), references,
                        'Choose edited colour references'))
            expect(panel.get_by_label('Saved colour reference').get_by_role('option', name=references[1].name, exact=True)).to_have_count(1)
            imported = call('list_color_references')
            assert len(imported) == 2
            for entry in manifest:
                saved = next(r for r in imported if r['name'] == Path(entry['file']).name)
                assert (saved['width'], saved['height']) == (entry['width'], entry['height'])

            def recipe(photo):
                return call('image_recipe', photoId=photo)

            def render(photo, path):
                frame = call('render_image', photoId=photo, level='full', purpose='export')
                pixels = base64.b64decode(frame['rgbBase64'])
                Image.frombytes('RGB', (frame['width'], frame['height']), pixels).save(path)
                return hashlib.sha256(pixels).hexdigest()

            baselines = {p['id']: (recipe(p['id']), render(p['id'], out / (p['fileName'] + '-before.png'))) for p in photos}

            def apply_ui(controls=None, remove=False, fine=None):
                if controls:
                    for key, value in controls.items():
                        panel.get_by_role('slider', name=LABELS[key], exact=True).fill(str(round(value * 100)))
                if fine is not None:
                    details = panel.locator('details').filter(has=page.locator('summary', has_text=re.compile('^Fine matching controls$')))
                    if details.get_attribute('open') is None:
                        details.locator('summary').click()
                    for key, value in fine.items():
                        panel.get_by_role('slider', name=FINE_LABELS[key], exact=True).fill(str(round(value * 100)))
                selected = next(p['id'] for p in photos if page.get_by_role('option', name=p['fileName'], exact=True).get_attribute('aria-selected') == 'true')
                desired = {key: int(panel.get_by_role('slider', name=label, exact=True).input_value()) / 100 for key, label in LABELS.items()}
                desired['detail'] = {key: int(panel.get_by_role('slider', name=label, exact=True, include_hidden=True).input_value()) / 100 for key, label in FINE_LABELS.items()}
                if remove:
                    desired['amount'] = 0
                panel.get_by_role('button', name='Remove matched treatment' if remove else 'Apply colour strengths', exact=True).click()
                page.wait_for_function('''async ([photoId,wanted]) => {
                    if(document.querySelector('.color-match-panel')?.getAttribute('aria-busy')==='true')return false;
                    if([...document.querySelectorAll('[role="alert"]')].some(e=>e.getClientRects().length&&e.textContent.includes('Edit could not be saved:')))return true;
                    const saved=await window.__TAURI_INTERNALS__.invoke('color_match_status',{photoId});
                    return saved?.current&&saved.fullResolutionChecked&&Object.entries(wanted).every(([key,value])=>
                        key==='detail'?Object.entries(value).every(([field,strength])=>Math.abs(saved.controls.detail[field]-strength)<.00001):Math.abs(saved.controls[key]-value)<.00001);
                }''', arg=[selected, desired])
                expect(upload).to_be_enabled()
                alerts = page.get_by_role('alert').all_text_contents()
                errors = [text for text in alerts if 'Edit could not be saved:' in text]
                if errors:
                    return dict(ok=False, error=' '.join(errors))
                status = call('color_match_status', photoId=selected)
                return dict(ok=True, result={key: value for key, value in status.items() if key not in ['reference', 'changed']})

            for reference in imported:
                style = Path(reference['name']).stem
                for photo in photos:
                    pid = photo['id']
                    page.get_by_role('option', name=photo['fileName'], exact=True).click()
                    expect(upload).to_be_enabled()
                    panel.get_by_label('Saved colour reference').select_option(reference['id'])
                    history = call('image_history', photoId=pid)
                    panel.get_by_role('button', name='Analyse reference match', exact=True).click()
                    expect(panel.get_by_role('button', name='Apply colour strengths', exact=True)).to_be_enabled()
                    expect(upload).to_be_enabled()
                    assert call('image_history', photoId=pid) == history
                    row = dict(reference=reference['name'], target=photo['fileName'], attempts=[])
                    report['matches'].append(row)
                    controls = dict(amount=1, tone=1, color=1, whiteBalance=1)
                    for amount in [1, .5, .25, .1, 0]:
                        controls['amount'] = amount
                        before = recipe(pid)['recipeHash']
                        attempt = apply_ui(controls)
                        row['attempts'].append(attempt)
                        if attempt['ok']:
                            break
                        assert recipe(pid)['recipeHash'] == before
                        assert 'skin-colour' in json.dumps(attempt['error']) or 'farther from the reference' in json.dumps(attempt['error'])
                    else:
                        raise AssertionError('A neutral match must be removable')
                    row.update(acceptedControls=copy_controls(controls), match=attempt['result'])
                    assert row['match']['fullResolutionChecked'] and row['match']['current']
                    matched = recipe(pid)
                    before_body = json.loads(baselines[pid][0]['body'])
                    after_body = json.loads(matched['body'])
                    assert all(before_body.get(key) == after_body.get(key) for key in ['geometry', 'retouch', 'masks'])
                    pixel_hash = render(pid, out / f'{style}-{photo["fileName"]}-matched.png')
                    row.update(pixelSha256=pixel_hash, pixelsChanged=pixel_hash != baselines[pid][1])
                    if matched['recipeHash'] != baselines[pid][0]['recipeHash']:
                        page.get_by_role('region', name='Photo editor', exact=True).get_by_role('button', name='Undo', exact=True).click()
                        expect(upload).to_be_enabled()
                        assert recipe(pid)['recipeHash'] == baselines[pid][0]['recipeHash']
                        page.get_by_role('region', name='Photo editor', exact=True).get_by_role('button', name='Redo', exact=True).click()
                        expect(upload).to_be_enabled()
                        assert recipe(pid)['recipeHash'] == matched['recipeHash']
                    row['undoRedoExact'] = True
                    outcome = apply_ui(remove=True)
                    assert outcome['ok']
                    assert render(pid, out / f'{style}-{photo["fileName"]}-removed.png') == baselines[pid][1]
                    row['zeroRestoresExact'] = True
                    assert apply_ui(controls)['ok']
                    (out / f'{style}-{photo["fileName"]}.recipe.json').write_text(recipe(pid)['body'])
                    step('Match ' + style + ' onto ' + photo['fileName'] + ', Undo/Redo and remove/restore', lambda row=row: row)

                nav('Export')
                page.get_by_test_id('preset').select_option('gallery')
                destination = out / ('export-' + style)
                page.get_by_test_id('destination').fill(str(destination))
                page.get_by_test_id('verify').check()
                expect(page.get_by_test_id('run')).to_be_enabled()
                page.get_by_test_id('run').click()
                deadline = time.monotonic() + 600
                while not (destination / 'aura-delivery-manifest.json').exists():
                    assert time.monotonic() < deadline, 'Export timed out'
                    time.sleep(.5)
                expect(page.get_by_test_id('run')).to_be_enabled()
                delivery = json.loads((destination / 'aura-delivery-manifest.json').read_text())
                assert delivery['verified'] and delivery['file_count'] == 2
                for file in delivery['files']:
                    with Image.open(destination / file['path']) as image:
                        image.verify()
                step('Verified JPEG export: ' + style, lambda delivery=delivery: delivery)
                nav('Match a look')
                expect(upload).to_be_enabled()
                # The next reference must start from the same saved baseline on each target.
                if reference != imported[-1]:
                    for photo in photos:
                        page.get_by_role('option', name=photo['fileName'], exact=True).click()
                        expect(upload).to_be_enabled()
                        assert apply_ui(remove=True)['ok']
                        assert json.loads(recipe(photo['id'])['body'])['global'] == json.loads(baselines[photo['id']][0]['body'])['global']

            saved = {p['id']: (recipe(p['id'])['recipeHash'], call('color_match_status', photoId=p['id'])) for p in photos}
            page.reload()
            page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
            for pid, (expected_hash, status) in saved.items():
                assert recipe(pid)['recipeHash'] == expected_hash
                assert call('color_match_status', photoId=pid) == status
            step('Saved recipes, references and matching strengths survive UI reload', lambda: dict(photos=2, references=len(call('list_color_references'))))
            with sqlite3.connect(out / 'catalog/catalog.sqlite') as source:
                with sqlite3.connect(out / 'catalog/qualified.sqlite') as destination:
                    source.backup(destination)

            nav('Match a look')
            # Active collection selection is UI session state, separate from saved recipes.
            page.get_by_role('region', name='Photo collections', exact=True).get_by_role('button', name=re.compile('^' + re.escape(name))).click()
            pid = photos[0]['id']
            page.get_by_role('option', name=photos[0]['fileName'], exact=True).click()
            expect(upload).to_be_enabled()
            default = dict(amount=1, tone=1, color=1, whiteBalance=1)
            fine_default = dict.fromkeys(FINE_LABELS, 1)
            report['controls'] = []
            for key in LABELS:
                for value in [0, 2]:
                    before = recipe(pid)['recipeHash']
                    history = call('image_history', photoId=pid)
                    outcome = apply_ui(dict(default, **{key: value}), fine=fine_default)
                    if not outcome['ok']:
                        assert 'skin-colour' in outcome['error'] or 'farther from the reference' in outcome['error']
                        assert recipe(pid)['recipeHash'] == before and call('image_history', photoId=pid) == history
                    report['controls'].append(dict(control=key, value=value, **outcome))
            for key in FINE_LABELS:
                before = recipe(pid)['recipeHash']
                history = call('image_history', photoId=pid)
                outcome = apply_ui(default, fine=dict(fine_default, **{key: 0}))
                if not outcome['ok']:
                    assert 'skin-colour' in outcome['error'] or 'farther from the reference' in outcome['error']
                    assert recipe(pid)['recipeHash'] == before and call('image_history', photoId=pid) == history
                report['controls'].append(dict(control='detail.' + key, value=0, **outcome))
            assert apply_ui(default, fine=fine_default)['ok']
            step('All four main strengths and twelve fine controls save or explicitly refuse unsafe combinations',
                 lambda: dict(tested=len(report['controls']), accepted=sum(c['ok'] for c in report['controls']),
                              refused=sum(not c['ok'] for c in report['controls'])))

            nav('Auto edit')
            exposure = page.get_by_role('spinbutton', name='Exposure value', exact=True)
            expect(exposure).to_be_enabled()
            manual = round(json.loads(recipe(pid)['body'])['global']['exposure'] + .05, 3)
            exposure.fill(str(manual))
            exposure.press('Enter')
            page.wait_for_function('''async ([photoId,value]) => {
                const recipe=await window.__TAURI_INTERNALS__.invoke('image_recipe',{input:{photoId}});
                return Math.abs(JSON.parse(recipe.body).global.exposure-value)<.0001;
            }''', arg=[pid, manual])
            nav('Match a look')
            expect(panel.get_by_text('This photo changed after analysis. Analyse again to keep your newer edits.')).to_be_visible()
            assert not panel.get_by_role('button', name='Apply colour strengths', exact=True).is_enabled()
            step('A later manual edit invalidates the previous analysis', lambda: dict(manualExposure=manual, staleApplyDisabled=True))
            panel.get_by_text('Match this collection', exact=True).click()
            # New analyses preserve manual fields while measuring each target separately.
            for key, value in dict(default, amount=.5).items():
                slider = panel.get_by_role('slider', name=LABELS[key], exact=True)
                if slider.is_enabled():
                    slider.fill(str(round(value * 100)))
            panel.get_by_role('button', name='Analyse reference match', exact=True).click()
            expect(panel.get_by_role('button', name='Apply colour strengths', exact=True)).to_be_enabled()
            expect(upload).to_be_enabled()
            for key, value in dict(default, amount=.5).items():
                panel.get_by_role('slider', name=LABELS[key], exact=True).fill(str(round(value * 100)))
            panel.get_by_role('button', name='Match colour across this collection', exact=True).click()
            expect(panel.get_by_role('list', name='Collection colour matching results').get_by_role('listitem')).to_have_count(2)
            expect(upload).to_be_enabled()
            outcomes = panel.get_by_role('list', name='Collection colour matching results').get_by_role('listitem').all_text_contents()
            assert all('failed' not in text.lower() for text in outcomes), outcomes
            assert abs(json.loads(recipe(pid)['body'])['global']['exposure'] - manual) < .0001
            step('Collection matching measures both photos and preserves a manual exposure', lambda: outcomes)
            report['originalsUnchanged'] = all(digest(Path(path)) == value for path, value in original_hashes.items())
            assert report['originalsUnchanged'] and not report['pageErrors']
            report['completed'] = True
            persist()
    except Exception as error:
        report['fatalError'] = str(error)
        raise
    finally:
        persist()
        if process.poll() is None:
            process.terminate()
        process.wait(timeout=30)


def copy_controls(value):
    return dict(value)


if __name__ == '__main__':
    main()
