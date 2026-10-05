"""Check per-photo adaptive editing in the real desktop app. ADR-0086.

Starts the desktop app with WebView2 CDP on port 9224 (AURA_EXE, default the dev build),
imports the photographs given on the command line into a new project, and edits every one
exactly as the batch editor does: `enhance_photo`, then the saved recipe and a rendered
preview. It records, per photograph, the scene that was measured, the tone that was saved,
what was measured about each face and which retouch settings were tuned for it, and saves
the rendered result beside the original for a person to look at. It then checks that a
second pass saves nothing new, that a pass with the option off uses the chosen settings
exactly, and opens Retouch in the window to confirm the control is there and works with a
genuine Windows mouse click. Writes .work-checks/adaptive-batch/.

    python scripts/test-adaptive-batch.py photo1.jpg photo2.jpg ...
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

from PIL import Image, ImageDraw
from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.work-checks/adaptive-batch'
OUT.mkdir(parents=True, exist_ok=True)
EXE = Path(os.environ.get('AURA_EXE', r'C:\Users\amrit\aura-c-target\debug\aura-desktop.exe'))
SOURCES = [Path(p).resolve() for p in sys.argv[1:]]
PORT = 9224
user32 = ctypes.windll.user32
user32.SetProcessDPIAware()
report = {'photos': [], 'checks': {}, 'pageErrors': [], 'ui': {}}


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
    time.sleep(0.3)


def launch():
    env = dict(os.environ, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-port={PORT}')
    proc = subprocess.Popen([str(EXE)], cwd=str(ROOT), env=env)
    for _ in range(180):
        try:
            urllib.request.urlopen(f'http://127.0.0.1:{PORT}/json/version', timeout=1)
            return proc
        except Exception:
            time.sleep(1)
    raise RuntimeError('AURA did not start')


def save(dto, path):
    image = Image.frombytes('RGB', (dto['width'], dto['height']), base64.b64decode(dto['rgbBase64']))
    image.save(path, quality=92)
    return image


def summary(recipe):
    body = json.loads(recipe['body'])
    g = body.get('global', {})
    rep = body.get('studio_portrait_auto_v1', {})
    faces = []
    for a in rep.get('assessments', []):
        e = a.get('expert')
        faces.append({
            'face': a.get('face'), 'status': a.get('status'),
            'spotsHealed': a.get('spotsHealed'), 'marksKept': a.get('marksKept'),
            'condition': e and e.get('condition'), 'intensity': e and e.get('intensity'),
            'adjusted': e and e.get('adjusted'), 'notes': e and e.get('notes'),
        })
    return {
        'scene': (rep.get('scene') or {}).get('kind'),
        'decisions': (rep.get('scene') or {}).get('decisions', []),
        'tone': {k: g.get(k) for k in ('exposure', 'highlights', 'shadows', 'whites', 'blacks', 'contrast',
                                        'temperature', 'tint', 'vibrance', 'clarity', 'dehaze')},
        'faces': faces,
        'operations': rep.get('operations'),
        'steps': [s.get('title') for s in rep.get('steps', [])],
        'message': rep.get('message'),
    }


proc = launch()
try:
    with sync_playwright() as pw:
        browser = pw.chromium.connect_over_cdp(f'http://127.0.0.1:{PORT}')
        page = None
        for _ in range(120):
            pages = [p for c in browser.contexts for p in c.pages]
            page = next((p for p in pages if 'localhost' in p.url or 'tauri' in p.url), None)
            if page:
                break
            time.sleep(1)
        page.set_default_timeout(240000)
        page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
        expect.set_options(timeout=60000)
        page.on('pageerror', lambda e: report['pageErrors'].append(str(e)[:300]))
        page.wait_for_load_state()

        def call(cmd, **args):
            return page.evaluate('async ([c,a])=>await window.__TAURI_INTERNALS__.invoke(c,{input:a})', [cmd, args])

        name = 'Adaptive batch ' + time.strftime('%H-%M-%S')
        project = call('create_project', name=name, coupleNames=None, eventDate=None)['id']
        call('start_ingest', projectId=project, roots=[str(p) for p in SOURCES])
        photos, deadline = [], time.monotonic() + 300
        while len(photos) < len(SOURCES) and time.monotonic() < deadline:
            photos = call('list_images', projectId=project, offset=0, limit=240, orderBy='timeline')
            time.sleep(0.5)
        report['checks']['imported'] = [len(photos), len(SOURCES)]
        by_name = {p.name: p for p in SOURCES}
        tiles = []
        for row in photos:
            photo = row['id']
            started = time.monotonic()
            record = {'file': row['fileName']}
            report['photos'].append(record)
            try:
                # The batch editor's own sequence (ui/src/components/autopilot/prepareCollection.ts).
                call('enhance_photo', photoId=photo)
                recipe = call('image_recipe', photoId=photo)
                rendered = call('render_image', photoId=photo, level='screen', screen=[900, 900], purpose='interactive')
                record['seconds'] = round(time.monotonic() - started, 1)
                record.update(summary(recipe))
                after = save(rendered, OUT / f"{Path(row['fileName']).stem}-after.jpg")
                source = by_name.get(row['fileName'])
                if source:
                    before = Image.open(source).convert('RGB').resize(after.size, Image.LANCZOS)
                    pair = Image.new('RGB', (after.width * 2 + 6, after.height), 'white')
                    pair.paste(before, (0, 0))
                    pair.paste(after, (after.width + 6, 0))
                    pair.save(OUT / f"{Path(row['fileName']).stem}-before-after.jpg", quality=90)
                    tiles.append((row['fileName'], pair))
                # A second pass on an unchanged photograph must save nothing new.
                again = call('enhance_photo', photoId=photo)
                record['repeatSavedNothing'] = again['recipeHash'] == recipe['recipeHash']
                record['outcome'] = 'ready'
            except Exception as error:  # noqa: BLE001 - every failure is recorded per photograph
                record['outcome'] = 'failed'
                record['error'] = str(error)[:400]
            tuned = [f"face {f['face']}: " + ', '.join(f"{k} {v[0]:.2f}->{v[1]:.2f}" for k, v in (f['adjusted'] or {}).items())
                     for f in record.get('faces', []) if f.get('adjusted') is not None]
            print(row['fileName'], record['outcome'], record.get('scene'), record.get('tone', {}).get('exposure'),
                  f"{record.get('seconds')}s", '|', ' ; '.join(tuned)[:260], flush=True)
            (OUT / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')

        ready = [p for p in report['photos'] if p['outcome'] == 'ready']
        portraits = [p for p in ready if any(f.get('adjusted') is not None for f in p['faces'])]
        signatures = {json.dumps([f.get('adjusted') for f in p['faces']], sort_keys=True) for p in portraits}
        tones = {json.dumps(p['tone'], sort_keys=True) for p in ready}
        report['checks'].update({
            'ready': [len(ready), len(photos)],
            'portraitsMeasured': len(portraits),
            'distinctRetouchSettings': len(signatures),
            'distinctToneSettings': len(tones),
            'scenes': sorted({p['scene'] for p in ready if p.get('scene')}),
            'repeatSavedNothing': all(p.get('repeatSavedNothing') for p in ready),
        })

        # With the option off, the chosen settings are used exactly as set.
        if portraits:
            target = next(r for r in photos if r['fileName'] == portraits[0]['file'])
            exact = call('auto_retouch', projectId=project, photoId=target['id'], **{'global': False},
                         options={'intensity': 1, 'blemishes': True, 'eyes': True, 'teeth': True, 'refine': True,
                                  'scope': 'face', 'adaptive': False})
            body = json.loads(exact['body'])['studio_portrait_auto_v1']
            report['checks']['exactPassHasNoAdaptation'] = all(a.get('expert') is None for a in body['assessments'])
            report['checks']['exactPassRemembered'] = body['options'].get('adaptive') is False

        # The window itself: open the project and Retouch, and use the control with real input.
        try:
            os_click(page, page.get_by_role('button', name='Weddings', exact=True))
            if not page.get_by_role('button', name=f'{name} {len(photos)}', exact=True).count():
                os_click(page, page.get_by_role('button', name='Import', exact=True))
                os_click(page, page.get_by_role('button', name='Weddings', exact=True))
            os_click(page, page.get_by_role('button', name=f'{name} {len(photos)}', exact=True))
            os_click(page, page.locator('button', has_text='4 Edit').or_(page.locator('button', has_text='Edit').filter(has_text='edits')).first)
            os_click(page, page.get_by_role('button', name='Photo studio', exact=True))
            page.screenshot(path=str(OUT / 'ui-project.png'))
            if portraits:
                option = page.get_by_role('option', name=portraits[0]['file'], exact=True)
                page.wait_for_function("()=>{const f=document.querySelector('fieldset.filmstrip-lock');return !f||!f.disabled}")
                for _ in range(5):
                    os_click(page, option)
                    time.sleep(1.5)
                    if option.get_attribute('aria-selected') == 'true':
                        break
                os_click(page, page.get_by_role('button', name='Retouch', exact=True))
                expect(page.get_by_alt_text('Retouched photograph', exact=True)).to_be_visible(timeout=120000)
                adapt = page.get_by_label('Adapt to each face')
                expect(adapt).to_be_visible()
                report['ui']['restoredOff'] = not adapt.is_checked()
                os_click(page, adapt)
                report['ui']['toggledOn'] = adapt.is_checked()
                target = next(r for r in photos if r['fileName'] == portraits[0]['file'])
                before = call('image_recipe', photoId=target['id'])['recipeHash']
                os_click(page, page.get_by_role('button', name='Auto retouch: Face', exact=True))
                deadline = time.monotonic() + 240
                while call('image_recipe', photoId=target['id'])['recipeHash'] == before and time.monotonic() < deadline:
                    time.sleep(1)
                body = json.loads(call('image_recipe', photoId=target['id'])['body'])['studio_portrait_auto_v1']
                report['ui']['clickRanAdaptivePass'] = any(a.get('expert') for a in body['assessments'])
                report['ui']['findingsShown'] = [f for a in body['assessments'] for f in a.get('findings', []) if f.startswith('Adaptive:')][:6]
                time.sleep(2)
                page.screenshot(path=str(OUT / 'ui-retouch.png'))
        except Exception as error:  # noqa: BLE001 - the measured results above stand on their own
            report['ui']['error'] = str(error)[:400]
            try:
                page.screenshot(path=str(OUT / 'ui-error.png'))
            except Exception:  # noqa: BLE001
                pass

        if tiles:
            width = 900
            scaled = [(n, t.resize((width, max(1, round(t.height * width / t.width))), Image.LANCZOS)) for n, t in tiles]
            sheet = Image.new('RGB', (width, sum(t.height + 4 for _, t in scaled)), 'black')
            y = 0
            for n, t in scaled:
                sheet.paste(t, (0, y))
                ImageDraw.Draw(sheet).text((6, y + 4), f'{n}: original | edited', fill='yellow')
                y += t.height + 4
            sheet.save(OUT / 'sheet.jpg', quality=85)
finally:
    (OUT / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    proc.terminate()
print(json.dumps(report['checks'], indent=2))
print(json.dumps(report['ui'], indent=2))
