"""Read-only desktop inspection of protected feature pixels and saved coverage.
Run after verify-professional-retouch.py --acne-only --retouch-only.
No pixels, recipe, or history are edited by this inspector.
"""
import argparse, base64, json, math
from pathlib import Path
from playwright.sync_api import sync_playwright, expect

parser=argparse.ArgumentParser()
parser.add_argument('output',type=Path)
parser.add_argument('--port',type=int,default=9337)
args=parser.parse_args()
args.output.mkdir(parents=True,exist_ok=True)
with sync_playwright() as p:
 browser=p.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}')
 page=browser.contexts[0].pages[0]
 page.set_default_timeout(180000)
 data=page.evaluate('''async () => {
  const invoke=window.__TAURI_INTERNALS__.invoke;
  const project=(await invoke('list_projects')).find(p=>p.name==='Professional retouch verification');
  const photo=(await invoke('list_images',{input:{projectId:project.id,offset:0,limit:10,orderBy:null}}))[0];
  const input={projectId:project.id,photoId:photo.id};
  const recipe=await invoke('image_recipe',{input:{photoId:photo.id}});
  const before=await invoke('native_retouch_preview',{...input,before:true});
  const after=await invoke('native_retouch_preview',{...input,before:false});
  const coverage=await invoke('native_retouch_saved_selection',{input:{...input,operationId:null}});
  return {project:project.id,photo:photo.id,recipe,before,after,coverage};
 }''')
 # Encode the application's rendered bytes without altering them for visual inspection.
 for name in ('before','after'):
  png=page.evaluate('''frame => {
   const raw=atob(frame.rgbBase64), canvas=document.createElement('canvas');
   canvas.width=frame.width; canvas.height=frame.height;
   const context=canvas.getContext('2d'), pixels=context.createImageData(frame.width,frame.height);
   for(let i=0,j=0;i<raw.length;i+=3,j+=4) {
    pixels.data[j]=raw.charCodeAt(i); pixels.data[j+1]=raw.charCodeAt(i+1);
    pixels.data[j+2]=raw.charCodeAt(i+2); pixels.data[j+3]=255;
   }
   context.putImageData(pixels,0,0);
   return canvas.toDataURL('image/png').split(',')[1];
  }''',data[name])
  (args.output/f'{name}-native-preview.png').write_bytes(base64.b64decode(png))
 body=json.loads(data['recipe']['body'])
 w,h=data['before']['width'],data['before']['height']
 assert (w,h)==(data['after']['width'],data['after']['height'])==(data['coverage']['width'],data['coverage']['height'])
 before=base64.b64decode(data['before']['rgbBase64'])
 after=base64.b64decode(data['after']['rgbBase64'])
 mask=base64.b64decode(data['coverage']['rgbBase64'])
 face=body['studio_portrait_auto_v1']['faces'][0]
 a,b,nose,mouth_a,mouth_b= [[x*w,y*h] for x,y in face['landmarks']]
 d=math.dist(a,b); u=[(b[0]-a[0])/d,(b[1]-a[1])/d]; v=[-u[1],u[0]]
 if sum(((mouth_a[i]+mouth_b[i]-a[i]-b[i])*.5)*v[i] for i in range(2))<0:
  v=[-v[0],-v[1]]
 cores={'left_eye':[],'right_eye':[],'nose':[]}
 for y in range(h):
  for x in range(w):
   for name,point,rx,ry,shift in [('left_eye',a,.30,.19,.035),('right_eye',b,.30,.19,.035),('nose',nose,.19,.38,-.17)]:
    dx=x+.5-point[0];dy=y+.5-point[1]
    across=(dx*u[0]+dy*u[1])/d;down=(dx*v[0]+dy*v[1])/d-shift
    if (across/rx)**2+(down/ry)**2<=1: cores[name].append((y*w+x)*3)
 metrics={}
 for name,indices in cores.items():
  difference=[abs(before[i+c]-after[i+c]) for i in indices for c in range(3)]
  metrics[name]={'pixels':len(indices),'max_channel_change':max(difference),'mean_channel_change':sum(difference)/len(difference),'max_selection':max(mask[i] for i in indices)}
  assert metrics[name]['max_selection']==0,metrics
  assert metrics[name]['max_channel_change']==0,metrics
 assert any(a!=b for a,b in zip(before,after)), 'Retouch must actually improve skin elsewhere'
 page.get_by_role('button',name='Professional retouch verification 1',exact=True).click()
 page.get_by_role('button',name='Auto edit One click, start to finish',exact=True).click()
 retouch=page.get_by_role('button',name='Retouch',exact=True)
 if retouch.count(): retouch.click()
 show=page.get_by_role('button',name='Show retouched areas',exact=True)
 expect(show).to_be_enabled(timeout=180000)
 show.click()
 expect(page.get_by_alt_text('Saved retouch coverage')).to_be_visible(timeout=180000)
 page.get_by_alt_text('Saved retouch coverage').scroll_into_view_if_needed()
 page.screenshot(path=str(args.output/'saved-coverage.png'))
 page.get_by_role('button',name='1:1 preview',exact=True).click()
 page.get_by_alt_text('Saved retouch coverage').scroll_into_view_if_needed()
 page.screenshot(path=str(args.output/'saved-coverage-detail.png'))
 page.get_by_role('button',name='Fit',exact=True).click()
 photo=data['photo']
 after_recipe=page.evaluate('(photoId)=>window.__TAURI_INTERNALS__.invoke("image_recipe",{input:{photoId}})',photo)
 assert after_recipe['recipeHash']==data['recipe']['recipeHash'], 'Coverage viewing changed the recipe'
 result={'protected_feature_pixels':metrics,'recipe_unchanged_by_viewing':True,'recipe_hash':data['recipe']['recipeHash'],'preview_dimensions':[w,h],'global':body['global']}
 (args.output/'feature-preservation.json').write_text(json.dumps(result,indent=2),encoding='utf-8')
 print(json.dumps(result),flush=True)
