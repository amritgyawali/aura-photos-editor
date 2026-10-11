"""Compare saved recipes and native RGB before and after a graceful app restart."""
import argparse
import base64
import hashlib
import json
import time
from pathlib import Path

from playwright.sync_api import sync_playwright

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('output', type=Path)
parser.add_argument('--port', type=int, default=9359)
parser.add_argument('--verify', action='store_true')
parser.add_argument('--collections', nargs='+', default=['Professional retouch verification',
                    'Second portrait verification', 'Body-only verification'])
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
with sync_playwright() as p:
    deadline = time.monotonic() + 30
    while True:
        try:
            browser = p.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}')
            page = browser.contexts[0].pages[0]
            page.wait_for_function('Boolean(window.__TAURI_INTERNALS__?.invoke)')
            break
        except Exception:
            if time.monotonic() >= deadline:
                raise
            time.sleep(.25)
    results = {}
    for name in args.collections:
        rows = page.evaluate('''async name => {
          const i=window.__TAURI_INTERNALS__.invoke;
          const project=(await i('list_projects')).find(p=>p.name===name);
          if(!project) throw new Error('Missing test collection: '+name);
          const photos=[];
          for(let offset=0;;offset+=240){
            const batch=await i('list_images',{input:{projectId:project.id,offset,limit:240,orderBy:null}});
            photos.push(...batch); if(batch.length<240) break;
          }
          const rows=[];
          for(const photo of photos){
            const recipe=await i('image_recipe',{input:{photoId:photo.id}});
            const frame=await i('native_retouch_preview',{projectId:project.id,photoId:photo.id,before:false});
            rows.push({fileName:photo.fileName,recipeHash:recipe.recipeHash,frame});
          }
          return rows;
        }''', name)
        assert rows, 'Empty verification collection'
        for data in rows:
            filename = data.pop('fileName')
            key = name if len(rows) == 1 else name + '/' + filename
            assert key not in results, 'Duplicate verification image name'
            frame = data.pop('frame')
            data.update(rgb_sha256=hashlib.sha256(base64.b64decode(frame['rgbBase64'])).hexdigest(),
                        dimensions=[frame['width'], frame['height']])
            results[key] = data
    baseline = args.output/'before-restart.json'
    if args.verify:
        assert results == json.loads(baseline.read_text()), 'Recipe or rendered pixels changed across restart'
        (args.output/'verification.json').write_text(json.dumps({'passed': True, 'collections': results}, indent=2), encoding='utf-8')
    else:
        baseline.write_text(json.dumps(results, indent=2), encoding='utf-8')
    print(json.dumps(results), flush=True)
