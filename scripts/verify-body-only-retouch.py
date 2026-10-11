"""Verify native healing coverage on a body-only photo without changing its recipe."""
import argparse
import base64
import json
from pathlib import Path

import numpy as np
from PIL import Image
from playwright.sync_api import sync_playwright

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('output', type=Path)
parser.add_argument('--port', type=int, default=9358)
parser.add_argument('--collection', default='Body-only verification')
parser.add_argument('--photo-name', help='Select the body-only photo in a batch verification collection')
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
with sync_playwright() as p:
    page = p.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}').contexts[0].pages[0]
    data = page.evaluate('''async ({name,photoName}) => {
      const i=window.__TAURI_INTERNALS__.invoke;
      const project=(await i('list_projects')).find(p=>p.name===name);
      const photos=await i('list_images',{input:{projectId:project.id,offset:0,limit:50,orderBy:null}});
      const matches=photos.filter(p=>!photoName||p.fileName===photoName);
      if(matches.length!==1) throw new Error('Select exactly one body-only test photo');
      const [photo]=matches;
      const input={projectId:project.id,photoId:photo.id};
      const recipe=await i('image_recipe',{input:{photoId:photo.id}});
      const before=await i('native_retouch_preview',{...input,before:true});
      const after=await i('native_retouch_preview',{...input,before:false});
      const coverage=await i('native_retouch_saved_selection',{input:{...input,operationId:null}});
      return {photoId:photo.id,recipe,before,after,coverage};
    }''', {'name':args.collection,'photoName':args.photo_name})

    def pixels(frame):
        return np.frombuffer(base64.b64decode(frame['rgbBase64']), dtype=np.uint8).reshape(frame['height'], frame['width'], 3)

    before, after, mask = [pixels(data[key]) for key in ('before', 'after', 'coverage')]
    assert before.shape == after.shape == mask.shape
    for name, frame in [('before', before), ('after', after), ('coverage', mask)]:
        Image.fromarray(frame).save(args.output/f'{name}.png')
    selected = mask[:, :, 0] > 127
    unchanged = mask[:, :, 0] == 0
    difference = np.max(np.abs(after.astype(np.int16)-before.astype(np.int16)), axis=2)
    assert selected.any(), 'No visible body skin was selected'
    assert (difference[selected] > 0).any(), 'Selected body skin received no healing'
    assert (difference[unchanged] == 0).all(), 'Healing changed unselected pixels'
    body = json.loads(data['recipe']['body'])
    assert not body['studio_portrait_auto_v1']['faces'], 'Expected a photo without a detected face'
    result = {'no_face_required': True, 'selected_pixels': int(selected.sum()),
              'changed_selected_pixels': int((difference[selected] > 0).sum()),
              'unselected_pixels_unchanged': True, 'recipe_hash': data['recipe']['recipeHash']}
    current = page.evaluate('(photoId)=>window.__TAURI_INTERNALS__.invoke("image_recipe",{input:{photoId}})', data['photoId'])
    assert current['recipeHash'] == result['recipe_hash'], 'Inspection changed the recipe'
    (args.output/'verification.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps(result), flush=True)
