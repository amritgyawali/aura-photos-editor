"""Resolve audit ambiguities using new collections with one source each."""
import base64
import hashlib
import json
import time
from pathlib import Path
from playwright.sync_api import sync_playwright, expect

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'.work-checks/full-audit'
report={'cases':[]}
with sync_playwright() as p:
    page=p.chromium.connect_over_cdp('http://127.0.0.1:9223').contexts[0].pages[0]
    expect.set_options(timeout=90000)
    page.set_default_timeout(90000)
    def call(cmd,**args):
        return page.evaluate('''async ([c,a])=>{try{return await window.__TAURI_INTERNALS__.invoke(c,{input:a})}catch(e){throw Error(JSON.stringify(e))}}''',[cmd,args])
    def new(filename):
        project=call('create_project',name='Isolated audit '+filename+' '+time.strftime('%H-%M-%S'),coupleNames=None,eventDate=None)['id']
        call('start_ingest',projectId=project,roots=[str(OUT/'inputs'/filename)])
        for _ in range(100):
            rows=call('list_images',projectId=project,offset=0,limit=20,orderBy='timeline')
            if rows:return project,rows[0]['id']
            time.sleep(.3)
        raise TimeoutError('Import did not appear')
    def render(photo):
        r=call('render_image',photoId=photo,level='full',purpose='export')
        return dict(size=[r['width'],r['height']],hash=hashlib.sha256(base64.b64decode(r['rgbBase64'])).hexdigest())
    def case(name,fn):
        try:r=dict(name=name,status='executed',result=fn())
        except Exception as e:r=dict(name=name,status='error',error=str(e))
        report['cases'].append(r)
        (OUT/'isolated-results.json').write_text(json.dumps(report,indent=2))
        print(name,r['status'],str(r.get('result',r.get('error')))[:250],flush=True)
    for filename in ['test-rgb.tif','test-gray16.tif','portrait-8386841.jpg']:
        def format_check(filename=filename):
            project,photo=new(filename)
            return dict(projectId=project,photoId=photo,render=render(photo))
        case('single-file:'+filename,format_check)
    def reset_check():
        project,photo=new('test-opaque.png')
        def edit(tool):
            return call('native_retouch_edit',projectId=project,photoId=photo,action='append',id=None,
                        edits=[dict(id='audit',tool=tool,enabled=True,region=[.5,.5,.1,.1],source=None,
                                    amount=.4,feather=.5,radius=.003,texture=1,tone=.5,warmth=0,tint=0)])
        original=render(photo)
        edit('dodge');dodge=render(photo)
        h1=call('image_history',photoId=photo)
        call('history_step',projectId=project,photoId=photo,action='reset_original')
        reset=render(photo)
        h2=call('image_history',photoId=photo)
        edit('burn')
        call('history_step',projectId=project,photoId=photo,action='undo')
        undone=render(photo)
        return dict(projectId=project,photoId=photo,original=original,dodge=dodge,reset=reset,undo=undone,
                    resetEqualsOriginal=reset==original,undoEqualsReset=undone==reset,undoEqualsEarlierDodge=undone==dodge,
                    historyBefore=h1,historyAfterReset=h2)
    case('fresh-retouch-reset-undo',reset_check)

    gui=json.loads((OUT/'gui/results.json').read_text())
    page.reload()
    page.get_by_role('button',name=gui['collectionName']+' 1',exact=True).click()
    page.get_by_role('button',name='Auto edit One click, start to finish',exact=True).click()
    page.get_by_role('option',name='portrait-1239291.jpg',exact=True).click()
    expect(page.get_by_role('button',name='Retouch',exact=True)).to_be_enabled()
    def exposure_retest():
        for value in [.4,.35]:
            field=page.get_by_role('spinbutton',name='Exposure value',exact=True)
            field.click();page.keyboard.press('Control+a');page.keyboard.type(str(value));page.keyboard.press('Enter')
            deadline=time.monotonic()+90
            while True:
                r=call('image_recipe',photoId=gui['photoId'])
                actual=next(v['value'] for v in r['params'] if v['path']=='global.exposure')
                if abs(actual-value)<1e-5:break
                if time.monotonic()>deadline:raise AssertionError(actual)
                time.sleep(.3)
            expect(page.get_by_role('button',name='Retouch',exact=True)).to_be_enabled()
        return dict(storedValue=actual,tolerance=1e-5)
    case('GUI exposure repeat with correct numeric tolerance',exposure_retest)
    page.screenshot(path=str(OUT/'gui/final-develop.png'))
    page.get_by_role('button',name='Export Ready to share',exact=True).click()
    (OUT/'gui/export-dom.txt').write_text(page.locator('body').inner_text(),encoding='utf-8')
