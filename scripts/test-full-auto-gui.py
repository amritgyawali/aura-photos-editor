"""The whole automatic workflow in the real window: import, cull, confirm, full edit. ADR-0187.

    python scripts/test-full-auto-gui.py <folder of photos> [--out DIR] [--port 9381] [--bring-back 1]

Starts the debug build (`AURA_EXE`, default `target/desktop-launch/debug/aura-desktop.exe`) on an
isolated catalogue (`AURA_TEST_CATALOG` under `--out`), so your own catalogue is never opened, and
works it the way a photographer does:

1. **Start -> Choose a folder.** The operating system's folder dialog is the one thing answered by
   the script (`window.__AURA_TEST_PICK__`, read by the app's pickers); everything after it is the
   app. The script checks the import came from that folder and nowhere else.
2. **The cull runs on its own** because several photos were imported, and the Cull screen waits for
   confirmation. The script reads both lists, brings back `--bring-back` left-out photos (a
   photographer changing the cull's mind) and presses "Edit the N best photos".
3. **The full automatic edit** runs on each confirmed photo, one at a time, in the Auto edit screen.
   The script waits for it to finish, reading the stage-by-stage progress line as it goes.
4. **Verification**, from the app itself: every confirmed photo has a stored ten-stage report and a
   changed render; every left-out photo is untouched; a second run on two photos changes no
   parameter; no page errors. Before and after renders (1200 px) and a contact sheet are written,
   with `report.json`.
"""
import argparse
import base64
import json
import os
import re
import shutil
import subprocess
import time
import urllib.request
from pathlib import Path

from PIL import Image, ImageDraw
from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parent.parent
STAGES = ['Basic edit', 'Colour correction', 'Calibration', 'Colour grading', 'Crop & straighten',
          'Blemishes', 'Skin smoothing', 'Face', 'Body', 'Advanced edit']
# Reports and provenance: text about the edit, not settings a render depends on.
PARAMETER_KEYS_IGNORED = {'full_edit_v1', 'studio_advanced_retouch_v1', 'studio_portrait_auto_v1', 'provenance'}


def launch(exe, port, out):
    env = dict(os.environ,
               WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-port={port}',
               WEBVIEW2_USER_DATA_FOLDER=str(out / 'webview'),
               AURA_TEST_CATALOG=str(out / 'catalog' / 'catalog.sqlite'))
    (out / 'catalog').mkdir(parents=True, exist_ok=True)
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


def to_image(render):
    raw = base64.b64decode(render['rgbBase64'])
    return Image.frombytes('RGB', (render['width'], render['height']), raw)


def mean_difference(a, b):
    if a.size != b.size:
        b = b.resize(a.size)
    small_a, small_b = a.resize((256, 256)), b.resize((256, 256))
    pa, pb = small_a.tobytes(), small_b.tobytes()
    return sum(abs(x - y) for x, y in zip(pa, pb)) / len(pa)


def parameters(body):
    """The recipe without its reports and provenance: what a render depends on."""
    return {k: v for k, v in body.items() if k not in PARAMETER_KEYS_IGNORED}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('folder', type=Path)
    parser.add_argument('--out', type=Path, default=Path('I:/aura-full-edit/gui'))
    parser.add_argument('--port', type=int, default=9381)
    parser.add_argument('--bring-back', type=int, default=1)
    args = parser.parse_args()
    exe = Path(os.environ.get('AURA_EXE', ROOT / 'target/desktop-launch/debug/aura-desktop.exe'))
    out = args.out
    if out.exists():
        shutil.rmtree(out)
    (out / 'before').mkdir(parents=True)
    (out / 'after').mkdir(parents=True)
    folder = args.folder.resolve()
    total = sum(1 for p in folder.iterdir() if p.is_file())
    report = {'exe': str(exe), 'folder': str(folder), 'files': total, 'checks': {}, 'pageErrors': [],
              'timings': {}, 'photos': []}

    def check(name, ok, detail=''):
        report['checks'][name] = {'ok': bool(ok), 'detail': detail}
        print(('PASS ' if ok else 'FAIL ') + name + (f' - {detail}' if detail else ''), flush=True)

    def save():
        (out / 'report.json').write_text(json.dumps(report, indent=2), encoding='utf-8')

    proc = launch(exe, args.port, out)
    time.sleep(2)
    assert (out / 'catalog' / 'catalog.sqlite').exists(), 'the catalogue is not isolated - stop'
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
            page.set_default_timeout(6 * 3600 * 1000)
            expect.set_options(timeout=6 * 3600 * 1000)
            page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
            page.on('pageerror', lambda e: report['pageErrors'].append(str(e)[:300]))

            def call(command, **kwargs):
                return page.evaluate('async ([c,a])=>await window.__TAURI_INTERNALS__.invoke(c,{input:a})',
                                     [command, kwargs])

            def raw(command, **kwargs):
                return page.evaluate('async ([c,a])=>await window.__TAURI_INTERNALS__.invoke(c,a)', [command, kwargs])

            def render(photo_id):
                return to_image(call('render_image', photoId=photo_id, level='screen', screen=[1200, 1200],
                                     colourSpace=None, purpose='export'))

            # The operating system's folder dialog cannot be clicked by a script; the app's pickers
            # return this folder instead (`testPick` in ui/src/ipc/client.ts). Nothing else is stubbed.
            page.evaluate('folder => { window.__AURA_TEST_PICK__ = folder; }', str(folder))

            # 1. Start -> Choose a folder.
            page.get_by_role('button', name='Start Look, reference, photos', exact=True).click()
            started = time.monotonic()
            page.get_by_role('button', name='Choose a folder', exact=True).click()

            # 2. The cull runs on its own and waits for confirmation.
            confirm = page.get_by_role('region', name='Confirm the best photos')
            expect(confirm).to_be_visible()
            expect(confirm.get_by_role('button', name=re.compile(r'^Edit the \d+ best photos'))).to_be_enabled()
            report['timings']['importAndCullSeconds'] = round(time.monotonic() - started, 1)
            # The isolated catalogue holds only the collection this import created.
            project, photos = None, []
            for candidate in raw('list_projects'):
                listed = call('list_images', projectId=candidate['id'], offset=0, limit=1000, orderBy='timeline')
                if len(listed) > len(photos):
                    project, photos = candidate['id'], listed
            check('every file was imported', len(photos) == total, f'{len(photos)} of {total}')
            if len(photos) != total:
                raise SystemExit('the import did not come from the test folder - stop')
            review = raw('photo_cull_review', projectId=project)
            check('the cull ran on its own and covers every photo', review and review['frames'] == len(photos),
                  f"{review and review['frames']} culled: {confirm.inner_text().splitlines()[0]}")
            check('nothing was edited before confirming',
                  all(not json.loads(raw('image_recipe', input={'photoId': p['id']})['body']).get('full_edit_v1')
                      for p in photos[:10]))
            page.screenshot(path=str(out / '1-cull-confirm.png'))
            report['cull'] = {'best': review['best'], 'leftOut': review['leftOut'], 'strictness': review['strictness'],
                              'leftOutReasons': {}}
            for p in review['photos']:
                if not p['keep']:
                    reasons = report['cull']['leftOutReasons']
                    reasons[p['reasonLabel']] = reasons.get(p['reasonLabel'], 0) + 1

            # The photographer checks the left-out list and brings photos back.
            brought = []
            if args.bring_back > 0 and review['leftOut'] > 0:
                page.get_by_role('tab', name=f"Left out ({review['leftOut']})", exact=True).click()
                for p in [p for p in review['photos'] if not p['keep']][:args.bring_back]:
                    page.get_by_role('article', name=p['fileName'], exact=True).get_by_role('button', name='Bring back').click()
                    brought.append(p['photoId'])
                    expect(page.get_by_role('tab', name=re.compile(r'^Left out \(\d+\)$'))).to_have_text(
                        f"Left out ({review['leftOut'] - len(brought)})")
            review = raw('photo_cull_review', projectId=project)
            best = [p for p in review['photos'] if p['keep']]
            left = [p for p in review['photos'] if not p['keep']]
            check('photos brought back are in the best set', all(any(b['photoId'] == x for b in best) for x in brought),
                  f'{len(brought)} brought back')

            # Before renders, from the app, for the comparison afterwards.
            names = {p['id']: p['fileName'] for p in photos}
            for p in best:
                render(p['photoId']).save(out / 'before' / f"{names[p['photoId']]}.jpg", quality=90)

            # 3. Confirm, and the full automatic edit works through the best set one photo at a time.
            button = confirm.get_by_role('button', name=f'Edit the {len(best)} best photos →', exact=True)
            expect(button).to_be_enabled()
            started = time.monotonic()
            button.click()
            status = page.locator('.automatic-result > p[role=status]')
            finished = re.compile(r'photos edited: all ten stages ran on each')
            last = ''
            stages_seen = set()
            while True:
                text = status.inner_text() if status.count() else ''
                if text != last:
                    last = text
                    m = re.search(r'stage (\d+) of 10: (.+)$', text)
                    if m:
                        stages_seen.add(m.group(2))
                    print(f'  [{round(time.monotonic() - started)}s] {text}', flush=True)
                if finished.search(text):
                    break
                time.sleep(2)
            report['timings']['editSeconds'] = round(time.monotonic() - started, 1)
            report['timings']['secondsPerPhoto'] = round(report['timings']['editSeconds'] / max(1, len(best)), 1)
            check('the window said which stage was running', len(stages_seen) >= 5, ', '.join(sorted(stages_seen)))
            check('every confirmed photo was edited', status.inner_text().startswith(f'{len(best)} of {len(best)} photos edited'),
                  status.inner_text())
            page.get_by_text(re.compile(r"^Review every photo's ten stages")).click()
            page.screenshot(path=str(out / '2-edited.png'), full_page=True)
            log_items = page.locator('.preparation-log > li')
            shown_seconds = {}
            for i in range(log_items.count()):
                head = log_items.nth(i).locator('strong').first.inner_text()
                m = re.match(r'^(.+?) · Edited · ([\d.]+) s$', head)
                if m:
                    shown_seconds[m.group(1)] = float(m.group(2))

            # 4. Verification, from the app.
            tally = {s: {} for s in STAGES}
            sheet_rows = []
            missing_report, unchanged_render = [], []
            for p in best:
                pid, name = p['photoId'], names[p['photoId']]
                body = json.loads(raw('image_recipe', input={'photoId': pid})['body'])
                stored = body.get('full_edit_v1')
                after = render(pid)
                after.save(out / 'after' / f'{name}.jpg', quality=90)
                before = Image.open(out / 'before' / f'{name}.jpg')
                diff = mean_difference(before, after)
                entry = {'file': name, 'seconds': shown_seconds.get(name), 'renderChange': round(diff, 2), 'stages': {}}
                if not stored or len(stored.get('stages', [])) != 10:
                    missing_report.append(name)
                else:
                    for s in stored['stages']:
                        entry['stages'][s['title']] = {'outcome': s['outcome'], 'change': (s['changes'] or s['checks'] or [''])[0]}
                        tally[s['title']][s['outcome']] = tally[s['title']].get(s['outcome'], 0) + 1
                if diff < 0.5:
                    unchanged_render.append(name)
                report['photos'].append(entry)
                sheet_rows.append((name, before, after))
            check('every edited photo stores its ten-stage report', not missing_report, ', '.join(missing_report[:5]))
            check('every edited photo renders differently from its original', not unchanged_render,
                  ', '.join(unchanged_render[:5]))
            report['tally'] = tally
            untouched = [p for p in left if json.loads(raw('image_recipe', input={'photoId': p['photoId']})['body']).get('full_edit_v1')]
            check('left-out photos were not edited', not untouched, f'{len(left)} left out')

            # A second run on two photos changes no parameter: the edit is repeatable.
            repeat_ok = []
            for p in best[:2]:
                was = parameters(json.loads(raw('image_recipe', input={'photoId': p['photoId']})['body']))
                raw('full_edit_photo', input={'projectId': project, 'photoId': p['photoId'], 'look': 'none'})
                now = parameters(json.loads(raw('image_recipe', input={'photoId': p['photoId']})['body']))
                repeat_ok.append(was == now)
            check('running the full edit again changes no parameter', all(repeat_ok), str(repeat_ok))
            check('no page errors', not report['pageErrors'], '; '.join(report['pageErrors'][:3]))

            # A contact sheet: before | after, four pairs a row.
            if sheet_rows:
                thumb = 360
                cols = 4
                rows = (len(sheet_rows) + cols - 1) // cols
                sheet = Image.new('RGB', (cols * (thumb * 2 + 30), rows * (thumb + 40)), (24, 24, 24))
                draw = ImageDraw.Draw(sheet)
                for i, (name, before, after) in enumerate(sheet_rows):
                    x, y = (i % cols) * (thumb * 2 + 30), (i // cols) * (thumb + 40)
                    for j, im in enumerate((before, after)):
                        im = im.copy()
                        im.thumbnail((thumb, thumb))
                        sheet.paste(im, (x + j * (thumb + 6), y + 20))
                    draw.text((x + 4, y + 4), name, fill=(220, 220, 220))
                sheet.save(out / 'contact-sheet.jpg', quality=85)
            save()
    finally:
        save()
        proc.terminate()
        try:
            proc.wait(timeout=20)
        except subprocess.TimeoutExpired:
            proc.kill()
    failed = [k for k, v in report['checks'].items() if not v['ok']]
    print(f"\n{len(report['checks']) - len(failed)} of {len(report['checks'])} checks passed."
          + (f" Failed: {', '.join(failed)}" if failed else ''))
    raise SystemExit(1 if failed else 0)


if __name__ == '__main__':
    main()
