"""Edit the explicitly supplied RAW folder through an isolated AURA desktop; preserve inputs."""
import argparse
import base64
import hashlib
import json
import os
import shutil
from pathlib import Path
import subprocess
import time
import urllib.request
import uuid
from PIL import Image
from playwright.sync_api import sync_playwright
from aura_qualification import exact_import, photo_success, separate_output, write_gallery

ROOT = Path(__file__).resolve().parents[1]

def digest(path):
    hasher = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            hasher.update(chunk)
    return hasher.hexdigest()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--port', type=int, default=9383)
    parser.add_argument('--full-export', action='store_true')
    parser.add_argument('--next100', action='store_true', help='Qualify a subtle new lower-tail tone layer and all45 native inspection measurements')
    parser.add_argument('--additions20261022', action='store_true', help='Qualify the new stop-zone exposure layer and all70 native inspection measurements')
    parser.add_argument('--preview-reference', type=Path, help='Optional watermarked screenshot reference for a separately labelled first-photo variant')
    parser.add_argument('--lab-tool', help='Explicit colour method for the gentle qualification layer')
    parser.add_argument('--expected-diagnostics', type=int, help='Expected current native inspection count')
    args = parser.parse_args()
    output = args.output.resolve()
    separate_output(args.source, output)
    output.mkdir(parents=True, exist_ok=True)
    sources = sorted(p for p in args.source.resolve().iterdir() if p.is_file() and p.suffix.lower() in {'.cr3','.cr2','.nef','.arw','.raf','.dng','.jpg','.jpeg','.png','.tif','.tiff'})
    assert sources, 'No supported photo inputs'
    report = {'source': str(args.source.resolve()), 'photos': [], 'pageErrors': [], 'commandCounts': {}, 'fullExportRequested': args.full_export, 'next100Requested': args.next100, 'additions20261022Requested':args.additions20261022, 'labTool':args.lab_tool, 'expectedDiagnostics':args.expected_diagnostics}
    report['inputManifest'] = [{'file': str(p), 'bytes': p.stat().st_size, 'sha256': digest(p)} for p in sources]
    (output / 'catalog').mkdir(exist_ok=True)
    env = dict(os.environ, AURA_TEST_CATALOG=str(output/'catalog/catalog.sqlite'), WEBVIEW2_USER_DATA_FOLDER=str(output/'webview'), WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-port={args.port}', AURA_IMAGE_WORKERS='1')
    startup = subprocess.STARTUPINFO(); startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW; startup.wShowWindow = 0
    executable = ROOT/'target/desktop-launch/debug/aura-desktop.exe'
    report['executableSha256'] = digest(executable)
    proc = subprocess.Popen([str(executable)], cwd=ROOT, env=env, startupinfo=startup)
    def persist():
        (output/'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    persist()
    try:
        for _ in range(120):
            try:
                urllib.request.urlopen(f'http://127.0.0.1:{args.port}/json/version', timeout=1); break
            except OSError: time.sleep(1)
        else: raise RuntimeError('AURA did not start')
        with sync_playwright() as pw:
            page = pw.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}').contexts[0].pages[0]
            page.set_default_timeout(600000)
            page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
            page.on('pageerror', lambda e: report['pageErrors'].append(str(e)))
            def call(command, **values):
                report['commandCounts'][command] = report['commandCounts'].get(command, 0) + 1
                return page.evaluate('async ([command,input]) => {try{return await window.__TAURI_INTERNALS__.invoke(command,{input})}catch(error){throw new Error(JSON.stringify(error))}}', [command,values])
            project = call('create_project', name='AURA 100 photo qualification '+time.strftime('%H%M%S'), coupleNames=None, eventDate=None)['id']
            report['projectId'] = project; persist()
            handle = call('start_ingest', projectId=project, roots=[str(args.source.resolve())])
            report['ingest'] = handle
            deadline = time.monotonic()+600
            while True:
                photos = call('list_images', projectId=project, offset=0, limit=100, orderBy=None)
                if len(photos) == len(sources): break
                if time.monotonic()>deadline: raise TimeoutError(f'Imported {len(photos)}/{len(sources)}')
                time.sleep(1)
            exact_import(sources, photos)
            for photo in photos:
                started = time.monotonic(); item = {'file': photo['fileName'], 'photoId': photo['id']}
                report['photos'].append(item); persist()
                try:
                    for stage in ['before','after']:
                        if stage == 'after':
                            edited = call('enhance_photo', photoId=photo['id'])
                            if args.next100 or args.additions20261022:
                                auto_body=json.loads(edited['body'])
                                masks=auto_body.get('studio_masks_v1', [])
                                mask_id='next100-tone-finish'
                                if any(mask['id']==mask_id for mask in masks):
                                    raise RuntimeError('Qualification mask identity is already in use')
                                masks.append(dict(id=mask_id,name='Gentle selected colour finish' if args.lab_tool else 'Gentle stop-zone lift' if args.additions20261022 else 'Gentle lower-tail tone finish',enabled=True,amount=1,
                                                  components=[dict(mode='add',source=dict(type='radial',centre=[.5,.5],radii=[1,1],angle=0,feather=0))],params={}))
                                selected=call('save_local_masks',projectId=project,photoId=photo['id'],masks=masks,label='New tone method qualification')['recipe']
                                operation=dict(id='next100-lower-tail',tool='lower_tail_lift',maskId=mask_id,enabled=True,
                                               amount=.3,strength=.6,radius=.025,pivot=.04,width=.5,source=[.25,.5],second=[.75,.5],target=[.3,.2,.1])
                                if args.additions20261022:
                                    operation.update(id='delivery-zone-lift',tool='zone_exposure',amount=.25,strength=.6,pivot=.18,width=.3)
                                if args.lab_tool:
                                    operation.update(id='advanced-tone-finish',tool=args.lab_tool,amount=.08,strength=.4)
                                edited=call('save_color_lab',projectId=project,photoId=photo['id'],recipeHash=selected['recipeHash'],operations=[operation])['recipe']
                                inspection=call('inspect_photo_diagnostics',projectId=project,photoId=photo['id'],recipeHash=edited['recipeHash'])
                                expected=args.expected_diagnostics or (70 if args.additions20261022 else 45)
                                if len(inspection['diagnostics'])!=expected or inspection['recipeHash']!=edited['recipeHash']:
                                    raise RuntimeError('Incomplete or stale native inspection report')
                                (output/(photo['fileName']+'.inspection.json')).write_text(json.dumps(inspection,indent=2),encoding='utf-8')
                                item['next100']=dict(tool=operation['tool'],operation=operation,inspectionCount=expected,
                                                     provenance='Independent AURA auto edit plus a gentle explicit selected colour method; not verified Evoto matching.')
                            body=json.loads(edited['body']); item['recipeHash']=edited['recipeHash']
                            (output/(photo['fileName']+'.recipe.json')).write_text(edited['body'],encoding='utf-8')
                            item['automatic']=body.get('studio_portrait_auto_v1'); item['recipeSections']=sorted(body)
                        rendered=call('render_image',photoId=photo['id'],level='screen',screen=[1600,1600],purpose='export')
                        pixels=base64.b64decode(rendered['rgbBase64'])
                        path=output/(photo['fileName']+f'-{stage}.jpg')
                        Image.frombytes('RGB',(rendered['width'],rendered['height']),pixels).save(path,quality=95)
                        item[stage]={'width':rendered['width'],'height':rendered['height'],'pixelSha256':hashlib.sha256(pixels).hexdigest(),'file':str(path),'fileSha256':digest(path)}
                    item['changed']=item['before']['pixelSha256']!=item['after']['pixelSha256']
                    item['passed']=True
                    print('PASS',photo['fileName'],round(time.monotonic()-started,1),'seconds',flush=True)
                except Exception as error:
                    item['passed']=False; item['error']=str(error); print('FAIL',photo['fileName'],str(error)[:200],flush=True)
                item['seconds']=round(time.monotonic()-started,3); persist()
            if args.full_export:
                successful=[item['photoId'] for item in report['photos'] if item.get('passed')]
                report['export']=call('export_run',projectId=project,destination=str(output/'full-resolution'),destinationKind='folder',copyright=None,contact=None,creator=None,keywords=[],stripGps=True,stripCameraSerial=True,verify=True,sets=[dict(name='edited',imageIds=successful,format='jpeg',quality=95,colour='srgb',bitDepth=8,resize='full',sharpen='none',naming='{original}',sidecar=True)])
                report['exportFiles']=[]
                for path in sorted((output/'full-resolution').rglob('*.jpg')):
                    with Image.open(path) as img:
                        img.verify()
                    with Image.open(path) as img:
                        report['exportFiles'].append({'file':str(path),'width':img.width,'height':img.height,'sha256':digest(path)})
                persist()
            if args.preview_reference:
                selected=next(item for item in report['photos'] if item['file']=='989A2698.CR3' and item.get('passed'))
                image_id=selected['photoId']
                baseline=call('image_recipe',photoId=image_id)
                reference=call('import_color_reference',path=str(args.preview_reference.resolve()),cancelId=str(uuid.uuid4()))
                session=call('prepare_color_match',photoId=image_id,referenceId=reference['id'],cancelId=str(uuid.uuid4()))
                attempts=[]
                match=None
                for amount in [.5,.25,.1]:
                    controls=dict(amount=amount,tone=.6,color=.6,whiteBalance=.5)
                    try:
                        match=call('apply_color_match',photoId=image_id,sessionId=session['sessionId'],controls=controls,cancelId=str(uuid.uuid4()))
                        attempts.append(dict(controls=controls,passed=True))
                        break
                    except Exception as error:
                        attempts.append(dict(controls=controls,passed=False,error=str(error)))
                        assert call('image_recipe',photoId=image_id)['recipeHash']==baseline['recipeHash'], 'Refused matching attempt changed the saved edit'
                if match is None:
                    raise RuntimeError('Native reference matching refused all conservative strengths: '+json.dumps(attempts))
                variant=call('image_recipe',photoId=image_id)
                folder=output/'screenshot-guided-variant'; folder.mkdir(exist_ok=True)
                rendered=call('render_image',photoId=image_id,level='screen',screen=[1600,1600],purpose='export')
                Image.frombytes('RGB',(rendered['width'],rendered['height']),base64.b64decode(rendered['rgbBase64'])).save(folder/'989A2698-preview.jpg',quality=95)
                shutil.copyfile(args.preview_reference,folder/'reference.png')
                (folder/'index.html').write_text('<!doctype html><meta charset="utf-8"><title>AURA screenshot-guided comparison</title>'
                    '<style>body{background:#17191e;color:#eee;font:16px system-ui;margin:24px}.photos{display:flex;gap:16px}figure{flex:1;margin:0}img{width:100%;max-height:80vh;object-fit:contain}figcaption{padding:12px}</style>'
                    '<h1>Screenshot-guided colour approximation</h1><p>The reference is an earlier watermarked Evoto preview. Display colour transforms and native export fidelity are unknown. No equivalence or perfection score is claimed.</p>'
                    '<div class="photos"><figure><img src="../989A2698.CR3-after.jpg"><figcaption>Primary AURA edit</figcaption></figure>'
                    '<figure><img src="989A2698-preview.jpg"><figcaption>Separate AURA reference-fit variant</figcaption></figure>'
                    '<figure><img src="reference.png"><figcaption>Observed Evoto preview; watermark preserved</figcaption></figure></div>',encoding='utf-8')
                (folder/'recipe.json').write_text(variant['body'],encoding='utf-8')
                report['screenshotGuidedVariant']=dict(source=selected['file'],referencePath=str(args.preview_reference.resolve()),referenceSha256=digest(args.preview_reference),match=match,attempts=attempts,
                    recipeHash=variant['recipeHash'],provenance='Approximation guided by the earlier displayed watermarked Evoto preview. No Evoto export or pixel-fidelity score is available.')
                if args.full_export:
                    report['screenshotGuidedVariant']['export']=call('export_run',projectId=project,destination=str(folder/'full-resolution'),destinationKind='folder',copyright=None,contact=None,creator=None,keywords=[],stripGps=True,stripCameraSerial=True,verify=True,sets=[dict(name='approximation',imageIds=[image_id],format='jpeg',quality=95,colour='srgb',bitDepth=8,resize='full',sharpen='none',naming='{original}',sidecar=True)])
                    variant_export=report['screenshotGuidedVariant']['export']
                    assert variant_export['requested']==variant_export['written']==variant_export['verified']==1
                    assert variant_export['manifestSealed'] and all(variant_export[key]==0 for key in ['unverified','corrupt','renderFailed'])
                if variant['recipeHash']!=baseline['recipeHash']:
                    call('history_step',projectId=project,photoId=image_id,action='undo')
                assert call('image_recipe',photoId=image_id)['recipeHash']==baseline['recipeHash'], 'Variant did not restore qualified edit'
                report['screenshotGuidedVariant']['baselineRestored']=True
                persist()
            report['originalsUnchanged']=all(digest(Path(item['file']))==item['sha256'] for item in report['inputManifest'])
            report['completed']=photo_success(report)
            write_gallery(output, report)
            if not report['completed']:
                raise RuntimeError('Photo qualification failed; see individual edit/export evidence')
    except Exception as error:
        report['fatalError']=str(error); raise
    finally:
        persist(); proc.terminate(); proc.wait(timeout=30)

if __name__ == '__main__': main()
