"""Exercise all new native lab routes and representative panel interactions in rebuilt AURA."""
import argparse
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
parser.add_argument('--output', type=Path, default=ROOT / '.work-checks/aura100/desktop')
parser.add_argument('--port', type=int, default=9386)
parser.add_argument('--selection-count', type=int, default=50)
arguments = parser.parse_args()
OUT = arguments.output.resolve()
if (OUT / 'results.json').exists() or (OUT / 'catalog/catalog.sqlite').exists():
    raise ValueError('Choose a fresh output folder for this executable qualification')
OUT.mkdir(parents=True, exist_ok=True)
(OUT / 'catalog').mkdir(exist_ok=True)
sources = []
for index in range(2):
    path = OUT / f'qualification-{index}.png'
    image = Image.new('RGB', (96, 72))
    image.putdata([((35+x*2+y%3*8+index*5)%256, (40+y*2+x%3*7)%256, (30+x+y)%256) for y in range(72) for x in range(96)])
    image.save(path)
    sources.append(path)
hashes = {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in sources}
ledger = json.loads((ROOT / 'docs/reports/AURA-hundred-capabilities-2026-10-21.json').read_text())['capabilities']
port = arguments.port
exe = ROOT / 'target/desktop-launch/debug/aura-desktop.exe'
env = dict(os.environ, AURA_TEST_CATALOG=str(OUT / 'catalog/catalog.sqlite'), WEBVIEW2_USER_DATA_FOLDER=str(OUT / 'webview'), WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-port={port}')
startup = subprocess.STARTUPINFO(); startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW; startup.wShowWindow = 0
proc = subprocess.Popen([str(exe)], cwd=ROOT, env=env, startupinfo=startup)
report = dict(cases=[], pageErrors=[], commandCounts={}, executableSha256=hashlib.sha256(exe.read_bytes()).hexdigest(), ledgerSha256=hashlib.sha256((ROOT/'docs/reports/AURA-hundred-capabilities-2026-10-21.json').read_bytes()).hexdigest())


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
            return page.evaluate('async ([command,input]) => {try{return await window.__TAURI_INTERNALS__.invoke(command,{input})}catch(error){throw new Error(JSON.stringify(error))}}', [command, values])

        name = 'AURA 100 desktop qualification ' + time.strftime('%H%M%S')
        project = call('create_project', name=name, coupleNames=None, eventDate=None)['id']
        call('start_ingest', projectId=project, roots=[str(p) for p in sources])
        for _ in range(120):
            photos = call('list_images', projectId=project, offset=0, limit=10, orderBy=None)
            if len(photos) == 2: break
            time.sleep(.5)
        assert len(photos) == 2
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
            operation = dict(id='qualification', maskId='one', tool=item['id'], enabled=True, amount=.7, radius=.04)
            if item['group'] == 'spatial':
                operation.update(threshold=.08, detail=.8, angle=30, source=[.2,.3], shift=[.02,-.01], path=[[-.04,0],[0,.03],[.04,0]])
                command = 'save_spatial_lab'
            else:
                operation.update(strength=.7, pivot=.18, width=.2, source=[.2,.3], second=[.8,.7], target=[.2,.3,.1])
                command = 'save_color_lab'
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
            passed(item['id'], group=item['group'], pixelsChanged=edited != original, staleRejected=True, undoRedoExact=True, noOpHistoryExact=True)

        for item in [c for c in ledger if c['group'] == 'selection']:
            values = dict(projectId=project, photoId=photo, maskId='one', recipeHash=baseline['recipeHash'],
                          cancelId=str(uuid.uuid4()), action=item['id'], value=0, point=[.2,.5],
                          advanced=dict(vertices=[[.2,.2],[.8,.2],[.8,.8],[.2,.8]], bounds=[.1,.1,.9,.9], range=[0,1], tolerance=.2,
                                        radius=4, corner=.3, angle=0, sweep=120, seedIndex=0, minArea=1, maxArea=262144, compactness=0))
            if item['id'] in ('append_stack','replace_selection','sync_adjustments','align_centroid'): values['otherMaskId'] = 'two'
            call('mask_workflow', **values); edited = render()
            assert recipe()['recipeHash'] != baseline['recipeHash']
            history('undo'); assert recipe()['recipeHash'] == baseline['recipeHash'] and render() == original
            history('redo'); assert render() == edited
            history('undo')
            passed(item['id'], group='selection', pixelsChanged=edited != original, undoRedoExact=True)

        snapshot = call('workflow_lab', projectId=project, action='read', photoIds=ids, reviewDate='2026-10-21')

        def workflow(action, **extra):
            global snapshot
            values = dict(projectId=project, action=action, photoIds=ids, expectedRevision=snapshot['revision'],
                          recipeHashes={image_id:call('image_recipe', photoId=image_id)['recipeHash'] for image_id in ids}, reviewDate='2026-10-21')
            values.update(extra)
            snapshot = call('workflow_lab', **values)
            passed('workflow:'+action)
            return snapshot

        workflow('checkpoint', name='Desktop baseline'); checkpoint = snapshot['checkpoints'][-1]['id']
        assert snapshot['checkpoints'][-1]['count'] == len(ids)
        call('set_param', projectId=project, photoId=photo, path='global.exposure', value=.6, label='Qualification source change')
        workflow('read')
        workflow('drift', checkpointId=checkpoint); assert any('tone' in r['changed'] for r in snapshot['rows'])
        workflow('delta_plan', checkpointId=checkpoint, sourcePhotoId=photo, sections=['tone'])
        assert any(r['changed'] for r in snapshot['plan']['entries'])
        workflow('apply_plan')
        assert not any(row['conflicts'] for row in snapshot['rows'])
        target_recipe = json.loads(call('image_recipe', photoId=ids[1])['body'])
        assert abs(target_recipe['global']['exposure'] - .6) < 1e-5
        workflow('rollback_plan', checkpointId=checkpoint, sections=['tone'])
        workflow('restore_plan', checkpointId=checkpoint, sections=['tone'])
        workflow('merge_plan', checkpointId=checkpoint, sourcePhotoId=photo, sections=['colour'])
        workflow('verify'); workflow('fingerprints'); workflow('matrix'); workflow('queue')
        assert snapshot['queue'] == ids
        assert len(snapshot['photos']) == len(ids) and all(len(p['sections']) == 7 for p in snapshot['photos'])
        workflow('criteria', text='Colour\nDetail')
        workflow('signoff', criterion='Colour', passed=True); workflow('signoff', criterion='Detail', passed=True)
        assert all(p['status'] == 'approved' for p in snapshot['photos'])
        workflow('handoff'); assert json.loads(snapshot['document'])['ready']
        call('set_param', projectId=project, photoId=photo, path='global.exposure', value=.7, label='Review drift probe')
        workflow('read'); assert any(p['status']=='changed_since_review' for p in snapshot['photos'])
        workflow('handoff'); assert not json.loads(snapshot['document'])['ready']
        workflow('assign', reviewer='Qualification reviewer'); workflow('schedule', dueDate='2026-10-20')
        assert all(snapshot['reviews'][image_id]['reviewer'] == 'Qualification reviewer' and snapshot['reviews'][image_id]['dueDate'] == '2026-10-20' for image_id in ids)
        only = {photo:call('image_recipe', photoId=photo)['recipeHash']}
        workflow('issue', photoIds=[photo], recipeHashes=only, text='Inspect source texture', point=[.3,.5]); first = snapshot['issues'][-1]['id']
        workflow('issue', photoIds=[photo], recipeHashes={photo:next(p['recipeHash'] for p in snapshot['photos'] if p['photoId']==photo)}, text='Verify correction', dependsOn=[first])
        second = snapshot['issues'][-1]['id']
        blocked = False
        try: workflow('resolve_issue', issueId=second, passed=True)
        except Exception: blocked = True
        assert blocked
        workflow('resolve_issue', issueId=first, passed=True); workflow('resolve_issue', issueId=second, passed=True)
        workflow('propagate_issue', issueId=first)
        assert any(issue['photoId'] == ids[1] and issue['point'] is None and not issue['resolved'] for issue in snapshot['issues'])
        workflow('compare', winner='tie', text='Compare recorded saved edits'); workflow('undecided')
        workflow('filter', filter=dict(name='Open corrections', reviewer='', state='', overdueOnly=False, issuesOnly=True))
        workflow('apply_filter', name='Open corrections')
        workflow('export'); portable = snapshot['document']
        workflow('import', document=portable); assert all(not i['resolved'] for i in snapshot['issues'])
        workflow('audit'); workflow('remove_checkpoint', checkpointId=checkpoint)
        passed('all_25_workflow_capabilities', revisionBoundSignoff=True, driftDetected=True, dependencyGate=True, importReopensApproval=True)
        workflow_actions = [
            ['checkpoint'], ['drift'], ['restore_plan'], ['rollback_plan'], ['delta_plan', 'apply_plan'],
            ['delta_plan'], ['merge_plan'], ['verify'], ['fingerprints'], ['matrix'], ['queue'],
            ['criteria', 'signoff'], ['read', 'handoff'], ['issue'], ['resolve_issue'], ['propagate_issue'],
            ['assign'], ['schedule'], ['compare'], ['undecided'], ['filter', 'apply_filter'],
            ['handoff'], ['export'], ['import'], ['audit'],
        ]
        observed = {case['name'] for case in report['cases'] if case['passed']}
        for item, actions in zip([entry for entry in ledger if entry['group'] == 'workflow'], workflow_actions, strict=True):
            assert all('workflow:' + action in observed for action in actions)
            passed(item['id'], group='workflow', operation=item['operation'],
                   actionEvidence=['workflow:' + action for action in actions],
                   scope='Sequential cohort scenario above; see workflow action records and native assertions')

        page.reload()
        page.get_by_role('button', name=f'{name} 2', exact=True).click()
        page.get_by_role('button', name='Auto edit One click, start to finish', exact=True).click()
        page.get_by_role('option', name=photos[0]['fileName'], exact=True).click()
        nav = page.get_by_role('navigation', name='Editing panels')
        nav.get_by_role('button', name='Spatial repair', exact=True).click()
        expect(page.get_by_role('region', name='Spatial repair lab')).to_be_visible()
        assert page.get_by_label('Spatial tool', exact=True).locator('option').count() >= 25
        page.get_by_label('Spatial tool', exact=True).select_option('haar_detail')
        page.get_by_role('button', name='Add spatial operation', exact=True).click()
        page.get_by_label('Spatial detail gain', exact=True).fill('1.2')
        nav.get_by_role('button', name='Colour lab', exact=True).click()
        expect(page.get_by_role('region', name='Selected colour and tone lab')).to_be_visible()
        assert page.get_by_label('Colour method', exact=True).locator('option').count() >= 25
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
        laboratory = page.get_by_text(f'Selection laboratory · {arguments.selection_count} workflows', exact=True)
        expect(laboratory).to_be_visible()
        laboratory.click()
        before_mask = recipe()['recipeHash']
        page.get_by_role('button', name='Apply laboratory workflow', exact=True).click()
        page.wait_for_function('async ([id,prior]) => (await window.__TAURI_INTERNALS__.invoke("image_recipe",{input:{photoId:id}})).recipeHash!==prior', arg=[photo,before_mask])
        expect(page.get_by_role('button', name='Apply laboratory workflow', exact=True)).to_be_enabled()
        page.screenshot(path=str(OUT/'selection.png'), full_page=True)
        passed('Selection laboratory opens and saves a native selection workflow')
        report['originalsUnchanged'] = all(hashlib.sha256(Path(p).read_bytes()).hexdigest()==h for p,h in hashes.items())
        assert report['originalsUnchanged'] and not report['pageErrors']
        report['completed'] = True
except Exception as error:
    report['fatalError'] = str(error); raise
finally:
    persist(); proc.terminate(); proc.wait(timeout=30)
