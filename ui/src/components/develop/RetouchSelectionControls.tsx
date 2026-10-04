import type { NativeRetouchEdit, RetouchSelection } from '../../ipc/nativeRetouch';

export function RetouchSelectionControls({ draft, onChange }: {
  draft: NativeRetouchEdit; onChange: (patch: Partial<NativeRetouchEdit>) => void;
}) {
  const selection = draft.selection ?? {};
  const update = (patch: Partial<RetouchSelection>) => onChange({ selection: { ...selection, ...patch } });
  const gradient = selection.gradient;
  const range = selection.luminance;
  return <details open><summary>Refine selection</summary>
    <label className="retouch-toggle"><input type="checkbox" checked={selection.inverted ?? false}
      onChange={event => update({ inverted: event.target.checked })}/>Outside shape</label>
    <p className="lr-hint">Invert the ellipse, brush or gradient first; brightness limits are applied afterward.</p>
    {gradient && <>
      <p className="lr-hint">{selection.inverted?'Drag from selected to protected.':'Drag from protected to selected.'} Set endpoints below for precise placement.</p>
      {(['start','end'] as const).flatMap(point => ([0,1] as const).map(axis => <label key={`${point}-${axis}`}>
        {`${point === 'start' ? 'Gradient start' : 'Gradient end'} ${axis === 0 ? 'X' : 'Y'} (%)`}
        <input type="number" min="0" max="100" step="0.1" value={Number((gradient[point][axis]*100).toFixed(2))}
          onChange={event => { const p: [number,number] = [...gradient[point]]; p[axis] = Number(event.target.value)/100;
            update({gradient:{...gradient,[point]:p}}); }}/>
      </label>))}
      <button type="button" onClick={() => update({gradient:{start:gradient.end,end:gradient.start}})}>Reverse gradient</button>
    </>}
    <label className="retouch-toggle"><input type="checkbox" checked={Boolean(range)}
      onChange={event => update({luminance:event.target.checked?{low:-2,high:2,softness:.5}:null})}/>Limit by brightness</label>
    {range && <>
      <p className="lr-hint">Stops from middle gray after earlier edits. Negative values select darker tones; positive values select brighter tones.</p>
      <div className="retouch-range-presets">
        <button type="button" onClick={() => update({luminance:{low:-16,high:-1,softness:1}})}>Shadows</button>
        <button type="button" onClick={() => update({luminance:{low:-2,high:2,softness:.5}})}>Midtones</button>
        <button type="button" onClick={() => update({luminance:{low:1,high:16,softness:1}})}>Highlights</button>
      </div>
      {(['low','high','softness'] as const).map(key => <label key={key}>
        {{low:'Dark limit (EV)',high:'Bright limit (EV)',softness:'Range falloff (EV)'}[key]}
        <input type="number" min={key==='softness'?0:-16} max={key==='softness'?4:16} step="0.1" value={range[key]}
          onChange={event => update({luminance:{...range,[key]:Number(event.target.value)}})}/>
      </label>)}
      {range.low > range.high && <p>Dark limit must be at or below Bright limit.</p>}
    </>}
  </details>;
}
