import { useEffect, useRef, useState } from 'react';
import { asIpcError } from '../../ipc/client';
import { masksOf } from '../../ipc/localMasks';
import { colorLab, colorLabOf, colorLabGroup, colorLabControlLabel, COLOR_LAB_TOOLS, COLOR_LAB_SOURCE_TOOLS, COLOR_LAB_SECOND_TOOLS, COLOR_LAB_LOCAL_TOOLS, COLOR_LAB_PIVOT_TOOLS, COLOR_LAB_SCALE_TOOLS, COLOR_LAB_TARGET_TOOLS, type ColorLabOperation, type ColorLabTool } from '../../ipc/colorLab';
import type { RecipeDto } from '../../ipc/types';

export function ColorLabPanel({ projectId, photoId, recipe, disabled, onApply }: { projectId: string; photoId: string; recipe: RecipeDto | null; disabled: boolean; onApply: (recipe: RecipeDto) => Promise<void> | void }): JSX.Element {
  const [draft, setDraft] = useState<ColorLabOperation[]>(() => colorLabOf(recipe));
  const [tool, setTool] = useState<ColorLabTool>('spatial_neutral_bridge');
  const [search, setSearch] = useState('');
  const matchingTools = COLOR_LAB_TOOLS.filter(entry => entry.join(' ').toLowerCase().includes(search.toLowerCase().trim()));
  const groups = [...new Set(matchingTools.map(([id]) => colorLabGroup(id)))];
  const [maskId, setMaskId] = useState('');
  const [selected, setSelected] = useState('');
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const identity = `${projectId}:${photoId}:${recipe?.recipeHash ?? ''}`;
  const current = useRef(identity); current.current = identity;
  const request = useRef(0);
  useEffect(() => { setDraft(colorLabOf(recipe)); setSelected(''); setBusy(false); setMessage(null); request.current++; }, [identity]);
  useEffect(() => () => { current.current = ''; request.current++; }, []);
  const masks = masksOf(recipe);
  const destination = masks.find(m => m.id === maskId)?.id ?? masks[0]?.id ?? '';
  const operation = draft.find(op => op.id === selected) ?? draft.at(-1);
  const update = (changes: Partial<ColorLabOperation>) => { if (operation) setDraft(v => v.map(op => op.id === operation.id ? { ...op, ...changes } : op)); };
  const add = () => {
    if (!destination || draft.length >= 32 || !matchingTools.length) return;
    let index = 1; while (draft.some(op => op.id === `color-lab-${index}`)) index++;
    const id = `color-lab-${index}`;
    setDraft(v => [...v, { id, tool, maskId: destination, enabled: true, amount: 0.5, strength: 1, radius: 0.025, pivot: 0.18, width: 0.2, source: [0.25, 0.5], second: [0.75, 0.5], target: [0.3, 0.2, 0.1] }]); setSelected(id);
  };
  const save = async () => {
    if (!recipe || busy) return;
    const started = identity; const token = ++request.current; setBusy(true); setMessage(null);
    try { const result = await colorLab.save(projectId, photoId, recipe.recipeHash, draft); if (current.current !== started || token !== request.current) return; await onApply(result.recipe); }
    catch (cause) { if (current.current === started && token === request.current) setMessage(asIpcError(cause).message); }
    finally { if (current.current === started && token === request.current) setBusy(false); }
  };
  const number = (label: string, value: number, min: number, max: number, step: number, change: (value: number) => void) => <label key={label}>{label}<input type="number" aria-label={label} min={min} max={max} step={step} value={value} onChange={event => { const next = event.currentTarget.valueAsNumber; if (Number.isFinite(next) && next >= min && next <= max) change(next); }}/></label>;
  const point = (key: 'source' | 'second', title: string) => operation && <div className="color-lab-point">{([0, 1] as const).map(axis => number(`${title} ${axis === 0 ? 'X' : 'Y'} (full-frame fraction)`, operation[key][axis], 0, 1, 0.01, value => { const next: [number, number] = [...operation[key]]; next[axis] = value; update({ [key]: next }); }))}</div>;
  return <section className="color-lab" aria-label="Selected colour and tone lab" aria-busy={busy}><h3>Selected colour & tone lab</h3>
    <p>Choose an existing mask, add a method, then apply to compare the saved result. Reference points use full-frame coordinates before crop. Zero strength leaves the photograph unchanged.</p>
    <fieldset disabled={disabled || busy || !recipe}><legend>Add a colour method</legend>
      <label>Colour method selection<select aria-label="Colour method selection" value={destination} onChange={event => setMaskId(event.target.value)}>{masks.map(mask => <option key={mask.id} value={mask.id}>{mask.name}{!mask.enabled || mask.amount === 0 ? ' (inactive)' : ''}</option>)}</select></label>
      {!masks.length && <p>Create a mask in the Masks panel first.</p>}
      <label>Search colour methods<input type="search" aria-label="Search colour methods" value={search} onChange={event => { const value = event.currentTarget.value; setSearch(value); const match = COLOR_LAB_TOOLS.find(entry => entry.join(' ').toLowerCase().includes(value.toLowerCase().trim())); if (match) setTool(match[0]); }}/></label>
      <p>{matchingTools.length} of {COLOR_LAB_TOOLS.length} methods</p>
      <label>Colour method<select aria-label="Colour method" value={tool} onChange={event => setTool(event.target.value as ColorLabTool)}>{!matchingTools.some(([id]) => id === tool) && <option value={tool}>{COLOR_LAB_TOOLS.find(([id]) => id === tool)?.[1]}</option>}{groups.map(group => <optgroup key={group} label={group}>{matchingTools.filter(([id]) => colorLabGroup(id) === group).map(([id, label]) => <option key={id} value={id}>{label}</option>)}</optgroup>)}</select></label>
      <p>{COLOR_LAB_TOOLS.find(entry => entry[0] === tool)?.[2]}</p>
      <button disabled={!destination || draft.length >= 32 || !matchingTools.length} onClick={add}>Add colour method</button>
    </fieldset>
    {operation && <fieldset disabled={disabled || busy || !recipe}><legend>Colour method stack ({draft.length}/32)</legend>
      <label>Selected colour operation<select aria-label="Selected colour operation" value={operation.id} onChange={event => setSelected(event.target.value)}>{draft.map((op, index) => <option key={op.id} value={op.id}>{index + 1}. {COLOR_LAB_TOOLS.find(entry => entry[0] === op.tool)?.[1]}</option>)}</select></label>
      <p>{COLOR_LAB_TOOLS.find(entry => entry[0] === operation.tool)?.[2]}</p>
      <label>Colour operation mask<select aria-label="Colour operation mask" value={operation.maskId} onChange={event => update({ maskId: event.target.value })}>{!masks.some(mask => mask.id === operation.maskId) && <option value={operation.maskId}>Missing mask: choose a selection</option>}{masks.map(mask => <option key={mask.id} value={mask.id}>{mask.name}{!mask.enabled || mask.amount === 0 ? ' (inactive)' : ''}</option>)}</select></label>
      <label><input type="checkbox" checked={operation.enabled} onChange={event => update({ enabled: event.target.checked })}/>Enable colour method</label>
      {number('Colour method strength', operation.amount, 0, 1, 0.05, amount => update({ amount }))}
      {number('Colour method intensity', operation.strength, 0, 1, 0.05, strength => update({ strength }))}
      {COLOR_LAB_LOCAL_TOOLS.includes(operation.tool) && number(colorLabControlLabel(operation.tool, 'radius', 'Colour neighborhood reach'), operation.radius, 0.001, 0.25, 0.001, radius => update({ radius }))}
      {COLOR_LAB_PIVOT_TOOLS.includes(operation.tool) && number(colorLabControlLabel(operation.tool, 'pivot', 'Linear luminance anchor'), operation.pivot, 0.001, 4, 0.01, pivot => update({ pivot }))}
      {COLOR_LAB_SCALE_TOOLS.includes(operation.tool) && number(colorLabControlLabel(operation.tool, 'width', 'Colour method scale'), operation.width, 0.001, 2, 0.01, width => update({ width }))}
      {COLOR_LAB_SOURCE_TOOLS.includes(operation.tool) && point('source', operation.tool === 'radial_log_illumination' ? 'Optical centre' : 'First reference')}
      {['gaussian_palette_transport', 'palette_simplex', 'kmeans_palette_recolor', 'fisher_monochrome', 'dichromatic_specular'].includes(operation.tool) && <p>Reference patches use selected pixels. If a required patch contains no selected pixels, this method leaves the photograph unchanged.</p>}
      {COLOR_LAB_SECOND_TOOLS.includes(operation.tool) && point('second', 'Second reference')}
      {COLOR_LAB_TARGET_TOOLS.includes(operation.tool) && <div><p>Target colour in linear Rec.2020 (0–4).</p>{([0, 1, 2] as const).map(channel => number(`Target ${['red', 'green', 'blue'][channel]} linear`, operation.target[channel], 0, 4, 0.01, value => { const target: [number, number, number] = [...operation.target]; target[channel] = value; update({ target }); }))}</div>}
      <button disabled={draft[0]?.id === operation.id} onClick={() => setDraft(v => { const next = [...v]; const index = next.findIndex(op => op.id === operation.id); if (index > 0) [next[index - 1], next[index]] = [next[index]!, next[index - 1]!]; return next; })}>Move colour method earlier</button>
      <button disabled={draft.at(-1)?.id === operation.id} onClick={() => setDraft(v => { const next = [...v]; const index = next.findIndex(op => op.id === operation.id); if (index >= 0 && index < next.length - 1) [next[index], next[index + 1]] = [next[index + 1]!, next[index]!]; return next; })}>Move colour method later</button>
      <button onClick={() => setDraft(v => v.filter(op => op.id !== operation.id))}>Remove colour method</button>
    </fieldset>}
    <button disabled={disabled || busy || !recipe || draft.some(op => !masks.some(mask => mask.id === op.maskId))} onClick={() => { void save(); }}>Apply colour methods</button>
    <button disabled={disabled || busy || !recipe} onClick={() => { setDraft(colorLabOf(recipe)); setSelected(''); setMessage(null); }}>Discard colour draft</button>
    {message && <p role="alert">{message}</p>}
  </section>;
}

