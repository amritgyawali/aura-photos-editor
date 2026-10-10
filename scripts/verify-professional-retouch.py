"""Drive the native AURA window through the Professional retouch preset.

AURA alone imports, retouches and exports the photograph; this script only presses the
controls a person would press and then reads back what AURA saved. It edits no pixels.

Launch a debug custom-protocol build first, with

    AURA_TEST_CATALOG                       an absolute path to an isolated catalog
    WEBVIEW2_USER_DATA_FOLDER               an isolated directory
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS   --remote-debugging-port=9337

then run

    python scripts/verify-professional-retouch.py PORTRAIT OUTPUT_FOLDER

`--retouch-only` presses **Reset photo** before retouching, so the export shows the retouch
on the photograph as it was taken, without the automatic exposure and colour edit an import
applies. `--resume` reuses the collection an earlier run of this script created.

Requires Python Playwright. Evidence is written to the output folder, not committed:
the saved recipe, screenshots, the exported JPEG and `verification.json`.
"""
import argparse
import hashlib
import json
import shutil
import time
from pathlib import Path

from playwright.sync_api import expect, sync_playwright

COLLECTION = 'Professional retouch verification'

READ_RECIPE = """async (name) => {
    const invoke = window.__TAURI_INTERNALS__.invoke;
    const projects = await invoke('list_projects');
    const project = projects.find(p => p.name === name);
    if (!project) throw new Error('Test collection is missing');
    const images = await invoke('list_images', {input: {
        projectId: project.id, offset: 0, limit: 10, orderBy: null
    }});
    if (images.length !== 1) throw new Error('Expected one test portrait');
    return invoke('image_recipe', {input: {photoId: images[0].id}});
}"""


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('source', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--port', type=int, default=9337)
    parser.add_argument('--collection', default=COLLECTION)
    parser.add_argument('--resume', action='store_true', help='the collection already holds the portrait')
    parser.add_argument('--retouch-only', action='store_true', help='reset the automatic global edit first')
    parser.add_argument('--acne-only', action='store_true', help='preserve detail and run only blemish repair')
    parser.add_argument('--skin-cleanup', action='store_true', help='blemish repair plus adjustable pore refinement')
    parser.add_argument('--body-only-photo', action='store_true', help='verify visible body skin on a photo with no detectable face')
    args = parser.parse_args()
    collection = args.collection
    args.output.mkdir(parents=True, exist_ok=True)
    original_hash = hashlib.sha256(args.source.read_bytes()).hexdigest()
    incoming = args.output / 'incoming'
    incoming.mkdir(exist_ok=True)
    imported_source = incoming / args.source.name
    shutil.copyfile(args.source, imported_source)
    with sync_playwright() as p:
        browser = p.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}')
        page = browser.contexts[0].pages[0]
        page.set_default_timeout(300000)
        expect.set_options(timeout=300000)
        expect(page.locator('.aura-studio')).to_be_visible()
        page.screenshot(path=str(args.output / 'start.png'))
        if not args.resume:
            page.get_by_label('New collection', exact=True).fill(collection)
            page.get_by_role('button', name='Create', exact=True).click()
            expect(page.get_by_role('button', name=f'{collection} 0', exact=True)).to_be_visible()
            page.get_by_role('button', name='Photos Browse your collection', exact=True).click()
            page.get_by_text('Enter folders manually', exact=True).click()
            page.get_by_label('Card or folder', exact=True).fill(str(incoming.resolve()))
            page.get_by_role('button', name='Add folder', exact=True).click()
            page.get_by_role('button', name='Start import', exact=True).click()
        else:
            page.get_by_role('button', name=f'{collection} 1', exact=True).click()
            page.get_by_role('button', name='Auto edit One click, start to finish', exact=True).click()
        retouch = page.get_by_role('button', name='Retouch', exact=True)
        back = page.get_by_role('button', name='Back to Develop', exact=True)
        if not retouch.count() and back.count():
            back.click()
        expect(retouch).to_be_enabled()
        if args.retouch_only:
            # The photograph as taken: no automatic exposure, colour or earlier retouch.
            page.get_by_role('button', name='Reset photo', exact=True).click()
            deadline = time.monotonic() + 120
            while True:
                body = json.loads(page.evaluate(READ_RECIPE, collection)['body'])
                if not body['global']['exposure'] and not body.get('studio_retouch_v1'):
                    reset_global = body['global']
                    break
                assert time.monotonic() < deadline, 'Reset photo did not clear the automatic edit'
                time.sleep(0.5)
            expect(retouch).to_be_enabled()
            # Exposure can already be zero in the imported edit. Record the
            # baseline only after the reset's write and preview have completed.
            reset_global = json.loads(page.evaluate(READ_RECIPE, collection)['body'])['global']
            (args.output / 'reset-global.json').write_text(json.dumps(reset_global, indent=2), encoding='utf-8')
        page.screenshot(path=str(args.output / 'develop.png'))
        retouch.click()
        preset = 'Skin cleanup \u00b7 refine pores' if args.skin_cleanup else ('Acne only \u00b7 preserve detail' if args.acne_only else 'Professional retouch')
        page.get_by_role('radio', name=preset, exact=True).check()
        expect(page.get_by_text('Frequency healing rebuilds the tone under each mark first')).to_be_visible()
        run = page.get_by_role('button', name='Auto retouch: Face + body skin' if args.acne_only or args.skin_cleanup else 'Auto retouch: Face', exact=True)
        started = time.monotonic()
        run.click()
        print(f'Running {preset} inside AURA', flush=True)
        expect(run).to_be_enabled()
        expect(page.get_by_alt_text('Retouched photograph')).to_be_visible()
        retouch_seconds = time.monotonic() - started
        page.screenshot(path=str(args.output / 'retouch.png'))
        (args.output / 'retouch.txt').write_text(page.locator('main').inner_text(), encoding='utf-8')

        # Read-only evidence from the application. Every edit above used its controls.
        recipe = page.evaluate(READ_RECIPE, collection)
        (args.output / 'recipe.json').write_text(json.dumps(recipe, indent=2), encoding='utf-8')
        body = json.loads(recipe['body'])
        edits = body['studio_retouch_v1']
        tools = [edit['tool'] for edit in edits]
        automatic = [edit for edit in edits if edit['id'].startswith('auto-portrait-v1-0-')]
        heal = [edit for edit in edits if edit['tool'] == 'acne_clear']
        graft = [edit for edit in edits if edit['tool'] == 'texture_graft']
        finish = [edit for edit in edits if edit['id'].endswith('-surface-finish')]
        if args.body_only_photo:
            assert args.acne_only or args.skin_cleanup, 'Body-only validation needs face-and-body scope'
            assert not body['studio_portrait_auto_v1']['faces']
            assert len(edits) == 1 and tools == ['frequency_heal'] and edits[0]['id'].endswith('-body-spots'), tools
            assert edits[0]['matte'] in body['studio_retouch_mattes_v1']
            assert 'without requiring a face' in body['studio_portrait_auto_v1']['message']
        elif args.skin_cleanup:
            assert len([e for e in heal if e['id'].endswith('-clear')]) == 1 and not graft and len(finish) == 1, tools
            assert any(e['id'].endswith('-body-spots') for e in heal)
            assert body['studio_portrait_auto_v1']['options']['scope'] == 'face_and_body'
            settings = body['studio_portrait_auto_v1']['options']['settings']
            assert settings['protectEyeArea'] and settings['protectNoseDetail']
            assert settings['poreRefine'] > 0 and settings['bodySmoothing'] > 0
            assert all(e.get('matte') for e in edits), 'Every cleanup operation needs a saved skin/detail mask'
        elif args.acne_only:
            assert len([e for e in heal if e['id'].endswith('-clear')]) == 1 and not graft and not finish, tools
            assert set(tools) <= {'frequency_heal', 'patch_heal', 'skin_uniformity'}, tools
            assert all('-spot-deep-' in edit['id'] and max(edit['region'][2:]) <= .1
                       for edit in edits if edit['tool'] == 'skin_uniformity'), 'Color correction must be confined to individual lesions'
            assert all(edit['matte'].endswith(('-feature-safe', '-feature-guard', '-outside-faces')) for edit in edits)
            assert body['studio_portrait_auto_v1']['options']['scope'] == 'face_and_body'
            assert body['studio_portrait_auto_v1']['options']['settings']['protectEyeArea']
            assert body['studio_portrait_auto_v1']['options']['settings']['protectNoseDetail']
        else:
            assert len(heal) == 1 and len(graft) == 1 and len(finish) == 1, tools
            # The order a retoucher works in: marks first, texture back last.
            assert automatic[0]['tool'] == 'frequency_heal', automatic[0]['id']
            order = [edit['id'] for edit in edits]
            assert order.index(heal[0]['id']) < order.index(finish[0]['id']) < order.index(graft[0]['id'])
            # Healing includes nose skin; broad finishing has a separate protection mask.
            # Texture restoration remains constrained by its own feature-safe skin mask.
            mattes = body['studio_retouch_mattes_v1']
            assert heal[0]['matte'] in mattes and finish[0]['matte'] in mattes
            assert heal[0]['matte'] != finish[0]['matte'], 'Blemish repair and broad finishing need separate nose protection'
            assert graft[0]['matte'].endswith(('-skin', '-skin-feature-safe')) and graft[0]['matte'] in mattes
            for wanted in ('micro_dodge_burn', 'portrait_dodge_burn', 'skin_smooth', 'skin_uniformity'):
                assert wanted in tools, f'{wanted} is missing from the pass'
        repairs = [edit for edit in edits if edit['tool'] == 'patch_heal']
        assert all(edit.get('textureHeal') for edit in repairs)
        if args.acne_only or args.skin_cleanup:
            assert all(abs(edit['amount'] - 1) < 1e-5 and abs(edit['feather'] - .25) < 1e-5 for edit in repairs)
        if args.retouch_only:
            assert not body['global']['exposure'], body['global']
            assert body['global'] == reset_global, 'Retouch changed the global photo adjustments'

        page.get_by_role('button', name='Export Ready to share', exact=True).click()
        page.get_by_test_id('destination').fill(str((args.output / 'export').resolve()))
        page.get_by_test_id('verify').check()
        page.get_by_test_id('preview-names').click()
        # An earlier export's summary can still be on screen, so wait for this run's own
        # manifest rather than for the summary.
        manifest_path = args.output / 'export/aura-delivery-manifest.json'
        manifest_path.unlink(missing_ok=True)
        started = time.monotonic()
        page.get_by_test_id('run').click()
        while not manifest_path.exists():
            assert time.monotonic() - started < 900, 'AURA wrote no delivery manifest'
            time.sleep(0.25)
        export_seconds = time.monotonic() - started
        expect(page.get_by_test_id('run')).to_be_enabled()
        expect(page.get_by_test_id('manifest-summary')).to_be_visible()
        manifest = json.loads(manifest_path.read_text())
        assert manifest['verified'] and manifest['file_count'] == 1, manifest
        assert hashlib.sha256(args.source.read_bytes()).hexdigest() == original_hash
        assert hashlib.sha256(imported_source.read_bytes()).hexdigest() == original_hash
        page.screenshot(path=str(args.output / 'export.png'))
        results = {
            'verified': True,
            'original_unchanged': True,
            'imported_copy_unchanged': True,
            'original_sha256': original_hash,
            'recipe_hash': recipe.get('recipeHash'),
            'retouch_only': args.retouch_only,
            'preset': preset,
            'global_exposure_ev': body['global']['exposure'],
            'operations': len(edits),
            'tools': {tool: tools.count(tool) for tool in sorted(set(tools))},
            'spot_repairs': len(repairs),
            'retouch_seconds': round(retouch_seconds, 2),
            'export_seconds': round(export_seconds, 2),
            'manifest': manifest,
        }
        (args.output / 'verification.json').write_text(json.dumps(results, indent=2), encoding='utf-8')
        print(json.dumps(results), flush=True)


if __name__ == '__main__':
    main()
