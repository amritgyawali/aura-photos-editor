"""Check rotated real portraits, adaptive decisions, reset and manual provenance.

Requires the rebuilt desktop running with CDP on port 9223, Pillow, Playwright,
and the public portrait fixtures used by test-auto-portrait.py.
"""
import base64
import hashlib
import json
import time
from pathlib import Path

from PIL import Image
from playwright.sync_api import sync_playwright, expect

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'.work-checks/portrait-v2'
INPUT=OUT/'inputs'
INPUT.mkdir(parents=True,exist_ok=True)
source=ROOT/'.work-checks/portrait-review/originals/portrait-1239291.jpg'
original_hash=hashlib.sha256(source.read_bytes()).hexdigest()
with Image.open(source) as image:
    for angle,method in [(90,Image.Transpose.ROTATE_90),(180,Image.Transpose.ROTATE_180),(270,Image.Transpose.ROTATE_270)]:
        image.transpose(method).save(INPUT/f'rotation-{angle}.png')
report={'photos':[]}
with sync_playwright() as p:
    page=p.chromium.connect_over_cdp('http://127.0.0.1:9223').contexts[0].pages[0]
    page.set_default_timeout(90000);expect.set_options(timeout=90000)
    def call(cmd,**args):
        return page.evaluate('''async ([c,a])=>{try{return await window.__TAURI_INTERNALS__.invoke(c,{input:a})}catch(e){throw Error(JSON.stringify(e))}}''',[cmd,args])
    name='Adaptive portrait verification '+time.strftime('%H-%M-%S')
    project=call('create_project',name=name,coupleNames=None,eventDate=None)['id']
    call('start_ingest',projectId=project,roots=[str(INPUT),str(source)])
    deadline=time.monotonic()+90
    photos=[]
    while len(photos)<4 and time.monotonic()<deadline:
        photos=call('list_images',projectId=project,offset=0,limit=20,orderBy='timeline')
        time.sleep(.3)
    assert len(photos)==4
    report.update(projectId=project,collectionName=name)
    def render(photo):
        r=call('render_image',photoId=photo,level='full',purpose='export')
        return Image.frombytes('RGB',(r['width'],r['height']),base64.b64decode(r['rgbBase64']))
    def stack(photo,action,edits=[]):
        return call('native_retouch_edit',projectId=project,photoId=photo,action=action,edits=edits,id=None)
    def step(photo,action):return call('history_step',projectId=project,photoId=photo,action=action)
    for row in photos:
        photo=row['id'];before=render(photo)
        result=call('enhance_portrait',photoId=photo)
        plan=json.loads(result['body'])['studio_portrait_auto_v1']
        assert plan['retouchedFaces']==1 and plan['operations']==3,plan
        assert plan['plannerVersion']=='sample-consensus-v2'
        assert len(plan['assessments'])==1 and plan['assessments'][0]['reason']
        after=render(photo)
        assert after.tobytes()!=before.tobytes()
        after.save(OUT/(row['fileName']+'-after.png'))
        history=call('image_history',photoId=photo)
        assert call('enhance_portrait',photoId=photo)['recipeHash']==result['recipeHash']
        assert call('image_history',photoId=photo)==history
        # Exercise the audit's exact failure: a retouch-only reset must clear
        # extensions, create history, and be reversible pixel for pixel.
        step(photo,'reset_original')
        assert stack(photo,'list')==[]
        assert render(photo).tobytes()==before.tobytes(), 'Reset kept a retouch effect'
        step(photo,'undo')
        assert render(photo).tobytes()==after.tobytes()
        step(photo,'redo')
        assert render(photo).tobytes()==before.tobytes()
        # A snapshot before retouch must also remove optional recipe fields.
        call('snapshot',projectId=project,photoId=photo,action='take',name='Before portrait')
        call('enhance_portrait',photoId=photo)
        call('snapshot',projectId=project,photoId=photo,action='restore',name='Before portrait')
        assert stack(photo,'list')==[] and render(photo).tobytes()==before.tobytes()
        step(photo,'undo')
        saved=stack(photo,'list')
        stack(photo,'update',[dict(saved[0],amount=.07)])
        manual=call('image_history',photoId=photo)['entries'][-1]
        assert manual['source']=='user',manual
        protected=call('enhance_portrait',photoId=photo)
        assert json.loads(protected['body'])['studio_portrait_auto_v1']['status']=='protected'
        assert abs(stack(photo,'list')[0]['amount']-.07)<1e-5
        step(photo,'undo');step(photo,'undo')
        report['photos'].append(dict(file=row['fileName'],photoId=photo,analysis=plan,resetUndoRedo=True,snapshotRestore=True,manualProvenance=True,manualProtected=True))
        (OUT/'results.json').write_text(json.dumps(report,indent=2))
        print('PASS',row['fileName'],flush=True)
    page.reload()
    page.get_by_role('button',name=name+' 4',exact=True).click()
    page.get_by_role('button',name='Auto edit One click, start to finish',exact=True).click()
    page.get_by_role('option',name=photos[0]['fileName'],exact=True).click()
    expect(page.get_by_role('button',name='Retouch',exact=True)).to_be_enabled()
    page.get_by_text('Automatic decisions by face (1)',exact=True).click()
    expect(page.get_by_text('Face 1 · retouched',exact=True)).to_be_visible()
    page.screenshot(path=str(OUT/'face-decisions.png'))
    page.get_by_role('button',name='Reset photo',exact=True).click()
    expect(page.get_by_role('button',name='Retouch',exact=True)).to_be_enabled()
    assert stack(photos[0]['id'],'list')==[]
    page.get_by_role('button',name='Undo',exact=True).click()
    expect(page.get_by_text('Automatic decisions by face (1)',exact=True)).to_be_visible()
    report.update(guiDetailsAndReset=True,originalUnchanged=hashlib.sha256(source.read_bytes()).hexdigest()==original_hash,status='passed')
    (OUT/'results.json').write_text(json.dumps(report,indent=2))
