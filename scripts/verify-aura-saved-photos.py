"""Reopen saved RAW edits in the final desktop, re-render and export every photo."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import time
import urllib.request

from PIL import Image
from playwright.sync_api import sync_playwright
from aura_qualification import exact_import, photo_success, separate_output, write_gallery

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('baseline', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--port', type=int, default=9620)
    args = parser.parse_args()
    baseline = json.loads((args.baseline / 'results.json').read_text(encoding='utf-8'))
    assert baseline.get('completed') and photo_success(baseline), 'Baseline qualification is incomplete'
    output = args.output.resolve()
    separate_output(Path(baseline['source']), output)
    output.mkdir(parents=True, exist_ok=True)
    (output / 'catalog').mkdir()
    with sqlite3.connect(args.baseline / 'catalog/catalog.sqlite') as source:
        with sqlite3.connect(output / 'catalog/catalog.sqlite') as destination:
            source.backup(destination)
    report = dict(source=baseline['source'],inputManifest=baseline['inputManifest'],photos=[],pageErrors=[],
                  commandCounts={},fullExportRequested=True,baseline=str(args.baseline.resolve()),
                  baselineExecutableSha256=baseline['executableSha256'],projectId=baseline['projectId'],
                  executableSha256=digest(ROOT/'target/desktop-launch/debug/aura-desktop.exe'))

    def persist():
        (output/'results.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')

    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    proc = subprocess.Popen([str(ROOT/'target/desktop-launch/debug/aura-desktop.exe')],cwd=ROOT,
        env=dict(os.environ,AURA_TEST_CATALOG=str(output/'catalog/catalog.sqlite'),
                 WEBVIEW2_USER_DATA_FOLDER=str(output/'webview'),AURA_IMAGE_WORKERS='1',
                 WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-port={args.port}'),startupinfo=startup)
    try:
        assert all(digest(Path(p['file']))==p['sha256'] for p in report['inputManifest'])
        for _ in range(120):
            try:
                urllib.request.urlopen(f'http://127.0.0.1:{args.port}/json/version',timeout=1)
                break
            except OSError:
                time.sleep(1)
        else:
            raise TimeoutError('Final desktop did not start')
        with sync_playwright() as pw:
            page = pw.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}').contexts[0].pages[0]
            page.set_default_timeout(600000)
            page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
            page.on('pageerror',lambda error:report['pageErrors'].append(str(error)))

            def call(command,**values):
                report['commandCounts'][command]=report['commandCounts'].get(command,0)+1
                return page.evaluate('''async ([command,input]) => {
                    try{return await window.__TAURI_INTERNALS__.invoke(command,{input})}
                    catch(e){throw Error(JSON.stringify(e))}
                }''',[command,values])

            project=report['projectId']
            photos=call('list_images',projectId=project,offset=0,limit=100,orderBy=None)
            exact_import([Path(p['file']) for p in report['inputManifest']],photos)
            previous={p['file']:p for p in baseline['photos']}
            for photo in photos:
                started=time.monotonic()
                earlier=previous[photo['fileName']]
                item=dict(file=photo['fileName'],photoId=photo['id'])
                report['photos'].append(item)
                try:
                    recipe=call('image_recipe',photoId=photo['id'])
                    assert recipe['recipeHash']==earlier['recipeHash'], 'Saved edit changed on restart'
                    history=call('image_history',photoId=photo['id'])
                    inspection=call('inspect_photo_diagnostics',projectId=project,photoId=photo['id'],recipeHash=recipe['recipeHash'])
                    assert len(inspection['diagnostics'])==95 and inspection['recipeHash']==recipe['recipeHash']
                    frame=call('render_image',photoId=photo['id'],level='screen',screen=[1600,1600],purpose='export')
                    pixels=base64.b64decode(frame['rgbBase64'])
                    pixel_hash=hashlib.sha256(pixels).hexdigest()
                    assert pixel_hash==earlier['after']['pixelSha256'], 'Final build changed saved-edit pixels'
                    assert call('image_history',photoId=photo['id'])==history
                    before=output/(photo['fileName']+'-before.jpg')
                    shutil.copyfile(args.baseline/before.name,before)
                    after=output/(photo['fileName']+'-after.jpg')
                    Image.frombytes('RGB',(frame['width'],frame['height']),pixels).save(after,quality=95)
                    (output/(photo['fileName']+'.recipe.json')).write_text(recipe['body'],encoding='utf-8')
                    (output/(photo['fileName']+'.inspection.json')).write_text(json.dumps(inspection,indent=2),encoding='utf-8')
                    item.update(passed=True,recipeHash=recipe['recipeHash'],samePixelsAfterRestart=True,
                                inspectionCount=95,changed=earlier['changed'],
                                before=dict(earlier['before'],file=str(before),fileSha256=digest(before)),
                                after=dict(width=frame['width'],height=frame['height'],pixelSha256=pixel_hash,
                                           file=str(after),fileSha256=digest(after)))
                    print('PASS',photo['fileName'],'final build saved-edit pixels',flush=True)
                except Exception as error:
                    item.update(passed=False,error=str(error))
                    print('FAIL',photo['fileName'],str(error),flush=True)
                item['seconds']=round(time.monotonic()-started,3)
                persist()
            ids=[p['photoId'] for p in report['photos'] if p.get('passed')]
            report['export']=call('export_run',projectId=project,destination=str(output/'full-resolution'),
                destinationKind='folder',copyright=None,contact=None,creator=None,keywords=[],stripGps=True,
                stripCameraSerial=True,verify=True,sets=[dict(name='edited',imageIds=ids,format='jpeg',quality=95,
                colour='srgb',bitDepth=8,resize='full',sharpen='none',naming='{original}',sidecar=True)])
            report['exportFiles']=[]
            for path in sorted((output/'full-resolution').rglob('*.jpg')):
                with Image.open(path) as image:
                    image.verify()
                with Image.open(path) as image:
                    report['exportFiles'].append(dict(file=str(path),width=image.width,height=image.height,sha256=digest(path)))
            sidecars=sorted((output/'full-resolution').rglob('*.xmp'))
            expected={Path(item['file']).stem:item for item in report['photos'] if item.get('passed')}
            assert report['export']['sidecars']==len(report['inputManifest'])==len(sidecars)
            assert {path.stem for path in sidecars}==set(expected), 'Incomplete or duplicate sidecar inventory'
            report['sidecarFiles']=[]
            for path in sidecars:
                recipe_path=output/(expected[path.stem]['file']+'.recipe.json')
                assert path.read_text(encoding='utf-8')==recipe_path.read_text(encoding='utf-8'), 'Sidecar differs from rendered recipe'
                report['sidecarFiles'].append(dict(file=str(path),sha256=digest(path),matchesRecipe=True))
            report['sidecarsVerified']=True
            report['originalsUnchanged']=all(digest(Path(p['file']))==p['sha256'] for p in report['inputManifest'])
            report['completed']=photo_success(report)
            write_gallery(output,report)
            persist()
            if not report['completed']:
                raise RuntimeError('Final desktop photo verification failed')
    except Exception as error:
        report['fatalError']=str(error)
        raise
    finally:
        persist()
        proc.terminate()
        proc.wait(timeout=30)


if __name__=='__main__':
    main()
