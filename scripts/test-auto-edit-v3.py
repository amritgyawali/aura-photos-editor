"""Qualify one-click automatic editing on the real desktop app (ADR-0081).

Requires the rebuilt desktop running with WebView2 CDP on port 9223, Pillow and
Playwright, and the photo set from scripts/prepare-auto-edit-fixtures.py.

For every photograph: render before, run Auto enhance, render after, and record the
scene analysis, every saved step, per-face findings and timing. Then verify, per photo:
repeat-pass idempotence, stepping back through each automatic step with the new
history jump, exact pixel restoration, manual-edit protection after going back, and
original-file hashes. Writes .work-checks/auto-v3/results.json plus before/after sheets.
"""
import base64
import hashlib
import json
import sys
import time
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.work-checks/auto-v3'
INPUTS = OUT / 'inputs'
SHEETS = OUT / 'sheets'
SHEETS.mkdir(parents=True, exist_ok=True)
only = set(sys.argv[1:])
hashes = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in INPUTS.iterdir() if p.is_file()}
results = {'photos': [], 'errors': [], 'startedAt': time.strftime('%Y-%m-%d %H:%M:%S')}


def persist():
    (OUT / 'results.json').write_text(json.dumps(results, indent=2), encoding='utf-8')


with sync_playwright() as pw:
    page = pw.chromium.connect_over_cdp('http://127.0.0.1:9223').contexts[0].pages[0]
    page.set_default_timeout(180000)

    def call(cmd, **args):
        return page.evaluate('''async ([c,a])=>{try{return await window.__TAURI_INTERNALS__.invoke(c,{input:a})}catch(e){throw Error(JSON.stringify(e))}}''', [cmd, args])

    def render(photo):
        r = call('render_image', photoId=photo, level='screen', screen=[1400, 1000], purpose='interactive')
        return Image.frombytes('RGB', (r['width'], r['height']), base64.b64decode(r['rgbBase64']))

    def history(photo):
        return call('image_history', photoId=photo)

    def step(photo, action):
        return call('history_step', projectId=project, photoId=photo, action=action)

    def stack(photo):
        return call('native_retouch_edit', projectId=project, photoId=photo, action='list', edits=[], id=None)

    name = 'Auto edit v3 qualification ' + time.strftime('%H-%M-%S')
    project = call('create_project', name=name, coupleNames=None, eventDate=None)['id']
    call('start_ingest', projectId=project, roots=[str(INPUTS)])
    deadline = time.monotonic() + 300
    photos, last = [], -1
    while time.monotonic() < deadline:
        photos = call('list_images', projectId=project, offset=0, limit=200, orderBy='timeline')
        if len(photos) == last and len(photos) > 0:
            break
        last = len(photos)
        time.sleep(4)
    results.update(projectId=project, collection=name, imported=len(photos))
    persist()
    print('imported', len(photos), flush=True)
    for row in photos:
        photo, file_name = row['id'], row['fileName']
        if only and file_name not in only:
            continue
        record = {'file': file_name}
        results['photos'].append(record)
        try:
            before = render(photo)
            started = time.monotonic()
            dto = call('enhance_photo', photoId=photo)
            record['seconds'] = round(time.monotonic() - started, 2)
            body = json.loads(dto['body'])
            report = body.get('studio_portrait_auto_v1', {})
            record['report'] = {k: report.get(k) for k in ('status', 'detectedFaces', 'retouchedFaces', 'operations', 'message', 'steps', 'scene', 'plannerVersion')}
            record['faces'] = [{k: a.get(k) for k in ('face', 'status', 'confidence', 'reason', 'strengths', 'findings', 'spotsHealed', 'marksKept')} for a in report.get('assessments', [])]
            record['global'] = {k: body['global'].get(k) for k in ('exposure', 'contrast', 'temperature', 'tint', 'highlights', 'shadows', 'clarity', 'dehaze', 'vibrance', 'saturation')}
            record['global']['sharpen'] = body['global'].get('sharpen')
            record['global']['noise'] = body['global'].get('noise')
            ops = stack(photo)
            record['operations'] = [{'id': e['id'], 'tool': e['tool'], 'amount': round(e['amount'], 3)} for e in ops]
            after = render(photo)
            record['pixelsChanged'] = after.tobytes() != before.tobytes()
            hist = history(photo)
            auto_entries = [e for e in hist['entries'] if e['source'] != 'user']
            record['historySteps'] = [e['label'] for e in auto_entries]
            # Idempotence: a second pass saves nothing new.
            again = call('enhance_photo', photoId=photo)
            record['idempotent'] = again['recipeHash'] == dto['recipeHash'] and history(photo) == hist
            # Walk back through every automatic step, then return to the head.
            walk = []
            for entry in reversed(hist['entries'][:-1]):
                step(photo, f"goto:{entry['seq']}")
                walk.append({'to': entry['label'][:60], 'ops': len(stack(photo))})
            step(photo, 'goto:0')
            original = render(photo)
            record['gotoOriginalRestoresPixels'] = original.tobytes() == before.tobytes()
            step(photo, f"goto:{hist['entries'][-1]['seq']}")
            head = render(photo)
            record['gotoHeadRestoresPixels'] = head.tobytes() == after.tobytes()
            record['walk'] = walk
            # Go back to the first step, change something by hand, and confirm a repeat pass keeps it.
            if len(hist['entries']) > 1:
                step(photo, f"goto:{hist['entries'][0]['seq']}")
                call('set_param', projectId=project, photoId=photo, path='global.exposure', value=0.11, label='Manual exposure')
                call('enhance_photo', photoId=photo)
                exposure = json.loads(call('image_recipe', photoId=photo)['body'])['global']['exposure']
                record['manualKeptAfterRepeat'] = abs(exposure - 0.11) < 1e-4
            # Before / after sheet.
            w = 700
            b = before.copy(); b.thumbnail((w, w)); a = after.copy(); a.thumbnail((w, w))
            sheet = Image.new('RGB', (b.width + a.width + 10, max(b.height, a.height) + 30), 'white')
            sheet.paste(b, (0, 30)); sheet.paste(a, (b.width + 10, 30))
            d = ImageDraw.Draw(sheet)
            d.text((5, 8), f'BEFORE  {file_name}', fill='black'); d.text((b.width + 15, 8), 'AFTER (Auto enhance)', fill='black')
            sheet.save(SHEETS / (Path(file_name).stem + '.jpg'), quality=88)
            print(file_name, record['seconds'], 's', record['report']['status'], record['report']['detectedFaces'], 'faces', len(ops), 'ops', 'idem', record['idempotent'], flush=True)
        except Exception as error:  # record and continue with the next photograph
            record['error'] = str(error)[:500]
            print(file_name, 'ERROR', str(error)[:200], flush=True)
        persist()
    results['originalsUnchanged'] = all(hashlib.sha256((INPUTS / n).read_bytes()).hexdigest() == h for n, h in hashes.items())
    results['finishedAt'] = time.strftime('%Y-%m-%d %H:%M:%S')
    persist()
    print('done; originals unchanged:', results['originalsUnchanged'])
