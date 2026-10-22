import { useEffect, useRef, useState } from 'react';
import { asIpcError } from '../../ipc/client';
import { masksOf } from '../../ipc/localMasks';
import { MATERIAL_TOOLS, materialLab, materialsOf, type MaterialOperation, type MaterialTool } from '../../ipc/materialLab';
import type { RecipeDto } from '../../ipc/types';

export function MaterialLabPanel({ projectId, photoId, recipe, disabled, onApply }: { projectId: string; photoId: string; recipe: RecipeDto | null; disabled: boolean; onApply: (recipe: RecipeDto) => Promise<void> | void }): JSX.Element {
  const [draft, setDraft] = useState<MaterialOperation[]>(() => materialsOf(recipe));
  const [tool, setTool] = useState<MaterialTool>('directional_smooth');
  const [maskId, setMaskId] = useState('');
  const [selected, setSelected] = useState('');
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const identity = `${projectId}:${photoId}:${recipe?.recipeHash ?? ''}`;
  const current = useRef(identity); current.current = identity;
  const request = useRef(0);
  useEffect(() => { setDraft(materialsOf(recipe)); setSelected(''); setBusy(false); setMessage(null); request.current++; }, [identity]);
  useEffect(() => () => { current.current = ''; request.current++; }, []);
  const masks = masksOf(recipe);
  const destination = masks.find(m => m.id === maskId)?.id ?? masks[0]?.id ?? '';
  const operation = draft.find(op => op.id === selected) ?? draft.at(-1);
  const update = (changes: Partial<MaterialOperation>) => { if (operation) setDraft(v => v.map(op => op.id === operation.id ? { ...op, ...changes } : op)); };
  const add = () => {
    if (!destination || draft.length >= 32) return;
    let index = 1; while (draft.some(op => op.id === `material-${index}`)) index++;
    const id = `material-${index}`;
    setDraft(v => [...v, { id, tool, maskId: destination, enabled: true, amount: 0.5, radius: 0.01, angle: 0, threshold: 0.04, detail: 1, coarse: 0.25 }]); setSelected(id);
  };
  const save = async () => {
    if (!recipe || busy) return;
    const started = identity; const token = ++request.current; setBusy(true); setMessage(null);
    try { const result = await materialLab.save(projectId, photoId, recipe.recipeHash, draft); if (current.current !== started || token !== request.current) return; await onApply(result.recipe); }
    catch (cause) { if (current.current === started && token === request.current) setMessage(asIpcError(cause).message); }
    finally { if (current.current === started && token === request.current) setBusy(false); }
  };
  const number = (label: string, key: 'amount' | 'radius' | 'angle' | 'threshold' | 'detail' | 'coarse', min: number, max: number, step: number) => operation && <label>{label}<input type="number" aria-label={label} min={min} max={max} step={step} value={operation[key]} onChange={event => { const value = event.currentTarget.valueAsNumber; if (Number.isFinite(value) && value >= min && value <= max) update({ [key]: value }); }}/></label>;
  const description = MATERIAL_TOOLS.find(entry => entry[0] === operation?.tool)?.[2];
  return <section className="material-lab" aria-label="Material and texture lab"><h3>Material & texture lab</h3>
    <p>Target hair, fabric, a backdrop, or a painted repair with an existing mask. Review each result at full size.</p>
    <fieldset disabled={disabled || busy || !recipe}>
      <legend>Add a material tool</legend>
      <label>Material selection<select aria-label="Material selection" value={destination} onChange={e => setMaskId(e.target.value)}>{masks.map(mask => <option key={mask.id} value={mask.id}>{mask.name}{mask.enabled ? '' : ' (disabled)'}</option>)}</select></label>
      {!masks.length && <p>Create a mask in the Masks panel first.</p>}
      <label>Material tool<select aria-label="Material tool" value={tool} onChange={e => setTool(e.target.value as MaterialTool)}>{MATERIAL_TOOLS.map(([id, label]) => <option key={id} value={id}>{label}</option>)}</select></label>
      <p>{MATERIAL_TOOLS.find(entry => entry[0] === tool)?.[2]}</p>
      <button disabled={!destination || draft.length >= 32} onClick={add}>Add material operation</button>
    </fieldset>
    {operation && <fieldset disabled={disabled || busy || !recipe}><legend>Material stack ({draft.length}/32)</legend>
      <label>Selected material operation<select aria-label="Selected material operation" value={operation.id} onChange={e => setSelected(e.target.value)}>{draft.map((op, i) => <option key={op.id} value={op.id}>{i + 1}. {MATERIAL_TOOLS.find(t => t[0] === op.tool)?.[1]}</option>)}</select></label>
      <p>{description}</p>
      <label>Operation mask<select aria-label="Operation mask" value={operation.maskId} onChange={e => update({ maskId: e.target.value })}>{!masks.some(m => m.id === operation.maskId) && <option value={operation.maskId}>Missing mask: choose a selection</option>}{masks.map(m => <option key={m.id} value={m.id}>{m.name}</option>)}</select></label>
      <label><input type="checkbox" checked={operation.enabled} onChange={e => update({ enabled: e.target.checked })}/>Enable material operation</label>
      {number('Material strength', 'amount', 0, 1, 0.05)}
      {!['chroma_median', 'impulse_repair', 'line_scratch', 'ringing', 'illumination_plane'].includes(operation.tool) && number('Material reach (fraction of image)', 'radius', 0.001, 0.1, 0.001)}
      {['directional_smooth', 'luma_moire', 'line_scratch', 'destripe'].includes(operation.tool) && number('Material direction (degrees)', 'angle', -180, 180, 1)}
      {!['fabric_bands', 'chroma_median', 'chroma_mottle', 'luma_moire'].includes(operation.tool) && number('Material contrast threshold', 'threshold', 0.0001, 1, 0.005)}
      {['fabric_bands', 'channel_detail'].includes(operation.tool) && number('Fine detail retention', 'detail', 0, 2, 0.05)}
      {operation.tool === 'fabric_bands' && number('Coarse fabric retention', 'coarse', 0, 2, 0.05)}
      <div className="material-stack-actions"><button disabled={draft[0]?.id === operation.id} onClick={() => setDraft(v => { const next = [...v]; const i = next.findIndex(op => op.id === operation.id); if (i > 0) [next[i - 1], next[i]] = [next[i]!, next[i - 1]!]; return next; })}>Move operation earlier</button>
      <button disabled={draft.at(-1)?.id === operation.id} onClick={() => setDraft(v => { const next = [...v]; const i = next.findIndex(op => op.id === operation.id); if (i >= 0 && i < next.length - 1) [next[i], next[i + 1]] = [next[i + 1]!, next[i]!]; return next; })}>Move operation later</button>
      <button onClick={() => setDraft(v => v.filter(op => op.id !== operation.id))}>Remove material operation</button></div>
    </fieldset>}
    <button disabled={disabled || busy || !recipe || draft.some(op => !masks.some(m => m.id === op.maskId))} onClick={() => { void save(); }}>Apply material tools</button>
    <button disabled={disabled || busy || !recipe} onClick={() => { setDraft(materialsOf(recipe)); setMessage(null); }}>Discard material draft</button>
    {message && <p role="status">{message}</p>}
  </section>;
}
