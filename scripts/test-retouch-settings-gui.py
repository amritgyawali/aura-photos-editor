"""Check AI skin detection and the automatic retouch settings in the real desktop app. ADR-0077.

Starts the desktop app with WebView2 CDP on port 9223 (AURA_EXE, default the dev build),
imports the photographs given on the command line, and for each one clicks Retouch, a scope,
a preset and Auto retouch with genuine Windows mouse input. It then records which operations
were created, whether they carry segmentation mattes, what the segmenter reported, renders
the result, and confirms that Undo goes back. Writes .work-checks/retouch-settings/.

    python scripts/test-retouch-settings-gui.py photo1.jpg photo2.jpg ...
"""
import base64
import ctypes
import json
import os
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

from PIL import Image
from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.work-checks/retouch-settings'
OUT.mkdir(parents=True, exist_ok=True)
EXE = Path(os.environ.get('AURA_EXE', r'C:\Users\amrit\aura-c-target\debug\aura-desktop.exe'))
SOURCES = [Path(p).resolve() for p in sys.argv[1:]]
RUNS = [('Face + body skin', 'Natural'), ('Face + body skin', 'Polished beauty')]
user32 = ctypes.windll.user32
user32.SetProcessDPIAware()
report = {'photos': [], 'osClicks': 0, 'pageErrors': []}


class POINT(ctypes.Structure):
    _fields_ = [('x', ctypes.c_long), ('y', ctypes.c_long)]


def os_click(page, locator):
    locator.scroll_into_view_if_needed()
    box = locator.bounding_box()
    ratio = page.evaluate('devicePixelRatio')
    hwnd = user32.FindWindowW(None, 'AURA')
    user32.ShowWindow(hwnd, 9)
    user32.SetForegroundWindow(hwnd)
    time.sleep(0.3)
    origin = POINT(0, 0)
    user32.ClientToScreen(hwnd, ctypes.byref(origin))
    user32.SetCursorPos(int(origin.x + (box['x'] + box['width'] / 2) * ratio),
                        int(origin.y + (box['y'] + box['height'] / 2) * ratio))
    time.sleep(0.15)
    user32.mouse_event(0x0002, 0, 0, 0, 0)
    time.sleep(0.05)
    user32.mouse_event(0x0004, 0, 0, 0, 0)
    report['osClicks'] += 1
    time.sleep(0.3)


def launch():
    env = dict(os.environ, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9223')
    proc = subprocess.Popen([str(EXE)], cwd=str(ROOT), env=env)
    for _ in range(180):
        try:
            urllib.request.urlopen('http://127.0.0.1:9223/json/version', timeout=1)
            return proc
        except Exception:
            time.sleep(1)
    raise RuntimeError('AURA did not start')


proc = launch()
try:
    with sync_playwright() as pw:
        page = pw.chromium.connect_over_cdp('http://127.0.0.1:9223').contexts[0].pages[0]
        page.set_default_timeout(240000)
        expect.set_options(timeout=240000)
        page.on('pageerror', lambda e: report['pageErrors'].append(str(e)[:300]))
        page.wait_for_load_state()

        def call(cmd, **args):
            return page.evaluate('async ([c,a])=>await window.__TAURI_INTERNALS__.invoke(c,{input:a})', [cmd, args])

        name = 'Retouch settings ' + time.strftime('%H-%M-%S')
        project = call('create_project', name=name, coupleNames=None, eventDate=None)['id']
        call('start_ingest', projectId=project, roots=[str(p) for p in SOURCES])
        photos, deadline = [], time.monotonic() + 240
        while len(photos) < len(SOURCES) and time.monotonic() < deadline:
            photos = call('list_images', projectId=project, offset=0, limit=20, orderBy='timeline')
            time.sleep(0.5)
        page.reload()
        os_click(page, page.get_by_role('button', name=f'{name} {len(photos)}', exact=True))
        os_click(page, page.get_by_role('button', name='Auto edit One click, start to finish', exact=True))
        for row in photos:
            photo = row['id']
            record = {'file': row['fileName'], 'runs': []}
            report['photos'].append(record)
            os_click(page, page.get_by_role('option', name=row['fileName'], exact=True))
            expect(page.get_by_role('button', name='Retouch', exact=True)).to_be_enabled()
            os_click(page, page.get_by_role('button', name='Retouch', exact=True))
            expect(page.get_by_alt_text('Retouched photograph', exact=True)).to_be_visible()
            for scope_name, preset in RUNS:
                os_click(page, page.get_by_role('radio', name=scope_name, exact=True))
                os_click(page, page.get_by_role('radio', name=preset, exact=True))
                before = call('image_recipe', photoId=photo)['recipeHash']
                started = time.monotonic()
                os_click(page, page.get_by_role('button', name=f'Auto retouch: {scope_name}', exact=True))
                deadline = time.monotonic() + 240
                while call('image_recipe', photoId=photo)['recipeHash'] == before and time.monotonic() < deadline:
                    time.sleep(1)
                expect(page.get_by_role('button', name=f'Auto retouch: {scope_name}', exact=True)).to_be_enabled()
                seconds = round(time.monotonic() - started, 1)
                ops = call('native_retouch_edit', projectId=project, photoId=photo, action='list', edits=[], id=None)
                body = json.loads(call('image_recipe', photoId=photo)['body'])
                rep = body.get('studio_portrait_auto_v1', {})
                mattes = body.get('studio_retouch_mattes_v1', {})
                r = call('render_image', photoId=photo, level='screen', screen=[1000, 1000], purpose='interactive')
                Image.frombytes('RGB', (r['width'], r['height']), base64.b64decode(r['rgbBase64'])).save(
                    OUT / f"{Path(row['fileName']).stem}-{preset.replace(' ', '-').lower()}.jpg", quality=92)
                history = [e['label'] for e in call('image_history', photoId=photo)['entries']]
                run = {
                    'scope': scope_name,
                    'preset': preset,
                    'seconds': seconds,
                    'changed': call('image_recipe', photoId=photo)['recipeHash'] != before,
                    'operations': len(ops),
                    'withMatte': sum(1 for o in ops if o.get('matte')),
                    'mattes': sorted(mattes),
                    'segmentation': {k: rep.get('segmentation', {}).get(k) for k in ('model', 'passes', 'people', 'unavailable')},
                    'message': rep.get('message'),
                    'findings': [f for a in rep.get('assessments', []) for f in a.get('findings', [])][:4],
                    'historySteps': len(history),
                }
                record['runs'].append(run)
                print(row['fileName'], preset, run['operations'], 'ops', run['withMatte'], 'with matte', seconds, 's', flush=True)
            # Undo walks back one saved step; the recipe must change back.
            now = call('image_recipe', photoId=photo)['recipeHash']
            undo = page.get_by_role('button', name='Undo', exact=True)
            if undo.count():
                os_click(page, undo.first)
                time.sleep(2)
                record['undoChangedRecipe'] = call('image_recipe', photoId=photo)['recipeHash'] != now
            page.screenshot(path=str(OUT / f"{Path(row['fileName']).stem}-ui.png"))
            page.reload()
            os_click(page, page.get_by_role('button', name=f'{name} {len(photos)}', exact=True))
            os_click(page, page.get_by_role('button', name='Auto edit One click, start to finish', exact=True))
            (OUT / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
finally:
    (OUT / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    proc.terminate()
print('done')
