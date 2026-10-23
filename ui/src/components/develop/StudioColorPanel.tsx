import { useEffect, useRef, useState } from 'react';
import { asIpcError } from '../../ipc/client';
import { masksOf } from '../../ipc/localMasks';
import { CMYK_NEUTRAL, HSL_BANDS, LEVEL_NEUTRAL, SELECTIVE_RANGES, colorOf, neutralLocalColor, studioColor, type LevelChannel, type LocalColor, type StudioColor } from '../../ipc/studioColor';
import type { RecipeDto } from '../../ipc/types';

export type StudioColorPanelProps = { projectId: string; photoId: string; recipe: RecipeDto | null; disabled: boolean; onApply: (recipe: RecipeDto) => Promise<void> | void };
const LEVEL_FIELDS: ReadonlyArray<readonly [keyof LevelChannel, string]> = [['inputBlack', 'Input black point'], ['inputWhite', 'Input white point'], ['gamma', 'Midtone gamma'], ['outputBlack', 'Output black point'], ['outputWhite', 'Output white point']];
function NumberControl({ label, value, min, max, step = 1, onChange }: { label: string; value: number; min: number; max: number; step?: number; onChange: (value: number) => void }): JSX.Element {
  return <label>{label}<input type="number" aria-label={label} min={min} max={max} step={step} value={value} onChange={e => { const v = e.currentTarget.valueAsNumber; if (Number.isFinite(v) && v >= min && v <= max) onChange(v); }} /></label>;
}
/** Explicit draft/apply workflow; no invoke or model work while mounting or switching panels. */
export function StudioColorPanel({ projectId, photoId, recipe, disabled, onApply }: StudioColorPanelProps): JSX.Element {
  const [draft, setDraft] = useState<StudioColor>(() => colorOf(recipe));
  const [channel, setChannel] = useState<'rgb' | 'red' | 'green' | 'blue'>('rgb');
  const [range, setRange] = useState<string>('reds');
  const [maskId, setMaskId] = useState('');
  const [band, setBand] = useState<string>('red');
  const [wheel, setWheel] = useState<'shadows' | 'midtones' | 'highlights'>('shadows');
  const [x, setX] = useState(50); const [y, setY] = useState(50);
  const [busy, setBusy] = useState(false); const [message, setMessage] = useState<string | null>(null);
  const identity = `${projectId}:${photoId}:${recipe?.recipeHash ?? ''}`; const current = useRef(identity); current.current = identity;
  useEffect(() => { setDraft(colorOf(recipe)); setMessage(null); setBusy(false); }, [identity, recipe]);
  useEffect(() => () => { current.current = ''; }, []);
  const masks = masksOf(recipe); const selected = masks.find(m => m.id === maskId)?.id ?? masks[0]?.id ?? '';
  const local = draft.local[selected] ?? neutralLocalColor();
  const updateLocal = (changes: Partial<LocalColor>) => setDraft(v => ({ ...v, local: { ...v.local, [selected]: { ...(v.local[selected] ?? neutralLocalColor()), ...changes } } }));
  const updateLevel = (key: keyof LevelChannel, value: number) => setDraft(v => ({ ...v, levels: { ...v.levels, [channel]: { ...v.levels[channel], [key]: value } } }));
  const measure = async (kind: 'auto' | 'black' | 'white' | 'hsl') => {
    if (!recipe) return; const started = identity; setBusy(true); setMessage(null);
    try { const result = await studioColor.measure(projectId, photoId, recipe.recipeHash, kind, channel, x / 100, y / 100); if (current.current !== started) return;
      if (kind === 'hsl') { if (result.hslBand) setBand(result.hslBand); }
      else setDraft(v => ({ ...v, levels: { ...v.levels, [channel]: result.color.levels[channel] } }));
      setMessage(`${result.description}. Review and apply.`);
    } catch (error) { if (current.current === started) setMessage(asIpcError(error).message); }
    finally { if (current.current === started) setBusy(false); }
  };
  const save = async () => {
    if (!recipe) return; const started = identity; setBusy(true); setMessage(null);
    try { const result = await studioColor.save(projectId, photoId, recipe.recipeHash, draft); if (current.current !== started) return; await onApply(result.recipe); }
    catch (error) { if (current.current === started) setMessage(asIpcError(error).message); }
    finally { if (current.current === started) setBusy(false); }
  };
  const cmyk = draft.selective.ranges[range] ?? CMYK_NEUTRAL;
  const hsl = local.hsl[band] ?? { h: 0, s: 0, l: 0 };
  return <section aria-label="Levels and selective colour">
    <h3>Levels & selective colour</h3>
    <fieldset disabled={disabled || busy || !recipe}>
      <legend>Input and output Levels</legend>
      <label>Levels channel<select aria-label="Levels channel" value={channel} onChange={e => setChannel(e.target.value as typeof channel)}>{['rgb', 'red', 'green', 'blue'].map(c => <option key={c} value={c}>{c.toUpperCase()}</option>)}</select></label>
      {LEVEL_FIELDS.map(([key, label]) => <NumberControl key={key} label={label} value={draft.levels[channel][key]} min={key === 'gamma' ? 0.1 : 0} max={key === 'gamma' ? 10 : 1} step={0.001} onChange={v => updateLevel(key, v)} />)}
      <label><input type="checkbox" checked={draft.levels.luminosity} onChange={e => setDraft(v => ({ ...v, levels: { ...v.levels, luminosity: e.target.checked } }))} />Preserve hue with luminosity Levels</label>
      <p>Levels use the curve domain. Measurements read the uncropped edited proxy before Levels.</p>
      <button onClick={() => { void measure('auto'); }}>Measure percentile auto Levels</button>
      <NumberControl label="Sample horizontal position (%)" value={x} min={0} max={100} onChange={setX} /><NumberControl label="Sample vertical position (%)" value={y} min={0} max={100} onChange={setY} />
      <button onClick={() => { void measure('black'); }}>Sample input black point</button><button onClick={() => { void measure('white'); }}>Sample input white point</button>
      <button onClick={() => setDraft(v => ({ ...v, levels: { ...v.levels, [channel]: { ...LEVEL_NEUTRAL } } }))}>Reset selected Levels channel</button>
    </fieldset>
    <fieldset disabled={disabled || busy || !recipe}>
      <legend>Selective CMYK colour</legend>
      <label>Selective colour range<select aria-label="Selective colour range" value={range} onChange={e => setRange(e.target.value)}>{SELECTIVE_RANGES.map(r => <option key={r}>{r}</option>)}</select></label>
      <label><input type="checkbox" checked={draft.selective.absolute} onChange={e => setDraft(v => ({ ...v, selective: { ...v.selective, absolute: e.target.checked } }))} />Absolute ink adjustment</label>
      {(['cyan', 'magenta', 'yellow', 'black'] as const).map(key => <NumberControl key={key} label={`Selective ${key}`} value={cmyk[key]} min={-100} max={100} onChange={amount => setDraft(v => ({ ...v, selective: { ...v.selective, ranges: { ...v.selective.ranges, [range]: { ...(v.selective.ranges[range] ?? CMYK_NEUTRAL), [key]: amount } } } }))} />)}
    </fieldset>
    <details><summary>Extended local colour tools</summary>
      <fieldset disabled={disabled || busy || !recipe || !selected}>
        <legend>Apply only inside an existing mask</legend>
        <label>Local colour mask<select aria-label="Local colour mask" value={selected} onChange={e => setMaskId(e.target.value)}>{masks.map(mask => <option key={mask.id} value={mask.id}>{mask.name}</option>)}</select></label>
        {!masks.length && <p>Create a mask first. These tools follow its enabled state and amount.</p>}
        <h4>Local point curve</h4>
        {local.curve.points.map(([px, py], i) => <div key={i}>
          <NumberControl label={`Local curve input ${i + 1}`} value={px} min={0} max={255} onChange={v => updateLocal({ curve: { points: local.curve.points.map((p, j) => j === i ? [v, p[1]] : p) } })} />
          <NumberControl label={`Local curve output ${i + 1}`} value={py} min={0} max={255} onChange={v => updateLocal({ curve: { points: local.curve.points.map((p, j) => j === i ? [p[0], v] : p) } })} />
          {local.curve.points.length > 2 && <button onClick={() => updateLocal({ curve: { points: local.curve.points.filter((_, j) => j !== i) } })}>Remove curve point {i + 1}</button>}
        </div>)}
        <button disabled={local.curve.points.length >= 32} onClick={() => {
          const points = local.curve.points; let gap = -1; let at = 0;
          points.slice(1).forEach((p, i) => { const previous = points[i]; if (!previous) return; const d = p[0] - previous[0]; if (d > gap) { gap = d; at = i; } });
          const a = points[at]; const b = points[at + 1];
          if (gap > 1 && a && b) updateLocal({ curve: { points: [...points.slice(0, at + 1), [Math.round((a[0] + b[0]) / 2), Math.round((a[1] + b[1]) / 2)], ...points.slice(at + 1)] } });
        }}>Add local curve point</button>
        <h4>Local eight-band HSL</h4>
        <button onClick={() => { void measure('hsl'); }}>Select HSL band from sample</button>
        <label>Local HSL band<select aria-label="Local HSL band" value={band} onChange={e => setBand(e.target.value)}>{HSL_BANDS.map(b => <option key={b}>{b}</option>)}</select></label>
        {(['h', 's', 'l'] as const).map(k => <NumberControl key={k} label={`Local HSL ${k}`} value={hsl[k]} min={-100} max={100} onChange={v => updateLocal({ hsl: { ...local.hsl, [band]: { ...hsl, [k]: v } } })} />)}
        <h4>Local three-way colour grading</h4>
        <label>Local grading wheel<select aria-label="Local grading wheel" value={wheel} onChange={e => setWheel(e.target.value as typeof wheel)}>{['shadows', 'midtones', 'highlights'].map(w => <option key={w}>{w}</option>)}</select></label>
        {(['hue', 'saturation', 'luminance'] as const).map(k => <NumberControl key={k} label={`Local grading ${k}`} value={local.grade[wheel][k]} min={k === 'luminance' ? -100 : 0} max={k === 'hue' ? 359 : 100} onChange={v => updateLocal({ grade: { ...local.grade, [wheel]: { ...local.grade[wheel], [k]: v } } })} />)}
        <NumberControl label="Local grading blending" value={local.grade.blending} min={0} max={100} onChange={v => updateLocal({ grade: { ...local.grade, blending: v } })} /><NumberControl label="Local grading balance" value={local.grade.balance} min={-100} max={100} onChange={v => updateLocal({ grade: { ...local.grade, balance: v } })} />
        {(['sharpen', 'dehaze', 'vibrance'] as const).map(k => <NumberControl key={k} label={`Local ${k}`} value={local[k]} min={k === 'sharpen' ? 0 : -100} max={k === 'sharpen' ? 150 : 100} onChange={v => updateLocal({ [k]: v })} />)}
        <button onClick={() => setDraft(v => { const remaining = { ...v.local }; delete remaining[selected]; return { ...v, local: remaining }; })}>Reset selected mask colour</button>
      </fieldset>
    </details>
    <button disabled={disabled || busy || !recipe} onClick={() => { void save(); }}>Apply colour tools</button><button disabled={disabled || busy || !recipe} onClick={() => setDraft(colorOf(recipe))}>Discard colour draft</button>
    {message && <p role="status">{message}</p>}
  </section>;
}
