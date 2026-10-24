"""Exercise the real reference workspace in a disposable desktop catalog.

Requires Pillow and Playwright. Pass an edited reference and two different target
photos. This checks workflow integrity, not proprietary algorithm equivalence.
No existing AURA process or catalog is touched.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time
import urllib.request
import uuid

from PIL import Image, ImageOps
from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('reference', type=Path)
    parser.add_argument('targets', type=Path, nargs=2)
    parser.add_argument('--exe', type=Path, default=ROOT / 'target/desktop-launch/debug/aura-desktop.exe')
    parser.add_argument('--port', type=int, default=9377)
    parser.add_argument('--intent', choices=['natural', 'palette'], default='natural')
    args = parser.parse_args()
    out = ROOT / '.work-checks' / ('reference-workspace-' + uuid.uuid4().hex[:8])
    out.mkdir(parents=True)
    sources = []
    fingerprints = []
    for index, source in enumerate([args.reference, *args.targets]):
        with Image.open(source) as image:
            image = ImageOps.exif_transpose(image).convert('RGB')
            normalized = hashlib.sha256(image.resize((32, 32)).tobytes()).hexdigest()
            gray = list(image.convert('L').resize((9, 8)).tobytes())
            dhash = sum(int(gray[row * 9 + column + 1] > gray[row * 9 + column]) << (row * 8 + column) for row in range(8) for column in range(8))
            assert all(normalized != previous['pixelHash'] and (dhash ^ previous['dHash']).bit_count() > 6 for previous in fingerprints), 'The smoke test requires different photos, not duplicate or before/after versions of the same frame.'
            fingerprints.append({'path': str(source.resolve()), 'pixelHash': normalized, 'dHash': dhash})
            image.thumbnail((640, 640))
            path = out / ('reference.png' if index == 0 else f'target-{index}.png')
            image.save(path)
            sources.append(path)
    hashes = {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in sources}
    env = dict(os.environ, AURA_TEST_CATALOG=str(out / 'catalog.sqlite'),
               WEBVIEW2_USER_DATA_FOLDER=str(out / 'webview'),
               WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-port={args.port}')
    process = subprocess.Popen([str(args.exe.resolve())], cwd=ROOT, env=env)
    report = {'checks': {}, 'pageErrors': [], 'fixtureHashes': hashes, 'uniqueInputs': fingerprints, 'intent': args.intent}
    try:
        for _ in range(120):
            try:
                urllib.request.urlopen(f'http://127.0.0.1:{args.port}/json/version', timeout=1)
                break
            except OSError:
                time.sleep(1)
        with sync_playwright() as pw:
            browser = pw.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}')
            page = next(p for c in browser.contexts for p in c.pages)
            page.set_viewport_size({'width': 1920, 'height': 1080})
            page.set_default_timeout(180000)
            expect.set_options(timeout=180000)
            page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
            page.on('pageerror', lambda error: report['pageErrors'].append(str(error)))

            def call(command, **kwargs):
                return page.evaluate('async ([c,a])=>window.__TAURI_INTERNALS__.invoke(c,{input:a})', [command, kwargs])

            def status(photo):
                return page.evaluate("photoId=>window.__TAURI_INTERNALS__.invoke('color_match_status',{photoId})", photo)

            name = 'Reference workspace ' + uuid.uuid4().hex[:6]
            project = call('create_project', name=name, coupleNames=None, eventDate=None)['id']
            call('start_ingest', projectId=project, roots=[str(path) for path in sources[1:]])
            photos = []
            for _ in range(120):
                photos = call('list_images', projectId=project, offset=0, limit=10, orderBy='timeline')
                if len(photos) == 2:
                    break
                time.sleep(.5)
            assert len(photos) == 2
            reference = call('import_color_reference', path=str(sources[0]), cancelId=str(uuid.uuid4()))
            assert reference.get('preview', '').startswith('data:image/jpeg;base64,')
            page.reload()
            page.get_by_role('button', name=f'{name} 2', exact=True).click()
            page.get_by_role('button', name='Match a look Your reference, your photos', exact=True).click()
            panel = page.get_by_role('region', name='Reference colour matching', exact=True)
            expect(panel).to_be_visible()
            expect(page.get_by_alt_text('Colour reference: ' + reference['name'], exact=True)).to_be_visible()
            expect(panel.get_by_role('checkbox', name='Real-Time Color Adjustments')).not_to_be_checked()
            panel.get_by_label('Matching goal', exact=True).select_option(args.intent)
            page.evaluate('''()=>{
                const probe={gaps:[],last:performance.now(),visible:document.visibilityState==='visible'};
                const tick=t=>{const visible=document.visibilityState==='visible';
                    if(visible&&probe.visible&&probe.gaps.length<18000)probe.gaps.push(Math.max(0,t-probe.last));
                    probe.last=t;probe.visible=visible;probe.frame=requestAnimationFrame(tick)};
                probe.frame=requestAnimationFrame(tick);window.__auraFrameProbe=probe;
            }''')
            panel.get_by_label('Matching mode', exact=True).select_option('quick')
            panel.get_by_role('button', name='Analyse reference match', exact=True).click()
            apply = panel.get_by_role('button', name='Apply colour strengths', exact=True)
            expect(apply).to_be_enabled()
            apply.click()
            expect(apply).to_be_enabled()
            active_name = page.get_by_role('listbox', name='Filmstrip').get_by_role('option', selected=True).get_attribute('aria-label')
            photo = next(item['id'] for item in photos if item['fileName'] == active_name)

            def wait_status(predicate):
                deadline = time.monotonic() + 180
                while time.monotonic() < deadline:
                    value = status(photo)
                    if value and predicate(value):
                        return value
                    time.sleep(.3)
                raise AssertionError('Timed out waiting for saved colour match')

            saved = wait_status(lambda value: value['fullResolutionChecked'])
            assert saved['fitting']['quality'] == 'quick'
            assert saved['fitting'].get('intent', 'natural') == args.intent
            assert saved['fitting']['targetSamples'] <= 10000 and saved['fitting']['maxIterations'] == 2
            report['checks']['quickNativeApply'] = True
            print('Quick fitting and native apply passed', flush=True)
            panel.get_by_role('checkbox', name='Real-Time Color Adjustments').check()
            panel.get_by_role('slider', name='Match amount', exact=True).evaluate("el => { Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(el, '75'); el.dispatchEvent(new Event('input', {bubbles:true})); }")
            wait_status(lambda value: abs(value['controls']['amount'] - .75) < .001)
            report['checks']['liveStrengthSaved'] = True
            expect(panel.get_by_role('button', name='Save Preset', exact=True)).to_be_enabled()
            panel.get_by_role('button', name='Save Preset', exact=True).click()
            expect(panel.get_by_role('button', name='Save Preset', exact=True)).to_be_enabled()
            refs = page.evaluate("()=>window.__TAURI_INTERNALS__.invoke('list_color_references')")
            assert next(item for item in refs if item['id'] == reference['id'])['defaultControls']['amount'] == .75
            report['checks']['presetSaved'] = True
            print('Live strength and persisted preset passed', flush=True)
            panel.get_by_label('Matching mode', exact=True).select_option('precise')
            saved = wait_status(lambda value: value['fitting']['quality'] == 'precise' and value['fullResolutionChecked'])
            assert saved['fitting']['targetSamples'] <= 40000 and saved['fitting']['maxIterations'] == 4
            report['checks']['preciseLiveRefit'] = saved['fullResolutionChecked']
            print('Precise live refit and native apply passed', flush=True)
            expect(panel.get_by_role('checkbox', name='Real-Time Color Adjustments')).to_be_enabled()
            panel.get_by_role('checkbox', name='Real-Time Color Adjustments').uncheck()
            panel.get_by_label('Sync destination', exact=True).select_option('collection')
            panel.get_by_role('button', name='Sync', exact=True).click()
            expect(panel.get_by_role('button', name='Sync', exact=True)).to_be_enabled()
            for item in photos:
                result = status(item['id'])
                assert result['current'] and result['fullResolutionChecked']
                assert result['reference']['id'] == reference['id']
                assert result['fitting'].get('intent', 'natural') == args.intent
            report['checks']['collectionNativeRefits'] = True
            frame_probe = page.evaluate('''()=>{const p=window.__auraFrameProbe;
                cancelAnimationFrame(p.frame);return {gaps:p.gaps,visibility:document.visibilityState}}''')
            gaps = sorted(frame_probe['gaps'])
            report['responsiveness'] = {'visibleFrameSamples': len(gaps), 'visibilityAtEnd': frame_probe['visibility'],
                'medianGapMilliseconds': gaps[len(gaps) // 2] if gaps else None,
                'p95GapMilliseconds': gaps[min(len(gaps) - 1, int(len(gaps) * .95))] if gaps else None,
                'maximumGapMilliseconds': max(gaps) if gaps else None}
            page.get_by_label('Comparison zoom', exact=True).select_option('1.5')
            page.locator('.reference-comparison').scroll_into_view_if_needed()
            page.screenshot(path=str(out / 'workspace.png'))
            report['checks']['originalsUnchanged'] = all(hashlib.sha256(Path(path).read_bytes()).hexdigest() == digest for path, digest in hashes.items())
            assert report['checks']['originalsUnchanged'] and not report['pageErrors']
            report['match'] = saved
            print(json.dumps({'output': str(out), 'checks': report['checks']}, indent=2), flush=True)
    except BaseException as error:
        report['failure'] = f'{type(error).__name__}: {error}'
        raise
    finally:
        (out / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
        process.terminate()
        try:
            process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            process.kill()


if __name__ == '__main__':
    main()
