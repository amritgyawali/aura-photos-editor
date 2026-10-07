import { useDeferredValue, useEffect, useMemo, useRef, useState } from 'react';
import { asIpcError, develop } from '../../ipc/client';
import { blemishBrush, freshRetouch, nativeRetouch, RETOUCH_TOOLS, type NativeRetouchEdit, type RetouchTool, type BrushStroke } from '../../ipc/nativeRetouch';
import type { HistoryDto, RecipeDto, RenderDto } from '../../ipc/types';
import { PortraitAutoReport } from './PortraitAutoReport';
import { AutoRetouchSettings } from './AutoRetouchSettings';
import { AdvancedRetouch } from './AdvancedRetouch';
import { changesDataUrl, coverageDataUrl } from './rgbImage';
import { previewSource, useProgressivePreview } from '../../state/previewCache';
import { useSavedRetouchCoverage } from './useSavedRetouchCoverage';
import { RetouchCanvas, type RetouchMode } from './RetouchCanvas';
import { RetouchControls, validRetouch } from './RetouchControls';
import { useRetouchDraftPreview } from './useRetouchDraftPreview';
import './precision-retouch.css';

/** A short provenance tag for operations the automatic pass wrote. */
export function automaticLabel(id: string): string {
  const scene = /^auto-scene-v\d+-/.exec(id);
  if (scene) return ' · Auto (scene)';
  if (/^auto-portrait-v\d+-backdrop$/.test(id)) return ' · Auto (backdrop)';
  if (/^auto-portrait-v\d+-dust-\d+$/.test(id)) return ' · Auto (backdrop dust)';
  if (/^auto-portrait-v\d+-background-tone$/.test(id)) return ' · Auto (background tone)';
  if (/^auto-portrait-v\d+-stray-\d+$/.test(id)) return ' · Auto (stray hair)';
  if (/^auto-portrait-v\d+-jewel-\d+$/.test(id)) return ' · Auto (reflection)';
  const face = /^auto-portrait-v\d+-(\d+)-(body-|lines-neck|hair-|fabric|cloth-)?/.exec(id);
  if (!face) return '';
  const kind = face[2] === 'hair-' ? 'hair' : face[2] === 'fabric' || face[2] === 'cloth-' ? 'clothes' : face[2] ? 'body' : 'face';
  return ` · Auto (${kind} ${Number(face[1]) + 1})`;
}

export function NativeRetouchWorkspace({projectId, photoId, disabled = false, revision = 0, onClose, onBusyChange}: {
  projectId: string; photoId: string; disabled?: boolean; revision?: number; onClose: () => void; onBusyChange: (busy: boolean) => void;
}) {
  const [edits,setEdits] = useState<NativeRetouchEdit[]>([]);
  const [recipe,setRecipe] = useState<RecipeDto|null>(null);
  const [analysing,setAnalysing] = useState(false);
  const [draft,setDraft] = useState(freshRetouch);
  const [selected,setSelected] = useState<string|null>(null);
  const [compare,setCompare] = useState(false);
  const [split,setSplit] = useState(false);
  const [maskView,setMaskView] = useState(false);
  const [coverageView,setCoverageView] = useState(false);
  const [coverageOperation,setCoverageOperation] = useState<string|null>(null);
  const [coverageOpacity,setCoverageOpacity] = useState(.5);
  /** What Show retouched areas draws: the saved selections, or the pixels the retouch changed. */
  const [coverageMode,setCoverageMode] = useState<'selection'|'changes'>('selection');
  const displayedCoverageOpacity = useDeferredValue(coverageOpacity);
  const [overlay,setOverlay] = useState(true);
  const [sourceMode,setSourceMode] = useState(false);
  const [history,setHistory] = useState<HistoryDto|null>(null);
  const [refresh,setRefresh] = useState(0);
  const [busy,setBusy] = useState(true);
  const [error,setError] = useState<string|null>(null);
  const [dirty,setDirty] = useState(false);
  const [live,setLive] = useState(false);
  const [mode,setMode] = useState<RetouchMode>('ellipse');
  const [brushRadius,setBrushRadius] = useState(.025);
  const [brushOpacity,setBrushOpacity] = useState(1);
  const draftState = useRetouchDraftPreview(projectId,photoId,draft,selected,(maskView||(live&&dirty))&&!busy&&!disabled&&validRetouch(draft),refresh+revision,maskView);
  const lock = useRef(false);
  const mounted = useRef(true);
  useEffect(()=>{mounted.current=true;return ()=>{mounted.current=false;onBusyChange(false);};},[onBusyChange]);
  useEffect(()=>{onBusyChange(busy||draftState.pending||dirty);},[busy,draftState.pending,dirty,onBusyChange]);
  // Which load of the saved stack this is; a preview is asked for once it has arrived.
  const [loaded,setLoaded] = useState(0);
  useEffect(()=>{
    let active=true; setBusy(true); setError(null);
    void Promise.all([nativeRetouch.edit(projectId,photoId,'list'),develop.imageHistory({photoId}),develop.imageRecipe({photoId})])
      .then(([next,h,r])=>{if(active){setEdits(next);setHistory(h);setRecipe(r);setLoaded(v=>v+1);}})
      .catch(cause=>{if(active)setError(asIpcError(cause).message);})
      .finally(()=>{if(active)setBusy(false);});
    return ()=>{active=false;};
  },[projectId,photoId,refresh,revision]);
  // The photograph at its own resolution, cached for the session and on disk (ADR-0097): a
  // fast first look while the full-quality preview is made, then the full-quality one.
  // Keyed by the recipe hash, so a saved change is a new picture and an unchanged one is reused.
  const version = recipe?.photoId===photoId&&recipe.recipeHash ? recipe.recipeHash : loaded ? `load-${loaded}` : null;
  const after = useProgressivePreview(version?`${projectId}:${photoId}:retouched:${version}`:null, photoId,
    quality=>nativeRetouch.preview(projectId,photoId,false,quality),refresh+revision);
  const original = useProgressivePreview(version?`${projectId}:${photoId}:before-retouch:${version}`:null, photoId,
    quality=>nativeRetouch.preview(projectId,photoId,true,quality),refresh+revision);
  const preview: RenderDto|null = after.image;
  const before: RenderDto|null = original.image;
  const previewError = after.error ?? original.error;
  const blocked = busy || disabled;
  const coverage = useSavedRetouchCoverage(projectId, photoId, `${recipe?.recipeHash}:${refresh}:${revision}`,
    coverageOperation, coverageView && coverageMode === 'selection' && !blocked);
  const coverageSrc = useMemo(() => {
    if (coverageMode === 'changes') return before && preview ? changesDataUrl(before, preview, displayedCoverageOpacity) : null;
    return coverage.image && preview ? coverageDataUrl(preview, coverage.image, displayedCoverageOpacity) : null;
  }, [coverageMode, coverage.image, before, preview, displayedCoverageOpacity]);
  useEffect(() => { setCoverageOperation(null); setCoverageView(false); }, [photoId, projectId]);
  useEffect(() => { if (coverageOperation && !edits.some(e => e.id === coverageOperation)) setCoverageOperation(null); }, [edits, coverageOperation]);
  const rendered=maskView?draftState.image:compare?before:draftState.image??preview;
  const src=useMemo(()=>previewSource(rendered),[rendered]);
  const beforeSrc=useMemo(()=>previewSource(before),[before]);
  const comparisonReady=Boolean(!maskView&&beforeSrc&&src&&before&&rendered&&before.width===rendered.width&&before.height===rendered.height);
  const stackBlocked=blocked||dirty;
  const retouchRecipe=recipe?.photoId===photoId?recipe:null;
  const draftNotice='Apply or discard your draft before selecting saved operations or changing history.';
  const save=async(action:()=>Promise<unknown>, appliesDraft=false)=>{
    if(dirty&&!appliesDraft){setError(draftNotice);return;}
    if(lock.current||blocked)return;
    lock.current=true;setBusy(true);setError(null);
    try {await action();if(mounted.current){setSelected(null);setDirty(false);setRefresh(v=>v+1);}}
    catch(cause){if(mounted.current){setError(asIpcError(cause).message);setBusy(false);}}
    finally {lock.current=false;}
  };
  const change=(patch:Partial<NativeRetouchEdit>)=>{setDraft(value=>({...value,...patch}));setDirty(true);setCompare(false);setCoverageView(false);};
  const autoPortrait=()=>void save(async()=>{
    setAnalysing(true);
    try { await nativeRetouch.autoPortrait(photoId); }
    finally { if(mounted.current)setAnalysing(false); }
  });
  const chooseTool=(tool:RetouchTool)=>{
    if(selected&&dirty){setError('Apply or discard your changes before choosing another tool.');return;}
    setSelected(null);change({id:'draft',tool,enabled:true,amount:.65,texture:1,tone:.5,warmth:tool==='makeup'?.2:0,tint:tool==='makeup'?.2:0});
  };
  const chooseMode=(next:RetouchMode)=>{
    setMode(next);setSourceMode(false);setMaskView(false);
    if(next==='gradient'){change({mask:null,selection:{...draft.selection,gradient:draft.selection?.gradient??{start:[.2,.5],end:[.8,.5]}}});return;}
    if(next!=='pan'&&draft.selection?.gradient)change({selection:{...draft.selection,gradient:null}});
    if(next==='ellipse'&&draft.mask)change({mask:null});
    if((next==='paint'||next==='erase')&&!draft.mask)change({mask:{strokes:[]}});
  };
  const addStroke=(stroke:BrushStroke)=>{
    if(blocked)return;
    const strokes=draft.mask?.strokes??[];
    if(strokes.length>=128||strokes.reduce((n,s)=>n+s.points.length,stroke.points.length)>8192){setError('Mask limit reached. Apply this operation before starting another.');return;}
    const first=stroke.points[0];
    change({mask:{strokes:[...strokes,stroke]},...(!strokes.length&&first?{region:[first[0],first[1],draft.region[2],draft.region[3]] as NativeRetouchEdit['region']}:{})});
  };
  const undoStroke=()=>{if(!blocked&&draft.mask?.strokes.length)change({mask:{strokes:draft.mask.strokes.slice(0,-1)}});};
  const apply=()=>{if(validRetouch(draft)&&(selected||edits.length<256)){setCompare(false);void save(()=>nativeRetouch.edit(projectId,photoId,selected?'update':'append',[draft]),true);}};
  const discard=()=>{const saved=edits.find(edit=>edit.id===selected);setDraft(saved??freshRetouch());setMode(saved?.selection?.gradient?'gradient':saved?.mask?'paint':'ellipse');setDirty(false);setError(null);};
  /** The blemish brush: paint over what the automatic retouch left; Apply keeps it. */
  const startBlemishBrush=()=>{
    if(dirty&&draft.tool!=='acne_clear'){setError(draftNotice);return;}
    setSelected(null);setMode('paint');setSourceMode(false);setMaskView(false);setCoverageView(false);setCompare(false);setSplit(false);setError(null);
    setBrushRadius(value=>Math.min(value,.03));
    setDraft(value=>value.tool==='acne_clear'&&value.mask?value:blemishBrush([value.region[0],value.region[1],.035,.035]));
    setDirty(true);setLive(true);
  };
  const preset=(polished:boolean)=>{
    if(selected&&dirty){setError(draftNotice);return;}
    const base={...draft,id:'draft',enabled:true,texture:1,source:null};
    const items:NativeRetouchEdit[]=[{...base,tool:'frequency',amount:polished?0.45:0.25,tone:0.5},{...base,tool:'micro_dodge_burn',amount:polished?0.35:0.18},{...base,tool:'mattify',amount:polished?0.35:0.2}];
    void save(()=>nativeRetouch.edit(projectId,photoId,'append',items),true);
  };
  return <section className="photo-studio native-retouch-workspace" aria-label="Native retouch workspace" onKeyDown={event=>{
    if(blocked||(event.target as HTMLElement).matches('input,select,textarea,button'))return;
    if((event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==='z'){
      if(dirty&&draft.mask?.strokes.length&&!event.shiftKey)undoStroke();
      else if(event.shiftKey?history?.canRedo:history?.canUndo)void save(()=>develop.historyStep({projectId,photoId,action:event.shiftKey?'redo':'undo'}));
    }else if(event.ctrlKey||event.metaKey||event.altKey)return;
    else if(event.key.toLowerCase()==='b')chooseMode('paint');
    else if(event.key.toLowerCase()==='e')chooseMode('erase');
    else if(event.key.toLowerCase()==='g')chooseMode('gradient');
    else if(event.key.toLowerCase()==='h')chooseMode('pan');
    else if(event.key.toLowerCase()==='v')chooseMode('ellipse');
    else if(event.key==='[')setBrushRadius(v=>Math.max(.0005,v/1.2));
    else if(event.key===']')setBrushRadius(v=>Math.min(.25,v*1.2));
    else if(event.key==='Enter'&&!coverageView)apply();
    else return;
    event.preventDefault();
  }}>
    <div className="studio-toolbar"><div><span className="eyebrow">NATIVE RETOUCH</span><h2>Precision, at your pace.</h2></div><button type="button" disabled={blocked} onClick={()=>{if(dirty)setError('Apply your changes or choose Discard draft before leaving Retouch.');else onClose();}}>Back to Develop</button></div>
    <p className="lr-hint">Paint a selection, preview your changes, then apply. Alt-click to sample a source. This view shows the full photo; your crop and perspective are applied in Develop and export.</p>
    {(error||draftState.error||previewError)&&<p role="alert">{error||draftState.error||previewError} <button type="button" disabled={blocked} onClick={()=>setRefresh(v=>v+1)}>Reload retouch</button></p>}
    <div className="retouch-blemish-brush">
      <button type="button" className="retouch-primary" disabled={blocked} aria-pressed={draft.tool==='acne_clear'&&mode==='paint'} onClick={startBlemishBrush}>Blemish brush</button>
      <p className="lr-hint">Something left? Choose Blemish brush, paint over the spots, pimples or marks you still see - on the nose and between the brows too - then Apply retouch. Each one is rebuilt from the clean skin around it and the pores stay. Use Show retouched areas to see what was changed.</p>
    </div>
    <div className="retouch-selection-toolbar" aria-label="Selection tools">
      {([['ellipse','Ellipse (V)'],['paint','Brush (B)'],['erase','Eraser (E)'],['gradient','Gradient (G)'],['pan','Hand (H)']] as const).map(([value,label])=><button type="button" key={value} disabled={blocked} aria-pressed={mode===value} onClick={()=>chooseMode(value)}>{label}</button>)}
      <button type="button" disabled={blocked||!draft.mask?.strokes.length} onClick={undoStroke}>Undo brush stroke</button>
      <button type="button" disabled={blocked||!draft.mask?.strokes.length} onClick={()=>change({mask:{strokes:[]}})}>Clear painted mask</button>
    </div>
    {(mode==='paint'||mode==='erase')&&<div className="retouch-brush-controls">
      <label>Brush radius (% of short edge)<input type="range" min="0.05" max="25" step="0.05" value={brushRadius*100} disabled={blocked} onChange={event=>setBrushRadius(Number(event.target.value)/100)}/><output>{(brushRadius*100).toFixed(2)}%</output></label>
      <label>Brush opacity<input type="range" min="0.05" max="1" step="0.05" value={brushOpacity} disabled={blocked} onChange={event=>setBrushOpacity(Number(event.target.value))}/><output>{Math.round(brushOpacity*100)}%</output></label>
      <button type="button" disabled={blocked} onClick={()=>addStroke({erase:mode==='erase',radius:brushRadius,opacity:brushOpacity,points:[[draft.region[0],draft.region[1],1]]})}>Dab at target coordinates</button>
    </div>}
    <div className="studio-layout">
      <div>
        <RetouchCanvas src={coverageView?coverageSrc:src} width={rendered?.width??preview?.width??1200} height={rendered?.height??preview?.height??800} compare={compare} beforeSrc={comparisonReady?beforeSrc:null} split={split&&!compare&&!maskView&&!coverageView} maskView={maskView} coverageView={coverageView} disabled={blocked}
          draft={draft} mode={mode} radius={brushRadius} opacity={brushOpacity} overlay={overlay} sourceMode={sourceMode}
          onTarget={point=>change({region:[...point,draft.region[2],draft.region[3]]})}
          onGradient={(start,end)=>change({mask:null,selection:{...draft.selection,gradient:{start,end}}})}
          onSource={point=>{change({source:point});setSourceMode(false);}} onStroke={addStroke} onNotice={setError}/>
        <div className="studio-history">
          <button type="button" disabled={blocked||!before} aria-pressed={compare} onClick={()=>{setCoverageView(false);setMaskView(false);setCompare(v=>!v);}}>{compare?'Show retouched':'Show before retouch'}</button>
          <button type="button" disabled={!comparisonReady||blocked} aria-pressed={split&&!compare&&!maskView&&!coverageView} onClick={()=>{setCoverageView(false);setCompare(false);setSplit(v=>compare||!v);}}>Split comparison</button>
          <button type="button" disabled={blocked||(!maskView&&!validRetouch(draft))} aria-pressed={maskView} onClick={()=>{setCoverageView(false);setCompare(false);setSplit(false);setMaskView(v=>!v);}}>Preview selection mask</button>
          <button type="button" disabled={stackBlocked||!edits.length} aria-pressed={coverageView} onClick={()=>{setCompare(false);setSplit(false);setMaskView(false);setCoverageView(v=>!v);}}>Show retouched areas</button>
          <button type="button" aria-pressed={overlay} onClick={()=>setOverlay(v=>!v)}>Selection overlay</button>
          <button type="button" disabled={stackBlocked||!history?.canUndo} onClick={()=>void save(()=>develop.historyStep({projectId,photoId,action:'undo'}))}>Undo</button>
          <button type="button" disabled={stackBlocked||!history?.canRedo} onClick={()=>void save(()=>develop.historyStep({projectId,photoId,action:'redo'}))}>Redo</button>
        </div>
        {coverageView && <div className="retouch-coverage-controls" aria-label="Saved retouch coverage">
          <label>Show<select aria-label="Show" value={coverageMode} disabled={blocked} onChange={event=>setCoverageMode(event.target.value==='changes'?'changes':'selection')}>
            <option value="selection">Skin selected for retouch (teal)</option>
            <option value="changes">Pixels the retouch changed (orange)</option>
          </select></label>
          {coverageMode==='selection'&&<label>Show selection for<select value={coverageOperation??''} disabled={blocked} onChange={event=>setCoverageOperation(event.target.value||null)}>
            <option value="">All enabled saved retouch</option>
            {edits.map((edit,index)=><option key={edit.id} value={edit.id}>{index+1}. {RETOUCH_TOOLS.find(t=>t[0]===edit.tool)?.[1]}{automaticLabel(edit.id)}{edit.enabled?'':' (disabled)'}</option>)}
          </select></label>}
          <label>Overlay visibility<input type="range" min=".15" max=".85" step=".05" value={coverageOpacity} onChange={event=>setCoverageOpacity(Number(event.target.value))}/></label>
          {coverageMode==='selection'?<>
            <p><span className="retouch-coverage-swatch"/> Teal shows the saved retouch selections, including their soft edges. Clear areas are excluded. A selected pixel may stay unchanged when the tool finds nothing to correct. Disabled steps and unsaved drafts are excluded.</p>
            {coverage.pending && <p role="status">Reading saved retouch selections…</p>}
            {coverage.error && <p role="alert">Could not show retouch coverage: {coverage.error}</p>}
            {coverage.image && !coverageSrc && <p role="alert">Coverage does not match this preview. Reload retouch to try again.</p>}
          </>:<>
            <p><span className="retouch-coverage-swatch retouch-changes-swatch"/> Orange marks every pixel the saved retouch changed, compared with the photo before retouch: each healed spot shows as a dot. Anything you still see that is not orange was left as photographed; paint it with the Blemish brush.</p>
            {!coverageSrc && (after.upgrading || original.upgrading ? <p role="status">Finishing the full-quality before and after…</p> : <p role="alert">The before and after previews do not match. Reload retouch to try again.</p>)}
          </>}
        </div>}
        <p role="status">{blocked?'Rendering your retouch…':draftState.pending?'Rendering unsaved preview…':draftState.image?maskView?'Selection preview only. White is selected; black is protected.':'Unsaved preview. Apply to keep this change.':dirty?'Unsaved changes. Preview or apply when ready.':`${edits.length} saved operation${edits.length===1?'':'s'}. Originals stay untouched.`}
          {preview && !draftState.image && <> · {after.upgrading ? 'Quick preview - rendering full quality…' : `Full quality ${preview.width} × ${preview.height}`}</>}</p>
        <AdvancedRetouch projectId={projectId} photoId={photoId} recipe={retouchRecipe} disabled={stackBlocked||!preview||!retouchRecipe} onRun={task=>void save(task)}/>
        <button type="button" disabled={stackBlocked||!preview} onClick={autoPortrait}>{analysing?'Detecting faces and preparing skin retouch…':'Auto portrait'}</button>
        <AutoRetouchSettings key={`${photoId}:${retouchRecipe?.recipeHash ?? 'loading'}`} recipe={retouchRecipe} disabled={stackBlocked||!preview||!retouchRecipe} busy={analysing} onRun={options=>void save(async()=>{
          setAnalysing(true);
          try { await nativeRetouch.autoRetouch(projectId,photoId,options); }
          finally { if(mounted.current)setAnalysing(false); }
        })}/>
        <p className="lr-hint">Detects face and body skin locally. Reuse detected skin selections in any retouch tool, preview the mask, or refine it with the brush. All steps can be edited or undone.</p>
        <PortraitAutoReport recipe={recipe}/>
        {dirty&&<p className="lr-hint">{draftNotice}</p>}
        <details open><summary>Saved retouch operations ({edits.length})</summary>
          <ol>{edits.map((edit,index)=><li key={edit.id}>
            <button type="button" disabled={stackBlocked} aria-pressed={selected===edit.id} onClick={()=>{setSelected(edit.id);setDirty(false);setMode(edit.selection?.gradient?'gradient':edit.mask?'paint':'ellipse');setDraft({...edit,region:[...edit.region],source:edit.source?[...edit.source]:null});setCompare(false);}}>{index+1}. {RETOUCH_TOOLS.find(t=>t[0]===edit.tool)?.[1]} · {Math.round(edit.amount*100)}%{automaticLabel(edit.id)}</button>
            <label><input type="checkbox" checked={edit.enabled} disabled={stackBlocked} onChange={event=>void save(()=>nativeRetouch.edit(projectId,photoId,'update',[{...edit,enabled:event.target.checked}]))}/>Enabled</label>
            <div className="retouch-operation-actions">
              <button type="button" disabled={stackBlocked||index===0} aria-label={`Move retouch ${index+1} earlier`} onClick={()=>void save(()=>nativeRetouch.edit(projectId,photoId,'earlier',[],edit.id))}>↑</button>
              <button type="button" disabled={stackBlocked||index===edits.length-1} aria-label={`Move retouch ${index+1} later`} onClick={()=>void save(()=>nativeRetouch.edit(projectId,photoId,'later',[],edit.id))}>↓</button>
              <button type="button" disabled={stackBlocked||edits.length>=256} aria-label={`Duplicate retouch ${index+1}`} onClick={()=>void save(()=>nativeRetouch.edit(projectId,photoId,'duplicate',[],edit.id))}>Duplicate</button>
            </div>
            <button type="button" disabled={stackBlocked} aria-label={`Remove retouch ${index+1}`} onClick={()=>void save(()=>nativeRetouch.edit(projectId,photoId,'remove',[],edit.id))}>Remove</button>
          </li>)}</ol>
        </details>
      </div>
      <RetouchControls draft={draft} edits={edits} selected={selected} count={edits.length} disabled={blocked||!preview} sourceMode={sourceMode} live={live} dirty={dirty}
        onChange={change} onTool={chooseTool} onSourceMode={()=>{setSourceMode(v=>!v);setCompare(false);setMaskView(false);setSplit(false);}} onLive={setLive} onApply={apply}
        onSelectAll={()=>{setMode('ellipse');change({region:[.5,.5,1,1],mask:null,matte:null,feather:0,selection:{...draft.selection,inverted:false,gradient:null}});}}
        onNew={()=>{if(dirty){setError(draftNotice);return;}setSelected(null);change({id:'draft',enabled:true});}} onDiscard={discard} onPreset={preset}/>

    </div>
    <button type="button" disabled={stackBlocked||!edits.length} onClick={()=>void save(()=>nativeRetouch.edit(projectId,photoId,'clear'))}>Clear native retouch</button>
    <details className="retouch-shortcuts"><summary>Keyboard shortcuts &amp; tips</summary>
      <p>B brush · E eraser · V ellipse · H hand · [ / ] brush size · Enter apply · Ctrl/Cmd+Z undo stroke or saved edit · Shift+Ctrl/Cmd+Z redo.</p>
      <p>Focus the photo to use shortcuts. Arrow keys pan; + / − zoom; 0 fits the preview. Numeric target coordinates and Dab at target coordinates provide an alternative to drawing. Pen pressure changes brush radius.</p>
    </details>
  </section>;
}
