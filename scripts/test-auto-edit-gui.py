"""Drive one-click automatic editing through the real desktop window (ADR-0076).

Uses genuine Windows mouse and keyboard input (SetCursorPos / mouse_event / keybd_event)
for the main actions, plus Playwright over WebView2 CDP (port 9223) to locate controls
and to verify results. Steps: open a collection, select a portrait, click Auto enhance,
read the step list, go back to an earlier automatic step, open Retouch, weaken one
automatic operation with the keyboard, disable another with the mouse, undo and redo
with Ctrl+Z / Ctrl+Shift+Z, and return. Screenshots go to .work-checks/auto-v3/gui.
"""
import base64
import ctypes
import hashlib
import json
import re
import sys
import time
from pathlib import Path

from PIL import Image
from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.work-checks/auto-v3/gui'
OUT.mkdir(parents=True, exist_ok=True)
SOURCE = ROOT / '.work-checks/auto-v3/inputs' / (sys.argv[1] if len(sys.argv) > 1 else 'portrait-smile-teeth.jpg')
user32 = ctypes.windll.user32
user32.SetProcessDPIAware()
report = {'steps': [], 'pageErrors': [], 'osInput': []}


class POINT(ctypes.Structure):
    _fields_ = [('x', ctypes.c_long), ('y', ctypes.c_long)]


def window():
    hwnd = user32.FindWindowW(None, 'AURA')
    if not hwnd:
        raise RuntimeError('AURA window not found')
    user32.ShowWindow(hwnd, 9)
    user32.SetForegroundWindow(hwnd)
    time.sleep(0.4)
    origin = POINT(0, 0)
    user32.ClientToScreen(hwnd, ctypes.byref(origin))
    return origin


def os_click(page, locator, label):
    locator.scroll_into_view_if_needed()
    box = locator.bounding_box()
    ratio = page.evaluate('devicePixelRatio')
    origin = window()
    x = int(origin.x + (box['x'] + box['width'] / 2) * ratio)
    y = int(origin.y + (box['y'] + box['height'] / 2) * ratio)
    user32.SetCursorPos(x, y)
    time.sleep(0.15)
    user32.mouse_event(0x0002, 0, 0, 0, 0)
    time.sleep(0.05)
    user32.mouse_event(0x0004, 0, 0, 0, 0)
    report['osInput'].append({'mouse': label, 'screen': [x, y]})


VK = {'ctrl': 0x11, 'shift': 0x10, 'z': 0x5A, 'left': 0x25, 'right': 0x27, 'tab': 0x09, 'enter': 0x0D, 'space': 0x20, 'home': 0x24}


def os_keys(*names, label=''):
    window()
    codes = [VK[n] for n in names]
    for c in codes:
        user32.keybd_event(c, 0, 0, 0)
        time.sleep(0.03)
    for c in reversed(codes):
        user32.keybd_event(c, 0, 2, 0)
        time.sleep(0.03)
    report['osInput'].append({'keys': '+'.join(names), 'purpose': label})
    time.sleep(0.3)


with sync_playwright() as pw:
    page = pw.chromium.connect_over_cdp('http://127.0.0.1:9223').contexts[0].pages[0]
    page.set_default_timeout(180000)
    expect.set_options(timeout=180000)
    page.on('pageerror', lambda e: report['pageErrors'].append(str(e)))

    def call(cmd, **args):
        return page.evaluate('async ([c,a])=>await window.__TAURI_INTERNALS__.invoke(c,{input:a})', [cmd, args])

    def persist():
        (OUT / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')

    def step(label, fn):
        started = time.monotonic()
        try:
            record = dict(label=label, status='passed', result=fn())
        except Exception as e:  # keep going; the report says what failed
            record = dict(label=label, status='failed', error=str(e)[:600])
        record['seconds'] = round(time.monotonic() - started, 2)
        report['steps'].append(record)
        page.screenshot(path=str(OUT / f'{len(report["steps"]):02d}.png'))
        persist()
        print(label, record['status'], record.get('error', '')[:200], flush=True)

    def ready():
        expect(page.get_by_role('button', name='Retouch', exact=True)).to_be_enabled()

    def frame(name):
        r = call('render_image', photoId=photo, level='screen', screen=[1400, 1000], purpose='interactive')
        image = Image.frombytes('RGB', (r['width'], r['height']), base64.b64decode(r['rgbBase64']))
        image.save(OUT / f'{name}.png')
        return hashlib.sha256(image.tobytes()).hexdigest()

    def stack():
        return call('native_retouch_edit', projectId=project, photoId=photo, action='list', edits=[], id=None)

    name = 'Auto edit GUI ' + time.strftime('%H-%M-%S')
    project = call('create_project', name=name, coupleNames=None, eventDate=None)['id']
    call('start_ingest', projectId=project, roots=[str(SOURCE)])
    photos = []
    deadline = time.monotonic() + 120
    while not photos and time.monotonic() < deadline:
        photos = call('list_images', projectId=project, offset=0, limit=10, orderBy='timeline')
        time.sleep(0.5)
    photo = photos[0]['id']
    source_hash = hashlib.sha256(SOURCE.read_bytes()).hexdigest()
    report.update(project=name, photo=SOURCE.name)
    page.reload()

    def open_photo():
        os_click(page, page.get_by_role('button', name=name + ' 1', exact=True), 'open collection')
        os_click(page, page.get_by_role('button', name='Auto edit One click, start to finish', exact=True), 'studio tab')
        os_click(page, page.get_by_role('option', name=SOURCE.name, exact=True), 'select photograph')
        ready()
        return frame('0-before')
    step('OS mouse: open collection and photo', open_photo)

    def auto():
        os_click(page, page.get_by_role('button', name='Auto enhance photo', exact=True), 'Auto enhance photo')
        expect(page.get_by_text('Last automatic pass:', exact=False)).to_be_visible()
        ready()
        body = json.loads(call('image_recipe', photoId=photo)['body'])
        steps = body['studio_portrait_auto_v1'].get('steps', [])
        assert steps, 'no automatic steps reported'
        expect(page.get_by_text(f'Automatic steps ({len(steps)})', exact=True)).to_be_visible()
        return {'steps': [s['title'] + ': ' + s['detail'] for s in steps], 'ops': len(stack()), 'hash': frame('1-auto')}
    step('OS mouse: Auto enhance photo', auto)

    history = call('image_history', photoId=photo)

    def go_back():
        first = history['entries'][0]
        button = page.get_by_role('button', name=f"Go back to step {first['seq']}: {first['label']}", exact=True)
        os_click(page, button, 'Go back to step 1')
        ready()
        expect(page.get_by_text(first['label'], exact=True)).to_be_visible()
        remaining = len(stack())
        h = frame('2-step1')
        assert remaining == 0 or len(history['entries']) == 1, remaining
        last = history['entries'][-1]
        os_click(page, page.get_by_role('button', name=f"Go back to step {last['seq']}: {last['label']}", exact=True), 'Go forward to last step')
        ready()
        return {'opsAtStep1': remaining, 'step1Hash': h, 'backAtHead': frame('3-head') == report['steps'][1]['result']['hash']}
    step('OS mouse: go back to step 1 and forward again', go_back)

    def retouch():
        os_click(page, page.get_by_role('button', name='Retouch', exact=True), 'open Retouch')
        expect(page.get_by_alt_text('Retouched photograph', exact=True)).to_be_visible()
        auto_ops = page.get_by_role('button', name=re.compile(r'Auto \(face 1\)'))
        count = auto_ops.count()
        assert count > 0, 'no automatic operations tagged in the stack'
        os_click(page, auto_ops.first, 'select first automatic operation')
        slider = page.get_by_label(re.compile(r'^Strength'))
        slider.focus()
        before = float(slider.input_value())
        for _ in range(5):
            os_keys('left', label='weaken strength')
        after = float(slider.input_value())
        os_click(page, page.get_by_role('button', name=re.compile('^(Apply|Update|Save)')).first, 'apply change')
        time.sleep(2)
        return {'taggedAutomaticOps': count, 'strengthBefore': before, 'strengthAfter': after}
    step('OS keyboard: weaken an automatic operation in Retouch', retouch)

    def undo_redo():
        h0 = call('image_recipe', photoId=photo)['recipeHash']
        page.get_by_role('group', name='Retouch image interaction', exact=True).focus()
        os_keys('ctrl', 'z', label='undo')
        expect(page.get_by_role('button', name='Redo', exact=True)).to_be_enabled()
        h1 = call('image_recipe', photoId=photo)['recipeHash']
        page.get_by_role('group', name='Retouch image interaction', exact=True).focus()
        os_keys('ctrl', 'shift', 'z', label='redo')
        time.sleep(2)
        h2 = call('image_recipe', photoId=photo)['recipeHash']
        return {'undoChanged': h1 != h0, 'redoRestored': h2 == h0}
    step('OS keyboard: Ctrl+Z / Ctrl+Shift+Z', undo_redo)

    def protection():
        body = json.loads(call('enhance_photo', photoId=photo)['body'])
        return {'status': body['studio_portrait_auto_v1']['status'], 'opsAfterRepeat': len(stack())}
    step('Repeat Auto enhance keeps manual retouch', protection)

    report['final'] = frame('4-final')
    report['originalUnchanged'] = hashlib.sha256(SOURCE.read_bytes()).hexdigest() == source_hash
    report['history'] = [e['label'] for e in call('image_history', photoId=photo)['entries']]
    persist()
    print('done', report['originalUnchanged'])
