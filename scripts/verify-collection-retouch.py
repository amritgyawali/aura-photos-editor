"""Exercise collection cleanup and cancellation through the running native UI.

Uses an existing isolated verification collection. AURA alone edits the images.
"""
import argparse
import hashlib
import json
from pathlib import Path

from playwright.sync_api import expect, sync_playwright

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('output', type=Path)
parser.add_argument('--collection', required=True)
parser.add_argument('--port', type=int, default=9359)
parser.add_argument('--sources', type=Path, nargs=3, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
report = {'page_errors': []}
with sync_playwright() as p:
    page = p.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}').contexts[0].pages[0]
    expect.set_options(timeout=300000)
    page.on('pageerror', lambda error: report['page_errors'].append(str(error)))
    page.get_by_role('button', name=args.collection + ' 3', exact=True).click()
    page.locator('.studio-nav').get_by_role('button', name='Auto edit', exact=False).click()
    retouch = page.get_by_role('button', name='Retouch', exact=True)
    expect(retouch).to_be_enabled()
    retouch.click()
    run = page.get_by_role('button', name='Apply cleanup settings to this collection', exact=True)
    expect(run).to_be_enabled()
    page.get_by_role('radio', name='Skin cleanup · refine pores', exact=True).check()
    read = '''async name => {
      const i=window.__TAURI_INTERNALS__.invoke;
      const project=(await i('list_projects')).find(p=>p.name===name);
      const photos=await i('list_images',{input:{projectId:project.id,offset:0,limit:240,orderBy:'timeline'}});
      const rows=[];
      for(const photo of photos) rows.push({photo,recipe:await i('image_recipe',{input:{photoId:photo.id}})});
      return rows;
    }'''
    before = page.evaluate(read, args.collection)
    assert len(before) == 3
    hashes = {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in args.sources}
    run.click()
    stop = page.get_by_role('button', name='Stop after current photo', exact=True)
    expect(stop).to_be_visible()
    stop.click()
    results = page.get_by_role('region', name='Collection cleanup results')
    expect(results.get_by_role('status')).to_contain_text('2 remaining.')
    expect(run).to_be_enabled()
    stopped = page.evaluate(read, args.collection)
    assert stopped[1]['recipe']['recipeHash'] == before[1]['recipe']['recipeHash']
    assert stopped[2]['recipe']['recipeHash'] == before[2]['recipe']['recipeHash']
    report['cancel_after_current'] = True
    report['cancel_status'] = results.get_by_role('status').inner_text()
    run.click()
    expect(results.get_by_role('status')).to_contain_text('0 remaining.')
    expect(run).to_be_enabled()
    assert results.locator('li').count() == 3
    assert '0 failed.' in results.get_by_role('status').inner_text()
    after = page.evaluate(read, args.collection)
    report['status'] = results.get_by_role('status').inner_text()
    report['photos'] = []
    for original, final in zip(before, after):
        old = json.loads(original['recipe']['body'])
        body = json.loads(final['recipe']['body'])
        auto = body['studio_portrait_auto_v1']
        assert auto['options']['settings']['maxSpots'] == 900
        assert auto['operations'] > 0
        # Retouch-only synchronization must preserve each photograph's own grade.
        for key in ['global', 'bw', 'geometry', 'lens', 'masks', 'restoration']:
            assert old.get(key) == body.get(key), (final['photo']['fileName'], key)
        manual = [edit for edit in old.get('studio_retouch_v1', [])
                  if not edit['id'].startswith('auto-portrait-v1-')]
        for edit in manual:
            assert edit in body.get('studio_retouch_v1', []), 'Manual repair changed'
        report['photos'].append({'name': final['photo']['fileName'],
            'recipe_hash': final['recipe']['recipeHash'], 'operations': auto['operations'],
            'planner_version': auto.get('plannerVersion'), 'grade_preserved': True})
    assert all(hashlib.sha256(Path(path).read_bytes()).hexdigest() == digest
               for path, digest in hashes.items())
    assert not report['page_errors']
    report['source_hashes_checked'] = hashes
    page.screenshot(path=str(args.output / 'collection-results.png'))
    (args.output / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    print(json.dumps(report), flush=True)
