"""Exercise diagnostic views in the native app and verify saved pixels stay unchanged."""
import argparse
import hashlib
import json
from pathlib import Path

from playwright.sync_api import expect, sync_playwright

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('output', type=Path)
parser.add_argument('--port', type=int, default=9359)
parser.add_argument('--collection', default='Professional retouch verification')
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
with sync_playwright() as p:
    page = p.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}').contexts[0].pages[0]
    page.set_default_timeout(180000)
    expect.set_options(timeout=180000)
    read = '''async name => {
      const i=window.__TAURI_INTERNALS__.invoke;
      const project=(await i('list_projects')).find(p=>p.name===name);
      const [photo]=await i('list_images',{input:{projectId:project.id,offset:0,limit:10,orderBy:null}});
      const recipe=await i('image_recipe',{input:{photoId:photo.id}});
      const frame=await i('native_retouch_preview',{projectId:project.id,photoId:photo.id,before:false});
      return {recipeHash:recipe.recipeHash,rgb:frame.rgbBase64};
    }'''
    before = page.evaluate(read, args.collection)
    page.get_by_role('button', name=f'{args.collection} 1', exact=True).click()
    page.get_by_role('button', name='Auto edit One click, start to finish', exact=True).click()
    retouch = page.get_by_role('button', name='Retouch', exact=True)
    if retouch.count():
        retouch.click()
    view = page.get_by_role('combobox', name='Skin analysis view', exact=True)
    expect(view).to_be_enabled()
    coverage = page.get_by_role('button', name='Show retouched areas', exact=True)
    if coverage.get_attribute('aria-pressed') == 'true':
        coverage.click()
    split = page.get_by_role('button', name='Split comparison', exact=True)
    expect(split).to_be_enabled()
    if split.get_attribute('aria-pressed') != 'true':
        split.click()
    main = page.get_by_alt_text('Retouched photograph', exact=True)
    old = page.get_by_alt_text('Before native retouch comparison', exact=True)
    filters = {'grayscale': 'grayscale(1)', 'high-contrast': 'grayscale(1) contrast(4)',
               'low-contrast': 'grayscale(1) contrast(0.25)', 'color': 'none'}
    measured = {}
    for name, wanted in filters.items():
        view.select_option(name)
        expect(main).to_be_visible()
        expect(old).to_be_visible()
        expect(main).to_have_css('filter', wanted)
        expect(old).to_have_css('filter', wanted)
        main.scroll_into_view_if_needed()
        page.screenshot(path=str(args.output/f'{name}.png'))
        measured[name] = {'before_filter': old.evaluate('(e)=>getComputedStyle(e).filter'),
                          'after_filter': main.evaluate('(e)=>getComputedStyle(e).filter')}
    view.select_option('high-contrast')
    coverage.click()
    expect(page.get_by_alt_text('Saved retouch coverage')).to_have_css('filter', 'none')
    coverage.click()
    view.select_option('color')
    if split.get_attribute('aria-pressed') == 'true':
        split.click()
    after = page.evaluate(read, args.collection)
    assert after == before, 'Diagnostic views changed saved recipe or native output'
    result = {'passed': True, 'views': measured, 'coverage_unfiltered': True,
              'recipe_unchanged': True, 'native_pixels_unchanged': True,
              'recipe_hash': before['recipeHash'],
              'native_rgb_base64_sha256': hashlib.sha256(before['rgb'].encode()).hexdigest()}
    (args.output/'verification.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps(result), flush=True)
