"""Verify automatic face detection and retouch end to end on the standalone desktop app.

For every photo: import, run Auto enhance exactly as the Develop button does, and record
detected faces, the retouch operations created, and how much the skin inside each face box
actually changed. Then paint a manual retouch operation and press Auto again, which must
still retouch the face (replacing only automatic operations) and keep the manual one.
Writes .work-checks/verify-face/results.json and a face crop sheet per photo.
"""
import base64
import json
import sys
import time
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]
INPUTS = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else ROOT / '.work-checks/verify-inputs'
OUT = ROOT / '.work-checks/verify-face'
OUT.mkdir(parents=True, exist_ok=True)
results = {'photos': []}

with sync_playwright() as pw:
    page = pw.chromium.connect_over_cdp('http://127.0.0.1:9223').contexts[0].pages[0]
    page.set_default_timeout(240000)

    def call(cmd, **args):
        return page.evaluate('''async ([c,a])=>{try{return await window.__TAURI_INTERNALS__.invoke(c,{input:a})}catch(e){throw Error(JSON.stringify(e))}}''', [cmd, args])

    def render(photo, before=False):
        if before:
            r = call('native_retouch_preview_input', projectId=project, photoId=photo) if False else None
        r = call('render_image', photoId=photo, level='screen', screen=[1200, 1200], purpose='interactive')
        return Image.frombytes('RGB', (r['width'], r['height']), base64.b64decode(r['rgbBase64']))

    name = 'Face retouch verify ' + time.strftime('%H-%M-%S')
    project = call('create_project', name=name, coupleNames=None, eventDate=None)['id']
    files = sorted(p for p in INPUTS.iterdir() if p.is_file())
    call('start_ingest', projectId=project, roots=[str(INPUTS)])
    photos, last, deadline = [], -1, time.monotonic() + 300
    while time.monotonic() < deadline:
        photos = call('list_images', projectId=project, offset=0, limit=100, orderBy='timeline')
        if len(photos) == last and photos:
            break
        last = len(photos)
        time.sleep(4)
    for row in photos:
        photo, file_name = row['id'], row['fileName']
        rec = {'file': file_name}
        results['photos'].append(rec)
        try:
            before = render(photo)
            started = time.monotonic()
            body = json.loads(call('enhance_photo', photoId=photo)['body'])
            rec['seconds'] = round(time.monotonic() - started, 1)
            rep = body.get('studio_portrait_auto_v1', {})
            rec['status'] = rep.get('status')
            rec['faces'] = rep.get('detectedFaces')
            rec['retouchedFaces'] = rep.get('retouchedFaces')
            ops = body.get('studio_retouch_v1', [])
            rec['skinOps'] = [(o['id'].split('-v1-')[-1], round(o['amount'], 2)) for o in ops if o['tool'] in ('skin_smooth', 'skin_uniformity', 'portrait_dodge_burn')]
            rec['otherOps'] = sorted({o['tool'] for o in ops if o['tool'] not in ('skin_smooth', 'skin_uniformity', 'portrait_dodge_burn')})
            # How much the skin inside each face box changed, after removing the global edit:
            # compare against a render with the retouch stack disabled.
            after = render(photo)
            disabled = [dict(o, enabled=False) for o in ops]
            if disabled:
                call('native_retouch_edit', projectId=project, photoId=photo, action='clear', edits=[], id=None)
                plain = render(photo)
                call('history_step', projectId=project, photoId=photo, action='undo')
            else:
                plain = after
            a = np.asarray(after, dtype=np.float32)
            p = np.asarray(plain.resize(after.size), dtype=np.float32)
            changes = []
            sheet_parts = []
            for f in rep.get('faces', []):
                l, t, r, b = f['bounds']
                W, H = after.size
                box = (int(l * W), int(t * H), int(r * W), int(b * H))
                diff = np.abs(a[box[1]:box[3], box[0]:box[2]] - p[box[1]:box[3], box[0]:box[2]])
                changes.append(round(float(diff.mean()), 2))
                pad = int((box[2] - box[0]) * 0.15)
                crop = (max(0, box[0] - pad), max(0, box[1] - pad), min(W, box[2] + pad), min(H, box[3] + pad))
                sheet_parts.append((plain.resize(after.size).crop(crop), after.crop(crop)))
            rec['faceMeanChange'] = changes
            if sheet_parts:
                w = max(x.width for x, _ in sheet_parts[:3]) * 2 + 10
                hgt = sum(x.height for x, _ in sheet_parts[:3]) + 10 * len(sheet_parts[:3])
                sheet = Image.new('RGB', (w, hgt), 'white')
                y = 0
                for x, z in sheet_parts[:3]:
                    sheet.paste(x, (0, y)); sheet.paste(z, (x.width + 10, y)); y += x.height + 10
                sheet.save(OUT / (Path(file_name).stem + '-faces.jpg'), quality=90)
            # Manual retouch first, then Auto again: must still retouch, keeping the manual op.
            if rec['retouchedFaces']:
                manual = {'id': 'm', 'tool': 'dodge', 'enabled': True, 'region': [0.5, 0.9, 0.05, 0.05], 'source': None,
                          'amount': 0.2, 'feather': 0.5, 'radius': 0.002, 'texture': 1.0, 'tone': 0.5, 'warmth': 0.0, 'tint': 0.0}
                call('native_retouch_edit', projectId=project, photoId=photo, action='append', edits=[manual], id=None)
                call('native_retouch_edit', projectId=project, photoId=photo, action='clear', edits=[], id=None)
                call('native_retouch_edit', projectId=project, photoId=photo, action='append', edits=[manual], id=None)
                again = json.loads(call('enhance_portrait', photoId=photo)['body'])
                ops2 = again.get('studio_retouch_v1', [])
                rec['afterManual'] = {
                    'status': again['studio_portrait_auto_v1'].get('status'),
                    'autoOps': sum(o['id'].startswith('auto-') for o in ops2),
                    'manualKept': sum(not o['id'].startswith('auto-') for o in ops2),
                }
            print(file_name, rec.get('faces'), 'faces', rec.get('retouchedFaces'), 'retouched', rec.get('faceMeanChange'), rec.get('afterManual'), f"{rec.get('seconds')}s", flush=True)
        except Exception as error:
            rec['error'] = str(error)[:300]
            print(file_name, 'ERROR', rec['error'][:200], flush=True)
        (OUT / 'results.json').write_text(json.dumps(results, indent=2), encoding='utf-8')
print('done')
