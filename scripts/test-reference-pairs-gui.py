"""Edit before/after pairs in the real desktop window and score what it delivers. ADR-0118.

Starts the desktop app against an isolated catalogue with WebView2 CDP, imports every
`NAME-before.png` in a folder, and for each one works the Retouch panel as a photographer
would: opens the photograph, clicks Retouch, picks a preset, clicks Auto retouch. It then
reads back what the window saved (which operations, which options the preset sent), renders
the result at full size, and scores it against `NAME-after.png`.

It also checks, in the same window: that Keep moles is on and switching it off changes what
is planned; that Undo walks a step back; that a look matched with a name is kept and can be
applied to another photograph; and that the page raised no errors.

    python scripts/test-reference-pairs-gui.py <folder> [--port 9364] [--only beauty,maternity]

`AURA_EXE` names the build (default `target/desktop-launch/debug/aura-desktop.exe`). Clicks go
through the WebView's own input pipeline, so the mouse is left alone. The native file picker
cannot be driven that way: the reference match is started through the same command the
button calls, after the name has been typed into the window. Writes
`.work-checks/reference-pairs-gui/`.
"""
import argparse
import base64
import json
import os
import subprocess
import time
import urllib.request
import uuid
from pathlib import Path

import numpy as np
from PIL import Image
from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.work-checks/reference-pairs-gui'
SCOPE = 'Face + body skin'
PORTRAIT = 'Portrait skin finish'
BODY = 'Body skin finish · stretch marks'
SAVED_EDIT_TIMEOUT_SECONDS = 120


def launch(exe, port):
    env = dict(os.environ,
               WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-port={port}',
               WEBVIEW2_USER_DATA_FOLDER=str(OUT / 'webview'),
               AURA_TEST_CATALOG=str(OUT / 'catalog' / 'catalog.sqlite'))
    (OUT / 'catalog').mkdir(parents=True, exist_ok=True)
    proc = subprocess.Popen([str(exe)], cwd=str(ROOT), env=env)
    for _ in range(180):
        try:
            urllib.request.urlopen(f'http://127.0.0.1:{port}/json/version', timeout=1)
            return proc
        except Exception:  # noqa: BLE001 - the port is not open until the window is
            time.sleep(1)
    proc.terminate()
    raise RuntimeError('AURA did not start')


def pixels(dto):
    return np.asarray(Image.frombytes('RGB', (dto['width'], dto['height']), base64.b64decode(dto['rgbBase64'])),
                      dtype=np.float32)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('folder', type=Path)
    parser.add_argument('--port', type=int, default=9364)
    parser.add_argument('--only', help='comma-separated pair names')
    args = parser.parse_args()
    exe = Path(os.environ.get('AURA_EXE', ROOT / 'target/desktop-launch/debug/aura-desktop.exe'))
    OUT.mkdir(parents=True, exist_ok=True)
    sources = sorted(args.folder.resolve().glob('*-before.png'))
    if args.only:
        wanted = args.only.split(',')
        sources = [p for p in sources if p.name[:-len('-before.png')] in wanted]
    assert sources, 'No NAME-before.png in the folder'
    report = {'exe': str(exe), 'photos': {}, 'checks': {}, 'pageErrors': []}
    proc = launch(exe, args.port)
    try:
        with sync_playwright() as pw:
            browser = pw.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}')
            page = None
            for _ in range(120):
                found = [p for c in browser.contexts for p in c.pages]
                page = next((p for p in found if 'localhost' in p.url or 'tauri' in p.url), None)
                if page:
                    break
                time.sleep(1)
            page.set_default_timeout(300000)
            expect.set_options(timeout=300000)
            page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
            page.on('pageerror', lambda e: report['pageErrors'].append(str(e)[:300]))
            page.wait_for_load_state()

            def call(command, **kwargs):
                return page.evaluate('async ([c,a])=>await window.__TAURI_INTERNALS__.invoke(c,{input:a})',
                                     [command, kwargs])

            def raw(command, **kwargs):
                return page.evaluate('async ([c,a])=>await window.__TAURI_INTERNALS__.invoke(c,a)', [command, kwargs])

            def recipe(photo):
                return call('image_recipe', photoId=photo)

            def saved(photo):
                return json.loads(recipe(photo)['body'])

            def write(name):
                (OUT / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
                page.screenshot(path=str(OUT / f'{name}.png'))

            name = 'Reference pairs ' + time.strftime('%H-%M-%S')
            project = call('create_project', name=name, coupleNames=None, eventDate=None)['id']
            call('start_ingest', projectId=project, roots=[str(p) for p in sources])
            photos, deadline = [], time.monotonic() + 300
            while len(photos) < len(sources) and time.monotonic() < deadline:
                photos = call('list_images', projectId=project, offset=0, limit=40, orderBy='timeline')
                time.sleep(0.5)
            assert len(photos) == len(sources), f'imported {len(photos)} of {len(sources)}'
            ids = {row['fileName'][:-len('-before.png')]: row['id'] for row in photos}

            def open_retouch(pair):
                page.reload()
                page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
                page.get_by_role('button', name=f'{name} {len(photos)}', exact=True).click()
                page.get_by_role('button', name='Auto edit One click, start to finish', exact=True).click()
                option = page.get_by_role('option', name=f'{pair}-before.png', exact=True)
                page.wait_for_function(
                    "()=>{const f=document.querySelector('fieldset.filmstrip-lock');return !f||!f.disabled}")
                for _ in range(6):
                    option.click()
                    time.sleep(1.5)
                    if option.get_attribute('aria-selected') == 'true':
                        break
                retouch = page.get_by_role('button', name='Retouch', exact=True)
                expect(retouch).to_be_enabled()
                workspace = page.get_by_alt_text('Retouched photograph', exact=True)
                for _ in range(4):
                    retouch.click()
                    try:
                        expect(workspace).to_be_visible(timeout=30000)
                        break
                    except AssertionError:
                        time.sleep(2)
                expect(workspace).to_be_visible()

            def auto_retouch(photo, preset):
                page.get_by_role('radio', name=preset, exact=True).click()
                # The finishing presets choose the scope themselves.
                expect(page.get_by_role('radio', name=SCOPE, exact=True)).to_be_checked()
                before = recipe(photo)['recipeHash']
                started = time.monotonic()
                button = page.get_by_role('button', name=f'Auto retouch: {SCOPE}', exact=True)
                button.click()
                deadline = time.monotonic() + 600
                while recipe(photo)['recipeHash'] == before and time.monotonic() < deadline:
                    time.sleep(1)
                expect(button).to_be_enabled()
                return round(time.monotonic() - started, 1)

            def score(pair, photo, tag):
                render = pixels(raw('native_retouch_preview', projectId=project, photoId=photo, before=False,
                                    quality='full'))
                unretouched = pixels(raw('native_retouch_preview', projectId=project, photoId=photo, before=True,
                                         quality='full'))
                original = np.asarray(Image.open(args.folder / f'{pair}-before.png').convert('RGB'), dtype=np.float32)
                finished = np.asarray(Image.open(args.folder / f'{pair}-after.png').convert('RGB'), dtype=np.float32)
                assert render.shape == finished.shape, 'the window delivered other dimensions'
                Image.fromarray(render.astype(np.uint8)).save(OUT / f'{pair}.{tag}.png')
                sheet = Image.fromarray(np.concatenate([original, render, finished], axis=1).astype(np.uint8))
                sheet.thumbnail((2400, 2400))
                sheet.save(OUT / f'{pair}.{tag}.sheet.jpg', quality=90)
                return render, {'error': round(float(np.abs(render - finished).mean()), 3),
                                'untouched': round(float(np.abs(original - finished).mean()), 3),
                                'unretouchedRender': round(float(np.abs(unretouched - finished).mean()), 3),
                                'renderRoundTripError': round(float(np.abs(unretouched - original).mean()), 3)}

            for pair, photo in ids.items():
                if pair in ('wedding', 'events'):
                    continue
                record = report['photos'].setdefault(pair, {})
                open_retouch(pair)
                preset = BODY if pair == 'maternity' else PORTRAIT
                record['preset'] = preset
                record['seconds'] = auto_retouch(photo, preset)
                # The panel reads the saved options back: the preset just used is still the one shown.
                record['presetStillShown'] = page.get_by_role('radio', name=preset, exact=True).is_checked()
                body = saved(photo)
                edits = body.get('studio_retouch_v1', [])
                auto = body.get('studio_portrait_auto_v1', {})
                settings = auto.get('options', {}).get('settings', {})
                tools = {}
                for edit in edits:
                    tools[edit['tool']] = tools.get(edit['tool'], 0) + 1
                record['operations'] = tools
                record['message'] = auto.get('message')
                record['sent'] = {k: settings.get(k) for k in
                                  ('keepMoles', 'stretchMarks', 'smoothing', 'lightEvenness', 'microDodgeBurn',
                                   'poreRefine', 'removeDarkMarks', 'blemishSensitivity')}
                record['moleEdges'] = sorted({edit.get('moleEdge') for edit in edits
                                              if edit['tool'] == 'frequency_heal'}, key=str)
                _, record['score'] = score(pair, photo, 'gui')
                print(pair, preset, record['seconds'], 's', tools, record['score'], flush=True)
                write(f'{pair}-ui')

            checks = report['checks']
            checks['presetStillShownAfterRunning'] = all(r.get('presetStillShown') for r in report['photos'].values())
            faces = [p for p in ('beauty', 'headshots', 'newBorn') if p in report['photos']]
            if faces:
                checks['presetSendsCalibratedValues'] = all(
                    report['photos'][p]['sent'] == {'keepMoles': True, 'stretchMarks': 0, 'smoothing': 0.05,
                                                    'lightEvenness': 0.1, 'microDodgeBurn': 0, 'poreRefine': 0.8,
                                                    'removeDarkMarks': True, 'blemishSensitivity': 1}
                    for p in faces)
                checks['facesAskForACrispEdge'] = all(1 in report['photos'][p]['moleEdges'] or
                                                      1.0 in report['photos'][p]['moleEdges'] for p in faces)
                checks['noStretchMarkStepOnFaces'] = all('stretch_marks' not in report['photos'][p]['operations']
                                                         for p in faces)
            if 'maternity' in report['photos']:
                m = report['photos']['maternity']
                checks['bodyPresetPlansStretchMarks'] = m['operations'].get('stretch_marks') == 1
                checks['bodyKeepsEveryPigmentPatch'] = m['moleEdges'] == [0] or m['moleEdges'] == [0.0]
                checks['bodyPresetSendsStretchMarks'] = m['sent']['stretchMarks'] == 1

            # Keep moles off, in the window: the planned healing no longer asks for an edge,
            # and Undo walks back to the plan that kept them.
            if 'headshots' in ids:
                photo = ids['headshots']
                open_retouch('headshots')
                page.get_by_role('radio', name=PORTRAIT, exact=True).click()
                page.locator('summary', has_text='Blemishes').click()
                toggle = page.get_by_label('Keep moles', exact=True)
                expect(toggle).to_be_checked()
                toggle.click()
                expect(page.get_by_text('moles and beauty marks can all be removed')).to_be_visible()
                kept_hash = recipe(photo)['recipeHash']
                button = page.get_by_role('button', name=f'Auto retouch: {SCOPE}', exact=True)
                button.click()
                deadline = time.monotonic() + 600
                while recipe(photo)['recipeHash'] == kept_hash and time.monotonic() < deadline:
                    time.sleep(1)
                expect(button).to_be_enabled()
                body = saved(photo)
                edges = {edit.get('moleEdge') for edit in body['studio_retouch_v1'] if edit['tool'] == 'frequency_heal'}
                checks['keepMolesOffIsSentAndPlanned'] = (
                    body['studio_portrait_auto_v1']['options']['settings']['keepMoles'] is False and edges == {None})
                _, checks['headshotsWithoutKeepMoles'] = score('headshots', photo, 'gui-moles-off')
                write('headshots-moles-off-ui')
                steps = len(call('image_history', photoId=photo)['entries'])
                undo = page.get_by_role('button', name='Undo', exact=True)
                if undo.count():
                    now = recipe(photo)['recipeHash']
                    undo.first.click()
                    # History may wait behind a full-quality render on the native worker.
                    # Check the saved edit rather than assuming a three-second response.
                    deadline = time.monotonic() + SAVED_EDIT_TIMEOUT_SECONDS
                    while recipe(photo)['recipeHash'] == now and time.monotonic() < deadline:
                        time.sleep(0.25)
                    checks['undoChangesTheSavedEdit'] = recipe(photo)['recipeHash'] != now
                checks['historySteps'] = steps

            # A matched look, kept under a name typed into the window, applied to another photograph.
            if 'wedding' in ids:
                photo = ids['wedding']
                open_retouch('wedding')
                auto_retouch(photo, PORTRAIT)
                back = page.get_by_role('button', name='Back to Develop', exact=True)
                if back.count():
                    back.first.click()
                field = page.get_by_label('Keep the matched look as', exact=True)
                expect(field).to_be_visible()
                field.fill('Wedding warm')
                checks['keepLookFieldInTheWindow'] = field.input_value() == 'Wedding warm'
                write('wedding-keep-look-ui')
                matched = call('match_paired_reference', photoId=photo,
                               referencePath=str((args.folder / 'wedding-after.png').resolve()),
                               cancelId=str(uuid.uuid4()), keepAs=field.input_value())
                checks['match'] = {k: matched.get(k) for k in ('changed', 'beforeError', 'afterError', 'keptLook')}
                _, checks['weddingMatched'] = score('wedding', photo, 'gui-match')
                looks = raw('list_edit_profiles')
                checks['keptLookIsListed'] = any(p['id'] == matched.get('keptLook') for p in looks)
                if 'events' in ids and matched.get('keptLook'):
                    other = ids['events']
                    applied = call('apply_edit_profile', photoId=other, profileId=matched['keptLook'], strength=1)
                    checks['keptLookAppliedToAnotherPhoto'] = {'changed': applied['changed'],
                                                               'adaptations': applied['adaptations']}
                    render = pixels(call('render_image', photoId=other, level='screen', screen=[1200, 1200],
                                         purpose='interactive'))
                    Image.fromarray(render.astype(np.uint8)).save(OUT / 'events.kept-wedding-look.jpg', quality=90)
                page.reload()
                page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
                write('after-keep-look-ui')

            checks['noPageErrors'] = not report['pageErrors']
            write('final-ui')
    finally:
        (OUT / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
        proc.terminate()
    print(json.dumps(report['checks'], indent=2))
    failed = [k for k, v in report['checks'].items() if v is False]
    if failed:
        raise SystemExit(f'failed: {failed}')
    print('done')


if __name__ == '__main__':
    main()
