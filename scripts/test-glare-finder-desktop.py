"""Check "Find and remove reflections" in the real, isolated Windows desktop.

Two photographs: a real portrait with a synthetic lens reflection added over one eye (it must be
found where it was put, removed, and be undoable in one step), and the same portrait clean (it
must report no reflection and save nothing). No native command is mocked.

usage: test-glare-finder-desktop.py VEILED_PNG CLEAN_PNG CX CY [--exe EXE] [--port PORT]
"""
import argparse
import base64
import json
import os
from pathlib import Path
import re
import subprocess
import time
import urllib.request
import uuid

from PIL import Image
from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('veiled', type=Path)
    parser.add_argument('clean', type=Path)
    parser.add_argument('cx', type=float)
    parser.add_argument('cy', type=float)
    parser.add_argument('--exe', type=Path, default=ROOT / 'target/desktop-launch/debug/aura-desktop.exe')
    parser.add_argument('--port', type=int, default=9421)
    args = parser.parse_args()
    run = ROOT / 'output/aura-glare-finder' / ('run-' + uuid.uuid4().hex[:8])
    run.mkdir(parents=True)
    result = dict(run=str(run), cases=[])
    env = dict(os.environ, AURA_TEST_CATALOG=str(run / 'catalog.sqlite'),
               WEBVIEW2_USER_DATA_FOLDER=str(run / 'webview'),
               WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-address=127.0.0.1 --remote-debugging-port={args.port}')
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    with (run / 'desktop.log').open('a', encoding='utf-8') as log:
        process = subprocess.Popen([str(args.exe.resolve())], cwd=ROOT, env=env, startupinfo=startup,
                                   stdout=log, stderr=subprocess.STDOUT)
        try:
            for _ in range(90):
                if process.poll() is not None:
                    raise RuntimeError('Desktop exited at startup')
                try:
                    urllib.request.urlopen(f'http://127.0.0.1:{args.port}/json/version', timeout=1).close()
                    break
                except OSError:
                    time.sleep(1)
            with sync_playwright() as pw:
                page = pw.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}').contexts[0].pages[0]
                page.set_default_timeout(120000)
                expect.set_options(timeout=120000)
                page.set_viewport_size(dict(width=1920, height=1080))
                page.wait_for_function('()=>!!window.__TAURI_INTERNALS__')
                errors = []
                page.on('pageerror', lambda e: errors.append(str(e)))

                def call(command, **kwargs):
                    return page.evaluate('async ([c,a])=>window.__TAURI_INTERNALS__.invoke(c,a)', [command, dict(input=kwargs)])

                def render(photo):
                    px = call('render_image', photoId=photo, level='full', purpose='export')
                    return Image.frombytes('RGB', (px['width'], px['height']), base64.b64decode(px['rgbBase64']))

                def edits(photo):
                    return page.evaluate('async ([p,i])=>window.__TAURI_INTERNALS__.invoke("native_retouch_edit",{input:{projectId:p,photoId:i,action:"list",edits:[],id:null}})', [project, photo])

                name = 'Glare finder ' + run.name
                project = call('create_project', name=name, coupleNames=None, eventDate=None)['id']
                call('start_ingest', projectId=project, roots=[str(args.veiled.resolve()), str(args.clean.resolve())])
                deadline = time.monotonic() + 120
                while time.monotonic() < deadline:
                    photos = call('list_images', projectId=project, offset=0, limit=10, orderBy='timeline')
                    if len(photos) == 2:
                        break
                    time.sleep(0.5)
                ids = {p['fileName']: p['id'] for p in photos}

                for label, path, expect_found in [('veiled', args.veiled, True), ('clean', args.clean, False)]:
                    photo = ids[path.name]
                    before = render(photo)
                    depth = len(call('image_history', photoId=photo)['entries'])
                    page.reload()
                    page.get_by_role('button', name=f'{name} 2', exact=True).click()
                    page.get_by_role('navigation', name='Workspace').get_by_role('button', name=re.compile(r'^Hot Features')).click()
                    page.get_by_role('button', name='Remove Glasses Glare', exact=True).click()
                    page.get_by_role('listbox', name='Filmstrip').get_by_role('option', name=path.name, exact=True).click()
                    button = page.get_by_role('button', name='Find and remove reflections', exact=True)
                    expect(button).to_be_enabled()
                    button.click()
                    note = page.get_by_text(re.compile(r'^(Found \d+ lens reflection|No lens reflection|No face was found|No face large enough)'))
                    expect(note).to_be_visible()
                    message = note.inner_text()
                    saved = edits(photo)
                    after = render(photo)
                    case = dict(case=label, message=message, edits=[dict(tool=e['tool'], region=e['region']) for e in saved],
                                changed=before.tobytes() != after.tobytes())
                    if expect_found:
                        assert saved and all(e['tool'] == 'glare' for e in saved), case
                        near = min(abs(e['region'][0] - args.cx) + abs(e['region'][1] - args.cy) for e in saved)
                        case['distanceToReflection'] = near
                        assert near < 0.05, case
                        assert case['changed'], case
                        added = len(call('image_history', photoId=photo)['entries']) - depth
                        case['historySteps'] = added
                        for _ in range(added):
                            call('history_step', projectId=project, photoId=photo, action='undo')
                        assert render(photo).tobytes() == before.tobytes(), 'undo did not restore the photograph'
                        case['undoRestores'] = True
                        after.save(run / f'{label}.after.png')
                    else:
                        assert not saved and not case['changed'], case
                    page.screenshot(path=str(run / f'{label}.ui.png'), full_page=True)
                    result['cases'].append(case)
                    print(json.dumps(case), flush=True)
                result['pageErrors'] = errors
                assert not errors, errors
                result['passed'] = True
        except BaseException as error:
            result['failure'] = f'{type(error).__name__}: {error}'
            raise
        finally:
            try:
                (run / 'results.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
            finally:
                process.terminate()
                try:
                    process.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    process.kill()


if __name__ == '__main__':
    main()
