"""The photo pass (ADR-0186) in the real desktop window, through an ordinary import.

    AURA_EXE=<debug aura-desktop.exe> python scripts/test-photo-pass-gui.py OUT PHOTO [PHOTO ...]

The app starts against an isolated catalogue (`AURA_TEST_CATALOG`, debug builds only), the
photographs are copied into OUT/incoming and imported with the Photos workspace's own controls,
and nothing else is clicked: an import edits every photograph by itself. The checks:

* every imported photograph carries the pass's report, with every built step in it;
* every step that changed a photograph is its own history entry;
* the Develop panel shows the report, and running the pass again saves nothing new;
* a value set by hand is kept, and the step that wanted it says so.

Results go to OUT/results.json, screenshots beside it.
"""
import json
import os
import re
import shutil
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

from playwright.sync_api import expect, sync_playwright

PORT = 9433
rows = []


def check(name, ok, detail):
    rows.append({'check': name, 'status': 'pass' if ok else 'fail', 'detail': detail})
    print(f"{'PASS' if ok else 'FAIL'}  {name} - {detail}", flush=True)


def launch(exe, out):
    env = dict(os.environ,
               WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-port={PORT}',
               WEBVIEW2_USER_DATA_FOLDER=str(out / 'webview'),
               AURA_TEST_CATALOG=str(out / 'catalog' / 'catalog.sqlite'),
               AURA_LICENCE_DIR=str(out / 'licence'))
    (out / 'catalog').mkdir(parents=True, exist_ok=True)
    proc = subprocess.Popen([str(exe)], cwd=str(Path(exe).parent), env=env)
    for _ in range(180):
        if proc.poll() is not None:
            raise RuntimeError(f'AURA exited during start-up ({proc.returncode})')
        try:
            with urllib.request.urlopen(f'http://127.0.0.1:{PORT}/json/list', timeout=1) as r:
                if any(p.get('type') == 'page' for p in json.loads(r.read())):
                    return proc
        except Exception:  # noqa: BLE001 - the port is not open yet
            pass
        time.sleep(1)
    proc.terminate()
    raise RuntimeError('AURA did not start')


def main():
    out, sources = Path(sys.argv[1]), [Path(p) for p in sys.argv[2:]]
    if out.exists():
        shutil.rmtree(out)
    incoming = out / 'incoming'
    incoming.mkdir(parents=True)
    for source in sources:
        shutil.copy2(source, incoming / source.name)
    proc = launch(Path(os.environ['AURA_EXE']), out)
    try:
        with sync_playwright() as pw:
            browser = pw.chromium.connect_over_cdp(f'http://127.0.0.1:{PORT}')
            page = None
            for _ in range(30):
                page = next((p for c in browser.contexts for p in c.pages), None)
                if page:
                    break
                time.sleep(1)
            page.set_default_timeout(600000)
            page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
            run(page, out, incoming, len(sources))
    finally:
        (out / 'results.json').write_text(json.dumps(rows, indent=1), encoding='utf-8')
        proc.terminate()
    passed = sum(r['status'] == 'pass' for r in rows)
    print(f'\n{passed} of {len(rows)} checks pass')
    sys.exit(0 if passed == len(rows) else 1)


def run(page, out, incoming, count):
    invoke = lambda cmd, args: page.evaluate(  # noqa: E731
        '([c,a])=>window.__TAURI_INTERNALS__.invoke(c,a)', [cmd, args])
    nav = lambda name: page.locator('.studio-nav').get_by_role('button', name=re.compile('^' + name)).click()  # noqa: E731

    collection = 'Photo pass ' + time.strftime('%H%M%S')
    nav('Start')
    page.get_by_label('New collection', exact=True).fill(collection)
    page.get_by_role('button', name='Create', exact=True).click()
    nav('Photos')
    page.get_by_text('Enter folders manually', exact=True).click()
    page.get_by_label('Card or folder', exact=True).fill(str(incoming.resolve()))
    page.get_by_role('button', name='Add folder', exact=True).click()
    page.get_by_role('button', name='Start import', exact=True).click()

    project = None
    for _ in range(600):
        project = next((p for p in invoke('list_projects', {}) if p['name'] == collection), None)
        if project:
            break
        time.sleep(1)
    photos = []
    deadline = time.monotonic() + 60 * 40
    reports = {}
    while time.monotonic() < deadline:
        photos = invoke('list_images', {'input': {'projectId': project['id'], 'offset': 0, 'limit': 50, 'orderBy': None}})
        for photo in photos:
            body = json.loads(invoke('image_recipe', {'input': {'photoId': photo['id']}})['body'])
            if 'photo_pass_v1' in body:
                reports[photo['id']] = body['photo_pass_v1']
        if len(photos) == count and len(reports) == count:
            break
        time.sleep(5)
    page.screenshot(path=str(out / '01-after-import.png'))
    check('an import gives every photograph the pass by itself', len(reports) == count == len(photos),
          f'{len(reports)} of {len(photos)} photographs carry a report')
    check('every report lists every built step', all(len(r['steps']) == 25 and r['planned'] == 100 for r in reports.values()),
          {p['fileName']: len(reports.get(p['id'], {}).get('steps', [])) for p in photos})

    entries = {}
    for photo in photos:
        history = invoke('image_history', {'input': {'photoId': photo['id']}})['entries']
        entries[photo['id']] = [e['label'] for e in history if e['label'].startswith('Photo pass')]
        applied = [s for s in reports.get(photo['id'], {}).get('steps', []) if s['outcome'] == 'applied']
        numbered = [l for l in entries[photo['id']] if re.match(r'Photo pass \d+/100', l)]
        check(f"{photo['fileName']}: one history entry per changed step", len(numbered) == len(applied),
              f"{len(applied)} changed: {[s['title'] for s in applied]}; history {numbered}")

    # The Develop panel, and a second run that finds nothing new to save.
    first = photos[0]
    nav('Auto edit')
    page.get_by_role('option', name=first['fileName'], exact=True).click()
    panel = page.get_by_role('region', name='Photo pass')
    expect(panel).to_be_visible(timeout=180000)
    status = panel.get_by_role('status').inner_text()
    page.screenshot(path=str(out / '02-develop-panel.png'))
    check('Develop shows the report', '25 of 100 steps built' in status, status)
    before = len(invoke('image_history', {'input': {'photoId': first['id']}})['entries'])
    again = panel.get_by_role('button', name='Run photo pass again', exact=True)
    expect(again).to_be_enabled(timeout=180000)
    again.click()
    time.sleep(3)
    expect(again).to_be_enabled(timeout=300000)
    after = len(invoke('image_history', {'input': {'photoId': first['id']}})['entries'])
    check('running the pass again on an unchanged photograph saves nothing', after == before, f'{before} -> {after} entries')

    # A value set by hand wins over the pass.
    target = next((p for p in photos if any(s['id'] == 'colour_noise' and s['outcome'] == 'applied'
                                            for s in reports[p['id']]['steps'])), None)
    if target is None:
        check('a hand-set value is kept', False, 'no photograph needed colour noise reduction to test with')
        return
    invoke('set_param', {'input': {'projectId': project['id'], 'photoId': target['id'], 'path': 'global.noise.colour',
                                   'value': 2, 'label': 'My colour noise'}})
    result = invoke('photo_pass', {'input': {'projectId': project['id'], 'photoId': target['id']}})
    step = next(s for s in result['report']['steps'] if s['id'] == 'colour_noise')
    body = json.loads(result['recipe']['body'])
    check('a hand-set value is kept, and the step says so', step['outcome'] == 'protected' and body['global']['noise']['colour'] == 2,
          f"{step['outcome']}, colour noise {body['global']['noise']['colour']}: {step['changes']}")


if __name__ == '__main__':
    main()
