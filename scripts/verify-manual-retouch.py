"""Check manual brush save/removal and saved coverage against the native recipe."""
import argparse
import json
from pathlib import Path
from playwright.sync_api import expect, sync_playwright


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output',type=Path)
    parser.add_argument('--port',type=int,default=9348)
    args=parser.parse_args()
    args.output.mkdir(parents=True,exist_ok=True)
    with sync_playwright() as p:
        page=p.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}').contexts[0].pages[0]
        page.set_default_timeout(180000)
        expect.set_options(timeout=180000)
        page.get_by_role('button',name='Professional retouch verification 1',exact=True).click()
        page.get_by_role('button',name='Auto edit One click, start to finish',exact=True).click()
        retouch=page.get_by_role('button',name='Retouch',exact=True)
        if retouch.count(): retouch.click()
        expect(page.get_by_role('button',name='Show retouched areas',exact=True)).to_be_enabled()
        initial=page.evaluate('''async()=>{
            const i=window.__TAURI_INTERNALS__.invoke;
            const p=(await i('list_projects')).find(p=>p.name==='Professional retouch verification');
            const [photo]=await i('list_images',{input:{projectId:p.id,offset:0,limit:10,orderBy:null}});
            return i('image_recipe',{input:{photoId:photo.id}});
        }''')
        def recipe():
            return page.evaluate('(photoId)=>window.__TAURI_INTERNALS__.invoke("image_recipe",{input:{photoId}})',initial['photoId'])
        count=len(json.loads(initial['body'])['studio_retouch_v1'])
        page.get_by_label('Tool',exact=True).select_option('dodge')
        page.get_by_role('button',name='Brush (B)',exact=True).click()
        page.get_by_label('Center X (%)',exact=True).fill('20')
        page.get_by_label('Center Y (%)',exact=True).fill('85')
        page.get_by_role('button',name='Dab at target coordinates',exact=True).click()
        assert recipe()['recipeHash']==initial['recipeHash'], 'Draft wrote the recipe'
        page.get_by_role('button',name='Apply retouch',exact=True).click()
        show=page.get_by_role('button',name='Show retouched areas',exact=True)
        expect(show).to_be_enabled()
        saved=recipe()
        edits=json.loads(saved['body'])['studio_retouch_v1']
        assert len(edits)==count+1 and edits[-1]['tool']=='dodge'
        assert edits[-1]['mask']['strokes'][0]['points'][0][:2]==[.2,.85]
        if show.get_attribute('aria-pressed') != 'true': show.click()
        expect(page.get_by_alt_text('Saved retouch coverage')).to_be_visible()
        page.get_by_label('Show selection for',exact=True).select_option(edits[-1]['id'])
        expect(page.get_by_alt_text('Saved retouch coverage')).to_be_visible()
        opacity=page.get_by_label('Overlay visibility',exact=True)
        opacity.press('End')
        expect(opacity).to_have_value('0.85')
        page.get_by_role('button',name='1:1 preview',exact=True).click()
        page.get_by_role('button',name='Fit',exact=True).click()
        assert recipe()['recipeHash']==saved['recipeHash'], 'Viewing wrote the recipe'
        page.screenshot(path=str(args.output/'manual-coverage.png'))
        page.get_by_role('button',name='Split comparison',exact=True).click()
        expect(page.get_by_alt_text('Before native retouch comparison',exact=True)).to_be_visible()
        page.get_by_role('button',name='Split comparison',exact=True).click()
        page.get_by_role('button',name=f'Remove retouch {count+1}',exact=True).click()
        expect(show).to_be_enabled()
        assert recipe()['recipeHash']==initial['recipeHash'], 'Removing manual repair did not restore the recipe'
        result={'draft_unsaved':True,'brush_saved':True,'operation_coverage':True,'zoom_and_opacity_read_only':True,
                'split_comparison':True,'remove_restored_recipe':True,'recipeHash':initial['recipeHash']}
        (args.output/'manual.json').write_text(json.dumps(result,indent=2),encoding='utf-8')
        print(json.dumps(result),flush=True)


if __name__=='__main__': main()
