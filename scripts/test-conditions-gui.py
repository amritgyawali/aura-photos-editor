"""One-click Auto edit in the real desktop window, across shooting conditions.

Imports every `NAME-before.png` in a folder (for example the output of
`make-condition-variants.py`) into an isolated catalogue, presses the same command the
**Auto edit** button does (`enhance_photo`: measured light and colour plus the one-click
retouch), renders the result at full size through the window, and scores it against
`NAME-after.png`: mean absolute error, untouched and after.

    python scripts/test-conditions-gui.py <folder> [--port 9368] [--only beauty_,wedding_]

`AURA_EXE` names the build. Writes `.work-checks/conditions-gui/`.
"""
import argparse
import base64
import json
import os
import subprocess
import time
import urllib.request
from pathlib import Path

import numpy as np
from PIL import Image
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.work-checks/conditions-gui'


def launch(exe, port):
    env = dict(os.environ,
               WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-port={port}',
               WEBVIEW2_USER_DATA_FOLDER=str(OUT / 'webview'),
               AURA_TEST_CATALOG=str(OUT / 'catalog' / 'catalog.sqlite'),
               # Measured editing only: this is the one-click path a photographer without a
               # provider gets, and the AI path has its own check.
               AURA_AI_CONFIG_FILE=str(OUT / 'no-providers.md'))
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
    parser.add_argument('--port', type=int, default=9368)
    parser.add_argument('--only', help='comma-separated name prefixes')
    args = parser.parse_args()
    exe = Path(os.environ.get('AURA_EXE', ROOT / 'target/desktop-launch/debug/aura-desktop.exe'))
    OUT.mkdir(parents=True, exist_ok=True)
    sources = sorted(p for p in args.folder.resolve().glob('*-before.*')
                     if p.suffix.lower() in ('.png', '.jpg', '.jpeg', '.tif', '.tiff'))
    if args.only:
        sources = [p for p in sources if any(p.name.startswith(n) for n in args.only.split(','))]
    assert sources, 'No NAME-before.png in the folder'
    report = {'exe': str(exe), 'photos': {}, 'pageErrors': []}
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
            assert page, 'the window did not open a page'
            page.set_default_timeout(600000)
            page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
            page.on('pageerror', lambda e: report['pageErrors'].append(str(e)[:300]))

            def call(command, **kwargs):
                return page.evaluate('async ([c,a])=>await window.__TAURI_INTERNALS__.invoke(c,{input:a})',
                                     [command, kwargs])

            def raw(command, **kwargs):
                return page.evaluate('async ([c,a])=>await window.__TAURI_INTERNALS__.invoke(c,a)', [command, kwargs])

            project = call('create_project', name='Conditions ' + time.strftime('%H-%M-%S'),
                           coupleNames=None, eventDate=None)['id']
            call('start_ingest', projectId=project, roots=[str(p) for p in sources])
            photos, deadline = [], time.monotonic() + 600
            while len(photos) < len(sources) and time.monotonic() < deadline:
                photos = call('list_images', projectId=project, offset=0, limit=500, orderBy='timeline')
                time.sleep(0.5)
            assert len(photos) == len(sources), f'imported {len(photos)} of {len(sources)}'
            for row in sorted(photos, key=lambda r: r['fileName']):
                name = row['fileName'].rsplit('-before.', 1)[0]
                source = next(p for p in sources if p.name == row['fileName'])
                started = time.monotonic()
                saved = call('enhance_photo', photoId=row['id'])
                seconds = round(time.monotonic() - started, 1)
                body = json.loads(saved['body'])
                render = pixels(raw('native_retouch_preview', projectId=project, photoId=row['id'], before=False,
                                    quality='full'))
                first = np.asarray(Image.open(source).convert('RGB'), dtype=np.float32)
                finished = np.asarray(Image.open(args.folder / f'{name}-after.png').convert('RGB'), dtype=np.float32)
                record = {'seconds': seconds,
                          'untouched': round(float(np.abs(first - finished).mean()), 2) if first.shape == finished.shape else None,
                          'after': round(float(np.abs(render - finished).mean()), 2) if render.shape == finished.shape else None,
                          'global': {k: body['global'].get(k) for k in ('exposure', 'highlights', 'shadows', 'whites',
                                                                         'blacks', 'temperature', 'tint', 'vibrance')},
                          'decisions': (body.get('studio_portrait_auto_v1', {}).get('scene') or {}).get('decisions', [])}
                report['photos'][name] = record
                if record['after'] is None or record['untouched'] is None:
                    print(f"{name:22} edited and rendered at {render.shape[1]}x{render.shape[0]}  ({seconds}s)", flush=True)
                else:
                    print(f"{name:22} {record['untouched']:6.2f} -> {record['after']:6.2f}  ({seconds}s)", flush=True)
                (OUT / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
            scored = [r for r in report['photos'].values() if r['after'] is not None and r['untouched'] is not None]
            report['total'] = {'untouched': round(sum(r['untouched'] for r in scored), 1),
                               'after': round(sum(r['after'] for r in scored), 1)}
            report['noPageErrors'] = not report['pageErrors']
            page.screenshot(path=str(OUT / 'final-ui.png'))
    finally:
        (OUT / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
        proc.terminate()
    print(json.dumps({k: report.get(k) for k in ('total', 'noPageErrors')}, indent=2))


if __name__ == '__main__':
    main()
