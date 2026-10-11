import { MAX_NATIVE_RETOUCH_EDITS, DEFAULT_SKIN, isSampledSkinTool, needsRetouchSource, validRetouchSelection, RETOUCH_TOOLS, type NativeRetouchEdit, type RetouchTool } from '../../ipc/nativeRetouch';
import { RetouchSelectionControls } from './RetouchSelectionControls';
import { RetouchPresets } from './RetouchPresets';
import { RetouchSkinSelection } from './RetouchSkinSelection';
import { colorHex, colorRgb } from './retouchColor';

export function validRetouch(draft: NativeRetouchEdit): boolean {
  return [...draft.region, draft.amount, draft.feather, draft.radius, draft.texture, draft.tone, draft.warmth, draft.tint, ...(draft.source ?? [])].every(Number.isFinite)
    && draft.region.every(v => v >= 0 && v <= 1) && draft.region[2] >= .001 && draft.region[3] >= .001
    && draft.amount >= 0 && draft.amount <= 1 && draft.feather >= 0 && draft.feather <= 1
    && draft.radius >= .0005 && draft.radius <= .05 && draft.texture >= 0 && draft.texture <= 2 && draft.tone >= 0 && draft.tone <= 1
    && Math.abs(draft.warmth) <= 1 && Math.abs(draft.tint) <= 1 && (!draft.source || draft.source.every(v => v >= 0 && v <= 1))
    && (!needsRetouchSource(draft) || Boolean(draft.source))
    && Number.isFinite(draft.sourceScale ?? 1) && (draft.sourceScale ?? 1) >= .2 && (draft.sourceScale ?? 1) <= 1
    && !(draft.tool === 'patch_heal' && (draft.sourceScale ?? 1) !== 1 && !draft.source)
    && validRetouchSelection(draft)
    && (!draft.targetColor || (draft.targetColor.length === 3 && draft.targetColor.every(v => Number.isFinite(v) && v >= 0 && v <= 1)))
    && (!['colorize', 'background_color'].includes(draft.tool) || Boolean(draft.targetColor))
    && !(draft.tool === 'reshape' && draft.selection?.inverted)
    // A blemish brush with nothing painted would save an operation that does nothing.
    && !(draft.tool === 'acne_clear' && draft.mask && draft.mask.strokes.length === 0)
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
  edits: NativeRetouchEdit[];
};

export function RetouchControls(props: Props) {
  const { draft, selected, count, disabled, onChange } = props;
  const info = RETOUCH_TOOLS.find(t => t[0] === draft.tool)!;
  const bands = ['frequency', 'wrinkle', 'fabric'].includes(draft.tool);
  const color = draft.tool === 'skin_color' || draft.tool === 'makeup';
  const sampled = isSampledSkinTool(draft.tool);
  const skin = draft.skin ?? DEFAULT_SKIN;
  const canApply = validRetouch(draft) && (selected !== null || count < MAX_NATIVE_RETOUCH_EDITS);
  return <fieldset className="studio-adjustments lr-adjustments" disabled={disabled}>
    <legend>{selected ? 'Refine saved operation' : 'Create a retouch'}</legend>
    <label>Tool<select aria-label="Tool" value={draft.tool} onChange={event => props.onTool(event.target.value as RetouchTool)}>
      {Array.from(new Set(RETOUCH_TOOLS.map(t => t[2]))).map(group => <optgroup label={group} key={group}>
        {RETOUCH_TOOLS.filter(t => t[2] === group).map(t => <option value={t[0]} key={t[0]}>{t[1]}</option>)}
      </optgroup>)}
    </select></label>
    <p className="lr-hint">{info[3]}</p>
    {draft.tool === 'reshape' && <>
      {(['warmth','tint'] as const).map((key,index) => <label key={key}>{index ? 'Local height' : 'Local width'} ({Math.round(draft[key]*25)}%)
        <input type="range" aria-label={index ? 'Local height' : 'Local width'} min="-1" max="1" step=".01" value={draft[key]} onChange={event=>onChange({[key]:Number(event.target.value)})}/>
      </label>)}
      <p className="lr-hint">Choose an ellipse around the area. The warp tapers to zero at its boundary. Undo restores the source geometry.</p>
    </>}
    {['colorize', 'background_color'].includes(draft.tool) && <label>Target color
      <input type="color" aria-label="Target color" value={colorHex(draft.targetColor ?? [0.45,0.2,0.1])} onChange={event => onChange({targetColor:colorRgb(event.target.value)})}/>
    </label>}
    {draft.tool === 'colorize' && <label>Color brightness ({(draft.warmth*4).toFixed(2)} stops)
      <input aria-label="Color brightness" type="range" min="-1" max="1" step=".01" value={draft.warmth} onChange={event=>onChange({warmth:Number(event.target.value)})}/>
    </label>}
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
    <RetouchSkinSelection draft={draft} edits={props.edits} onChange={onChange}/>
    <RetouchSelectionControls draft={draft} onChange={onChange}/>
    {draft.tool === 'reshape' && draft.selection?.inverted && <p role="status">Local proportions changes the selected ellipse. Turn off Invert selection before applying.</p>}
    <button type="button" onClick={props.onSelectAll}>Select entire photo</button>
    <details open><summary>{draft.mask || draft.selection?.gradient ? 'Clone anchor / keyboard target' : 'Target region'}</summary>
      {['Center X (%)', 'Center Y (%)', 'Horizontal radius (%)', 'Vertical radius (%)'].map((label, index) => <label key={label}>{label}
        <input type="number" min={index > 1 ? .1 : 0} max="100" step="0.1" value={Number(((draft.region[index] ?? 0) * 100).toFixed(2))}
          onChange={event => { const region = [...draft.region] as NativeRetouchEdit['region']; region[index] = Number(event.target.value) / 100; onChange({ region }); }}/>
      </label>)}
    </details>
    {(sampled || (['heal', 'patch_heal', 'clone', 'color_match', 'texture_graft'] as RetouchTool[]).includes(draft.tool)) && <details open><summary>{sampled ? 'Skin reference sample' : 'Source sample'}</summary>
      <button type="button" aria-pressed={props.sourceMode} onClick={props.onSourceMode}>{props.sourceMode ? 'Cancel source picker' : sampled ? 'Pick skin sample on photo' : 'Pick source on photo'}</button>
      {['Source X (%)', 'Source Y (%)'].map((label, index) => <label key={label}>{label}
        <input type="number" min="0" max="100" step="0.1" value={draft.source ? Number(((draft.source[index] ?? 0) * 100).toFixed(2)) : ''}
          onChange={event => { const source: [number, number] = draft.source ? [...draft.source] : [.5, .5]; source[index] = Number(event.target.value) / 100; onChange({ source }); }}/>
      </label>)}
      {draft.tool === 'patch_heal' && draft.source && <label>Source patch size ({Math.round((draft.sourceScale ?? 1) * 100)}%)
        <input aria-label="Source patch size" type="range" min=".2" max="1" step=".05" value={draft.sourceScale ?? 1}
          onChange={event => onChange({ sourceScale: Number(event.target.value) })} />
      </label>}
      <button type="button" onClick={() => onChange({ source: null, sourceScale: 1 })}>Clear source</button>
      {needsRetouchSource(draft) && !draft.source && <p>{sampled ? 'Choose a clean skin sample before applying this tool.' : draft.tool === 'patch_heal' ? 'Pick a source for painted, gradient, inverted or large patch repairs.' : 'Choose a source before applying this tool.'}</p>}
    </details>}
    {(sampled || bands || ['micro_dodge_burn', 'eye_detail', 'under_eye', 'backdrop', 'frequency_heal', 'acne_clear', 'texture_graft'].includes(draft.tool)) && <label>{draft.tool === 'texture_graft' ? 'Pore size' : draft.tool === 'frequency_heal' ? 'Smallest mark size' : draft.tool === 'acne_clear' ? 'Spot size' : 'Frequency radius'} (% of short edge)
      <input type="number" min="0.05" max="5" step="0.05" value={Number((draft.radius * 100).toFixed(2))} onChange={event => onChange({ radius: Number(event.target.value) / 100 })}/>
    </label>}
      {draft.tool === 'patch_heal' && <label className="retouch-toggle"><input type="checkbox" checked={draft.textureHeal ?? false} onChange={event => onChange({ textureHeal: event.target.checked })}/>Match local lighting with real skin texture</label>}
    {draft.tool === 'frequency_heal' && <>
      <label>Tone rebuilt under marks ({Math.round(draft.tone * 100)}%)<input type="range" min="0" max="1" step="0.01" value={draft.tone} onChange={event => onChange({ tone: Number(event.target.value) })}/></label>
      <label>Mark relief kept ({Math.round(Math.min(draft.texture, 1) * 100)}%)<input type="range" min="0" max="1" step="0.01" value={Math.min(draft.texture, 1)} onChange={event => onChange({ texture: Number(event.target.value) })}/></label>
      <label>Mark sensitivity ({Math.round((draft.sensitivity ?? .5) * 100)}%)<input type="range" min="0" max="1" step="0.01" value={draft.sensitivity ?? .5} onChange={event => onChange({ sensitivity: Number(event.target.value) })}/></label>
      <label className="retouch-toggle"><input type="checkbox" checked={draft.keepDarkMarks ?? false} onChange={event => onChange({ keepDarkMarks: event.target.checked })}/>Keep dark marks (moles, freckles)</label>
      <p className="lr-hint">Only compact marks are rebuilt, from the clean skin around each one; skin with nothing wrong with it is left exactly as it is. Ordinary pore detail under a mark stays. Bright spots that are not also red are kept. Use Preview unsaved changes to review.</p>
    </>}
    {draft.tool === 'acne_clear' && <>
      <label>Spot sensitivity ({Math.round((draft.sensitivity ?? .5) * 100)}%)<input type="range" min="0" max="1" step="0.01" value={draft.sensitivity ?? .5} onChange={event => onChange({ sensitivity: Number(event.target.value) })}/></label>
      <label>How completely spots are rebuilt ({Math.round(draft.tone * 100)}%)<input type="range" min="0" max="1" step="0.01" value={draft.tone} onChange={event => onChange({ tone: Number(event.target.value) })}/></label>
      <label>Spot relief kept ({Math.round(Math.min(draft.texture, 1) * 100)}%)<input type="range" min="0" max="1" step="0.01" value={Math.min(draft.texture, 1)} onChange={event => onChange({ texture: Number(event.target.value) })}/></label>
      <label className="retouch-toggle"><input type="checkbox" checked={draft.preserveMicrotexture ?? false} onChange={event => onChange({ preserveMicrotexture: event.target.checked })}/>Also even leftover redness</label>
      <label className="retouch-toggle"><input type="checkbox" checked={draft.keepDarkMarks ?? false} onChange={event => onChange({ keepDarkMarks: event.target.checked })}/>Keep dark marks (moles, freckles)</label>
      <p className="lr-hint">Brush over what is left. Inside your strokes every pimple, red or brown mark and small bump is measured against the clean skin around it and rebuilt from that skin; the pores stay. Raise the spot size for bigger marks.</p>
    </>}
    {draft.tool === 'texture_graft' && <>
      <label>Texture level ({Math.round(draft.texture * 100)}%)<input type="range" min="0" max="2" step="0.01" value={draft.texture} onChange={event => onChange({ texture: Number(event.target.value) })}/></label>
      <label>Limit glints ({Math.round(draft.tone * 100)}%)<input type="range" min="0" max="1" step="0.01" value={draft.tone} onChange={event => onChange({ tone: Number(event.target.value) })}/></label>
      <p className="lr-hint">100% is the level this selection’s clean skin had before any retouch step. Texture is borrowed from clean skin in the same selection (or near the source you pick) and follows the light it lands in; colour does not change and nothing is generated.</p>
    </>}
    {bands && <>
      {draft.tool === 'frequency' && <label className="retouch-toggle"><input type="checkbox" checked={draft.preserveMicrotexture ?? false} onChange={event => onChange({ preserveMicrotexture: event.target.checked })}/>Preserve fine skin texture</label>}
      <label>Tone smoothing ({Math.round(draft.tone * 100)}%)<input type="range" min="0" max="1" step="0.01" value={draft.tone} onChange={event => onChange({ tone: Number(event.target.value) })}/></label>
      <label>Texture gain ({Math.round(draft.texture * 100)}%)<input type="range" min="0" max="2" step="0.01" value={draft.texture} onChange={event => onChange({ texture: Number(event.target.value) })}/></label>
      <p className="lr-hint">{draft.preserveMicrotexture ? 'Separates fine pores from larger uneven texture. Texture gain controls the original fine detail; tone smoothing evens larger variations. No artificial texture is added.' : '100% texture retains the high-frequency band. With tone smoothing 0%, the operation is neutral.'}</p>
    </>}
    {color && (['warmth', 'tint'] as const).map(key => <label key={key}>{key === 'warmth' ? 'Warmth' : 'Tint'}
      <input type="range" min="-1" max="1" step="0.01" value={draft[key]} onChange={event => onChange({ [key]: Number(event.target.value) })}/>
    </label>)}
    <details><summary>Quick skin presets</summary><p className="lr-hint">Target skin first. Adds three operations as one undoable change.</p>
      <button type="button" disabled={Boolean(selected && props.dirty) || !validRetouch({ ...draft, tool: 'frequency' }) || count > MAX_NATIVE_RETOUCH_EDITS - 3} onClick={() => props.onPreset(false)}>Natural skin in selection</button>
      <button type="button" disabled={Boolean(selected && props.dirty) || !validRetouch({ ...draft, tool: 'frequency' }) || count > MAX_NATIVE_RETOUCH_EDITS - 3} onClick={() => props.onPreset(true)}>Polished skin in selection</button>
    </details>
    <RetouchPresets draft={draft} disabled={disabled} onApply={settings => onChange(settings)}/>
  </fieldset>;
}
