"""Drive the native AURA UI; AURA alone processes and exports the photograph.

Launch a debug custom-protocol build with AURA_TEST_CATALOG set to an absolute
isolated catalog, WEBVIEW2_USER_DATA_FOLDER to an isolated directory, and CDP on
localhost:9337. Requires Python Playwright. No image-editing dependency is used.
"""
import argparse
import hashlib
import json
import re
import shutil
from pathlib import Path
from playwright.sync_api import expect, sync_playwright


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('source', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--port', type=int, default=9337)
    parser.add_argument('--resume', action='store_true')
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    original_hash = hashlib.sha256(args.source.read_bytes()).hexdigest()
    incoming = args.output / 'incoming'
    incoming.mkdir(exist_ok=True)
    shutil.copyfile(args.source, incoming / args.source.name)
    with sync_playwright() as p:
        browser = p.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}')
        page = browser.contexts[0].pages[0]
        page.set_default_timeout(240000)
        expect(page.locator('.aura-studio')).to_be_visible()
        expected = ['Start', 'Photos', 'Auto edit', 'Instagram style', 'Export', 'Advanced']
        assert page.locator('.studio-nav strong').all_text_contents() == expected
        page.emulate_media(color_scheme='light')
        page.evaluate("localStorage.setItem('aura.theme', 'light')")
        page.reload()
        expect(page.locator('.aura-studio')).to_be_visible()
        assert page.evaluate("getComputedStyle(document.documentElement).getPropertyValue('--bg').trim()") == '#19181e'
        assert page.get_by_role('button', name=re.compile('Theme:')).count() == 0
        page.screenshot(path=str(args.output / 'start.png'))
        if not args.resume:
            page.get_by_label('New collection', exact=True).fill('Restored studio verification')
            page.get_by_role('button', name='Create', exact=True).click()
            expect(page.get_by_role('button', name='Restored studio verification 0', exact=True)).to_be_visible()
            page.get_by_role('button', name='Photos Browse your collection', exact=True).click()
            page.get_by_text('Enter folders manually', exact=True).click()
            page.get_by_label('Card or folder', exact=True).fill(str(incoming.resolve()))
            page.get_by_role('button', name='Add folder', exact=True).click()
            page.get_by_role('button', name='Start import', exact=True).click()
        else:
            page.get_by_role('button', name='Restored studio verification 1', exact=True).click()
            page.get_by_role('button', name='Auto edit One click, start to finish', exact=True).click()
        retouch = page.get_by_role('button', name='Retouch', exact=True)
        expect(retouch).to_be_enabled()
        page.get_by_text('Make it yours.', exact=True).scroll_into_view_if_needed()
        page.screenshot(path=str(args.output / 'develop.png'))
        retouch.click()
        page.get_by_role('radio', name='Deep acne cleanup', exact=True).check()
        run = page.get_by_role('button', name='Auto retouch: Face', exact=True)
        run.click()
        print('Running deep cleanup inside AURA', flush=True)
        expect(run).to_be_enabled()
        expect(page.get_by_alt_text('Retouched photograph')).to_be_visible()
        page.get_by_role('heading', name='Precision, at your pace.').scroll_into_view_if_needed()
        page.screenshot(path=str(args.output / 'retouch.png'))
        (args.output / 'retouch.txt').write_text(page.locator('main').inner_text(), encoding='utf-8')
        page.get_by_role('button', name='Export Ready to share', exact=True).click()
        page.get_by_test_id('destination').fill(str((args.output / 'export').resolve()))
        page.get_by_test_id('verify').check()
        page.get_by_test_id('run').click()
        expect(page.get_by_test_id('run')).to_be_enabled()
        manifest = json.loads((args.output / 'export/aura-delivery-manifest.json').read_text())
        assert manifest['verified'] and manifest['file_count'] == 1, manifest
        assert hashlib.sha256(args.source.read_bytes()).hexdigest() == original_hash
        page.screenshot(path=str(args.output / 'export.png'))
        print(json.dumps({'verified': True, 'original_unchanged': True, 'manifest': manifest}), flush=True)


if __name__ == '__main__':
    main()
