import { useEffect, useState } from 'react';
import type { LocalMask } from '../../ipc/localMasks';
import type { MaskWorkflowAction, MaskWorkflowRequest } from '../../ipc/maskWorkflows';

export function MaskWorkflowControls({ masks, selected, disabled, onWorkflow }: {
  masks: LocalMask[]; selected: string; disabled: boolean; onWorkflow: (request: MaskWorkflowRequest) => void;
}): JSX.Element {
  const others = masks.filter(mask => mask.id !== selected);
  const [other, setOther] = useState(others[0]?.id ?? '');
  const [radius, setRadius] = useState(3);
  const [contrast, setContrast] = useState(25);
  const [area, setArea] = useState(16);
  useEffect(() => { if (!others.some(mask => mask.id === other)) setOther(others[0]?.id ?? ''); }, [masks, selected, other]);
  const action = (kind: MaskWorkflowAction, value?: number, otherMaskId?: string) => onWorkflow({ action: kind, ...(value !== undefined ? { value } : {}), ...(otherMaskId ? { otherMaskId } : {}) });
  return <details className="mask-workflows">
    <summary>Selection tools</summary>
    <p className="lr-hint">Selection refinements are saved on a grid up to 512 cells across the full uncropped photo. Existing adjustment sliders stay unchanged. Undo restores the previous selection.</p>
    <fieldset disabled={disabled}>
      <legend>Whole mask</legend>
      <button type="button" onClick={() => action('duplicate')}>Duplicate whole mask</button>
      <button type="button" onClick={() => action('invert')}>Invert whole selection</button>
      <button type="button" onClick={() => action('freeze')}>Freeze composed selection</button>
    </fieldset>
    <fieldset disabled={disabled || others.length === 0}>
      <legend>Combine saved selections</legend>
      <label>Second mask<select aria-label="Second saved mask" value={other} onChange={event => setOther(event.target.value)}>
        {others.map(mask => <option key={mask.id} value={mask.id}>{mask.name}</option>)}
      </select></label>
      <button type="button" onClick={() => action('union', undefined, other)}>Union with saved mask</button>
      <button type="button" onClick={() => action('intersection', undefined, other)}>Intersect with saved mask</button>
      <button type="button" onClick={() => action('subtraction', undefined, other)}>Subtract saved mask</button>
    </fieldset>
    <fieldset disabled={disabled}>
      <legend>Refine selection</legend>
      <label>Radius in selection cells<input aria-label="Selection refinement radius" type="number" min={0} max={32} step={1} value={radius} onChange={event => setRadius(Math.min(32, Math.max(0, Math.round(Number(event.target.value)))))} /></label>
      <button type="button" disabled={radius === 0} onClick={() => action('feather', radius)}>Feather saved selection</button>
      <button type="button" disabled={radius === 0} onClick={() => action('expand', radius)}>Expand selection</button>
      <button type="button" disabled={radius === 0} onClick={() => action('contract', radius)}>Contract selection</button>
      <label>Edge contrast<input aria-label="Selection edge contrast" type="range" min={0} max={100} step={1} value={contrast} onChange={event => setContrast(Number(event.target.value))} />{contrast}%</label>
      <button type="button" disabled={contrast === 0} onClick={() => action('edge_contrast', contrast)}>Refine edge contrast</button>
      <label>Maximum connected area in cells<input aria-label="Selection cleanup area" type="number" min={0} max={4096} step={1} value={area} onChange={event => setArea(Math.min(4096, Math.max(0, Math.round(Number(event.target.value)))))} /></label>
      <button type="button" disabled={area === 0} onClick={() => action('remove_islands', area)}>Remove small selection islands</button>
      <button type="button" disabled={area === 0} onClick={() => action('fill_holes', area)}>Fill small selection holes</button>
      <p className="lr-hint">Island and hole cleanup uses four connected neighbours at 50% coverage. Frame-connected background is preserved.</p>
    </fieldset>
  </details>;
}
