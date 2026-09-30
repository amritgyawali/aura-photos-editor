"""Validate automatic portrait editing in the running desktop with five real photos.

Uses an isolated collection, never changes input photos. Requires Pillow, Playwright.
"""
import argparse
import base64
import hashlib
import json
import re
import time
from pathlib import Path
from PIL import Image, ImageChops
from playwright.sync_api import expect, sync_playwright


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--photos', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--endpoint', default='http://127.0.0.1:9223')
    parser.add_argument('--resume', action='store_true', help='Verify saved results and resume the same test collection after a desktop restart')
    args = parser.parse_args()
    paths = sorted(args.photos.resolve().glob('*.jpg'))
    assert len(paths) == 5
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    digest = lambda data: hashlib.sha256(data).hexdigest()
    originals = {p.name: digest(p.read_bytes()) for p in paths}
    report = json.loads((output/'results.json').read_text(encoding='utf-8')) if args.resume else {'photos': [], 'originalHashes': originals}
    assert report['originalHashes']==originals, 'Originals changed since the earlier run'
    report.update(status='running')
    report.pop('error',None)
    name = report.get('collectionName') or f'Automatic portrait validation {time.strftime("%Y-%m-%d %H-%M-%S")}'
    with sync_playwright() as p:
        browser = p.chromium.connect_over_cdp(args.endpoint)
        page = next(p for c in browser.contexts for p in c.pages if 'tauri' in p.url or 'localhost' in p.url)
        page.set_default_timeout(60000)
        expect.set_options(timeout=60000)
        def invoke(command, args):
            return page.evaluate('async ([cmd,args]) => await window.__TAURI_INTERNALS__.invoke(cmd,args)', [command,args])
        def pixels(photo):
            result = invoke('native_retouch_preview', {'projectId':project,'photoId':photo,'before':False})
            return Image.frombytes('RGB',(result['width'],result['height']),base64.b64decode(result['rgbBase64']))
        def full_pixels(photo):
            result=invoke('render_image',{'input':{'photoId':photo,'level':'full','purpose':'export'}})
            return Image.frombytes('RGB',(result['width'],result['height']),base64.b64decode(result['rgbBase64']))
        def edits(photo):
            return invoke('native_retouch_edit',{'input':{'projectId':project,'photoId':photo,'action':'list','edits':[],'id':None}})
        def history(photo):
            return invoke('image_history',{'input':{'photoId':photo}})
        def step(photo, action):
            invoke('history_step',{'input':{'projectId':project,'photoId':photo,'action':action}})
        try:
            project=report.get('projectId') or invoke('create_project',{'input':{'name':name,'coupleNames':None,'eventDate':None}})['id']
            report.update(projectId=project,collectionName=name)
            invoke('start_ingest',{'input':{'projectId':project,'roots':[str(p) for p in paths]}})
            deadline=time.monotonic()+90
            photos=[]
            while len(photos)!=5 and time.monotonic()<deadline:
                photos=invoke('list_images',{'input':{'projectId':project,'offset':0,'limit':10,'orderBy':'timeline'}})
                time.sleep(.5)
            assert len(photos)==5
            for index,row in enumerate(photos):
                photo=row['id']
                previous=next((p for p in report['photos'] if p['id']==photo),None)
                if previous:
                    assert invoke('image_recipe',{'input':{'photoId':photo}})['recipeHash']==previous['recipeHash']
                    assert digest(pixels(photo).tobytes())==previous['afterSha256'], 'Saved pixels changed after restart'
                    assert len(edits(photo))==previous['analysis']['operations']
                    print(f"PASS {row['fileName']}: persisted operations and pixels after restart",flush=True)
                    continue
                before=pixels(photo)
                start=time.monotonic()
                command='enhance_photo' if index%2==0 else 'enhance_portrait'
                result=invoke(command,{'input':{'photoId':photo}})
                analysis=json.loads(result['body'])['studio_portrait_auto_v1']
                assert analysis['detectedFaces']>=1 and analysis['retouchedFaces']>=1, analysis
                saved=edits(photo)
                assert len(saved)==analysis['operations']==analysis['retouchedFaces']*3
                assert {e['tool'] for e in saved}=={'skin_smooth','skin_uniformity','portrait_dodge_burn'}
                elapsed=time.monotonic()-start
                after=pixels(photo)
                assert before.tobytes()!=after.tobytes(), 'Automatic steps had no visible pixel effect'
                if command=='enhance_portrait':
                    changed=ImageChops.difference(before,after).getbbox()
                    boxes=[f['bounds'] for f in analysis['faces']]
                    assert changed[0]>=min(b[0] for b in boxes)*after.width-2
                    assert changed[1]>=min(b[1] for b in boxes)*after.height-2
                    assert changed[2]<=max(b[2] for b in boxes)*after.width+2
                    assert changed[3]<=max(b[3] for b in boxes)*after.height+2
                before.save(output/f"{row['fileName']}-before.png")
                after.save(output/f"{row['fileName']}-after.png")
                previous_history=history(photo)
                again=invoke(command,{'input':{'photoId':photo}})
                assert again['recipeHash']==result['recipeHash']
                assert history(photo)==previous_history and edits(photo)==saved, 'Rerun duplicated edits/history'
                step(photo,'undo')
                assert edits(photo)==[] and pixels(photo).tobytes()==before.tobytes()
                step(photo,'redo')
                assert edits(photo)==saved and pixels(photo).tobytes()==after.tobytes()
                change=dict(saved[0],amount=.07)
                invoke('native_retouch_edit',{'input':{'projectId':project,'photoId':photo,'action':'update','edits':[change],'id':None}})
                manual=edits(photo)
                protected=invoke('enhance_photo',{'input':{'photoId':photo}})
                assert json.loads(protected['body'])['studio_portrait_auto_v1']['status']=='protected'
                assert edits(photo)==manual
                step(photo,'undo') # remove global auto pass and its report
                step(photo,'undo') # restore original automatic operations
                assert edits(photo)==saved
                assert invoke('image_recipe',{'input':{'photoId':photo}})['recipeHash']==result['recipeHash']
                item={'id':photo,'file':row['fileName'],'command':command,'analysisSeconds':elapsed,'analysis':analysis,
                      'idempotent':True,'undoRedo':True,'manualProtected':True,'recipeHash':result['recipeHash'],
                      'afterSha256':digest(after.tobytes()),'fullAfterSha256':digest(full_pixels(photo).tobytes())}
                report['photos'].append(item)
                (output/'results.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
                print(f"PASS {row['fileName']}: detected, retouched, repeatable, undo/redo, manual protection",flush=True)
            exported=invoke('export_run',{'input':{'projectId':project,'destination':str(output/'exports'),
                'destinationKind':'folder','copyright':None,'contact':None,'creator':None,'keywords':[],
                'stripGps':True,'stripCameraSerial':True,'verify':True,
                'sets':[{'name':'auto-portrait','imageIds':[p['id'] for p in photos],'format':'png','quality':95,
                         'colour':'srgb','bitDepth':8,'resize':'full','sharpen':'none','naming':'{original}','sidecar':True}]}})
            assert exported['written']==exported['verified']==5
            assert exported['corrupt']==exported['renderFailed']==0
            report['export']=exported
            files=invoke('export_files',{'projectId':project})
            for row in report['photos']:
                matches=[f for f in files if f['imageId']==row['id']]
                assert matches
                for file in matches:
                    with Image.open(output/'exports'/file['path']) as image:
                        assert digest(image.convert('RGB').tobytes())==row['fullAfterSha256'], 'Export differs from the full renderer'
            report['exactFullRendererMatch']=True
            # Exercise the real automatic control and its persisted explanation.
            page.reload()
            page.get_by_role('button',name=f'{name} 5',exact=True).click()
            page.get_by_role('button',name='Auto edit One click, start to finish',exact=True).click()
            page.get_by_role('option',name=photos[0]['fileName'],exact=True).click()
            page.get_by_role('button',name='Retouch',exact=True).click()
            expect(page.get_by_alt_text('Retouched photograph',exact=True)).to_be_visible()
            page.get_by_role('button',name='Auto portrait',exact=True).click()
            expect(page.get_by_role('button',name='Auto portrait',exact=True)).to_be_enabled()
            expect(page.get_by_text('Last automatic pass:',exact=False)).to_be_visible()
            page.screenshot(path=str(output/'auto-portrait-workspace.png'),full_page=True)
            # Refine an automatically authored step using the real controls,
            # then undo it and verify the complete original operation stack.
            photo=photos[0]['id']
            automatic=edits(photo)
            page.get_by_role('button',name=re.compile(r'^1\. Skin smoothing')).click()
            page.get_by_role('button',name='Preview selection mask',exact=True).click()
            expect(page.get_by_alt_text('Selection mask',exact=True)).to_be_visible()
            page.screenshot(path=str(output/'automatic-skin-selection.png'),full_page=True)
            page.get_by_role('button',name='Preview selection mask',exact=True).click()
            strength=page.get_by_label(re.compile(r'^Strength'))
            strength.focus()
            strength.press('Home')
            for _ in range(9): strength.press('ArrowRight')
            page.get_by_role('button',name='Update selected retouch',exact=True).click()
            expect(page.get_by_role('button',name=re.compile(r'^1\. Skin smoothing.*9%$'))).to_be_enabled()
            assert abs(edits(photo)[0]['amount']-.09)<1e-6
            page.get_by_role('button',name='Undo',exact=True).click()
            expect(page.get_by_role('button',name='Auto portrait',exact=True)).to_be_enabled()
            assert edits(photo)==automatic
            report['manualRefinementAndUndo']=True
            report['desktopControls']=True
            assert originals=={p.name:digest(p.read_bytes()) for p in paths}
            report.update(status='passed',originalsUnchanged=True)
        except Exception as error:
            report.update(status='failed',error=str(error))
            raise
        finally:
            (output/'results.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
    print(json.dumps({'status':report['status'],'photos':len(report['photos'])}))


if __name__=='__main__':
    main()
