"""Extend a completed twenty-photo GUI assessment to every reference/workflow pair.

Uses a SQLite backup of the assessment catalogue and the identical desktop binary.
The initial 60 cases retain their real GUI evidence. Additional cases invoke the
same native commands, reset and verify baselines, and check native preview parity.
"""
import argparse
import csv
from datetime import datetime, timezone
import hashlib
import html
import importlib.util
import json
import os
from pathlib import Path
import sqlite3
import statistics
import subprocess
import time
import urllib.request
import uuid

import numpy as np
from PIL import Image, ImageDraw, ImageOps
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('desktop_assessment', ROOT / 'scripts/test-main-match-twenty.py')
assessment = importlib.util.module_from_spec(spec)
spec.loader.exec_module(assessment)
metrics, helpers = assessment.metrics, assessment.helpers
STYLES = metrics.STYLES
WORKFLOWS = ['collection', 'natural', 'palette']


def summarize(rows):
    return dict(attempted=len(rows), refused=sum('error' in x for x in rows),
                changed=sum(x['meanRgbChange'] > 0 for x in rows),
                referenceColorImproved=sum(x['paletteAfter']['color'] + 1e-4 < x['paletteBefore']['color'] for x in rows),
                jointColorImproved=sum(x['jointAfter'] + 1e-4 < x['jointBefore'] for x in rows),
                closeReferenceColorTone=sum(x['paletteAfter']['color'] <= 3 and x['paletteAfter']['tone'] <= 3 and x['jointAfter'] <= 2 for x in rows),
                knownEditImproved=sum(x['oracle']['meanDeltaE76'] + 1e-4 < x['oracleBefore']['meanDeltaE76'] for x in rows),
                closeKnownEdit=sum(x['oracle']['closeKnownLook'] for x in rows),
                closeKnownChanged=sum(x['oracle']['closeKnownLook'] and x['meanRgbChange'] > 0 and 'error' not in x for x in rows),
                exactKnownEdit=sum(x['oracle']['bitwiseIdentical'] for x in rows),
                medianKnownEditError=statistics.median(x['oracle']['meanDeltaE76'] for x in rows),
                medianApplySeconds=statistics.median(x['applyMilliseconds'] / 1000 for x in rows))


def artifacts(out, report):
    report['summary'] = [{**summarize([x for x in report['pairs'] if x['workflow'] == workflow]), 'workflow': workflow}
                         for workflow in WORKFLOWS if any(x['workflow'] == workflow for x in report['pairs'])]
    (out / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    with (out / 'pairs.csv').open('w', newline='', encoding='utf-8-sig') as stream:
        names = ['target', 'description', 'workflow', 'style', 'outcome', 'colorBefore', 'colorAfter', 'toneBefore', 'toneAfter', 'jointBefore', 'jointAfter', 'knownBefore', 'knownAfter', 'knownP95', 'closeKnown', 'meanRgbChange', 'seconds', 'error', 'image']
        writer = csv.DictWriter(stream, fieldnames=names)
        writer.writeheader()
        for row in report['pairs']:
            writer.writerow(dict(target=row['targetIndex'], description=row['description'], workflow=row['workflow'], style=row['style'], outcome=row['verdict'],
                                 colorBefore=row['paletteBefore']['color'], colorAfter=row['paletteAfter']['color'], toneBefore=row['paletteBefore']['tone'], toneAfter=row['paletteAfter']['tone'],
                                 jointBefore=row['jointBefore'], jointAfter=row['jointAfter'], knownBefore=row['oracleBefore']['meanDeltaE76'], knownAfter=row['oracle']['meanDeltaE76'], knownP95=row['oracle']['p95DeltaE76'],
                                 closeKnown=row['oracle']['closeKnownLook'], meanRgbChange=row['meanRgbChange'], seconds=row['applyMilliseconds'] / 1000, error=row.get('error', ''), image=row['afterFile']))
    page = ['<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>AURA complete reference-match matrix</title>',
            '<style>body{background:#19191f;color:#eee;font:16px system-ui;margin:24px}a{color:#dac5ff}.grid{display:grid;grid-template-columns:repeat(4,1fr);gap:12px}figure{margin:0}img{width:100%;height:270px;object-fit:contain;background:#09090b}article{border-top:1px solid #666;margin:26px 0}select{margin:10px}p{line-height:1.5}@media(max-width:850px){.grid{grid-template-columns:1fr 1fr}}</style>',
            '<h1>20 photographs × 6 references × 3 native matching paths</h1><p>All matches request 80%. Reference colour resemblance, known-edit recovery and visible artifact freedom are separate criteria. Collection uses an automatic-correction baseline; the studio modes use their neutral native baseline. The known 80% edit is an encoded-sRGB blend, which differs from parameter interpolation. No target or original reference pixels train the unpaired matcher.</p>',
            '<p><a href="results.json">Raw measurements</a> · <a href="pairs.csv">All pair measurements (CSV)</a></p>',
            '<label>Photo<select id="photo"><option value="">All</option>' + ''.join(f'<option>{i}</option>' for i in range(1, 21)) + '</select></label>',
            '<label>Workflow<select id="workflow"><option value="">All</option>' + ''.join(f'<option>{w}</option>' for w in WORKFLOWS) + '</select></label>',
            '<label>Look<select id="style"><option value="">All</option>' + ''.join(f'<option>{s}</option>' for s in STYLES) + '</select></label>']
    refs = {r['style']: r for r in report['references']}
    for row in sorted(report['pairs'], key=lambda x: (x['targetIndex'], x['style'], WORKFLOWS.index(x['workflow']))):
        reference = Path(refs[row['style']]['path']).relative_to(out.parent).as_posix()
        page.append(f'<article data-photo="{row["targetIndex"]}" data-workflow="{row["workflow"]}" data-style="{row["style"]}"><h2>{row["targetIndex"]:02}. {html.escape(row["description"])} / {row["workflow"]} / {row["style"]}</h2><p>{html.escape(row.get("error",row["verdict"]))}</p><div class="grid">')
        for path, label in [(f'../{reference}', 'Different edited reference'), (row['beforeFile'], 'Native workflow baseline'), (row['afterFile'], 'Actual AURA'), (row['expectedFile'], 'Declared known edit')]:
            page.append(f'<figure><a href="{path}"><img loading="lazy" src="{path}"></a><figcaption>{label}</figcaption></figure>')
        page.append(f'</div><p>Colour distance {row["paletteBefore"]["color"]:.3f} → {row["paletteAfter"]["color"]:.3f}; joint colour {row["jointBefore"]:.3f} → {row["jointAfter"]:.3f}. Known-edit mean CIE76 {row["oracleBefore"]["meanDeltaE76"]:.3f} → {row["oracle"]["meanDeltaE76"]:.3f}; p95 {row["oracle"]["p95DeltaE76"]:.3f}.</p></article>')
    page.append('''<script>function filter(){document.querySelectorAll('article').forEach(a=>a.hidden=['photo','workflow','style'].some(k=>document.getElementById(k).value&&a.dataset[k]!==document.getElementById(k).value))}document.querySelectorAll('select').forEach(s=>s.addEventListener('change',filter))</script>''')
    (out / 'index.html').write_text('\n'.join(page), encoding='utf-8')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('assessment', type=Path)
    parser.add_argument('--port', type=int, default=9398)
    args = parser.parse_args()
    source = args.assessment.resolve()
    prior = json.loads((source / 'results.json').read_text())
    assert prior['completed'] and all(prior['checks'].values())
    exe = Path(prior['exe'])
    assert assessment.digest(exe) == prior['exeSha256']
    out = source / 'matrix'
    out.mkdir(exist_ok=True)
    for name in ['baseline', 'matched', 'expected']:
        (out / name).mkdir(exist_ok=True)
    if (out / 'results.json').exists():
        report = json.loads((out / 'results.json').read_text())
        assert report['exeSha256'] == prior['exeSha256']
        report.setdefault('priorFailures', []).append(report.pop('failure', None))
    else:
        report = dict(startedUtc=datetime.now(timezone.utc).isoformat(), exe=str(exe), exeSha256=prior['exeSha256'], references=prior['references'], pairs=[], checks={}, pageErrors=[])
        for item in prior['pairs']:
            row = dict(item)
            row.update(beforeFile=f'../{row["stem"]}.before.png', afterFile=f'../{row["stem"]}.matched.png', expectedFile=f'../{row["stem"]}.expected.png', testedThroughGui=row['workflow'] != 'collection')
            report['pairs'].append(row)
        with sqlite3.connect(source / 'catalog.sqlite') as src, sqlite3.connect(out / 'catalog.sqlite') as dst:
            src.backup(dst)
    dataset = ROOT / 'output/aura-twenty-color-match-2026-10-24/dataset'
    manifest = json.loads((dataset / 'manifest.json').read_text())
    targets = [x for x in manifest['images'] if x['role'] == 'target']
    hashes = {str(dataset / x[k]['path']): x[k]['sha256'] for x in manifest['images'] for k in ['source', 'input']}
    assert all(assessment.digest(path) == value for path, value in hashes.items())
    refs = {ref['style']: ref for ref in report['references']}
    baselines = {}
    for workflow in WORKFLOWS:
        for target in targets:
            row = next(x for x in prior['pairs'] if x['workflow'] == workflow and x['targetIndex'] == target['index'])
            baselines[(workflow, target['index'])] = Image.open(source / f'{row["stem"]}.before.png').convert('RGB')
            target['auraPhotoId'] = row['auraPhotoId']
    artifacts(out, report)
    print(json.dumps(dict(matrix=str(out), retained=len(report['pairs']), total=360)), flush=True)
    env = dict(os.environ, AURA_TEST_CATALOG=str(out / 'catalog.sqlite'), WEBVIEW2_USER_DATA_FOLDER=str(out / 'webview'), WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-address=127.0.0.1 --remote-debugging-port={args.port}')
    startup = subprocess.STARTUPINFO(); startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW; startup.wShowWindow = 0
    with (out / 'desktop.log').open('a', encoding='utf-8') as log:
        process = subprocess.Popen([str(exe)], cwd=ROOT, env=env, startupinfo=startup, stdout=log, stderr=subprocess.STDOUT)
        try:
            for _ in range(90):
                if process.poll() is not None:
                    raise RuntimeError(f'Desktop exited: {process.returncode}')
                try:
                    urllib.request.urlopen(f'http://127.0.0.1:{args.port}/json/version', timeout=1).close(); break
                except OSError:
                    time.sleep(1)
            with sync_playwright() as pw:
                page = pw.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}').contexts[0].pages[0]
                page.wait_for_function('()=>!!window.__TAURI_INTERNALS__')
                page.on('pageerror', lambda error: report['pageErrors'].append(str(error)))
                def call(command, **kwargs):
                    return page.evaluate('async ([c,a])=>{try{return await window.__TAURI_INTERNALS__.invoke(c,{input:a})}catch(e){throw new Error(JSON.stringify(e))}}', [command, kwargs])
                def raw(command, **kwargs):
                    return page.evaluate('async ([c,a])=>window.__TAURI_INTERNALS__.invoke(c,a)', [command, kwargs])
                def render(photo):
                    return helpers.image_from_render(call('render_image', photoId=photo, level='full', purpose='export'))
                # Reference evidence lives beside its catalogue, outside SQLite. Rebuild
                # that evidence from the retained edited pixels for this isolated copy.
                for ref in report['references']:
                    ref['collection'] = call('analyse_reference_style', address=ref['style'], folder=ref['path'], cancelId=str(uuid.uuid4()), export=False)
                    ref['studio'] = call('import_color_reference', path=ref['path'], cancelId=str(uuid.uuid4()))
                artifacts(out, report)
                baseline_bodies = {}
                for workflow in WORKFLOWS:
                    for style in STYLES:
                        with Image.open(refs[style]['path']) as image:
                            reference = image.convert('RGB'); distribution = helpers.lab_distribution(reference); reference_joint = assessment.joint(reference)
                        for target in targets:
                            index, photo = target['index'], target['auraPhotoId']
                            if any(x['workflow'] == workflow and x['style'] == style and x['targetIndex'] == index for x in report['pairs']):
                                continue
                            call('history_step', projectId=prior['projectId'], photoId=photo, action='reset_original')
                            if workflow == 'collection':
                                call('apply_reference_style', photoId=photo, referenceId=refs[style]['collection']['id'], strength=0, cancelId=str(uuid.uuid4()))
                            recipe = call('image_recipe', photoId=photo)
                            body = json.loads(recipe['body']); body.pop('provenance', None)
                            key = (workflow, index)
                            before = baselines[key]
                            if key not in baseline_bodies:
                                assert helpers.change(before, render(photo)) == 0, 'Baseline differs from GUI assessment'
                                baseline_bodies[key] = body
                            else:
                                assert body == baseline_bodies[key], 'A prior style contaminated the baseline'
                            before_path = out / 'baseline' / f'{index:02}.{workflow}.png'
                            if not before_path.exists(): before.save(before_path)
                            expected = Image.fromarray(np.rint(.2 * np.asarray(before, dtype=float) + .8 * np.asarray(metrics.grade(before, style), dtype=float)).astype('uint8'))
                            stem = f'{index:02}.{workflow}.{style}'
                            expected.save(out / 'expected' / f'{stem}.png')
                            row = dict(targetIndex=index, description=target['description'], workflow=workflow, style=style, photoId=target['photoId'], referencePhotoId=refs[style]['photoId'], auraPhotoId=photo, stem=stem,
                                       beforeFile=before_path.relative_to(out).as_posix(), afterFile=f'matched/{stem}.png', expectedFile=f'expected/{stem}.png', testedThroughGui=False,
                                       paletteBefore=metrics.distributions(before, distribution), jointBefore=float(np.abs(assessment.joint(before) - reference_joint).mean()), oracleBefore=metrics.oracle_error(before, expected), clippingBefore=helpers.clipping(before))
                            started = time.perf_counter()
                            try:
                                if workflow == 'collection':
                                    row['native'] = call('apply_reference_style', photoId=photo, referenceId=refs[style]['collection']['id'], strength=.8, cancelId=str(uuid.uuid4()))
                                else:
                                    fitted = call('prepare_color_match', photoId=photo, referenceId=refs[style]['studio']['id'], cancelId=str(uuid.uuid4()), replacePrevious=True, quality='precise', intent=workflow)
                                    saved = call('apply_color_match', photoId=photo, sessionId=fitted['sessionId'], controls=dict(amount=.8, tone=1, color=1, whiteBalance=1), cancelId=str(uuid.uuid4()))
                                    assert saved['current'] and saved['fullResolutionChecked']
                                    row['native'] = helpers.compact(saved)
                            except Exception as error:
                                row['error'] = str(error)
                                assert call('image_recipe', photoId=photo)['recipeHash'] == recipe['recipeHash'], 'A refused fit changed the recipe'
                                row['refusalPreservedRecipe'] = True
                            row['applyMilliseconds'] = (time.perf_counter() - started) * 1000
                            after = render(photo)
                            preview = helpers.image_from_render(raw('native_retouch_preview', projectId=prior['projectId'], photoId=photo, before=False, quality='full'))
                            assert after.size == before.size and helpers.change(after, preview) == 0
                            after.save(out / row['afterFile'])
                            row.update(previewExportMeanDifference=0, paletteAfter=metrics.distributions(after, distribution), jointAfter=float(np.abs(assessment.joint(after) - reference_joint).mean()), oracle=metrics.oracle_error(after, expected), meanRgbChange=helpers.change(before, after), clippingAfter=helpers.clipping(after))
                            row['verdict'] = 'refused safely' if 'error' in row else 'unchanged' if row['meanRgbChange'] == 0 else 'close known edit' if row['oracle']['closeKnownLook'] else 'partial match'
                            report['pairs'].append(row)
                            artifacts(out, report)
                            print(json.dumps(dict(done=len(report['pairs']), workflow=workflow, style=style, target=index, verdict=row['verdict'], knownError=round(row['oracle']['meanDeltaE76'], 2))), flush=True)
                report['checks'] = dict(all360=len(report['pairs']) == 360,
                                       allPairsUnique=len({(x['workflow'], x['style'], x['targetIndex']) for x in report['pairs']}) == 360,
                                       originalsUnchanged=all(assessment.digest(path) == value for path, value in hashes.items()),
                                       referencesUnchanged=all(assessment.digest(ref['path']) == ref['sha256'] for ref in report['references']),
                                       noPageErrors=not report['pageErrors'], previewExportEqual=all(x['previewExportMeanDifference'] == 0 for x in report['pairs']))
                assert all(report['checks'].values())
                report['completed'] = True; report['finishedUtc'] = datetime.now(timezone.utc).isoformat()
                print(json.dumps(dict(completed=str(out), summary=report['summary'], checks=report['checks'])), flush=True)
        except BaseException as error:
            report['failure'] = f'{type(error).__name__}: {error}'
            raise
        finally:
            artifacts(out, report)
            process.terminate()
            try: process.wait(timeout=15)
            except subprocess.TimeoutExpired: process.kill(); process.wait(timeout=10)


if __name__ == '__main__':
    main()
