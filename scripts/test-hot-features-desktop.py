"""Run the five Hot Features through the real, isolated Windows desktop.

Uses six cached real photographs plus a labelled synthetic reflection chart.
Training uses 120 repeated authored RAW/export charts: this qualifies plumbing,
not generalisation to independent photographs. No native command is mocked.
"""
import argparse
import base64
import hashlib
from io import BytesIO
import json
import re
import os
from pathlib import Path
import subprocess
import time
import urllib.request
import uuid

import numpy as np
from PIL import Image, ImageDraw
from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def difference(before, after):
    a, b = np.asarray(before).astype(np.int16), np.asarray(after).astype(np.int16)
    assert a.shape == b.shape
    return dict(identical=bool(np.array_equal(a, b)), meanRgbChange=float(np.abs(a-b).mean()),
                changedPixelFraction=float(np.any(a != b, axis=2).mean()))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exe', type=Path, default=ROOT/'target/desktop-launch/debug/aura-desktop.exe')
    parser.add_argument('--port', type=int, default=9420)
    parser.add_argument('--resume', type=Path)
    args = parser.parse_args()
    base = ROOT/'output/aura-hot-features-2026-10-25'
    run = args.resume.resolve() if args.resume else base/('run-'+uuid.uuid4().hex[:8])
    assert run.parent == base and run.name.startswith('run-')
    run.mkdir(exist_ok=bool(args.resume))
    manifest = json.loads((ROOT/'output/aura-twenty-color-match-2026-10-24/dataset/manifest.json').read_text())
    dataset = ROOT/'output/aura-twenty-color-match-2026-10-24/dataset'
    sources = {str(row['index']): dataset/row['input']['path'] for row in manifest['images']
               if row['role'] == 'target' and row['index'] in [1,9,11,12,16,20]}
    glare = run/'controlled-reflection.png'
    if not glare.exists():
        chart = Image.new('RGB', (512,384), (78,78,78))
        draw = ImageDraw.Draw(chart)
        draw.ellipse((170,120,342,264), fill=(220,225,230))
        draw.rectangle((235,165,242,219), fill=(10,10,10))
        draw.line((210,210,300,210), fill=(18,18,18), width=3)
        chart.save(glare)
    sources['glare'] = glare
    training = base/'authored-training-pairs'
    assert len(list(training.glob('*.dng'))) == len(list(training.glob('*.png'))) == 120
    reference = ROOT/'output/aura-main-match-twenty-2026-10-25/run-df0711e1/references/warm-matte.png'
    hashes = {str(path): digest(path) for path in [*sources.values(), reference, *training.glob('*')]}
    data = json.loads((run/'results.json').read_text()) if args.resume else dict(
        run=str(run), exe=str(args.exe.resolve()), exeSha256=digest(args.exe), cases=[], pageErrors=[],
        originalHashes=hashes, sourceDescriptions='Six real photographs; one synthetic reflection chart; 120 repeated authored RAW/export charts for training only.')
    assert data['exeSha256'] == digest(args.exe) and data['originalHashes'] == hashes
    verify_restart = bool(args.resume and data.get('completed'))
    data.pop('completed', None)
    data.pop('checks', None)
    if 'failure' in data:
        data.setdefault('previousHarnessFailures', []).append(data.pop('failure'))
    print(json.dumps(dict(started=str(run), resumed=bool(args.resume))), flush=True)

    def save():
        (run/'results.json').write_text(json.dumps(data, indent=2), encoding='utf-8')

    env = dict(os.environ, AURA_TEST_CATALOG=str(run/'catalog.sqlite'),
               WEBVIEW2_USER_DATA_FOLDER=str(run/'webview'),
               WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-address=127.0.0.1 --remote-debugging-port={args.port}')
    startup = subprocess.STARTUPINFO(); startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW; startup.wShowWindow = 0
    with (run/'desktop.log').open('a', encoding='utf-8') as log:
        process = subprocess.Popen([str(args.exe.resolve())], cwd=ROOT, env=env, startupinfo=startup, stdout=log, stderr=subprocess.STDOUT)
        try:
            for _ in range(90):
                if process.poll() is not None: raise RuntimeError('Desktop exited at startup')
                try:
                    urllib.request.urlopen(f'http://127.0.0.1:{args.port}/json/version', timeout=1).close(); break
                except OSError: time.sleep(1)
            with sync_playwright() as pw:
                page = pw.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}').contexts[0].pages[0]
                page.set_default_timeout(120000); expect.set_options(timeout=120000)
                page.set_viewport_size(dict(width=1920,height=1080))
                page.wait_for_function('()=>!!window.__TAURI_INTERNALS__')
                page.on('pageerror', lambda error: data['pageErrors'].append(str(error)))

                def native(command, **kwargs):
                    return page.evaluate('async ([c,a])=>window.__TAURI_INTERNALS__.invoke(c,a)', [command, kwargs])

                def call(command, **kwargs): return native(command, input=kwargs)

                def render(photo):
                    pixels = call('render_image', photoId=photo, level='full', purpose='export')
                    return Image.frombytes('RGB',(pixels['width'],pixels['height']),base64.b64decode(pixels['rgbBase64']))

                name = 'Hot Features '+run.name
                if not args.resume:
                    data['projectId'] = call('create_project',name=name,coupleNames=None,eventDate=None)['id']
                    call('start_ingest', projectId=data['projectId'], roots=[str(path) for path in sources.values()])
                project = data['projectId']
                deadline = time.monotonic()+120
                while time.monotonic()<deadline:
                    photos = call('list_images', projectId=project,offset=0,limit=50,orderBy='timeline')
                    if len(photos)==len(sources): break
                    time.sleep(.5)
                assert len(photos)==7
                ids = {key: next(p['id'] for p in photos if p['fileName']==path.name) for key,path in sources.items()}
                data['photoIds'] = ids
                for key,photo in ids.items():
                    if not args.resume: render(photo).save(run/f'{key}.original.png')
                save()

                def open_tool(title, key=None):
                    page.reload()
                    page.get_by_role('button',name=f'{name} 7',exact=True).click()
                    page.get_by_role('navigation',name='Workspace').get_by_role('button',name=re.compile(r'^Hot Features')).click()
                    page.get_by_role('button',name=title,exact=True).click()
                    if key:
                        page.get_by_role('listbox',name='Filmstrip').get_by_role('option',name=sources[key].name,exact=True).click()

                def complete(case):
                    data['cases'].append(case); save(); print(json.dumps(case),flush=True)

                def done(key): return any(case['case']==key for case in data['cases'])

                def history_len(photo):
                    return len(call('image_history',photoId=photo)['entries'])

                def history_check(photo, before, after, steps=1):
                    # Auto retouch saves one history step per stage, so undo every step it added.
                    for _ in range(max(1,steps)):
                        call('history_step',projectId=project,photoId=photo,action='undo')
                    assert render(photo).tobytes()==before.tobytes(), f'Undo x{steps} did not restore the original'
                    for _ in range(max(1,steps)):
                        call('history_step',projectId=project,photoId=photo,action='redo')
                    assert render(photo).tobytes()==after.tobytes(), f'Redo x{steps} did not restore the edit'

                if not done('background'):
                    photo=ids['12']
                    call('history_step',projectId=project,photoId=photo,action='reset_original')
                    finish=call('studio_finish',projectId=project,photoId=photo)['finish']
                    finish.update(backgroundRepair=0, liquify=[dict(mode='push',radius=.4,strength=.8,points=[[.15,.5],[.25,.5]])])
                    call('save_studio_finish',projectId=project,photoId=photo,finish=finish,label='Qualification background warp')
                    before=render(photo); before.save(run/'background.warped.png')
                    open_tool('Liquify Background Repair','12')
                    button=page.get_by_role('button',name='Repair background automatically',exact=True)
                    expect(button).to_be_enabled(); button.click(); expect(button).to_be_disabled()
                    deadline=time.monotonic()+120
                    while time.monotonic()<deadline:
                        finish=call('studio_finish',projectId=project,photoId=photo)['finish']
                        if finish.get('backgroundRepair')==100: break
                        if page.get_by_role('alert').count(): raise AssertionError(page.get_by_role('alert').all_text_contents())
                        time.sleep(.25)
                    assert finish.get('backgroundRepair')==100
                    after=render(photo); after.save(run/'background.repaired.png')
                    assert before.tobytes()!=after.tobytes()
                    history_check(photo,before,after)
                    page.screenshot(path=str(run/'background.ui.png'),full_page=True)
                    complete(dict(case='background',photo='12',result='applied',undoRedo=True,**difference(before,after)))

                for key in ['1','11','20']:
                    case='skin-'+key
                    if done(case): continue
                    photo=ids[key]
                    call('history_step',projectId=project,photoId=photo,action='reset_original')
                    before=render(photo); depth=history_len(photo)
                    open_tool('Face Freckles & Acne',key)
                    button=page.get_by_role('button',name='Auto retouch: Face + body skin',exact=True)
                    expect(button).to_be_enabled()
                    # These controls are measured defaults of the actual Hot Features preset.
                    expect(page.get_by_label('Keep freckles',exact=True)).to_be_checked()
                    expect(page.get_by_label('Keep moles',exact=True)).to_be_checked()
                    button.click(); expect(button).to_be_enabled(timeout=300000)
                    after=render(photo); after.save(run/f'{case}.after.png')
                    edits=call('native_retouch_edit',projectId=project,photoId=photo,action='list',edits=[],id=None)
                    errors=page.get_by_role('alert').all_text_contents()
                    if before.tobytes()!=after.tobytes(): history_check(photo,before,after,history_len(photo)-depth)
                    page.screenshot(path=str(run/f'{case}.ui.png'),full_page=True)
                    complete(dict(case=case,photo=key,operations=len(edits),errors=errors,
                                  result='refused' if errors else 'unchanged' if before.tobytes()==after.tobytes() else 'applied',**difference(before,after)))

                if not done('glare'):
                    photo=ids['glare']
                    call('history_step',projectId=project,photoId=photo,action='reset_original')
                    before=render(photo)
                    open_tool('Remove Glasses Glare','glare')
                    expect(page.get_by_label('Preview unsaved changes')).to_be_checked()
                    expect(page.get_by_label('Tool',exact=True)).to_have_value('glare')
                    recipe=call('image_recipe',photoId=photo)['recipeHash']
                    for label,value in [('Center X (%)','50'),('Center Y (%)','50'),('Horizontal radius (%)','20'),('Vertical radius (%)','25')]:
                        page.get_by_label(label,exact=True).fill(value)
                    expect(page.get_by_role('button',name='Face Freckles & Acne',exact=True)).to_be_disabled()
                    assert call('image_recipe',photoId=photo)['recipeHash']==recipe
                    assert call('native_retouch_edit',projectId=project,photoId=photo,action='list',edits=[],id=None)==[]
                    button=page.get_by_role('button',name='Apply retouch',exact=True)
                    expect(button).to_be_enabled();button.click()
                    expect(page.get_by_role('button',name='Face Freckles & Acne',exact=True)).to_be_enabled()
                    edits=call('native_retouch_edit',projectId=project,photoId=photo,action='list',edits=[],id=None)
                    assert len(edits)==1 and edits[0]['tool']=='glare'
                    after=render(photo);after.save(run/'glare.after.png')
                    a,b=np.asarray(before),np.asarray(after)
                    assert after.tobytes()!=before.tobytes()
                    assert np.array_equal(a[165:219,235:242],b[165:219,235:242]), 'Dark detail changed'
                    assert np.array_equal(a[:80],b[:80]), 'Unselected pixels changed'
                    history_check(photo,before,after)
                    preview=call('native_retouch_draft_preview',projectId=project,photoId=photo,edit=edits[0],replaceId=edits[0]['id'],quality='full')
                    assert base64.b64decode(preview['rgbBase64'])==after.tobytes()
                    page.screenshot(path=str(run/'glare.ui.png'),full_page=True)
                    complete(dict(case='glare',result='applied',input='synthetic reflection chart',darkDetailPreserved=True,
                                  outsideSelectionUnchanged=True,previewDidNotSave=True,previewEqualsSaved=True,undoRedo=True,**difference(before,after)))

                if not done('train'):
                    open_tool('Train Your AI Look')
                    page.get_by_label('What to call this look').fill('hot-qualification')
                    page.get_by_label('Folders, one per line').fill(str(run/'missing-folder'))
                    page.get_by_role('button',name='Look at these folders',exact=True).click()
                    expect(page.get_by_role('alert')).to_be_visible()
                    assert page.get_by_role('button',name='Learn my style',exact=True).count()==0
                    page.get_by_label('Folders, one per line').fill(str(training))
                    page.get_by_role('button',name='Look at these folders',exact=True).click()
                    learn=page.get_by_role('button',name='Learn my style',exact=True);expect(learn).to_be_enabled()
                    expect(page.get_by_role('alert')).to_have_count(0)
                    started=time.monotonic()
                    learn.click();expect(page.get_by_role('button',name='Stop learning',exact=True)).to_be_enabled()
                    expect(page.locator('.teach-result')).to_be_visible(timeout=600000)
                    profiles=native('list_profiles')
                    profile=next(profile for profile in profiles if profile['name']=='hot-qualification')
                    assert profile['trainedPairs']==120 and profile['status']=='candidate'
                    expect(page.get_by_role('button',name='Use this profile',exact=True)).to_be_disabled()
                    pairs=native('profile_pairs',name='hot-qualification',limit=200)
                    assert len(pairs)==120 and all(pair['accepted'] for pair in pairs)
                    report=native('profile_report',profileId=profile['profileId'])
                    data['trainingReport']=report
                    page.screenshot(path=str(run/'training.ui.png'),full_page=True)
                    complete(dict(case='train',result='candidate',pairs=120,profile=profile,
                                  elapsedSeconds=time.monotonic()-started,
                                  failedRescanCannotTrain=True,errorClearedOnSuccessfulScan=True,
                                  insufficientlyQualifiedCandidateCannotBeAdopted=True,
                                  generalisationQualified=False,fixture='120 copies of one authored RAW/graded chart'))

                if not done('color'):
                    ref=call('import_color_reference',path=str(reference),cancelId=str(uuid.uuid4()))
                    photo=ids['16']
                    call('history_step',projectId=project,photoId=photo,action='reset_original')
                    before=render(photo)
                    open_tool('AI Color Match','16')
                    panel=page.get_by_role('region',name='Reference colour matching',exact=True)
                    panel.get_by_label('Saved colour reference',exact=True).select_option(ref['id'])
                    panel.get_by_label('Matching mode',exact=True).select_option('precise')
                    button=panel.get_by_role('button',name='Match reference to this photo',exact=True)
                    expect(button).to_be_enabled();button.click();expect(button).to_be_enabled()
                    after=render(photo);after.save(run/'color.after.png')
                    errors=page.get_by_role('alert').all_text_contents()
                    status=native('color_match_status',photoId=photo)
                    if not errors: assert status and status['current'] and status['fullResolutionChecked']
                    page.screenshot(path=str(run/'color.ui.png'),full_page=True)
                    complete(dict(case='color',photo='16',result='refused' if errors else 'unchanged' if before.tobytes()==after.tobytes() else 'applied',
                                  errors=errors,status=status,**difference(before,after)))

                for key,photo in ids.items():
                    saved=render(photo)
                    recipe_hash=call('image_recipe',photoId=photo)['recipeHash']
                    if verify_restart:
                        with Image.open(run/f'{key}.saved.png') as previous:
                            assert previous.size==saved.size and previous.convert('RGB').tobytes()==saved.tobytes(), f'Reopened output changed: {key}'
                        assert data['savedRecipeHashes'][key]==recipe_hash, f'Reopened recipe changed: {key}'
                    data.setdefault('savedRecipeHashes', {})[key]=recipe_hash
                    saved.save(run/f'{key}.saved.png')
                    preview=native('native_retouch_preview',projectId=project,photoId=photo,before=False,quality='full')
                    assert base64.b64decode(preview['rgbBase64'])==saved.tobytes()
                delivery=call('export_run',projectId=project,destination=str(run/'delivery'),destinationKind='folder',
                    sets=[dict(name='Hot Features verification',imageIds=list(ids.values()),format='png',quality=95,colour='srgb',bitDepth=8,resize='full',sharpen='none',naming='{seq}_{original}',sidecar=False)],
                    copyright=None,contact=None,creator=None,keywords=[],stripGps=True,stripCameraSerial=True,verify=True)
                assert delivery['written']==delivery['verified']==7 and delivery['corrupt']==delivery['renderFailed']==0
                for file in native('export_files',projectId=project):
                    key=next(key for key,photo in ids.items() if photo==file['imageId'])
                    with Image.open(run/'delivery'/file['path']) as delivered,Image.open(run/f'{key}.saved.png') as saved:
                        assert delivered.convert('RGB').tobytes()==saved.convert('RGB').tobytes()
                data['delivery']=delivery
                data['checks']=dict(sevenCasesCompleted=len(data['cases'])==7,allSourcesUnchanged=all(digest(path)==value for path,value in hashes.items()),
                                    previewExportEqual=True,sevenDeliveriesVerified=True,noPageErrors=not data['pageErrors'])
                assert all(data['checks'].values())
                data['completed']=True
                if verify_restart: data['restartVerification']=dict(allSevenSavedPixelsIdentical=True,allSevenRecipeHashesUnchanged=True)
                print(json.dumps(dict(completed=str(run),checks=data['checks'])),flush=True)
        except BaseException as error:
            data['failure']=f'{type(error).__name__}: {error}';raise
        finally:
            try: save()
            finally: process.terminate()
            try:process.wait(timeout=15)
            except subprocess.TimeoutExpired:process.kill();process.wait(timeout=10)


if __name__=='__main__':main()
