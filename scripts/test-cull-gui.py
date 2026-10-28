"""The Cull screen in the real window. ADR-0185.

    python scripts/test-cull-gui.py <folder of JPEGs> [--count 20] [--port 9373]

Copies `--count` photographs from the folder, adds a defocused, a dark and a smeared copy of
three of them, imports them into an isolated catalogue, and drives the Cull screen by clicking:
cull, read both lists, open a photograph's hundred checks, bring one back, keep every photo,
undo every choice. Uses the debug build (`AURA_EXE`, default
`target/desktop-launch/debug/aura-desktop.exe`) so `AURA_TEST_CATALOG` is honoured; your own
catalogue is never opened. Writes screenshots and `report.json` to `.work-checks/cull-gui/`.
"""
import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

from PIL import Image, ImageFilter
from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / '.work-checks' / 'cull-gui'


def launch(exe, port):
    env = dict(os.environ,
               WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-port={port}',
               WEBVIEW2_USER_DATA_FOLDER=str(OUT / 'webview'),
               AURA_TEST_CATALOG=str(OUT / 'catalog' / 'catalog.sqlite'))
    (OUT / 'catalog').mkdir(parents=True, exist_ok=True)
    proc = subprocess.Popen([str(exe)], cwd=str(ROOT), env=env)
    for _ in range(180):
        if proc.poll() is not None:
            raise RuntimeError(f'AURA exited during start-up (code {proc.returncode})')
        try:
            with urllib.request.urlopen(f'http://127.0.0.1:{port}/json/list', timeout=1) as reply:
                pages = json.loads(reply.read())
            if any(p.get('type') == 'page' and 'tauri' in p.get('url', '') for p in pages):
                return proc
        except Exception:  # noqa: BLE001 - the port is not open until the window is
            pass
        time.sleep(1)
    proc.terminate()
    raise RuntimeError('AURA did not start')


def prepare(folder, count):
    sources = OUT / 'sources'
    sources.mkdir(parents=True)
    photos = sorted(p for p in folder.iterdir() if p.suffix.lower() in ('.jpg', '.jpeg'))
    step = max(1, len(photos) // count)
    chosen = photos[::step][:count]
    for photo in chosen:
        shutil.copy2(photo, sources / photo.name)
    damaged = {}
    for photo, kind in zip(chosen[:3], ['defocused', 'dark', 'smeared']):
        image = Image.open(photo).convert('RGB')
        image.thumbnail((2400, 2400))
        if kind == 'defocused':
            image = image.filter(ImageFilter.GaussianBlur(9))
        elif kind == 'dark':
            image = image.point(lambda v: v // 10)
        else:
            # A horizontal smear: the middle row of a 5x5 kernel, six times over.
            horizontal = ImageFilter.Kernel((5, 5), [0] * 10 + [1] * 5 + [0] * 10, 5)
            for _ in range(6):
                image = image.filter(horizontal)
        name = f'{photo.stem}-{kind}.jpg'
        image.save(sources / name, quality=90)
        damaged[name] = kind
    return sources, damaged


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('folder', type=Path)
    parser.add_argument('--count', type=int, default=20)
    parser.add_argument('--port', type=int, default=9373)
    args = parser.parse_args()
    exe = Path(os.environ.get('AURA_EXE', ROOT / 'target/desktop-launch/debug/aura-desktop.exe'))
    if OUT.exists():
        shutil.rmtree(OUT)
    OUT.mkdir(parents=True)
    sources, damaged = prepare(args.folder, args.count)
    total = len(list(sources.iterdir()))
    report = {'exe': str(exe), 'photos': total, 'damaged': damaged, 'checks': {}, 'pageErrors': []}

    def check(name, ok, detail=''):
        report['checks'][name] = {'ok': bool(ok), 'detail': detail}
        print(('PASS ' if ok else 'FAIL ') + name + (f' - {detail}' if detail else ''))

    proc = launch(exe, args.port)
    time.sleep(2)
    assert (OUT / 'catalog' / 'catalog.sqlite').exists(), 'the catalogue is not isolated - stop'
    try:
        with sync_playwright() as pw:
            page = None
            for _ in range(8):
                browser = pw.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}')
                for _ in range(15):
                    found = [p for c in browser.contexts for p in c.pages]
                    page = next((p for p in found if 'localhost' in p.url or 'tauri' in p.url), None)
                    if page:
                        break
                    time.sleep(1)
                if page:
                    break
                browser.close()
            if page is None:
                raise RuntimeError(f'no AURA page on port {args.port}')
            page.set_default_timeout(600000)
            expect.set_options(timeout=600000)
            page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
            page.on('pageerror', lambda e: report['pageErrors'].append(str(e)[:300]))

            def call(command, **kwargs):
                return page.evaluate('async ([c,a])=>await window.__TAURI_INTERNALS__.invoke(c,{input:a})',
                                     [command, kwargs])

            def raw(command, **kwargs):
                return page.evaluate('async ([c,a])=>await window.__TAURI_INTERNALS__.invoke(c,a)', [command, kwargs])

            name = 'Cull ' + time.strftime('%H-%M-%S')
            project = call('create_project', name=name, coupleNames=None, eventDate=None)['id']
            call('start_ingest', projectId=project, roots=[str(sources)])
            photos, deadline = [], time.monotonic() + 600
            while len(photos) < total and time.monotonic() < deadline:
                photos = call('list_images', projectId=project, offset=0, limit=500, orderBy='timeline')
                time.sleep(1)
            check('every photo imported', len(photos) == total, f'{len(photos)} of {total}')

            page.reload()
            page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
            page.get_by_role('button', name=re.compile(re.escape(name))).first.click()
            page.get_by_role('button', name='Cull Best photos & left out', exact=True).click()
            expect(page.get_by_text('This collection has not been culled yet.')).to_be_visible()
            started = time.monotonic()
            page.get_by_role('button', name='Cull all photos', exact=True).click()
            best_tab = page.get_by_role('tab', name=re.compile(r'^Best \(\d+\)$'))
            expect(best_tab).to_be_visible()
            report['cullSeconds'] = round(time.monotonic() - started, 1)
            summary = page.locator('.photo-cull-summary').inner_text()
            check('the cull finished and says what it did', f'{total} photos checked' in summary, summary)
            page.screenshot(path=str(OUT / '1-best.png'))

            review = raw('photo_cull_review', projectId=project)
            by_name = {p['fileName']: p for p in review['photos']}
            report['review'] = [{k: p[k] for k in ('fileName', 'keep', 'reasonLabel', 'score', 'topPick')}
                                for p in review['photos']]
            for photo_name, kind in damaged.items():
                p = by_name.get(photo_name)
                original = by_name.get(photo_name.replace(f'-{kind}', ''))
                check(f'the {kind} copy is left out or scores well below its original',
                      p is not None and (not p['keep'] or (original and p['score'] is not None
                                                           and original['score'] - p['score'] >= 12)),
                      f"{p and p['reasonLabel']} {p and p['score']} vs {original and original['score']}")
            left = review['leftOut']
            check('the left-out list is not empty', left > 0, f'{left} left out')

            page.get_by_role('tab', name=f'Left out ({left})', exact=True).click()
            page.screenshot(path=str(OUT / '2-left-out.png'))
            first = page.locator('.cull-card').first
            first_name = first.get_attribute('aria-label')
            first.get_by_title('Show its hundred checks').click()
            detail = page.get_by_role('complementary', name=f'Checks for {first_name}')
            expect(detail).to_be_visible()
            detail.get_by_text('All 100 checks').click()
            expect(detail.locator('.cull-checks li')).to_have_count(100)
            check('a photo opens with all 100 checks', True, first_name)
            page.screenshot(path=str(OUT / '3-checks.png'))

            first.get_by_role('button', name='Bring back', exact=True).click()
            expect(page.get_by_role('tab', name=f'Left out ({left - 1})', exact=True)).to_be_visible()
            again = raw('photo_cull_review', projectId=project)
            back = next(p for p in again['photos'] if p['fileName'] == first_name)
            check('bring back moves the photo to Best and records your choice',
                  back['keep'] and back['choice'] == 'keep', first_name)

            if left > 1:
                page.get_by_role('button', name='Keep every photo (remove none)', exact=True).click()
                expect(page.get_by_text('Nothing was left out: every photo is delivered.')).to_be_visible()
            everything = raw('photo_cull_review', projectId=project)
            check('keep every photo leaves nothing out', everything['leftOut'] == 0,
                  f"{everything['best']} best, {everything['keptByYou']} kept by you")
            page.screenshot(path=str(OUT / '4-kept-everything.png'))

            page.get_by_role('button', name=re.compile(r'^Undo all my choices')).click()
            expect(page.get_by_role('tab', name=f'Left out ({left})', exact=True)).to_be_visible()
            check('undo hands every photo back to the cull', True, f'{left} left out again')
            report['ok'] = all(c['ok'] for c in report['checks'].values()) and not report['pageErrors']
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=20)
        except subprocess.TimeoutExpired:
            proc.kill()
        (OUT / 'report.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    print('report:', OUT / 'report.json')
    sys.exit(0 if report.get('ok') else 1)


if __name__ == '__main__':
    main()
