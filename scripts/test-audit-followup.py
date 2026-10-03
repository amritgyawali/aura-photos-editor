"""Focused retests for findings from test-capability-audit.py; retains both runs."""
import base64
import hashlib
import json
import time
from pathlib import Path
from PIL import Image
from playwright.sync_api import sync_playwright

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'.work-checks/full-audit'
baseline=json.loads((OUT/'runtime-results.json').read_text())
report={'cases':[]}
project=baseline['projectId']
photo=next(p['id'] for p in baseline['photos'] if p['fileName']=='test-opaque.png')

def save():
    (OUT/'followup-results.json').write_text(json.dumps(report,indent=2),encoding='utf-8')

with sync_playwright() as pw:
    browser=pw.chromium.connect_over_cdp('http://127.0.0.1:9223')
    page=browser.contexts[0].pages[0]
    def invoke(cmd,args):
        return page.evaluate('''async ([c,a])=>{try{return await window.__TAURI_INTERNALS__.invoke(c,a)}catch(e){throw Error(JSON.stringify(e))}}''',[cmd,args])
    def call(cmd,**args): return invoke(cmd,{'input':args})
    def recipe(): return call('image_recipe',photoId=photo)
    def pixels(id=photo):
        r=call('render_image',photoId=id,level='full',purpose='export')
        return hashlib.sha256(base64.b64decode(r['rgbBase64'])).hexdigest()
    def edit(action,edits=[]):
        return call('native_retouch_edit',projectId=project,photoId=photo,action=action,edits=edits,id=None)
    def history(action): return call('history_step',projectId=project,photoId=photo,action=action)
    def case(name,fn):
        started=time.monotonic()
        try: row={'name':name,'status':'executed','result':fn()}
        except Exception as e: row={'name':name,'status':'error','error':str(e)}
        row['seconds']=round(time.monotonic()-started,3)
        report['cases'].append(row);save()
        print(name,row['status'],str(row.get('result',row.get('error')))[:200],flush=True)

    case('extreme-exposure-is-clamped',lambda:next(p for p in recipe()['params'] if p['path']=='global.exposure'))
    history('reset_original')
    edit('clear')
    for tool in [r['name'].split(':')[1] for r in baseline['cases'] if r['name'].startswith('tool:')]:
        def check(tool=tool):
            edit('clear')
            before=pixels()
            op=dict(id='audit',tool=tool,enabled=True,region=[.46,.53,.07,.08],source=[.49,.5],amount=.45,
                    feather=.7,radius=.008,texture=.7,tone=.6,warmth=.25,tint=.1)
            edit('append',[op]);after=pixels()
            history('undo');undone=pixels()
            history('redo');redone=pixels()
            return dict(changed=after!=before,exactUndo=undone==before,exactRedo=redone==after)
        case('isolated-tool:'+tool,check)

    def reset_bug():
        edit('clear');before=pixels()
        op=dict(id='reset-check',tool='dodge',enabled=True,region=[.5,.5,.1,.1],source=None,
                amount=.3,feather=.5,radius=.003,texture=1,tone=.5,warmth=0,tint=0)
        edit('append',[op])
        prior=call('image_history',photoId=photo)
        history('reset_original');reset=pixels()
        following=call('image_history',photoId=photo)
        edit('append',[dict(op,tool='burn')]);history('undo')
        return dict(resetLooksOriginal=reset==before,undoAfterNewEditRestoresReset=pixels()==reset,
                    historyBefore=prior,historyAfterReset=following)
    case('reset-retouch-history-reproduction',reset_bug)
    for filename in ['portrait-8386841.jpg','test-rgb.tif','test-gray16.tif']:
        id=next(p['id'] for p in baseline['photos'] if p['fileName']==filename)
        case('retry:'+filename,lambda id=id:dict(pixelHash=pixels(id)))

    def profiles():
        rows=invoke('list_edit_profiles',{})
        result=call('apply_edit_profile',photoId=photo,profileId=rows[0]['id'],strength=.65)
        first=recipe()['recipeHash']
        call('apply_edit_profile',photoId=photo,profileId=rows[0]['id'],strength=.65)
        return dict(count=len(rows),firstProfile=rows[0]['id'],report=result,idempotent=first==recipe()['recipeHash'])
    case('adaptive-profile',profiles)

    def sync():
        target=next(p['id'] for p in baseline['photos'] if p['fileName']=='landscape.jpg')
        call('set_param',projectId=project,photoId=photo,path='global.exposure',value=.25,label='Audit sync source')
        r=call('sync_settings',projectId=project,sourcePhotoId=photo,targetPhotoIds=[target],includeGeometry=False,groups=['tone'])
        return dict(report=r,targetExposure=next(p for p in call('image_recipe',photoId=target)['params'] if p['path']=='global.exposure'))
    case('selective-sync',sync)

    def export_resize():
        destination=OUT/'export-resize-watermark'
        job=dict(projectId=project,destination=str(destination),destinationKind='folder',copyright='AURA audit',contact=None,creator='AURA QA',
                 keywords=['audit'],stripGps=True,stripCameraSerial=True,verify=True,
                 sets=[dict(name='watermarked',imageIds=[photo],format='png',quality=95,colour='srgb',bitDepth=8,
                            resize='long_edge:256',sharpen='none',naming='{original}',sidecar=True)])
        watermark=dict(width=2,height=2,rgba=[255,0,0,255]*4,opacity=.8,widthFraction=.15,marginFraction=.03,anchor='bottom_right')
        r=invoke('export_run_watermarked',{'input':job,'watermark':watermark})
        outputs=[]
        for f in destination.rglob('*.png'):
            with Image.open(f) as img:
                outputs.append(dict(file=str(f.relative_to(destination)),size=list(img.size),sample=img.getpixel((int(img.width*.9),int(img.height*.9)))))
        return dict(export=r,files=outputs)
    case('resized-watermarked-export',export_resize)
    report['completed']=True;save()
