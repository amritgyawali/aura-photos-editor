"""Run the shipped desktop matcher on twenty distinct, locally cached photographs.

Collection matching uses the reference-first IPC. Natural and Palette matching
use the actual single-photo GUI controls. No matcher, renderer or IPC is mocked.
Only file import and reference library preparation use direct native commands.
"""
import argparse
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
from PIL import Image, ImageDraw, ImageOps
from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('twenty_metrics', ROOT / 'scripts/test-color-match-twenty.py')
metrics = importlib.util.module_from_spec(spec)
spec.loader.exec_module(metrics)
helpers = metrics.helpers
STYLES = metrics.STYLES


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def joint(image):
    pixels = metrics.lab(image).reshape(-1, 3)[:, 1:]
    indices = ((np.arange(2048) + .5) * len(pixels) / 2048).astype(int).clip(0, len(pixels) - 1)
    angles = np.arange(32) * 2.399963229728653
    return np.sort(pixels[indices] @ np.stack([np.cos(angles), np.sin(angles)]), axis=0)


def visible_change(before, after):
    """Mean CIE76 difference between the photograph before and after: below 1.0 nobody sees it."""
    return float(np.linalg.norm(metrics.lab(before) - metrics.lab(after), axis=-1).mean())


def new_bands(before, after):
    """Adjacent pixel pairs that were smooth (CIE76 <= 1) and now step (> 3): bands and patches a
    colour match made, anywhere in the frame. Fraction of the smooth pairs, and their count."""
    a, b = metrics.lab(before), metrics.lab(after)
    smooth = bad = 0
    for axis in (0, 1):
        da = np.linalg.norm(np.diff(a, axis=axis), axis=-1)
        db = np.linalg.norm(np.diff(b, axis=axis), axis=-1)
        mask = da <= 1.0
        smooth += int(mask.sum())
        bad += int((mask & (db > 3.0)).sum())
    return dict(pairs=bad, fraction=bad / max(smooth, 1))


def new_steps(before, after):
    """New steps between flat colour: adjacent pairs smooth before (CIE76 <= 1) that step after (> 3)
    with smooth pairs (<= 1.5) on both sides - bands and posterised blocks, not texture a strong
    look brings into view."""
    a, b = metrics.lab(before), metrics.lab(after)
    smooth = bad = 0
    for axis in (0, 1):
        da = np.linalg.norm(np.diff(a, axis=axis), axis=-1)
        db = np.linalg.norm(np.diff(b, axis=axis), axis=-1)
        n = db.shape[axis]
        cut = lambda s, e: tuple(slice(s, e) if i == axis else slice(None) for i in range(2))
        flat = np.zeros_like(db, dtype=bool)
        flat[cut(2, n - 2)] = (db[cut(0, n - 4)] <= 1.5) & (db[cut(4, n)] <= 1.5)
        mask = da <= 1.0
        smooth += int(mask.sum())
        bad += int((mask & (db > 3.0) & flat).sum())
    return dict(pairs=bad, fraction=bad / max(smooth, 1))


def save_report(out, report):
    (out / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    parts = ['<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width">',
             '<title>AURA: twenty-photo desktop matching assessment</title>',
             '<style>body{background:#18181d;color:#eee;font:16px system-ui;margin:26px}a{color:#dac2ff}.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(210px,1fr));gap:12px}img{width:100%;height:290px;object-fit:contain;background:#08080a}figure{margin:0}article{border-top:1px solid #777;margin:26px 0}p{line-height:1.5}@media(max-width:800px){.grid{grid-template-columns:1fr 1fr}}</style>',
             '<h1>Twenty-photo desktop Match a look test</h1>',
             '<p>Collection reference matching and the single-photo Natural/Palette GUI. All matches request 80% strength. References are different photographs. Independent colour distance measures resemblance, not recovery of an unknown editing operation. Known-edit comparisons use a declared 80% encoded-sRGB blend; native parameter interpolation can differ. Close known edit means mean CIE76 ≤3 and p95 ≤8, authored QA thresholds.</p>',
             '<p><a href="results.json">Measurements and checks</a></p>']
    for row in report['pairs']:
        stem = row['stem']
        parts.append(f'<article><h2>{row["targetIndex"]:02}. {html.escape(row["description"])} — {row["workflow"]} / {row["style"]}</h2>')
        parts.append('<p>' + html.escape(row.get('error', row.get('verdict', ''))) + '</p><div class="grid">')
        pictures = [(f'references/{row["style"]}.png', 'Different edited reference')]
        original_file = f'{row["targetIndex"]:02}.original.png'
        if (out / original_file).exists():
            pictures.append((original_file, 'Unedited native original'))
        pictures.extend([(f'{stem}.before.png', 'Native workflow baseline'),
                         (f'{stem}.matched.png', 'Actual saved AURA result'),
                         (f'{stem}.expected.png', 'Declared known 80% edit')])
        for path, label in pictures:
            parts.append(f'<figure><a href="{path}"><img loading="lazy" src="{path}"></a><figcaption>{label}</figcaption></figure>')
        parts.append('</div>')
        if 'paletteAfter' in row:
            parts.append(f'<p>Reference colour distance {row["paletteBefore"]["color"]:.3f} → {row["paletteAfter"]["color"]:.3f}; joint a/b distance {row["jointBefore"]:.3f} → {row["jointAfter"]:.3f}. Known-edit error {row["oracleBefore"]["meanDeltaE76"]:.3f} → {row["oracle"]["meanDeltaE76"]:.3f}, p95 {row["oracle"]["p95DeltaE76"]:.3f}. Apply {row["applyMilliseconds"] / 1000:.2f}s.</p>')
        if 'originalMeanRgbChange' in row:
            parts.append(f'<p>Output versus unedited original: mean RGB8 change {row["originalMeanRgbChange"]:.3f}; {row["originalChangedPixelFraction"] * 100:.2f}% of pixels differ. Mean CIE76 {row["originalMeanDeltaE76"]:.3f}; tone change {row["originalMeanToneChange"]:.3f}; chromatic change {row["originalMeanChromaChange"]:.3f}. Identical: {row["originalPixelIdentical"]}.</p>')
        parts.append('</article>')
    (out / 'index.html').write_text('\n'.join(parts), encoding='utf-8')


def contact_sheets(out, report):
    for workflow in report['workflows']:
        rows = [row for row in report['pairs'] if row['workflow'] == workflow]
        for offset in range(0, len(rows), 5):
            selected = rows[offset:offset + 5]
            sheet = Image.new('RGB', (1000, len(selected) * 280), '#202027')
            draw = ImageDraw.Draw(sheet)
            for index, row in enumerate(selected):
                y = index * 280
                draw.text((8, y + 4), f'{row["targetIndex"]:02} {row["description"]} / {row["style"]}', fill='white')
                paths = [f'references/{row["style"]}.png', f'{row["stem"]}.before.png', f'{row["stem"]}.matched.png', f'{row["stem"]}.expected.png']
                for column, (path, label) in enumerate(zip(paths, ['Reference', 'Baseline', 'Actual', 'Known edit'])):
                    with Image.open(out / path) as image:
                        thumbnail = ImageOps.contain(image.convert('RGB'), (244, 235))
                        sheet.paste(thumbnail, (column * 250 + (250 - thumbnail.width) // 2, y + 25))
                    draw.text((column * 250 + 8, y + 262), label, fill='#ddddee')
            sheet.save(out / f'{workflow}-contact-{offset // 5 + 1}.jpg', quality=94)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exe', type=Path, default=ROOT / 'target/desktop-launch/debug/aura-desktop.exe')
    parser.add_argument('--dataset', type=Path, default=ROOT / 'output/aura-twenty-color-match-2026-10-24/dataset')
    parser.add_argument('--limit', type=int, choices=range(1, 21), default=20)
    parser.add_argument('--workflows', nargs='+', choices=['collection', 'natural', 'palette', 'learned', 'exact'], default=['collection', 'natural', 'palette'])
    parser.add_argument('--out-base', type=Path, default=ROOT / 'output/aura-main-match-twenty-2026-10-25',
                        help='Folder the run directory is created in.')
    parser.add_argument('--port', type=int, default=9397)
    parser.add_argument('--resume', type=Path, help='Continue this harness\'s own isolated run, retaining completed cases.')
    args = parser.parse_args()
    base = args.out_base.resolve()
    base.mkdir(parents=True, exist_ok=True)
    out = args.resume.resolve() if args.resume else base / ('run-' + uuid.uuid4().hex[:8])
    assert out.parent == base.resolve() and out.name.startswith('run-')
    (out / 'references').mkdir(parents=True, exist_ok=bool(args.resume))
    dataset = args.dataset.resolve()
    manifest = json.loads((dataset / 'manifest.json').read_text(encoding='utf-8'))
    targets = [x for x in manifest['images'] if x['role'] == 'target'][:args.limit]
    references = [x for x in manifest['images'] if x['role'] == 'reference']
    assert len({x['photoId'] for x in targets + references}) == len(targets + references)
    hashes = {str(dataset / x[k]['path']): x[k]['sha256'] for x in manifest['images'] for k in ('source', 'input')}
    assert all(digest(path) == value for path, value in hashes.items()), 'Cached input integrity failed'
    report = dict(startedUtc=datetime.now(timezone.utc).isoformat(), run=str(out), exe=str(args.exe.resolve()),
                  exeSha256=digest(args.exe), workflows=args.workflows, requestedStrength=.8, targetCount=len(targets),
                  methodology='Native desktop WebView2. Collection uses analyse_reference_style/apply_reference_style; Natural and Palette use single-photo GUI controls. One cyclically assigned reference per target, with six different edited source photographs. Main and studio baselines are measured separately. No oracle pixels or grade parameters are supplied to matching.',
                  pairs=[], references=[], deliveries=[], controls=[], pageErrors=[], checks={})
    if args.resume:
        saved = json.loads((out / 'results.json').read_text(encoding='utf-8'))
        assert saved['exeSha256'] == report['exeSha256']
        assert saved['workflows'] == args.workflows and saved['targetCount'] == len(targets)
        report = saved
        report['resumedUtc'] = datetime.now(timezone.utc).isoformat()
        report.setdefault('previousHarnessFailures', []).append(report.pop('failure', None))
        report.pop('completed', None)
        reference_map = {ref['style']: ref for ref in report['references']}
        assert all(digest(ref['path']) == ref['sha256'] for ref in report['references'])
    else:
        reference_map = {}
        for style, entry in zip(STYLES, references):
            path = out / 'references' / f'{style}.png'
            metrics.grade(Image.open(dataset / entry['input']['path']), style).save(path)
            reference_map[style] = dict(style=style, photoId=entry['photoId'], path=str(path), source=entry, sha256=digest(path))
        report['references'] = list(reference_map.values())
    save_report(out, report)
    print(json.dumps(dict(started=str(out), targets=len(targets), workflows=args.workflows)), flush=True)
    env = dict(os.environ, AURA_TEST_CATALOG=str(out / 'catalog.sqlite'), WEBVIEW2_USER_DATA_FOLDER=str(out / 'webview'),
               WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-address=127.0.0.1 --remote-debugging-port={args.port}')
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    with (out / 'desktop.log').open('a', encoding='utf-8') as desktop_log:
        process = subprocess.Popen([str(args.exe.resolve())], cwd=ROOT, env=env, startupinfo=startup, stdout=desktop_log, stderr=subprocess.STDOUT)
        report['desktopPid'] = process.pid
        try:
            for _ in range(90):
                if process.poll() is not None:
                    raise RuntimeError(f'Desktop exited during startup: {process.returncode}')
                try:
                    urllib.request.urlopen(f'http://127.0.0.1:{args.port}/json/version', timeout=1).close()
                    break
                except OSError:
                    time.sleep(1)
            with sync_playwright() as pw:
                browser = pw.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}')
                page = next(p for context in browser.contexts for p in context.pages)
                page.set_default_timeout(120000)
                expect.set_options(timeout=120000)
                page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
                page.on('pageerror', lambda error: report['pageErrors'].append(str(error)))
                page.set_viewport_size(dict(width=1920, height=1080))
                page.screenshot(path=str(out / 'startup.png'))

                def call(command, **kwargs):
                    return page.evaluate('async ([c,a])=>{try{return await window.__TAURI_INTERNALS__.invoke(c,{input:a})}catch(e){throw new Error(JSON.stringify(e))}}', [command, kwargs])

                def raw(command, **kwargs):
                    return page.evaluate('async ([c,a])=>window.__TAURI_INTERNALS__.invoke(c,a)', [command, kwargs])

                def rendered(photo):
                    return helpers.image_from_render(call('render_image', photoId=photo, level='full', purpose='export'))

                name = 'Twenty-photo Match a look ' + out.name
                project = report['projectId'] if args.resume else call('create_project', name=name, coupleNames=None, eventDate=None)['id']
                report['projectId'] = project
                if not args.resume:
                    call('start_ingest', projectId=project, roots=[str(dataset / x['input']['path']) for x in targets])
                deadline = time.monotonic() + 120
                while time.monotonic() < deadline:
                    photos = call('list_images', projectId=project, offset=0, limit=60, orderBy='timeline')
                    if len(photos) == len(targets):
                        break
                    time.sleep(.5)
                assert len(photos) == len(targets)
                originals = {}
                for target in targets:
                    photo = next(x['id'] for x in photos if x['fileName'] == Path(target['input']['path']).name)
                    target['auraPhotoId'] = photo
                    if not args.resume:
                        call('set_param', projectId=project, photoId=photo, path='global.exposure', value=.125, label='Isolated test setup')
                        call('history_step', projectId=project, photoId=photo, action='reset_original')
                        originals[photo] = rendered(photo)
                        originals[photo].save(out / f'{target["index"]:02}.original.png')
                    else:
                        originals[photo] = Image.open(out / f'{target["index"]:02}.original.png').convert('RGB')
                if not args.resume:
                    for reference in report['references']:
                        reference['collection'] = call('analyse_reference_style', address=reference['style'], folder=reference['path'], cancelId=str(uuid.uuid4()), export=False)
                        reference['studio'] = call('import_color_reference', path=reference['path'], cancelId=str(uuid.uuid4()))
                        assert not reference['collection'].get('calibration'), 'Unpaired test must not receive original reference pixels'
                if 'learned' in args.workflows and not report.get('learnedReferences'):
                    report['learnedReferences'] = {}
                    for target in targets:
                        source = targets[target['index'] % len(targets)]
                        style = STYLES[(target['index'] - 1) % len(STYLES)]
                        stem = Path(source['input']['path']).stem
                        path = out / 'references' / f'{stem}-Edit.png'
                        if not path.exists():
                            metrics.grade(Image.open(dataset / source['input']['path']), style).save(path)
                        studio = call('import_color_reference', path=str(path), cancelId=str(uuid.uuid4()))
                        report['learnedReferences'][str(target['index'])] = dict(style=style, path=str(path), studio=studio,
                                                                                original=Path(source['input']['path']).name,
                                                                                photoId=source['photoId'], sha256=digest(path))
                save_report(out, report)
                for workflow in args.workflows:
                    if sum(row['workflow'] == workflow for row in report['pairs']) == len(targets) and any(item['workflow'] == workflow for item in report['deliveries']):
                        continue
                    for target in targets:
                        if any(row['workflow'] == workflow and row['targetIndex'] == target['index'] for row in report['pairs']):
                            continue
                        style = STYLES[(target['index'] - 1) % len(STYLES)]
                        ref = reference_map[style] if workflow != 'learned' else report['learnedReferences'][str(target['index'])]
                        goal = 'natural' if workflow == 'learned' else workflow
                        photo = target['auraPhotoId']
                        call('history_step', projectId=project, photoId=photo, action='reset_original')
                        if workflow == 'collection':
                            call('apply_reference_style', photoId=photo, referenceId=ref['collection']['id'], strength=0, cancelId=str(uuid.uuid4()))
                        before = rendered(photo)
                        reset_hash = call('image_recipe', photoId=photo)['recipeHash']
                        stem = f'{target["index"]:02}.{workflow}.{style}'
                        before.save(out / f'{stem}.before.png')
                        full = metrics.grade(before, style)
                        expected = Image.fromarray(np.rint(.2 * np.asarray(before, dtype=float) + .8 * np.asarray(full, dtype=float)).astype('uint8'))
                        expected.save(out / f'{stem}.expected.png')
                        with Image.open(ref['path']) as ref_image:
                            ref_image = ref_image.convert('RGB')
                            distribution = helpers.lab_distribution(ref_image)
                            reference_joint = joint(ref_image)
                        row = dict(targetIndex=target['index'], description=target['description'], workflow=workflow, style=style,
                                   photoId=target['photoId'], referencePhotoId=ref['photoId'], auraPhotoId=photo, stem=stem,
                                   paletteBefore=metrics.distributions(before, distribution), oracleBefore=metrics.oracle_error(before, expected),
                                   jointBefore=float(np.abs(joint(before) - reference_joint).mean()), clippingBefore=helpers.clipping(before))
                        started = time.perf_counter()
                        if workflow == 'collection':
                            try:
                                row['native'] = call('apply_reference_style', photoId=photo, referenceId=ref['collection']['id'], strength=.8, cancelId=str(uuid.uuid4()))
                            except Exception as error:
                                row['error'] = str(error)
                        else:
                            page.reload()
                            page.get_by_role('button', name=f'{name} {len(targets)}', exact=True).click()
                            page.get_by_role('button', name='Match a look Your reference, your photos', exact=True).click()
                            page.get_by_role('listbox', name='Filmstrip').get_by_role('option', name=Path(target['input']['path']).name, exact=True).click()
                            panel = page.get_by_role('region', name='Reference colour matching', exact=True)
                            expect(panel).to_be_visible()
                            panel.get_by_label('Saved colour reference', exact=True).select_option(ref['studio']['id'])
                            live = panel.get_by_role('checkbox', name='Real-Time Color Adjustments')
                            expect(live).not_to_be_checked()
                            panel.get_by_label('Matching mode', exact=True).select_option('precise')
                            panel.get_by_label('Matching goal', exact=True).select_option(goal)
                            panel.get_by_role('slider', name='Match amount', exact=True).evaluate("el=>{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(el,'80');el.dispatchEvent(new Event('input',{bubbles:true}));}")
                            previous = raw('color_match_status', photoId=photo)
                            previous_session = previous['sessionId'] if previous else None
                            started = time.perf_counter()
                            button = panel.get_by_role('button', name='Match reference to this photo', exact=True)
                            button.click()
                            deadline = time.monotonic() + 180
                            while time.monotonic() < deadline:
                                status = raw('color_match_status', photoId=photo)
                                if status and status['sessionId'] != previous_session and status['current'] and status['fullResolutionChecked']:
                                    assert status['reference']['id'] == ref['studio']['id']
                                    assert status['fitting'].get('intent', 'natural') == goal
                                    # Apply may save a smaller share when 80 % fails this photo's checks.
                                    assert status['controls']['amount'] <= .8 + .001
                                    row['native'] = helpers.compact(status)
                                    break
                                alerts = page.get_by_role('alert').all_text_contents()
                                if alerts:
                                    row['error'] = ' '.join(alerts)
                                    row['uiProgressAfterRefusal'] = panel.get_by_role('status').all_text_contents()
                                    break
                                time.sleep(.25)
                            else:
                                raise AssertionError(f'GUI match did not finish: {workflow}, target {target["index"]}')
                            expect(button).to_be_enabled()
                            if 'error' not in row:
                                # The app measures how different the saved result looks from the
                                # original. A result nobody can see must be reported as such, and a
                                # visible one must not be.
                                messages = panel.get_by_role('status').all_text_contents()
                                visible = row['native'].get('visibleChange')
                                slight = any('looks almost the same' in message for message in messages)
                                claims_match = any(message.startswith('Matched ') for message in messages)
                                row['panelMessages'] = messages
                                row['appVisibleChange'] = visible
                                row['slightReportedHonestly'] = visible is not None and (slight == (visible < 0.45)) and not (slight and claims_match)
                                assert row['slightReportedHonestly'], f'Panel did not report the visible change honestly: {visible} {messages}'
                            if 'error' in row:
                                row['terminalUiProgress'] = panel.get_by_role('status').all_text_contents()
                                row['refusalClearsMatchingProgress'] = not any('Matching the selected reference' in message for message in row['terminalUiProgress'])
                                assert row['refusalClearsMatchingProgress'], 'Refused match retained a running progress message'
                                # The panel itself must say why nothing was saved, in its own words,
                                # and no refusal may reach the photographer as a generic failure.
                                for _ in range(40):
                                    panel_alerts = panel.get_by_role('alert').all_text_contents()
                                    if panel_alerts:
                                        break
                                    time.sleep(.25)
                                row['panelRefusal'] = ' '.join(panel_alerts)
                                row['refusalExplained'] = bool(row['panelRefusal']) and not any('unexpected problem' in text for text in page.get_by_role('alert').all_text_contents())
                                assert row['refusalExplained'], f'Refusal not explained: {page.get_by_role("alert").all_text_contents()}'
                            if target['index'] in [1, 11, 17, 20]:
                                panel.scroll_into_view_if_needed()
                                page.screenshot(path=str(out / f'{stem}.ui.png'), full_page=True)
                        row['applyMilliseconds'] = (time.perf_counter() - started) * 1000
                        if 'error' in row:
                            row['refusalPreservedRecipe'] = call('image_recipe', photoId=photo)['recipeHash'] == reset_hash
                            assert row['refusalPreservedRecipe'], 'A refused match changed the saved recipe'
                        after = rendered(photo)
                        preview = helpers.image_from_render(raw('native_retouch_preview', projectId=project, photoId=photo, before=False, quality='full'))
                        assert before.size == after.size == preview.size
                        row['previewExportMeanDifference'] = helpers.change(after, preview)
                        assert row['previewExportMeanDifference'] == 0
                        after.save(out / f'{stem}.matched.png')
                        original = originals[photo]
                        original_metrics = metrics.oracle_error(after, original)
                        row['originalPixelIdentical'] = original_metrics['bitwiseIdentical']
                        row['originalMeanRgbChange'] = helpers.change(original, after)
                        row['originalMeanDeltaE76'] = original_metrics['meanDeltaE76']
                        row['originalP95DeltaE76'] = original_metrics['p95DeltaE76']
                        row['originalMeanToneChange'] = original_metrics['meanAbsToneError']
                        row['originalMeanChromaChange'] = original_metrics['meanChromaPlaneError']
                        row['originalChangedPixelFraction'] = float(np.any(np.asarray(after) != np.asarray(original), axis=2).mean())
                        row.update(oracle=metrics.oracle_error(after, expected), paletteAfter=metrics.distributions(after, distribution),
                                   jointAfter=float(np.abs(joint(after) - reference_joint).mean()), meanRgbChange=helpers.change(before, after), clippingAfter=helpers.clipping(after),
                                   visibleChange=visible_change(before, after), newBands=new_bands(before, after),
                                   newSteps=new_steps(before, after))
                        transfer = (row.get('native') or {}).get('fitting', {}).get('transfer', {}) if isinstance(row.get('native'), dict) else {}
                        row['learnedFrom'] = transfer.get('learnedFrom')
                        row['originalRefused'] = transfer.get('originalRefused')
                        row['verdict'] = ('refused safely' if 'error' in row else 'unchanged' if row['meanRgbChange'] == 0
                                          else 'no visible change' if row['visibleChange'] < 1.0
                                          else 'close known edit' if row['oracle']['closeKnownLook'] else 'partial match')
                        report['pairs'].append(row)
                        save_report(out, report)
                        print(json.dumps(dict(workflow=workflow, target=target['index'], style=style, verdict=row['verdict'], colorBefore=round(row['paletteBefore']['color'], 3), colorAfter=round(row['paletteAfter']['color'], 3), knownEditBefore=round(row['oracleBefore']['meanDeltaE76'], 3), knownEditError=round(row['oracle']['meanDeltaE76'], 3), visibleChange=round(row['visibleChange'], 2), newBandPairs=row['newBands']['pairs'], learnedFrom=row.get('learnedFrom'), seconds=round(row['applyMilliseconds'] / 1000, 2), error=row.get('error'))), flush=True)
                    destination = out / 'delivery' / workflow
                    delivery = call('export_run', projectId=project, destination=str(destination), destinationKind='folder',
                                    sets=[dict(name='Match verification', imageIds=[x['auraPhotoId'] for x in targets], format='png', quality=95, colour='srgb', bitDepth=8, resize='full', sharpen='none', naming='{seq}_{original}', sidecar=False)],
                                    copyright=None, contact=None, creator=None, keywords=[], stripGps=True, stripCameraSerial=True, verify=True)
                    assert delivery['requested'] == delivery['written'] == delivery['verified'] == len(targets)
                    assert delivery['manifestSealed'] and delivery['renderFailed'] == delivery['corrupt'] == 0
                    files = raw('export_files', projectId=project)
                    assert len(files) == len(targets)
                    for file in files:
                        row = next(x for x in report['pairs'] if x['workflow'] == workflow and x['auraPhotoId'] == file['imageId'])
                        with Image.open(destination / file['path']) as exported, Image.open(out / f'{row["stem"]}.matched.png') as saved:
                            assert helpers.change(exported.convert('RGB'), saved.convert('RGB')) == 0
                    report['deliveries'].append(dict(workflow=workflow, native=delivery, allPixelsEqualSavedRender=True))
                    # Verify native undo/redo without changing the completed delivery.
                    row = next(x for x in report['pairs'] if x['workflow'] == workflow and 'error' not in x and x['meanRgbChange'] > 0)
                    photo = row['auraPhotoId']
                    call('history_step', projectId=project, photoId=photo, action='undo')
                    with Image.open(out / f'{row["stem"]}.before.png') as before:
                        undo_equal = helpers.change(rendered(photo), before.convert('RGB')) == 0
                    call('history_step', projectId=project, photoId=photo, action='redo')
                    with Image.open(out / f'{row["stem"]}.matched.png') as matched:
                        redo_equal = helpers.change(rendered(photo), matched.convert('RGB')) == 0
                    report['controls'].append(dict(workflow=workflow, target=row['targetIndex'], undoRestoresBaseline=undo_equal, redoRestoresMatch=redo_equal))
                    assert undo_equal and redo_equal
                    contact_sheets(out, report)
                    save_report(out, report)
                report['checks'] = dict(allMatchesAttempted=len(report['pairs']) == len(targets) * len(args.workflows),
                                       differentReferences=all(row['photoId'] != row['referencePhotoId'] for row in report['pairs']),
                                       previewExportEqual=all(row['previewExportMeanDifference'] == 0 for row in report['pairs']),
                                       originalsUnchanged=all(digest(path) == value for path, value in hashes.items()),
                                       referenceFilesUnchanged=all(digest(ref['path']) == ref['sha256'] for ref in list(report['references']) + list(report.get('learnedReferences', {}).values())),
                                       allDeliveriesVerified=len(report['deliveries']) == len(args.workflows), noPageErrors=not report['pageErrors'],
                                       visibleChangeReportedHonestly=all(row.get('slightReportedHonestly', True) for row in report['pairs']),
                                       refusalsExplained=all(row.get('refusalExplained', True) for row in report['pairs']))
                assert all(report['checks'].values())
                # These are product requirements under test, independent of file/workflow
                # integrity. Record failures rather than hiding unchanged/refused photos.
                report['outputRequirements'] = dict(
                    allFinalOutputsDifferFromOriginal=all(not row['originalPixelIdentical'] for row in report['pairs']),
                    everyReferenceStageChangesPixels=all(row['meanRgbChange'] > 0 for row in report['pairs']),
                    allRequestedMatchesSaved=all('error' not in row for row in report['pairs']),
                    unchangedOriginalCases=[dict(target=row['targetIndex'], workflow=row['workflow'], style=row['style'], error=row.get('error')) for row in report['pairs'] if row['originalPixelIdentical']],
                    unchangedReferenceStageCases=[dict(target=row['targetIndex'], workflow=row['workflow'], style=row['style'], error=row.get('error')) for row in report['pairs'] if row['meanRgbChange'] == 0])
                report['summary'] = []
                for workflow in args.workflows:
                    rows = [row for row in report['pairs'] if row['workflow'] == workflow]
                    report['summary'].append(dict(workflow=workflow, attempted=len(rows), refused=sum('error' in row for row in rows),
                                                  changed=sum(row['meanRgbChange'] > 0 for row in rows),
                                                  referenceColorImproved=sum(row['paletteAfter']['color'] + 1e-4 < row['paletteBefore']['color'] for row in rows),
                                                  jointColorImproved=sum(row['jointAfter'] + 1e-4 < row['jointBefore'] for row in rows),
                                                  knownEditImproved=sum(row['oracle']['meanDeltaE76'] + 1e-4 < row['oracleBefore']['meanDeltaE76'] for row in rows),
                                                  closeKnownEdit=sum(row['oracle']['closeKnownLook'] for row in rows),
                                                  originalChanged=sum(not row['originalPixelIdentical'] for row in rows),
                                                  closeKnownEditChanged=sum(row['oracle']['closeKnownLook'] and row['visibleChange'] >= 1.0 for row in rows),
                                                  noVisibleChange=sum('error' not in row and row['visibleChange'] < 1.0 for row in rows),
                                                  withNewBands=sum(row['newBands']['fraction'] > 0.005 for row in rows),
                                                  # The stricter count: steps between flat colour (assessment's banded sky: 0.0077).
                                                  withNewSteps=sum(row['newSteps']['fraction'] > 0.001 for row in rows),
                                                  learnedFromOriginal=sum(bool(row.get('learnedFrom')) for row in rows),
                                                  medianKnownEditError=statistics.median(row['oracle']['meanDeltaE76'] for row in rows),
                                                  medianKnownEditBefore=statistics.median(row['oracleBefore']['meanDeltaE76'] for row in rows),
                                                  # How close the photo's colours came to the reference's own (the Exact goal's aim).
                                                  medianReferenceColorBefore=statistics.median(row['paletteBefore']['color'] for row in rows),
                                                  medianReferenceColorAfter=statistics.median(row['paletteAfter']['color'] for row in rows),
                                                  medianApplySeconds=statistics.median(row['applyMilliseconds'] / 1000 for row in rows)))
                report['completed'] = True
                print(json.dumps(dict(completed=str(out), summary=report['summary'], checks=report['checks'])), flush=True)
        except BaseException as error:
            report['failure'] = f'{type(error).__name__}: {error}'
            raise
        finally:
            report['desktopExitBeforeCleanup'] = process.poll()
            save_report(out, report)
            process.terminate()
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=10)


if __name__ == '__main__':
    main()
