"""Use the photographer's own AI providers in the real desktop window. ADR-0114.

Starts the desktop app against an isolated catalogue, from the project folder, so the
`AI-CONFIG.md` beside it is the one imported. It then asks the window, through the commands
its own panels call:

1. which providers were imported and which can be used (`internal_ai_status`, refreshed);
2. for every photograph in a folder, what its condition is (`photo_analysis`, measured
   locally), what the AI recommends and saves (`photo_auto_edit`), and what the AI-advised
   skin retouch plans (`enhance_portrait`);
3. which model answered each call, how long it took and what it cost (`cloud_calls`,
   `cloud_spend`).

Each photograph is rendered before and after, and scored against `NAME-after.png` when the
folder has one.

    python scripts/test-ai-config-gui.py <folder> [--port 9366] [--only beauty,wedding]

Nothing secret is read, printed or written by this script: a provider's status carries its
name, address, model and prices, and never a key. `AURA_EXE` names the build. Writes
`.work-checks/ai-config-gui/`.
"""
import argparse
import base64
import json
import os
import subprocess
import time
import urllib.request
import uuid
from pathlib import Path
from urllib.parse import urlparse

import numpy as np
from PIL import Image
from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.work-checks/ai-config-gui'


def launch(exe, port):
    env = dict(os.environ,
               WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-port={port}',
               WEBVIEW2_USER_DATA_FOLDER=str(OUT / 'webview'),
               AURA_TEST_CATALOG=str(OUT / 'catalog' / 'catalog.sqlite'))
    # The file beside the project is the one under test: an explicit override would hide it.
    env.pop('AURA_AI_CONFIG_FILE', None)
    (OUT / 'catalog').mkdir(parents=True, exist_ok=True)
    proc = subprocess.Popen([str(exe)], cwd=str(ROOT), env=env)
    for _ in range(180):
        try:
            urllib.request.urlopen(f'http://127.0.0.1:{port}/json/version', timeout=1)
            return proc
        except Exception:  # noqa: BLE001 - the port is not open until the window is
            time.sleep(1)
    proc.terminate()
    raise RuntimeError('AURA did not start')


def pixels(dto):
    return np.asarray(Image.frombytes('RGB', (dto['width'], dto['height']), base64.b64decode(dto['rgbBase64'])),
                      dtype=np.float32)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('folder', type=Path)
    parser.add_argument('--port', type=int, default=9366)
    parser.add_argument('--only', help='comma-separated photograph names')
    args = parser.parse_args()
    exe = Path(os.environ.get('AURA_EXE', ROOT / 'target/desktop-launch/debug/aura-desktop.exe'))
    OUT.mkdir(parents=True, exist_ok=True)
    sources = sorted(args.folder.resolve().glob('*-before.png'))
    if args.only:
        wanted = args.only.split(',')
        sources = [p for p in sources if p.name[:-len('-before.png')] in wanted]
    assert sources, 'No NAME-before.png in the folder'
    report = {'exe': str(exe), 'providers': [], 'photos': {}, 'checks': {}, 'pageErrors': []}
    proc = launch(exe, args.port)
    try:
        with sync_playwright() as pw:
            browser = pw.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}')
            page = None
            for _ in range(120):
                found = [p for c in browser.contexts for p in c.pages]
                page = next((p for p in found if 'localhost' in p.url or 'tauri' in p.url), None)
                if page:
                    break
                time.sleep(1)
            page.set_default_timeout(600000)
            expect.set_options(timeout=600000)
            page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
            page.on('pageerror', lambda e: report['pageErrors'].append(str(e)[:300]))
            page.wait_for_load_state()

            def call(command, **kwargs):
                return page.evaluate('async ([c,a])=>await window.__TAURI_INTERNALS__.invoke(c,{input:a})',
                                     [command, kwargs])

            def raw(command, **kwargs):
                return page.evaluate('async ([c,a])=>await window.__TAURI_INTERNALS__.invoke(c,a)', [command, kwargs])

            def attempt(run):
                try:
                    return run(), None
                except Exception as error:  # noqa: BLE001 - a refusal is a result here
                    return None, str(error)[:400]

            def write(name=None):
                (OUT / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
                if name:
                    page.screenshot(path=str(OUT / f'{name}.png'))

            started = time.monotonic()
            status = raw('internal_ai_status', refresh=True)
            report['importSeconds'] = round(time.monotonic() - started, 1)
            report['sourceAvailable'] = status.get('sourceAvailable')
            for profile in status.get('profiles', []):
                report['providers'].append({
                    'id': profile['id'], 'host': urlparse(profile['endpoint']).hostname,
                    'model': profile['model'], 'vision': profile['vision'], 'status': profile['status'],
                    'inputUsdPerMtok': profile.get('inputUsdPerMtok'),
                    'outputUsdPerMtok': profile.get('outputUsdPerMtok')})
            usable = [p for p in report['providers']
                      if p['status'] == 'catalogue_ready' and p['inputUsdPerMtok'] is not None
                      and p['outputUsdPerMtok'] is not None]
            checks = report['checks']
            checks['configFileFound'] = bool(status.get('sourceAvailable'))
            checks['providersImported'] = len(report['providers'])
            checks['providersUsable'] = len(usable)
            checks['visionProvidersUsable'] = sum(1 for p in usable if p['vision'])
            for p in report['providers']:
                print(f"{p['id']:28} {p['status']:22} vision={p['vision']!s:5} "
                      f"in={p['inputUsdPerMtok']} out={p['outputUsdPerMtok']}  {p['model']}", flush=True)
            cloud, error = attempt(lambda: raw('cloud_status'))
            # Not even the masked fingerprint of a key belongs in a report.
            report['cloudStatus'] = ({k: v for k, v in cloud.items() if k != 'keyFingerprint'}
                                     if cloud is not None else {'error': error})
            write('providers')

            name = 'AI providers ' + time.strftime('%H-%M-%S')
            project = call('create_project', name=name, coupleNames=None, eventDate=None)['id']
            call('start_ingest', projectId=project, roots=[str(p) for p in sources])
            photos, deadline = [], time.monotonic() + 300
            while len(photos) < len(sources) and time.monotonic() < deadline:
                photos = call('list_images', projectId=project, offset=0, limit=40, orderBy='timeline')
                time.sleep(0.5)
            assert len(photos) == len(sources), f'imported {len(photos)} of {len(sources)}'
            ids = {row['fileName'][:-len('-before.png')]: row['id'] for row in photos}

            def shown(photo, retouched):
                # Full size both times, so the three scores of a photograph are comparable:
                # `before` is the photograph with its light and colour and without its retouch.
                return pixels(raw('native_retouch_preview', projectId=project, photoId=photo,
                                  before=not retouched, quality='full'))

            def against(render, pair):
                path = args.folder / f'{pair}-after.png'
                if not path.is_file():
                    return None
                finished = np.asarray(Image.open(path).convert('RGB'), dtype=np.float32)
                if finished.shape != render.shape:
                    return None
                return round(float(np.abs(render - finished).mean()), 3)

            for pair, photo in ids.items():
                record = report['photos'].setdefault(pair, {})
                analysis, error = attempt(lambda: call('photo_analysis', projectId=project, photoId=photo,
                                                       jobId=str(uuid.uuid4())))
                record['condition'] = ({'readings': {k: v for k, v in analysis['readings'].items()
                                                     if k != 'image_hash'},
                                        'localRecommendation': analysis['recommendation']}
                                       if analysis else {'error': error})
                record['untouched'] = against(
                    np.asarray(Image.open(args.folder / f'{pair}-before.png').convert('RGB'), dtype=np.float32),
                    pair)

                started = time.monotonic()
                edit, error = attempt(lambda: call('photo_auto_edit', projectId=project, photoId=photo,
                                                   jobId=str(uuid.uuid4())))
                record['grade'] = ({'seconds': round(time.monotonic() - started, 1), 'source': edit['source'],
                                    'model': edit['model'], 'reasons': edit['reasons']}
                                   if edit else {'error': error})
                graded, error = attempt(lambda: shown(photo, False))
                record['afterGrade'] = against(graded, pair) if graded is not None else {'error': error}
                if edit:
                    body = json.loads(edit['recipe']['body'])
                    record['grade']['saved'] = {k: body['global'].get(k) for k in
                                                ('exposure', 'contrast', 'highlights', 'shadows', 'vibrance',
                                                 'temperature', 'tint', 'saturation')}

                started = time.monotonic()
                retouch, error = attempt(lambda: raw('enhance_portrait', input={'photoId': photo}))
                if retouch:
                    body = json.loads(retouch['body'])
                    auto = body.get('studio_portrait_auto_v1', {})
                    settings = auto.get('options', {}).get('settings', {})
                    tools = {}
                    for step in body.get('studio_retouch_v1', []):
                        tools[step['tool']] = tools.get(step['tool'], 0) + 1
                    record['retouch'] = {'seconds': round(time.monotonic() - started, 1), 'operations': tools,
                                         'message': auto.get('message'),
                                         'settings': {k: settings.get(k) for k in
                                                      ('cleanupPasses', 'blemishSensitivity', 'frequencyHeal',
                                                       'poreRefine', 'keepMoles', 'removeDarkMarks')}}
                else:
                    record['retouch'] = {'error': error}
                finished, error = attempt(lambda: shown(photo, True))
                if finished is not None:
                    record['afterRetouch'] = against(finished, pair)
                    Image.fromarray(finished.astype(np.uint8)).save(OUT / f'{pair}.ai.png')
                    before = np.asarray(Image.open(args.folder / f'{pair}-before.png').convert('RGB'))
                    row = [before, finished.astype(np.uint8)]
                    target = args.folder / f'{pair}-after.png'
                    if target.is_file() and finished.shape == before.shape:
                        row.append(np.asarray(Image.open(target).convert('RGB')))
                    if all(part.shape == row[0].shape for part in row):
                        sheet = Image.fromarray(np.concatenate(row, axis=1))
                        sheet.thumbnail((2400, 2400))
                        sheet.save(OUT / f'{pair}.ai.sheet.jpg', quality=90)
                else:
                    record['afterRetouch'] = {'error': error}
                print(pair, json.dumps({k: record[k] for k in ('untouched', 'afterGrade', 'afterRetouch')}),
                      record['grade'].get('source'), record['grade'].get('model'), flush=True)
                write()

            calls, error = attempt(lambda: raw('cloud_calls', projectId=project, limit=200))
            report['calls'] = ([{k: row.get(k) for k in ('task', 'model', 'source', 'fallbackReason', 'status',
                                                          'tokensIn', 'tokensOut', 'costUsd', 'latencyMs',
                                                          'retryCount', 'confidence')} for row in calls]
                               if calls is not None else {'error': error})
            spend, error = attempt(lambda: raw('cloud_spend', projectId=project))
            report['spend'] = spend if spend is not None else {'error': error}
            answered = [row for row in (calls or []) if row.get('source') == 'cloud'
                        and row.get('status') in ('ok', 'repaired')]
            checks['callsAnsweredByAProvider'] = len(answered)
            checks['modelsThatAnswered'] = sorted({row['model'] for row in answered})
            checks['photosGradedByAi'] = sum(1 for r in report['photos'].values()
                                             if r['grade'].get('source') == 'cloud')
            checks['photosGraded'] = sum(1 for r in report['photos'].values() if 'source' in r['grade'])
            checks['photosRetouched'] = sum(1 for r in report['photos'].values() if 'operations' in r['retouch'])
            advised = {row.get('decisionRef') for row in answered if row.get('task') == 'native_skin_advice'}
            for pair, photo in ids.items():
                report['photos'][pair]['retouch']['advisedByAi'] = photo in advised
            checks['retouchesAdvisedByAi'] = len(advised & set(ids.values()))
            # No automatic grade may leave a photograph far further from its finished version
            # than it started: the stand-in this replaced took bright frames from 2 to 29.
            scored = [(r['untouched'], r['afterGrade']) for r in report['photos'].values()
                      if isinstance(r.get('untouched'), float) and isinstance(r.get('afterGrade'), float)]
            checks['noGradeSpoilsAPhotograph'] = all(after <= before + 4 for before, after in scored)
            checks['noPageErrors'] = not report['pageErrors']
            page.reload()
            page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
            write('final-ui')
    finally:
        (OUT / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
        proc.terminate()
    print(json.dumps(report['checks'], indent=2))


if __name__ == '__main__':
    main()
