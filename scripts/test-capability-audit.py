"""Audit the actual running desktop through IPC. GUI checks are a separate script.

Requires prepare-audit-fixtures.py, Pillow, Playwright and CDP port 9223.
Failures are retained per case so unsupported formats cannot hide later checks.
Pixel changes demonstrate execution, not professional perceptual quality.
"""
import base64
import hashlib
import json
import sys
import time
from pathlib import Path

from PIL import Image, ImageChops
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.work-checks/full-audit'
INPUT = OUT / 'inputs'
report = {'cases': [], 'photos': [], 'mode': 'actual desktop IPC, not mouse/keyboard'}
if '--resume' in sys.argv:
    report = json.loads((OUT/'runtime-results.json').read_text(encoding='utf-8'))


def persist():
    (OUT / 'runtime-results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')


def case(name, fn):
    previous = next((r for r in report['cases'] if r['name']==name), None)
    if previous:
        return previous
    start = time.monotonic()
    try:
        result = fn()
        row = dict(name=name, status='executed', result=result)
    except Exception as error:
        row = dict(name=name, status='error', error=str(error))
    row['seconds'] = round(time.monotonic() - start, 3)
    report['cases'].append(row)
    persist()
    print(name, row['status'], str(row.get('result', row.get('error')))[:240], flush=True)
    return row


with sync_playwright() as pw:
    browser = pw.chromium.connect_over_cdp('http://127.0.0.1:9223')
    page = browser.contexts[0].pages[0]
    page.set_default_timeout(60000)

    def invoke(cmd, args):
        return page.evaluate('''async ([cmd,args])=>{try{return await window.__TAURI_INTERNALS__.invoke(cmd,args)}
            catch(e){throw new Error(JSON.stringify(e))}}''', [cmd, args])

    def call(cmd, **args):
        return invoke(cmd, {'input': args})

    def render(photo):
        r = call('render_image', photoId=photo, level='full', purpose='export')
        return Image.frombytes('RGB', (r['width'], r['height']), base64.b64decode(r['rgbBase64']))

    def recipe(photo):
        return call('image_recipe', photoId=photo)

    name = report.get('collectionName') or 'Full capability audit ' + time.strftime('%Y-%m-%d %H-%M-%S')
    project = report.get('projectId') or call('create_project', name=name, coupleNames=None, eventDate=None)['id']
    report.update(projectId=project, collectionName=name)
    page.evaluate('''async()=>{window.__auditIngest=[];const handler=window.__TAURI_INTERNALS__.transformCallback(e=>window.__auditIngest.push(e.payload));
        await window.__TAURI_INTERNALS__.invoke('plugin:event|listen',{event:'ingest',target:{kind:'Any'},handler})}''')
    call('start_ingest', projectId=project, roots=[str(INPUT)])
    deadline = time.monotonic() + 120
    while not page.evaluate("()=>window.__auditIngest.some(e=>e.kind==='finished')"):
        if time.monotonic() > deadline:
            raise TimeoutError('Import did not finish in 120 seconds')
        time.sleep(.5)
    report.setdefault('ingestRuns', []).append(page.evaluate('window.__auditIngest'))
    photos = []
    while True:
        batch = call('list_images', projectId=project, offset=len(photos), limit=20, orderBy='timeline')
        if not batch:
            break
        photos.extend(batch)
    report['photos'] = photos
    report['notCatalogued'] = sorted(p.name for p in INPUT.iterdir() if p.name not in {r['fileName'] for r in photos})
    persist()

    for row in photos:
        def probe(row=row):
            before = render(row['id'])
            before.save(OUT / (row['fileName'] + '-before.png'))
            enhanced = call('enhance_photo', photoId=row['id'])
            after = render(row['id'])
            after.save(OUT / (row['fileName'] + '-after.png'))
            auto = json.loads(enhanced['body']).get('studio_portrait_auto_v1')
            return dict(dimensions=list(before.size), changed=before.tobytes() != after.tobytes(), auto=auto,
                        notes='Appearance is reviewed separately; detection count is not ground truth.')
        case('image:' + row['fileName'], probe)

    photo = next(p['id'] for p in photos if p['fileName'] == 'test-opaque.png')

    def reset():
        call('history_step', projectId=project, photoId=photo, action='reset_original')

    reset()
    baseline = render(photo)
    tools = ['heal','patch_heal','clone','auto_blemish','frequency','skin_smooth','skin_uniformity',
             'portrait_dodge_burn','micro_dodge_burn','dodge','burn','skin_color','color_match','mattify',
             'under_eye','wrinkle','teeth','eye_clean','eye_detail','red_eye','fabric','backdrop','glare','makeup']
    for tool in tools:
        def tool_case(tool=tool):
            reset()
            edit = dict(id='audit', tool=tool, enabled=True, region=[.46,.53,.07,.08], source=[.49,.5],
                        amount=.45, feather=.7, radius=.008, texture=.7, tone=.6, warmth=.25, tint=.1)
            saved = call('native_retouch_edit', projectId=project, photoId=photo, action='append', edits=[edit], id=None)
            after = render(photo)
            changed = ImageChops.difference(baseline, after).getbbox()
            call('history_step', projectId=project, photoId=photo, action='undo')
            assert render(photo).tobytes() == baseline.tobytes(), 'Undo did not restore pixels'
            call('history_step', projectId=project, photoId=photo, action='redo')
            assert render(photo).tobytes() == after.tobytes(), 'Redo did not restore pixels'
            return dict(saved=len(saved), changedPixels=changed is not None, bounds=changed, exactUndoRedo=True)
        case('tool:' + tool, tool_case)

    params = {'global.exposure':.5, 'global.temperature':6500, 'global.tint':20, 'global.contrast':25,
              'global.highlights':-40,'global.shadows':40,'global.whites':20,'global.blacks':-20,
              'global.texture':30,'global.clarity':30,'global.dehaze':30,'global.vibrance':35,
              'global.saturation':-30,'global.hsl.red.h':30,'global.hsl.orange.s':-40,
              'global.hsl.blue.l':30,'global.curve.points':[[0,0],[128,165],[255,255]],
              'global.channel_curves.red.points':[[0,0],[128,153],[255,255]],
              'global.parametric.lights':30,'global.sharpen.amount':70,'global.noise.luminance':50,
              'global.noise.colour':50,'global.effects.vignette.amount':-45,
              'global.effects.grain.amount':35,'global.calibration.red_hue':35,
              'global.colour_grade.shadows.saturation':40,'bw':{'mix':{},'grade':None},
              'geometry.rotate':10,'geometry.crop':[.1,.1,.8,.8],
              'lens.distortion':True,'lens.ca':True,'lens.vignette':50}
    for path, value in params.items():
        def param_case(path=path, value=value):
            reset()
            call('set_param', projectId=project, photoId=photo, path=path, value=value, label='Audit ' + path)
            after = render(photo)
            return dict(changedPixels=baseline.size != after.size or baseline.tobytes() != after.tobytes(),
                        dimensions=list(after.size), hash=recipe(photo)['recipeHash'])
        case('parameter:' + path, param_case)

    reset()
    eligible = [p['id'] for p in photos if p['fileName'] in ['landscape.jpg','portrait-1239291.jpg','test-opaque.png']]
    for fmt, depth in [('jpeg',8),('png',8),('tiff',16)]:
        def export_case(fmt=fmt, depth=depth):
            destination = OUT / ('export-' + fmt)
            result = call('export_run', projectId=project, destination=str(destination), destinationKind='folder',
                          copyright=None,contact=None,creator=None,keywords=[],stripGps=True,stripCameraSerial=True,verify=True,
                          sets=[dict(name='audit',imageIds=eligible,format=fmt,quality=95,colour='srgb',bitDepth=depth,
                                     resize='full',sharpen='none',naming='{original}',sidecar=True)])
            checks = []
            for item in invoke('export_files', {'projectId':project}):
                target = destination / item['path']
                if not target.exists():
                    continue
                with Image.open(target) as img:
                    check = dict(file=item['path'],format=img.format,size=list(img.size),mode=img.mode,
                                 bits=img.tag_v2.get(258) if img.format=='TIFF' else 8)
                    if fmt=='png':
                        check['exactRendererMatch'] = img.convert('RGB').tobytes()==render(item['imageId']).tobytes()
                    checks.append(check)
            return dict(export=result,files=checks)
        case('export:' + fmt, export_case)

    def invalid():
        before = recipe(photo)['recipeHash']
        try:
            call('set_param',projectId=project,photoId=photo,path='global.exposure',value=99999,label='Invalid')
            return {'rejected':False,'unchanged':recipe(photo)['recipeHash']==before}
        except Exception as error:
            return {'rejected':True,'unchanged':recipe(photo)['recipeHash']==before,'error':str(error)}
    case('invalid-parameter', invalid)
    manifest = json.loads((OUT/'input-manifest.json').read_text())
    report['originalsUnchanged'] = all(hashlib.sha256((INPUT/r['file']).read_bytes()).hexdigest()==r['sha256'] for r in manifest)
    report['completed'] = True
    persist()
