import type { RetouchSelection } from '../../ipc/nativeRetouch';

export function RetouchColorRangeControls({ selection, update, picking, onPick }: {
  selection: RetouchSelection; update: (patch: Partial<RetouchSelection>) => void;
  picking: boolean; onPick: () => void;
}) {
  const color = selection.color;
  return <>
    <label className="retouch-toggle"><input type="checkbox" checked={Boolean(color)}
      onChange={event => update({color:event.target.checked ? {sample:[.5,.5],tolerance:15,softness:10,lightness:.5} : null})}/>Limit by sampled color</label>
    {color && <>
      <p className="lr-hint">Sample a color on this photo. Matching colors refine the shape and brightness range. The sample follows earlier edits; use Preview selection mask to check coverage.</p>
      <button type="button" aria-pressed={picking} onClick={onPick}>{picking ? 'Cancel color range picker' : 'Pick selection color on photo'}</button>
      {(['Color sample X (%)','Color sample Y (%)'] as const).map((label,index) => <label key={label}>{label}
        <input type="number" min="0" max="100" step=".1" value={Number((color.sample[index]! * 100).toFixed(2))}
          onChange={event => { const sample: [number,number] = [...color.sample]; sample[index] = Number(event.target.value) / 100; update({color:{...color,sample}}); }}/>
      </label>)}
      {(['tolerance','softness'] as const).map(key => <label key={key}>{key === 'tolerance' ? 'Color tolerance' : 'Color range falloff'}
        <input type="number" min="0" max="100" step=".5" value={color[key]}
          onChange={event => update({color:{...color,[key]:Number(event.target.value)}})}/>
      </label>)}
      <label>Color lightness sensitivity ({Math.round(color.lightness * 100)}%)
        <input aria-label="Color lightness sensitivity" type="range" min="0" max="100" step="1" value={color.lightness * 100}
          onChange={event => update({color:{...color,lightness:Number(event.target.value) / 100}})}/>
      </label>
      <p className="lr-hint">Lower sensitivity allows a wider brightness difference. Color tolerance and falloff still compare both color axes; this is not object detection.</p>
    </>}
  </>;
}
