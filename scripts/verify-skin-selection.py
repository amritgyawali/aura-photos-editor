"""Show exactly which pixels automatic face and body retouch select, on real photos.

For each photo: run Auto retouch with Face + body skin, then ask the app for the selection
mask of every automatic skin operation (the same mask the Retouch "Preview selection mask"
shows, now including skin likeness and connectivity). Writes a red overlay per photo and
the share of each face operation's selection that falls outside the face box (background
leakage) to .work-checks/skin-selection/.
"""
import base64
import json
import sys
import time
from pathlib import Path

import numpy as np
from PIL import Image
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.work-checks/skin-selection'
OUT.mkdir(parents=True, exist_ok=True)
files = [Path(p).resolve() for p in sys.argv[1:]]
results = {}
with sync_playwright() as pw:
    page = pw.chromium.connect_over_cdp('http://127.0.0.1:9223').contexts[0].pages[0]
    page.set_default_timeout(240000)
    call = lambda c, **a: page.evaluate('async ([c,a])=>await window.__TAURI_INTERNALS__.invoke(c,{input:a})', [c, a])
    project = call('create_project', name='Skin selection ' + time.strftime('%H-%M-%S'), coupleNames=None, eventDate=None)['id']
    call('start_ingest', projectId=project, roots=[str(f) for f in files])
    photos = []
    while len(photos) < len(files):
        photos = call('list_images', projectId=project, offset=0, limit=50, orderBy='timeline'); time.sleep(0.5)
    for row in photos:
        photo, name = row['id'], row['fileName']
        options = {'intensity': 1, 'blemishes': True, 'eyes': True, 'teeth': True, 'refine': True, 'scope': 'face_and_body'}
        body = json.loads(call('auto_retouch', projectId=project, photoId=photo, global_=False, options=options)['body']) if False else \
            json.loads(page.evaluate('async ([p,ph,o])=>await window.__TAURI_INTERNALS__.invoke("auto_retouch",{input:{projectId:p,photoId:ph,global:false,options:o}})', [project, photo, options])['body'])
        faces = body['studio_portrait_auto_v1'].get('faces', [])
        ops = [o for o in body.get('studio_retouch_v1', []) if o['tool'] == 'skin_smooth']
        r = call('render_image', photoId=photo, level='screen', screen=[1600, 1200], purpose='interactive')
        base = np.asarray(Image.frombytes('RGB', (r['width'], r['height']), base64.b64decode(r['rgbBase64'])), dtype=np.float32)
        overlay = base.copy()
        stats = []
        for op in ops:
            sel = page.evaluate('async ([p,ph,e])=>await window.__TAURI_INTERNALS__.invoke("native_retouch_selection_preview",{input:{projectId:p,photoId:ph,edit:e,replaceId:e.id}})', [project, photo, op])
            m = np.asarray(Image.frombytes('RGB', (sel['width'], sel['height']), base64.b64decode(sel['rgbBase64'])).convert('L').resize((base.shape[1], base.shape[0])), dtype=np.float32) / 255
            colour = np.array([255, 40, 40]) if '-body-' not in op['id'] else np.array([40, 120, 255])
            overlay = overlay * (1 - 0.55 * m[..., None]) + colour * 0.55 * m[..., None]
            H, W = m.shape
            index = int(op['id'].split('-v1-')[1].split('-')[0])
            entry = {'op': op['id'], 'selectedPixels': int((m > 0.5).sum())}
            if index < len(faces):
                l, t, rr, b = faces[index]['bounds']
                fw, fh = rr - l, b - t
                if '-body-' in op['id']:
                    # Body skin must sit below the chin and within a few face widths.
                    yy, xx = np.mgrid[0:H, 0:W]
                    inside = (yy / H >= t) & (np.abs(xx / W - (l + rr) / 2) <= fw * 2.8)
                    entry['outsideBodyArea'] = round(float((m * ~inside).sum() / max(m.sum(), 1e-6)), 4)
                else:
                    box = np.zeros_like(m, dtype=bool)
                    box[int(max(0, t - fh * 0.15) * H):int(min(1, b + fh * 0.15) * H), int(max(0, l - fw * 0.15) * W):int(min(1, rr + fw * 0.15) * W)] = True
                    entry['outsideFace'] = round(float((m * ~box).sum() / max(m.sum(), 1e-6)), 4)
            stats.append(entry)
        Image.fromarray(np.clip(overlay, 0, 255).astype(np.uint8)).save(OUT / (Path(name).stem + '-selection.jpg'), quality=88)
        results[name] = stats
        print(name, stats, flush=True)
(OUT / 'results.json').write_text(json.dumps(results, indent=2), encoding='utf-8')
