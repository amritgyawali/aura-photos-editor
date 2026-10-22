import { EFFECT_COLOR, EFFECT_PARAMS, EFFECT_SHORTCUTS, EFFECT_SOURCE, isWarpTool, type EffectTool } from '../../ipc/retouchEffects';
import type { NativeRetouchEdit, RetouchTool } from '../../ipc/nativeRetouch';
import { colorHex, colorRgb } from './retouchColor';

/** The controls of one selected effect (ADR-0131), from its description in `retouchEffects`. */
export function EffectControls({ draft, onChange, onTool }: {
  draft: NativeRetouchEdit & { tool: EffectTool };
  onChange: (patch: Partial<NativeRetouchEdit>) => void;
  onTool: (tool: RetouchTool) => void;
}) {
  const tool = draft.tool;
  const colour = EFFECT_COLOR[tool];
  const source = EFFECT_SOURCE[tool];
  return <div className="retouch-effect-controls">
    {EFFECT_PARAMS[tool].map(p => p.toggle
      ? <label key={p.key} className="retouch-toggle"><input type="checkbox" checked={draft[p.key] === p.max}
          onChange={event => onChange({ [p.key]: event.target.checked ? p.max : p.min })}/>{p.label}</label>
      : <label key={p.key}>{p.label} ({p.show(draft[p.key])})
          <input type="range" aria-label={p.label} min={p.min} max={p.max} step={p.step} value={draft[p.key]}
            onChange={event => onChange({ [p.key]: Number(event.target.value) })}/>
        </label>)}
    {colour && <>
      <label>{colour.label}
        <input type="color" aria-label={colour.label} value={colorHex(draft.targetColor ?? colour.initial)}
          onChange={event => onChange({ targetColor: colorRgb(event.target.value) })}/>
      </label>
      {colour.optional && draft.targetColor && <button type="button" onClick={() => onChange({ targetColor: null })}>
        {tool === 'chroma_key' ? 'Only remove the spill' : 'Use the natural colour'}</button>}
    </>}
    {source && <p className="lr-hint">{source.label}{source.required && !draft.source ? ' — needed before applying.' : '.'}</p>}
    {isWarpTool(tool) && <p className="lr-hint">The warp tapers to nothing at the ellipse edge, so it never tears. Strength is how far it moves; Undo restores the original shape.</p>}
    <details><summary>Shortcuts</summary>
      {EFFECT_SHORTCUTS.map(s => <button type="button" key={s.label} onClick={() => { onTool(s.tool); onChange(s.patch); }}>{s.label}</button>)}
    </details>
  </div>;
}
