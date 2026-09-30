import { useEffect, useMemo, useRef, useState } from 'react';
import { asIpcError, develop } from '../../ipc/client';
import { freshRetouch, nativeRetouch, RETOUCH_TOOLS, type NativeRetouchEdit, type RetouchTool, type BrushStroke } from '../../ipc/nativeRetouch';
import type { HistoryDto, RenderDto } from '../../ipc/types';
import { rgbDataUrl } from './rgbImage';
import { RetouchCanvas, type RetouchMode } from './RetouchCanvas';
import { RetouchControls, validRetouch } from './RetouchControls';
import { useRetouchDraftPreview } from './useRetouchDraftPreview';
import './precision-retouch.css';

export function NativeRetouchWorkspace({projectId, photoId, disabled = false, revision = 0, onClose, onBusyChange}: {
  projectId: string; photoId: string; disabled?: boolean; revision?: number; onClose: () => void; onBusyChange: (busy: boolean) => void;
}) {
  const [edits,setEdits] = useState<NativeRetouchEdit[]>([]);
  const [draft,setDraft] = useState(freshRetouch);
  const [selected,setSelected] = useState<string|null>(null);
  const [preview,setPreview] = useState<RenderDto|null>(null);
  const [before,setBefore] = useState<RenderDto|null>(null);
  const [compare,setCompare] = useState(false);
  const [split,setSplit] = useState(false);
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
  const draftState = useRetouchDraftPreview(projectId,photoId,draft,selected,live&&dirty&&!busy&&!disabled&&validRetouch(draft),refresh+revision);
  const lock = useRef(false);
  const mounted = useRef(true);
  useEffect(()=>{mounted.current=true;return ()=>{mounted.current=false;onBusyChange(false);};},[onBusyChange]);
  useEffect(()=>{onBusyChange(busy||draftState.pending||dirty);},[busy,draftState.pending,dirty,onBusyChange]);
  useEffect(()=>{
    let active=true; setBusy(true); setError(null);
    void Promise.all([nativeRetouch.edit(projectId,photoId,'list'),nativeRetouch.preview(projectId,photoId),nativeRetouch.preview(projectId,photoId,true),develop.imageHistory({photoId})])
      .then(([next,image,original,h])=>{if(active){setEdits(next);setPreview(image);setBefore(original);setHistory(h);}})
      .catch(cause=>{if(active)setError(asIpcError(cause).message);})
      .finally(()=>{if(active)setBusy(false);});
    return ()=>{active=false;};
  },[projectId,photoId,refresh,revision]);
  const blocked = busy || disabled;
  const rendered=compare?before:draftState.image??preview;
  const src=useMemo(()=>rendered?rgbDataUrl(rendered):null,[rendered]);
  const beforeSrc=useMemo(()=>before?rgbDataUrl(before):null,[before]);
  const comparisonReady=Boolean(before&&rendered&&before.width===rendered.width&&before.height===rendered.height);
  const stackBlocked=blocked||dirty;
  const draftNotice='Apply or discard your draft before selecting saved operations or changing history.';
  const save=async(action:()=>Promise<unknown>, appliesDraft=false)=>{
    if(dirty&&!appliesDraft){setError(draftNotice);return;}
    if(lock.current||blocked)return;
    lock.current=true;setBusy(true);setError(null);
    try {await action();if(mounted.current){setSelected(null);setDirty(false);setRefresh(v=>v+1);}}
    catch(cause){if(mounted.current){setError(asIpcError(cause).message);setBusy(false);}}
    finally {lock.current=false;}
  };
  const change=(patch:Partial<NativeRetouchEdit>)=>{setDraft(value=>({...value,...patch}));setDirty(true);setCompare(false);};
  const chooseTool=(tool:RetouchTool)=>{
    if(selected&&dirty){setError('Apply or discard your changes before choosing another tool.');return;}
    setSelected(null);change({id:'draft',tool,enabled:true,amount:.65,texture:1,tone:.5,warmth:tool==='makeup'?.2:0,tint:tool==='makeup'?.2:0});
  };
  const chooseMode=(next:RetouchMode)=>{
    setMode(next);setSourceMode(false);
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
  const discard=()=>{const saved=edits.find(edit=>edit.id===selected);setDraft(saved??freshRetouch());setMode(saved?.mask?'paint':'ellipse');setDirty(false);setError(null);};
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
    else if(event.key.toLowerCase()==='h')chooseMode('pan');
    else if(event.key.toLowerCase()==='v')chooseMode('ellipse');
    else if(event.key==='[')setBrushRadius(v=>Math.max(.0005,v/1.2));
    else if(event.key===']')setBrushRadius(v=>Math.min(.25,v*1.2));
    else if(event.key==='Enter')apply();
    else return;
    event.preventDefault();
  }}>
    <div className="studio-toolbar"><div><span className="eyebrow">NATIVE RETOUCH</span><h2>Precision, at your pace.</h2></div><button type="button" disabled={blocked} onClick={()=>{if(dirty)setError('Apply your changes or choose Discard draft before leaving Retouch.');else onClose();}}>Back to Develop</button></div>
    <p className="lr-hint">Paint a selection, preview your changes, then apply. Alt-click to sample a source. This view shows the full photo; your crop and perspective are applied in Develop and export.</p>
    {(error||draftState.error)&&<p role="alert">{error||draftState.error} <button type="button" disabled={blocked} onClick={()=>setRefresh(v=>v+1)}>Reload retouch</button></p>}
    <div className="retouch-selection-toolbar" aria-label="Selection tools">
      {([['ellipse','Ellipse (V)'],['paint','Brush (B)'],['erase','Eraser (E)'],['pan','Hand (H)']] as const).map(([value,label])=><button type="button" key={value} disabled={blocked} aria-pressed={mode===value} onClick={()=>chooseMode(value)}>{label}</button>)}
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
        <RetouchCanvas src={src} width={rendered?.width??1200} height={rendered?.height??800} compare={compare} beforeSrc={comparisonReady?beforeSrc:null} split={split&&!compare} disabled={blocked}
          draft={draft} mode={mode} radius={brushRadius} opacity={brushOpacity} overlay={overlay} sourceMode={sourceMode}
          onTarget={point=>change({region:[...point,draft.region[2],draft.region[3]]})}
          onSource={point=>{change({source:point});setSourceMode(false);}} onStroke={addStroke} onNotice={setError}/>
        <div className="studio-history">
          <button type="button" disabled={blocked||!before} aria-pressed={compare} onClick={()=>setCompare(v=>!v)}>{compare?'Show retouched':'Show before retouch'}</button>
          <button type="button" disabled={!comparisonReady||blocked} aria-pressed={split&&!compare} onClick={()=>{setCompare(false);setSplit(v=>compare||!v);}}>Split comparison</button>
          <button type="button" aria-pressed={overlay} onClick={()=>setOverlay(v=>!v)}>Selection overlay</button>
          <button type="button" disabled={stackBlocked||!history?.canUndo} onClick={()=>void save(()=>develop.historyStep({projectId,photoId,action:'undo'}))}>Undo</button>
          <button type="button" disabled={stackBlocked||!history?.canRedo} onClick={()=>void save(()=>develop.historyStep({projectId,photoId,action:'redo'}))}>Redo</button>
        </div>
        <p role="status">{blocked?'Rendering your retouch…':draftState.pending?'Rendering unsaved preview…':draftState.image?'Unsaved preview. Apply to keep this change.':dirty?'Unsaved changes. Preview or apply when ready.':`${edits.length} saved operation${edits.length===1?'':'s'}. Originals stay untouched.`}</p>
        {dirty&&<p className="lr-hint">{draftNotice}</p>}
        <details open><summary>Saved retouch operations ({edits.length})</summary>
          <ol>{edits.map((edit,index)=><li key={edit.id}>
            <button type="button" disabled={stackBlocked} aria-pressed={selected===edit.id} onClick={()=>{setSelected(edit.id);setDirty(false);setMode(edit.mask?'paint':'ellipse');setDraft({...edit,region:[...edit.region],source:edit.source?[...edit.source]:null});setCompare(false);}}>{index+1}. {RETOUCH_TOOLS.find(t=>t[0]===edit.tool)?.[1]} · {Math.round(edit.amount*100)}%</button>
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
      <RetouchControls draft={draft} selected={selected} count={edits.length} disabled={blocked||!preview} sourceMode={sourceMode} live={live} dirty={dirty}
        onChange={change} onTool={chooseTool} onSourceMode={()=>{setSourceMode(v=>!v);setCompare(false);}} onLive={setLive} onApply={apply}
        onSelectAll={()=>{setMode('ellipse');change({region:[.5,.5,1,1],mask:null,feather:0});}}
        onNew={()=>{if(dirty){setError(draftNotice);return;}setSelected(null);change({id:'draft',enabled:true});}} onDiscard={discard} onPreset={preset}/>

    </div>
    <button type="button" disabled={stackBlocked||!edits.length} onClick={()=>void save(()=>nativeRetouch.edit(projectId,photoId,'clear'))}>Clear native retouch</button>
    <details className="retouch-shortcuts"><summary>Keyboard shortcuts &amp; tips</summary>
      <p>B brush · E eraser · V ellipse · H hand · [ / ] brush size · Enter apply · Ctrl/Cmd+Z undo stroke or saved edit · Shift+Ctrl/Cmd+Z redo.</p>
      <p>Focus the photo to use shortcuts. Arrow keys pan; + / − zoom; 0 fits the preview. Numeric target coordinates and Dab at target coordinates provide an alternative to drawing. Pen pressure changes brush radius.</p>
    </details>
  </section>;
}
