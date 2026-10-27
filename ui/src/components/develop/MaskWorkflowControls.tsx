import { useEffect, useState } from 'react';
import type { LocalMask } from '../../ipc/localMasks';
import type { MaskWorkflowAction, MaskWorkflowRequest } from '../../ipc/maskWorkflows';
import { MaskAuthoringControls } from './MaskAuthoringControls';
import { MaskLabControls } from './MaskLabControls';

export function MaskWorkflowControls({ masks, selected, disabled, onWorkflow }: {
  masks: LocalMask[]; selected: string; disabled: boolean; onWorkflow: (request: MaskWorkflowRequest) => void;
}): JSX.Element {
  const others = masks.filter(mask => mask.id !== selected);
  const [other, setOther] = useState(others[0]?.id ?? '');
  const [radius, setRadius] = useState(3);
  const [contrast, setContrast] = useState(25);
  const [area, setArea] = useState(16);
  const [offset, setOffset] = useState<[number, number]>([0, 0]);
  const [scale, setScale] = useState<[number, number]>([1, 1]);
  const [angle, setAngle] = useState(0);
  const [seed, setSeed] = useState<[number, number]>([0.5, 0.5]);
  useEffect(() => { if (!others.some(mask => mask.id === other)) setOther(others[0]?.id ?? ''); }, [masks, selected, other]);
  const action = (kind: MaskWorkflowAction, value?: number, otherMaskId?: string) => onWorkflow({ action: kind, ...(value !== undefined ? { value } : {}), ...(otherMaskId ? { otherMaskId } : {}) });
  return <details className="mask-workflows">
    <summary>Selection tools</summary>
    <p className="lr-hint">Selection refinements are saved on a grid up to 512 cells across the full uncropped photo. Existing adjustment sliders stay unchanged. Undo restores the previous selection.</p>
    <MaskAuthoringControls masks={masks} selected={selected} disabled={disabled} onWorkflow={onWorkflow} />
    <MaskLabControls masks={masks} selected={selected} disabled={disabled} onWorkflow={onWorkflow} />
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
      <button type="button" onClick={() => action('exclusive_union', undefined, other)}>Symmetric difference with saved mask</button>
    </fieldset>
    <fieldset disabled={disabled}>
      <legend>Refine selection</legend>
      <label>Radius in selection cells<input aria-label="Selection refinement radius" type="number" min={0} max={32} step={1} value={radius} onChange={event => setRadius(Math.min(32, Math.max(0, Math.round(Number(event.target.value)))))} /></label>
      <button type="button" disabled={radius === 0} onClick={() => action('feather', radius)}>Feather saved selection</button>
      <button type="button" disabled={radius === 0} onClick={() => action('expand', radius)}>Expand selection</button>
      <button type="button" disabled={radius === 0} onClick={() => action('contract', radius)}>Contract selection</button>
      <button type="button" disabled={radius === 0} onClick={() => action('border_selection', radius)}>Select boundary band</button>
      <button type="button" disabled={radius === 0 || radius > 4} onClick={() => action('median_cleanup', radius)}>Median speckle cleanup (radius up to 4)</button>
      <button type="button" onClick={() => action('keep_largest')}>Keep largest connected selection</button>
      <button type="button" onClick={() => action('convex_hull')}>Create convex hull selection</button>
      <label>Edge contrast<input aria-label="Selection edge contrast" type="range" min={0} max={100} step={1} value={contrast} onChange={event => setContrast(Number(event.target.value))} />{contrast}%</label>
      <button type="button" disabled={contrast === 0} onClick={() => action('edge_contrast', contrast)}>Refine edge contrast</button>
      <label>Maximum connected area in cells<input aria-label="Selection cleanup area" type="number" min={0} max={4096} step={1} value={area} onChange={event => setArea(Math.min(4096, Math.max(0, Math.round(Number(event.target.value)))))} /></label>
      <button type="button" disabled={area === 0} onClick={() => action('remove_islands', area)}>Remove small selection islands</button>
      <button type="button" disabled={area === 0} onClick={() => action('fill_holes', area)}>Fill small selection holes</button>
      <p className="lr-hint">Island and hole cleanup uses four connected neighbours at 50% coverage. Frame-connected background is preserved.</p>
    </fieldset>
    <fieldset disabled={disabled}>
      <legend>Seed-connected region</legend>
      {[0, 1].map(axis => <label key={axis}>Seed {axis === 0 ? 'X' : 'Y'}<input aria-label={`Selection seed ${axis === 0 ? 'X' : 'Y'}`} type="number" min={0} max={1} step={0.01} value={seed[axis]} onChange={event => { const next: [number, number] = [...seed]; next[axis] = Math.min(1, Math.max(0, Number(event.target.value))); setSeed(next); }} /></label>)}
      <button type="button" onClick={() => onWorkflow({ action: 'keep_connected', point: seed })}>Isolate region at seed</button>
      <p className="lr-hint">Keeps four-connected cells at 50% coverage. Other regions and soft cells below 50% are removed.</p>
    </fieldset>
    <fieldset disabled={disabled}>
      <legend>Place composed selection</legend>
      {[0, 1].map(axis => <label key={`offset-${axis}`}>Offset {axis === 0 ? 'X' : 'Y'}<input aria-label={`Selection offset ${axis === 0 ? 'X' : 'Y'}`} type="number" min={-1} max={1} step={0.01} value={offset[axis]} onChange={event => { const next: [number, number] = [...offset]; next[axis] = Number(event.target.value); setOffset(next); }} /></label>)}
      {[0, 1].map(axis => <label key={`scale-${axis}`}>Scale {axis === 0 ? 'X' : 'Y'}<input aria-label={`Selection scale ${axis === 0 ? 'X' : 'Y'}`} type="number" min={0.1} max={4} step={0.05} value={scale[axis]} onChange={event => { const next: [number, number] = [...scale]; next[axis] = Number(event.target.value); setScale(next); }} /></label>)}
      <label>Rotation in degrees<input aria-label="Selection rotation" type="number" min={-360} max={360} step={1} value={angle} onChange={event => setAngle(Number(event.target.value))} /></label>
      <button type="button" onClick={() => onWorkflow({ action: 'transform_selection', transform: { offset, scale, angle } })}>Apply selection placement</button>
      <p className="lr-hint">Rotates around the frame centre in normalized coordinates, then moves the selection. Content outside the frame is clipped.</p>
    </fieldset>
  </details>;
}
