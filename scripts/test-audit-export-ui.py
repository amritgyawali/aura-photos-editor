"""Finish the audited portrait with the actual desktop export controls."""
import base64
import hashlib
import json
import time
from pathlib import Path
from PIL import Image
from playwright.sync_api import sync_playwright,expect

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'.work-checks/full-audit'
gui=json.loads((OUT/'gui/results.json').read_text())
report={}
with sync_playwright() as p:
    page=p.chromium.connect_over_cdp('http://127.0.0.1:9223').contexts[0].pages[0]
    page.set_default_timeout(90000);expect.set_options(timeout=90000)
    page.get_by_test_id('preset').select_option('gallery')
    destination=OUT/'gui/export'
    field=page.get_by_test_id('destination')
    field.click();page.keyboard.press('Control+a');page.keyboard.type(str(destination))
    page.get_by_test_id('verify').check()
    page.get_by_test_id('preview-names').click()
    expect(page.get_by_test_id('names')).to_be_visible()
    page.screenshot(path=str(OUT/'gui/12-export-names.png'))
    page.get_by_test_id('run').click()
    expect(page.get_by_test_id('written')).to_have_text('1')
    expect(page.get_by_test_id('verified')).to_have_text('1')
    page.screenshot(path=str(OUT/'gui/13-export-complete.png'))
    report['guiExportVerified']=True
    report['files']=[]
    for f in destination.rglob('*.jpg'):
        with Image.open(f) as image:
            report['files'].append({'file':str(f.relative_to(OUT)), 'size':list(image.size),'format':image.format})
    def call(cmd,**args):
        return page.evaluate('async ([c,a])=>await window.__TAURI_INTERNALS__.invoke(c,{input:a})',[cmd,args])
    report['recipeHash']=call('image_recipe',photoId=gui['photoId'])['recipeHash']
    r=call('render_image',photoId=gui['photoId'],level='full',purpose='export')
    data=base64.b64decode(r['rgbBase64'])
    report['pixelHash']=hashlib.sha256(data).hexdigest()
    Image.frombytes('RGB',(r['width'],r['height']),data).save(OUT/'gui/final.png')
    isolated=json.loads((OUT/'isolated-results.json').read_text())
    photo=next(x['result']['photoId'] for x in isolated['cases'] if x['name']=='single-file:portrait-8386841.jpg')
    report['retriedPortraitAuto']=json.loads(call('enhance_photo',photoId=photo)['body'])['studio_portrait_auto_v1']
    (OUT/'gui/export-results.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
    print(json.dumps(report),flush=True)
