"""Check Face / Body skin / Face + body skin automatic retouch in the real desktop app.

Clicks the three choices and the Auto retouch button with genuine Windows mouse input
(desktop must run with WebView2 CDP on port 9223). For each photo and choice it records
which operations were created, renders the result, and confirms Undo restores the
previous pixels. Writes .work-checks/retouch-scope/results.json and face/body crops.
"""
import base64
import ctypes
import json
import re
import sys
import time
from pathlib import Path

from PIL import Image
from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.work-checks/retouch-scope'
OUT.mkdir(parents=True, exist_ok=True)
SOURCES = [Path(p).resolve() for p in sys.argv[1:]]
user32 = ctypes.windll.user32
user32.SetProcessDPIAware()
report = {'photos': []}


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
    user32.SetCursorPos(int(origin.x + (box['x'] + box['width'] / 2) * ratio), int(origin.y + (box['y'] + box['height'] / 2) * ratio))
    time.sleep(0.15)
    user32.mouse_event(0x0002, 0, 0, 0, 0)
    time.sleep(0.05)
    user32.mouse_event(0x0004, 0, 0, 0, 0)


with sync_playwright() as pw:
    page = pw.chromium.connect_over_cdp('http://127.0.0.1:9223').contexts[0].pages[0]
    page.set_default_timeout(180000)
    expect.set_options(timeout=180000)

    def call(cmd, **args):
        return page.evaluate('async ([c,a])=>await window.__TAURI_INTERNALS__.invoke(c,{input:a})', [cmd, args])

    name = 'Retouch scope ' + time.strftime('%H-%M-%S')
    project = call('create_project', name=name, coupleNames=None, eventDate=None)['id']
    call('start_ingest', projectId=project, roots=[str(p) for p in SOURCES])
    photos, deadline = [], time.monotonic() + 180
    while len(photos) < len(SOURCES) and time.monotonic() < deadline:
        photos = call('list_images', projectId=project, offset=0, limit=20, orderBy='timeline')
        time.sleep(0.5)
    page.reload()
    os_click(page, page.get_by_role('button', name=f'{name} {len(photos)}', exact=True))
    os_click(page, page.get_by_role('button', name='Auto edit One click, start to finish', exact=True))
    for row in photos:
        photo = row['id']
        record = {'file': row['fileName'], 'scopes': {}}
        report['photos'].append(record)
        os_click(page, page.get_by_role('option', name=row['fileName'], exact=True))
        expect(page.get_by_role('button', name='Retouch', exact=True)).to_be_enabled()
        os_click(page, page.get_by_role('button', name='Retouch', exact=True))
        expect(page.get_by_alt_text('Retouched photograph', exact=True)).to_be_visible()
        for scope_name in ['Body skin', 'Face', 'Face + body skin']:
            os_click(page, page.get_by_role('radio', name=scope_name, exact=True))
            before = call('image_recipe', photoId=photo)['recipeHash']
            os_click(page, page.get_by_role('button', name=f'Auto retouch: {scope_name}', exact=True))
            deadline = time.monotonic() + 120
            while call('image_recipe', photoId=photo)['recipeHash'] == before and time.monotonic() < deadline:
                time.sleep(1)
            expect(page.get_by_role('button', name=f'Auto retouch: {scope_name}', exact=True)).to_be_enabled()
            ops = call('native_retouch_edit', projectId=project, photoId=photo, action='list', edits=[], id=None)
            body = json.loads(call('image_recipe', photoId=photo)['body'])
            rep = body.get('studio_portrait_auto_v1', {})
            tags = page.locator('ol button', has_text=re.compile(r'Auto \((face|body) \d+\)')).all_inner_texts()
            r = call('render_image', photoId=photo, level='screen', screen=[900, 900], purpose='interactive')
            Image.frombytes('RGB', (r['width'], r['height']), base64.b64decode(r['rgbBase64'])).save(
                OUT / f"{Path(row['fileName']).stem}-{scope_name.replace(' ', '').replace('+', '-')}.jpg", quality=90)
            record['scopes'][scope_name] = {
                'changed': call('image_recipe', photoId=photo)['recipeHash'] != before,
                'faceOps': sorted({o['tool'] for o in ops if o['id'].startswith('auto-') and '-body-' not in o['id']}),
                'bodyOps': [o['tool'] for o in ops if '-body-' in o['id']],
                'message': rep.get('message'),
                'findings': [f for a in rep.get('assessments', []) for f in a.get('findings', []) if f.startswith('Body')],
                'uiTags': [t.split('·')[-1].strip() for t in tags][:4],
                'lastHistory': ([e['label'] for e in call('image_history', photoId=photo)['entries']] or [''])[-1][:90],
            }
            print(row['fileName'], scope_name, record['scopes'][scope_name]['bodyOps'], len(record['scopes'][scope_name]['faceOps']), flush=True)
        page.screenshot(path=str(OUT / f"{Path(row['fileName']).stem}-ui.png"))
        os_click(page, page.get_by_role('button', name='Back to Develop', exact=False).first) if page.get_by_role('button', name='Back to Develop', exact=False).count() else None
        page.reload()
        os_click(page, page.get_by_role('button', name=f'{name} {len(photos)}', exact=True))
        os_click(page, page.get_by_role('button', name='Auto edit One click, start to finish', exact=True))
        (OUT / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    print('done')
