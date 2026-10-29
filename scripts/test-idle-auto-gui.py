"""Nobody at the screen: the automatic workflow goes on by itself after an idle minute. ADR-0189.

    python scripts/test-idle-auto-gui.py <folder of photos> [--out DIR] [--port 9382]

Starts the debug build (`AURA_EXE`) on an isolated catalogue, presses Start -> Choose a folder
(the folder dialog answered by `window.__AURA_TEST_PICK__`) and then sends no input at all. It
checks, from the window, that:

1. the cull runs, the Cull screen shows the countdown, and about a minute later the best set is
   edited without "Edit the N best photos" being pressed;
2. once every photo is edited, the countdown appears again and about a minute later the Export
   screen opens without "Render final output" being pressed;
3. about a minute after that the finished photos are exported, without "Run export" being pressed,
   into a new folder under `Pictures\\AURA Exports`, one verified file per edited photo.

The export folder the run created is removed at the end, unless `--keep-export`.
"""
import argparse
import importlib.util
import json
import re
import shutil
import time
from pathlib import Path

from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parent.parent
_spec = importlib.util.spec_from_file_location('full_auto', ROOT / 'scripts' / 'test-full-auto-gui.py')
full_auto = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(full_auto)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('folder', type=Path)
    parser.add_argument('--out', type=Path, default=Path('I:/aura-idle-test/gui'))
    parser.add_argument('--port', type=int, default=9382)
    parser.add_argument('--keep-export', action='store_true')
    args = parser.parse_args()
    import os
    exe = Path(os.environ.get('AURA_EXE', ROOT / 'target/desktop-launch/debug/aura-desktop.exe'))
    out = args.out
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)
    folder = args.folder.resolve()
    report = {'exe': str(exe), 'folder': str(folder), 'checks': {}, 'timings': {}, 'pageErrors': []}

    def check(name, ok, detail=''):
        report['checks'][name] = {'ok': bool(ok), 'detail': detail}
        print(('PASS ' if ok else 'FAIL ') + name + (f' - {detail}' if detail else ''), flush=True)

    proc = full_auto.launch(exe, args.port, out)
    time.sleep(2)
    assert (out / 'catalog' / 'catalog.sqlite').exists(), 'the catalogue is not isolated - stop'
    export_dir = None
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
            page.set_default_timeout(3 * 3600 * 1000)
            expect.set_options(timeout=3 * 3600 * 1000)
            page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
            page.on('pageerror', lambda e: report['pageErrors'].append(str(e)[:300]))

            def raw(command, **kwargs):
                return page.evaluate('async ([c,a])=>await window.__TAURI_INTERNALS__.invoke(c,a)', [command, kwargs])

            def wait_text(pattern, what):
                """Poll the page's text without sending it any input."""
                started = time.monotonic()
                while True:
                    text = page.evaluate('() => document.body.innerText')
                    m = re.search(pattern, text)
                    if m:
                        return m, round(time.monotonic() - started, 1)
                    if time.monotonic() - started > 3 * 3600:
                        raise RuntimeError(f'timed out waiting for {what}')
                    time.sleep(0.5)

            page.evaluate('folder => { window.__AURA_TEST_PICK__ = folder; }', str(folder))
            page.get_by_role('button', name='Start Look, reference, photos', exact=True).click()
            page.get_by_role('button', name='Choose a folder', exact=True).click()
            # From here on the script sends the window nothing: it only reads.

            # 1. The cull, then a minute, then the edit.
            m, _ = wait_text(r'AURA will edit the (\d+) best photos in (\d:\d\d)', 'the cull countdown')
            best = int(m.group(1))
            shown_at = time.monotonic()
            check('the Cull screen counts down to editing the best set', m.group(2) in ('1:00', '0:59'), m.group(0))
            page.screenshot(path=str(out / '1-cull-countdown.png'))
            _, _ = wait_text(r'Preparing photos|stage \d+ of 10|photos edited: all ten stages', 'the edit to start')
            waited = round(time.monotonic() - shown_at, 1)
            report['timings']['cullWaitSeconds'] = waited
            check('the best set was edited after about a minute, with no click', 58 <= waited <= 75, f'{waited} s')

            # 2. Every photo edited, then a minute, then the Export screen.
            m, edit_seconds = wait_text(r'(\d+) of (\d+) photos edited: all ten stages ran on each', 'the edit to finish')
            report['timings']['editSeconds'] = edit_seconds
            check('every best photo was edited', m.group(1) == m.group(2) == str(best), m.group(0))
            m, _ = wait_text(r'AURA will go on to render the (\d+) edited photos in (\d:\d\d)', 'the render countdown')
            shown_at = time.monotonic()
            page.screenshot(path=str(out / '2-render-countdown.png'))
            wait_text(r'Render your final output\.', 'the Export screen')
            waited = round(time.monotonic() - shown_at, 1)
            report['timings']['renderWaitSeconds'] = waited
            check('the Export screen opened after about a minute, with no click', 58 <= waited <= 75, f'{waited} s')

            # 3. A minute, then the export into a new folder in Pictures\AURA Exports.
            m, _ = wait_text(r'AURA will export the (\d+) photos into a new folder in Pictures\\AURA Exports in (\d:\d\d)',
                             'the export countdown')
            check('the export covers the edited photos', m.group(1) == str(best), m.group(0))
            shown_at = time.monotonic()
            page.screenshot(path=str(out / '3-export-countdown.png'))
            run = page.get_by_test_id('run')
            # The timer fills in the folder and runs the export in the same moment.
            while run.inner_text() != 'Exporting…' and not page.get_by_test_id('destination').input_value():
                time.sleep(0.25)
            waited = round(time.monotonic() - shown_at, 1)
            report['timings']['exportWaitSeconds'] = waited
            check('the export started after about a minute, with no click', 58 <= waited <= 75, f'{waited} s')
            while run.inner_text() == 'Exporting…':
                time.sleep(1)
            destination = page.get_by_test_id('destination').input_value()
            export_dir = Path(destination)
            check('the export went to a new folder in Pictures\\AURA Exports',
                  export_dir.parent.name == 'AURA Exports' and export_dir.is_dir(), destination)
            page.screenshot(path=str(out / '4-exported.png'), full_page=True)
            banner = page.locator('[role=alert]')
            problem = banner.inner_text() if banner.count() else ''
            check('the export finished without an error', not problem, problem)
            written = page.get_by_test_id('written').inner_text() if page.get_by_test_id('written').count() else '?'
            verified = page.get_by_test_id('verified').inner_text() if page.get_by_test_id('verified').count() else '?'
            files = [p for p in export_dir.rglob('*') if p.is_file() and p.suffix.lower() in {'.jpg', '.jpeg'}]
            check('one file written and verified per edited photo', written == verified == str(best) and len(files) == best,
                  f'{written} written, {verified} verified, {len(files)} files, {best} edited')
            check('no page errors', not report['pageErrors'], '; '.join(report['pageErrors'][:3]))
    finally:
        proc.terminate()
        if export_dir and export_dir.parent.name == 'AURA Exports' and export_dir.is_dir() and not args.keep_export:
            shutil.rmtree(export_dir)
        (out / 'report.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    failed = [k for k, v in report['checks'].items() if not v['ok']]
    print(f"\n{len(report['checks']) - len(failed)} of {len(report['checks'])} checks passed", flush=True)
    raise SystemExit(1 if failed else 0)


if __name__ == '__main__':
    main()
