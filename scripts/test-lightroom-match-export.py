"""Test reference replacement and standalone XMP export on 20 separate photos.

Requires a completed test-color-match-twenty.py baseline, numpy, Pillow and
Playwright. Uses a SQLite backup and its own desktop process. The independent
decoded-table comparison measures AURA portability, not Lightroom rendering.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import html
import importlib.util
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import time
import urllib.request
import uuid

from PIL import Image
from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parents[1]


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'scripts' / filename)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


assessment = module('colour_assessment', 'test-color-match-twenty.py')
validator = module('preset_validator', 'validate-lightroom-preset.py')


def write(out, report):
    (out / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    rows = []
    for row in report['photos']:
        stem = row['stem']
        images = ''.join(f'<figure><img src="{stem}.{suffix}.png"><figcaption>{label}</figcaption></figure>'
                         for suffix, label in [('before', 'Native baseline'), ('matched', 'AURA saved match'), ('table', 'Decoded XMP colour table')])
        rows.append(f'<article><h2>{row["targetIndex"]:02}: {html.escape(row["style"])}</h2><p>Replacement equals a fresh fit: {row["replacementEqualsFreshFit"]}. '
                    f'Table/native mean Delta E76 {row["tableVsNative"]["meanDeltaE76"]:.3f}, p95 {row["tableVsNative"]["p95DeltaE76"]:.3f}. '
                    f'<a href="{html.escape(row["presetFile"])}">Download this XMP</a></p><div>{images}</div></article>')
    (out / 'index.html').write_text('<!doctype html><meta charset="utf-8"><title>AURA Lightroom preset verification</title>'
        '<style>body{background:#17171d;color:#eee;font:16px system-ui;margin:28px}a{color:#d7baff}article{border-top:1px solid #555;margin-top:28px}article div{display:grid;grid-template-columns:repeat(3,1fr);gap:12px}figure{margin:0}img{width:100%;height:330px;object-fit:contain;background:#09090b}</style>'
        '<h1>AURA reference replacement and Lightroom XMP export</h1><p>Different target and reference photos. The decoded-table comparison is independent of Rust; it does not certify Lightroom import or rendering.</p>'
        '<a href="results.json">Raw results</a>' + ''.join(rows), encoding='utf-8')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', type=Path, default=ROOT / 'output/aura-twenty-color-match-2026-10-24/run-a4149173')
    parser.add_argument('--exe', type=Path, default=ROOT / 'target/desktop-launch/debug/aura-desktop.exe')
    parser.add_argument('--count', type=int, choices=range(1, 21), default=20)
    parser.add_argument('--port', type=int, default=9394)
    parser.add_argument('--resume', type=Path, help='Resume this script\'s disposable run with the identical executable.')
    args = parser.parse_args()
    baseline, exe = args.baseline.resolve(), args.exe.resolve()
    previous = json.loads((baseline / 'results.json').read_text(encoding='utf-8'))
    dataset = baseline.parent / 'dataset'
    manifest = json.loads((dataset / 'manifest.json').read_text(encoding='utf-8'))
    assert manifest['duplicateChecks']['exactDuplicates'] == 0
    assert len({item['input']['pixelSha256'] for item in manifest['images']}) == 26
    targets = [item for item in manifest['images'] if item['role'] == 'target'][:args.count]
    hashes = {str(dataset / item[key]['path']): item[key]['sha256'] for item in manifest['images'] for key in ('source', 'input')}
    assert all(hashlib.sha256(Path(path).read_bytes()).hexdigest() == digest for path, digest in hashes.items())
    parent = ROOT / 'output/aura-lightroom-preset-2026-10-24'
    out = args.resume.resolve() if args.resume else parent / ('run-' + uuid.uuid4().hex[:8])
    assert out.parent == parent and out.name.startswith('run-')
    out.mkdir(parents=True, exist_ok=bool(args.resume))
    (out / 'downloads').mkdir(exist_ok=bool(args.resume))
    if not args.resume:
        with sqlite3.connect((baseline / 'catalog.sqlite').as_uri() + '?mode=ro', uri=True) as source, sqlite3.connect(out / 'catalog.sqlite') as destination:
            source.backup(destination)
    report = dict(createdUtc=datetime.now(timezone.utc).isoformat(), baseline=str(baseline), out=str(out),
                  exe=str(exe), exeSha256=hashlib.sha256(exe.read_bytes()).hexdigest(), photos=[], checks={}, pageErrors=[])
    if args.resume:
        saved_report = json.loads((out / 'results.json').read_text(encoding='utf-8'))
        assert saved_report['exeSha256'] == report['exeSha256'] and saved_report['baseline'] == report['baseline']
        report = saved_report
        report.setdefault('previousFailures', []).append(report.pop('failure', None))
    write(out, report)
    print(json.dumps({'started': str(out), 'photos': len(targets)}), flush=True)
    env = dict(os.environ, AURA_TEST_CATALOG=str(out / 'catalog.sqlite'), WEBVIEW2_USER_DATA_FOLDER=str(out / 'webview'),
               WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-port={args.port}')
    log = (out / 'desktop.log').open('w', encoding='utf-8')
    process = subprocess.Popen([str(exe)], cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
    try:
        for _ in range(120):
            if process.poll() is not None:
                raise RuntimeError(f'AURA exited: {process.returncode}')
            try:
                urllib.request.urlopen(f'http://127.0.0.1:{args.port}/json/version', timeout=1).close()
                break
            except OSError:
                time.sleep(1)
        with sync_playwright() as pw:
            browser = pw.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}')
            page = next(page for context in browser.contexts for page in context.pages)
            page.set_default_timeout(180000)
            expect.set_options(timeout=180000)
            page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
            page.on('pageerror', lambda error: report['pageErrors'].append(str(error)))

            def call(command, **kwargs):
                return page.evaluate('async ([c,a])=>{try{return await window.__TAURI_INTERNALS__.invoke(c,{input:a})}catch(e){throw new Error(JSON.stringify(e))}}', [command, kwargs])

            def status(photo):
                return page.evaluate("photoId=>window.__TAURI_INTERNALS__.invoke('color_match_status',{photoId})", photo)

            def match(photo, style, amount=1):
                reference = next(item['auraReference'] for item in previous['references'] if item['style'] == style)
                analysis = call('prepare_color_match', photoId=photo, referenceId=reference['id'], quality='precise', intent='palette',
                                replacePrevious=True, cancelId=str(uuid.uuid4()))
                saved = call('apply_color_match', photoId=photo, sessionId=analysis['sessionId'],
                             controls=dict(amount=amount, tone=1, color=1, whiteBalance=1), cancelId=str(uuid.uuid4()))
                assert saved['current'] and saved['fullResolutionChecked']
                assert saved['fitting']['engine'] == 'reference-palette-transport-v7'
                return saved

            def render(photo):
                return assessment.helpers.image_from_render(call('render_image', photoId=photo, level='full', purpose='export'))

            first_photo = None
            for index, target in enumerate(targets):
                photo = next(item['auraPhotoId'] for item in previous['pairs'] if item['targetIndex'] == target['index'])
                first_photo = first_photo or photo
                if any(row['targetIndex'] == target['index'] for row in report['photos']):
                    continue
                # Exercise actual reference replacement from a saved monochrome edit.
                if status(photo)['reference']['id'] != next(item['auraReference']['id'] for item in previous['references'] if item['style'] == 'monochrome'):
                    match(photo, 'monochrome')
                style = assessment.STYLES[index % 5]
                started = time.perf_counter()
                saved = match(photo, style)
                elapsed = (time.perf_counter() - started) * 1000
                actual = render(photo)
                stem = f'{target["index"]:02}.{style}'
                fresh = Image.open(baseline / f'{stem}.matched.png').convert('RGB')
                assert actual.size == fresh.size and actual.tobytes() == fresh.tobytes(), f'Reference compounding on {stem}'
                before = Image.open(baseline / f'{target["index"]:02}.before.png').convert('RGB')
                actual.save(out / f'{stem}.matched.png')
                before.save(out / f'{stem}.before.png')
                recipe_hash = call('image_recipe', photoId=photo)['recipeHash']
                started = time.perf_counter()
                preset = call('generate_color_match_preset', photoId=photo, sessionId=saved['sessionId'],
                              name=f'AURA {target["index"]:02} {style} & colour', cancelId=str(uuid.uuid4()))
                generation_ms = (time.perf_counter() - started) * 1000
                path = out / preset['fileName']
                path.write_text(preset.pop('xml'), encoding='utf-8')
                protocol, grid = validator.read_preset(path)
                decoded = Image.fromarray(validator.apply_table(grid, before))
                decoded.save(out / f'{stem}.table.png')
                assert call('image_recipe', photoId=photo)['recipeHash'] == recipe_hash, 'Preset generation mutated the edit'
                row = dict(targetIndex=target['index'], targetSourceId=target['photoId'], style=style,
                           referenceSourceId=next(item['source']['photoId'] for item in previous['references'] if item['style'] == style),
                           auraPhotoId=photo, stem=stem, replacementEqualsFreshFit=True, presetFile=path.name,
                           generationMilliseconds=generation_ms, replacementMilliseconds=elapsed, protocol=protocol,
                           preset=preset, tableVsNative=assessment.oracle_error(decoded, actual))
                report['photos'].append(row)
                write(out, report)
                print(json.dumps({'photo': target['index'], 'style': style, 'replacementExact': True,
                                  'tableMeanDeltaE76': row['tableVsNative']['meanDeltaE76'], 'generationMs': generation_ms}), flush=True)

            # The actual WebView download path, not just the IPC's XML string.
            call('history_step', projectId=previous['projectId'], photoId=first_photo, action='reset_original')
            match(first_photo, 'warm-matte')
            page.reload()
            page.get_by_role('button', name='Twenty separate colour tests run-a4149173 20', exact=True).click()
            page.get_by_role('button', name='Match a look Your reference, your photos', exact=True).click()
            first_name = next(item['input']['path'] for item in targets if item['index'] == 1)
            page.get_by_role('listbox', name='Filmstrip').get_by_role('option', name=Path(first_name).name, exact=True).click()
            panel = page.get_by_role('region', name='Reference colour matching', exact=True)
            footer = page.get_by_role('region', name='Lightroom preset export', exact=True)
            expect(footer).to_be_visible()
            assert panel.evaluate("el=>el.lastElementChild.getAttribute('aria-label')") == 'Lightroom preset export'
            session = page.context.new_cdp_session(page)
            session.send('Browser.setDownloadBehavior', {'behavior': 'allow', 'downloadPath': str(out / 'downloads'), 'eventsEnabled': True})

            def download(label):
                generate = footer.get_by_role('button', name='Generate .xmp for Lightroom', exact=True)
                expect(generate).to_be_enabled()
                footer.get_by_label('Lightroom preset name', exact=True).fill(label)
                generate.click()
                link = footer.get_by_role('link', name='Download .xmp', exact=True)
                expect(link).to_be_visible()
                filename = link.get_attribute('download')
                link.click()
                destination = out / 'downloads' / filename
                deadline = time.monotonic() + 60
                while time.monotonic() < deadline:
                    if destination.exists() and destination.stat().st_size:
                        break
                    time.sleep(.2)
                assert destination.exists(), 'WebView did not save the downloadable preset'
                protocol, _ = validator.read_preset(destination)
                return protocol

            report['uiDownloads'] = [download('AURA UI 100 percent & soft')]
            panel.get_by_role('checkbox', name='Real-Time Color Adjustments').check()
            for amount in (75, 0):
                panel.get_by_role('slider', name='Match amount', exact=True).evaluate("(el,value)=>{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(el,String(value));el.dispatchEvent(new Event('input',{bubbles:true}))}", amount)
                deadline = time.monotonic() + 180
                while time.monotonic() < deadline:
                    saved = status(first_photo)
                    if saved and saved['current'] and saved['fullResolutionChecked'] and abs(saved['controls']['amount'] - amount / 100) < .001:
                        break
                    time.sleep(.3)
                assert abs(saved['controls']['amount'] - amount / 100) < .001
                report['uiDownloads'].append(download(f'AURA UI {amount} percent'))
            assert len({item['tableFingerprint'] for item in report['uiDownloads']}) == 3
            report['checks']['bottomGenerateAndActualDownloads100_75_0'] = True
            # Stale state must refuse instead of exporting a different manual edit.
            project = previous['projectId']
            call('set_param', projectId=project, photoId=first_photo, path='global.exposure', value=.25, label='QA stale preset guard')
            changed_hash = call('image_recipe', photoId=first_photo)['recipeHash']
            try:
                call('generate_color_match_preset', photoId=first_photo, sessionId=saved['sessionId'], name='Stale must refuse', cancelId=str(uuid.uuid4()))
            except Exception as error:
                report['staleRefusal'] = str(error)
            else:
                raise AssertionError('Stale edit was exported')
            assert call('image_recipe', photoId=first_photo)['recipeHash'] == changed_hash
            report['checks']['staleRefusalPreservesManualEdit'] = True
            # Restore the disposable review photo, leaving originals and live catalogues alone.
            call('history_step', projectId=project, photoId=first_photo, action='reset_original')
            match(first_photo, 'warm-matte')
            page.reload()
            page.get_by_role('button', name='Twenty separate colour tests run-a4149173 20', exact=True).click()
            page.get_by_role('button', name='Match a look Your reference, your photos', exact=True).click()
            page.get_by_role('listbox', name='Filmstrip').get_by_role('option', name=Path(first_name).name, exact=True).click()
            download('AURA Final Warm Matte')
            page.screenshot(path=str(out / 'native-preset-workspace.png'), full_page=True)
            assert not report['pageErrors'], report['pageErrors']
            report['checks']['allSourceBytesUnchanged'] = all(hashlib.sha256(Path(path).read_bytes()).hexdigest() == digest for path, digest in hashes.items())
            assert report['checks']['allSourceBytesUnchanged']
            report['checks']['twentySeparateReplacementFits'] = len(report['photos']) == args.count
            report['checks']['allPresetProtocolsValid'] = all(row['protocol']['selfContained'] for row in report['photos'])
            report['completedUtc'] = datetime.now(timezone.utc).isoformat()
    except Exception as error:
        report['failure'] = str(error)
        raise
    finally:
        write(out, report)
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=20)
        log.close()
        print(json.dumps({'report': str(out / 'results.json'), 'checks': report['checks'], 'failure': report.get('failure')}), flush=True)


if __name__ == '__main__':
    main()
