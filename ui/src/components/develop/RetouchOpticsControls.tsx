import type { NativeRetouchEdit } from '../../ipc/nativeRetouch';

export function RetouchOpticsControls({ draft, onChange, sourceMode, onSourceMode }: {
  draft: NativeRetouchEdit; onChange: (patch: Partial<NativeRetouchEdit>) => void;
  sourceMode: boolean; onSourceMode: () => void;
}): JSX.Element | null {
  if (draft.tool === 'color_saturation') return <>
    {(['warmth','tint'] as const).map((key,index) => <label key={key}>{index ? 'Local vibrance' : 'Local saturation'} ({Math.round(draft[key] * 100)}%)
      <input aria-label={index ? 'Local vibrance' : 'Local saturation'} type="range" min="-100" max="100" step="1"
        value={draft[key] * 100} onChange={event => onChange({[key]:Number(event.target.value) / 100})}/>
    </label>)}
  </>;
  if (draft.tool === 'motion_blur') return <>
    <label>Motion travel (% of short edge)<input aria-label="Motion travel" type="number" min=".05" max="5" step=".05"
      value={Number((draft.radius * 100).toFixed(2))} onChange={event => onChange({radius:Number(event.target.value) / 100})} /></label>
    <label>Motion angle ({Math.round(draft.warmth * 180)}°)<input aria-label="Motion angle" type="range" min="-180" max="180" step="1"
      value={draft.warmth * 180} onChange={event => onChange({warmth:Number(event.target.value) / 180})} /></label>
  </>;
  if (draft.tool === 'zoom_blur') return <>
    <button type="button" aria-pressed={sourceMode} onClick={onSourceMode}>{sourceMode ? 'Cancel zoom center picker' : 'Pick zoom center on photo'}</button>
    <label>Zoom travel (%)<input aria-label="Zoom travel" type="number" min=".5" max="50" step=".5"
      value={Number((draft.radius * 1000).toFixed(1))} onChange={event => onChange({radius:Number(event.target.value) / 1000})} /></label>
    {['Zoom center X (%)','Zoom center Y (%)'].map((label, index) => <label key={label}>{label}
      <input type="number" min="0" max="100" step=".1" value={Number(((draft.source?.[index] ?? .5) * 100).toFixed(2))}
        onChange={event => { const source: [number,number] = [...(draft.source ?? [.5,.5])]; source[index] = Number(event.target.value) / 100; onChange({source}); }} />
    </label>)}
    <button type="button" onClick={() => onChange({source:[.5,.5]})}>Center zoom on image</button>
  </>;
  if (draft.tool === 'color_despill') return <>
    <label>Spill hue selectivity ({Math.round(draft.tone * 100)}%)<input aria-label="Spill hue selectivity" type="range" min="0" max="1" step=".01"
      value={draft.tone} onChange={event => onChange({tone:Number(event.target.value)})} /></label>
    {draft.targetColor && Math.max(...draft.targetColor) - Math.min(...draft.targetColor) < .02 &&
      <p role="alert">Choose a colored spill sample. Gray, white and black do not specify a spill hue.</p>}
  </>;
  if (draft.tool === 'highlight_compress') return <>
    <label>Compression threshold ({(draft.tone * 4).toFixed(2)} stops above middle gray)<input aria-label="Compression threshold" type="range" min="0" max="4" step=".05"
      value={draft.tone * 4} onChange={event => onChange({tone:Number(event.target.value) / 4})} /></label>
    <label>Highlight compression ({Math.round(draft.texture * 50)}%)<input aria-label="Highlight compression" type="range" min="0" max="100" step="1"
      value={draft.texture * 50} onChange={event => onChange({texture:Number(event.target.value) / 50})} /></label>
  </>;
  return null;
}
