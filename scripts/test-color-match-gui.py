"""One-click AI Color Match in the real desktop window. ADR-0170.

Starts the desktop app against an isolated catalogue with WebView2 CDP, imports the target
photos and the edited references (photos of *other* people and scenes), opens Match a look,
and works the AI Color Match panel the way a photographer would: picks a recommended
reference, checks it appears beside the photo, moves through the preferences up to Extreme,
saves a preset, makes a reference from one photo with Create Reference and checks it is refused
on that same photo but used on another, syncs the collection, switches reference and removes
the match. Every step is timed and screenshotted.

    python scripts/test-color-match-gui.py <targets folder> <references folder> [--port 9371]

`AURA_EXE` names a debug build (a release build ignores the isolated catalogue). The native
file picker cannot be driven through the WebView, so references are imported with the same
command the Upload References button calls, before the panel opens. Writes
`.work-checks/color-match-gui/` (results.json, screenshots, log.txt).
"""
import argparse
import json
import os
import shutil
import subprocess
import time
import urllib.request
import uuid
from pathlib import Path

from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.work-checks/color-match-gui'
RAW = ('.cr3', '.cr2', '.nef', '.arw', '.dng')
PICTURES = ('.jpg', '.jpeg', '.png', '.tif', '.tiff')


def launch(exe, port):
    env = dict(os.environ,
               WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-port={port}',
               WEBVIEW2_USER_DATA_FOLDER=str(OUT / 'webview'),
               AURA_TEST_CATALOG=str(OUT / 'catalog' / 'catalog.sqlite'))
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('targets', type=Path)
    parser.add_argument('references', type=Path)
    parser.add_argument('--port', type=int, default=9371)
    args = parser.parse_args()
    exe = Path(os.environ.get('AURA_EXE', ROOT / 'target/desktop-launch/debug/aura-desktop.exe'))
    OUT.mkdir(parents=True, exist_ok=True)
    # Every run starts from an empty catalogue. The browser profile is kept (a brand-new one can
    # leave the first window blank while the harness attaches); its remembered settings are
    # cleared through the page instead.
    for leftover in ('catalog',):
        shutil.rmtree(OUT / leftover, ignore_errors=True)
    log = (OUT / 'log.txt').open('w', encoding='utf-8')
    targets = sorted(p for p in args.targets.resolve().iterdir() if p.suffix.lower() in RAW + PICTURES)
    references = sorted(p for p in args.references.resolve().iterdir() if p.suffix.lower() in PICTURES)
    assert len(targets) >= 2 and len(references) >= 2, 'need two targets and two references'
    assert not {p.name for p in targets} & {p.name for p in references}, 'references must differ from targets'
    report = {'exe': str(exe), 'targets': [p.name for p in targets], 'references': [p.name for p in references],
              'steps': [], 'checks': {}, 'pageErrors': []}
    proc = launch(exe, args.port)
    started = time.monotonic()

    def step(name, ok, **detail):
        report['steps'].append({'step': name, 'ok': bool(ok), 'at_s': round(time.monotonic() - started, 1), **detail})
        line = ('PASS ' if ok else 'FAIL ') + name + ' ' + json.dumps(detail)[:400]
        print(line, flush=True)
        log.write(line + '\n')
        log.flush()

    try:
        with sync_playwright() as pw:
            browser = pw.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}')
            page = None
            seen = []
            for _ in range(300):
                found = [p for c in browser.contexts for p in c.pages]
                seen = [p.url for p in found]
                page = next((p for p in found if 'localhost' in p.url or 'tauri' in p.url), None)
                if page:
                    break
                time.sleep(1)
            if page is None:
                raise RuntimeError(f'no AURA page after 300 s; pages seen: {seen}')
            page.set_default_timeout(300000)
            expect.set_options(timeout=300000)
            page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
            page.on('pageerror', lambda e: report['pageErrors'].append(str(e)[:300]))

            def call(command, **kwargs):
                return page.evaluate('async ([c,a])=>await window.__TAURI_INTERNALS__.invoke(c,{input:a})',
                                     [command, kwargs])

            def raw(command, **kwargs):
                return page.evaluate('async ([c,a])=>await window.__TAURI_INTERNALS__.invoke(c,a)', [command, kwargs])

            def shot(name):
                page.screenshot(path=str(OUT / f'{name}.png'))
                (OUT / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')

            def status(photo):
                return raw('color_match_status', photoId=photo)

            panel = page.get_by_role('region', name='AI Color Match')
            message = panel.locator('.cm-message')
            alert = panel.get_by_role('alert')

            def settled():
                # The panel is idle when no Stop button is showing; give a click time to start work.
                time.sleep(1.0)
                expect(panel.get_by_role('button', name='Stop', exact=True)).to_have_count(0)

            def open_photo(file_name):
                option = page.get_by_role('option', name=file_name, exact=True)
                for _ in range(8):
                    option.click()
                    time.sleep(1.5)
                    if option.get_attribute('aria-selected') == 'true':
                        break
                expect(panel).to_be_visible()
                expect(page.get_by_alt_text('Edited photograph', exact=True)).to_be_visible()

            name = 'Color match ' + time.strftime('%H-%M-%S')
            project = call('create_project', name=name, coupleNames=None, eventDate=None)['id']
            call('start_ingest', projectId=project, roots=[str(p) for p in targets])
            photos, deadline = [], time.monotonic() + 600
            while len(photos) < len(targets) and time.monotonic() < deadline:
                photos = call('list_images', projectId=project, offset=0, limit=40, orderBy='timeline')
                time.sleep(0.5)
            assert len(photos) == len(targets), f'imported {len(photos)} of {len(targets)}'
            ids = {row['fileName']: row['id'] for row in photos}
            imported = {}
            for ref in references:
                t0 = time.monotonic()
                summary = call('import_color_reference', path=str(ref), cancelId=str(uuid.uuid4()))
                imported[ref.name] = summary['id']
                step(f'import reference {ref.name}', True, seconds=round(time.monotonic() - t0, 1))
            first_ref, second_ref = references[0].name, references[1].name
            first, second = targets[0].name, targets[1].name

            # Start from a fresh panel: remembered tabs and toggles belong to an earlier run.
            page.evaluate('() => localStorage.clear()')
            page.reload()
            page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
            page.get_by_role('button', name=f'{name} {len(photos)}', exact=True).click()
            page.get_by_role('button', name='Match a look Your reference, your photos', exact=True).click()
            open_photo(first)
            step('panel visible in Match a look', True)

            # Recommended lists every reference, best fit first.
            t0 = time.monotonic()
            panel.get_by_role('tab', name='Recommended').click()
            expect(panel.locator('.cm-grid .cm-card').first).to_be_visible()
            expect(panel.locator('.cm-grid .cm-card')).to_have_count(len(references))
            order = panel.locator('.cm-card-name').all_inner_texts()
            step('recommended references listed', True, order=order, seconds=round(time.monotonic() - t0, 1))
            shot('01-recommended')

            # One click.
            t0 = time.monotonic()
            panel.get_by_role('button', name=first_ref, exact=False).first.click()
            expect(message).to_contain_text(f'Matched to {first_ref}')
            saved = status(ids[first])
            step('one click matches the photo', saved and saved['current'] and saved['reference']['id'] == imported[first_ref]
                 and saved['afterDistance'] <= saved['beforeDistance'],
                 seconds=round(time.monotonic() - t0, 1), text=message.inner_text(),
                 before=saved and saved['beforeDistance'], after=saved and saved['afterDistance'],
                 amount=saved and saved['controls']['amount'])
            reference_img = page.get_by_alt_text(f'Reference: {first_ref}', exact=True)
            matched_img = page.get_by_alt_text('Photo matched to the reference', exact=True)
            expect(reference_img).to_be_visible()
            expect(matched_img).to_be_visible()
            src = reference_img.get_attribute('src') or ''
            step('reference shown beside the photo', src.startswith('data:image/jpeg'), sharp_preview=src[:24])
            shot('02-matched')
            history = call('image_history', photoId=ids[first])
            step('unedited photo got a starting point', any(e['label'] == 'Starting point for reference colour'
                                                             for e in history['entries']),
                 labels=[e['label'][:60] for e in history['entries']])

            # Every preference level, each saved or reduced, never a bare refusal.
            for level in ['light', 'natural', 'strong', 'extreme', 'standard']:
                t0 = time.monotonic()
                panel.get_by_role('combobox', name='Color match preference').select_option(level)
                settled()
                text = message.inner_text() if message.count() else ''
                problem = alert.inner_text() if alert.count() else ''
                saved = status(ids[first])
                step(f'preference {level}', bool(saved and saved['current']) and not problem,
                     seconds=round(time.monotonic() - t0, 1), text=text, error=problem,
                     amount=saved and saved['controls']['amount'])
                if level == 'extreme':
                    shot('03-extreme')

            # Save Preset.
            panel.get_by_role('button', name='Save Preset', exact=True).click()
            panel.get_by_role('textbox', name='Preset name').fill('My look')
            panel.get_by_role('button', name='Save preset', exact=True).click()
            expect(panel.get_by_role('button', name='Use preset My look')).to_be_visible()
            step('preset saved', True)
            shot('04-preset')

            # A reference made from this photo is refused on this photo and used on another.
            panel.get_by_role('button', name='Create Reference').click()
            panel.get_by_role('textbox', name='Reference name').fill('This bride edit')
            panel.get_by_role('button', name='Save this edit as a reference', exact=True).click()
            expect(message).to_contain_text('Saved this edit as')
            panel.get_by_role('tab', name='My References').click()
            panel.get_by_role('button', name='This bride edit', exact=False).first.click()
            settled()
            problem = alert.inner_text() if alert.count() else ''
            step('same photo refused as its own reference', 'same photo' in problem, error=problem)
            shot('05-duplicate-refused')
            open_photo(second)
            t0 = time.monotonic()
            panel.get_by_role('tab', name='My References').click()
            panel.get_by_role('button', name='This bride edit', exact=False).first.click()
            settled()
            saved = status(ids[second])
            step('created reference used on a different photo',
                 bool(saved and saved['current'] and saved['reference']['name'] == 'This bride edit'),
                 seconds=round(time.monotonic() - t0, 1), text=message.inner_text() if message.count() else '',
                 error=alert.inner_text() if alert.count() else '')
            shot('06-created-reference-on-another-photo')

            # Sync the whole collection to the second Pexels reference.
            panel.get_by_role('tab', name='My References').click()
            panel.get_by_role('button', name=second_ref, exact=False).first.click()
            settled()
            t0 = time.monotonic()
            panel.get_by_role('button', name='Sync options').click()
            panel.get_by_role('radio', name='Whole collection').check()
            panel.get_by_role('button', name='Sync all', exact=True).click()
            expect(message).to_contain_text('Synced')
            others = [ids[p.name] for p in targets if p.name != second]
            synced = [status(p) for p in others]
            step('sync matched every other photo',
                 all(s and s['current'] and s['reference']['id'] == imported[second_ref] for s in synced),
                 seconds=round(time.monotonic() - t0, 1), text=message.inner_text(),
                 amounts=[s and s['controls']['amount'] for s in synced])
            shot('07-synced')

            # Reference view alone, then remove the match.
            page.get_by_role('button', name='Reference', exact=True).click()
            expect(matched_img).to_have_count(0)
            expect(page.get_by_alt_text(f'Reference: {second_ref}', exact=True)).to_be_visible()
            shot('08-reference-only')
            page.get_by_role('button', name='Color Match', exact=True).click()
            panel.get_by_role('button', name='Remove matched look').click()
            expect(message).to_contain_text('Matched look removed')
            saved = status(ids[second])
            step('remove restores the earlier edit', saved['controls']['amount'] == 0, text=message.inner_text())
            shot('09-removed')
            step('no page errors', not report['pageErrors'], errors=report['pageErrors'][:3])
    except Exception as error:  # noqa: BLE001 - record the failure, then re-raise
        step('harness error', False, error=str(error)[:1500])
        try:
            page.screenshot(path=str(OUT / 'failure.png'))
        except Exception:  # noqa: BLE001
            pass
        raise
    finally:
        report['checks'] = {'passed': sum(s['ok'] for s in report['steps']), 'total': len(report['steps'])}
        (OUT / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
        print(json.dumps(report['checks']))
        log.close()
        proc.terminate()


if __name__ == '__main__':
    main()
