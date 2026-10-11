"""Native desktop inspection of protected feature pixels and saved coverage.
Run after verify-professional-retouch.py --acne-only --retouch-only.
Default inspection is read-only. --allow-structure-spots temporarily renders a
test stack without compact patches and undoes its writes; use an isolated catalog.
"""
import argparse, base64, json, math
from pathlib import Path
from playwright.sync_api import sync_playwright, expect
from retouch_evidence import only_local_feather_changes, only_local_spot_changes, coarse_region_change
import numpy as np

parser=argparse.ArgumentParser()
parser.add_argument('output',type=Path)
parser.add_argument('--port',type=int,default=9337)
parser.add_argument('--collection',default='Professional retouch verification')
parser.add_argument('--expect-nose-repair',action='store_true')
parser.add_argument('--include-nose',action='store_true',help='expect nose skin repair with nostril protection')
parser.add_argument('--allow-local-wing-repairs',action='store_true',help='allow measured local spot repair on wing skin; broad finishing must still exclude it')
parser.add_argument('--allow-structure-feather',action='store_true',help='allow only a neighboring compact repair feather to touch the structure rectangle; broad selection and repair cores remain excluded')
parser.add_argument('--allow-structure-spots',action='store_true',help='allow saved compact repairs in the rectangle only when broad rendered pixels are unchanged and coarse contour/color changes stay below 1 percent')
parser.add_argument('--min-nose-coverage',type=float,default=.6,
                    help='minimum selected fraction of the landmark nose sample; use 0 to measure coverage without certifying it')
parser.add_argument('--require-body',action='store_true',help='verify body selection and no body coverage inside detected faces')
parser.add_argument('--skip-ui',action='store_true',help='measure native pixels without repeating the overlay UI check')
parser.add_argument('--opening-core',type=float,nargs=4,action='append',help='independently inspected anatomical opening rectangle; supply both openings instead of landmark guesses')
parser.add_argument('--protected-rect',type=float,nargs=4,metavar=('LEFT','TOP','RIGHT','BOTTOM'),
                    help='additional normalized structure region that must remain pixel-identical')
args=parser.parse_args()
assert not args.allow_structure_feather or args.allow_local_wing_repairs
assert not args.allow_structure_spots or args.collection.endswith('verification')
args.output.mkdir(parents=True,exist_ok=True)
with sync_playwright() as p:
 browser=p.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}')
 page=browser.contexts[0].pages[0]
 page.set_default_timeout(180000)
 data=page.evaluate('''async ({collection,localWings,structureSpots}) => {
  const invoke=window.__TAURI_INTERNALS__.invoke;
  const project=(await invoke('list_projects')).find(p=>p.name===collection);
  const photo=(await invoke('list_images',{input:{projectId:project.id,offset:0,limit:10,orderBy:null}}))[0];
  const input={projectId:project.id,photoId:photo.id};
  const recipe=await invoke('image_recipe',{input:{photoId:photo.id}});
  const before=await invoke('native_retouch_preview',{...input,before:true});
  const after=await invoke('native_retouch_preview',{...input,before:false});
  const coverage=await invoke('native_retouch_saved_selection',{input:{...input,operationId:null}});
  const bodyEdit=JSON.parse(recipe.body).studio_retouch_v1.find(e=>e.id.endsWith('-body-spots'));
  const bodyCoverage=bodyEdit?await invoke('native_retouch_saved_selection',{input:{...input,operationId:bodyEdit.id}}):null;
  let broadCoverage=null;
  if(localWings) {
   const wide=JSON.parse(recipe.body).studio_retouch_v1.filter(e=>e.tool!=='patch_heal' && e.tool!=='heal' && !(e.tool==='skin_uniformity' && e.id.includes('-spot-deep-') && Math.max(e.region[2],e.region[3])<=.1));
   const union=new Uint8Array(coverage.width*coverage.height*3);
   for(const e of wide) {
    const frame=await invoke('native_retouch_saved_selection',{input:{...input,operationId:e.id}});
    const raw=atob(frame.rgbBase64);
    for(let j=0;j<union.length;j+=3) {
     const a=Math.max(union[j],raw.charCodeAt(j));
     union[j]=a;union[j+1]=a;union[j+2]=a;
    }
   }
   let binary='';
   for(let j=0;j<union.length;j+=32768) binary+=String.fromCharCode(...union.subarray(j,j+32768));
   broadCoverage=btoa(binary);
  }
  let broadFrame=null;
  if(structureSpots) {
   const wide=JSON.parse(recipe.body).studio_retouch_v1.filter(e=>e.tool!=='patch_heal' && e.tool!=='heal' && !(e.tool==='skin_uniformity' && e.id.includes('-spot-deep-') && Math.max(e.region[2],e.region[3])<=.1));
   let undoCount=0;
   try {
    await invoke('native_retouch_edit',{input:{...input,action:'clear',edits:[],id:null}});
    undoCount++;
    if(wide.length) {
     await invoke('native_retouch_edit',{input:{...input,action:'append',edits:wide,id:null}});
     undoCount++;
    }
    broadFrame=await invoke('native_retouch_preview',{...input,before:false});
   } finally {
    for(let j=0;j<undoCount;j++) await invoke('history_step',{input:{...input,action:'undo'}});
    const restored=await invoke('image_recipe',{input:{photoId:photo.id}});
    if(restored.recipeHash!==recipe.recipeHash) throw new Error('Diagnostic recipe did not restore exactly');
   }
  }
  return {project:project.id,photo:photo.id,recipe,before,after,coverage,bodyCoverage,broadCoverage,broadFrame};
 }''',{'collection':args.collection,'localWings':args.allow_local_wing_repairs,'structureSpots':args.allow_structure_spots})
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
 broad_mask=base64.b64decode(data['broadCoverage']) if data['broadCoverage'] else None
 face=body['studio_portrait_auto_v1']['faces'][0]
 a,b,nose,mouth_a,mouth_b= [[x*w,y*h] for x,y in face['landmarks']]
 d=math.dist(a,b); u=[(b[0]-a[0])/d,(b[1]-a[1])/d]; v=[-u[1],u[0]]
 if sum(((mouth_a[i]+mouth_b[i]-a[i]-b[i])*.5)*v[i] for i in range(2))<0:
  v=[-v[0],-v[1]]
 features=[('left_eye',a,.30,.19,.035),('right_eye',b,.30,.19,.035),('nose',nose,.19,.38,-.17)]
 if args.include_nose:
  features=features[:2]+[(name,[nose[i]+side*d*u[i] for i in range(2)],.075,.04,.12)
    for name,side in [('left_nostril',-.18),('right_nostril',.18)]]
  features += [(name,[nose[i]+side*d*u[i] for i in range(2)],.09,.10,.12)
    for name,side in [('left_nostril_wing',-.30),('right_nostril_wing',.30)]]
 cores={name:[] for name,*_ in features}
 nose_skin=[]
 yy,xx=np.mgrid[:h,:w]
 for name,point,rx,ry,shift in features:
  dx=xx+.5-point[0];dy=yy+.5-point[1]
  across=(dx*u[0]+dy*u[1])/d;down=(dx*v[0]+dy*v[1])/d-shift
  cores[name]=(np.flatnonzero((across/rx)**2+(down/ry)**2<=1)*3).tolist()
 if args.include_nose:
  dx=xx+.5-nose[0];dy=yy+.5-nose[1]
  across=(dx*u[0]+dy*u[1])/d;down=(dx*v[0]+dy*v[1])/d+.22
  nose_skin=(np.flatnonzero((across/.17)**2+(down/.26)**2<=1)*3).tolist()
 metrics={}
 for name,indices in cores.items():
  if args.opening_core and name in ('left_nostril','right_nostril'): continue
  difference=[abs(before[i+c]-after[i+c]) for i in indices for c in range(3)]
  metrics[name]={'pixels':len(indices),'max_channel_change':max(difference),'mean_channel_change':sum(difference)/len(difference),'max_selection':max(mask[i] for i in indices)}
  if args.allow_local_wing_repairs and name.endswith('_wing'):
   metrics[name]['max_broad_selection']=max(broad_mask[i] for i in indices)
   assert metrics[name]['max_broad_selection']==0,metrics
   assert all(mask[i]>0 or all(before[i+c]==after[i+c] for c in range(3)) for i in indices),metrics
   metrics[name]['changes_limited_to_local_spot_selection']=True
  else:
   assert metrics[name]['max_selection']==0,metrics
   assert metrics[name]['max_channel_change']==0,metrics
 if args.opening_core:
  assert len(args.opening_core)==2,'Inspect both anatomical openings'
  for side,(l,t,r,b) in enumerate(args.opening_core):
   assert 0<=l<r<=1 and 0<=t<b<=1
   indices=[(y*w+x)*3 for y in range(int(t*h),int(b*h)) for x in range(int(l*w),int(r*w))]
   changes=[abs(before[i+c]-after[i+c]) for i in indices for c in range(3)]
   metrics[f'anatomical_opening_{side}']={'bounds':[l,t,r,b],'pixels':len(indices),'max_channel_change':max(changes),'max_selection':max(mask[i] for i in indices)}
   assert max(changes)==max(mask[i] for i in indices)==0,metrics
 if args.include_nose:
  selected=sum(mask[i]>127 for i in nose_skin)
  changed=sum(any(before[i+c]!=after[i+c] for c in range(3)) for i in nose_skin)
  metrics['nose_skin']={'pixels':len(nose_skin),'selected_pixels':selected,'changed_pixels':changed,
    'selected_fraction':selected/len(nose_skin)}
  assert 0<=args.min_nose_coverage<=1
  assert selected/len(nose_skin)>=args.min_nose_coverage,metrics
  if args.expect_nose_repair: assert changed>0, 'Nose is selected but no nose skin changed'
 if args.protected_rect:
  l,t,r,b=args.protected_rect
  assert 0<=l<r<=1 and 0<=t<b<=1
  indices=[(y*w+x)*3 for y in range(int(t*h),int(b*h)) for x in range(int(l*w),int(r*w))]
  difference=[abs(before[i+c]-after[i+c]) for i in indices for c in range(3)]
  metrics['structure_region']={'bounds':[l,t,r,b],'pixels':len(indices),'max_channel_change':max(difference)}
  if args.allow_structure_spots:
   changed=[i for i in indices if any(before[i+c]!=after[i+c] for c in range(3))]
   points=[((i//3)%w,(i//3)//w) for i in changed]
   assert only_local_spot_changes(points,body['studio_retouch_v1'],w,h), 'Unlocalized changes reach the structure region'
   broad_pixels=base64.b64decode(data['broadFrame']['rgbBase64'])
   assert all(before[i+c]==broad_pixels[i+c] for i in indices for c in range(3)), 'Broad processing changes the structure region'
   baseline=np.frombuffer(before,dtype=np.uint8).reshape(h,w,3)
   edited=np.frombuffer(after,dtype=np.uint8).reshape(h,w,3)
   contour=coarse_region_change(baseline,edited,[l,t,r,b],d)
   assert contour['max_relative_coarse_change']<.01,contour
   metrics['structure_region'].update(contour,changed_pixels=len(changed),broad_pixels_unchanged=True,changes_limited_to_saved_compact_spots=True)
  elif args.allow_structure_feather:
   changed=[i for i in indices if any(before[i+c]!=after[i+c] for c in range(3))]
   assert max(broad_mask[i] for i in indices)==0, 'Broad processing reaches the structure region'
   assert only_local_feather_changes([((i//3)%w,(i//3)//w) for i in changed],body['studio_retouch_v1'],w,h), 'A repair core or unlocalized change reaches the structure region'
   metrics['structure_region'].update(changed_pixels=len(changed),max_broad_selection=0,changes_limited_to_neighboring_repair_feather=True)
  else:
   assert max(difference)==0,metrics['structure_region']
 assert any(a!=b for a,b in zip(before,after)), 'Retouch must actually improve skin elsewhere'
 if args.require_body:
  assert data['bodyCoverage'], 'Visible body skin needs a saved blemish operation'
  body_mask=base64.b64decode(data['bodyCoverage']['rgbBase64'])
  selected=[i for i in range(0,len(body_mask),3) if body_mask[i]>127]
  metrics['body_skin']={'selected_pixels':len(selected),
    'changed_pixels':sum(any(before[i+c]!=after[i+c] for c in range(3)) for i in selected)}
  assert selected, 'Body operation has no selected pixels'
  for detected in body['studio_portrait_auto_v1']['faces']:
   l,t,r,b=detected['bounds']
   face_indices=[(y*w+x)*3 for y in range(int(t*h)+1,int(b*h)) for x in range(int(l*w)+1,int(r*w))]
   assert max(body_mask[i] for i in face_indices)==0, 'Body cleanup overlaps facial features'
 if not args.skip_ui:
  page.get_by_role('button',name=f'{args.collection} 1',exact=True).click()
  page.get_by_role('button',name='Auto edit One click, start to finish',exact=True).click()
  retouch=page.get_by_role('button',name='Retouch',exact=True)
  if retouch.count(): retouch.click()
  show=page.get_by_role('button',name='Show retouched areas',exact=True)
  expect(show).to_be_enabled(timeout=180000)
  if show.get_attribute('aria-pressed')!='true': show.click()
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
