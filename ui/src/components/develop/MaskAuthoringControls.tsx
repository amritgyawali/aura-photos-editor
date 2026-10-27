import { useEffect, useState } from 'react';
import { describe, type LocalMask, type MaskMode, type MaskSource } from '../../ipc/localMasks';
import type { MaskWorkflowRequest } from '../../ipc/maskWorkflows';

function NumberField({ label, value, onChange, min = -1, max = 2, step = 0.01 }: { label: string; value: number; onChange: (value: number) => void; min?: number; max?: number; step?: number }): JSX.Element {
  return <label>{label}<input aria-label={label} type="number" value={value} min={min} max={max} step={step} onChange={event => { const n = Number(event.target.value); if (Number.isFinite(n)) onChange(Math.min(max, Math.max(min, n))); }} /></label>;
}

function SourceEditor({ source, setSource }: { source: MaskSource; setSource: (source: MaskSource) => void }): JSX.Element {
  const [strokeIndex, setStrokeIndex] = useState(0);
  const [pointIndex, setPointIndex] = useState(0);
  if (source.type === 'linear') return <>
    {(['start', 'end'] as const).flatMap(key => ([0, 1] as const).map(axis => <NumberField key={`${key}${axis}`} label={`Gradient ${key} ${axis === 0 ? 'X' : 'Y'}`} value={source[key][axis]} onChange={value => { const point: [number, number] = [...source[key]]; point[axis] = value; setSource({ ...source, [key]: point }); }} />))}
  </>;
  if (source.type === 'radial') return <>
    {(['centre', 'radii'] as const).flatMap(key => ([0, 1] as const).map(axis => <NumberField key={`${key}${axis}`} label={`Ellipse ${key} ${axis === 0 ? 'X' : 'Y'}`} min={key === 'radii' ? 0.001 : -1} value={source[key][axis]} onChange={value => { const point: [number, number] = [...source[key]]; point[axis] = value; setSource({ ...source, [key]: point }); }} />))}
    <NumberField label="Ellipse angle" min={-360} max={360} step={1} value={source.angle} onChange={angle => setSource({ ...source, angle })} />
    <NumberField label="Ellipse feather" min={0} max={1} value={source.feather} onChange={feather => setSource({ ...source, feather })} />
  </>;
  if (source.type === 'luminance') return <>
    {(['low', 'high', 'softness'] as const).map(key => <NumberField key={key} label={`Luminance ${key} in stops`} min={key === 'softness' ? 0 : -16} max={key === 'softness' ? 8 : 16} value={source.range[key]} onChange={value => setSource({ ...source, range: { ...source.range, [key]: value } })} />)}
  </>;
  if (source.type === 'brush') {
    const stroke = source.strokes[strokeIndex] ?? source.strokes[0];
    const actualStroke = stroke ? source.strokes.indexOf(stroke) : 0;
    const point = stroke?.points[pointIndex] ?? stroke?.points[0];
    const actualPoint = point ? stroke?.points.indexOf(point) : 0;
    const update = (patch: Partial<typeof stroke>) => setSource({ ...source, strokes: source.strokes.map((item, index) => index === actualStroke ? { ...item, ...patch } : item) });
    return <>
      <NumberField label="Brush component feather" min={0} max={1} value={source.feather} onChange={feather => setSource({ ...source, feather })} />
      <label>Authored stroke<select aria-label="Authored stroke" value={Math.max(0, actualStroke)} onChange={event => { setStrokeIndex(Number(event.target.value)); setPointIndex(0); }}>{source.strokes.map((_, index) => <option key={index} value={index}>Stroke {index + 1}</option>)}</select></label>
      {stroke && <>
        <NumberField label="Authored stroke radius" min={0.0005} max={0.25} step={0.0005} value={stroke.radius} onChange={radius => update({ radius })} />
        <NumberField label="Authored stroke opacity" min={0} max={1} value={stroke.opacity} onChange={opacity => update({ opacity })} />
        <label><input aria-label="Authored stroke erases" type="checkbox" checked={stroke.erase ?? false} onChange={event => update({ erase: event.target.checked })} /> Erasing stroke</label>
        <NumberField label="Authored point number" min={1} max={stroke.points.length} step={1} value={(actualPoint ?? 0) + 1} onChange={value => setPointIndex(Math.round(value) - 1)} />
        {point && ([0, 1, 2] as const).map(axis => <NumberField key={axis} label={`Authored point ${axis === 0 ? 'X' : axis === 1 ? 'Y' : 'pressure'}`} min={0} max={1} value={point[axis]} onChange={value => update({ points: stroke.points.map((p, index) => { if (index !== actualPoint) return p; const next: [number, number, number] = [...p]; next[axis] = value; return next; }) })} />)}
      </>}
    </>;
  }
  return <p className="lr-hint">This measured component has no authored control points. Its combination and position can still be edited.</p>;
}

export function MaskAuthoringControls({ masks, selected, disabled, onWorkflow }: { masks: LocalMask[]; selected: string; disabled: boolean; onWorkflow: (request: MaskWorkflowRequest) => void }): JSX.Element {
  const mask = masks.find(item => item.id === selected);
  const [index, setIndex] = useState(0);
  const component = mask?.components[index] ?? mask?.components[0];
  const actualIndex = mask && component ? mask.components.indexOf(component) : 0;
  const [source, setSource] = useState<MaskSource | null>(component?.source ?? null);
  const [mode, setMode] = useState<MaskMode>(component?.mode ?? 'add');
  const [targetIndex, setTargetIndex] = useState(0);
  const [target, setTarget] = useState('');
  useEffect(() => { setSource(component?.source ?? null); setMode(component?.mode ?? 'add'); }, [component]);
  const others = masks.filter(item => item.id !== selected);
  const destination = others.some(item => item.id === target) ? target : others[0]?.id ?? '';
  if (!mask || !component || !source) return <></>;
  const geometryAction = source.type === 'linear' ? 'edit_linear' : source.type === 'radial' ? 'edit_radial' : source.type === 'brush' ? 'edit_brush' : source.type === 'luminance' ? 'edit_luminance' : null;
  const submit = (request: MaskWorkflowRequest) => onWorkflow({ ...request, componentIndex: actualIndex });
  return <details>
    <summary>Authored components</summary>
    <fieldset disabled={disabled}>
      <legend>Control points and composition</legend>
      <p className="lr-hint">Coordinates use the uncropped photo: 0 is the left or top edge, 1 the right or bottom. Apply saves one undo step.</p>
      <label>Component<select aria-label="Authored component" value={actualIndex} onChange={event => setIndex(Number(event.target.value))}>{mask.components.map((item, i) => <option key={i} value={i}>{i + 1}. {describe(item)}</option>)}</select></label>
      <SourceEditor key={`${selected}-${actualIndex}-${source.type}`} source={source} setSource={setSource} />
      {geometryAction && <button type="button" disabled={source.type === 'luminance' && source.range.low > source.range.high} onClick={() => submit({ action: geometryAction, source })}>Apply component geometry</button>}
      <label>Component combination<select aria-label="Component combination" value={mode} onChange={event => setMode(event.target.value as MaskMode)}><option value="add">Add</option><option value="subtract">Subtract</option><option value="intersect">Intersect</option></select></label>
      <button type="button" onClick={() => submit({ action: 'component_mode', mode })}>Apply combination mode</button>
      <NumberField label="Component destination position" min={1} max={mask.components.length} step={1} value={Math.min(targetIndex + 1, mask.components.length)} onChange={value => setTargetIndex(Math.round(value) - 1)} />
      <button type="button" onClick={() => submit({ action: 'reorder_component', targetIndex: Math.min(targetIndex, mask.components.length - 1) })}>Reorder component</button>
      <button type="button" disabled={masks.length >= 32} onClick={() => submit({ action: 'extract_component' })}>Extract independent component mask</button>
      <label>Copy destination<select aria-label="Component copy destination" value={destination} onChange={event => setTarget(event.target.value)}>{others.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}</select></label>
      <button type="button" disabled={!destination} onClick={() => submit({ action: 'copy_component', otherMaskId: destination })}>Copy component to destination</button>
    </fieldset>
  </details>;
}
