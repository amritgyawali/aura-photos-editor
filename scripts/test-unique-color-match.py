"""Measure AURA on one reference and ten distinct photographs in a disposable catalog.

Run prepare-unique-color-match-dataset.py first. Requires Pillow, numpy and
Playwright. Uses native fitting, full export renders and full preview renders.
Distribution scores measure palette resemblance across different scenes; they
cannot establish recovery of an unknown proprietary edit or photographic intent.
"""
import argparse
import base64
from collections import Counter
import hashlib
import html
import json
import os
from pathlib import Path
import subprocess
import time
import urllib.request
import uuid

import numpy as np
from PIL import Image, ImageDraw, ImageFont
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]


def image_from_render(value):
    return Image.frombytes('RGB', (value['width'], value['height']), base64.b64decode(value['rgbBase64']))


def lab_distribution(image):
    rgb = np.asarray(image.resize((192, 192)), dtype=np.float64) / 255
    linear = np.where(rgb <= .04045, rgb / 12.92, ((rgb + .055) / 1.055) ** 2.4)
    xyz = linear @ np.array([[.4124564, .3575761, .1804375], [.2126729, .7151522, .0721750], [.0193339, .1191920, .9503041]]).T
    xyz /= [.95047, 1, 1.08883]
    f = np.where(xyz > (6 / 29) ** 3, np.cbrt(xyz), xyz / (3 * (6 / 29) ** 2) + 4 / 29)
    lab = np.stack([116 * f[..., 1] - 16, 500 * (f[..., 0] - f[..., 1]), 200 * (f[..., 1] - f[..., 2])], axis=-1)
    return np.quantile(lab.reshape(-1, 3), [.1, .25, .5, .75, .9], axis=0)


def palette_distance(image, reference):
    return float(np.sqrt(np.square(lab_distribution(image) - reference).sum(axis=1)).mean())


def change(before, after):
    return float(np.abs(np.asarray(before, dtype=np.float32) - np.asarray(after, dtype=np.float32)).mean())


def clipping(image):
    rgb = np.asarray(image)
    return {'nearWhitePercent': float(np.any(rgb >= 254, axis=2).mean() * 100),
            'nearBlackPercent': float(np.all(rgb <= 1, axis=2).mean() * 100)}


def reduction(before, after):
    return 100 * (before - after) / max(before, 1e-9)


def compact(value):
    result = dict(value)
    result['reference'] = {key: data for key, data in value['reference'].items() if key not in ('preview', 'thumbnail')}
    return result


def artifacts(out, report, manifest):
    report.setdefault('assessmentRule', {'nativeMinimumImprovementPercent': 20, 'independentLabMinimumImprovementPercent': 10,
                                        'meaning': 'Statistical palette resemblance; no known editing ground truth or exact clone claim.'})
    (out / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    reference = next(item for item in manifest['images'] if item['role'] == 'reference')
    rows = []
    for item in report['photos']:
        standard = item['attempts'].get('standard', {})
        extreme = item['attempts'].get('extreme', {})
        metrics = lambda value: html.escape(value.get('error', f"Native {value.get('nativeReductionPercent', 0):.1f}% closer; independent Lab {value.get('labReductionPercent', 0):.1f}% closer"))
        pictures = ''.join(f'<figure><img loading="lazy" src="{html.escape(item["stem"])}.{name}.png"><figcaption>{label}{" — refused; saved edit kept" if item["attempts"].get(name, {}).get("error") else ""}</figcaption></figure>' for name, label in [('before', 'Before'), ('standard', 'Standard 100%'), ('extreme', 'Extreme 200%')])
        rows.append(f'<article><h2>{item["index"]}. {html.escape(item["description"])} · {html.escape(item["verdict"])}</h2><p><a href="{html.escape(item["sourcePage"])}">Photo {item["photoId"]} by {html.escape(item["author"])}</a></p><div class="pictures">{pictures}</div><p>Standard: {metrics(standard)}</p><p>Extreme: {metrics(extreme)}</p></article>')
    counts = report.get('summary', {})
    summary_text = f'{counts.get("strong statistical match", 0)} strong statistical matches; {counts.get("partial statistical match", 0)} partial matches; {counts.get("refused", 0)} refused.'
    summary = html.escape(summary_text)
    document = f'''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>AURA · ten unique photo assessment</title><style>body{{background:#141418;color:#eee;font:16px system-ui;margin:0;padding:32px}}main{{max-width:1500px;margin:auto}}a{{color:#d6baff}}header{{display:grid;grid-template-columns:240px 1fr;gap:28px}}header img{{width:100%;max-height:340px;object-fit:contain}}article{{border-top:1px solid #555;padding:24px 0}}.pictures{{display:grid;grid-template-columns:repeat(3,1fr);gap:12px}}figure{{margin:0}}figure img{{width:100%;height:420px;object-fit:contain;background:#08080a}}figcaption{{text-align:center}}p{{line-height:1.6}}@media(max-width:800px){{header,.pictures{{grid-template-columns:1fr}}}}</style><main><header><figure><img src="../dataset/reference.jpg"><figcaption>Reference · different from every target</figcaption></figure><div><h1>One reference. Ten different photographs.</h1><p>{summary}</p><p>Native full-resolution application and export renders, with an independent CIE Lab distribution comparison. Positive percentages mean closer to the reference. A strong statistical match requires at least 20% native-distance improvement and 10% independent palette improvement; this is a declared assessment rule, not proof of an identical editing algorithm.</p><p>All eleven source photos passed byte, normalized-pixel and perceptual duplicate screening, plus visual review. No before/after pair is used as a target/reference pair. Source images are unchanged.</p><p>Reference: <a href="{html.escape(reference['sourcePage'])}">{html.escape(reference['author'])}, Pexels {reference['photoId']}</a>. <a href="../dataset/ATTRIBUTION.md">All photo credits</a> · <a href="results.json">Raw results</a> · <a href="../dataset/manifest.json">Uniqueness evidence</a></p></div></header>{''.join(rows)}</main></html>'''
    (out / 'index.html').write_text(document, encoding='utf-8')
    if report['photos']:
        font = ImageFont.truetype('C:/Windows/Fonts/segoeui.ttf', 18)
        sheet = Image.new('RGB', (1160, 360 + len(report['photos']) * 315), '#18181c')
        draw = ImageDraw.Draw(sheet)
        with Image.open(out.parent / 'dataset/reference.jpg') as source:
            source.thumbnail((250, 280)); sheet.paste(source, (20, 45))
        draw.text((290, 60), 'AURA: one distinct reference + ten unique targets', font=font, fill='white')
        draw.text((290, 100), 'Before | Standard 100% | Extreme 200%', font=font, fill='white')
        draw.text((290, 140), summary_text, font=font, fill='#ddd')
        for row, item in enumerate(report['photos']):
            y = 360 + row * 315
            draw.text((20, y), f'{item["index"]}. {item["description"]} | {item["verdict"]}', font=font, fill='white')
            for column, name in enumerate(('before', 'standard', 'extreme')):
                with Image.open(out / f'{item["stem"]}.{name}.png') as photo:
                    photo.thumbnail((360, 260)); sheet.paste(photo, (20 + column * 380 + (360 - photo.width) // 2, y + 32))
                if item['attempts'].get(name, {}).get('error'):
                    draw.text((20 + column * 380, y + 292), 'Refused; saved edit kept', font=font, fill='#ffbd8c')
        sheet.save(out / 'comparison-sheet.jpg', quality=92)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--dataset', type=Path, default=ROOT / 'output/aura-unique-color-match-2026-10-24/dataset')
    parser.add_argument('--exe', type=Path, default=ROOT / 'target/desktop-launch/debug/aura-desktop.exe')
    parser.add_argument('--port', type=int, default=9378)
    args = parser.parse_args()
    dataset = args.dataset.resolve()
    manifest = json.loads((dataset / 'manifest.json').read_text(encoding='utf-8'))
    entries = manifest['images']
    targets = [item for item in entries if item['role'] == 'target']
    assert len(entries) == 11 and len(targets) == 10
    assert manifest['duplicateChecks']['exactDuplicates'] == 0 and not manifest['duplicateChecks']['flaggedPairs']
    assert manifest['duplicateChecks']['visualInspection']['status'] == 'reviewed'
    assert len({item['input']['pixelSha256'] for item in entries}) == 11
    hashes = {str(dataset / item[kind]['path']): item[kind]['sha256'] for item in entries for kind in ('source', 'input')}
    assert all(hashlib.sha256(Path(path).read_bytes()).hexdigest() == digest for path, digest in hashes.items())
    out = dataset.parent / ('run-' + uuid.uuid4().hex[:8])
    out.mkdir()
    report = {'photos': [], 'checks': {}, 'pageErrors': [], 'manifest': str(dataset / 'manifest.json'), 'exe': str(args.exe.resolve())}
    reference_entry = next(item for item in entries if item['role'] == 'reference')
    reference_palette = lab_distribution(Image.open(dataset / reference_entry['input']['path']).convert('RGB'))
    env = dict(os.environ, AURA_TEST_CATALOG=str(out / 'catalog.sqlite'), WEBVIEW2_USER_DATA_FOLDER=str(out / 'webview'),
               WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-port={args.port}')
    process = subprocess.Popen([str(args.exe.resolve())], cwd=ROOT, env=env)
    try:
        for _ in range(120):
            try:
                urllib.request.urlopen(f'http://127.0.0.1:{args.port}/json/version', timeout=1); break
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

            project_name = 'Ten unique photos ' + uuid.uuid4().hex[:6]
            project = call('create_project', name=project_name, coupleNames=None, eventDate=None)['id']
            call('start_ingest', projectId=project, roots=[str(dataset / item['input']['path']) for item in targets])
            photos = []
            for _ in range(180):
                photos = call('list_images', projectId=project, offset=0, limit=40, orderBy='timeline')
                if len(photos) == 10: break
                time.sleep(.5)
            assert len(photos) == len({item['id'] for item in photos}) == 10
            report['checks']['tenDistinctImportedPhotoIds'] = True
            reference = call('import_color_reference', path=str(dataset / reference_entry['input']['path']), cancelId=str(uuid.uuid4()))
            for target in targets:
                photo = next(item['id'] for item in photos if item['fileName'] == Path(target['input']['path']).name)
                before = image_from_render(call('render_image', photoId=photo, level='full', purpose='export'))
                stem = f'{target["index"]:02}-{target["photoId"]}'
                before.save(out / f'{stem}.before.png')
                item = {key: target[key] for key in ('index', 'photoId', 'author', 'description', 'sourcePage')}
                item.update(stem=stem, auraPhotoId=photo, attempts={}, labBefore=palette_distance(before, reference_palette), clippingBefore=clipping(before))
                report['photos'].append(item)
                try:
                    analysis = call('prepare_color_match', photoId=photo, referenceId=reference['id'], cancelId=str(uuid.uuid4()), quality='precise')
                    for name, amount in [('standard', 1), ('extreme', 2)]:
                        prior_hash = call('image_recipe', photoId=photo)['recipeHash']
                        try:
                            result = call('apply_color_match', photoId=photo, sessionId=analysis['sessionId'], cancelId=str(uuid.uuid4()),
                                          controls={'amount': amount, 'tone': 1, 'color': 1, 'whiteBalance': 1})
                            assert result['current'] and result['fullResolutionChecked'] and result['reference']['id'] == reference['id']
                            after = image_from_render(call('render_image', photoId=photo, level='full', purpose='export'))
                            preview = image_from_render(raw('native_retouch_preview', projectId=project, photoId=photo, before=False, quality='full'))
                            assert after.size == before.size == preview.size
                            parity = change(preview, after)
                            palette = palette_distance(after, reference_palette)
                            item['attempts'][name] = dict(report=compact(result), nativeReductionPercent=reduction(result['beforeDistance'], result['afterDistance']),
                                                         labAfter=palette, labReductionPercent=reduction(item['labBefore'], palette), meanPixelChange=change(before, after),
                                                         previewExportMeanDifference=parity, previewExportMatches=parity <= 1, clippingAfter=clipping(after))
                        except AssertionError:
                            raise
                        except Exception as error:
                            assert call('image_recipe', photoId=photo)['recipeHash'] == prior_hash, 'Refused matching changed the saved edit'
                            item['attempts'][name] = {'error': str(error), 'refusalPreservedRecipe': True}
                            after = image_from_render(call('render_image', photoId=photo, level='full', purpose='export'))
                        after.save(out / f'{stem}.{name}.png')
                except AssertionError:
                    raise
                except Exception as error:
                    item['attempts']['standard'] = {'error': str(error)}
                    item['attempts']['extreme'] = {'error': 'Analysis unavailable'}
                    for name in ('standard', 'extreme'): before.save(out / f'{stem}.{name}.png')
                standard = item['attempts']['standard']
                item['verdict'] = ('refused' if 'error' in standard else 'strong statistical match' if standard['nativeReductionPercent'] >= 20 and standard['labReductionPercent'] >= 10
                                   else 'partial statistical match' if standard['nativeReductionPercent'] > .01 else 'unchanged')
                print(json.dumps({'photo': target['photoId'], 'verdict': item['verdict'], 'nativePercent': standard.get('nativeReductionPercent'), 'labPercent': standard.get('labReductionPercent'), 'error': standard.get('error')}), flush=True)
                report['summary'] = dict(Counter(value['verdict'] for value in report['photos']))
                artifacts(out, report, manifest)
            first = targets[0]
            first_photo = report['photos'][0]['auraPhotoId']
            before_hash = call('image_recipe', photoId=first_photo)['recipeHash']
            before_session = raw('color_match_status', photoId=first_photo)['sessionId']
            duplicate = out / 'same-content-renamed.jpg'
            duplicate.write_bytes((dataset / first['input']['path']).read_bytes())
            duplicate_reference = call('import_color_reference', path=str(duplicate), cancelId=str(uuid.uuid4()))
            try:
                call('prepare_color_match', photoId=first_photo, referenceId=duplicate_reference['id'], cancelId=str(uuid.uuid4()), quality='precise')
                raise AssertionError('Identical renamed reference was accepted')
            except Exception as error:
                assert 'different' in str(error).lower() or 'same' in str(error).lower(), str(error)
            assert call('image_recipe', photoId=first_photo)['recipeHash'] == before_hash
            assert raw('color_match_status', photoId=first_photo)['sessionId'] == before_session
            report['checks']['renamedDuplicateRefusedWithoutChangingEditOrSession'] = True
            report['checks']['sourcesUnchanged'] = all(hashlib.sha256(Path(path).read_bytes()).hexdigest() == digest for path, digest in hashes.items())
            successes = [attempt for item in report['photos'] for attempt in item['attempts'].values() if 'error' not in attempt]
            report['checks']['nativePreviewMatchesExport'] = bool(successes) and all(attempt['previewExportMatches'] for attempt in successes)
            report['checks']['noPageErrors'] = not report['pageErrors']
            report['summary'] = dict(Counter(item['verdict'] for item in report['photos']))
            page.reload()
            page.get_by_role('button', name=f'{project_name} 10', exact=True).click()
            page.get_by_role('button', name='Match a look Your reference, your photos', exact=True).click()
            page.set_viewport_size({'width': 1920, 'height': 1080})
            page.get_by_alt_text('Colour reference: ' + reference['name'], exact=True).wait_for()
            page.locator('.reference-comparison').scroll_into_view_if_needed()
            page.screenshot(path=str(out / 'native-workspace.png'))
            assert all(report['checks'].values()), report['checks']
            (out.parent / 'index.html').write_text(f'<!doctype html><meta charset="utf-8"><meta http-equiv="refresh" content="0;url={out.name}/index.html"><title>AURA unique colour assessment</title><a href="{out.name}/index.html">Open the latest completed assessment</a>', encoding='utf-8')
            print(json.dumps({'output': str(out), 'summary': report['summary'], 'checks': report['checks']}, indent=2), flush=True)
    except BaseException as error:
        report['failure'] = f'{type(error).__name__}: {error}'
        raise
    finally:
        artifacts(out, report, manifest)
        process.terminate()
        try: process.wait(timeout=15)
        except subprocess.TimeoutExpired: process.kill()


if __name__ == '__main__':
    main()
