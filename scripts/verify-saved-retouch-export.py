"""Export an existing native test collection without resetting its reviewed edits."""
import argparse
import hashlib
import json
import time
from pathlib import Path

from playwright.sync_api import expect, sync_playwright

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('source', type=Path)
parser.add_argument('output', type=Path)
parser.add_argument('--collection', default='Professional retouch verification')
parser.add_argument('--port', type=int, default=9353)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
source_hash = hashlib.sha256(args.source.read_bytes()).hexdigest()
with sync_playwright() as p:
    page = p.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}').contexts[0].pages[0]
    page.set_default_timeout(180000)
    expect.set_options(timeout=180000)
    page.get_by_role('button', name=f'{args.collection} 1', exact=True).click()
    read = '''async name => {
        const i = window.__TAURI_INTERNALS__.invoke;
        const project = (await i('list_projects')).find(p => p.name === name);
        const [photo] = await i('list_images', {input: {projectId: project.id, offset: 0, limit: 10, orderBy: null}});
        return i('image_recipe', {input: {photoId: photo.id}});
    }'''
    before = page.evaluate(read, args.collection)
    page.get_by_role('button', name='Export Ready to share', exact=True).click()
    page.get_by_test_id('destination').fill(str((args.output / 'export').resolve()))
    page.get_by_test_id('verify').check()
    run = page.get_by_test_id('run')
    expect(run).to_be_enabled()
    manifest_path = args.output / 'export/aura-delivery-manifest.json'
    assert not manifest_path.exists(), 'Use a fresh output folder to verify this export'
    run.click()
    deadline = time.monotonic() + 600
    while not manifest_path.exists():
        assert time.monotonic() < deadline, 'Export did not finish'
        time.sleep(.5)
    expect(run).to_be_enabled()
    manifest = json.loads(manifest_path.read_text())
    after = page.evaluate(read, args.collection)
    assert before['recipeHash'] == after['recipeHash']
    assert manifest['verified'] and manifest['file_count'] == 1
    assert hashlib.sha256(args.source.read_bytes()).hexdigest() == source_hash
    result = {'verified': True, 'original_unchanged': True, 'original_sha256': source_hash,
              'recipe_unchanged': True, 'recipe_hash': after['recipeHash'], 'manifest': manifest}
    (args.output / 'recipe.json').write_text(json.dumps(after, indent=2), encoding='utf-8')
    (args.output / 'verification.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    page.screenshot(path=str(args.output / 'export.png'))
    print(json.dumps(result), flush=True)
