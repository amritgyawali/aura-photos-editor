import { useEffect, useRef, useState } from 'react';
import { asIpcError } from '../../ipc/client';
import { RETOUCH_CATEGORY_LABELS, retouchIntensity, type IntensitySlider, type RetouchCategory } from '../../ipc/retouchIntensity';
import './retouch-intensity.css';

const percent = (factor: number) => Math.round(factor * 100);

/**
 * Strength sliders over what the automatic retouch did (ADR-0130): one per kind of retouch,
 * moved after the pass. 100% is exactly what the pass decided; the operations themselves are
 * never rewritten, so a slider moved back is the pass's own result again.
 */
export function RetouchIntensity({ projectId, photoId, version, disabled, onRun }: {
  projectId: string; photoId: string;
  /** Changes whenever the saved recipe does, so the sliders are re-read after any edit. */
  version: string | null;
  disabled: boolean;
  /** Runs a change through the workspace's own busy lock and reload. */
  onRun: (task: () => Promise<void>) => void;
}) {
  const [sliders, setSliders] = useState<IntensitySlider[]>([]);
  const [draft, setDraft] = useState<Partial<Record<RetouchCategory, number>>>({});
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState('');
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  useEffect(() => { setMessage(''); }, [photoId]);
  useEffect(() => {
    if (!version) return;
    let active = true;
    retouchIntensity.get(projectId, photoId)
      .then(report => { if (active) { setSliders(report.sliders); setDraft({}); setError(null); } })
      .catch(cause => { if (active) setError(asIpcError(cause).message); });
    return () => { active = false; };
  }, [projectId, photoId, version]);
  if (!sliders.length) {
    return error ? <p role="alert">{error}</p> : null;
  }
  const commit = (slider: IntensitySlider) => {
    const value = draft[slider.category];
    if (value === undefined || percent(value) === percent(slider.factor)) return;
    setMessage('');
    onRun(async () => {
      try { await retouchIntensity.set(projectId, photoId, { [slider.category]: value }); }
      catch (cause) { if (mounted.current) { setDraft(d => { const next = { ...d }; delete next[slider.category]; return next; }); setError(asIpcError(cause).message); } throw cause; }
    });
  };
  const moved = sliders.some(s => percent(s.factor) !== 100);
  return <section className="retouch-intensity" aria-label="Retouch strength">
    <span className="eyebrow">RETOUCH STRENGTH</span>
    <p className="lr-hint">Turn each kind of automatic retouch up or down. 100% is exactly what Auto retouch did; 0% removes it. Operations you changed by hand keep their own strength.</p>
    {sliders.map(slider => {
      const value = draft[slider.category] ?? slider.factor;
      const label = RETOUCH_CATEGORY_LABELS[slider.category];
      const max = Math.max(100, Math.floor(slider.headroom * 100));
      return <label key={slider.category} className="retouch-intensity-row">
        <span>{label} <small>({slider.operations} {slider.operations === 1 ? 'area' : 'areas'})</small></span>
        {/* A tick at 100%, which sits at a different place on each track because each stops
            where it stops changing anything. */}
        <input type="range" aria-label={`${label} strength`} min="0" max={max} step="1" disabled={disabled}
          list={max > 100 ? `retouch-auto-${slider.category}` : undefined}
          value={percent(value)}
          onChange={event => setDraft(d => ({ ...d, [slider.category]: Number(event.target.value) / 100 }))}
          onPointerUp={() => commit(slider)} onKeyUp={() => commit(slider)} onBlur={() => commit(slider)}/>
        {max > 100 && <datalist id={`retouch-auto-${slider.category}`}><option value="100" label="Auto"/></datalist>}
        <output aria-live="off">{percent(value)}%</output>
        {max === 100 && <small className="lr-hint" title="Every area in this group is already at full strength.">max</small>}
      </label>;
    })}
    <div className="retouch-intensity-actions">
      <button type="button" disabled={disabled || !moved} onClick={() => { setMessage(''); onRun(async () => { await retouchIntensity.reset(projectId, photoId); }); }}>Reset to Auto retouch</button>
      <button type="button" disabled={disabled} onClick={() => onRun(async () => {
        const report = await retouchIntensity.sync(projectId, photoId);
        if (mounted.current) setMessage(`Applied to ${report.synced} other photo${report.synced === 1 ? '' : 's'}${report.skipped ? `; ${report.skipped} not auto-retouched yet were left alone` : ''}.`);
      })}>Apply these strengths to the collection</button>
    </div>
    {message && <p role="status">{message}</p>}
    {error && <p role="alert">{error}</p>}
  </section>;
}
