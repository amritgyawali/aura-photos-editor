"""Exercise local desktop workspaces in a separate collection via native UI controls."""
import argparse
from collections import Counter
import hashlib
import json
import re
import shutil
import time
from pathlib import Path

from PIL import Image
from playwright.sync_api import expect, sync_playwright


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    parser.add_argument('sources', type=Path, nargs='+')
    parser.add_argument('--port', type=int, default=9348)
    parser.add_argument('--skin-cleanup', action='store_true',
                        help='Save face-and-body pore cleanup for each photo, then verify the collection edit retains it')
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    incoming = args.output / 'incoming'
    incoming.mkdir(exist_ok=True)
    hashes = {}
    for index, source in enumerate(args.sources):
        copied = incoming / f'{index}-{source.name}'
        shutil.copyfile(source, copied)
        hashes[str(copied)] = hashlib.sha256(copied.read_bytes()).hexdigest()
    report = {'steps': [], 'page_errors': []}
    with sync_playwright() as p:
        page = p.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}').contexts[0].pages[0]
        page.set_default_timeout(60000)
        expect.set_options(timeout=60000)
        page.on('pageerror', lambda error: report['page_errors'].append(str(error)))

        def save():
            (args.output / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')

        def step(name, action):
            started = time.monotonic()
            try:
                result = action()
                report['steps'].append({'name': name, 'passed': True, 'result': result,
                                        'seconds': round(time.monotonic() - started, 2)})
                page.screenshot(path=str(args.output / f'{len(report["steps"]):02d}.png'))
                print(f'PASS {name}', flush=True)
            except Exception as error:
                report['steps'].append({'name': name, 'passed': False, 'error': str(error)})
                save()
                raise
            save()

        def nav(name):
            page.locator('.studio-nav').get_by_role('button', name=re.compile('^' + name)).click()

        def read_recipe(photo):
            return page.evaluate('(photoId)=>window.__TAURI_INTERNALS__.invoke("image_recipe",{input:{photoId}})', photo)

        def ready():
            expect(page.get_by_role('button', name='Retouch', exact=True)).to_be_enabled(timeout=180000)

        def navigation():
            expected = ['Start', 'Photos', 'Auto edit', 'Instagram style', 'Export', 'Advanced']
            assert page.locator('.studio-nav strong').all_text_contents() == expected
            for name in expected:
                nav(name)
                expect(page.locator('main')).to_be_visible()
            return expected
        step('All six workspaces mount', navigation)

        collection = 'Desktop workflows ' + time.strftime('%H%M%S') + ' verification'
        nav('Start')
        page.get_by_label('New collection', exact=True).fill(collection)
        page.get_by_role('button', name='Create', exact=True).click()
        nav('Photos')
        page.get_by_text('Enter folders manually', exact=True).click()
        page.get_by_label('Card or folder', exact=True).fill(str(incoming.resolve()))
        page.get_by_role('button', name='Add folder', exact=True).click()
        page.get_by_role('button', name='Start import', exact=True).click()
        ready()
        state = page.evaluate('''async name => {
            const invoke=window.__TAURI_INTERNALS__.invoke;
            const project=(await invoke('list_projects')).find(p=>p.name===name);
            const photos=await invoke('list_images',{input:{projectId:project.id,offset:0,limit:50,orderBy:null}});
            return {project,photos};
        }''', collection)
        assert len(state['photos']) == len(args.sources)
        report['project'] = state['project']
        report['photos'] = state['photos']
        step('Import multiple photos and prepare saved edits', lambda: [p['fileName'] for p in state['photos']])

        saved_preferences = {}
        if args.skin_cleanup:
            def prepare_skin():
                nav('Auto edit')
                for item in state['photos']:
                    page.get_by_role('option', name=item['fileName'], exact=True).click()
                    ready()
                    page.get_by_role('button', name='Retouch', exact=True).click()
                    page.get_by_role('radio', name='Skin cleanup \u00b7 refine pores', exact=True).check()
                    run = page.get_by_role('button', name='Auto retouch: Face + body skin', exact=True)
                    run.click()
                    expect(run).to_be_enabled(timeout=300000)
                    expect(page.get_by_alt_text('Retouched photograph')).to_be_visible()
                    body = json.loads(read_recipe(item['id'])['body'])
                    saved_preferences[item['id']] = body['studio_portrait_auto_v1']['options']
                    assert saved_preferences[item['id']]['settings']['maxSpots'] == 900
                    page.get_by_role('button', name='Back to Develop', exact=True).click()
                    print('Prepared skin cleanup: ' + item['fileName'], flush=True)
                return saved_preferences
            step('Save independent face and body cleanup preferences', prepare_skin)

        def auto_all():
            nav('Auto edit')
            run = page.get_by_role('button', name='Auto edit all photos', exact=True)
            expect(run).to_be_enabled(timeout=180000)
            run.click()
            expect(page.get_by_text(re.compile('photos have saved edits')).first).to_be_visible(timeout=300000)
            expect(run).to_be_enabled(timeout=300000)
            recipes = {photo['id']: read_recipe(photo['id']) for photo in state['photos']}
            assert all(json.loads(recipe['body']).get('global') for recipe in recipes.values())
            if args.skin_cleanup:
                for identifier, recipe in recipes.items():
                    body = json.loads(recipe['body'])
                    assert body['studio_portrait_auto_v1']['options'] == saved_preferences[identifier]
                    assert body.get('studio_retouch_v1'), 'Available skin was not retouched'
                report['per_photo_edit'] = {identifier: {
                    'global': json.loads(recipe['body'])['global'],
                    'options': json.loads(recipe['body'])['studio_portrait_auto_v1']['options'],
                    'scene': json.loads(recipe['body'])['studio_portrait_auto_v1'].get('scene'),
                    'operations': len(json.loads(recipe['body']).get('studio_retouch_v1', [])),
                    'faces': len(json.loads(recipe['body'])['studio_portrait_auto_v1']['faces']),
                } for identifier, recipe in recipes.items()}
            return {photo: recipe['recipeHash'] for photo, recipe in recipes.items()}
        step('One-click collection editing', auto_all)
        photo = state['photos'][0]
        other = state['photos'][1]
        before_other = read_recipe(other['id'])['recipeHash']
        option = page.get_by_role('option', name=photo['fileName'], exact=True)
        expect(option).to_be_enabled(timeout=180000)
        option.click()
        ready()

        if args.skin_cleanup:
            def reopened_preferences():
                page.get_by_role('button', name='Retouch', exact=True).click()
                expect(page.get_by_label('Most spots per face', exact=True)).to_have_value('900')
                page.get_by_role('button', name='Back to Develop', exact=True).click()
                ready()
                return 'Reopened cleanup controls retain the saved 900-spot budget'
            step('Reopen per-photo cleanup controls', reopened_preferences)

        def exposure():
            field = page.get_by_role('spinbutton', name='Exposure value', exact=True)
            field.fill('0.3')
            field.press('Enter')
            page.wait_for_function('''async id => {
                const r=await window.__TAURI_INTERNALS__.invoke('image_recipe',{input:{photoId:id}});
                return Math.abs(JSON.parse(r.body).global.exposure - .3)<.001;
            }''', arg=photo['id'], timeout=60000)
            ready()
            assert read_recipe(other['id'])['recipeHash'] == before_other
            return 'Exposure saved only to the selected photo'
        step('Manual exposure and per-photo independence', exposure)

        if args.skin_cleanup:
            def protected_batch():
                auto_all()
                assert abs(json.loads(read_recipe(photo['id'])['body'])['global']['exposure'] - .3) < .001
                return 'Repeated automatic editing preserves manual exposure and each photo\'s cleanup choices'
            step('Repeat collection editing preserves manual work and cleanup settings', protected_batch)

        def history():
            initial = read_recipe(photo['id'])['recipeHash']
            page.get_by_role('button', name='Undo', exact=True).first.click()
            ready()
            assert read_recipe(photo['id'])['recipeHash'] != initial
            page.get_by_role('button', name='Redo', exact=True).first.click()
            ready()
            assert read_recipe(photo['id'])['recipeHash'] == initial
            return initial
        step('Develop undo and redo restore the recipe', history)

        def comparison():
            initial = read_recipe(photo['id'])['recipeHash']
            page.get_by_role('button', name='Compare', exact=True).click()
            divider = page.get_by_label('Before and after divider', exact=True)
            divider.fill('30')
            expect(divider).to_have_value('30')
            page.get_by_role('button', name='Edited', exact=True).click()
            assert read_recipe(photo['id'])['recipeHash'] == initial
            return 'Comparison leaves saved edits unchanged'
        step('Before/after comparison', comparison)

        def export():
            nav('Export')
            page.get_by_test_id('preset').select_option('gallery')
            page.get_by_test_id('destination').fill(str((args.output / 'export').resolve()))
            page.get_by_test_id('verify').check()
            # Export must work directly once its preset is loaded, without first
            # opening the filename preview (the earlier readiness regression).
            expect(page.get_by_test_id('run')).to_be_enabled()
            page.get_by_test_id('run').click()
            manifest = args.output / 'export/aura-delivery-manifest.json'
            deadline = time.monotonic() + 600
            while not manifest.exists():
                assert time.monotonic() < deadline, 'Export did not finish'
                time.sleep(.5)
            expect(page.get_by_test_id('run')).to_be_enabled()
            result = json.loads(manifest.read_text())
            assert result['verified'] and result['file_count'] == len(args.sources)
            readback = []
            for record in result['files']:
                exported = args.output / 'export' / record['path']
                with Image.open(exported) as image:
                    image.load()
                    readback.append({'path': str(exported), 'dimensions': list(image.size),
                                     'sha256': hashlib.sha256(exported.read_bytes()).hexdigest()})
            expected_sizes = []
            for source_path in hashes:
                with Image.open(source_path) as image:
                    expected_sizes.append(image.size)
            assert Counter(tuple(item['dimensions']) for item in readback) == Counter(expected_sizes)
            report['export_readback'] = readback
            return result
        step('Verified export of every photo', export)

        def advanced():
            nav('Advanced')
            names = ['Quality review', 'Gallery consistency', 'Albums & curation', 'AI provider', 'Performance & storage']
            for name in names:
                summary=page.locator('summary').filter(has_text=re.compile('^'+re.escape(name))).first
                summary.click()
                expect(summary.locator('..')).to_have_attribute('open','')
            return names
        step('Advanced panels open', advanced)
        report['final_recipes'] = {item['id']: read_recipe(item['id'])['recipeHash'] for item in state['photos']}
        for item in state['photos']:
            (args.output / (item['fileName'] + '.recipe.json')).write_text(
                json.dumps(read_recipe(item['id']), indent=2), encoding='utf-8')
        report['originals_unchanged'] = all(hashlib.sha256(Path(path).read_bytes()).hexdigest() == value for path,value in hashes.items())
        assert report['originals_unchanged'] and not report['page_errors']
        report['limits'] = ['External AI providers and Instagram network retrieval were not exercised.']
        save()


if __name__ == '__main__':
    main()
