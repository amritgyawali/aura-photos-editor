"""Exercise the next100 native lab routes and representative panel interactions in rebuilt AURA."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time
import urllib.request
import uuid

from PIL import Image
from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output', type=Path, default=ROOT / '.work-checks/aura-next100/desktop')
parser.add_argument('--port', type=int, default=9386)
parser.add_argument('--ledger', type=Path, default=ROOT / 'docs/reports/AURA-next-hundred-capabilities-2026-10-21.json')
parser.add_argument('--expected-tools', type=int, default=50)
parser.add_argument('--expected-diagnostics', type=int, default=45)
parser.add_argument('--photo', type=Path, help='Use an existing rendered real-photo proxy for the first source')
parser.add_argument('--native-only', action='store_true', help='Invoke native routes without repeating the representative panel checks')
arguments = parser.parse_args()
OUT = arguments.output.resolve()
if (OUT / 'results.json').exists() or (OUT / 'catalog/catalog.sqlite').exists():
    raise ValueError('Choose a fresh output folder for this executable qualification')
OUT.mkdir(parents=True, exist_ok=True)
(OUT / 'catalog').mkdir(exist_ok=True)
sources = []
for index in range(2):
    path = OUT / f'qualification-{index}.png'
    image = Image.new('RGB', (640, 96))
    image.putdata([((35+x*2+y%3*8+index*5)%256, (40+y*2+x%3*7)%256, (30+x+y)%256) for y in range(96) for x in range(640)])
    image.save(path)
    if index == 0 and arguments.photo:
        with Image.open(arguments.photo) as photographed:
            proxy = photographed.convert('RGB')
            proxy.thumbnail((640, 640))
            proxy.save(path)
    sources.append(path)
hashes = {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in sources}
ledger = json.loads(arguments.ledger.read_text(encoding='utf-8'))['capabilities']
port = arguments.port
exe = ROOT / 'target/desktop-launch/debug/aura-desktop.exe'
env = dict(os.environ, AURA_TEST_CATALOG=str(OUT / 'catalog/catalog.sqlite'), WEBVIEW2_USER_DATA_FOLDER=str(OUT / 'webview'), WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-port={port}')
startup = subprocess.STARTUPINFO(); startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW; startup.wShowWindow = 0
proc = subprocess.Popen([str(exe)], cwd=ROOT, env=env, startupinfo=startup)
report = dict(cases=[], pageErrors=[], commandCounts={}, executableSha256=hashlib.sha256(exe.read_bytes()).hexdigest(), realPhotoSource=str(arguments.photo.resolve()) if arguments.photo else None, realPhotoSha256=hashlib.sha256(arguments.photo.read_bytes()).hexdigest() if arguments.photo else None, nativeOnly=arguments.native_only, ledgerSha256=hashlib.sha256(arguments.ledger.read_bytes()).hexdigest())


def persist():
    (OUT / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')


def passed(name, **details):
    report['cases'].append(dict(name=name, passed=True, **details)); persist()
    print('PASS', name, flush=True)


try:
    for _ in range(120):
        try:
            urllib.request.urlopen(f'http://127.0.0.1:{port}/json/version', timeout=1); break
        except OSError: time.sleep(1)
    else: raise RuntimeError('Desktop did not start')
    with sync_playwright() as pw:
        page = pw.chromium.connect_over_cdp(f'http://127.0.0.1:{port}').contexts[0].pages[0]
        page.set_default_timeout(180000); expect.set_options(timeout=60000)
        page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
        page.on('pageerror', lambda error: report['pageErrors'].append(str(error)))

        def call(command, **values):
            report['commandCounts'][command] = report['commandCounts'].get(command, 0) + 1
            return page.evaluate('async ([command,input]) => {let timer;try{return await Promise.race([window.__TAURI_INTERNALS__.invoke(command,{input}),new Promise((_,reject)=>{timer=setTimeout(()=>reject(new Error("Native command timed out: "+command)),120000)})])}catch(error){throw new Error(JSON.stringify({message:error.message??String(error),...error}))}finally{clearTimeout(timer)}}', [command, values])

        name = 'AURA next100 desktop qualification ' + time.strftime('%H%M%S')
        project = call('create_project', name=name, coupleNames=None, eventDate=None)['id']
        call('start_ingest', projectId=project, roots=[str(p) for p in sources])
        for _ in range(120):
            photos = call('list_images', projectId=project, offset=0, limit=10, orderBy=None)
            if len(photos) == 2: break
            time.sleep(.5)
        assert len(photos) == 2
        assert {p['fileName'] for p in photos} == {p.name for p in sources}, 'Unexpected fixture import'
        photos = sorted(photos, key=lambda p: p['fileName'])
        assert photos[0]['fileName'] == sources[0].name
        report['selectedSourceFile'] = photos[0]['fileName']
        report['importedSourceHashes'] = hashes
        ids = [p['id'] for p in photos]; photo = ids[0]
        radial = lambda x, r: dict(mode='add', source=dict(type='radial', centre=[x,.5], radii=[r,.3], angle=0, feather=.2))
        masks = [dict(id='one', name='Lab source', enabled=True, amount=1, components=[radial(.25,.2),radial(.75,.15)], params=dict(exposure=.4)),
                 dict(id='two', name='Lab destination', enabled=True, amount=.8, components=[radial(.55,.3)], params=dict(exposure=-.3))]
        for image_id in ids:
            call('save_local_masks', projectId=project, photoId=image_id, masks=masks, label='Qualification masks')

        def recipe(): return call('image_recipe', photoId=photo)
        def render(): return call('render_image', photoId=photo, level='full', purpose='export')['rgbBase64']
        def history(action): return call('history_step', projectId=project, photoId=photo, action=action)

        baseline = recipe(); original = render()
        for item in [c for c in ledger if c['group'] in ('colour','spatial')]:
            started = time.perf_counter()
            operation = dict(id='qualification', maskId='one', tool=item['id'], enabled=True, amount=.7, radius=.04)
            if item['group'] == 'spatial':
                operation.update(threshold=.08, detail=.8, angle=30, source=[.2,.3], shift=[.02,-.01], path=[[-.04,0],[0,.03],[.04,0]])
                command = 'save_spatial_lab'
            else:
                operation.update(strength=.7, pivot=.18, width=.2, source=[.2,.3], second=[.8,.7], target=[.2,.3,.1])
                command = 'save_color_lab'
            operation.update(item.get('qualificationOperation', item.get('qualificationOverrides', {})))
            values = dict(projectId=project, photoId=photo, recipeHash=baseline['recipeHash'], operations=[operation])
            call(command, **values)
            edited = render()
            saved = recipe()
            before_history = call('image_history', photoId=photo)
            unchanged_values = dict(values, recipeHash=saved['recipeHash'])
            call(command, **unchanged_values)
            assert call('image_history', photoId=photo) == before_history, 'Same saved operation appended history'
            assert recipe()['recipeHash'] == saved['recipeHash']
            refused = False
            try: call(command, **values)
            except Exception: refused = True
            assert refused, 'Stale lab write was accepted'
            history('undo'); assert recipe()['recipeHash'] == baseline['recipeHash'] and render() == original
            history('redo'); assert render() == edited
            history('undo')
            passed(item['id'], group=item['group'], pixelsChanged=edited != original, operation=operation, originalPixelsSha256=hashlib.sha256(base64.b64decode(original)).hexdigest(), editedPixelsSha256=hashlib.sha256(base64.b64decode(edited)).hexdigest(), staleRejected=True, undoRedoExact=True, noOpHistoryExact=True, scenarioSeconds=round(time.perf_counter()-started,4))

        for item in [c for c in ledger if c['group'] == 'selection']:
            started = time.perf_counter()
            values = dict(projectId=project, photoId=photo, maskId='one', recipeHash=baseline['recipeHash'],
                          cancelId=str(uuid.uuid4()), action=item['id'], value=0, point=[.2,.5],
                          advanced=dict(vertices=[[.2,.2],[.8,.2],[.8,.8],[.2,.8]], bounds=[.1,.1,.9,.9], range=[0,1], tolerance=.2,
                                        radius=4, corner=.3, angle=0, sweep=120, seedIndex=0, minArea=1, maxArea=262144, compactness=0))
            if item['id'] in ('append_stack','replace_selection','sync_adjustments','align_centroid'): values['otherMaskId'] = 'two'
            overrides = item.get('qualificationOverrides', {})
            values['advanced'].update(overrides.get('advanced', {}))
            values.update({key: value for key, value in overrides.items() if key != 'advanced'})
            call('mask_workflow', **values); edited = render()
            changed = recipe()['recipeHash'] != baseline['recipeHash']
            if changed:
                history('undo'); assert recipe()['recipeHash'] == baseline['recipeHash'] and render() == original
                history('redo'); assert render() == edited
                history('undo')
            else:
                assert edited == original, 'Unchanged recipe changed rendered pixels'
            passed(item['id'], group='selection', pixelsChanged=edited != original, nativeRequest=values, originalPixelsSha256=hashlib.sha256(base64.b64decode(original)).hexdigest(), editedPixelsSha256=hashlib.sha256(base64.b64decode(edited)).hexdigest(), recipeChanged=changed, undoRedoExact=changed, scenarioSeconds=round(time.perf_counter()-started,4))

        before_history = call('image_history', photoId=photo)
        inspection_started = time.perf_counter()
        diagnostics = call('inspect_photo_diagnostics', projectId=project, photoId=photo, recipeHash=baseline['recipeHash'])
        report['inspectionBatchSeconds'] = round(time.perf_counter()-inspection_started,4)
        assert len(diagnostics['diagnostics']) == arguments.expected_diagnostics
        import math
        for item in [entry for entry in ledger if entry['group'] == 'inspection']:
            tool = next(value for value in diagnostics['diagnostics'] if value['id'] == item['id'])
            assert tool['title'] == item['name']
            assert len(tool['values']) == tool['width'] * tool['height']
            assert all(math.isfinite(value) for value in tool['values'])
            passed(item['id'], group='inspection', samples=tool['samples'], values=len(tool['values']), nativeRevisionBound=True)
        assert call('inspect_photo_diagnostics', projectId=project, photoId=photo, recipeHash=baseline['recipeHash']) == diagnostics
        assert call('image_history', photoId=photo) == before_history
        assert recipe()['recipeHash'] == baseline['recipeHash']
        stale = False
        try: call('inspect_photo_diagnostics', projectId=project, photoId=photo, recipeHash='stale')
        except Exception: stale = True
        assert stale
        (OUT/'diagnostics.json').write_text(json.dumps(diagnostics, indent=2), encoding='utf-8')
        passed('Inspection is deterministic, rejects stale revisions and never writes history')

        if not arguments.native_only:
            page.reload()
            page.get_by_role('button', name=f'{name} 2', exact=True).click()
            page.get_by_role('button', name='Auto edit One click, start to finish', exact=True).click()
            page.get_by_role('option', name=photos[0]['fileName'], exact=True).click()
            nav = page.get_by_role('navigation', name='Editing panels')
            nav.get_by_role('button', name='Spatial repair', exact=True).click()
            expect(page.get_by_role('region', name='Spatial repair lab')).to_be_visible()
            assert page.get_by_label('Spatial tool', exact=True).locator('option').count() == arguments.expected_tools
            page.get_by_label('Spatial tool', exact=True).select_option('haar_detail')
            page.get_by_role('button', name='Add spatial operation', exact=True).click()
            page.get_by_label('Spatial detail gain', exact=True).fill('1.2')
            nav.get_by_role('button', name='Colour lab', exact=True).click()
            expect(page.get_by_role('region', name='Selected colour and tone lab')).to_be_visible()
            assert page.get_by_label('Colour method', exact=True).locator('option').count() == arguments.expected_tools
            page.get_by_label('Colour method', exact=True).select_option('opponent_contrast')
            page.get_by_role('button', name='Add colour method', exact=True).click()
            page.screenshot(path=str(OUT/'colour.png'), full_page=True)
            nav.get_by_role('button', name='Spatial repair', exact=True).click()
            expect(page.get_by_label('Spatial detail gain', exact=True)).to_have_value('1.2')
            page.get_by_role('button', name='Apply spatial tools', exact=True).click()
            page.wait_for_function('async id => JSON.parse((await window.__TAURI_INTERNALS__.invoke("image_recipe",{input:{photoId:id}})).body).spatial_lab_v1?.length===1', arg=photo)
            expect(page.get_by_role('button', name='Apply spatial tools', exact=True)).to_be_enabled()
            page.screenshot(path=str(OUT/'spatial.png'), full_page=True)
            passed('Spatial panel saves native operation and retains draft across tabs')
            nav.get_by_role('button', name='Colour lab', exact=True).click()
            page.get_by_label('Colour method', exact=True).select_option('opponent_contrast')
            page.get_by_role('button', name='Add colour method', exact=True).click()
            page.get_by_role('button', name='Apply colour methods', exact=True).click()
            page.wait_for_function('async id => JSON.parse((await window.__TAURI_INTERNALS__.invoke("image_recipe",{input:{photoId:id}})).body).color_lab_v1?.length===1', arg=photo)
            expect(page.get_by_role('button', name='Apply colour methods', exact=True)).to_be_enabled()
            page.screenshot(path=str(OUT/'colour.png'), full_page=True)
            passed('Colour panel saves native selected-colour operation')
            nav.get_by_role('button', name='Review & checkpoints', exact=True).click()
            page.get_by_role('button', name='Load review workspace', exact=True).click()
            expect(page.get_by_role('button', name='Save ordered cohort as review queue', exact=True)).to_be_enabled()
            page.get_by_role('button', name='Save ordered cohort as review queue', exact=True).click()
            expect(page.get_by_role('button', name='Save ordered cohort as review queue', exact=True)).to_be_enabled()
            page.screenshot(path=str(OUT/'workflow.png'), full_page=True)
            passed('Workflow panel loads and persists review queue')
            nav.get_by_role('button', name='Masks', exact=True).click()
            page.get_by_role('button', name='Lab source', exact=True).click()
            page.get_by_text('Selection tools', exact=True).click()
            laboratory = page.get_by_text(f'Selection laboratory · {arguments.expected_tools} workflows', exact=True)
            expect(laboratory).to_be_visible()
            laboratory.click()
            before_mask = recipe()['recipeHash']
            page.get_by_role('button', name='Apply laboratory workflow', exact=True).click()
            page.wait_for_function('async ([id,prior]) => (await window.__TAURI_INTERNALS__.invoke("image_recipe",{input:{photoId:id}})).recipeHash!==prior', arg=[photo,before_mask])
            expect(page.get_by_role('button', name='Apply laboratory workflow', exact=True)).to_be_enabled()
            page.screenshot(path=str(OUT/'selection.png'), full_page=True)
            passed('Selection laboratory opens and saves a native selection workflow')
            nav.get_by_role('button', name='Palette & tone review', exact=True).click()
            page.get_by_role('button', name='Run photo inspection', exact=True).click()
            expect(page.get_by_role('status').filter(has_text=f'{arguments.expected_diagnostics} measurements')).to_be_visible()
            groups = [
                ('Palette & tone review', ['channel-information','palette-cube','hue-energy','chroma-light-joint','neutral-cast-trend','chroma-covariance','tonal-moments','level-gap-runs']),
                ('Detail review', ['laplacian-crossings','edge-effective-width','edge-overshoot','luma-autocorrelation','diagonal-phase','residual-correlation','haar-energy','edge-spread','border-edge-arrivals']),
                ('Composition review', ['light-centroid','detail-centroid','quadrant-light','border-light-energy','thirds-edge-mass','mirror-residual','bright-components','radial-light-profile']),
            ]
            review_groups = list(dict.fromkeys(item['reviewGroup'] for item in ledger if item.get('reviewGroup')))
            if review_groups:
                groups = [(name, [item['id'] for item in ledger if item.get('reviewGroup') == name]) for name in review_groups]
            for group, tool_ids in groups:
                nav.get_by_role('button', name=group, exact=True).click()
                expect(page.get_by_label('Inspection group', exact=True)).to_have_value(group)
                assert page.get_by_label('Measurement', exact=True).locator('option').count() == len(tool_ids)
                for tool_id in tool_ids:
                    page.get_by_label('Measurement', exact=True).select_option(tool_id)
                    item = next(entry for entry in ledger if entry['id'] == tool_id)
                    expect(page.get_by_role('heading', name=item['name'], exact=True)).to_be_visible()
                page.screenshot(path=str(OUT/(group.split()[0].lower()+'-review.png')), full_page=True)
                passed('Visible '+group+' screen exposes every native tool')
        observed = {case['name'] for case in report['cases'] if case['passed']}
        assert all(item['id'] in observed for item in ledger)
        report['qualifiedCapabilities'] = len(ledger)
        assert hashlib.sha256(arguments.ledger.read_bytes()).hexdigest() == report['ledgerSha256'], 'Ledger changed during scenario'
        report['originalsUnchanged'] = all(hashlib.sha256(Path(p).read_bytes()).hexdigest()==h for p,h in hashes.items())
        assert report['originalsUnchanged'] and not report['pageErrors']
        report['completed'] = True
except Exception as error:
    report['fatalError'] = str(error); raise
finally:
    persist(); proc.terminate(); proc.wait(timeout=30)
