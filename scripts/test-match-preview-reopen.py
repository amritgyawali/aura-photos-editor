"""Reopen an isolated twenty-photo run and verify its actual Your photograph pane.

Reads the last saved workflow for each target. No matching or recipe writes are
performed. PNG pixels read from the visible Edited image must equal the saved
native output, and the Original button must show the recorded unedited render.
"""
import argparse
import base64
import hashlib
from io import BytesIO
import json
import os
from pathlib import Path
import subprocess
import time
import urllib.request

from PIL import Image
from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parents[1]


def displayed_pixels(page, label, expected):
    image = page.get_by_role('figure', name='Active photograph preview', exact=True).get_by_role('img', name=label, exact=True)
    deadline = time.monotonic() + 90
    while time.monotonic() < deadline:
        if image.count():
            encoded = image.evaluate("""img => {
                if (!img.complete || !img.naturalWidth) return null;
                const canvas = document.createElement('canvas');
                canvas.width = img.naturalWidth; canvas.height = img.naturalHeight;
                canvas.getContext('2d').drawImage(img, 0, 0);
                return canvas.toDataURL('image/png').split(',')[1];
            }""")
            if encoded:
                with Image.open(BytesIO(base64.b64decode(encoded))) as actual:
                    if actual.size == expected.size and actual.convert('RGB').tobytes() == expected.tobytes():
                        return True
        time.sleep(.25)
    raise AssertionError(f'The displayed {label} pixels did not reach the saved native render.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('run', type=Path)
    parser.add_argument('--port', type=int, default=9411)
    args = parser.parse_args()
    run = args.run.resolve()
    if run.parent != ROOT / 'output/aura-main-match-twenty-2026-10-25':
        raise ValueError('Use an isolated run created by test-main-match-twenty.py.')
    data = json.loads((run / 'results.json').read_text(encoding='utf-8'))
    assert data['completed'] and all(data['checks'].values())
    with Path(data['exe']).open('rb') as executable:
        assert hashlib.file_digest(executable, 'sha256').hexdigest() == data['exeSha256']
    last = {row['targetIndex']: row for row in data['pairs']}
    assert len(last) == data['targetCount']
    report = dict(exeSha256=data['exeSha256'], photos=[], pageErrors=[],
                  method='Actual reopened Edited/Original image pixels read through a browser canvas. Last saved workflow only; no matching or recipe writes.')
    env = dict(os.environ, AURA_TEST_CATALOG=str(run / 'catalog.sqlite'),
               WEBVIEW2_USER_DATA_FOLDER=str(run / 'webview-preview-reopen'),
               WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-address=127.0.0.1 --remote-debugging-port={args.port}')
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    with (run / 'preview-reopen-desktop.log').open('w', encoding='utf-8') as log:
        process = subprocess.Popen([data['exe']], cwd=ROOT, env=env, startupinfo=startup,
                                   stdout=log, stderr=subprocess.STDOUT)
        try:
            for _ in range(90):
                if process.poll() is not None:
                    raise RuntimeError('Desktop exited during preview verification.')
                try:
                    urllib.request.urlopen(f'http://127.0.0.1:{args.port}/json/version', timeout=1).close()
                    break
                except OSError:
                    time.sleep(1)
            with sync_playwright() as pw:
                page = pw.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}').contexts[0].pages[0]
                page.set_default_timeout(120000)
                expect.set_options(timeout=120000)
                page.wait_for_function('()=>!!window.__TAURI_INTERNALS__')
                page.on('pageerror', lambda error: report['pageErrors'].append(str(error)))
                page.set_viewport_size(dict(width=1920, height=1080))

                def call(command, **kwargs):
                    return page.evaluate('async ([c,a])=>window.__TAURI_INTERNALS__.invoke(c,{input:a})', [command, kwargs])

                photos = call('list_images', projectId=data['projectId'], offset=0, limit=60, orderBy='timeline')
                assert len(photos) == len(last)
                name = 'Twenty-photo Match a look ' + run.name
                page.get_by_role('button', name=f'{name} {len(last)}', exact=True).click()
                page.get_by_role('button', name='Match a look Your reference, your photos', exact=True).click()
                for index, row in sorted(last.items()):
                    photo = row['auraPhotoId']
                    recipe = call('image_recipe', photoId=photo)['recipeHash']
                    file_name = next(item['fileName'] for item in photos if item['id'] == photo)
                    page.get_by_role('listbox', name='Filmstrip').get_by_role('option', name=file_name, exact=True).click()
                    with Image.open(run / f'{row["stem"]}.matched.png') as expected:
                        expect(page.get_by_role('button', name='Edited', exact=True)).to_be_enabled()
                        page.get_by_role('button', name='Edited', exact=True).click()
                        displayed_pixels(page, 'Edited photograph', expected.convert('RGB'))
                    with Image.open(run / f'{index:02}.original.png') as original:
                        page.get_by_role('button', name='Original', exact=True).click()
                        displayed_pixels(page, 'Original photograph', original.convert('RGB'))
                    page.get_by_role('button', name='Edited', exact=True).click()
                    assert call('image_recipe', photoId=photo)['recipeHash'] == recipe
                    report['photos'].append(dict(target=index, workflow=row['workflow'],
                                                 editedPaneEqualsSaved=True, originalPaneEqualsUnedited=True,
                                                 outputEqualsOriginal=row['originalPixelIdentical'], recipeUnchanged=True))
                    print(f'Preview {index:02}: saved and original panes exact', flush=True)
                page.screenshot(path=str(run / 'preview-reopened.png'), full_page=True)
                assert not report['pageErrors']
                report['completed'] = True
        except BaseException as error:
            report['failure'] = f'{type(error).__name__}: {error}'
            raise
        finally:
            (run / 'preview-reopen.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
            process.terminate()
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=10)


if __name__ == '__main__':
    main()
