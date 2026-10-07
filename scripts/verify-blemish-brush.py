"""Drive the native AURA window through the blemish brush and the retouched-areas view.

Run after verify-professional-retouch.py has retouched the portrait in the collection
'Professional retouch verification'. This presses the controls a person would press - Blemish
brush, the keyboard target fields, Dab at target coordinates, Apply retouch, Show retouched
areas - and reads back what AURA saved. It edits no pixels.

    python scripts/verify-blemish-brush.py OUTPUT_FOLDER --at 0.47,0.44 --at 0.55,0.43

Each `--at` is a normalized x,y on the full photograph to paint a dab over. Requires Python
Playwright and a window started with a remote debugging port (see the other script).
"""
import argparse
import base64
import json
from pathlib import Path

from playwright.sync_api import expect, sync_playwright

COLLECTION = 'Professional retouch verification'

READ = """async (name) => {
    const invoke = window.__TAURI_INTERNALS__.invoke;
    const project = (await invoke('list_projects')).find(p => p.name === name);
    const photo = (await invoke('list_images', {input: {
        projectId: project.id, offset: 0, limit: 10, orderBy: null}}))[0];
    const input = {projectId: project.id, photoId: photo.id};
    const recipe = await invoke('image_recipe', {input: {photoId: photo.id}});
    const before = await invoke('native_retouch_preview', {...input, before: true});
    const after = await invoke('native_retouch_preview', {...input, before: false});
    return {recipe, before, after};
}"""

PNG = """frame => {
    const raw = atob(frame.rgbBase64), canvas = document.createElement('canvas');
    canvas.width = frame.width; canvas.height = frame.height;
    const context = canvas.getContext('2d'), pixels = context.createImageData(frame.width, frame.height);
    for (let i = 0, j = 0; i < raw.length; i += 3, j += 4) {
        pixels.data[j] = raw.charCodeAt(i); pixels.data[j + 1] = raw.charCodeAt(i + 1);
        pixels.data[j + 2] = raw.charCodeAt(i + 2); pixels.data[j + 3] = 255;
    }
    context.putImageData(pixels, 0, 0);
    return canvas.toDataURL('image/png').split(',')[1];
}"""


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('output', type=Path)
    parser.add_argument('--port', type=int, default=9337)
    parser.add_argument('--at', action='append', default=[], help='normalized x,y to paint')
    parser.add_argument('--radius', type=float, default=1.5, help='brush radius, percent of the short edge')
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    with sync_playwright() as p:
        page = p.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}').contexts[0].pages[0]
        page.set_default_timeout(300000)
        expect.set_options(timeout=300000)
        first = page.evaluate(READ, COLLECTION)
        edits_before = json.loads(first['recipe']['body']).get('studio_retouch_v1', [])
        if not page.get_by_role('button', name='Blemish brush', exact=True).count():
            page.get_by_role('button', name=f'{COLLECTION} 1', exact=True).click()
            page.get_by_role('button', name='Auto edit One click, start to finish', exact=True).click()
            page.get_by_role('button', name='Retouch', exact=True).click()
        brush = page.get_by_role('button', name='Blemish brush', exact=True)
        expect(brush).to_be_enabled()
        brush.click()
        page.get_by_label('Brush radius (% of short edge)').fill(str(args.radius))
        for spot in args.at:
            x, y = (float(v) for v in spot.split(','))
            page.get_by_label('Center X (%)').fill(f'{x * 100:.2f}')
            page.get_by_label('Center Y (%)').fill(f'{y * 100:.2f}')
            page.get_by_role('button', name='Dab at target coordinates', exact=True).click()
        page.screenshot(path=str(args.output / 'brush-draft.png'))
        apply = page.get_by_role('button', name='Apply retouch', exact=True)
        expect(apply).to_be_enabled()
        apply.click()
        expect(page.get_by_text(f'{len(edits_before) + 1} saved operation', exact=False)).to_be_visible()
        show = page.get_by_role('button', name='Show retouched areas', exact=True)
        expect(show).to_be_enabled()
        show.click()
        page.get_by_label('Show', exact=True).select_option('changes')
        expect(page.get_by_alt_text('Saved retouch coverage')).to_be_visible()
        page.get_by_alt_text('Saved retouch coverage').scroll_into_view_if_needed()
        page.screenshot(path=str(args.output / 'retouched-pixels.png'))
        page.get_by_label('Show', exact=True).select_option('selection')
        expect(page.get_by_alt_text('Saved retouch coverage')).to_be_visible()
        page.screenshot(path=str(args.output / 'retouched-selection.png'))
        show.click()
        final = page.evaluate(READ, COLLECTION)
        for name in ('before', 'after'):
            png = page.evaluate(PNG, final[name])
            (args.output / f'{name}-preview.png').write_bytes(base64.b64decode(png))
        edits = json.loads(final['recipe']['body'])['studio_retouch_v1']
        added = [e for e in edits if e['id'] not in {b['id'] for b in edits_before}]
        assert len(added) == 1 and added[0]['tool'] == 'acne_clear', added
        assert added[0]['mask'] and len(added[0]['mask']['strokes']) == len(args.at), added[0]
        assert added[0].get('matte') is None, added[0]
        result = {'operations': len(edits), 'brush': added[0], 'recipe_hash': final['recipe']['recipeHash']}
        (args.output / 'brush.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
        print(json.dumps({'operations': len(edits), 'strokes': len(args.at)}), flush=True)


if __name__ == '__main__':
    main()
