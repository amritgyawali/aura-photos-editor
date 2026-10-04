"""Verify split comparison and draft protection in a running AURA desktop.

Reuses a completed five-portrait test collection. Does not save any image edits.
Requires Playwright and a desktop with WebView2 CDP enabled.
"""
import argparse
import hashlib
import json
import re
from pathlib import Path

from playwright.sync_api import expect, sync_playwright


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fixture-report', type=Path, required=True)
    parser.add_argument('--photos', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--endpoint', default='http://127.0.0.1:9223')
    args = parser.parse_args()
    fixture = json.loads(args.fixture_report.read_text(encoding='utf-8'))
    assert fixture['status'] == 'passed' and len(fixture['photos']) == 5
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    report = {'status': 'running', 'photos': [], 'projectId': fixture['projectId']}

    def hashes():
        return {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                for p in args.photos.glob('*.jpg')}

    assert hashes() == fixture['originalHashes']
    with sync_playwright() as playwright:
        browser = playwright.chromium.connect_over_cdp(args.endpoint)
        page = next(p for c in browser.contexts for p in c.pages
                    if 'tauri' in p.url or 'localhost' in p.url)
        page.set_default_timeout(60_000)
        expect.set_options(timeout=60_000)

        def invoke(command, data):
            return page.evaluate('''async ([command, args]) => {
                try { return await window.__TAURI_INTERNALS__.invoke(command, args); }
                catch (error) { throw new Error(command + ': ' + JSON.stringify(error)); }
            }''', [command, data])

        def state(photo):
            return {
                'edits': invoke('native_retouch_edit', {'input': {
                    'projectId': fixture['projectId'], 'photoId': photo,
                    'action': 'list', 'edits': [], 'id': None}}),
                'history': invoke('image_history', {'input': {'photoId': photo}}),
            }

        try:
            page.reload()
            page.get_by_role('button', name=f"{fixture['collectionName']} 5", exact=True).click()
            page.get_by_role('button', name='Auto edit One click, start to finish', exact=True).click()
            for index, photo in enumerate(fixture['photos']):
                page.get_by_role('option', name=photo['file'], exact=True).click()
                page.get_by_role('button', name='Retouch', exact=True).click()
                expect(page.get_by_alt_text('Retouched photograph', exact=True)).to_be_visible()
                toggle = page.get_by_role('button', name='Split comparison', exact=True)
                expect(toggle).to_be_enabled()
                baseline = state(photo['id'])
                target = page.get_by_label('Center X (%)', exact=True).input_value()
                toggle.click()
                slider = page.get_by_label('Before/after split', exact=True)
                slider.focus()
                slider.press('End')
                expect(slider).to_have_value('100')
                slider.press('Home')
                expect(slider).to_have_value('0')
                slider.press('ArrowRight')
                expect(slider).to_have_value('1')
                page.get_by_role('button', name='Center divider', exact=True).click()
                page.get_by_role('button', name='Zoom in', exact=True).click()
                after = page.get_by_alt_text('Retouched photograph', exact=True)
                before = page.get_by_alt_text('Before native retouch comparison', exact=True)
                expect(before).to_be_visible()
                assert before.bounding_box() == after.bounding_box(), 'Comparison images are misaligned'
                surface = page.get_by_label('Retouch image interaction', exact=True)
                bounds = surface.bounding_box()
                viewport = page.get_by_label('Retouch photo viewport', exact=True).bounding_box()
                grip = page.locator('.retouch-compare-divider span').bounding_box()
                goal_x = min(bounds['x'] + bounds['width'] * .65,
                             viewport['x'] + viewport['width'] - 25)
                grip_y = grip['y'] + grip['height'] / 2
                page.mouse.move(grip['x'] + grip['width'] / 2, grip_y)
                page.mouse.down()
                page.mouse.move(goal_x, grip_y, steps=5)
                page.mouse.up()
                expected = (goal_x - bounds['x']) / bounds['width'] * 100
                assert abs(float(slider.input_value()) - expected) <= 1
                assert page.get_by_label('Center X (%)', exact=True).input_value() == target
                expect(page.get_by_role('button', name='Discard draft', exact=True)).to_have_count(0)
                assert state(photo['id']) == baseline, 'Comparison wrote an edit or history entry'

                page.get_by_role('button', name='Fit', exact=True).click()
                page.get_by_role('button', name='Center divider', exact=True).click()
                after.scroll_into_view_if_needed()
                page.screenshot(path=str(output / f"{photo['file']}-comparison.png"), full_page=True)
                toggle.click()
                page.get_by_label('Center X (%)', exact=True).fill('61')
                for label in ['Undo', 'Clear native retouch', 'Duplicate retouch 1', 'Remove retouch 1']:
                    expect(page.get_by_role('button', name=label, exact=True)).to_be_disabled()
                surface.focus()
                surface.press('Control+z')
                assert page.get_by_label('Center X (%)', exact=True).input_value() == '61'
                assert state(photo['id']) == baseline
                page.get_by_role('button', name='Discard draft', exact=True).click()

                if index == 0:
                    # A saved operation can be refined, compared as a disposable draft,
                    # and discarded without changing the saved recipe or history.
                    page.get_by_role('button', name='1. Texture-aware patch heal', exact=False).click()
                    page.get_by_label(re.compile(r'^Strength \(')).focus()
                    page.keyboard.press('Home')
                    for _ in range(30):
                        page.keyboard.press('ArrowRight')
                    page.get_by_label('Tool', exact=True).select_option('burn')
                    expect(page.get_by_label('Tool', exact=True)).to_have_value('patch_heal')
                    expect(page.get_by_role('button', name='Start another operation', exact=True)).to_be_disabled()
                    page.get_by_label('Preview unsaved changes', exact=True).check()
                    expect(page.get_by_text('Unsaved preview. Apply to keep this change.', exact=True)).to_be_visible()
                    toggle.click()
                    expect(before).to_be_visible()
                    assert state(photo['id']) == baseline
                    page.get_by_role('button', name='Discard draft', exact=True).click()
                    expect(page.get_by_label('Strength (100%)', exact=True)).to_have_value('1')

                assert state(photo['id']) == baseline
                page.get_by_role('button', name='Back to Develop', exact=True).click()
                expect(page.get_by_role('button', name='Retouch', exact=True)).to_be_enabled()
                report['photos'].append({'file': photo['file'], 'aligned': True,
                                         'keyboardAndPointer': True, 'historyUnchanged': True,
                                         'draftProtected': True})
                print(f"PASS {photo['file']}: comparison and draft protection", flush=True)
            assert hashes() == fixture['originalHashes']
            report.update(status='passed', originalsUnchanged=True)
        except Exception as error:
            report.update(status='failed', error=str(error))
            raise
        finally:
            (output / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')


if __name__ == '__main__':
    main()
