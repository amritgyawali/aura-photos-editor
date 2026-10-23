"""Measure reference-look transfer against known AURA edits, without rating perfection."""
import argparse
import base64
import copy
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
from PIL import Image, ImageCms
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]
STYLES = {
    'warm': {'global.temperature': ('add', 1800), 'global.tint': ('add', 8), 'global.exposure': ('add', .15)},
    'cool': {'global.temperature': ('add', -1300), 'global.tint': ('add', -5), 'global.contrast': ('add', 12)},
    'matte': {'global.curve.points': [[0, 20], [64, 75], [128, 135], [192, 195], [255, 245]], 'global.saturation': ('add', -20)},
    'split-tone': {'global.colour_grade.shadows.hue': 190, 'global.colour_grade.shadows.saturation': 25,
                   'global.colour_grade.highlights.hue': 35, 'global.colour_grade.highlights.saturation': 18},
    'punchy': {'global.contrast': ('add', 25), 'global.saturation': ('add', 25), 'global.exposure': ('add', -.1)},
    'monochrome': {'bw': {'mix': {}, 'grade': None}},
}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def lab(rgb):
    srgb = rgb.astype(np.float64) / 255
    linear = np.where(srgb <= .04045, srgb / 12.92, ((srgb + .055) / 1.055) ** 2.4)
    xyz = linear @ np.array([[.4124564, .2126729, .0193339], [.3575761, .7151522, .119192],
                             [.1804375, .072175, .9503041]])
    xyz /= np.array([.95047, 1, 1.08883])
    f = np.where(xyz > (6 / 29) ** 3, np.cbrt(xyz), xyz / (3 * (6 / 29) ** 2) + 4 / 29)
    return np.stack([116 * f[..., 1] - 16, 500 * (f[..., 0] - f[..., 1]),
                     200 * (f[..., 1] - f[..., 2])], axis=-1)


def errors(actual, expected):
    delta = lab(actual) - lab(expected)
    de = np.linalg.norm(delta, axis=-1)
    rgb = np.abs(actual.astype(np.float64) - expected.astype(np.float64))
    return dict(rgbMae=float(rgb.mean()), meanDeltaE76=float(de.mean()),
                p95DeltaE76=float(np.quantile(de, .95)), lightnessMae=float(np.abs(delta[..., 0]).mean()),
                exactPixels=bool(np.array_equal(actual, expected)))


def guarded_refusal(error):
    return any(text in str(error) for text in ['failed the full-resolution highlight or skin-colour check',
                                              'farther from the reference'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--photos', type=Path, nargs=3, required=True)
    parser.add_argument('--port', type=int, default=9750)
    parser.add_argument('--styles', choices=list(STYLES), nargs='+', default=list(STYLES))
    args = parser.parse_args()
    out = args.output.resolve()
    if out.exists():
        parser.error('Use a fresh output directory to retain earlier attempts')
    (out / 'catalog').mkdir(parents=True)
    (out / 'incoming').mkdir()
    sources = {str(p.resolve()): digest(p) for p in args.photos}
    for i, path in enumerate(args.photos):
        with Image.open(path) as original:
            image = original.convert('RGB')
            image.thumbnail((512, 512))
            image.save(out / f'incoming/{i}.png')
    report = dict(cases=[], formats=[], pageErrors=[], commandCounts={}, sources=sources,
                  executableSha256=digest(ROOT / 'target/desktop-launch/debug/aura-desktop.exe'),
                  scriptSha256=digest(Path(__file__)), styles={key: STYLES[key] for key in args.styles},
                  metric='CIE76 per aligned pixel against the known grade on this target; lower is closer, not a perfection percentage')

    def persist():
        (out / 'results.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')

    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    proc = subprocess.Popen([str(ROOT / 'target/desktop-launch/debug/aura-desktop.exe')], cwd=ROOT,
        startupinfo=startup, env=dict(os.environ, AURA_TEST_CATALOG=str(out / 'catalog/catalog.sqlite'),
        AURA_IMAGE_WORKERS='1', WEBVIEW2_USER_DATA_FOLDER=str(out / 'webview'),
        WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-port={args.port}'))
    try:
        for _ in range(120):
            try:
                urllib.request.urlopen(f'http://127.0.0.1:{args.port}/json/version', timeout=1)
                break
            except OSError:
                time.sleep(1)
        else:
            raise TimeoutError('Desktop did not start')
        with sync_playwright() as pw:
            page = pw.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}').contexts[0].pages[0]
            page.set_default_timeout(600000)
            page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
            page.on('pageerror', lambda e: report['pageErrors'].append(str(e)))

            def call(command, **values):
                report['commandCounts'][command] = report['commandCounts'].get(command, 0) + 1
                return page.evaluate('''async ([command,input]) => {
                    try{return await window.__TAURI_INTERNALS__.invoke(command,{input})}
                    catch(e){throw Error(JSON.stringify(e))}
                }''', [command, values])

            project = call('create_project', name='Reference grade transfer verification', coupleNames=None, eventDate=None)['id']
            call('start_ingest', projectId=project, roots=[str(out / 'incoming')])
            for _ in range(120):
                photos = call('list_images', projectId=project, offset=0, limit=10, orderBy=None)
                if len(photos) == 3:
                    break
                time.sleep(.5)
            assert len(photos) == 3
            photos.sort(key=lambda p: p['fileName'])
            report['photos'] = photos

            def recipe(photo):
                return call('image_recipe', photoId=photo)

            def render(photo, path=None):
                frame = call('render_image', photoId=photo, level='full', purpose='export')
                rgb = np.frombuffer(base64.b64decode(frame['rgbBase64']), dtype=np.uint8).reshape(frame['height'], frame['width'], 3)
                if path:
                    Image.fromarray(rgb).save(path)
                return rgb

            baseline = {}
            for photo in photos:
                pid = photo['id']
                call('photo_auto_edit', projectId=project, photoId=pid, jobId=str(uuid.uuid4()))
                saved = recipe(pid)
                assert not saved['userEditedFields'], 'The matcher must not inherit manual protection from the fixture setup'
                baseline[pid] = (saved, render(pid, out / (photo['fileName'] + '-before.png')))
            for style in args.styles:
                parameters = STYLES[style]
                expected = {}
                for photo in photos:
                    pid = photo['id']
                    base, _ = baseline[pid]
                    values = json.loads(base['body'])
                    applied = 0
                    for path, value in parameters.items():
                        if isinstance(value, tuple):
                            current = values
                            for part in path.split('.'):
                                current = current[part]
                            value = current + value[1]
                        previous = recipe(pid)['recipeHash']
                        saved = call('set_param', projectId=project, photoId=pid, path=path, value=value, label='Known reference grade ' + style)
                        if recipe(pid)['recipeHash'] != previous:
                            applied += 1
                    expected[pid] = render(pid, out / f'{style}-{photo["fileName"]}-expected.png')
                    for _ in range(applied):
                        call('history_step', projectId=project, photoId=pid, action='undo')
                    assert recipe(pid)['recipeHash'] == base['recipeHash']
                    assert np.array_equal(render(pid), baseline[pid][1])
                extension = {'warm': 'jpg', 'cool': 'tiff'}.get(style, 'png')
                ref_path = out / f'{style}-reference.{extension}'
                Image.fromarray(expected[photos[0]['id']]).save(ref_path, **({'quality': 98} if extension == 'jpg' else {}))
                reference = call('import_color_reference', path=str(ref_path), cancelId=str(uuid.uuid4()))
                report.setdefault('references', {})[style] = {key: value for key, value in reference.items() if key != 'thumbnail'}
                for photo in photos:
                    pid = photo['id']
                    base, pixels = baseline[pid]
                    case = dict(style=style, target=photo['fileName'], sameImage=photo == photos[0],
                                baselineToKnownGrade=errors(pixels, expected[pid]))
                    report['cases'].append(case)
                    try:
                        history_before = call('image_history', photoId=pid)
                        prepared = call('prepare_color_match', photoId=pid, referenceId=reference['id'], cancelId=str(uuid.uuid4()))
                        assert call('image_history', photoId=pid) == history_before, 'Analysis modified history'
                        assert recipe(pid)['recipeHash'] == base['recipeHash']
                        controls = dict(amount=1, tone=1, color=1, whiteBalance=1)

                        def apply(**updates):
                            return call('apply_color_match', photoId=pid, sessionId=prepared['sessionId'],
                                        controls=dict(controls, **updates), cancelId=str(uuid.uuid4()))

                        case['strengthAttempts'] = []
                        for strength in [1, .5, .25, .1, 0]:
                            before_attempt = recipe(pid)['recipeHash']
                            attempt_history = call('image_history', photoId=pid)
                            try:
                                result = apply(amount=strength)
                            except Exception as error:
                                if not guarded_refusal(error):
                                    raise
                                assert recipe(pid)['recipeHash'] == before_attempt
                                assert call('image_history', photoId=pid) == attempt_history
                                case['strengthAttempts'].append(dict(amount=strength, accepted=False, reason=str(error), noEditSaved=True))
                            else:
                                case['strengthAttempts'].append(dict(amount=strength, accepted=True))
                                controls['amount'] = strength
                                case['acceptedAmount'] = strength
                                break
                        else:
                            raise AssertionError('Even a neutral match was refused')
                        assert result['fullResolutionChecked'] and result['current']
                        matched = render(pid, out / f'{style}-{photo["fileName"]}-matched.png')
                        matched_recipe = recipe(pid)
                        case.update(analysis={key: value for key, value in prepared.items() if key != 'reference'},
                                    match={key: value for key, value in result.items() if key != 'reference'},
                                    matchedToKnownGrade=errors(matched, expected[pid]), pixelsChanged=not np.array_equal(matched, pixels))
                        if result['changed']:
                            call('history_step', projectId=project, photoId=pid, action='undo')
                            assert np.array_equal(render(pid), pixels)
                            call('history_step', projectId=project, photoId=pid, action='redo')
                            assert recipe(pid)['recipeHash'] == matched_recipe['recipeHash']
                            assert np.array_equal(render(pid), matched)
                        case['undoRedoExact'] = True
                        if style == 'warm' and photo == photos[1]:
                            axes = {}
                            base_global = json.loads(base['body'])['global']
                            for axis in ['tone', 'color', 'whiteBalance']:
                                axis_before = recipe(pid)['recipeHash']
                                axis_history = call('image_history', photoId=pid)
                                try:
                                    axis_result = apply(**{axis: 0})
                                except Exception as error:
                                    if not guarded_refusal(error):
                                        raise
                                    assert recipe(pid)['recipeHash'] == axis_before
                                    assert call('image_history', photoId=pid) == axis_history
                                    axes[axis] = dict(accepted=False, reason=str(error), noEditSaved=True)
                                    continue
                                current = json.loads(recipe(pid)['body'])['global']
                                if axis == 'whiteBalance':
                                    assert all(current[key] == base_global[key] for key in ['temperature', 'tint'])
                                elif axis == 'tone':
                                    assert all(current[key] == base_global[key] for key in ['exposure', 'contrast', 'highlights', 'shadows', 'whites', 'blacks', 'curve'])
                                else:
                                    assert all(current[key] == base_global[key] for key in ['vibrance', 'saturation'])
                                axes[axis] = dict(accepted=True, disabledFieldsRestored=True, changed=axis_result['changed'])
                            case['independentControls'] = axes
                        apply(amount=0)
                        assert json.loads(recipe(pid)['body'])['global'] == json.loads(base['body'])['global']
                        assert np.array_equal(render(pid), pixels)
                        case.update(passed=True, zeroRestoresExact=True)
                        print('PASS', style, photo['fileName'], 'known-grade mean DeltaE76', round(case['matchedToKnownGrade']['meanDeltaE76'], 3), flush=True)
                    except Exception as error:
                        case.update(passed=False, error=str(error))
                        raise
                    finally:
                        persist()

            sample_style = args.styles[0]
            sample = Image.open(out / (sample_style + '-reference.' + {'warm': 'jpg', 'cool': 'tiff'}.get(sample_style, 'png'))).convert('RGB')
            profiled = out / 'reference-srgb-profile.png'
            sample.save(profiled, icc_profile=ImageCms.ImageCmsProfile(ImageCms.createProfile('sRGB')).tobytes())
            webp = out / 'unsupported.webp'
            sample.save(webp)
            cutout = out / 'transparent.png'
            rgba = sample.convert('RGBA')
            rgba.putalpha(128)
            rgba.save(cutout)
            corrupt = out / 'corrupt.jpg'
            corrupt.write_bytes(b'not a photo')
            for path, accepted in [(profiled, True), (webp, False), (cutout, False), (corrupt, False)]:
                try:
                    value = call('import_color_reference', path=str(path), cancelId=str(uuid.uuid4()))
                except Exception as error:
                    if accepted:
                        raise
                    report['formats'].append(dict(file=path.name, passed=True, accepted=False, reason=str(error)))
                else:
                    assert accepted, 'Unsupported input was accepted'
                    assert value['convertedIcc']
                    report['formats'].append(dict(file=path.name, passed=True, accepted=True, convertedIcc=True))
            report['originalsUnchanged'] = all(digest(Path(path)) == value for path, value in sources.items())
            report['completed'] = report['originalsUnchanged'] and not report['pageErrors'] and all(c.get('passed') for c in report['cases'])
            assert report['completed']
            persist()
    except Exception as error:
        report['fatalError'] = str(error)
        raise
    finally:
        persist()
        proc.terminate()
        proc.wait(timeout=30)

    cards = []
    for case in report['cases']:
        style, target = case['style'], case['target']
        images = [(f'{style}-reference.' + {'warm': 'jpg', 'cool': 'tiff'}.get(style, 'png'), 'Uploaded reference'),
                  (target + '-before.png', 'New photo before'), (f'{style}-{target}-expected.png', 'Known grade on this target'),
                  (f'{style}-{target}-matched.png', 'AURA reference match')]
        # Browsers do not reliably display TIFF; use the identical lossless PNG oracle for that reference.
        if style == 'cool':
            images[0] = (f'{style}-0.png-expected.png', 'Uploaded TIFF reference (PNG view)')
        cards.append('<section><h2>' + html.escape(style + ' / ' + target) + '</h2><div class="row">' + ''.join(
            f'<figure><img src="{html.escape(path)}"><figcaption>{label}</figcaption></figure>' for path, label in images) +
            f'</div><p>Known-grade mean CIE76 error: before {case["baselineToKnownGrade"]["meanDeltaE76"]:.3f}; after {case["matchedToKnownGrade"]["meanDeltaE76"]:.3f}. Lower is closer; no universal quality score.</p></section>')
    (out / 'index.html').write_text('<!doctype html><meta charset="utf-8"><title>AURA reference grade transfer</title><style>body{font:16px system-ui;margin:24px;background:#171719;color:#eee}.row{display:flex;gap:12px}figure{margin:0;width:25%}img{width:100%}section{margin-bottom:36px}p{color:#ccc}</style><h1>AURA reference colour matching</h1><p>Controlled known grades made by AURA. The first target repeats the reference source; targets 1 and 2 are different photos. This is not an Evoto equivalence test.</p>' + ''.join(cards), encoding='utf-8')


if __name__ == '__main__':
    main()
