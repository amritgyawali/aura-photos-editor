import { useEffect, useMemo, useRef, useState } from 'react';
import { asIpcError, develop } from '../../ipc/client';
import { freshRetouch, nativeRetouch, RETOUCH_TOOLS, type NativeRetouchEdit, type RetouchTool } from '../../ipc/nativeRetouch';
import type { HistoryDto, RenderDto } from '../../ipc/types';
import { rgbDataUrl } from './rgbImage';

export function NativeRetouchWorkspace({projectId, photoId, disabled = false, revision = 0, onClose, onBusyChange}: {
  projectId: string; photoId: string; disabled?: boolean; revision?: number; onClose: () => void; onBusyChange: (busy: boolean) => void;
}) {
  const [edits,setEdits] = useState<NativeRetouchEdit[]>([]);
  const [draft,setDraft] = useState(freshRetouch);
  const [selected,setSelected] = useState<string|null>(null);
  const [preview,setPreview] = useState<RenderDto|null>(null);
  const [before,setBefore] = useState<RenderDto|null>(null);
  const [compare,setCompare] = useState(false);
  const [overlay,setOverlay] = useState(true);
  const [sourceMode,setSourceMode] = useState(false);
  const [history,setHistory] = useState<HistoryDto|null>(null);
  const [refresh,setRefresh] = useState(0);
  const [busy,setBusy] = useState(true);
  const [error,setError] = useState<string|null>(null);
  const lock = useRef(false);
  const mounted = useRef(true);
  useEffect(()=>{mounted.current=true;return ()=>{mounted.current=false;onBusyChange(false);};},[onBusyChange]);
  useEffect(()=>{onBusyChange(busy);},[busy,onBusyChange]);
  useEffect(()=>{
    let active=true; setBusy(true); setError(null);
    void Promise.all([nativeRetouch.edit(projectId,photoId,'list'),nativeRetouch.preview(projectId,photoId),nativeRetouch.preview(projectId,photoId,true),develop.imageHistory({photoId})])
      .then(([next,image,original,h])=>{if(active){setEdits(next);setPreview(image);setBefore(original);setHistory(h);}})
      .catch(cause=>{if(active)setError(asIpcError(cause).message);})
      .finally(()=>{if(active)setBusy(false);});
    return ()=>{active=false;};
  },[projectId,photoId,refresh,revision]);
  const blocked = busy || disabled;
  const src=useMemo(()=>{const r=compare?before:preview;return r?rgbDataUrl(r):null;},[compare,before,preview]);
  const info=RETOUCH_TOOLS.find(t=>t[0]===draft.tool)!;
  const needsSource=draft.tool==='clone'||draft.tool==='color_match';
  const bands=['frequency','wrinkle','fabric'].includes(draft.tool);
  const color=draft.tool==='skin_color'||draft.tool==='makeup';
  const numericValid=[...draft.region,draft.amount,draft.feather,draft.radius,draft.texture,draft.tone,draft.warmth,draft.tint,...(draft.source??[])].every(Number.isFinite)
    &&draft.region.every(v=>v>=0&&v<=1)&&draft.region[2]>=0.001&&draft.region[3]>=0.001
    &&draft.amount>=0&&draft.amount<=1&&draft.feather>=0&&draft.feather<=1
    &&draft.radius>=0.0005&&draft.radius<=0.05&&draft.texture>=0&&draft.texture<=2&&draft.tone>=0&&draft.tone<=1
    &&Math.abs(draft.warmth)<=1&&Math.abs(draft.tint)<=1&&(!draft.source||draft.source.every(v=>v>=0&&v<=1));
  const save=async(action:()=>Promise<unknown>)=>{
    if(lock.current||blocked)return;
    lock.current=true;setBusy(true);setError(null);
    try {await action();if(mounted.current){setSelected(null);setRefresh(v=>v+1);}}
    catch(cause){if(mounted.current){setError(asIpcError(cause).message);setBusy(false);}}
    finally {lock.current=false;}
  };
  const change=(patch:Partial<NativeRetouchEdit>)=>setDraft(value=>({...value,...patch}));
  const point=(index:number,value:number)=>setDraft(old=>{const region=[...old.region] as NativeRetouchEdit['region'];region[index]=value;return {...old,region};});
  const chooseTool=(tool:RetouchTool)=>{
    setSelected(null);setDraft(old=>({...old,id:'draft',tool,enabled:true,amount:0.65,texture:1,tone:0.5,warmth:tool==='makeup'?0.2:0,tint:tool==='makeup'?0.2:0}));
  };
  const preset=(polished:boolean)=>{
    const base={...draft,id:'draft',enabled:true,texture:1,source:null};
    const items:NativeRetouchEdit[]=[{...base,tool:'frequency',amount:polished?0.45:0.25,tone:0.5},{...base,tool:'micro_dodge_burn',amount:polished?0.35:0.18},{...base,tool:'mattify',amount:polished?0.35:0.2}];
    void save(()=>nativeRetouch.edit(projectId,photoId,'append',items));
  };
  return <section className="photo-studio native-retouch-workspace" aria-label="Native retouch workspace">
    <div className="studio-toolbar"><div><span className="eyebrow">NATIVE RETOUCH</span><h2>Texture, light, and detail.</h2></div><button type="button" disabled={blocked} onClick={onClose}>Back to Develop</button></div>
    <p className="lr-hint">Select a tool, click a target, and apply. Alt-click to sample a source. This view shows the full photo; your crop and perspective are applied in Develop and export.</p>
    {error&&<p role="alert">{error} <button type="button" disabled={blocked} onClick={()=>setRefresh(v=>v+1)}>Reload retouch</button></p>}
    <div className="studio-layout">
      <div>
        <div className="native-retouch-image" aria-busy={blocked} onClick={event=>{
          if(blocked||compare||!preview)return;
          const rect=event.currentTarget.getBoundingClientRect();
          const p:[number,number]=[Math.max(0,Math.min(1,(event.clientX-rect.left)/rect.width)),Math.max(0,Math.min(1,(event.clientY-rect.top)/rect.height))];
          if(event.altKey||sourceMode){change({source:p});setSourceMode(false);}else{change({region:[...p,draft.region[2],draft.region[3]]});}
        }} style={{position:'relative',cursor:sourceMode?'copy':'crosshair',lineHeight:0}}>
          {src?<img src={src} alt={compare?'Before native retouch':'Retouched photograph'} style={{width:'100%',height:'auto',display:'block'}}/>:<p>Loading retouch preview…</p>}
          {src&&overlay&&!compare&&<svg aria-hidden="true" viewBox="0 0 1000 1000" preserveAspectRatio="none" style={{position:'absolute',inset:0,width:'100%',height:'100%',pointerEvents:'none'}}>
            <ellipse cx={draft.region[0]*1000} cy={draft.region[1]*1000} rx={draft.region[2]*1000} ry={draft.region[3]*1000} fill="rgba(100,170,255,0.12)" stroke="white" strokeWidth="2" vectorEffect="non-scaling-stroke"/>
            {draft.source&&<><path d={`M ${draft.source[0]*1000-8} ${draft.source[1]*1000} h 16 M ${draft.source[0]*1000} ${draft.source[1]*1000-8} v 16`} stroke="white" strokeWidth="2" vectorEffect="non-scaling-stroke"/><line x1={draft.source[0]*1000} y1={draft.source[1]*1000} x2={draft.region[0]*1000} y2={draft.region[1]*1000} stroke="white" strokeDasharray="4 4" vectorEffect="non-scaling-stroke"/></>}
          </svg>}
        </div>
        <div className="studio-history">
          <button type="button" disabled={blocked||!before} aria-pressed={compare} onClick={()=>setCompare(v=>!v)}>{compare?'Show retouched':'Show before retouch'}</button>
          <button type="button" aria-pressed={overlay} onClick={()=>setOverlay(v=>!v)}>Selection overlay</button>
          <button type="button" disabled={blocked||!history?.canUndo} onClick={()=>void save(()=>develop.historyStep({projectId,photoId,action:'undo'}))}>Undo</button>
          <button type="button" disabled={blocked||!history?.canRedo} onClick={()=>void save(()=>develop.historyStep({projectId,photoId,action:'redo'}))}>Redo</button>
        </div>
        <p role="status">{blocked?'Rendering your retouch…':`${edits.length} saved operation${edits.length===1?'':'s'}. Originals stay untouched.`}</p>
        <details open><summary>Saved retouch operations ({edits.length})</summary>
          <ol>{edits.map((edit,index)=><li key={edit.id}>
            <button type="button" disabled={blocked} aria-pressed={selected===edit.id} onClick={()=>{setSelected(edit.id);setDraft({...edit,region:[...edit.region],source:edit.source?[...edit.source]:null});setCompare(false);}}>{index+1}. {RETOUCH_TOOLS.find(t=>t[0]===edit.tool)?.[1]} · {Math.round(edit.amount*100)}%</button>
            <label><input type="checkbox" checked={edit.enabled} disabled={blocked} onChange={event=>void save(()=>nativeRetouch.edit(projectId,photoId,'update',[{...edit,enabled:event.target.checked}]))}/>Enabled</label>
            <button type="button" disabled={blocked} aria-label={`Remove retouch ${index+1}`} onClick={()=>void save(()=>nativeRetouch.edit(projectId,photoId,'remove',[],edit.id))}>Remove</button>
          </li>)}</ol>
        </details>
      </div>
      <fieldset className="studio-adjustments lr-adjustments" disabled={blocked||!preview}>
        <legend>Retouch controls</legend>
        <label>Tool<select aria-label="Tool" value={draft.tool} onChange={event=>chooseTool(event.target.value as RetouchTool)}>{Array.from(new Set(RETOUCH_TOOLS.map(t=>t[2]))).map(group=><optgroup label={group} key={group}>{RETOUCH_TOOLS.filter(t=>t[2]===group).map(t=><option value={t[0]} key={t[0]}>{t[1]}</option>)}</optgroup>)}</select></label>
        <p className="lr-hint">{info[3]}</p>
        <label>Strength ({Math.round(draft.amount*100)}%)<input type="range" min="0" max="1" step="0.01" value={draft.amount} onChange={event=>change({amount:Number(event.target.value)})}/></label>
        <label>Feather ({Math.round(draft.feather*100)}%)<input type="range" min="0" max="1" step="0.01" value={draft.feather} onChange={event=>change({feather:Number(event.target.value)})}/></label>
        <details open><summary>Target region</summary>{['Center X (%)','Center Y (%)','Horizontal radius (%)','Vertical radius (%)'].map((label,index)=><label key={label}>{label}<input type="number" min={index>1?0.1:0} max="100" step="0.1" value={Number(((draft.region[index]??0)*100).toFixed(2))} onChange={event=>point(index,Number(event.target.value)/100)}/></label>)}</details>
        {(['heal','clone','color_match'] as RetouchTool[]).includes(draft.tool)&&<details open><summary>Source sample</summary>
          <button type="button" aria-pressed={sourceMode} onClick={()=>{setSourceMode(v=>!v);setCompare(false);}}>{sourceMode?'Cancel source picker':'Pick source on photo'}</button>
          {['Source X (%)','Source Y (%)'].map((label,index)=><label key={label}>{label}<input type="number" min="0" max="100" step="0.1" value={draft.source?Number(((draft.source[index]??0)*100).toFixed(2)):''} onChange={event=>{const p:[number,number]=draft.source?[...draft.source]:[0.5,0.5];p[index]=Number(event.target.value)/100;change({source:p});}}/></label>)}
          <button type="button" onClick={()=>change({source:null})}>Clear source</button>{needsSource&&!draft.source&&<p>Choose a source before applying this tool.</p>}
        </details>}
        {(bands||['micro_dodge_burn','eye_detail','under_eye','backdrop'].includes(draft.tool))&&<label>Frequency radius (% of short edge)<input type="number" min="0.05" max="5" step="0.05" value={Number((draft.radius*100).toFixed(2))} onChange={event=>change({radius:Number(event.target.value)/100})}/></label>}
        {bands&&<><label>Tone smoothing ({Math.round(draft.tone*100)}%)<input type="range" min="0" max="1" step="0.01" value={draft.tone} onChange={event=>change({tone:Number(event.target.value)})}/></label><label>Texture gain ({Math.round(draft.texture*100)}%)<input type="range" min="0" max="2" step="0.01" value={draft.texture} onChange={event=>change({texture:Number(event.target.value)})}/></label><p className="lr-hint">100% texture gain retains the original high-frequency band. Tone smoothing 0% with texture gain 100% is neutral.</p></>}
        {color&&<>{(['warmth','tint'] as const).map(key=><label key={key}>{key==='warmth'?'Warmth':'Tint'}<input type="range" min="-1" max="1" step="0.01" value={draft[key]} onChange={event=>change({[key]:Number(event.target.value)})}/></label>)}</>}
        <button type="button" disabled={!numericValid||(needsSource&&!draft.source)||(!selected&&edits.length>=256)} onClick={()=>{setCompare(false);void save(()=>nativeRetouch.edit(projectId,photoId,selected?'update':'append',[draft]));}}>{selected?'Update selected retouch':'Apply retouch'}</button>
        {selected&&<button type="button" onClick={()=>{setSelected(null);change({id:'draft'});}}>Start another operation</button>}
        <details><summary>Quick skin presets</summary><p className="lr-hint">Choose a skin region first. These apply frequency smoothing, tonal evening and shine reduction as one undoable edit.</p><button type="button" disabled={!numericValid||edits.length>253} onClick={()=>preset(false)}>Natural skin in selection</button><button type="button" disabled={!numericValid||edits.length>253} onClick={()=>preset(true)}>Polished skin in selection</button></details>
        <button type="button" disabled={!edits.length} onClick={()=>void save(()=>nativeRetouch.edit(projectId,photoId,'clear'))}>Clear native retouch</button>
      </fieldset>
    </div>
  </section>;
}
