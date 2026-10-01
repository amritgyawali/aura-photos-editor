import { DEFAULT_SKIN, isSampledSkinTool, needsRetouchSource, validRetouchSelection, RETOUCH_TOOLS, type NativeRetouchEdit, type RetouchTool } from '../../ipc/nativeRetouch';
import { RetouchSelectionControls } from './RetouchSelectionControls';
import { RetouchPresets } from './RetouchPresets';

export function validRetouch(draft: NativeRetouchEdit): boolean {
  return [...draft.region, draft.amount, draft.feather, draft.radius, draft.texture, draft.tone, draft.warmth, draft.tint, ...(draft.source ?? [])].every(Number.isFinite)
    && draft.region.every(v => v >= 0 && v <= 1) && draft.region[2] >= .001 && draft.region[3] >= .001
    && draft.amount >= 0 && draft.amount <= 1 && draft.feather >= 0 && draft.feather <= 1
    && draft.radius >= .0005 && draft.radius <= .05 && draft.texture >= 0 && draft.texture <= 2 && draft.tone >= 0 && draft.tone <= 1
    && Math.abs(draft.warmth) <= 1 && Math.abs(draft.tint) <= 1 && (!draft.source || draft.source.every(v => v >= 0 && v <= 1))
    && (!needsRetouchSource(draft) || Boolean(draft.source))
    && validRetouchSelection(draft)
    && (!draft.skin || (Number.isFinite(draft.skin.tolerance) && draft.skin.tolerance >= .015 && draft.skin.tolerance <= .3
      && Number.isFinite(draft.skin.edgeProtection) && draft.skin.edgeProtection >= 0 && draft.skin.edgeProtection <= 1));
}

type Props = {
  draft: NativeRetouchEdit; selected: string | null; count: number; disabled: boolean;
  sourceMode: boolean; live: boolean; dirty: boolean;
  onChange: (patch: Partial<NativeRetouchEdit>) => void; onTool: (tool: RetouchTool) => void;
  onSourceMode: () => void; onLive: (live: boolean) => void; onApply: () => void;
  onNew: () => void; onDiscard: () => void; onPreset: (polished: boolean) => void;
  onSelectAll: () => void;
};

export function RetouchControls(props: Props) {
  const { draft, selected, count, disabled, onChange } = props;
  const info = RETOUCH_TOOLS.find(t => t[0] === draft.tool)!;
  const bands = ['frequency', 'wrinkle', 'fabric'].includes(draft.tool);
  const color = draft.tool === 'skin_color' || draft.tool === 'makeup';
  const sampled = isSampledSkinTool(draft.tool);
  const skin = draft.skin ?? DEFAULT_SKIN;
  const canApply = validRetouch(draft) && (selected !== null || count < 256);
  return <fieldset className="studio-adjustments lr-adjustments" disabled={disabled}>
    <legend>{selected ? 'Refine saved operation' : 'Create a retouch'}</legend>
    <label>Tool<select aria-label="Tool" value={draft.tool} onChange={event => props.onTool(event.target.value as RetouchTool)}>
      {Array.from(new Set(RETOUCH_TOOLS.map(t => t[2]))).map(group => <optgroup label={group} key={group}>
        {RETOUCH_TOOLS.filter(t => t[2] === group).map(t => <option value={t[0]} key={t[0]}>{t[1]}</option>)}
      </optgroup>)}
    </select></label>
    <p className="lr-hint">{info[3]}</p>
    <label>Strength ({Math.round(draft.amount * 100)}%)<input type="range" min="0" max="1" step="0.01" value={draft.amount} onChange={event => onChange({ amount: Number(event.target.value) })}/></label>
    {!draft.selection?.gradient && <label>Feather ({Math.round(draft.feather * 100)}%)<input type="range" min="0" max="1" step="0.01" value={draft.feather} onChange={event => onChange({ feather: Number(event.target.value) })}/></label>}
    <div className="retouch-apply-bar">
      <label className="retouch-toggle"><input type="checkbox" checked={props.live} onChange={event => props.onLive(event.target.checked)}/>Preview unsaved changes</label>
      <button className="retouch-primary" type="button" disabled={!canApply} onClick={props.onApply}>{selected ? 'Update selected retouch' : 'Apply retouch'}</button>
      {props.dirty && <button type="button" onClick={props.onDiscard}>Discard draft</button>}
      {selected && <button type="button" disabled={props.dirty} onClick={props.onNew}>Start another operation</button>}
    </div>
    {sampled && <details open><summary>Sampled skin range</summary>
      <p className="lr-hint">Sample a clean skin patch. Matching colors are selected inside your ellipse or brush mask. Use Preview selection mask to see exactly which pixels change.</p>
      <label className="retouch-toggle"><input type="checkbox" checked={skin.connected ?? false} onChange={event => onChange({ skin: { ...skin, connected: event.target.checked } })}/>Only skin connected to the sample (never a same-coloured background)</label>
      <button type="button" onClick={props.onSelectAll}>Use full photo selection</button>
      <label>Skin color tolerance ({Math.round(skin.tolerance * 1000) / 10}%)<input type="range" min="0.015" max="0.3" step="0.005" value={skin.tolerance}
        onChange={event => onChange({ skin: { ...skin, tolerance: Number(event.target.value) } })}/></label>
      {draft.tool !== 'skin_uniformity' && <label>Edge protection ({Math.round(skin.edgeProtection * 100)}%)<input type="range" min="0" max="1" step="0.01" value={skin.edgeProtection}
        onChange={event => onChange({ skin: { ...skin, edgeProtection: Number(event.target.value) } })}/></label>}
      <label>{draft.tool === 'skin_uniformity' ? 'Color evening' : 'Tonal evening'} ({Math.round(draft.tone * 100)}%)<input type="range" min="0" max="1" step="0.01" value={draft.tone} onChange={event => onChange({ tone: Number(event.target.value) })}/></label>
      {draft.tool === 'skin_smooth' && <label>Fine detail ({Math.round(draft.texture * 100)}%)<input type="range" min="0" max="2" step="0.01" value={draft.texture} onChange={event => onChange({ texture: Number(event.target.value) })}/><span className="lr-hint">100% retains the fine-detail band.</span></label>}
    </details>}
    <RetouchSelectionControls draft={draft} onChange={onChange}/>
    <button type="button" onClick={props.onSelectAll}>Select entire photo</button>
    <details open><summary>{draft.mask || draft.selection?.gradient ? 'Clone anchor / keyboard target' : 'Target region'}</summary>
      {['Center X (%)', 'Center Y (%)', 'Horizontal radius (%)', 'Vertical radius (%)'].map((label, index) => <label key={label}>{label}
        <input type="number" min={index > 1 ? .1 : 0} max="100" step="0.1" value={Number(((draft.region[index] ?? 0) * 100).toFixed(2))}
          onChange={event => { const region = [...draft.region] as NativeRetouchEdit['region']; region[index] = Number(event.target.value) / 100; onChange({ region }); }}/>
      </label>)}
    </details>
    {(sampled || (['heal', 'patch_heal', 'clone', 'color_match'] as RetouchTool[]).includes(draft.tool)) && <details open><summary>{sampled ? 'Skin reference sample' : 'Source sample'}</summary>
      <button type="button" aria-pressed={props.sourceMode} onClick={props.onSourceMode}>{props.sourceMode ? 'Cancel source picker' : sampled ? 'Pick skin sample on photo' : 'Pick source on photo'}</button>
      {['Source X (%)', 'Source Y (%)'].map((label, index) => <label key={label}>{label}
        <input type="number" min="0" max="100" step="0.1" value={draft.source ? Number(((draft.source[index] ?? 0) * 100).toFixed(2)) : ''}
          onChange={event => { const source: [number, number] = draft.source ? [...draft.source] : [.5, .5]; source[index] = Number(event.target.value) / 100; onChange({ source }); }}/>
      </label>)}
      <button type="button" onClick={() => onChange({ source: null })}>Clear source</button>
      {needsRetouchSource(draft) && !draft.source && <p>{sampled ? 'Choose a clean skin sample before applying this tool.' : draft.tool === 'patch_heal' ? 'Pick a source for painted, gradient, inverted or large patch repairs.' : 'Choose a source before applying this tool.'}</p>}
    </details>}
    {(sampled || bands || ['micro_dodge_burn', 'eye_detail', 'under_eye', 'backdrop'].includes(draft.tool)) && <label>Frequency radius (% of short edge)
      <input type="number" min="0.05" max="5" step="0.05" value={Number((draft.radius * 100).toFixed(2))} onChange={event => onChange({ radius: Number(event.target.value) / 100 })}/>
    </label>}
    {bands && <>
      <label>Tone smoothing ({Math.round(draft.tone * 100)}%)<input type="range" min="0" max="1" step="0.01" value={draft.tone} onChange={event => onChange({ tone: Number(event.target.value) })}/></label>
      <label>Texture gain ({Math.round(draft.texture * 100)}%)<input type="range" min="0" max="2" step="0.01" value={draft.texture} onChange={event => onChange({ texture: Number(event.target.value) })}/></label>
      <p className="lr-hint">100% texture retains the high-frequency band. With tone smoothing 0%, the operation is neutral.</p>
    </>}
    {color && (['warmth', 'tint'] as const).map(key => <label key={key}>{key === 'warmth' ? 'Warmth' : 'Tint'}
      <input type="range" min="-1" max="1" step="0.01" value={draft[key]} onChange={event => onChange({ [key]: Number(event.target.value) })}/>
    </label>)}
    <details><summary>Quick skin presets</summary><p className="lr-hint">Target skin first. Adds three operations as one undoable change.</p>
      <button type="button" disabled={Boolean(selected && props.dirty) || !validRetouch({ ...draft, tool: 'frequency' }) || count > 253} onClick={() => props.onPreset(false)}>Natural skin in selection</button>
      <button type="button" disabled={Boolean(selected && props.dirty) || !validRetouch({ ...draft, tool: 'frequency' }) || count > 253} onClick={() => props.onPreset(true)}>Polished skin in selection</button>
    </details>
    <RetouchPresets draft={draft} disabled={disabled} onApply={settings => onChange(settings)}/>
  </fieldset>;
}
