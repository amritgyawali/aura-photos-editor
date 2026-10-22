import { useEffect,useRef,useState } from 'react';
import { asIpcError } from '../../ipc/client';
import { masksOf } from '../../ipc/localMasks';
import { SPATIAL_TOOLS,SPATIAL_ADDITION_CONTROLS,NATIVE_SPATIAL_TOOLS,spatialLab,spatialOf,type SpatialOperation,type SpatialTool,type SpatialControl } from '../../ipc/spatialLab';
import type { RecipeDto } from '../../ipc/types';
export function SpatialLabPanel({projectId,photoId,recipe,disabled,onApply}:{projectId:string;photoId:string;recipe:RecipeDto|null;disabled:boolean;onApply:(recipe:RecipeDto)=>void|Promise<void>}):JSX.Element{
  const [draft,setDraft]=useState<SpatialOperation[]>(()=>spatialOf(recipe));
  const [tool,setTool]=useState<SpatialTool>('anisotropic_diffusion');const[maskId,setMaskId]=useState('');const[selected,setSelected]=useState('');
  const [busy,setBusy]=useState(false);const[message,setMessage]=useState<string|null>(null);
  const identity=`${projectId}:${photoId}:${recipe?.recipeHash??''}`;const current=useRef(identity);current.current=identity;const sequence=useRef(0);
  useEffect(()=>{sequence.current++;setDraft(spatialOf(recipe));setSelected('');setMessage(null);setBusy(false);},[identity]);
  useEffect(()=>()=>{current.current='';sequence.current++;},[]);
  const masks=masksOf(recipe);const destination=masks.find(m=>m.id===maskId)?.id??masks[0]?.id??'';const op=draft.find(v=>v.id===selected)??draft.at(-1);
  const shiftLabels=op?.tool==='radial_chromatic'?['Red radial coefficient','Blue radial coefficient']:op?.tool==='perspective_plane'?['Horizontal perspective coefficient','Vertical perspective coefficient']:op?.tool==='thin_plate_warp'?['Horizontal warp displacement','Vertical warp displacement']:['Channel horizontal shift','Channel vertical shift'];
  const update=(changes:Partial<SpatialOperation>)=>{if(op)setDraft(v=>v.map(o=>o.id===op.id?{...o,...changes}:o));};
  const additionControls=op?SPATIAL_ADDITION_CONTROLS[op.tool]:undefined;
  const additionControl=(item:SpatialControl)=>{
    if(!op)return null;
    const field=item.field;
    const value=field==='shiftX'?op.shift[0]:field==='shiftY'?op.shift[1]:field==='sourceX'?op.source[0]:field==='sourceY'?op.source[1]:op[field];
    const change=(v:number)=>{if(field==='shiftX')update({shift:[v,op.shift[1]]});else if(field==='shiftY')update({shift:[op.shift[0],v]});else if(field==='sourceX')update({source:[v,op.source[1]]});else if(field==='sourceY')update({source:[op.source[0],v]});else update({[field]:v});};
    return <div key={field}>{control(item.label,value,item.min,item.max,item.step,change)}</div>;
  };
  const add=()=>{if(!destination||draft.length>=32)return;let index=1;while(draft.some(v=>v.id===`spatial-${index}`))index++;const id=`spatial-${index}`;setDraft(v=>[...v,{id,maskId:destination,tool,enabled:true,amount:0.5,radius:0.025,threshold:0.05,detail:0.5,angle:45,source:[0.25,0.5],shift:[0.005,0],path:[[-0.025,0],[0,0.01],[0.025,0]]}]);setSelected(id);};
  const save=async()=>{if(!recipe||busy)return;const started=identity;const token=++sequence.current;setBusy(true);setMessage(null);try{const result=await spatialLab.save(projectId,photoId,recipe.recipeHash,draft);if(current.current!==started||token!==sequence.current)return;await onApply(result.recipe);}catch(e){if(current.current===started&&token===sequence.current)setMessage(asIpcError(e).message);}finally{if(current.current===started&&token===sequence.current)setBusy(false);}};
  const control=(label:string,value:number,min:number,max:number,step:number,change:(v:number)=>void)=><label>{label}<input aria-label={label} type="number" value={value} min={min} max={max} step={step} onChange={e=>{const v=e.currentTarget.valueAsNumber;if(Number.isFinite(v)&&v>=min&&v<=max)change(v);}}/></label>;
  return <section className="material-lab" aria-label="Spatial repair lab" aria-busy={busy}><h3>Spatial repair lab</h3><p>Repair or shape an explicit selection. The 44 bounded local methods retain native pixel detail; geometry, donor and iterative methods use a maximum 256-pixel working grid. Review full-size exports before delivery.</p>
    <fieldset disabled={disabled||busy||!recipe}><legend>New spatial operation</legend>
      <label>Spatial selection<select aria-label="Spatial selection" value={destination} onChange={e=>setMaskId(e.target.value)}>{masks.map(m=><option key={m.id} value={m.id}>{m.name}</option>)}</select></label>
      {!masks.length&&<p>Create an authored mask first.</p>}
      <label>Spatial tool<select aria-label="Spatial tool" value={tool} onChange={e=>setTool(e.target.value as SpatialTool)}>{SPATIAL_TOOLS.map(([id,name])=><option key={id} value={id}>{name}</option>)}</select></label><p>{SPATIAL_TOOLS.find(v=>v[0]===tool)?.[2]}</p>
      <button disabled={!destination||draft.length>=32} onClick={add}>Add spatial operation</button>
    </fieldset>
    {op&&<fieldset disabled={disabled||busy||!recipe}><legend>Spatial stack ({draft.length}/32)</legend>
      <label>Selected spatial operation<select aria-label="Selected spatial operation" value={op.id} onChange={e=>setSelected(e.target.value)}>{draft.map((v,i)=><option key={v.id} value={v.id}>{i+1}. {SPATIAL_TOOLS.find(t=>t[0]===v.tool)?.[1]}</option>)}</select></label>
      <p>{SPATIAL_TOOLS.find(v=>v[0]===op.tool)?.[2]}</p>
      <p>{NATIVE_SPATIAL_TOOLS.has(op.tool)?'Native resolution: small selections retain their pixel footprint. Repair still requires selected neighbouring context.':'Working resolution: maximum 256 pixels on the long edge. Very fine detail or thin selections may be below this resolution.'}</p>
      <label>Spatial operation mask<select aria-label="Spatial operation mask" value={op.maskId} onChange={e=>update({maskId:e.target.value})}>{!masks.some(m=>m.id===op.maskId)&&<option value={op.maskId}>Missing mask</option>}{masks.map(m=><option key={m.id} value={m.id}>{m.name}</option>)}</select></label>
      <label><input type="checkbox" checked={op.enabled} onChange={e=>update({enabled:e.target.checked})}/>Enable spatial operation</label>
      {control('Spatial strength',op.amount,0,1,0.05,v=>update({amount:v}))}
      {additionControls?.map(additionControl)}
      {['aperture_bokeh','starburst','rotation_blur','domain_transform','airy_diffraction','annular_bokeh','tilt_shift_integration','zoom_integration','thin_plate_warp'].includes(op.tool)&&control('Spatial reach',op.radius,0.002,0.15,0.001,v=>update({radius:v}))}
      {additionControls===undefined&&!['vector_medoid','trimmed_mean','haar_detail','laplacian_detail','kuwahara','aperture_bokeh','rotation_blur','trajectory_blur','harmonic_fill','poisson_donor','channel_register','edge_directed_smooth','texture_quilt','richardson_lucy','opening_reconstruct','closing_reconstruct','toggle_mapping','contraharmonic_mean','geometric_mean','local_plane_residual','side_window_mean','median_of_means','airy_diffraction','annular_bokeh','zoom_integration','radial_chromatic','perspective_plane','thin_plate_warp'].includes(op.tool)&&control('Spatial contrast threshold',op.threshold,0.0001,1,0.005,v=>update({threshold:v}))}
      {['haar_detail','laplacian_detail','starburst','morphology_clean','local_rank_contrast','shock_sharpen','contraharmonic_mean','local_plane_residual','bilateral_sharpen'].includes(op.tool)&&control('Spatial detail gain',op.detail,-1,2,0.05,v=>update({detail:v}))}
      {['rotation_blur','starburst','tilt_shift_integration'].includes(op.tool)&&control('Spatial angle',op.angle,-180,180,1,v=>update({angle:v}))}
      {['rotation_blur','poisson_donor','donor_bilateral','texture_quilt','tilt_shift_integration','zoom_integration','radial_chromatic','perspective_plane','thin_plate_warp'].includes(op.tool)&&<>{control('Source horizontal',op.source[0],0,1,0.01,v=>update({source:[v,op.source[1]]}))}{control('Source vertical',op.source[1],0,1,0.01,v=>update({source:[op.source[0],v]}))}</>}
      {['channel_register','radial_chromatic','perspective_plane','thin_plate_warp'].includes(op.tool)&&<>{control(shiftLabels[0]!,op.shift[0],-0.1,0.1,0.001,v=>update({shift:[v,op.shift[1]]}))}{control(shiftLabels[1]!,op.shift[1],-0.1,0.1,0.001,v=>update({shift:[op.shift[0],v]}))}</>}
      {op.tool==='trajectory_blur'&&<div><p>Normalized path offsets; each entered point contributes one equal sample.</p>{op.path.map((p,i)=><div key={i}>{control(`Path ${i+1} horizontal`,p[0],-0.2,0.2,0.005,v=>update({path:op.path.map((q,j)=>i===j?[v,q[1]]:q)}))}{control(`Path ${i+1} vertical`,p[1],-0.2,0.2,0.005,v=>update({path:op.path.map((q,j)=>i===j?[q[0],v]:q)}))}</div>)}<button disabled={op.path.length>=16} onClick={()=>update({path:[...op.path,[0,0]]})}>Add path sample</button><button disabled={op.path.length<=2} onClick={()=>update({path:op.path.slice(0,-1)})}>Remove last path sample</button></div>}
      <button type="button" disabled={draft[0]?.id===op.id} onClick={()=>setDraft(v=>{const next=[...v];const index=next.findIndex(item=>item.id===op.id);if(index>0)[next[index-1],next[index]]=[next[index]!,next[index-1]!];return next;})}>Move spatial operation earlier</button>
      <button type="button" disabled={draft.at(-1)?.id===op.id} onClick={()=>setDraft(v=>{const next=[...v];const index=next.findIndex(item=>item.id===op.id);if(index>=0&&index<next.length-1)[next[index],next[index+1]]=[next[index+1]!,next[index]!];return next;})}>Move spatial operation later</button>
      <button onClick={()=>setDraft(v=>v.filter(x=>x.id!==op.id))}>Remove spatial operation</button>
    </fieldset>}
    <button disabled={disabled||busy||!recipe||draft.some(v=>!masks.some(m=>m.id===v.maskId))} onClick={()=>{void save();}}>Apply spatial tools</button><button disabled={disabled||busy||!recipe} onClick={()=>{setDraft(spatialOf(recipe));setSelected('');setMessage(null);}}>Discard spatial draft</button>{message&&<p role="alert">{message}</p>}
  </section>;
}
