"""Run and export the eighteen-stage native workflow in an isolated collection."""
import argparse
import hashlib
import json
import time
from pathlib import Path

from PIL import Image
from playwright.sync_api import expect, sync_playwright

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('source', type=Path)
parser.add_argument('output', type=Path)
parser.add_argument('--collection', default='Second portrait verification')
parser.add_argument('--port', type=int, default=9359)
args = parser.parse_args()
assert args.collection.endswith('verification')
args.output.mkdir(parents=True, exist_ok=True)
source_hash = hashlib.sha256(args.source.read_bytes()).hexdigest()
with sync_playwright() as p:
    page = p.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}').contexts[0].pages[0]
    expect.set_options(timeout=300000)
    errors = []
    page.on('pageerror', lambda error: errors.append(str(error)))
    close = page.get_by_role('button', name='Back to Develop', exact=True)
    if close.count():
        close.click()
    page.get_by_role('button', name=args.collection + ' 1', exact=True).click()
    page.locator('.studio-nav').get_by_role('button', name='Auto edit', exact=False).click()
    retouch = page.get_by_role('button', name='Retouch', exact=True)
    expect(retouch).to_be_enabled()
    retouch.click()
    panel = page.get_by_role('region', name='Auto advanced retouch', exact=True)
    run = panel.get_by_role('button', name='Auto advanced retouch', exact=False).first
    expect(run).to_be_enabled()
    run.click()
    expect(panel.get_by_role('button', name='Run Auto advanced retouch again', exact=True)).to_be_enabled()
    saved = page.evaluate('''async name=>{
      const i=window.__TAURI_INTERNALS__.invoke;
      const project=(await i('list_projects')).find(p=>p.name===name);
      const photos=await i('list_images',{input:{projectId:project.id,offset:0,limit:10,orderBy:null}});
      if(photos.length!==1) throw new Error('Expected one verification photo');
      return i('image_recipe',{input:{photoId:photos[0].id}});
    }''', args.collection)
    report = json.loads(saved['body'])['studio_advanced_retouch_v1']
    assert [stage['number'] for stage in report['stages']] == list(range(1, 19))
    assert len({stage['stage'] for stage in report['stages']}) == 18
    assert all(stage['outcome'] in ['applied', 'unchanged', 'not_applicable', 'protected']
               for stage in report['stages'])
    page.screenshot(path=str(args.output / 'advanced-report.png'))
    page.locator('.studio-nav').get_by_role('button', name='Export', exact=False).click()
    destination = args.output / 'export'
    page.get_by_test_id('destination').fill(str(destination.resolve()))
    page.get_by_test_id('verify').check()
    export = page.get_by_test_id('run')
    expect(export).to_be_enabled()
    manifest_path = destination / 'aura-delivery-manifest.json'
    assert not manifest_path.exists(), 'Use a fresh export destination'
    export.click()
    deadline = time.monotonic() + 600
    while not manifest_path.exists():
        assert time.monotonic() < deadline
        time.sleep(.5)
    expect(export).to_be_enabled()
    manifest = json.loads(manifest_path.read_text(encoding='utf-8'))
    assert manifest['verified'] and manifest['file_count'] == 1
    with Image.open(destination / manifest['files'][0]['path']) as image:
        image.load()
        size = list(image.size)
    assert hashlib.sha256(args.source.read_bytes()).hexdigest() == source_hash
    assert not errors
    result = {'stages': report, 'manifest': manifest, 'export_dimensions': size,
              'source_sha256': source_hash, 'original_unchanged': True, 'page_errors': errors,
              'quality_equivalence_verified': False}
    (args.output / 'recipe.json').write_text(json.dumps(saved, indent=2), encoding='utf-8')
    (args.output / 'results.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps({'stages': len(report['stages']), 'export_verified': True,
                      'dimensions': size, 'measured_quality': report['quality']}), flush=True)
