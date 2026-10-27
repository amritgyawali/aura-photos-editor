"""Retouch strength sliders in the real desktop window. ADR-0130.

Starts the desktop app against an isolated catalogue with WebView2 CDP, imports the photographs
given, runs Auto retouch on the first, and then works the Retouch strength panel the way a
photographer would - keyboard on the sliders, the Reset and Apply-to-collection buttons - and
checks, from what the window renders and saves:

- the panel appears after Auto retouch, one slider per kind of retouch the pass did;
- every slider at 0% renders exactly the photo before retouch;
- a lowered slider renders between the two, and the saved operations are unchanged;
- Reset renders exactly the automatic result again;
- Apply to collection gives the other auto-retouched photo the same strengths;
- the page raised no errors.

    python scripts/test-retouch-strength-gui.py PHOTO PHOTO [--port 9371]

`AURA_EXE` names the build (default `target/desktop-launch/debug/aura-desktop.exe`; a debug
build, because only a debug build honours `AURA_TEST_CATALOG`). Writes
`.work-checks/retouch-strength-gui/`.
"""
import argparse
import base64
import json
import os
import shutil
import subprocess
import time
import urllib.request
from pathlib import Path

import numpy as np
from PIL import Image
from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.work-checks/retouch-strength-gui'
SCOPE = 'Face + body skin'


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
    parser.add_argument('photos', type=Path, nargs=2)
    parser.add_argument('--port', type=int, default=9371)
    args = parser.parse_args()
    exe = Path(os.environ.get('AURA_EXE', ROOT / 'target/desktop-launch/debug/aura-desktop.exe'))
    if OUT.exists():
        shutil.rmtree(OUT)
    sources = OUT / 'sources'
    sources.mkdir(parents=True)
    for path in args.photos:
        shutil.copy2(path, sources / path.name)
    report = {'exe': str(exe), 'checks': {}, 'pageErrors': []}
    proc = launch(exe, args.port)
    if not (OUT / 'catalog' / 'catalog.sqlite').exists():
        time.sleep(3)
    assert (OUT / 'catalog' / 'catalog.sqlite').exists(), 'the catalogue is not isolated - stop'

    def check(name, ok, detail=''):
        report['checks'][name] = {'ok': bool(ok), 'detail': detail}
        print(('PASS ' if ok else 'FAIL ') + name + (f' - {detail}' if detail else ''))

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

            def call(command, **kwargs):
                return page.evaluate('async ([c,a])=>await window.__TAURI_INTERNALS__.invoke(c,{input:a})',
                                     [command, kwargs])

            def raw(command, **kwargs):
                return page.evaluate('async ([c,a])=>await window.__TAURI_INTERNALS__.invoke(c,a)', [command, kwargs])

            def recipe_hash(photo):
                return call('image_recipe', photoId=photo)['recipeHash']

            def render(photo, before=False):
                return pixels(raw('native_retouch_preview', projectId=project, photoId=photo, before=before,
                                  quality='full'))

            def wait_saved(photo, old, seconds=300):
                deadline = time.monotonic() + seconds
                while recipe_hash(photo) == old and time.monotonic() < deadline:
                    time.sleep(0.5)
                assert recipe_hash(photo) != old, 'nothing was saved'

            def shot(name):
                page.screenshot(path=str(OUT / f'{name}.png'), full_page=True)

            name = 'Retouch strength ' + time.strftime('%H-%M-%S')
            project = call('create_project', name=name, coupleNames=None, eventDate=None)['id']
            call('start_ingest', projectId=project, roots=[str(sources)])
            photos, deadline = [], time.monotonic() + 300
            while len(photos) < 2 and time.monotonic() < deadline:
                photos = call('list_images', projectId=project, offset=0, limit=10, orderBy='timeline')
                time.sleep(0.5)
            assert len(photos) == 2, f'imported {len(photos)} of 2'
            first, second = photos[0], photos[1]

            def open_retouch(row):
                page.reload()
                page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
                page.get_by_role('button', name=f'{name} 2', exact=True).click()
                page.get_by_role('button', name='Auto edit One click, start to finish', exact=True).click()
                option = page.get_by_role('option', name=row['fileName'], exact=True)
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

            def auto_retouch(row):
                old = recipe_hash(row['id'])
                started = time.monotonic()
                button = page.get_by_role('button', name=f'Auto retouch: {SCOPE}', exact=True)
                if not button.count():
                    page.get_by_role('radio', name=SCOPE, exact=True).click()
                button.click()
                wait_saved(row['id'], old, 900)
                expect(button).to_be_enabled()
                return round(time.monotonic() - started, 1)

            # The second photo first, so it is auto-retouched before the sync from the first.
            open_retouch(second)
            report['autoRetouchSecondSeconds'] = auto_retouch(second)
            open_retouch(first)
            report['autoRetouchFirstSeconds'] = auto_retouch(first)
            panel = page.get_by_role('region', name='Retouch strength')
            expect(panel).to_be_visible()
            sliders = panel.get_by_role('slider')
            labels = [sliders.nth(i).get_attribute('aria-label') for i in range(sliders.count())]
            report['sliders'] = labels
            check('panel appears after Auto retouch', len(labels) > 0, ', '.join(labels))
            shot('01-after-auto-retouch')

            photo = first['id']
            automatic = render(photo)
            before = render(photo, before=True)
            changed = float(np.abs(automatic - before).mean())
            check('auto retouch changed the photo', changed > 0, f'mean difference {changed:.4f}')
            stack = raw('native_retouch_edit', input=dict(projectId=project, photoId=photo, action='list',
                                                          edits=[], id=None))

            def set_slider(label, keys):
                slider = panel.get_by_role('slider', name=label, exact=True)
                old = recipe_hash(photo)
                slider.focus()
                for key in keys:
                    page.keyboard.press(key)
                wait_saved(photo, old)
                expect(slider).to_be_enabled()

            target = next((l for l in labels if l.startswith('Skin smoothing')), labels[0])
            set_slider(target, ['Home'])
            lowered = render(photo)
            between = float(np.abs(lowered - before).mean())
            check(f'{target} at 0% moves the photo toward the original',
                  0 <= between < changed and not np.array_equal(lowered, automatic),
                  f'{between:.4f} of {changed:.4f}')
            after_stack = raw('native_retouch_edit', input=dict(projectId=project, photoId=photo, action='list',
                                                                edits=[], id=None))
            check('the saved operations are not rewritten', after_stack == stack)
            shot('02-one-slider-at-zero')

            for label in labels:
                if label != target:
                    set_slider(label, ['Home'])
            zero = render(photo)
            check('every slider at 0% is exactly the photo before retouch', np.array_equal(zero, before),
                  f'max difference {float(np.abs(zero - before).max()):.1f}')
            shot('03-all-at-zero')

            old = recipe_hash(photo)
            panel.get_by_role('button', name='Reset to Auto retouch', exact=True).click()
            wait_saved(photo, old)
            check('Reset is exactly the automatic result again', np.array_equal(render(photo), automatic))

            blemish = next((l for l in labels if l.startswith('Blemish')), target)
            # One key press: each release saves, and the slider is disabled while a save runs.
            set_slider(blemish, ['PageDown'])
            mine = {s['category']: s['factor'] for s in call('retouch_intensity', projectId=project, photoId=photo,
                                                              action='get', factors={})['sliders']}
            panel.get_by_role('button', name='Apply these strengths to the collection', exact=True).click()
            # Each slider's <output> is a status too; the sentence is the paragraph.
            status = panel.locator('p[role="status"]')
            expect(status).to_contain_text('Applied to 1 other photo')
            theirs = {s['category']: s['factor'] for s in call('retouch_intensity', projectId=project,
                                                                photoId=second['id'], action='get',
                                                                factors={})['sliders']}
            shared = {c: f for c, f in mine.items() if c in theirs}
            check('Apply to collection gives the other photo the same strengths',
                  shared and all(abs(theirs[c] - f) < 1e-6 for c, f in shared.items()), json.dumps(theirs))
            shot('04-synced')
            check('the page raised no errors', not report['pageErrors'], '; '.join(report['pageErrors']))
    finally:
        (OUT / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
        proc.terminate()
    failed = [k for k, v in report['checks'].items() if not v['ok']]
    print(f"{len(report['checks']) - len(failed)} of {len(report['checks'])} checks passed")
    raise SystemExit(1 if failed else 0)


if __name__ == '__main__':
    main()
