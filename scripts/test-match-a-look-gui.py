"""Drive Match a look in the real desktop window, step by step, on a real camera file.

Starts a debug desktop build against an isolated catalogue (`AURA_TEST_CATALOG`; the
photographer's own catalogue is never touched) with WebView2 CDP, imports the photographs and the
references, opens the Match a look workspace and walks it the way a photographer would: the goal
it opens on, a one-click match, the strength slider, removing the match, every goal, the other
reference, Quick mode and syncing the collection. After each step it saves a screenshot and the
photograph's export render, and records the engine's report and an independent colour distance
to the reference (sliced Wasserstein distance between the two images' OKLab colours, x100).

    python scripts/test-match-a-look-gui.py REFERENCE OTHER_REFERENCE PHOTO [MORE_PHOTOS...]

Output: `.work-checks/match-a-look-<id>/` - `results.json`, `steps/*.png`, `renders/*.png`.
"""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
import urllib.request
import uuid

import numpy as np
from PIL import Image
from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parents[1]


def oklab(rgb):
    c = np.where(rgb <= 0.04045, rgb / 12.92, ((rgb + 0.055) / 1.055) ** 2.4)
    m1 = np.array([[0.4122214708, 0.5363325363, 0.0514459929], [0.2119034982, 0.6806995451, 0.1073969566],
                   [0.0883024619, 0.2817188376, 0.6299787005]])
    m2 = np.array([[0.2104542553, 0.7936177850, -0.0040720468], [1.9779984951, -2.4285922050, 0.4505937099],
                   [0.0259040371, 0.7827717662, -0.8086757660]])
    return np.cbrt(c @ m1.T) @ m2.T


def colours(image):
    image = image.convert('RGB')
    image.thumbnail((400, 400))
    lab = oklab(np.asarray(image, dtype=np.float32) / 255).reshape(-1, 3)
    pick = np.random.default_rng(1).choice(len(lab), min(20000, len(lab)), replace=False)
    return lab[pick]


DIRECTIONS = np.random.default_rng(7).normal(size=(64, 3))
DIRECTIONS /= np.linalg.norm(DIRECTIONS, axis=1, keepdims=True)


def distance(a, b):
    q = np.linspace(0.01, 0.99, 99)
    return float(100 * np.mean([np.mean(np.abs(np.quantile(a @ d, q) - np.quantile(b @ d, q))) for d in DIRECTIONS]))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('reference', type=Path)
    parser.add_argument('other_reference', type=Path)
    parser.add_argument('photos', type=Path, nargs='+')
    parser.add_argument('--exe', type=Path, default=ROOT / 'target/desktop-launch/debug/aura-desktop.exe')
    parser.add_argument('--port', type=int, default=9381)
    args = parser.parse_args()
    out = ROOT / '.work-checks' / ('match-a-look-' + uuid.uuid4().hex[:8])
    (out / 'inputs').mkdir(parents=True)
    (out / 'steps').mkdir()
    (out / 'renders').mkdir()
    for photo in args.photos:
        shutil.copy2(photo, out / 'inputs' / photo.name)
    originals = {str(p): hashlib.sha256(p.read_bytes()).hexdigest()
                 for p in [args.reference, args.other_reference, *(out / 'inputs').iterdir()]}
    reference_colours = {args.reference.name: colours(Image.open(args.reference)),
                         args.other_reference.name: colours(Image.open(args.other_reference))}
    env = dict(os.environ, AURA_TEST_CATALOG=str(out / 'catalog.sqlite'),
               WEBVIEW2_USER_DATA_FOLDER=str(out / 'webview'),
               WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-port={args.port}')
    log = (out / 'desktop.log').open('w', encoding='utf-8')
    process = subprocess.Popen([str(args.exe.resolve())], cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
    report = {'steps': [], 'pageErrors': [], 'failures': []}
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
            page.set_default_timeout(300000)
            expect.set_options(timeout=300000)
            page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
            page.on('pageerror', lambda error: report['pageErrors'].append(str(error)))
            page.on('crash', lambda: report.setdefault('runtimeEvents', []).append('page crashed'))
            page.on('close', lambda: report.setdefault('runtimeEvents', []).append(f'page closed; desktop exit code {process.poll()}'))

            def call(command, **kwargs):
                return page.evaluate('async ([c,a])=>window.__TAURI_INTERNALS__.invoke(c,{input:a})', [command, kwargs])

            def status(photo):
                return page.evaluate("photoId=>window.__TAURI_INTERNALS__.invoke('color_match_status',{photoId})", photo)

            def render(photo):
                r = call('render_image', photoId=photo, level='screen', screen=[1600, 1600], colourSpace=None, purpose='export')
                return Image.frombytes('RGB', (r['width'], r['height']), base64.b64decode(r['rgbBase64']))

            def step(name, photo, reference_name, check, **extra):
                image = render(photo)
                image.save(out / 'renders' / f'{len(report["steps"]):02d}-{name}.png')
                page.screenshot(path=str(out / 'steps' / f'{len(report["steps"]):02d}-{name}.png'))
                value = status(photo)
                fitting = (value or {}).get('fitting') or {}
                row = dict(step=name, photo=photo, reference=reference_name,
                           distanceToReference=round(distance(colours(image), reference_colours[reference_name]), 2),
                           engine=fitting.get('engine'), intent=fitting.get('intent'), quality=fitting.get('quality'),
                           kept=fitting.get('nativeBacktrackFactor'), amount=(value or {}).get('controls', {}).get('amount'),
                           engineBefore=(value or {}).get('beforeDistance'), engineAfter=(value or {}).get('afterDistance'),
                           visibleChange=(value or {}).get('visibleChange'), reducedFrom=(value or {}).get('reducedFrom'),
                           messages=page.get_by_role('status').all_text_contents()[-3:],
                           alerts=page.get_by_role('alert').all_text_contents(), **extra)
                try:
                    check(value, row)
                    row['result'] = 'pass'
                except AssertionError as error:
                    row['result'] = f'fail: {error}'
                    report['failures'].append(f'{name}: {error}')
                report['steps'].append(row)
                print(json.dumps({k: row[k] for k in ('step', 'result', 'distanceToReference', 'engine', 'kept', 'amount', 'alerts')}), flush=True)
                return value, image

            def wait_status(photo, predicate, seconds=600):
                deadline = time.monotonic() + seconds
                while time.monotonic() < deadline:
                    value = status(photo)
                    if value and predicate(value):
                        return value
                    alerts = page.get_by_role('alert').all_text_contents()
                    if alerts:
                        return {'alerts': alerts}
                    # An honest refusal is a status, not an alert: nothing closer passed the checks.
                    refused = [t for t in page.get_by_role('status').all_text_contents() if 'No stronger match was saved' in t]
                    if refused:
                        return {'refused': refused[-1]}
                    time.sleep(.5)
                raise AssertionError('timed out waiting for the saved match')

            name = 'Match a look ' + uuid.uuid4().hex[:6]
            project = call('create_project', name=name, coupleNames=None, eventDate=None)['id']
            call('start_ingest', projectId=project, roots=[str(out / 'inputs')])
            photos = []
            for _ in range(600):
                photos = call('list_images', projectId=project, offset=0, limit=20, orderBy='timeline')
                if len(photos) == len(args.photos):
                    break
                time.sleep(.5)
            assert len(photos) == len(args.photos), 'not every photograph imported'
            warm = call('import_color_reference', path=str(args.reference), cancelId=str(uuid.uuid4()))
            other = call('import_color_reference', path=str(args.other_reference), cancelId=str(uuid.uuid4()))
            page.reload()
            page.get_by_role('button', name=f'{name} {len(photos)}', exact=True).click()
            page.get_by_role('button', name='Match a look Your reference, your photos', exact=True).click()
            panel = page.get_by_role('region', name='Reference colour matching', exact=True)
            expect(panel).to_be_visible()
            # The first photograph named is the one matched step by step.
            page.get_by_role('listbox', name='Filmstrip').get_by_role('option', name=args.photos[0].name, exact=True).click()
            expect(page.get_by_role('listbox', name='Filmstrip').get_by_role('option', selected=True)).to_have_attribute('aria-label', args.photos[0].name)
            active = page.get_by_role('listbox', name='Filmstrip').get_by_role('option', selected=True).get_attribute('aria-label')
            photo = next(p['id'] for p in photos if p['fileName'] == active)
            before = render(photo)
            before.save(out / 'renders' / 'before.png')
            report['photo'] = active
            report['beforeDistance'] = {k: round(distance(colours(before), v), 2) for k, v in reference_colours.items()}

            def opens_on_exact(value, row):
                goal = panel.get_by_label('Matching goal', exact=True).input_value()
                row['goal'] = goal
                assert goal == 'exact', f'the panel opened on {goal}'
                assert value is None, 'a match existed before any was made'
            step('opens', photo, args.reference.name, opens_on_exact)

            panel.get_by_label('Saved colour reference', exact=True).select_option(warm['id'])
            panel.get_by_role('button', name='Match reference to this photo', exact=True).click()
            saved = wait_status(photo, lambda v: v['reference']['id'] == warm['id'] and v['fullResolutionChecked'])

            def matched_fully(value, row):
                assert 'alerts' not in value, value.get('alerts')
                assert value['fitting']['engine'] == 'reference-exact-colours-v1'
                assert row['distanceToReference'] < 0.5 * report['beforeDistance'][args.reference.name], \
                    f"only {report['beforeDistance'][args.reference.name]} -> {row['distanceToReference']}"
            step('exact-one-click', photo, args.reference.name, matched_fully)

            panel.get_by_role('slider', name='Match amount', exact=True).evaluate(
                "el => { Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(el, '50'); el.dispatchEvent(new Event('input', {bubbles:true})); }")
            panel.get_by_role('button', name='Apply colour strengths', exact=True).click()
            wait_status(photo, lambda v: abs(v['controls']['amount'] - .5) < .001)
            full = report['steps'][-1]['distanceToReference']

            def half_is_between(value, row):
                assert abs(value['controls']['amount'] - .5) < .001
                assert full < row['distanceToReference'] < report['beforeDistance'][args.reference.name]
            step('exact-amount-50', photo, args.reference.name, half_is_between)

            panel.get_by_role('button', name='Remove matched treatment', exact=True).click()
            wait_status(photo, lambda v: v['controls']['amount'] == 0)

            def back_to_original(value, row):
                diff = np.abs(np.asarray(render(photo), dtype=np.int16) - np.asarray(before, dtype=np.int16))
                row['meanDifferenceFromOriginal'] = float(diff.mean())
                assert diff.mean() < 0.5, f'mean difference {diff.mean():.2f}'
            step('removed', photo, args.reference.name, back_to_original)

            for goal in ['palette', 'natural', 'exact']:
                panel.get_by_label('Matching goal', exact=True).select_option(goal)
                match = panel.get_by_role('button', name='Match reference to this photo', exact=True)
                match.click()
                # Wait for this goal's job to start and finish, so an earlier goal's message is not read as its result.
                try:
                    expect(match).to_be_disabled(timeout=20000)
                except AssertionError:
                    pass
                expect(match).to_be_enabled()
                value = wait_status(photo, lambda v: (v.get('fitting') or {}).get('intent', 'natural') == goal
                                    and v['fullResolutionChecked'] and v['controls']['amount'] > 0)

                def goal_saved(value, row, goal=goal, outcome=value):
                    if 'refused' in outcome or 'alerts' in outcome:
                        # A refusal must leave the photograph as it was before this goal was tried.
                        row['refused'] = outcome.get('refused') or outcome.get('alerts')
                        assert row['distanceToReference'] == report['steps'][-1]['distanceToReference'], 'a refused goal changed the photo'
                        return
                    assert row['intent'] == goal
                step(f'goal-{goal}', photo, args.reference.name, goal_saved)

            panel.get_by_label('Saved colour reference', exact=True).select_option(other['id'])
            panel.get_by_role('button', name='Match reference to this photo', exact=True).click()
            wait_status(photo, lambda v: v['reference']['id'] == other['id'] and v['fullResolutionChecked'])

            def other_reference(value, row):
                assert 'alerts' not in value, value.get('alerts')
                assert value['reference']['id'] == other['id']
                assert row['distanceToReference'] < report['beforeDistance'][args.other_reference.name]
            step('other-reference', photo, args.other_reference.name, other_reference)

            panel.get_by_label('Saved colour reference', exact=True).select_option(warm['id'])
            panel.get_by_label('Matching mode', exact=True).select_option('quick')
            panel.get_by_role('button', name='Match reference to this photo', exact=True).click()
            wait_status(photo, lambda v: v['reference']['id'] == warm['id'] and v['fitting']['quality'] == 'quick'
                        and v['fullResolutionChecked'])

            def quick(value, row):
                assert 'alerts' not in value, value.get('alerts')
                assert row['quality'] == 'quick' and row['intent'] == 'exact'
                assert row['distanceToReference'] < 0.5 * report['beforeDistance'][args.reference.name]
            step('quick-mode', photo, args.reference.name, quick)
            panel.get_by_label('Matching mode', exact=True).select_option('precise')

            if len(photos) > 1:
                panel.get_by_label('Sync destination', exact=True).select_option('collection')
                panel.get_by_role('button', name='Sync', exact=True).click()
                expect(panel.get_by_role('button', name='Sync', exact=True)).to_be_enabled()
                for item in photos:
                    if item['id'] == photo:
                        continue
                    wait_status(item['id'], lambda v: v['fullResolutionChecked'], seconds=900)

                    def synced(value, row):
                        assert 'alerts' not in value, value.get('alerts')
                        assert value['reference']['id'] == warm['id'] and value['current']
                        assert row['intent'] == 'exact'
                    step(f'sync-{item["fileName"]}', item['id'], args.reference.name, synced)

            report['originalsUnchanged'] = all(
                hashlib.sha256(Path(p).read_bytes()).hexdigest() == digest for p, digest in originals.items())
            if not report['originalsUnchanged']:
                report['failures'].append('an original file changed')
            if report['pageErrors']:
                report['failures'].append(f'{len(report["pageErrors"])} page errors')
    except BaseException as error:
        report['failures'].append(f'{type(error).__name__}: {error}')
        raise
    finally:
        report['desktopExitCodeBeforeCleanup'] = process.poll()
        (out / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
        process.terminate()
        try:
            process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            process.kill()
        log.close()
        print(json.dumps({'output': str(out), 'failures': report['failures']}, indent=2), flush=True)


if __name__ == '__main__':
    main()
