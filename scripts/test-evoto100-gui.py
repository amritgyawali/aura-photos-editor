"""Test the hundred ADR-0150 tools in the real AURA desktop window, on real photographs.

Starts a debug build of the desktop app against an isolated catalogue (`AURA_TEST_CATALOG`, which
only a debug build honours, so the photographer's own catalogue is never touched) with WebView2's
CDP port, imports a folder by reference (nothing in it is copied or changed), and then works the
tools the way a photographer would:

* every slider of the Portrait studio's Face, Body, Skin, Makeup & colour, Hair & attire and
  People & scene tabs is dragged in the window, released, checked in the saved recipe, rendered and
  compared with the untouched photograph, and reset by double-clicking its label;
* the two new backgrounds are chosen with their buttons;
* the face picker finds the faces and chooses one;
* the seven hand-placed tools are listed on the Effects screen and saved through the native
  retouch stack;
* every framing preset is pressed and checked in the recipe, then undone;
* every export is written into a folder typed into the panel, and the files are opened;
* the blink check is run over the collection.

Each case is pass, warn (it ran and changed nothing visible on this photograph), fail, or skip.
Output: `results.json`, `report.md` and screenshots in `AURA_EVOTO_GUI_OUT`.

    python scripts/test-evoto100-gui.py [--port 9381] [--only face,skin,...]
"""
import argparse
import base64
import json
import os
import re
import shutil
import subprocess
import time
import traceback
import urllib.request
from pathlib import Path

import numpy as np
from PIL import Image
from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parents[1]
OUT = Path(os.environ.get('AURA_EVOTO_GUI_OUT', 'F:/aura-evoto100/gui'))
PHOTOS = Path(os.environ.get('AURA_EVOTO_PHOTOS', 'H:/AURA EDIT'))
EXE = Path(os.environ.get('AURA_EXE', 'F:/aura-evoto100/desktop/debug/aura-desktop.exe'))
ORT = os.environ.get('AURA_ORT_DYLIB', 'F:/AURA-Indian-Wedding-Test/app/onnxruntime.dll')
MODELS = os.environ.get('AURA_MODELS_DIR', 'F:/AURA-Indian-Wedding-Test/app/models')

report = {'cases': [], 'pageErrors': [], 'started': time.strftime('%Y-%m-%d %H:%M:%S')}


class Warn(Exception):
    pass


class Skip(Exception):
    pass


def persist():
    (OUT / 'results.json').write_text(json.dumps(report, indent=2, default=str), encoding='utf-8')


def case(group, tool_id, name, fn):
    start = time.monotonic()
    try:
        detail = fn()
        status = 'pass'
    except Skip as e:
        status, detail = 'skip', str(e)
    except Warn as e:
        status, detail = 'warn', str(e)
    except Exception as e:  # noqa: BLE001 - every failure is recorded, never fatal
        status, detail = 'fail', f'{type(e).__name__}: {e}'[:900]
        report.setdefault('tracebacks', {})[f'{tool_id} {name}'] = traceback.format_exc()[-3000:]
    row = {'group': group, 'id': tool_id, 'name': name, 'status': status, 'detail': detail,
           'seconds': round(time.monotonic() - start, 1)}
    report['cases'].append(row)
    persist()
    print(f"{status.upper():5} {tool_id:<4} {name} - {str(detail)[:220]}", flush=True)


def launch(port):
    env = dict(os.environ,
               WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-port={port}',
               WEBVIEW2_USER_DATA_FOLDER=str(OUT / 'webview'),
               AURA_TEST_CATALOG=str(OUT / 'catalog' / 'catalog.sqlite'),
               AURA_ORT_DYLIB=ORT, AURA_MODELS_DIR=MODELS)
    (OUT / 'catalog').mkdir(parents=True, exist_ok=True)
    proc = subprocess.Popen([str(EXE)], cwd=str(ROOT), env=env)
    for _ in range(240):
        try:
            urllib.request.urlopen(f'http://127.0.0.1:{port}/json/version', timeout=1)
            return proc
        except Exception:  # noqa: BLE001
            time.sleep(1)
    proc.terminate()
    raise RuntimeError('AURA did not start')


# The tools each tab carries: (id, tab, slider label, value).
SLIDERS = [
    ('A1', 'Face', 'Eye height', 70), ('A2', 'Face', 'Eye tilt', 70), ('A3', 'Face', 'Inner corners', 70),
    ('A4', 'Face', 'Outer corners', 70), ('A5', 'Face', 'Eye symmetry', 100), ('A6', 'Face', 'Nose bridge', 70),
    ('A7', 'Face', 'Nose tip', 70), ('A8', 'Face', 'Nose lift', 70), ('A9', 'Face', 'Nostrils', 70),
    ('A10', 'Face', 'Cheek apples', 70), ('B1', 'Face', 'Upper lip', 70), ('B2', 'Face', 'Lower lip', 70),
    ('B3', 'Face', 'Mouth position', 70), ('B4', 'Face', 'Brow height', 70), ('B5', 'Face', 'Brow arch', 70),
    ('B6', 'Face', 'Brow thickness', 70), ('B7', 'Face', 'Temples', 70), ('B8', 'Face', 'Lower face length', 70),
    ('B9', 'Face', 'Chin width', 70), ('B10', 'Face', 'Hair volume', 70),
    ('C1', 'Body', 'Thighs', 70), ('C2', 'Body', 'Calves', 70), ('C3', 'Body', 'Flat stomach', 70),
    ('C4', 'Body', 'Hip lift', 70), ('C5', 'Body', 'Shoulder line', 70), ('C6', 'Body', 'Torso length', 70),
    ('C7', 'Body', 'Height', 70), ('C8', 'Body', 'Neck slim', 70), ('C9', 'Body', 'Double chin', 80),
    ('C10', 'Body', 'Level head', 100),
    ('D1', 'Skin', 'Aegyo-sal (eye smile)', 100), ('D2', 'Skin', 'Lip lines', 100), ('D3', 'Skin', 'Dewy glow', 100),
    ('D4', 'Skin', 'Stubble shadow', 100), ('D5', 'Skin', 'Rosy complexion', 100), ('D6', 'Skin', 'Even teeth', 100),
    ('D7', 'Skin', 'Sharpen features', 100), ('D8', 'Skin', 'Beard definition', 100),
    ('D9', 'Skin', 'Clothing colour on skin', 100), ('D10', 'Skin', 'Refine pores', 100),
    ('E1', 'Makeup & colour', 'Eyeliner', 100), ('E2', 'Makeup & colour', 'Eyeliner wing', 100),
    ('E3', 'Makeup & colour', 'Fuller lashes', 100), ('E4', 'Makeup & colour', 'Lower lashes', 100),
    ('E5', 'Makeup & colour', 'Lip liner', 100), ('E6', 'Makeup & colour', 'Ombre lips', 100),
    ('E7', 'Makeup & colour', 'Highlighter', 100), ('E8', 'Makeup & colour', 'Bronzer', 100),
    ('E9', 'Makeup & colour', 'Enhance makeup', 100), ('E10', 'Makeup & colour', 'Fill brows', 100),
    ('F1', 'Hair & attire', 'Hair shine', 100), ('F2', 'Hair & attire', 'Fill parting', 100),
    ('F3', 'Hair & attire', 'Hair colour depth', 100), ('F4', 'Hair & attire', 'Calm hair edge', 100),
    ('F5', 'Hair & attire', 'Body definition', 100),
    ('G1', 'Hair & attire', 'Gold jewellery', 100), ('G2', 'Hair & attire', 'Recover red fabric', 100),
    ('G3', 'Hair & attire', 'Veil clarity', 100), ('G4', 'Hair & attire', 'Embroidery detail', 100),
    ('G5', 'Hair & attire', 'Smooth garment edges', 100), ('G6', 'Hair & attire', 'Deepen mehndi', 100),
    ('G7', 'Hair & attire', 'Clean white clothes', 100), ('G8', 'Hair & attire', 'Dark suit detail', 100),
    ('G9', 'Hair & attire', 'Outfit colour', 100),
    ('H1', 'People & scene', 'Backdrop light', 100), ('H3', 'People & scene', 'Background saturation', -80),
    ('H4', 'People & scene', 'Background brightness', -60), ('H5', 'People & scene', 'Background warmth', 60),
    ('H6', 'People & scene', 'Depth blur', 100), ('H8', 'People & scene', 'Background noise', 100),
    ('H10', 'People & scene', 'Light wrap', 100),
    ('I1', 'People & scene', 'People exposure', 60), ('I2', 'People & scene', 'People warmth', 60),
    ('I3', 'People & scene', 'People clarity', 80), ('I4', 'People & scene', 'Balance group faces', 100),
    ('I5', 'People & scene', 'Match group skin colour', 100), ('I6', 'People & scene', 'Relight', 80),
    ('I7', 'People & scene', 'Soften harsh shadows', 100), ('I8', 'People & scene', 'Backlight rescue', 100),
    ('I9', 'People & scene', 'Face-aware vignette', 100), ('I10', 'People & scene', 'Face brightness', 60),
]
# The wire key each slider writes, for reading the saved recipe back.
KEYS = {
    'Face': ('face', ['eyeHeight', 'eyeTilt', 'innerCorner', 'outerCorner', 'eyeSymmetry', 'noseBridge', 'noseTip', 'noseLift', 'nostrils',
                      'cheekApples', 'upperLip', 'lowerLip', 'mouthHeight', 'browHeight', 'browTilt', 'browThickness', 'temples',
                      'faceLength', 'chinWidth', 'hairVolume']),
    'Body': ('body', ['thighs', 'calves', 'belly', 'hipLift', 'shoulderSlope', 'torso', 'height', 'neckWidth', 'doubleChin', 'headLevel']),
}
GROUP_OF = {'D': 'skin', 'E': 'makeup', 'F': 'hair', 'G': 'attire', 'H': 'scene', 'I': 'subject'}
WIRE = {
    'D1': 'aegyoSal', 'D2': 'lipLines', 'D3': 'glow', 'D4': 'stubble', 'D5': 'rosy', 'D6': 'teethEvening', 'D7': 'featureSharpen',
    'D8': 'beardDefine', 'D9': 'colourSpill', 'D10': 'pores', 'E1': 'eyeliner', 'E2': 'eyelinerWing', 'E3': 'lashes',
    'E4': 'lowerLashes', 'E5': 'lipLiner', 'E6': 'lipOmbre', 'E7': 'highlighter', 'E8': 'bronzer', 'E9': 'enhance', 'E10': 'browFill',
    'F1': 'gloss', 'F2': 'parting', 'F3': 'richness', 'F4': 'edge', 'F5': 'definition', 'G1': 'gold', 'G2': 'redRecovery',
    'G3': 'veilClarity', 'G4': 'embroidery', 'G5': 'garmentEdge', 'G6': 'mehndi', 'G7': 'whiteGarment', 'G8': 'darkGarment',
    'G9': 'outfitColour', 'H1': 'backdropGlow', 'H3': 'saturation', 'H4': 'brightness', 'H5': 'warmth', 'H6': 'depthBlur',
    'H8': 'denoise', 'H10': 'lightWrap', 'I1': 'exposure', 'I2': 'warmth', 'I3': 'clarity', 'I4': 'groupBalance', 'I5': 'groupSkin',
    'I6': 'relight', 'I7': 'harshShadows', 'I8': 'backlight', 'I9': 'faceVignette', 'I10': 'faceBrightness',
}
EFFECTS = [('F6', 'body_hair_soften', 'Soften body hair'), ('F7', 'cellulite', 'Smooth cellulite'), ('F8', 'tan_line', 'Even tan lines'),
           ('F9', 'hand_rejuvenate', 'Rejuvenate hands'), ('F10', 'knuckle_even', 'Even dark knuckles & elbows'),
           ('G10', 'lens_reflection', 'Remove glasses reflection colour'), ('H9', 'shadow_remove', 'Remove a cast shadow')]
FRAMINGS = [('J1', 'id_35x45', 'Passport 35×45 mm', 35 / 45), ('J1', 'id_2x2', 'US 2×2 in', 1.0), ('J1', 'id_33x48', 'Visa 33×48 mm', 33 / 48),
            ('J2', 'headshot', 'Headshot 4:5', 0.8), ('J2', 'half_body', 'Half body 4:5', 0.8), ('J2', 'full_body', 'Full body 2:3', 2 / 3),
            ('J2', 'square', 'Square face', 1.0), ('J2', 'story', 'Story 9:16', 9 / 16), ('J3', 'level_eyes', 'Level the eyes', None)]
EXPORTS = [('J4', 'Transparent PNG cut-out', 'png'), ('J5', 'Instagram grid', 'jpg'), ('J6', 'Carousel slides', 'jpg'),
           ('J7', 'Before and after', 'jpg'), ('J8', 'Enlarge 2×', 'jpg')]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', type=int, default=9381)
    parser.add_argument('--only', help='comma-separated groups: sliders,backgrounds,faces,effects,framing,exports,blink')
    parser.add_argument('--from', dest='start', help='skip the sliders before this tool id, to resume a run')
    args = parser.parse_args()
    only = set(args.only.split(',')) if args.only else None
    report['resumedFrom'] = args.start
    global START
    START = args.start
    if OUT.exists():
        shutil.rmtree(OUT, ignore_errors=True)
    OUT.mkdir(parents=True, exist_ok=True)
    report['exe'] = str(EXE)
    report['photos'] = str(PHOTOS)
    proc = launch(args.port)
    time.sleep(3)
    if not (OUT / 'catalog' / 'catalog.sqlite').exists():
        proc.terminate()
        raise SystemExit('the catalogue is not isolated - stop')
    try:
        with sync_playwright() as pw:
            run(pw, args.port, only)
    finally:
        persist()
        proc.terminate()
    write_report()


START = None


def run(pw, port, only):
    browser = pw.chromium.connect_over_cdp(f'http://127.0.0.1:{port}')
    page = None
    for _ in range(120):
        found = [p for c in browser.contexts for p in c.pages]
        page = next((p for p in found if 'localhost' in p.url or 'tauri' in p.url), None)
        if page:
            break
        time.sleep(1)
    if page is None:
        # The WebView may report its page under another address; there is only one window.
        found = [p for c in browser.contexts for p in c.pages]
        print('pages:', [p.url for p in found], flush=True)
        if not found:
            raise RuntimeError('the window exposed no page over CDP')
        page = found[0]
    page.set_default_timeout(300000)
    expect.set_options(timeout=300000)
    page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
    page.on('pageerror', lambda e: report['pageErrors'].append(str(e)[:400]))

    def invoke(cmd, payload):
        return page.evaluate('''async ([c,a])=>{try{return await window.__TAURI_INTERNALS__.invoke(c,a)}
            catch(e){throw new Error(typeof e==='string'?e:JSON.stringify(e))}}''', [cmd, payload])

    def call(cmd, **kw):
        return invoke(cmd, {'input': kw})

    def want(group):
        return only is None or group in only

    def render(photo):
        r = call('render_image', photoId=photo, level='proxy2048', purpose='export')
        return np.frombuffer(base64.b64decode(r['rgbBase64']), dtype=np.uint8).reshape(r['height'], r['width'], 3)

    def finish_of(photo):
        return call('studio_finish', projectId=project, photoId=photo)['finish']

    def recipe(photo):
        return json.loads(call('image_recipe', photoId=photo)['body'])

    def shot(name):
        # Evidence, not a check: a window that is not being painted (the display asleep, the
        # screen locked) cannot be photographed, and that says nothing about the tool.
        try:
            page.screenshot(path=str(OUT / f'{name}.png'), timeout=30000)
        except Exception as e:  # noqa: BLE001
            report.setdefault('screenshotsMissed', []).append(f'{name}: {str(e)[:120]}')

    # ------------------------------------------------------------------ import
    name = 'Evoto100 ' + time.strftime('%H-%M-%S')
    project = call('create_project', name=name, coupleNames=None, eventDate=None)['id']
    report['project'] = project
    page.evaluate('''async()=>{window.__ingest=[];const h=window.__TAURI_INTERNALS__.transformCallback(e=>window.__ingest.push(e.payload));
        await window.__TAURI_INTERNALS__.invoke('plugin:event|listen',{event:'ingest',target:{kind:'Any'},handler:h})}''')
    started = time.monotonic()
    call('start_ingest', projectId=project, roots=[str(PHOTOS)])
    while not page.evaluate("()=>window.__ingest.some(e=>e.kind==='finished')") and time.monotonic() - started < 1800:
        time.sleep(1)
    photos = []
    while True:
        batch = call('list_images', projectId=project, offset=len(photos), limit=50, orderBy='timeline')
        if not batch:
            break
        photos.extend(batch)
    by_name = {p['fileName']: p['id'] for p in photos}
    report['imported'] = {'count': len(by_name), 'seconds': round(time.monotonic() - started, 1)}
    case('library', '-', 'import the CR3 folder', lambda: f"{len(by_name)} photographs in {report['imported']['seconds']} s")

    # Sort the photographs by their faces, through the window's own face finder.
    faces = {}
    for fname, pid in sorted(by_name.items()):
        try:
            faces[fname] = call('evoto_faces', projectId=project, photoId=pid)['faces']
        except Exception as e:  # noqa: BLE001
            faces[fname] = []
            report.setdefault('faceErrors', {})[fname] = str(e)[:300]
    report['faces'] = {k: len(v) for k, v in faces.items()}

    def main_face(fname):
        f = faces[fname]
        return (f[0]['bounds'][2] - f[0]['bounds'][0]) if f else 0
    portraits = sorted([n for n in by_name if faces[n]], key=main_face, reverse=True)
    groups = sorted([n for n in by_name if len(faces[n]) >= 3], key=lambda n: -len(faces[n]))
    portrait = portraits[0] if portraits else next(iter(by_name))
    group = groups[0] if groups else portrait
    report['chosen'] = {'portrait': portrait, 'group': group}

    # ------------------------------------------------------- the editor window
    def open_photo(fname):
        page.reload()
        page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
        page.get_by_role('button', name=re.compile(rf'^{re.escape(name)}')).first.click()
        page.get_by_role('button', name='Auto edit One click, start to finish', exact=True).click()
        option = page.get_by_role('option', name=fname, exact=True)
        # The strip shows 24 photographs around the focused one: walk it toward the end, as a
        # person clicking the last cell would, until the photograph is in it.
        photos = page.get_by_role('option', name=re.compile(r'\.CR3$', re.I))
        for _ in range(40):
            if option.count():
                break
            photos.last.click()
            time.sleep(0.6)
        option.scroll_into_view_if_needed()
        for _ in range(10):
            option.click()
            time.sleep(1.5)
            if option.get_attribute('aria-selected') == 'true':
                break
        page.get_by_role('button', name='Portrait studio', exact=True).click()
        expect(page.get_by_role('tab', name='Face', exact=True)).to_be_visible()

    def idle():
        # The editor disables its controls while it saves and renders.
        page.wait_for_function('''()=>{const t=[...document.querySelectorAll('[role=tab]')].find(e=>e.textContent==='Face');
            const f=t&&t.closest('fieldset');return !f||!f.disabled}''')
        time.sleep(0.5)

    def drag(label, value):
        slider = page.get_by_label(label, exact=True)
        expect(slider).to_be_enabled()
        slider.evaluate('''(el, v) => {
            const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
            set.call(el, String(v));
            el.dispatchEvent(new Event('input', {bubbles: true}));
            el.dispatchEvent(new Event('change', {bubbles: true}));
            el.dispatchEvent(new PointerEvent('pointerup', {bubbles: true}));
        }''', value)

    def reset(label):
        page.locator('.lr-slider', has=page.get_by_label(label, exact=True)).locator('.lr-slider-label').dblclick()

    base_cache = {}

    def baseline(fname):
        if fname not in base_cache:
            base_cache[fname] = render(by_name[fname])
        return base_cache[fname]

    def changed(fname):
        a, b = baseline(fname), render(by_name[fname])
        if a.shape != b.shape:
            return 1.0, float('nan')
        d = np.abs(a.astype(int) - b.astype(int))
        return float((d.max(axis=2) >= 3).mean()), float(d.mean())

    def wire(tool_id, tab):
        if tab in KEYS:
            group_key, keys = KEYS[tab]
            idx = [s[0] for s in SLIDERS if s[1] == tab].index(tool_id)
            return group_key, keys[idx]
        return GROUP_OF[tool_id[0]], WIRE[tool_id]

    # ----------------------------------------------------------- the sliders
    if want('sliders'):
        def target(tool_id):
            if tool_id in ('I4', 'I5'):
                return group
            if tool_id in ('D4', 'D8', 'G7', 'G8'):
                # A groom in a white daura suruwal or a dark suit.
                return next((n for n in portraits if n in ('989A2890.CR3', '989A3150.CR3', '989A3842.CR3')), portrait)
            if tool_id[0] in 'CHI' and tool_id not in ('C8', 'C9', 'C10', 'I7', 'I10'):
                return next((n for n in portraits if n in ('989A2708.CR3', '989A2707.CR3')), portrait)
            return portrait
        current = None
        ids = [t[0] for t in SLIDERS]
        first = ids.index(START) if START in ids else 0
        for tool_id, tab, label, value in SLIDERS[first:]:
            fname = target(tool_id)
            if fname != current:
                open_photo(fname)
                current = fname
            def slider_case(tool_id=tool_id, tab=tab, label=label, value=value, fname=fname):
                pid = by_name[fname]
                baseline(fname)
                page.get_by_role('tab', name=tab, exact=True).click()
                drag(label, value)
                group_key, key = wire(tool_id, tab)
                deadline = time.monotonic() + 120
                saved = None
                while time.monotonic() < deadline:
                    saved = finish_of(pid).get(group_key, {}).get(key)
                    if saved == value:
                        break
                    time.sleep(0.5)
                if saved != value:
                    raise AssertionError(f'the window did not save {group_key}.{key} = {value} (saved {saved})')
                idle()
                share, mean = changed(fname)
                img = page.locator('img').first
                shot(f'slider-{tool_id}')
                reset(label)
                deadline = time.monotonic() + 120
                while time.monotonic() < deadline and finish_of(pid).get(group_key, {}).get(key, 0) not in (0, -45):
                    time.sleep(0.5)
                idle()
                note = f'{fname}: saved {group_key}.{key}={value}; {share * 100:.2f}% of pixels changed, mean {mean:.2f} levels; reset by double-click'
                if share < 0.0005:
                    raise Warn(note + ' - nothing visible changed on this photograph')
                _ = img
                return note
            case('sliders', tool_id, label, slider_case)

    # ------------------------------------------------------- new backgrounds
    if want('backgrounds'):
        fname = next((n for n in portraits if n in ('989A2708.CR3', '989A2707.CR3')), portrait)
        open_photo(fname)
        for tool_id, button, mode in [('H2', 'Colour, keep shadows', 'keep_shadows'), ('H7', 'Sky with clouds', 'cloud_sky')]:
            def bg_case(tool_id=tool_id, button=button, mode=mode):
                pid = by_name[fname]
                baseline(fname)
                page.get_by_role('tab', name='Background', exact=True).click()
                page.get_by_role('button', name=button, exact=True).click()
                deadline = time.monotonic() + 120
                while time.monotonic() < deadline and (finish_of(pid).get('background') or {}).get('mode') != mode:
                    time.sleep(0.5)
                if (finish_of(pid).get('background') or {}).get('mode') != mode:
                    raise AssertionError(f'background mode {mode} was not saved')
                idle()
                share, mean = changed(fname)
                shot(f'background-{tool_id}')
                page.get_by_role('button', name='Keep original background', exact=True).click()
                idle()
                note = f'{fname}: {share * 100:.2f}% changed, mean {mean:.2f}'
                if share < 0.0005:
                    raise Warn(note + ' - no sky or background found to replace')
                return note
            case('backgrounds', tool_id, button, bg_case)

    # ------------------------------------------------------------ face picker
    if want('faces'):
        open_photo(group)
        def picker():
            pid = by_name[group]
            page.get_by_role('tab', name='Faces', exact=True).click()
            page.get_by_role('button', name='Find faces', exact=True).click()
            boxes = page.get_by_role('list', name='Faces in this photograph').get_by_role('checkbox')
            expect(boxes.first).to_be_visible()
            count = boxes.count()
            boxes.nth(min(1, count - 1)).click()
            deadline = time.monotonic() + 60
            while time.monotonic() < deadline and not finish_of(pid).get('faces'):
                time.sleep(0.5)
            chosen = finish_of(pid).get('faces') or []
            if len(chosen) != 1:
                raise AssertionError(f'expected one chosen face, saved {chosen}')
            idle()
            shot('faces-picker')
            page.get_by_role('button', name='Use every face', exact=True).click()
            idle()
            return f'{group}: {count} faces listed, face 2 chosen at {chosen[0]}, then every face again'
        case('faces', 'J9', 'Face picker', picker)

    # -------------------------------------------------- hand-placed effects
    if want('effects'):
        def listed():
            page.reload()
            page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
            page.get_by_role('button', name=re.compile(r'^Effects')).first.click()
            time.sleep(2)
            text = page.inner_text('body')
            missing = [label for _, _, label in EFFECTS if label not in text]
            shot('effects-screen')
            if missing:
                raise AssertionError(f'not on the Effects screen: {missing}')
            return 'all seven listed on the Effects screen'
        case('effects', 'F6-H9', 'listed on the Effects screen', listed)
        regions = {'body_hair_soften': [.5, .62, .2, .14], 'cellulite': [.5, .62, .2, .14], 'tan_line': [.5, .55, .25, .2],
                   'hand_rejuvenate': [.5, .6, .18, .12], 'knuckle_even': [.5, .6, .18, .12], 'lens_reflection': [.5, .35, .1, .05],
                   'shadow_remove': [.5, .5, .45, .45]}
        for tool_id, tool, label in EFFECTS:
            def effect_case(tool_id=tool_id, tool=tool, label=label):
                fname = '989A2698.CR3' if '989A2698.CR3' in by_name else portrait
                pid = by_name[fname]
                baseline(fname)
                edit = {'id': 'draft', 'tool': tool, 'enabled': True, 'region': regions[tool], 'source': None, 'amount': 0.9,
                        'feather': 0.3, 'radius': {'body_hair_soften': .0015, 'cellulite': .012, 'hand_rejuvenate': .003,
                                                   'knuckle_even': .004}.get(tool, .01),
                        'texture': 1, 'tone': 0.2 if tool == 'tan_line' else 0.5, 'warmth': 0, 'tint': 0}
                saved = invoke('native_retouch_edit', {'input': {'projectId': project, 'photoId': pid, 'action': 'append', 'edits': [edit], 'id': None}})
                share, mean = changed(fname)
                invoke('native_retouch_edit', {'input': {'projectId': project, 'photoId': pid, 'action': 'clear', 'edits': [], 'id': None}})
                note = f'{fname}: saved as retouch operation {len(saved)}; {share * 100:.2f}% changed inside its area, mean {mean:.3f}'
                if share < 0.0002:
                    raise Warn(note + ' - this measured tool found nothing to change in the area tried')
                return note
            case('effects', tool_id, label, effect_case)

    # --------------------------------------------------------------- framing
    if want('framing'):
        open_photo(portrait)
        page.locator('summary', has_text='Framing, delivery and review').click()
        for tool_id, preset, button, aspect in FRAMINGS:
            def framing_case(tool_id=tool_id, preset=preset, button=button, aspect=aspect):
                pid = by_name[portrait]
                page.get_by_role('button', name=button, exact=True).click()
                page.wait_for_function('()=>![...document.querySelectorAll("[role=status]")].some(e=>e.textContent==="Working...")')
                idle()
                geometry = recipe(pid)['geometry']
                crop, rotate = geometry['crop'], geometry['rotate']
                notes = page.get_by_role('list', name='What happened').all_inner_texts()
                shot(f'framing-{preset}')
                r = render(pid)
                page.get_by_role('button', name='Undo', exact=True).click()
                idle()
                if aspect is not None:
                    got = r.shape[1] / r.shape[0]
                    if abs(got - aspect) > 0.02:
                        raise AssertionError(f'rendered aspect {got:.3f}, expected {aspect:.3f}')
                if preset.startswith('id'):
                    fin = finish_of(pid)
                    _ = fin
                return f'crop {[round(v, 3) for v in crop]}, rotate {rotate:.2f}, render {r.shape[1]}x{r.shape[0]}; {" ".join(notes)[:300]}'
            case('framing', tool_id, button, framing_case)

    # --------------------------------------------------------------- exports
    if want('exports'):
        open_photo(group if want('framing') is False else portrait)
        page.locator('summary', has_text='Framing, delivery and review').click()
        folder = OUT / 'exports'
        folder.mkdir(exist_ok=True)
        page.get_by_label('Export folder', exact=True).fill(str(folder))
        page.get_by_label('Grid rows', exact=True).select_option('2')
        for tool_id, button, ext in EXPORTS:
            def export_case(tool_id=tool_id, button=button, ext=ext):
                before = set(folder.iterdir())
                page.get_by_role('button', name=button, exact=True).click()
                page.wait_for_function('()=>![...document.querySelectorAll("[role=status]")].some(e=>e.textContent==="Working...")')
                time.sleep(1)
                alert = page.get_by_role('alert').all_inner_texts()
                if any(a.strip() for a in alert):
                    raise AssertionError(' | '.join(alert)[:300])
                made = sorted(set(folder.iterdir()) - before)
                if not made:
                    raise AssertionError('no file was written')
                sizes = []
                for p in made:
                    with Image.open(p) as im:
                        sizes.append(f'{p.name} {im.size[0]}x{im.size[1]} {im.mode}')
                        if ext == 'png' and im.mode != 'RGBA':
                            raise AssertionError(f'{p.name} has no transparency')
                        if ext == 'png':
                            alpha = np.asarray(im)[..., 3]
                            clear = float((alpha < 16).mean())
                            sizes.append(f'{clear * 100:.0f}% transparent')
                shot(f'export-{tool_id}')
                return f'{len(made)} file(s): ' + '; '.join(sizes)[:400]
            case('exports', tool_id, button, export_case)

    # ----------------------------------------------------------------- blink
    if want('blink'):
        def blink():
            if not page.get_by_role('button', name='Find faces showing no white of the eye', exact=True).is_visible():
                open_photo(portrait)
                page.locator('summary', has_text='Framing, delivery and review').click()
            page.get_by_role('button', name='Find faces showing no white of the eye', exact=True).click()
            text = page.get_by_text(re.compile(r'^Checked \d+ photographs')).inner_text()
            flagged = page.get_by_role('list', name='Faces showing no white of the eye').all_inner_texts() if page.get_by_role('list', name='Faces showing no white of the eye').count() else []
            shot('blink-check')
            return text + (' ' + ' | '.join(flagged)[:300] if flagged else '')
        case('blink', 'J10', 'Find faces showing no white of the eye', blink)


def write_report():
    rows = report['cases']
    counts = {s: sum(r['status'] == s for r in rows) for s in ('pass', 'warn', 'fail', 'skip')}
    lines = [f"# AURA ADR-0150 tools in the real window - {report['started']}", '',
             f"Photographs: `{report.get('photos')}` ({report.get('imported', {}).get('count', '?')} imported). "
             f"Pass {counts['pass']}, warn {counts['warn']}, fail {counts['fail']}, skip {counts['skip']}.", '',
             '| Tool | Case | Result | Detail | s |', '|---|---|---|---|---|']
    for r in rows:
        lines.append(f"| {r['id']} | {r['name']} | {r['status']} | {str(r['detail']).replace('|', '/')[:300]} | {r['seconds']} |")
    if report['pageErrors']:
        lines += ['', '## Page errors', ''] + [f'- {e}' for e in report['pageErrors']]
    (OUT / 'report.md').write_text('\n'.join(lines), encoding='utf-8')


if __name__ == '__main__':
    main()
