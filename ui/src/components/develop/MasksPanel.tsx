import { useEffect, useRef, useState } from 'react';
import { AI_SELECTIONS, MASK_SLIDERS, describe, type CreateMask, type LocalMask, type MaskMode, type MaskParams } from '../../ipc/localMasks';

/** What the photograph does when it is clicked or dragged while the panel is open. */
export type MaskTool = { kind: 'linear' | 'radial' | 'brush'; into: string | null; mode: MaskMode; invert: boolean } | null;
export type BrushSettings = { size: number; feather: number; flow: number; erase: boolean };

export type MasksPanelProps = {
  masks: LocalMask[];
  selected: string | null;
  disabled: boolean;
  message: string | null;
  tool: MaskTool;
  brush: BrushSettings;
  overlay: boolean;
  onSelect: (id: string | null) => void;
  onCreate: (request: CreateMask) => void;
  onSave: (masks: LocalMask[], label: string) => void;
  onTool: (tool: MaskTool) => void;
  onBrush: (brush: BrushSettings) => void;
  onOverlay: (on: boolean) => void;
};

const DRAWN: ReadonlyArray<readonly [NonNullable<MaskTool>['kind'], string, string]> = [
  ['linear', 'Linear gradient', 'Drag across the photograph: full effect where you start, none where you let go.'],
  ['radial', 'Radial gradient', 'Drag from the centre outwards: full effect inside the ellipse, fading at its edge.'],
  ['brush', 'Brush', 'Paint over the photograph. Hold Alt, or switch on Erase, to take away.'],
];

/** One mask slider: commits when released, so a drag does not queue a render per step. */
function MaskSlider({ name, label, min, max, step, value, disabled, onCommit }: {
  name: string; label: string; min: number; max: number; step: number; value: number; disabled: boolean;
  onCommit: (value: number | null) => void;
}): JSX.Element {
  const [draft, setDraft] = useState(value);
  const committed = useRef(value);
  useEffect(() => { setDraft(value); committed.current = value; }, [value]);
  const commit = (next: number) => {
    const clamped = Math.min(max, Math.max(min, next));
    if (Math.abs(clamped - committed.current) > 1e-9) {
      committed.current = clamped;
      onCommit(clamped === 0 ? null : clamped);
    }
  };
  return <label className="lr-slider" title="Double-click to reset">
    <span className="lr-slider-label" onDoubleClick={() => { setDraft(0); commit(0); }}>{label}</span>
    <input type="range" min={min} max={max} step={step} value={draft} disabled={disabled} aria-label={`Mask ${label}`} name={name}
      onChange={event => setDraft(Number(event.target.value))}
      onPointerUp={() => commit(draft)} onKeyUp={() => commit(draft)} onBlur={() => commit(draft)} />
    <span className="lr-number" aria-hidden="true">{step < 1 ? draft.toFixed(2) : draft}</span>
  </label>;
}

/** Lightroom-style masking: AI selections, gradients and a brush, combined, each mask with its
 * own light, colour and detail sliders. ADR-0102. */
export function MasksPanel(props: MasksPanelProps): JSX.Element {
  const { masks, selected, disabled, message, tool, brush, overlay } = props;
  const current = masks.find(m => m.id === selected) ?? null;
  const [addWhat, setAddWhat] = useState('subject');
  const [addMode, setAddMode] = useState<MaskMode>('add');
  const [addInvert, setAddInvert] = useState(false);
  const update = (mask: LocalMask, label: string) => props.onSave(masks.map(m => m.id === mask.id ? mask : m), label);
  const setParam = (key: keyof MaskParams, value: number | null) => {
    if (!current) return;
    const params: MaskParams = { ...current.params };
    if (value === null) delete params[key]; else params[key] = value;
    update({ ...current, params }, `Mask ${String(key)}`);
  };
  const drawn = (kind: NonNullable<MaskTool>['kind'], into: string | null) =>
    props.onTool(tool?.kind === kind && tool.into === into ? null : { kind, into, mode: into ? addMode : 'add', invert: into ? addInvert : false });

  return <div className="masks-panel" aria-label="Masks">
    <p className="lr-hint">Select part of the photograph and adjust only that part. A new mask changes nothing until you move one of its sliders.</p>
    <div className="masks-create" role="group" aria-label="Create a new mask">
      {AI_SELECTIONS.slice(0, 3).map(([id, label, hint]) =>
        <button type="button" key={id} title={hint} disabled={disabled} onClick={() => props.onCreate({ what: id })}>{label}</button>)}
      <select aria-label="People and face" value="" disabled={disabled}
        onChange={event => { if (event.target.value) props.onCreate({ what: event.target.value }); }}>
        <option value="">People & face…</option>
        {AI_SELECTIONS.slice(3).map(([id, label, group]) => <option key={id} value={id}>{group}: {label}</option>)}
      </select>
      {DRAWN.map(([kind, label, hint]) =>
        <button type="button" key={kind} title={hint} disabled={disabled} aria-pressed={tool?.kind === kind && tool.into === null}
          onClick={() => drawn(kind, null)}>{label}</button>)}
    </div>
    {tool && <p className="lr-notice" role="status">{DRAWN.find(([kind]) => kind === tool.kind)?.[2]} <button type="button" onClick={() => props.onTool(null)}>Done</button></p>}
    {tool?.kind === 'brush' && <div className="masks-brush" aria-label="Brush">
      <label>Size<input type="range" min={0.005} max={0.15} step={0.005} value={brush.size} onChange={event => props.onBrush({ ...brush, size: Number(event.target.value) })} /></label>
      <label>Feather<input type="range" min={0} max={1} step={0.05} value={brush.feather} onChange={event => props.onBrush({ ...brush, feather: Number(event.target.value) })} /></label>
      <label>Flow<input type="range" min={0.1} max={1} step={0.05} value={brush.flow} onChange={event => props.onBrush({ ...brush, flow: Number(event.target.value) })} /></label>
      <label><input type="checkbox" checked={brush.erase} onChange={event => props.onBrush({ ...brush, erase: event.target.checked })} /> Erase</label>
    </div>}
    {message && <p className="lr-notice" role="status">{message}</p>}
    {masks.length > 0 && <ol className="masks-list">
      {masks.map(mask => <li key={mask.id} aria-current={mask.id === selected ? 'true' : undefined}>
        <button type="button" className="masks-name" onClick={() => props.onSelect(mask.id === selected ? null : mask.id)} aria-pressed={mask.id === selected}>
          {mask.name}{Object.keys(mask.params).length === 0 ? ' · no adjustment yet' : ''}
        </button>
        <label className="masks-toggle"><input type="checkbox" checked={mask.enabled} disabled={disabled}
          onChange={event => update({ ...mask, enabled: event.target.checked }, event.target.checked ? 'Show mask' : 'Hide mask')} /> On</label>
        <button type="button" disabled={disabled} aria-label={`Delete ${mask.name}`}
          onClick={() => { props.onSave(masks.filter(m => m.id !== mask.id), 'Delete mask'); if (mask.id === selected) props.onSelect(null); }}>Delete</button>
      </li>)}
    </ol>}
    {current && <div className="masks-current" aria-label={`Adjust ${current.name}`}>
      <label className="masks-overlay"><input type="checkbox" checked={overlay} onChange={event => props.onOverlay(event.target.checked)} /> Show mask overlay</label>
      <ul className="masks-components">
        {current.components.map((component, index) => <li key={index}>
          <span>{describe(component)}</span>
          <button type="button" disabled={disabled} aria-label={`Invert ${describe(component)}`} onClick={() => update({ ...current, components: current.components.map((c, i) => i === index ? { ...c, invert: !c.invert } : c) }, 'Invert mask component')}>Invert</button>
          {current.components.length > 1 && <button type="button" disabled={disabled}
            onClick={() => update({ ...current, components: current.components.filter((_, i) => i !== index) }, 'Remove mask component')}>Remove</button>}
        </li>)}
      </ul>
      <div className="masks-combine" role="group" aria-label="Add to this mask">
        <select aria-label="How to combine" value={addMode} disabled={disabled} onChange={event => setAddMode(event.target.value as MaskMode)}>
          <option value="add">Add</option><option value="subtract">Subtract</option><option value="intersect">Intersect with</option>
        </select>
        <select aria-label="Selection to combine" value={addWhat} disabled={disabled} onChange={event => setAddWhat(event.target.value)}>
          {AI_SELECTIONS.map(([id, label]) => <option key={id} value={id}>{label}</option>)}
        </select>
        <label><input type="checkbox" checked={addInvert} onChange={event => setAddInvert(event.target.checked)} /> Invert</label>
        <button type="button" disabled={disabled} onClick={() => props.onCreate({ what: addWhat, into: current.id, mode: addMode, invert: addInvert })}>Apply</button>
        {DRAWN.map(([kind, label]) => <button type="button" key={kind} disabled={disabled} aria-pressed={tool?.kind === kind && tool.into === current.id}
          onClick={() => drawn(kind, current.id)}>{label}</button>)}
        <button type="button" disabled={disabled} title="Only the bright parts of this mask"
          onClick={() => props.onCreate({ what: 'geometry', into: current.id, mode: 'intersect', source: { type: 'luminance', range: { low: 0, high: 16, softness: 1 } } })}>Bright parts only</button>
        <button type="button" disabled={disabled} title="Only the dark parts of this mask"
          onClick={() => props.onCreate({ what: 'geometry', into: current.id, mode: 'intersect', source: { type: 'luminance', range: { low: -16, high: -1, softness: 1 } } })}>Dark parts only</button>
      </div>
      <MaskSlider name="amount" label="Amount" min={0} max={100} step={1} value={Math.round(current.amount * 100)} disabled={disabled}
        onCommit={value => update({ ...current, amount: (value ?? 0) / 100 }, 'Mask amount')} />
      {MASK_SLIDERS.map(([key, label, min, max, step]) => <MaskSlider key={key} name={key} label={label} min={min} max={max} step={step}
        value={Number(current.params[key] ?? 0)} disabled={disabled} onCommit={value => setParam(key, value)} />)}
      <label className="masks-rename">Name<input type="text" defaultValue={current.name} key={current.id} disabled={disabled} maxLength={60}
        onBlur={event => { const name = event.target.value.trim(); if (name && name !== current.name) update({ ...current, name }, 'Rename mask'); }} /></label>
    </div>}
  </div>;
}
