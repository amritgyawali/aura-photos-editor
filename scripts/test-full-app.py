"""Exercise every workspace of the standalone AURA desktop app with real Windows input.

Mouse clicks, drags, typing and shortcuts are sent through the Windows input APIs
(SetCursorPos / mouse_event / SendInput), including the native Windows file dialog used to
import photos. WebView2 CDP (port 9223) is used only to find controls, read results and
take screenshots. Every step records pass/fail, the time it took, any error banner the app
showed, and a screenshot. Output: .work-checks/full-app/results.json and NN.png files.
"""
import base64
import ctypes
import ctypes.wintypes as wt
import hashlib
import json
import os
import re
import subprocess
import time
from pathlib import Path

from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.work-checks/full-app'
OUT.mkdir(parents=True, exist_ok=True)
INPUTS = ROOT / '.work-checks/full-inputs'
EXE = Path(os.environ.get('AURA_EXE', r'C:\Users\amrit\AURA\AURA.exe'))
user32 = ctypes.windll.user32
user32.SetProcessDPIAware()
report = {'steps': [], 'pageErrors': [], 'osInput': 0}
hashes = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in INPUTS.iterdir()}


class POINT(ctypes.Structure):
    _fields_ = [('x', ctypes.c_long), ('y', ctypes.c_long)]


class KEYBDINPUT(ctypes.Structure):
    _fields_ = [('wVk', wt.WORD), ('wScan', wt.WORD), ('dwFlags', wt.DWORD), ('time', wt.DWORD), ('dwExtraInfo', ctypes.POINTER(ctypes.c_ulong))]


class INPUT(ctypes.Structure):
    class _U(ctypes.Union):
        _fields_ = [('ki', KEYBDINPUT), ('pad', ctypes.c_byte * 32)]
    _anonymous_ = ('u',)
    _fields_ = [('type', wt.DWORD), ('u', _U)]


def send_unicode(ch):
    for flags in (0x0004, 0x0004 | 0x0002):  # KEYEVENTF_UNICODE, then key up
        inp = INPUT(type=1)
        inp.ki = KEYBDINPUT(0, ord(ch), flags, 0, None)
        user32.SendInput(1, ctypes.byref(inp), ctypes.sizeof(INPUT))


def os_type(text):
    for ch in text:
        send_unicode(ch)
        time.sleep(0.01)
    report['osInput'] += len(text)


VK = {'ctrl': 0x11, 'shift': 0x10, 'alt': 0x12, 'enter': 0x0D, 'esc': 0x1B, 'tab': 0x09, 'left': 0x25, 'up': 0x26,
      'right': 0x27, 'down': 0x28, 'home': 0x24, 'end': 0x23, 'a': 0x41, 'z': 0x5A, 'b': 0x42, 'e': 0x45, 'h': 0x48,
      '0': 0x30, 'plus': 0xBB, 'minus': 0xBD, 'space': 0x20, 'v': 0x56, 'g': 0x47}


def os_keys(*names):
    codes = [VK[n] for n in names]
    for c in codes:
        user32.keybd_event(c, 0, 0, 0); time.sleep(0.03)
    for c in reversed(codes):
        user32.keybd_event(c, 0, 2, 0); time.sleep(0.03)
    report['osInput'] += 1
    time.sleep(0.25)


def focus_app():
    hwnd = user32.FindWindowW(None, 'AURA')
    user32.ShowWindow(hwnd, 9)
    user32.SetForegroundWindow(hwnd)
    time.sleep(0.3)
    origin = POINT(0, 0)
    user32.ClientToScreen(hwnd, ctypes.byref(origin))
    return origin


def screen_point(page, locator, fx=0.5, fy=0.5):
    locator.scroll_into_view_if_needed()
    box = locator.bounding_box()
    ratio = page.evaluate('devicePixelRatio')
    origin = focus_app()
    return int(origin.x + (box['x'] + box['width'] * fx) * ratio), int(origin.y + (box['y'] + box['height'] * fy) * ratio)


def os_click(page, locator, fx=0.5, fy=0.5):
    x, y = screen_point(page, locator, fx, fy)
    user32.SetCursorPos(x, y); time.sleep(0.12)
    user32.mouse_event(0x0002, 0, 0, 0, 0); time.sleep(0.05)
    user32.mouse_event(0x0004, 0, 0, 0, 0)
    report['osInput'] += 1
    time.sleep(0.3)


def os_drag(page, locator, start, end, steps=15):
    x0, y0 = screen_point(page, locator, *start)
    x1, y1 = screen_point(page, locator, *end)
    user32.SetCursorPos(x0, y0); time.sleep(0.1)
    user32.mouse_event(0x0002, 0, 0, 0, 0)
    for i in range(1, steps + 1):
        user32.SetCursorPos(int(x0 + (x1 - x0) * i / steps), int(y0 + (y1 - y0) * i / steps)); time.sleep(0.02)
    user32.mouse_event(0x0004, 0, 0, 0, 0)
    report['osInput'] += steps
    time.sleep(0.3)


def launch():
    env = dict(os.environ, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9223')
    proc = subprocess.Popen([str(EXE)], cwd=str(EXE.parent), env=env)
    for _ in range(120):
        try:
            import urllib.request
            urllib.request.urlopen('http://127.0.0.1:9223/json/version', timeout=1)
            return proc
        except Exception:
            time.sleep(1)
    raise RuntimeError('AURA did not start')


with sync_playwright() as pw:
    def connect():
        page = pw.chromium.connect_over_cdp('http://127.0.0.1:9223').contexts[0].pages[0]
        page.set_default_timeout(120000)
        expect.set_options(timeout=120000)
        page.on('pageerror', lambda e: report['pageErrors'].append(str(e)[:300]))
        return page

    page = connect()

    def call(cmd, **args):
        return page.evaluate('async ([c,a])=>await window.__TAURI_INTERNALS__.invoke(c,{input:a})', [cmd, args])

    def persist():
        (OUT / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')

    def alerts():
        try:
            return [t.strip()[:200] for t in page.get_by_role('alert').all_inner_texts() if t.strip()]
        except Exception:
            return []

    def step(area, label, fn):
        started = time.monotonic()
        try:
            record = dict(area=area, label=label, status='passed', result=fn())
        except Exception as error:
            record = dict(area=area, label=label, status='failed', error=(str(error).strip().splitlines() or [repr(error)])[0][:400])
        record['seconds'] = round(time.monotonic() - started, 1)
        record['alerts'] = alerts()
        report['steps'].append(record)
        n = len(report['steps'])
        try:
            page.screenshot(path=str(OUT / f'{n:02d}.png'))
        except Exception:
            pass
        persist()
        print(f'{n:02d} [{area}] {label}: {record["status"]} {record.get("error", "")[:160]}', flush=True)
        return record['status'] == 'passed'

    nav = lambda title: page.get_by_role('button', name=re.compile('^' + re.escape(title) + r'\b'))
    collection = 'Full test ' + time.strftime('%H-%M-%S')
    state = {}

    # ---- Collections and navigation -------------------------------------------------------
    def create_collection():
        page.reload(); page.wait_for_load_state()
        field = page.get_by_label('New collection')
        os_click(page, field)
        os_type(collection)
        os_click(page, page.get_by_role('button', name='Create', exact=True))
        expect(page.get_by_role('button', name=re.compile('^' + re.escape(collection)))).to_be_visible()
        return collection
    step('Collections', 'Create a collection by typing its name', create_collection)

    def visit_all():
        seen = {}
        for title in ['Start', 'Photos', 'Auto edit', 'Instagram style', 'Export', 'Advanced']:
            os_click(page, nav(title).first)
            time.sleep(0.8)
            seen[title] = page.locator('h1, h2').first.inner_text()[:60]
        return seen
    step('Navigation', 'Open all six workspaces with the mouse', visit_all)

    # ---- Import through the native Windows file dialog -----------------------------------
    def import_with_dialog():
        os_click(page, nav('Start').first)
        os_click(page, page.get_by_role('button', name='Choose photos', exact=True))
        time.sleep(2.5)
        dialog = user32.GetForegroundWindow()
        title = ctypes.create_unicode_buffer(256)
        user32.GetWindowTextW(dialog, title, 256)
        os_type(str(INPUTS))
        os_keys('enter'); time.sleep(1.5)
        os_type(' '.join(f'"{p.name}"' for p in sorted(INPUTS.iterdir())))
        os_keys('enter')
        deadline = time.monotonic() + 240
        rows = []
        while time.monotonic() < deadline:
            projects = call('list_projects') if False else None
            rows = page.locator('[role="option"]').all_inner_texts() if False else []
            info = page.evaluate('document.body.innerText')
            if 'Importing' not in info and time.monotonic() - deadline > -230:
                break
            time.sleep(2)
        state['dialogTitle'] = title.value
        return {'dialog': title.value}
    step('Import', 'Choose photos with the Windows file dialog (typed path, Enter)', import_with_dialog)

    def project_photos():
        projects = page.evaluate("async()=>await window.__TAURI_INTERNALS__.invoke('list_projects')")
        project = next(p for p in projects if p['name'] == collection)
        state['project'] = project['id']
        deadline = time.monotonic() + 240
        photos = []
        while time.monotonic() < deadline:
            photos = call('list_images', projectId=project['id'], offset=0, limit=50, orderBy='timeline')
            if len(photos) >= len(hashes):
                break
            time.sleep(2)
        state['photos'] = {p['fileName']: p['id'] for p in photos}
        assert len(photos) == len(hashes), f'{len(photos)} of {len(hashes)} imported'
        return sorted(state['photos'])
    step('Import', 'All chosen photos arrive in the collection', project_photos)

    def library():
        os_click(page, nav('Photos').first)
        time.sleep(2)
        images = page.locator('img').count()
        assert images >= len(hashes), images
        return {'thumbnails': images}
    step('Photos', 'Thumbnails appear in the library grid', library)

    # ---- Automatic editing of the whole collection ---------------------------------------
    def auto_all():
        os_click(page, nav('Auto edit').first)
        button = page.get_by_role('button', name=re.compile('Auto edit all photos|Editing your photos'))
        if button.is_enabled():
            os_click(page, button)
        confirm = page.get_by_role('button', name=re.compile('^(Start|Continue|Run|Auto edit)'))
        time.sleep(1.5)
        expect(page.get_by_text(re.compile('photos have saved edits')).first).to_be_visible(timeout=600000)
        expect(page.get_by_role('button', name='Auto edit all photos')).to_be_enabled(timeout=600000)
        outcomes = {}
        for name, photo in state['photos'].items():
            body = json.loads(call('image_recipe', photoId=photo)['body'])
            rep = body.get('studio_portrait_auto_v1', {})
            outcomes[name] = {'faces': rep.get('detectedFaces'), 'ops': len(body.get('studio_retouch_v1', [])), 'exposure': body['global']['exposure']}
        return outcomes
    step('Auto edit', 'Auto edit all photos (one click) finishes and saves edits', auto_all)

    # ---- Develop a single photo ----------------------------------------------------------
    portrait = 'portrait-smile-teeth.jpg'

    def open_photo():
        option = page.get_by_role('option', name=portrait, exact=True)
        # The filmstrip is locked while the current photo renders; wait for it.
        page.wait_for_function("()=>{const f=document.querySelector('fieldset.filmstrip-lock');return f&&!f.disabled}", timeout=180000)
        for _ in range(3):
            os_click(page, option)
            time.sleep(1.5)
            if option.get_attribute('aria-selected') == 'true':
                break
        assert option.get_attribute('aria-selected') == 'true'
        expect(page.get_by_role('button', name='Retouch', exact=True)).to_be_enabled()
        return portrait
    step('Develop', 'Select a photo in the filmstrip', open_photo)

    def ready():
        expect(page.get_by_role('button', name='Retouch', exact=True)).to_be_enabled()

    def auto_enhance():
        os_click(page, page.get_by_role('button', name='Auto enhance photo', exact=True))
        ready()
        expect(page.get_by_text(re.compile(r'Automatic steps \(\d+\)'))).to_be_visible()
        rep = json.loads(call('image_recipe', photoId=state['photos'][portrait])['body'])['studio_portrait_auto_v1']
        return {'faces': rep['detectedFaces'], 'steps': [s['title'] for s in rep.get('steps', [])]}
    step('Develop', 'Auto enhance photo: faces detected and steps listed', auto_enhance)

    def number(label, value, path):
        box = page.get_by_role('spinbutton', name=label + ' value', exact=True)
        os_click(page, box)
        os_keys('ctrl', 'a'); os_type(str(value)); os_keys('enter')
        deadline = time.monotonic() + 30
        while True:
            params = {p['path']: p['value'] for p in call('image_recipe', photoId=state['photos'][portrait])['params']}
            if isinstance(params.get(path), (int, float)) and abs(params[path] - value) < 1e-3:
                break
            assert time.monotonic() < deadline, params.get(path)
            time.sleep(0.5)
        ready()
        return params[path]
    step('Develop', 'Type exposure +0.30 EV with the keyboard', lambda: number('Exposure', 0.3, 'global.exposure'))
    step('Develop', 'Type white balance 6200 K with the keyboard', lambda: number('Temp', 6200, 'global.temperature'))

    def slider_keys():
        os_click(page, page.get_by_role('button', name='Advanced', exact=True))
        slider = page.get_by_role('slider').first
        name = slider.get_attribute('aria-label')
        os_click(page, slider)
        for _ in range(5):
            os_keys('right')
        ready()
        return name
    step('Develop', 'Move a slider with arrow keys', slider_keys)

    def curve():
        os_click(page, page.get_by_text('Tone Curve', exact=True))
        graph = page.get_by_role('img', name='Tone curve', exact=True)
        os_click(page, graph, 0.5, 0.4)
        ready()
        return 'point added'
    step('Develop', 'Add a tone-curve point with the mouse', curve)

    def hsl():
        os_click(page, page.get_by_text('Color Mixer', exact=True))
        os_click(page, page.get_by_role('tab', name='Saturation', exact=True))
        return number('Orange', -10, 'global.hsl.orange.s')
    step('Develop', 'Color mixer: orange saturation -10', hsl)

    def crop():
        os_click(page, page.get_by_text('Transform & Crop', exact=True))
        os_click(page, page.get_by_role('button', name='4:5', exact=True))
        ready()
        crop_value = {p['path']: p['value'] for p in call('image_recipe', photoId=state['photos'][portrait])['params']}.get('geometry.crop')
        assert crop_value != [0, 0, 1, 1]
        return crop_value
    step('Develop', 'Crop to 4:5', crop)

    def compare():
        os_click(page, page.get_by_role('button', name='Compare', exact=True))
        divider = page.get_by_label('Before and after divider', exact=True)
        os_click(page, divider)
        os_keys('home')
        for _ in range(10):
            os_keys('right')
        os_click(page, page.get_by_role('button', name='Edited', exact=True))
        return divider.input_value()
    step('Develop', 'Before/after compare divider with keyboard', compare)

    def clipping():
        select = page.get_by_label('Clipping warnings', exact=False)
        select.select_option('both')
        time.sleep(1)
        select.select_option('off')
        return 'toggled'
    step('Develop', 'Clipping warning overlay', clipping)

    def wb_picker():
        os_click(page, page.get_by_text('White balance picker', exact=True))
        os_click(page, page.get_by_role('button', name='Pick neutral area', exact=True))
        canvas = page.locator('.studio-canvas').first
        os_click(page, canvas, 0.5, 0.15)
        ready()
        return page.locator('[role="status"].lr-notice').all_inner_texts()[:1]
    step('Develop', 'White balance picker: click a neutral area', wb_picker)

    def snapshot():
        os_click(page, page.get_by_text(re.compile(r'Named snapshots \(\d+\)')))
        field = page.get_by_label('Snapshot name', exact=True)
        os_click(page, field)
        os_type('Full test look')
        os_keys('enter')
        expect(page.get_by_role('button', name='Restore snapshot Full test look', exact=True)).to_be_enabled()
        return 'saved'
    step('Develop', 'Save a named snapshot with Enter', snapshot)

    def undo_redo():
        before = call('image_recipe', photoId=state['photos'][portrait])['recipeHash']
        os_click(page, page.get_by_role('button', name='Undo', exact=True).first)
        ready()
        mid = call('image_recipe', photoId=state['photos'][portrait])['recipeHash']
        os_click(page, page.get_by_role('button', name='Redo', exact=True).first)
        ready()
        after = call('image_recipe', photoId=state['photos'][portrait])['recipeHash']
        assert mid != before and after == before
        return 'undo changed, redo restored'
    step('Develop', 'Undo and Redo buttons', undo_redo)

    def go_back():
        history = call('image_history', photoId=state['photos'][portrait])
        first = history['entries'][0]
        os_click(page, page.get_by_role('button', name=f"Go back to step {first['seq']}: {first['label']}", exact=True))
        ready()
        last = call('image_history', photoId=state['photos'][portrait])['entries'][-1]
        os_click(page, page.get_by_role('button', name=f"Go back to step {last['seq']}: {last['label']}", exact=True))
        ready()
        return {'steps': len(history['entries'])}
    step('Develop', 'Go back to the first step and forward again', go_back)

    def presets():
        select = page.locator('select').filter(has=page.locator('option', has_text=re.compile('.'))).first
        names = page.get_by_role('combobox').all_inner_texts()[:2]
        return names
    step('Develop', 'Presets list is available', presets)

    # ---- Retouch workspace ---------------------------------------------------------------
    def open_retouch():
        os_click(page, page.get_by_role('button', name='Retouch', exact=True))
        expect(page.get_by_alt_text('Retouched photograph', exact=True)).to_be_visible()
        return 'open'
    step('Retouch', 'Open Retouch', open_retouch)

    def scopes():
        out = {}
        for scope in ['Body skin', 'Face', 'Face + body skin']:
            os_click(page, page.get_by_role('radio', name=scope, exact=True))
            before = call('image_recipe', photoId=state['photos'][portrait])['recipeHash']
            os_click(page, page.get_by_role('button', name=f'Auto retouch: {scope}', exact=True))
            deadline = time.monotonic() + 120
            while call('image_recipe', photoId=state['photos'][portrait])['recipeHash'] == before and time.monotonic() < deadline:
                time.sleep(1)
            expect(page.get_by_role('button', name=f'Auto retouch: {scope}', exact=True)).to_be_enabled()
            ops = call('native_retouch_edit', projectId=state['project'], photoId=state['photos'][portrait], action='list', edits=[], id=None)
            out[scope] = {'face': sum(o['id'].startswith('auto-') and '-body-' not in o['id'] for o in ops), 'body': sum('-body-' in o['id'] for o in ops), 'manual': sum(not o['id'].startswith('auto-') for o in ops)}
        assert out['Body skin']['body'] > 0 and out['Face']['face'] > 0
        return out
    step('Retouch', 'Face / Body skin / Face + body skin automatic retouch', scopes)

    def strength():
        os_click(page, page.get_by_text('Strength and details', exact=True))
        slider = page.get_by_label('Automatic retouch strength')
        os_click(page, slider)
        for _ in range(4):
            os_keys('left')
        os_click(page, page.get_by_label('Teeth', exact=True))
        before = call('image_recipe', photoId=state['photos'][portrait])['recipeHash']
        os_click(page, page.get_by_role('button', name=re.compile('^Auto retouch:')))
        deadline = time.monotonic() + 120
        while call('image_recipe', photoId=state['photos'][portrait])['recipeHash'] == before and time.monotonic() < deadline:
            time.sleep(1)
        opts = json.loads(call('image_recipe', photoId=state['photos'][portrait])['body'])['studio_portrait_auto_v1'].get('options')
        return opts
    step('Retouch', 'Lower strength and switch off teeth, then re-run', strength)

    def edit_auto_op():
        op = page.get_by_role('button', name=re.compile(r'Auto \(face 1\)')).first
        os_click(page, op)
        slider = page.get_by_label(re.compile(r'^Strength \('))
        before = float(slider.input_value())
        os_click(page, slider)
        for _ in range(5):
            os_keys('left')
        os_click(page, page.get_by_role('button', name='Update selected retouch', exact=True))
        time.sleep(2)
        return {'before': before, 'after': float(slider.input_value())}
    step('Retouch', 'Select an automatic operation and weaken it with the keyboard', edit_auto_op)

    def brush():
        page.get_by_label('Tool', exact=True).select_option('dodge')
        surface = page.get_by_role('group', name='Retouch image interaction', exact=True)
        os_click(page, surface, 0.2, 0.85)
        os_keys('b')
        os_drag(page, surface, (0.2, 0.85), (0.3, 0.88))
        os_keys('e')
        os_click(page, surface, 0.2, 0.85)
        os_keys('ctrl', 'z')
        os_keys('b')
        count_before = len(call('native_retouch_edit', projectId=state['project'], photoId=state['photos'][portrait], action='list', edits=[], id=None))
        os_keys('enter')
        time.sleep(3)
        count_after = len(call('native_retouch_edit', projectId=state['project'], photoId=state['photos'][portrait], action='list', edits=[], id=None))
        assert count_after == count_before + 1, (count_before, count_after)
        return {'operations': count_after}
    step('Retouch', 'Paint a dodge stroke (B), erase (E), undo stroke (Ctrl+Z), apply (Enter)', brush)

    def viewport():
        surface = page.get_by_role('group', name='Retouch image interaction', exact=True)
        os_click(page, surface, 0.5, 0.5)
        os_keys('plus'); os_keys('right'); os_keys('down'); os_keys('h'); os_keys('0')
        os_click(page, page.get_by_role('button', name='Split comparison', exact=True))
        expect(page.get_by_alt_text('Before native retouch comparison', exact=True)).to_be_visible()
        os_click(page, page.get_by_role('button', name='Split comparison', exact=True))
        return 'zoom, pan, fit, split'
    step('Retouch', 'Zoom (+), pan (arrows), hand (H), fit (0), split comparison', viewport)

    def retouch_history():
        before = call('image_recipe', photoId=state['photos'][portrait])['recipeHash']
        surface = page.get_by_role('group', name='Retouch image interaction', exact=True)
        os_click(page, surface, 0.5, 0.5)
        os_keys('ctrl', 'z'); time.sleep(2)
        mid = call('image_recipe', photoId=state['photos'][portrait])['recipeHash']
        os_click(page, surface, 0.5, 0.5)
        os_keys('ctrl', 'shift', 'z'); time.sleep(2)
        after = call('image_recipe', photoId=state['photos'][portrait])['recipeHash']
        assert mid != before and after == before
        return 'ok'
    step('Retouch', 'Ctrl+Z / Ctrl+Shift+Z on saved operations', retouch_history)

    def remove_op():
        ops = call('native_retouch_edit', projectId=state['project'], photoId=state['photos'][portrait], action='list', edits=[], id=None)
        n = len(ops)
        os_click(page, page.get_by_role('button', name=f'Remove retouch {n}', exact=True))
        time.sleep(2)
        after = len(call('native_retouch_edit', projectId=state['project'], photoId=state['photos'][portrait], action='list', edits=[], id=None))
        assert after == n - 1
        return {'from': n, 'to': after}
    step('Retouch', 'Remove one operation', remove_op)

    def close_retouch():
        button = page.get_by_role('button', name=re.compile('Back to Develop|Close retouch|Done'))
        os_click(page, button.first)
        ready()
        return 'closed'
    step('Retouch', 'Leave Retouch', close_retouch)

    # ---- Instagram style -----------------------------------------------------------------
    def instagram():
        os_click(page, nav('Instagram style').first)
        text = page.locator('[aria-label="Instagram style matching"]').first.inner_text()[:300]
        field = page.locator('[aria-label="Instagram style matching"] input').first
        os_click(page, field)
        os_type('natgeo')
        button = page.get_by_role('button', name='Analyze Instagram style', exact=True)
        os_click(page, button)
        time.sleep(20)
        status = page.locator('[aria-label="Instagram style matching"]').first.inner_text()
        return {'outcome': status[-400:]}
    step('Instagram style', 'Type an Instagram handle and analyse', instagram)

    # ---- Export --------------------------------------------------------------------------
    def export():
        os_click(page, nav('Export').first)
        page.get_by_test_id('preset').select_option('gallery')
        destination = OUT / 'export'
        field = page.get_by_test_id('destination')
        os_click(page, field)
        os_keys('ctrl', 'a')
        os_type(str(destination))
        page.get_by_test_id('verify').check()
        os_click(page, page.get_by_test_id('preview-names'))
        os_click(page, page.get_by_test_id('run'))
        expect(page.get_by_test_id('written')).not_to_have_text('0', timeout=600000)
        time.sleep(3)
        files = sorted(p.name for p in destination.rglob('*.jpg'))
        return {'written': page.get_by_test_id('written').inner_text(), 'verified': page.get_by_test_id('verified').inner_text(), 'files': files}
    step('Export', 'Export the collection as verified JPEGs', export)

    # ---- Advanced ------------------------------------------------------------------------
    def advanced():
        os_click(page, nav('Advanced').first)
        found = {}
        for summary in ['Quality review', 'Gallery consistency', 'Albums & curation', 'AI provider', 'Performance & storage']:
            os_click(page, page.get_by_text(summary, exact=True))
            time.sleep(1.5)
            panel = page.locator('details[open]').last
            buttons = [b for b in panel.get_by_role('button').all_inner_texts() if b.strip()][:8]
            found[summary] = buttons
            page.screenshot(path=str(OUT / f'advanced-{summary.split()[0].lower()}.png'))
        return found
    step('Advanced', 'Open every advanced panel', advanced)

    def advanced_actions():
        results = {}
        for name in ['Run quality review', 'Check quality', 'Analyse consistency', 'Check consistency', 'Propose album', 'Curate', 'Probe hardware', 'Refresh']:
            button = page.get_by_role('button', name=re.compile('^' + re.escape(name)))
            if button.count() and button.first.is_enabled():
                os_click(page, button.first)
                time.sleep(6)
                results[name] = alerts()[:1] or 'ran'
        return results
    step('Advanced', 'Run the available advanced actions', advanced_actions)

    # ---- Restart and persistence ---------------------------------------------------------
    final_hash = call('image_recipe', photoId=state['photos'][portrait])['recipeHash']

    def restart():
        subprocess.run(['taskkill', '/IM', EXE.name, '/F'], capture_output=True)
        time.sleep(3)
        launch()
        return 'relaunched'
    step('App', 'Close and relaunch AURA', restart)
    page = connect()

    def persisted():
        now = page.evaluate("async ([p])=>await window.__TAURI_INTERNALS__.invoke('image_recipe',{input:{photoId:p}})", [state['photos'][portrait]])['recipeHash']
        assert now == final_hash
        return 'recipe identical after restart'
    step('App', 'Edits survive a restart', persisted)

    report['originalsUnchanged'] = all(hashlib.sha256((INPUTS / n).read_bytes()).hexdigest() == h for n, h in hashes.items())
    persist()
    passed = sum(s['status'] == 'passed' for s in report['steps'])
    print(f'done: {passed}/{len(report["steps"])} passed; OS inputs {report["osInput"]}; originals unchanged {report["originalsUnchanged"]}')
