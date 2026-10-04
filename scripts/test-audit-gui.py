"""Edit a real photograph through AURA's actual WebView2 mouse/keyboard controls.

Setup/import and verification use native IPC; all steps labelled GUI use visible
controls. Does not claim to automate the Windows file picker or physical devices.
"""
import base64
import hashlib
import json
import re
import time
from pathlib import Path
from PIL import Image
from playwright.sync_api import sync_playwright, expect

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.work-checks/full-audit/gui'
OUT.mkdir(parents=True, exist_ok=True)
report = {'steps': [], 'pageErrors': []}

with sync_playwright() as pw:
    browser = pw.chromium.connect_over_cdp('http://127.0.0.1:9223')
    page = browser.contexts[0].pages[0]
    page.set_default_timeout(90000)
    expect.set_options(timeout=90000)
    page.on('pageerror', lambda e: report['pageErrors'].append(str(e)))

    def call(cmd, **args):
        return page.evaluate('async ([cmd,args])=>await window.__TAURI_INTERNALS__.invoke(cmd,{input:args})', [cmd,args])

    def persist():
        (OUT/'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')

    def step(label, fn):
        started=time.monotonic()
        try:
            value=fn()
            record=dict(label=label,status='passed',result=value)
        except Exception as e:
            record=dict(label=label,status='failed',error=str(e))
        record['seconds']=round(time.monotonic()-started,2)
        report['steps'].append(record)
        persist()
        page.screenshot(path=str(OUT/f'{len(report["steps"]):02d}.png'))
        print(label, record['status'], str(record.get('error',''))[:180], flush=True)
        return record['status']=='passed'

    def ready():
        expect(page.get_by_role('button',name='Retouch',exact=True)).to_be_enabled()

    def param(path):
        r=call('image_recipe',photoId=photo)
        return next((p['value'] for p in r['params'] if p['path']==path),None)

    def wait_param(path, expected):
        deadline=time.monotonic()+90
        while not (isinstance(param(path),(int,float)) and abs(param(path)-expected)<1e-5):
            if time.monotonic()>deadline: raise AssertionError(f'{path}: {param(path)} != {expected}')
            time.sleep(.3)
        ready()

    def number(label,value,path):
        box=page.get_by_role('spinbutton',name=label+' value',exact=True)
        box.click()
        page.keyboard.press('Control+a')
        page.keyboard.type(str(value))
        page.keyboard.press('Enter')
        wait_param(path,value)

    def capture(name):
        r=call('render_image',photoId=photo,level='full',purpose='export')
        image=Image.frombytes('RGB',(r['width'],r['height']),base64.b64decode(r['rgbBase64']))
        image.save(OUT/(name+'.png'))
        return hashlib.sha256(image.tobytes()).hexdigest()

    name='Mouse and keyboard audit '+time.strftime('%H-%M-%S')
    project=call('create_project',name=name,coupleNames=None,eventDate=None)['id']
    source=ROOT/'.work-checks/full-audit/inputs/portrait-1239291.jpg'
    call('start_ingest',projectId=project,roots=[str(source)])
    deadline=time.monotonic()+90
    photos=[]
    while not photos and time.monotonic()<deadline:
        photos=call('list_images',projectId=project,offset=0,limit=10,orderBy='timeline')
        time.sleep(.3)
    photo=photos[0]['id']
    report.update(projectId=project,photoId=photo,collectionName=name,sourceHash=hashlib.sha256(source.read_bytes()).hexdigest())
    page.reload()
    page.get_by_role('button',name=name+' 1',exact=True).click()
    page.get_by_role('button',name='Auto edit One click, start to finish',exact=True).click()
    page.get_by_role('option',name=source.name,exact=True).click()
    ready()
    report['beforeHash']=capture('before')

    def automatic():
        page.get_by_role('button',name='Auto enhance photo',exact=True).click()
        ready()
        expect(page.get_by_text('Last automatic pass:',exact=False)).to_be_visible()
        return json.loads(call('image_recipe',photoId=photo)['body'])['studio_portrait_auto_v1']
    step('GUI: auto enhance portrait',automatic)
    step('GUI: keyboard exposure +0.35 EV',lambda:number('Exposure',.35,'global.exposure'))
    step('GUI: keyboard white balance 6000 K',lambda:number('Temp',6000,'global.temperature'))
    page.get_by_role('button',name='Advanced',exact=True).click()

    def curve():
        page.get_by_text('Tone Curve',exact=True).click()
        graph=page.get_by_role('img',name='Tone curve',exact=True)
        graph.scroll_into_view_if_needed()
        box=graph.bounding_box()
        page.mouse.click(box['x']+box['width']*.5,box['y']+box['height']*.4)
        ready()
        values=param('global.curve.points')
        assert len(values)>2,values
        return values
    step('GUI: add tone-curve point with mouse',curve)

    def hsl():
        page.get_by_text('Color Mixer',exact=True).click()
        page.get_by_role('tab',name='Saturation',exact=True).click()
        number('Orange',-12,'global.hsl.orange.s')
    step('GUI: selective orange saturation',hsl)

    def crop():
        page.get_by_text('Transform & Crop',exact=True).click()
        page.get_by_role('button',name='4:5',exact=True).click()
        ready()
        values=param('geometry.crop')
        assert values!=[0,0,1,1]
        return values
    step('GUI: crop to 4:5',crop)

    def snapshot():
        page.get_by_text('Named snapshots (0)',exact=True).click()
        field=page.get_by_label('Snapshot name',exact=True)
        field.fill('Audit natural portrait')
        field.press('Enter')
        expect(page.get_by_role('button',name='Restore snapshot Audit natural portrait',exact=True)).to_be_enabled()
    step('GUI: save named snapshot with Enter',snapshot)

    def compare():
        page.get_by_role('button',name='Compare',exact=True).click()
        divider=page.get_by_label('Before and after divider',exact=True)
        divider.focus()
        divider.press('Home')
        for _ in range(35): divider.press('ArrowRight')
        assert divider.input_value()=='35'
        page.get_by_label('Clipping warnings',exact=False).select_option('both')
    step('GUI: before/after divider and clipping overlay',compare)

    page.get_by_role('button',name='Retouch',exact=True).click()
    expect(page.get_by_alt_text('Retouched photograph',exact=True)).to_be_visible()
    surface=page.get_by_role('group',name='Retouch image interaction',exact=True)

    def brush():
        page.get_by_label('Tool',exact=True).select_option('dodge')
        surface.scroll_into_view_if_needed()
        surface.focus()
        page.keyboard.press('b')
        expect(page.get_by_role('button',name='Brush (B)',exact=True)).to_have_attribute('aria-pressed','true')
        page.keyboard.press(']')
        box=surface.bounding_box()
        page.mouse.move(box['x']+box['width']*.43,box['y']+box['height']*.56)
        page.mouse.down()
        page.mouse.move(box['x']+box['width']*.49,box['y']+box['height']*.58,steps=14)
        page.mouse.up()
        expect(page.get_by_role('button',name='Undo brush stroke',exact=True)).to_be_enabled()
        surface.focus()
        page.keyboard.press('e')
        expect(page.get_by_role('button',name='Eraser (E)',exact=True)).to_have_attribute('aria-pressed','true')
        page.mouse.click(box['x']+box['width']*.43,box['y']+box['height']*.56)
        surface.focus()
        page.keyboard.press('Control+z')
        page.keyboard.press('b')
        page.keyboard.press('Enter')
        expect(page.get_by_role('button',name=re.compile(r'^4\. Dodge'))).to_be_enabled()
        return len(call('native_retouch_edit',projectId=project,photoId=photo,action='list',edits=[],id=None))
    step('GUI: paint, erase, undo stroke, apply with Enter',brush)

    def viewport():
        surface.scroll_into_view_if_needed()
        surface.focus()
        page.keyboard.press('+')
        page.keyboard.press('ArrowRight')
        page.keyboard.press('ArrowDown')
        page.keyboard.press('h')
        expect(page.get_by_role('button',name='Hand (H)',exact=True)).to_have_attribute('aria-pressed','true')
        page.keyboard.press('0')
        page.get_by_role('button',name='Split comparison',exact=True).click()
        expect(page.get_by_alt_text('Before native retouch comparison',exact=True)).to_be_visible()
        page.get_by_label('Before/after split',exact=True).press('Home')
        page.get_by_role('button',name='Center divider',exact=True).click()
        page.get_by_role('button',name='Split comparison',exact=True).click()
    step('GUI: zoom, pan, fit and retouch split',viewport)

    def history():
        before=call('image_recipe',photoId=photo)['recipeHash']
        surface.focus()
        page.keyboard.press('Control+z')
        expect(page.get_by_role('button',name='Redo',exact=True)).to_be_enabled()
        assert call('image_recipe',photoId=photo)['recipeHash']!=before
        surface.focus()
        page.keyboard.press('Control+Shift+z')
        expect(page.get_by_role('button',name=re.compile(r'^4\. Dodge'))).to_be_enabled()
        assert call('image_recipe',photoId=photo)['recipeHash']==before
    step('GUI: keyboard Undo and Redo saved edit',history)
    report['afterHash']=capture('after')
    report['recipe']=call('image_recipe',photoId=photo)
    report['history']=call('image_history',photoId=photo)
    report['sourceUnchanged']=hashlib.sha256(source.read_bytes()).hexdigest()==report['sourceHash']
    persist()
