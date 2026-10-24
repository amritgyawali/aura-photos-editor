"""Assess 20 real targets separately against six distinct, edited references.

Uses the running native desktop renderer and IPC in a disposable catalogue.
Known sRGB edits provide an independent per-target oracle; their parameters and
unedited reference pixels are NEVER passed to AURA's matcher. Palette resemblance
and recovery of the known edit are deliberately measured separately.
"""
import argparse
import base64
from collections import Counter
from datetime import datetime, timezone
import hashlib
import html
import importlib.util
import json
import os
from pathlib import Path
import statistics
import subprocess
import time
import urllib.request
import uuid

import numpy as np
from PIL import Image
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('match_helpers', ROOT / 'scripts/test-unique-color-match.py')
helpers = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helpers)
STYLES = ['warm-matte', 'cool-cyan', 'teal-orange', 'pastel', 'warm-punch', 'monochrome']


def grade(image, style):
    """Declared, deterministic edits of encoded sRGB; no spatial modifications."""
    x = np.asarray(image.convert('RGB'), dtype=np.float64) / 255
    y = (x * [.2126, .7152, .0722]).sum(axis=-1, keepdims=True)
    if style == 'warm-matte':
        x = (y + .82 * (x - y)) ** .92 * [1.07, 1.015, .88]
        x = .035 + .935 * x
    elif style == 'cool-cyan':
        x = (y + .86 * (x - y)) ** 1.06 * [.88, 1.015, 1.10]
        x += (1 - y) * [-.005, .012, .025]
    elif style == 'teal-orange':
        shadow = np.clip((.65 - y) / .65, 0, 1)
        highlight = np.clip((y - .35) / .65, 0, 1)
        x = (x - .5) * 1.08 + .5 + shadow * [-.035, .025, .055] + highlight * [.05, .012, -.04]
    elif style == 'pastel':
        x = .055 + .90 * (y + .62 * (x - y)) ** .82 + [.015, 0, .018]
    elif style == 'warm-punch':
        x = ((y + 1.3 * (x - y) - .5) * 1.18 + .5) * [1.04, 1, .94]
    elif style == 'monochrome':
        x = np.repeat(.015 + .97 * np.clip((y - .5) * 1.08 + .5, 0, 1), 3, axis=-1)
    else:
        raise ValueError(style)
    return Image.fromarray(np.rint(np.clip(x, 0, 1) * 255).astype(np.uint8))


def lab(image):
    rgb = np.asarray(image.convert('RGB'), dtype=np.float64) / 255
    linear = np.where(rgb <= .04045, rgb / 12.92, ((rgb + .055) / 1.055) ** 2.4)
    xyz = linear @ np.array([[.4124564, .3575761, .1804375], [.2126729, .7151522, .0721750], [.0193339, .1191920, .9503041]]).T
    xyz /= [.95047, 1, 1.08883]
    f = np.where(xyz > (6 / 29) ** 3, np.cbrt(xyz), xyz / (3 * (6 / 29) ** 2) + 4 / 29)
    return np.stack([116 * f[..., 1] - 16, 500 * (f[..., 0] - f[..., 1]), 200 * (f[..., 1] - f[..., 2])], axis=-1)


def oracle_error(actual, expected):
    a, b = lab(actual), lab(expected)
    delta = a - b
    de = np.linalg.norm(delta, axis=-1)
    # Known-look check only. These are declared QA cutoffs, not universal JNDs.
    mean, p95 = float(de.mean()), float(np.quantile(de, .95))
    result = dict(meanDeltaE76=mean, p95DeltaE76=p95,
                  meanAbsToneError=float(np.abs(delta[..., 0]).mean()),
                  meanChromaPlaneError=float(np.linalg.norm(delta[..., 1:], axis=-1).mean()),
                  meanSignedLabError=delta.mean(axis=(0, 1)).tolist(),
                  meanRgbDifference=helpers.change(actual, expected),
                  bitwiseIdentical=actual.tobytes() == expected.tobytes(),
                  closeKnownLook=mean <= 3 and p95 <= 8)
    for name, mask in [('shadows', b[..., 0] < 30), ('midtones', (b[..., 0] >= 30) & (b[..., 0] < 75)), ('highlights', b[..., 0] >= 75)]:
        result[name] = dict(pixels=int(mask.sum()), meanDeltaE76=float(de[mask].mean()) if mask.any() else None,
                            signedToneError=float(delta[..., 0][mask].mean()) if mask.any() else None)
    return result


def distributions(image, reference):
    q = helpers.lab_distribution(image)
    residual = q - reference
    return dict(full=float(np.linalg.norm(residual, axis=1).mean()),
                color=float(np.linalg.norm(residual[:, 1:], axis=1).mean()),
                tone=float(np.abs(residual[:, 0]).mean()), signedQuantileErrors=residual.tolist())


def write_artifacts(out, report, manifest):
    (out / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    parts = ['<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>AURA: 20 photos, six edited references</title>',
             '<style>body{background:#17171c;color:#eee;font:16px system-ui;margin:28px}a{color:#d8bcff}article{border-top:1px solid #666;margin-top:30px}.grid{display:grid;grid-template-columns:repeat(4,1fr);gap:12px}img{width:100%;height:300px;object-fit:contain;background:#09090b}figure{margin:0}p{line-height:1.5} @media(max-width:800px){.grid{grid-template-columns:1fr 1fr}}</style>',
             '<h1>20 new photographs × 6 edited references</h1><p>Every reference is a different photograph from every target. The expected result uses the declared edit on that target; AURA receives only the edited reference. A closer reference palette does not prove recovery of that edit. Exact means identical pixels. Close known look requires mean ΔE76 ≤3 and p95 ≤8; these are QA thresholds.</p>',
             '<p><a href="results.json">Raw native results</a> · <a href="../dataset/ATTRIBUTION.md">Photo credits</a> · <a href="../dataset/manifest.json">Duplicate screening</a></p>',
             '<p>Workflow: <strong>' + html.escape(report['intent']) + '</strong>. ' + html.escape(json.dumps(report.get('summary', {}))) + '</p>',
             '<p><label>Target <select id="target-filter"><option value="">All 20</option>' + ''.join(f'<option value="{i}">{i:02}</option>' for i in range(1, 21)) + '</select></label> <label>Grade <select id="style-filter"><option value="">All six</option>' + ''.join(f'<option>{style}</option>' for style in STYLES) + '</select></label></p>']
    for target in [x for x in manifest['images'] if x['role'] == 'target']:
        rows = [x for x in report['pairs'] if x['targetIndex'] == target['index']]
        if not rows:
            continue
        parts.append(f'<article data-target="{target["index"]}"><h2>{target["index"]:02}. {html.escape(target["description"])}</h2><p><a href="{target["sourcePage"]}">Photo {target["photoId"]} by {html.escape(target["author"])}</a></p>')
        for row in rows:
            parts.append(f'<section data-style="{row["style"]}"><h3>{row["style"]} — {html.escape(row["verdict"])}</h3><div class="grid">')
            for path, label in [(f'../dataset/{target["input"]["path"]}', 'Original input'), (f'references/{row["style"]}.png', 'Different edited reference'), (f'{row["stem"]}.expected.png', 'Expected edit on this target'), (f'{row["stem"]}.matched.png', 'Actual AURA output')]:
                parts.append(f'<figure><a href="{path}"><img loading="lazy" src="{path}"></a><figcaption>{label}</figcaption></figure>')
            parts.append('</div><p>' + html.escape(row.get('error', f'Reference colour distance: {row["paletteBefore"]["color"]:.2f} → {row["paletteAfter"]["color"]:.2f}. Known-edit mean ΔE76: {row["oracle"]["meanDeltaE76"]:.2f}; p95 {row["oracle"]["p95DeltaE76"]:.2f}.')) + '</p></section>')
        parts.append('</article>')
    parts.append('''<script>
    function filter(){
      const target=document.getElementById('target-filter').value,style=document.getElementById('style-filter').value;
      document.querySelectorAll('article[data-target]').forEach(row=>row.hidden=!!target&&row.dataset.target!==target);
      document.querySelectorAll('section[data-style]').forEach(row=>row.hidden=!!style&&row.dataset.style!==style);
    }
    document.querySelectorAll('select').forEach(control=>control.addEventListener('change',filter));
    </script>''')
    (out / 'index.html').write_text('\n'.join(parts), encoding='utf-8')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--dataset', type=Path, default=ROOT / 'output/aura-twenty-color-match-2026-10-24/dataset')
    parser.add_argument('--exe', type=Path, default=ROOT / 'target/desktop-launch/debug/aura-desktop.exe')
    parser.add_argument('--port', type=int, default=9385)
    parser.add_argument('--target-limit', type=int, default=20)
    parser.add_argument('--style-limit', type=int, default=6)
    parser.add_argument('--intent', choices=['palette', 'natural'], default='palette')
    parser.add_argument('--paired-calibration', action='store_true', help='Separately test the existing original + edited reference calibration workflow. Training source is never a target.')
    parser.add_argument('--resume', type=Path, help='Resume only this assessment script\'s disposable run; completed pairs remain unchanged.')
    args = parser.parse_args()
    dataset, exe = args.dataset.resolve(), args.exe.resolve()
    manifest = json.loads((dataset / 'manifest.json').read_text(encoding='utf-8'))
    assert manifest['duplicateChecks']['visualInspection']['status'] == 'reviewed'
    assert manifest['duplicateChecks']['exactDuplicates'] == 0
    targets = [x for x in manifest['images'] if x['role'] == 'target'][:args.target_limit]
    refs = [x for x in manifest['images'] if x['role'] == 'reference'][:args.style_limit]
    assert len({x['input']['pixelSha256'] for x in manifest['images']}) == 26
    hashes = {str(dataset / x[k]['path']): x[k]['sha256'] for x in manifest['images'] for k in ('source', 'input')}
    assert all(hashlib.sha256(Path(p).read_bytes()).hexdigest() == h for p, h in hashes.items())
    out = args.resume.resolve() if args.resume else dataset.parent / ('run-' + uuid.uuid4().hex[:8])
    assert out.parent == dataset.parent and out.name.startswith('run-'), 'Assessment must stay within its dedicated output directory'
    (out / 'references').mkdir(parents=True, exist_ok=bool(args.resume))
    report = dict(createdUtc=datetime.now(timezone.utc).isoformat(), exe=str(exe), exeSha256=hashlib.sha256(exe.read_bytes()).hexdigest(),
                  intent='paired-calibration' if args.paired_calibration else args.intent, quality='precise', controls=dict(amount=1, tone=1, color=1, whiteBalance=1),
                  methodology='Six known encoded-sRGB edits on six separate source photos. Matcher sees only edited reference pixels; target oracle is grade(native baseline). Known-edit recovery and unpaired palette resemblance are different objectives.',
                  oracleThresholds={'meanDeltaE76': 3, 'p95DeltaE76': 8, 'exact': 'bitwise identical RGB8 pixels'},
                  pairs=[], references=[], checks={}, pageErrors=[], deliveries=[])
    if args.resume:
        previous = json.loads((out / 'results.json').read_text(encoding='utf-8'))
        assert previous['exeSha256'] == report['exeSha256'] and previous['intent'] == report['intent']
        report = previous
        report['resumedUtc'] = datetime.now(timezone.utc).isoformat()
        report['previousRunFailure'] = report.pop('failure', None)
        assert not args.paired_calibration, 'Paired preparation resume is not supported'
    else:
        for style, entry in zip(STYLES, refs):
            edited = grade(Image.open(dataset / entry['input']['path']).convert('RGB'), style)
            path = out / 'references' / f'{style}.png'
            edited.save(path)
            report['references'].append(dict(style=style, source=entry, path=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest()))
    write_artifacts(out, report, manifest)
    print(json.dumps({'started': str(out), 'pairs': len(targets) * len(refs), 'intent': report['intent']}), flush=True)
    env = dict(os.environ, AURA_TEST_CATALOG=str(out / 'catalog.sqlite'), WEBVIEW2_USER_DATA_FOLDER=str(out / 'webview'),
               WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-port={args.port}')
    process = subprocess.Popen([str(exe)], cwd=ROOT, env=env)
    try:
        for _ in range(120):
            if process.poll() is not None:
                raise RuntimeError(f'Desktop exited before connection: {process.returncode}')
            try:
                urllib.request.urlopen(f'http://127.0.0.1:{args.port}/json/version', timeout=1).close()
                break
            except OSError:
                time.sleep(1)
        with sync_playwright() as pw:
            browser = pw.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}')
            page = next(p for c in browser.contexts for p in c.pages)
            page.set_default_timeout(180000)
            page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
            page.on('pageerror', lambda error: report['pageErrors'].append(str(error)))

            def call(command, **kwargs):
                return page.evaluate('async ([c,a])=>{try{return await window.__TAURI_INTERNALS__.invoke(c,{input:a})}catch(e){throw new Error(JSON.stringify(e))}}', [command, kwargs])

            def raw(command, **kwargs):
                return page.evaluate('async ([c,a])=>{try{return await window.__TAURI_INTERNALS__.invoke(c,a)}catch(e){throw new Error(JSON.stringify(e))}}', [command, kwargs])

            project = report['projectId'] if args.resume else call('create_project', name='Twenty separate colour tests ' + out.name, coupleNames=None, eventDate=None)['id']
            report['projectId'] = project
            ingested = targets + refs if args.paired_calibration else targets
            if not args.resume:
                call('start_ingest', projectId=project, roots=[str(dataset / x['input']['path']) for x in ingested])
            for _ in range(180):
                photos = call('list_images', projectId=project, offset=0, limit=60, orderBy='timeline')
                if len(photos) == len(ingested):
                    break
                time.sleep(.5)
            assert len(photos) == len({x['id'] for x in photos}) == len(ingested)
            baselines = {}
            auto_baselines = {}
            if args.paired_calibration:
                report['methodology'] += ' Paired calibration has its own automatic correction stage. Both training edits and target oracles use native zero-strength reference-style baselines, not the unpaired workflow baseline. Results across workflows therefore compare different complete pipelines, not an isolated algorithm A/B.'
                dummy = call('analyse_reference_style', address='Baseline only', folder=str(dataset / refs[0]['input']['path']), cancelId=str(uuid.uuid4()), export=False)
                for entry in ingested:
                    photo = next(x['id'] for x in photos if x['fileName'] == Path(entry['input']['path']).name)
                    # This workflow requires a stored edit. Create and reset a real
                    # disposable edit; a no-op reset is intentionally not journalled.
                    call('set_param', projectId=project, photoId=photo, path='global.exposure', value=.125, label='Assessment setup')
                    call('history_step', projectId=project, photoId=photo, action='reset_original')
                    call('apply_reference_style', photoId=photo, referenceId=dummy['id'], strength=0, cancelId=str(uuid.uuid4()))
                    auto_baselines[photo] = helpers.image_from_render(call('render_image', photoId=photo, level='full', purpose='export'))
                    call('history_step', projectId=project, photoId=photo, action='reset_original')
                for reference in report['references']:
                    entry = reference['source']
                    photo = next(x['id'] for x in photos if x['fileName'] == Path(entry['input']['path']).name)
                    auto_baselines[photo].save(out / 'references' / f'{reference["style"]}.auto-baseline.png')
                    grade(auto_baselines[photo], reference['style']).save(reference['path'])
                    reference['sha256'] = hashlib.sha256(Path(reference['path']).read_bytes()).hexdigest()
                    reference['preparation'] = 'Declared grade applied to native automatic-correction baseline; calibration is given raw original source + this edit.'
            for target in targets:
                photo = next(x['id'] for x in photos if x['fileName'] == Path(target['input']['path']).name)
                call('history_step', projectId=project, photoId=photo, action='reset_original')
                before = helpers.image_from_render(call('render_image', photoId=photo, level='full', purpose='export'))
                if args.resume:
                    with Image.open(out / f'{target["index"]:02}.before.png') as saved:
                        assert helpers.change(before, saved.convert('RGB')) == 0, 'Resumed original differs from first measured baseline'
                baselines[photo] = before
                target['auraPhotoId'] = photo
                target['baselineRecipeHash'] = call('image_recipe', photoId=photo)['recipeHash']
                baseline_body = json.loads(call('image_recipe', photoId=photo)['body'])
                baseline_body['provenance'].pop('source', None)
                target['baselineRecipeBodyWithoutSource'] = baseline_body
                before.save(out / f'{target["index"]:02}.before.png')
            for reference in report['references']:
                style = reference['style']
                if sum(x['style'] == style for x in report['pairs']) == len(targets):
                    continue
                ref_image = Image.open(reference['path']).convert('RGB')
                ref_q = helpers.lab_distribution(ref_image)
                started = time.perf_counter()
                if args.paired_calibration:
                    imported = call('analyse_reference_style', address=style, folder=reference['path'], original=str(dataset / reference['source']['input']['path']), cancelId=str(uuid.uuid4()), export=False)
                    assert imported.get('calibration'), 'Original + edited pair did not create a calibrated model'
                    reference['trainingMilliseconds'] = (time.perf_counter() - started) * 1000
                else:
                    imported = call('import_color_reference', path=reference['path'], cancelId=str(uuid.uuid4()))
                reference['auraReference'] = imported
                for target in targets:
                    if any(x['targetIndex'] == target['index'] and x['style'] == style for x in report['pairs']):
                        continue
                    photo = target['auraPhotoId']
                    call('history_step', projectId=project, photoId=photo, action='reset_original')
                    reset_recipe = call('image_recipe', photoId=photo)
                    reset_body = json.loads(reset_recipe['body'])
                    reset_body['provenance'].pop('source', None)
                    assert reset_body == target['baselineRecipeBodyWithoutSource'], 'Reset left earlier edits/protections in the baseline'
                    reset_hash = reset_recipe['recipeHash']
                    before = baselines[photo]
                    reset_render = helpers.image_from_render(call('render_image', photoId=photo, level='full', purpose='export'))
                    assert helpers.change(reset_render, before) == 0, 'A previous pair contaminated this target'
                    expected = grade(auto_baselines[photo] if args.paired_calibration else before, style)
                    stem = f'{target["index"]:02}.{style}'
                    expected.save(out / f'{stem}.expected.png')
                    row = dict(targetIndex=target['index'], photoId=target['photoId'], auraPhotoId=photo,
                               style=style, referencePhotoId=reference['source']['photoId'], stem=stem,
                               paletteBefore=distributions(before, ref_q), oracleBefore=oracle_error(before, expected),
                               clippingBefore=helpers.clipping(before), baselineResetPixelIdentical=True)
                    started = time.perf_counter()
                    try:
                        if args.paired_calibration:
                            result = call('apply_reference_style', photoId=photo, referenceId=imported['id'], strength=1, cancelId=str(uuid.uuid4()))
                            row['applyMilliseconds'] = (time.perf_counter() - started) * 1000
                            row['native'] = result
                            row['autoBaselineToOracle'] = oracle_error(auto_baselines[photo], expected)
                        else:
                            analysis = call('prepare_color_match', photoId=photo, referenceId=imported['id'], cancelId=str(uuid.uuid4()), quality='precise', intent=args.intent)
                            row['analysisMilliseconds'] = (time.perf_counter() - started) * 1000
                            assert analysis['fitting']['intent'] == args.intent
                            row['analysis'] = helpers.compact(analysis)
                            started = time.perf_counter()
                            result = call('apply_color_match', photoId=photo, sessionId=analysis['sessionId'], controls=report['controls'], cancelId=str(uuid.uuid4()))
                            row['applyMilliseconds'] = (time.perf_counter() - started) * 1000
                            assert result['current'] and result['fullResolutionChecked'] and result['reference']['id'] == imported['id']
                            row['native'] = helpers.compact(result)
                    except Exception as error:
                        row['error'] = str(error)
                        assert call('image_recipe', photoId=photo)['recipeHash'] == reset_hash, 'Refusal modified the saved recipe'
                        row['refusalPreservedBaseline'] = True
                    actual = helpers.image_from_render(call('render_image', photoId=photo, level='full', purpose='export'))
                    preview = helpers.image_from_render(raw('native_retouch_preview', projectId=project, photoId=photo, before=False, quality='full'))
                    assert actual.size == preview.size == before.size
                    row['previewExportMeanDifference'] = helpers.change(preview, actual)
                    assert row['previewExportMeanDifference'] == 0
                    actual.save(out / f'{stem}.matched.png')
                    row['oracle'] = oracle_error(actual, expected)
                    row['paletteAfter'] = distributions(actual, ref_q)
                    row['clippingAfter'] = helpers.clipping(actual)
                    row['meanRgbChange'] = helpers.change(before, actual)
                    row['colorImprovementPercent'] = helpers.reduction(row['paletteBefore']['color'], row['paletteAfter']['color'])
                    row['oracleImprovementPercent'] = helpers.reduction(row['oracleBefore']['meanDeltaE76'], row['oracle']['meanDeltaE76'])
                    row['verdict'] = ('refused safely' if 'error' in row else 'exact known look' if row['oracle']['bitwiseIdentical'] else 'close known look' if row['oracle']['closeKnownLook'] else 'partial palette match' if row['colorImprovementPercent'] >= 10 else 'weak or worse colour match')
                    report['pairs'].append(row)
                    report['summary'] = dict(Counter(x['verdict'] for x in report['pairs']))
                    write_artifacts(out, report, manifest)
                    print(json.dumps(dict(target=target['index'], style=style, verdict=row['verdict'], colorPercent=round(row['colorImprovementPercent'], 2), oracleDeltaE=round(row['oracle']['meanDeltaE76'], 2), error=row.get('error'))), flush=True)
                # The native Delivery path is checked separately, once for every saved target per style.
                destination = out / 'delivery' / style
                delivery = call('export_run', projectId=project, destination=str(destination), destinationKind='folder',
                                sets=[dict(name='Known-look verification', imageIds=[x['auraPhotoId'] for x in targets], format='png', quality=95, colour='srgb', bitDepth=8, resize='full', sharpen='none', naming='{seq}_{original}', sidecar=False)],
                                copyright=None, contact=None, creator=None, keywords=[], stripGps=True, stripCameraSerial=True, verify=True)
                assert delivery['written'] == delivery['verified'] == delivery['requested'] == len(targets)
                assert delivery['manifestSealed'] and delivery['renderFailed'] == delivery['corrupt'] == 0
                files = raw('export_files', projectId=project)
                comparisons = []
                for file in files:
                    row = next(x for x in report['pairs'] if x['auraPhotoId'] == file['imageId'] and x['style'] == style)
                    with Image.open(destination / file['path']) as actual, Image.open(out / f'{row["stem"]}.matched.png') as expected:
                        assert actual.size == expected.size and helpers.change(actual.convert('RGB'), expected.convert('RGB')) == 0
                    comparisons.append(dict(photoId=row['photoId'], path=file['path'], identical=True))
                assert len(comparisons) == len(targets)
                report['deliveries'].append(dict(style=style, status=delivery, comparisons=comparisons))
                write_artifacts(out, report, manifest)
            report['checks'] = dict(allPairsRun=len(report['pairs']) == len(targets) * len(refs),
                                   distinctTargets=len({x['photoId'] for x in targets}) == len(targets),
                                   differentReferenceForEveryTarget=all(x['photoId'] != x['referencePhotoId'] for x in report['pairs']),
                                   everyPairResetToVerifiedOriginal=all(x['baselineResetPixelIdentical'] for x in report['pairs']),
                                   previewExportIdentical=all(x['previewExportMeanDifference'] == 0 for x in report['pairs']),
                                   sourceAndInputFilesUnchanged=all(hashlib.sha256(Path(p).read_bytes()).hexdigest() == h for p, h in hashes.items()),
                                   noPageErrors=not report['pageErrors'], allDeliveriesVerified=len(report['deliveries']) == len(refs))
            assert all(report['checks'].values())
            print(json.dumps({'completed': str(out), 'summary': report['summary'], 'checks': report['checks']}), flush=True)
    except BaseException as error:
        report['failure'] = f'{type(error).__name__}: {error}'
        raise
    finally:
        report['desktopExitCodeBeforeCleanup'] = process.poll()
        write_artifacts(out, report, manifest)
        process.terminate()
        try:
            process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=10)


if __name__ == '__main__':
    main()
