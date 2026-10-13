"""Exercise the native residual-review controls and reversible focused repair."""
import argparse
import json
from pathlib import Path
from playwright.sync_api import expect, sync_playwright

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('output', type=Path)
parser.add_argument('--collection', required=True)
parser.add_argument('--port', type=int, default=9363)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
report = {'page_errors': []}
with sync_playwright() as p:
    page = p.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}').contexts[0].pages[0]
    page.set_default_timeout(300000)
    expect.set_options(timeout=300000)
    page.on('pageerror', lambda error: report['page_errors'].append(str(error)))
    def call(command, value=None):
        return page.evaluate('async ([c,v])=>{try{return await window.__TAURI_INTERNALS__.invoke(c,v)}catch(e){throw new Error(JSON.stringify(e))}}', [command, value or {}])
    project = next(row for row in call('list_projects') if row['name'] == args.collection)
    [photo] = call('list_images', {'input': {'projectId': project['id'], 'offset': 0, 'limit': 10, 'orderBy': None}})
    def recipe():
        return call('image_recipe', {'input': {'photoId': photo['id']}})
    baseline = recipe()['recipeHash']
    page.get_by_role('button', name=f'{args.collection} 1', exact=True).click()
    page.locator('.studio-nav').get_by_role('button', name='Auto edit One click, start to finish', exact=True).click()
    page.get_by_role('button', name='Retouch', exact=True).click()
    body = json.loads(recipe()['body'])
    index = next(i for i, edit in enumerate(body['studio_retouch_v1']) if edit['tool'] in ('frequency_heal', 'acne_clear'))
    operation = page.get_by_role('button', name=__import__('re').compile(r'^' + str(index + 1) + r'\. ')).first
    expect(operation).to_be_enabled()
    operation.click()
    expect(page.get_by_label('Blemish check resolution')).to_have_value('full')
    page.get_by_role('button', name='Check remaining marks', exact=True).click()
    prepare = page.get_by_role('button', name='Prepare blemish brush here', exact=True).first
    expect(prepare).to_be_enabled()
    assert recipe()['recipeHash'] == baseline, 'Checking remaining marks saved an edit'
    prepare.click()
    expect(page.get_by_role('button', name='Discard draft', exact=True)).to_be_visible()
    assert recipe()['recipeHash'] == baseline, 'Preparing a focused brush saved an edit'
    page.get_by_role('button', name='Check remaining marks', exact=True).click()
    expect(page.get_by_text('Apply or discard your draft before preparing another brush.', exact=True)).to_be_visible()
    for button in page.get_by_role('button', name='Prepare blemish brush here', exact=True).all():
        expect(button).to_be_disabled()
    assert recipe()['recipeHash'] == baseline
    page.get_by_role('button', name='Apply retouch', exact=True).click()
    expect(page.get_by_role('button', name='Check remaining marks', exact=True)).to_be_enabled()
    applied = recipe()
    edits = json.loads(applied['body'])['studio_retouch_v1']
    assert applied['recipeHash'] != baseline and len(edits) == len(body['studio_retouch_v1']) + 1
    assert edits[-1]['tool'] == 'acne_clear' and edits[-1]['repairPasses'] == 4
    page.get_by_role('button', name='Undo', exact=True).click()
    expect(page.get_by_role('button', name='Check remaining marks', exact=True)).to_be_enabled()
    assert recipe()['recipeHash'] == baseline, 'Undo did not restore the review baseline'
    assert not report['page_errors']
    report.update({'review_read_only': True, 'prepare_read_only': True, 'draft_protected': True,
                   'focused_four_pass_repair_saved': True, 'undo_restored_recipe': True, 'baseline_hash': baseline})
    (args.output / 'verification.json').write_text(json.dumps(report, indent=2), encoding='utf8')
    page.screenshot(path=str(args.output / 'review-ui.png'))
    print(json.dumps(report), flush=True)
