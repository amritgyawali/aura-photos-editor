"""Test every tool and feature of AURA in the real desktop window, on real photographs.

Starts the desktop app against an isolated catalogue (a debug build, so `AURA_TEST_CATALOG` is
honoured and the photographer's own catalogue is never touched) with WebView2 CDP, and runs one
recorded case per feature. Clicks go through the WebView's own input pipeline, so the mouse and
keyboard of the machine are left alone. Every case records pass, warn (it ran and did nothing
visible on this photograph, which some tools legitimately do), fail (an error, a wrong result,
or a broken undo), or skip (something this machine cannot provide), with timings, and the page's
own errors. Output: `.work-checks/everything/` - `results.json`, `report.md`, screenshots.

    python scripts/test-everything-gui.py [--port 9373] [--only group,group]

Inputs are copied from `AURA_TEST_SOURCES` (default `F:/AURA-Indian-Wedding-Test/source`, licensed
Wikimedia Commons wedding photographs and Canon CR2 files) and, when present, one FiveK DNG.
"""
import argparse
import base64
import hashlib
import json
import os
import re
import shutil
import subprocess
import time
import traceback
import urllib.request
import uuid
from pathlib import Path

import numpy as np
from PIL import Image, ImageEnhance, ImageOps
from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.work-checks/everything'
SOURCES = Path(os.environ.get('AURA_TEST_SOURCES', 'F:/AURA-Indian-Wedding-Test/source'))
FIVEK = Path(os.environ.get('AURA_FIVEK_DNG', 'C:/Users/Admin/Documents/project/.work-checks/fivek/dng'))

report = {'cases': [], 'pageErrors': [], 'started': time.strftime('%Y-%m-%d %H:%M:%S')}


class Skip(Exception):
    pass


class Warn(Exception):
    pass


def persist():
    (OUT / 'results.json').write_text(json.dumps(report, indent=2, default=str), encoding='utf-8')


def case(group, name, fn):
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
        report.setdefault('tracebacks', {})[f'{group}/{name}'] = traceback.format_exc()[-3000:]
    row = {'group': group, 'name': name, 'status': status, 'detail': detail,
           'seconds': round(time.monotonic() - start, 2)}
    report['cases'].append(row)
    persist()
    print(f"{status.upper():5} {group} / {name} - {str(detail)[:200]}", flush=True)
    return status == 'pass'


def prepare_inputs():
    """Copy a varied set of photographs and make the reference material the look tests need."""
    inputs = OUT / 'inputs'
    inputs.mkdir(parents=True, exist_ok=True)
    manifest = json.loads((SOURCES / 'manifest.json').read_text(encoding='utf-8'))
    wanted = [('indian bride', 3), ('mehndi ceremony', 1), ('sangeet ceremony', 1), ('baraat', 1),
              ('haldi ceremony', 1), ('kerala wedding', 1)]
    chosen = []
    for category, n in wanted:
        rows = [m for m in manifest if m['category'] == category and (SOURCES / m['file']).exists()
                and m['file'] not in chosen and max(m['width'], m['height']) <= 5200]
        chosen += [r['file'] for r in rows[:n]]
    for name in chosen:
        shutil.copy2(SOURCES / name, inputs / name)
    for raw in sorted(SOURCES.glob('*.CR2'))[:2]:
        shutil.copy2(raw, inputs / raw.name)
    dng = sorted(FIVEK.glob('*.dng'))[:1] if FIVEK.exists() else []
    for raw in dng:
        shutil.copy2(raw, inputs / raw.name)
    # One PNG, made from the first bride photograph.
    with Image.open(inputs / chosen[0]) as im:
        ImageOps.exif_transpose(im).convert('RGB').save(inputs / 'bride-as-png.png')
    report['inputs'] = sorted(p.name for p in inputs.iterdir())
    report['inputHashes'] = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in inputs.iterdir()}
    # A finished version of the PNG for the paired match: brighter and warmer, same pixels.
    paired = OUT / 'paired'
    paired.mkdir(exist_ok=True)
    with Image.open(inputs / 'bride-as-png.png') as im:
        warm = ImageEnhance.Brightness(im).enhance(1.12)
        r, g, b = warm.split()
        r = r.point(lambda v: min(255, int(v * 1.06)))
        b = b.point(lambda v: int(v * 0.92))
        Image.merge('RGB', (r, g, b)).save(paired / 'bride-finished.png')
    # Eight photographs graded with one look, for "match a look" and the reference style.
    looks = OUT / 'look-references'
    looks.mkdir(exist_ok=True)
    others = [m['file'] for m in manifest if m['file'] not in chosen and (SOURCES / m['file']).exists()][:8]
    for name in others:
        with Image.open(SOURCES / name) as im:
            im = ImageOps.exif_transpose(im).convert('RGB')
            im.thumbnail((1600, 1600))
            im = ImageEnhance.Contrast(im).enhance(1.15)
            r, g, b = im.split()
            im = Image.merge('RGB', (r.point(lambda v: min(255, int(v * 1.08 + 6))), g,
                                     b.point(lambda v: int(v * 0.85))))
            im.save(looks / (Path(name).stem + '.jpg'), quality=92)
    return inputs, chosen


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
        except Exception:  # noqa: BLE001
            time.sleep(1)
    proc.terminate()
    raise RuntimeError('AURA did not start')


def tool_ids():
    """Every retouch tool the window offers, read from the window's own source."""
    text = (ROOT / 'ui/src/ipc/nativeRetouch.ts').read_text(encoding='utf-8')
    base = re.findall(r"^\s*\['([a-z_]+)', '", text.split('export const RETOUCH_TOOLS = [')[1].split('] as const')[0], re.M)
    effects = re.findall(r"^\s*\['([a-z_]+)', '", (ROOT / 'ui/src/ipc/retouchEffects.ts').read_text(encoding='utf-8'), re.M)
    return base + [e for e in effects if e not in base]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', type=int, default=9373)
    parser.add_argument('--only', help='comma-separated groups')
    args = parser.parse_args()
    only = set(args.only.split(',')) if args.only else None
    exe = Path(os.environ.get('AURA_EXE', ROOT / 'target/desktop-launch/debug/aura-desktop.exe'))
    if OUT.exists():
        shutil.rmtree(OUT)
    OUT.mkdir(parents=True)
    inputs, chosen = prepare_inputs()
    report['exe'] = str(exe)
    proc = launch(exe, args.port)
    time.sleep(2)
    assert (OUT / 'catalog' / 'catalog.sqlite').exists(), 'the catalogue is not isolated - stop'
    try:
        with sync_playwright() as pw:
            run(pw, args.port, inputs, chosen, only)
    finally:
        persist()
        proc.terminate()
    write_report()


def run(pw, port, inputs, chosen, only):
    browser = pw.chromium.connect_over_cdp(f'http://127.0.0.1:{port}')
    page = None
    for _ in range(120):
        found = [p for c in browser.contexts for p in c.pages]
        page = next((p for p in found if 'localhost' in p.url or 'tauri' in p.url), None)
        if page:
            break
        time.sleep(1)
    page.set_default_timeout(600000)
    expect.set_options(timeout=600000)
    page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
    page.on('pageerror', lambda e: report['pageErrors'].append(str(e)[:400]))

    def invoke(cmd, payload):
        return page.evaluate('''async ([c,a])=>{try{return await window.__TAURI_INTERNALS__.invoke(c,a)}
            catch(e){throw new Error(typeof e==='string'?e:JSON.stringify(e))}}''', [cmd, payload])

    def call(cmd, **kw):
        return invoke(cmd, {'input': kw})

    def want(group):
        return only is None or group in only

    def proxy(photo):
        r = call('render_image', photoId=photo, level='proxy2048', purpose='interactive')
        return np.frombuffer(base64.b64decode(r['rgbBase64']), dtype=np.uint8).reshape(r['height'], r['width'], 3)

    def recipe_hash(photo):
        return call('image_recipe', photoId=photo)['recipeHash']

    def history(photo, action):
        return call('history_step', projectId=project, photoId=photo, action=action)

    def shot(name):
        page.screenshot(path=str(OUT / f'{name}.png'), full_page=True)

    # ---------------------------------------------------------------- import
    name = 'Everything ' + time.strftime('%H-%M-%S')
    project = call('create_project', name=name, coupleNames=None, eventDate=None)['id']
    report['project'] = project
    page.evaluate('''async()=>{window.__ingest=[];const h=window.__TAURI_INTERNALS__.transformCallback(e=>window.__ingest.push(e.payload));
        await window.__TAURI_INTERNALS__.invoke('plugin:event|listen',{event:'ingest',target:{kind:'Any'},handler:h})}''')
    call('start_ingest', projectId=project, roots=[str(inputs)])
    deadline = time.monotonic() + 900
    while not page.evaluate("()=>window.__ingest.some(e=>e.kind==='finished')") and time.monotonic() < deadline:
        time.sleep(1)
    photos = []
    while True:
        batch = call('list_images', projectId=project, offset=len(photos), limit=50, orderBy='timeline')
        if not batch:
            break
        photos.extend(batch)
    by_name = {p['fileName']: p['id'] for p in photos}
    report['photos'] = by_name

    def imported():
        missing = sorted(set(report['inputs']) - set(by_name))
        if missing:
            raise AssertionError(f'not catalogued: {missing}')
        return f'{len(by_name)} photographs, including {sum(n.endswith(("CR2", "dng")) for n in by_name)} RAW'
    case('library', 'import every format', imported)

    bride = by_name.get('bride-as-png.png') or next(iter(by_name.values()))
    bride_jpeg = by_name.get(chosen[0], bride)
    raws = [v for k, v in by_name.items() if k.endswith(('.CR2', '.dng'))]

    if want('library'):
        for fname, pid in by_name.items():
            def preview(pid=pid):
                thumb = call('get_preview', projectId=project, photoId=pid, level='thumb', priority='interactive')
                original = invoke('photo_original', {'projectId': project, 'photoId': pid, 'quality': 'fast'})
                img = proxy(pid)
                if img.std() < 1:
                    raise AssertionError('the render is a flat colour')
                return f"thumbnail ok, original {original['width']}x{original['height']}, proxy {img.shape[1]}x{img.shape[0]}"
            case('library', f'previews and render: {fname}', preview)
        case('library', 'prefetch previews', lambda: call('prefetch_previews', projectId=project, photoIds=list(by_name.values()), level='thumb'))
        def early_descriptors():
            try:
                invoke('image_descriptors', {'projectId': project, 'photoId': bride})
                return 'already analysed'
            except Exception as e:  # noqa: BLE001 - a refusal is expected before analysis
                if 'not been analysed' not in str(e):
                    raise AssertionError(f'misleading refusal: {str(e)[:200]}')
                return 'refused before analysis, with an accurate message'
        case('library', 'descriptors before analysis', early_descriptors)
        case('library', 'list collections', lambda: f"{len(invoke('list_projects', {}))} collection(s)")

    # ------------------------------------------------------------- auto edit
    if want('auto-edit'):
        for fname, pid in by_name.items():
            def auto(pid=pid):
                before = proxy(pid)
                result = call('photo_auto_edit', projectId=project, photoId=pid, jobId=str(uuid.uuid4()))
                after = proxy(pid)
                moved = float(np.abs(after.astype(int) - before.astype(int)).mean())
                if moved == 0:
                    raise Warn('Auto edit left this photograph exactly as it was')
                return f'mean change {moved:.2f} levels; {str(result)[:120]}'
            case('auto-edit', f'Auto edit: {fname}', auto)
        case('auto-edit', 'photo analysis', lambda: str(call('photo_analysis', projectId=project, photoId=bride, jobId=str(uuid.uuid4())))[:200])
        case('auto-edit', 'enhance photo', lambda: call('enhance_photo', photoId=bride_jpeg)['recipeHash'][:12])
        case('auto-edit', 'enhance portrait', lambda: call('enhance_portrait', photoId=bride_jpeg)['recipeHash'][:12])

    # --------------------------------------------------------------- develop
    if want('develop'):
        dev = bride_jpeg
        params = {'global.exposure': .5, 'global.temperature': 6500, 'global.tint': 20, 'global.contrast': 25,
                  'global.highlights': -40, 'global.shadows': 40, 'global.whites': 20, 'global.blacks': -20,
                  'global.texture': 30, 'global.clarity': 30, 'global.dehaze': 30, 'global.vibrance': 35,
                  'global.saturation': -30, 'global.hsl.red.h': 30, 'global.hsl.orange.s': -40, 'global.hsl.blue.l': 30,
                  'global.curve.points': [[0, 0], [128, 165], [255, 255]],
                  'global.channel_curves.red.points': [[0, 0], [128, 153], [255, 255]],
                  'global.parametric.lights': 30, 'global.sharpen.amount': 70, 'global.noise.luminance': 50,
                  'global.noise.colour': 50, 'global.effects.vignette.amount': -45, 'global.effects.grain.amount': 35,
                  'global.calibration.red_hue': 35, 'global.colour_grade.shadows.saturation': 40,
                  'bw': {'mix': {}, 'grade': None}, 'geometry.rotate': 10, 'geometry.crop': [.1, .1, .8, .8],
                  'lens.distortion': True, 'lens.ca': True, 'lens.vignette': 50}
        history(dev, 'reset_original')
        base = proxy(dev)
        for path, value in params.items():
            def param(path=path, value=value):
                call('set_param', projectId=project, photoId=dev, path=path, value=value, label='Test ' + path)
                after = proxy(dev)
                changed = after.shape != base.shape or not np.array_equal(after, base)
                history(dev, 'undo')
                if not np.array_equal(proxy(dev), base):
                    raise AssertionError('Undo did not restore the photograph exactly')
                if not changed:
                    raise Warn('no visible change at this value')
                return 'changes the photograph; Undo restores it exactly'
            case('develop', f'slider {path}', param)

        def invalid():
            before = recipe_hash(dev)
            try:
                call('set_param', projectId=project, photoId=dev, path='global.exposure', value=99999, label='bad')
            except Exception:  # noqa: BLE001 - refusal is the expected outcome
                if recipe_hash(dev) != before:
                    raise AssertionError('refused but changed the recipe')
                return 'refused, recipe unchanged'
            if recipe_hash(dev) != before:
                return 'clamped to the allowed range'
            raise AssertionError('accepted an impossible exposure')
        case('develop', 'out-of-range value refused', invalid)

        def dehaze_export():
            history(dev, 'reset_original')
            r0 = call('render_image', photoId=dev, level='proxy2048', purpose='export')
            call('set_param', projectId=project, photoId=dev, path='global.dehaze', value=60, label='dehaze')
            r1 = call('render_image', photoId=dev, level='proxy2048', purpose='export')
            a = np.frombuffer(base64.b64decode(r0['rgbBase64']), dtype=np.uint8).astype(int)
            b = np.frombuffer(base64.b64decode(r1['rgbBase64']), dtype=np.uint8).astype(int)
            history(dev, 'reset_original')
            moved = float(np.abs(a - b).mean())
            if moved < 0.5:
                raise Warn(f'dehaze 60 moves the exported photo by only {moved:.2f} levels on average')
            return f'mean change {moved:.2f} levels in the export render'
        case('develop', 'dehaze in the export render', dehaze_export)

        def snapshots():
            call('set_param', projectId=project, photoId=dev, path='global.exposure', value=.4, label='snap')
            call('snapshot', projectId=project, photoId=dev, name='test', action='take')
            at = recipe_hash(dev)
            call('set_param', projectId=project, photoId=dev, path='global.exposure', value=-.4, label='snap2')
            call('snapshot', projectId=project, photoId=dev, name='test', action='restore')
            if recipe_hash(dev) != at:
                raise AssertionError('restored snapshot differs')
            return 'take and restore'
        case('develop', 'snapshots', snapshots)

        def wb_pick():
            img = proxy(dev).astype(float)
            lum = img.mean(axis=2)
            chroma = img.max(axis=2) - img.min(axis=2)
            score = np.where((lum > 90) & (lum < 235), chroma, 999)
            y, x = np.unravel_index(int(np.argmin(score)), score.shape)
            r = call('pick_white_balance', projectId=project, photoId=dev, x=(x + .5) / img.shape[1], y=(y + .5) / img.shape[0])
            # A coloured spot must still be refused.
            score = np.where(lum > 60, -chroma, 999)
            cy, cx = np.unravel_index(int(np.argmin(score)), score.shape)
            try:
                call('pick_white_balance', projectId=project, photoId=dev, x=(cx + .5) / img.shape[1], y=(cy + .5) / img.shape[0])
                refused = False
            except Exception:  # noqa: BLE001
                refused = True
            if not refused:
                raise Warn('a strongly coloured spot was accepted as neutral')
            return f"neutral spot accepted ({r['recipeHash'][:10]}); coloured spot refused"
        case('develop', 'white balance picker', wb_pick)

        profiles = invoke('list_edit_profiles', {})
        report['editProfiles'] = [p.get('id') for p in profiles]
        for prof in profiles:
            def apply_profile(prof=prof):
                history(dev, 'reset_original')
                before = proxy(dev)
                r = call('apply_edit_profile', photoId=dev, profileId=prof['id'], strength=1.0)
                if np.array_equal(proxy(dev), before):
                    raise Warn('the profile changed nothing on this photograph')
                return f"{r.get('changed')} fields; {', '.join(r.get('adaptations', [])[:2])}"[:200]
            case('develop', f"edit profile: {prof.get('name', prof['id'])}", apply_profile)
        case('develop', 'profile preview', lambda: len(call('preview_edit_profile', profileId=profiles[0]['id'], strength=.7, photoId=dev, size=256)['after']) if profiles else (_ for _ in ()).throw(Skip('no profiles')))

        def sync():
            r = invoke('sync_settings', {'input': {'projectId': project, 'sourcePhotoId': dev,
                                                   'targetPhotoIds': [bride], 'includeGeometry': False, 'groups': None}})
            if r['failed']:
                raise AssertionError(r['failed'])
            return f"synced {r['synced']}"
        case('develop', 'sync settings to another photo', sync)
        history(dev, 'reset_original')

    # --------------------------------------------------------- retouch tools
    if want('retouch-tools'):
        rt = bride
        history(rt, 'reset_original')
        call('native_retouch_edit', projectId=project, photoId=rt, action='clear', edits=[], id=None)
        base = proxy(rt)
        quiet = {'red_eye', 'eye_glow_fix', 'eye_clean', 'teeth', 'auto_blemish', 'acne_clear', 'frequency_heal',
                 'defringe', 'vein_reduce', 'glare', 'stretch_marks', 'texture_graft', 'mattify', 'lip_gloss', 'halation',
                 'under_eye', 'chroma_key', 'local_white_balance'}
        for tool in tool_ids():
            def tool_case(tool=tool):
                warps = tool.startswith('liquify_') or tool == 'reshape'
                edit = dict(id='test', tool=tool, enabled=True, region=[.5, .35, .08, .06], source=[.56, .36],
                            amount=.6, feather=.6, radius=.008, texture=.7 if tool != 'local_sharpen' else 1.5,
                            tone=.6, warmth=.4 if warps else .25, tint=.1, targetColor=[.6, .3, .2])
                if tool == 'acne_clear':
                    edit['mask'] = {'strokes': [{'erase': False, 'radius': .03, 'opacity': 1, 'points': [[.5, .35, 1]]}]}
                saved = call('native_retouch_edit', projectId=project, photoId=rt, action='append', edits=[edit], id=None)
                after = proxy(rt)
                changed = int((np.abs(after.astype(int) - base.astype(int)).max(axis=2) > 1).sum())
                history(rt, 'undo')
                if not np.array_equal(proxy(rt), base):
                    raise AssertionError('Undo did not restore the photograph exactly')
                history(rt, 'redo')
                if not np.array_equal(proxy(rt), after):
                    raise AssertionError('Redo did not bring the retouch back exactly')
                call('native_retouch_edit', projectId=project, photoId=rt, action='clear', edits=[], id=None)
                h_, w_ = after.shape[:2]
                crop = after[int(h_ * .2):int(h_ * .5), int(w_ * .35):int(w_ * .65)]
                (OUT / 'tools').mkdir(exist_ok=True)
                Image.fromarray(crop).resize((240, 240)).save(OUT / 'tools' / f'{tool}.png')
                if changed == 0:
                    if tool in quiet:
                        raise Warn('ran; found nothing to change where it was placed (expected for this tool on this spot)')
                    raise AssertionError('saved but changed no pixels')
                return f'{changed} pixels changed; Undo and Redo exact; {len(saved)} op'
            case('retouch-tools', tool, tool_case)

    # ----------------------------------------------- the retouch workspace UI
    def open_retouch(fname):
        page.reload()
        page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
        page.get_by_role('button', name=f'{name} {len(by_name)}', exact=True).click()
        page.get_by_role('button', name='Auto edit One click, start to finish', exact=True).click()
        option = page.get_by_role('option', name=fname, exact=True)
        for _ in range(8):
            option.click()
            time.sleep(1.5)
            if option.get_attribute('aria-selected') == 'true':
                break
        retouch = page.get_by_role('button', name='Retouch', exact=True)
        expect(retouch).to_be_enabled()
        retouch.click()
        expect(page.get_by_alt_text('Retouched photograph', exact=True)).to_be_visible()

    if want('auto-retouch'):
        text = (ROOT / 'ui/src/components/develop/AutoRetouchSettings.tsx').read_text(encoding='utf-8')
        presets = re.findall(r"^\s*\['[a-z_]+', '([^']+)', ", text.split('export const PRESETS')[1].split('];')[0], re.M)
        report['presets'] = presets
        faces = {}
        for fname, pid in by_name.items():
            if fname.lower().endswith(('.jpg', '.png')):
                try:
                    faces[fname] = len(call('analyse_portrait', photoId=pid).get('faces', []))
                except Exception:  # noqa: BLE001
                    faces[fname] = 0
        report['facesFound'] = faces
        with_face = [f for f, n in faces.items() if n > 0]
        case('auto-retouch', 'a face is found on at least one photograph',
             lambda: f'{len(with_face)} of {len(faces)} photographs have a detected face: {with_face}' if with_face
             else (_ for _ in ()).throw(AssertionError(f'no face detected on any photograph: {faces}')))
        portrait_name = with_face[0] if with_face else chosen[0]
        bride_jpeg = by_name[portrait_name]
        try:
            open_retouch(portrait_name)
            history(bride_jpeg, 'reset_original')
            ok = True
        except Exception as e:  # noqa: BLE001
            case('auto-retouch', 'open the Retouch workspace', lambda e=e: (_ for _ in ()).throw(e))
            ok = False
        if ok:
            for label in presets:
                def preset_case(label=label):
                    radio = page.get_by_role('radio', name=label, exact=True)
                    radio.click()
                    button = page.get_by_role('button', name=re.compile(r'^Auto retouch'))
                    before = recipe_hash(bride_jpeg)
                    button.first.click()
                    deadline = time.monotonic() + 900
                    while recipe_hash(bride_jpeg) == before and time.monotonic() < deadline:
                        time.sleep(1)
                    expect(button.first).to_be_enabled()
                    ops = call('native_retouch_edit', projectId=project, photoId=bride_jpeg, action='list', edits=[], id=None)
                    auto = [o for o in ops if o['id'].startswith('auto-')]
                    if recipe_hash(bride_jpeg) == before:
                        raise Warn('saved nothing new (the same plan as the previous preset)')
                    if not auto:
                        raise AssertionError('no automatic retouch operations were saved')
                    return f'{len(auto)} automatic operations'
                case('auto-retouch', f'preset: {label}', preset_case)
            shot('auto-retouch-presets')

            def strength():
                panel = page.get_by_role('region', name='Retouch strength')
                expect(panel).to_be_visible()
                sliders = panel.get_by_role('slider')
                count = sliders.count()
                before = recipe_hash(bride_jpeg)
                sliders.first.focus()
                page.keyboard.press('Home')
                deadline = time.monotonic() + 300
                while recipe_hash(bride_jpeg) == before and time.monotonic() < deadline:
                    time.sleep(.5)
                if recipe_hash(bride_jpeg) == before:
                    raise AssertionError('moving a strength slider saved nothing')
                expect(panel.get_by_role('button', name='Reset to Auto retouch')).to_be_enabled()
                panel.get_by_role('button', name='Reset to Auto retouch').click()
                time.sleep(3)
                return f'{count} sliders; moved, saved and reset'
            case('auto-retouch', 'retouch strength sliders', strength)

            def portrait_button():
                before = recipe_hash(bride_jpeg)
                page.get_by_role('button', name='Auto portrait', exact=True).click()
                deadline = time.monotonic() + 600
                while recipe_hash(bride_jpeg) == before and time.monotonic() < deadline:
                    time.sleep(1)
                return 'saved' if recipe_hash(bride_jpeg) != before else (_ for _ in ()).throw(Warn('no change'))
            case('auto-retouch', 'Auto portrait button', portrait_button)
            shot('retouch-workspace')

        def advanced():
            r = invoke('auto_advanced_retouch', {'input': {'projectId': project, 'photoId': bride_jpeg, 'options': None}})
            stages = r['report']['stages']
            if len(stages) != 18:
                raise AssertionError(f'{len(stages)} stages')
            applied = sum(s['outcome'] == 'applied' for s in stages)
            return f"18 stages, {applied} applied; quality passed={r['report']['quality'].get('passed')}"
        case('auto-retouch', 'Auto advanced retouch (18 stages)', advanced)
        case('auto-retouch', 'analyse portrait', lambda: str(call('analyse_portrait', photoId=bride_jpeg))[:160])
        for style in ['natural', 'soft', 'polished']:
            case('auto-retouch', f'portrait retouch style: {style}',
                 lambda style=style: str(call('auto_portrait_retouch', projectId=project, photoId=bride_jpeg, style=style))[:160])
        case('auto-retouch', 'retouch strength sync', lambda: str(call('retouch_intensity', projectId=project, photoId=bride_jpeg, action='sync', factors={}))[:160])

    # ----------------------------------------------------------------- masks
    if want('masks'):
        case('masks', 'mask kinds', lambda: invoke('mask_kinds', {}))
        case('masks', 'ensure masks', lambda: f"{len(call('ensure_masks', projectId=project, imageId=bride_jpeg))} masks")
        for what in ['subject', 'background', 'sky', 'face_skin', 'body_skin', 'hair', 'clothes', 'eyes', 'iris',
                     'sclera', 'eyebrows', 'lips']:
            def mask(what=what):
                r = invoke('create_local_mask', {'input': {'projectId': project, 'photoId': bride_jpeg, 'what': what,
                                                           'source': None, 'into': None, 'mode': None, 'invert': False,
                                                           'bounds': None}})
                masks = r.get('masks', [])
                if not masks:
                    raise AssertionError('no mask was created')
                cov = invoke('local_mask_coverage', {'input': {'projectId': project, 'photoId': bride_jpeg,
                                                               'maskId': masks[-1]['id']}})
                plane = np.frombuffer(base64.b64decode(cov['rgbBase64']), dtype=np.uint8)
                share = float((plane > 127).mean())
                if share <= 0:
                    raise Warn(f'{what} selected nothing on this photograph')
                return f'{len(masks)} mask(s); {share:.1%} of the photo selected'
            case('masks', f'AI selection: {what}', mask)

    # ----------------------------------------------------------------- looks
    if want('looks'):
        def paired():
            r = call('match_paired_reference', photoId=bride, referencePath=str(OUT / 'paired/bride-finished.png'),
                     cancelId=str(uuid.uuid4()), keepAs='Test paired look')
            if r['afterError'] > r['beforeError']:
                raise AssertionError(f"worse: {r['beforeError']} -> {r['afterError']}")
            return f"error {r['beforeError']:.2f} -> {r['afterError']:.2f}"
        case('looks', 'match a finished version of the same photo', paired)

        def reference_style():
            r = call('analyse_reference_style', address='', folder=str(OUT / 'look-references'),
                     cancelId=str(uuid.uuid4()), export=False)
            rid = r.get('id') or r.get('referenceId') or r.get('reference', {}).get('id')
            if not rid:
                return f'analysed: {str(r)[:160]}'
            before = proxy(bride_jpeg)
            call('apply_reference_style', photoId=bride_jpeg, referenceId=rid, strength=1.0, profileId=None, profileStrength=None)
            if np.array_equal(before, proxy(bride_jpeg)):
                raise Warn('applied, no visible change')
            return f'analysed {rid[:10]} and applied'
        case('looks', 'reference look from a folder of photos', reference_style)

        def measure():
            r = call('measure_look', projectId=project, address='', source='folder', folder=str(OUT / 'look-references'),
                     name='Test look', baselineFrames=None, cancelId=str(uuid.uuid4()))
            return str(r)[:200]
        case('looks', 'match a look (lighting buckets)', measure)
        case('looks', 'list looks', lambda: f"{len(invoke('list_looks', {}))} look(s)")

    # --------------------------------------------------------- analysis passes
    if want('analysis'):
        passes = [
            ('estimate_tone', {'projectId': project}), ('estimate_colour', {'projectId': project}),
            ('analyse_integrity', {'projectId': project}), ('analyse_composition', {'projectId': project}),
            ('embed_project', {'projectId': project}), ('classify_scenes', {'projectId': project}),
            ('scan_faces', {'projectId': project}), ('group_people', {'projectId': project}),
            ('group_moments', {'projectId': project}), ('score_emotion', {'projectId': project}),
            ('cull_project', {'projectId': project}), ('curate_project', {'projectId': project, 'albumSize': None}),
            ('sculpt_local', {'projectId': project}), ('retouch_pass', {'projectId': project, 'preset': 'natural'}),
            ('micro_pass', {'projectId': project}), ('restore_pass', {'projectId': project}),
            ('plan_geometry', {'projectId': project}), ('cleanup_pass', {'projectId': project}),
            ('gallery_pass', {'projectId': project}), ('camera_pass', {'projectId': project}),
            ('qc_run', {'projectId': project, 'remediate': False}),
        ]
        for cmd, payload in passes:
            case('analysis', cmd, lambda cmd=cmd, payload=payload: str(invoke(cmd, {'input': payload}))[:220])
        case('analysis', 'descriptors after analysis',
             lambda: str(invoke('image_descriptors', {'projectId': project, 'photoId': bride}))[:160])
        case('analysis', 'build similarity index', lambda: str(invoke('build_index', {'projectId': project}))[:200])
        case('analysis', 'find similar', lambda: str(call('find_similar', projectId=project, photoId=bride, k=4,
                                                          timeWindowS=None, cameraId=None, exclude=[]))[:200])
        case('analysis', 'story outline', lambda: str(invoke('segment_story', {'projectId': project}))[:200])
        for status in ['tone_status', 'colour_status', 'integrity_status', 'composition_status', 'people_status',
                       'moment_status', 'emotion_status', 'cull_status', 'curate_status', 'gallery_status',
                       'camera_status', 'qc_status', 'restore_status', 'geometry_status', 'cleanup_status',
                       'micro_status', 'retouch_status', 'local_status', 'story_status', 'index_status',
                       'develop_status', 'look_status', 'style_status', 'learn_status']:
            def status_case(status=status):
                try:
                    return str(invoke(status, {'projectId': project}))[:200]
                except Exception:  # noqa: BLE001 - a few take their project inside `input`
                    return str(invoke(status, {'input': {'projectId': project}}))[:200]
            case('analysis', status, status_case)

    # ---------------------------------------------------------------- export
    if want('export'):
        eligible = [bride_jpeg, bride] + raws[:1]
        for fmt, depth in [('jpeg', 8), ('png', 8), ('tiff', 16)]:
            def export_case(fmt=fmt, depth=depth):
                dest = OUT / f'export-{fmt}'
                r = call('export_run', projectId=project, destination=str(dest), destinationKind='folder',
                         copyright=None, contact=None, creator=None, keywords=[], stripGps=True, stripCameraSerial=True,
                         verify=True, sets=[dict(name='test', imageIds=eligible, format=fmt, quality=92, colour='srgb',
                                                 bitDepth=depth, resize='full', sharpen='none', naming='{original}',
                                                 sidecar=False)])
                files = [p for p in dest.rglob('*') if p.is_file() and p.suffix.lower() in ('.jpg', '.jpeg', '.png', '.tif', '.tiff')]
                for f in files:
                    with Image.open(f) as im:
                        im.load()
                if len(files) < len(eligible):
                    raise AssertionError(f'{len(files)} of {len(eligible)} files written; {str(r)[:200]}')
                return f'{len(files)} files written and read back'
            case('export', f'export {fmt} {depth}-bit', export_case)

        def watermark():
            mark = {'width': 2, 'height': 2, 'rgba': [255, 255, 255, 255] * 4, 'opacity': .5, 'widthFraction': .1,
                    'marginFraction': .02, 'anchor': 'bottom_right'}
            dest = OUT / 'export-watermark'
            invoke('export_run_watermarked', {'input': dict(projectId=project, destination=str(dest), destinationKind='folder',
                   copyright='Test', contact=None, creator=None, keywords=[], stripGps=True, stripCameraSerial=True, verify=True,
                   sets=[dict(name='wm', imageIds=[bride_jpeg], format='jpeg', quality=92, colour='srgb', bitDepth=8,
                              resize='full', sharpen='none', naming='{original}', sidecar=False)]), 'watermark': mark})
            files = [p for p in dest.rglob('*.jpg')]
            if not files:
                raise AssertionError('no watermarked file')
            return f'{len(files)} watermarked file(s)'
        case('export', 'watermarked export', watermark)
        case('export', 'delivery manifest', lambda: str(invoke('export_manifest', {'projectId': project}))[:200])
        case('export', 'delivery status', lambda: str(invoke('delivery_status', {'projectId': project}))[:200])
        case('export', 'export presets', lambda: f"{len(invoke('export_presets', {}))} presets")

    # ------------------------------------------------------- one-click run
    if want('one-click'):
        def finish():
            dest = OUT / 'finish-folder'
            job = call('one_click_finish', projectId=project, destination=str(dest), ingestJobId=None, keepEverything=True)['jobId']
            deadline = time.monotonic() + 3600
            status = None
            while time.monotonic() < deadline:
                status = invoke('one_click_status', {'jobId': job})
                if status['status'] not in ('running', 'cancelling'):
                    break
                time.sleep(3)
            files = [p for p in dest.rglob('*') if p.is_file() and p.suffix.lower() in ('.jpg', '.jpeg')]
            if status['status'] not in ('completed', 'completed_with_issues') or not files:
                raise AssertionError(f"{status['status']}: {str(status)[:300]}")
            return (f"{status['status']}: analysed {status.get('analyzed')}, edited {status.get('aiEdited')}+{status.get('localEdited')}, "
                    f"written {status.get('written')}, verified {status.get('verified')}, {len(files)} files")
        case('one-click', 'finish a whole folder (edit, retouch, cull, export)', finish)

    if want('autopilot'):
        case('autopilot', 'pre-flight', lambda: str(invoke('autopilot_preflight', {'projectId': project}))[:200])

        def autopilot():
            invoke('autopilot_start', {'input': {'projectId': project, 'disabled': [], 'zeroTouch': False,
                                                 'allowOnBattery': True, 'quietMode': True}})
            deadline = time.monotonic() + 3600
            while time.monotonic() < deadline:
                p = invoke('autopilot_progress', {'projectId': project})
                if not p or p.get('status') != 'running':
                    break
                time.sleep(3)
            s = invoke('autopilot_summary', {'projectId': project})
            if not s:
                raise AssertionError('no run summary')
            detail = (f"{s['status']}: selected {s['selected']}, exported {s['exported']}, {s['totalMs']} ms; "
                      f"degraded: {s.get('degradedStages')}")
            if s['status'] not in ('completed', 'completed_degraded'):
                raise AssertionError(detail)
            if s['selected'] == 0:
                raise Warn(detail)
            return detail
        case('autopilot', 'run Autopilot on the wedding', autopilot)

    # ------------------------------------------------------- system settings
    if want('system'):
        for cmd, payload in [('hardware_plan', {}), ('render_caps', {}), ('licence_status', {}), ('list_ai_providers', {}),
                             ('ai_setup_status', {}), ('internal_ai_status', {'refresh': False}), ('cloud_status', {}),
                             ('list_models', {}), ('infer_stats', {}), ('diagnostics_report', {}), ('preview_stats', {'projectId': project}),
                             ('list_problems', {'projectId': project}), ('activity_status', {}),
                             ('cloud_spend', {'projectId': project})]:
            case('system', cmd, lambda cmd=cmd, payload=payload: str(invoke(cmd, payload))[:200])

    # --------------------------------------------------------- the whole UI
    if want('ui'):
        page.reload()
        page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
        page.get_by_role('button', name=f'{name} {len(by_name)}', exact=True).click()
        tabs = [('Start', 'Look, reference, photos'), ('Photos', 'Browse your collection'),
                ('Auto edit', 'One click, start to finish'), ('Match a look', 'Your reference, your photos'),
                ('Export', 'Ready to share'), ('Advanced', 'Quality, curation & settings')]
        for title, desc in tabs:
            def tab(title=title, desc=desc):
                errors = len(report['pageErrors'])
                page.get_by_role('button', name=f'{title} {desc}', exact=True).click()
                time.sleep(2)
                shot(f'ui-{title.replace(" ", "-").lower()}')
                if len(report['pageErrors']) > errors:
                    raise AssertionError(report['pageErrors'][-1])
                alerts = [t for t in page.get_by_role('alert').all_inner_texts() if t.strip()]
                if alerts:
                    raise Warn('the page shows: ' + ' | '.join(alerts)[:300])
                return 'opens without errors'
            case('ui', f'workspace: {title}', tab)
        page.get_by_role('button', name='Advanced Quality, curation & settings', exact=True).click()
        time.sleep(1)
        visible = page.locator('details > summary:visible')
        summaries = [t.strip() for t in visible.all_inner_texts() if t.strip()]
        report['advancedSections'] = summaries
        for text in summaries:
            def section(text=text):
                errors = len(report['pageErrors'])
                page.locator('details > summary:visible', has_text=text).first.click(timeout=30000)
                time.sleep(2.5)
                shot('ui-advanced-' + re.sub(r'[^a-z0-9]+', '-', text.lower())[:40])
                if len(report['pageErrors']) > errors:
                    raise AssertionError(report['pageErrors'][-1])
                alerts = [t for t in page.get_by_role('alert').all_inner_texts() if t.strip()]
                if alerts:
                    raise Warn('the page shows: ' + ' | '.join(alerts)[:300])
                return 'opens without errors'
            case('ui', f'Advanced: {text.strip()[:50]}', section)

    def originals():
        changed = [n for n, h in report['inputHashes'].items()
                   if hashlib.sha256((inputs / n).read_bytes()).hexdigest() != h]
        if changed:
            raise AssertionError(f'originals modified: {changed}')
        return f'all {len(report["inputHashes"])} originals byte-identical'
    case('safety', 'originals never modified', originals)
    case('safety', 'no page errors', lambda: 'none' if not report['pageErrors'] else (_ for _ in ()).throw(AssertionError(report['pageErrors'][:3])))


def write_report():
    rows = report['cases']
    counts = {s: sum(r['status'] == s for r in rows) for s in ('pass', 'warn', 'fail', 'skip')}
    lines = [f"# AURA full test - {report['started']}", '',
             f"{len(rows)} checks: **{counts['pass']} pass**, {counts['warn']} warn, **{counts['fail']} fail**, {counts['skip']} skip.", '']
    groups = []
    for r in rows:
        if r['group'] not in groups:
            groups.append(r['group'])
    for g in groups:
        sub = [r for r in rows if r['group'] == g]
        lines += [f"## {g} ({sum(r['status'] == 'pass' for r in sub)}/{len(sub)} pass)", '',
                  '| Check | Result | Seconds | Detail |', '|---|---|---|---|']
        for r in sub:
            detail = str(r['detail']).replace('|', '/').replace('\n', ' ')[:220]
            lines.append(f"| {r['name']} | {r['status'].upper()} | {r['seconds']} | {detail} |")
        lines.append('')
    (OUT / 'report.md').write_text('\n'.join(lines), encoding='utf-8')
    print(f"\n{len(rows)} checks: {counts}")


if __name__ == '__main__':
    main()
