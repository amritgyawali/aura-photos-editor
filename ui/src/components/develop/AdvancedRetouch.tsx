import { useEffect, useRef, useState } from 'react';
import { ADVANCED_STAGES, advancedRetouch, readAdvancedReport, type AdvancedPreset, type AdvancedRetouchReport, type StageOutcome } from '../../ipc/advancedRetouch';
import type { RecipeDto } from '../../ipc/types';

type Live = Record<number, { state: 'running' | 'done'; outcome: StageOutcome | null }>;

const STATUS: Record<StageOutcome, string> = {
  applied: 'Done · saved as its own step',
  unchanged: 'Checked · nothing needed',
  not_applicable: 'Not applicable to this photo',
  protected: 'Kept your own settings',
};
const MARK: Record<StageOutcome | 'running' | 'waiting', string> = {
  applied: '✓', unchanged: '○', not_applicable: '–', protected: '🔒', running: '…', waiting: '·',
};
const percent = (v: number | null) => v === null ? 'not measured' : `${Math.round(v * 100)}%`;

/**
 * Auto advanced retouch: every stage of a professional retouch, run automatically and in order
 * (ADR-0093). Shows each stage as it runs, then what it checked and what it changed.
 */
export function AdvancedRetouch({ projectId, photoId, recipe, disabled, onRun }: {
  projectId: string; photoId: string; recipe: RecipeDto | null; disabled: boolean;
  /** Runs the pass through the workspace's own busy lock and reload. */
  onRun: (task: () => Promise<void>) => void;
}) {
  const [running, setRunning] = useState(false);
  const [live, setLive] = useState<Live>({});
  const [report, setReport] = useState<AdvancedRetouchReport | null>(null);
  const [preset, setPreset] = useState<AdvancedPreset>('professional');
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  useEffect(() => { setReport(null); setLive({}); }, [photoId]);
  const shown = report ?? readAdvancedReport(recipe);
  const current = Object.entries(live).filter(([, v]) => v.state === 'running').map(([n]) => Number(n)).sort((a, b) => b - a)[0];
  const start = () => onRun(async () => {
    setRunning(true); setLive({}); setReport(null);
    let stop: (() => void) | null = null;
    try {
      stop = await advancedRetouch.onProgress(event => {
        if (event.photoId === photoId && mounted.current) setLive(prev => ({ ...prev, [event.number]: { state: event.state, outcome: event.outcome } }));
      }).catch(() => null);
      const result = await advancedRetouch.run(projectId, photoId, undefined, preset);
      if (mounted.current) setReport(result.report);
    } finally {
      stop?.();
      if (mounted.current) setRunning(false);
    }
  });
  return <section className="advanced-retouch" aria-label="Auto advanced retouch">
    <div className="advanced-retouch-head">
      <div>
        <span className="eyebrow">AUTO ADVANCED RETOUCH</span>
        <h3>The full professional workflow, one step at a time.</h3>
        <p className="lr-hint">Eighteen steps in the order a high-end retoucher works - RAW foundation first, the look last - with none skipped. Every step is inspected and reported; each one that changes the photo is saved as its own history step, so you can go back to any point. Nobody is reshaped, moles and freckles stay, and pores are kept.</p>
      </div>
      <div className="advanced-retouch-modes" role="radiogroup" aria-label="Retouch style">
        {([['professional', 'Professional', 'The full high-end workflow; the backdrop keeps its own light.'],
          ['beauty_fashion', 'Beauty & Fashion (Evoto-style)', 'Campaign-ready in one pass: high-end skin, clothing creases softened, stray hair faded, and a plain grey studio backdrop lifted to a clean bright grey.']] as const)
          .map(([value, label, hint]) => <label key={value} title={hint}>
            <input type="radio" name={`advanced-preset-${photoId}`} value={value} checked={preset === value} disabled={disabled || running} onChange={() => setPreset(value)} /> {label}
          </label>)}
      </div>
      <button type="button" className="retouch-primary" disabled={disabled || running} onClick={start}>
        {running ? (current ? `Step ${current} of 18 · ${ADVANCED_STAGES[current - 1]?.[1] ?? ''}…` : 'Starting…') : shown ? 'Run Auto advanced retouch again' : 'Auto advanced retouch'}
      </button>
    </div>
    <ol className="advanced-retouch-steps" aria-label="Advanced retouch steps">
      {ADVANCED_STAGES.map(([id, title, hint], index) => {
        const number = index + 1;
        const stage = running ? undefined : shown?.stages.find(s => s.stage === id);
        const progress = live[number];
        const state = progress?.state === 'running' ? 'running' : progress?.outcome ?? stage?.outcome ?? 'waiting';
        const status = state === 'running' ? 'Working…' : state === 'waiting' ? (running ? 'Waiting' : '') : STATUS[state];
        return <li key={id} data-state={state} aria-current={state === 'running' ? 'step' : undefined}>
          <span className="advanced-step-mark" aria-hidden="true">{MARK[state]}</span>
          <div>
            <p><strong>{number}. {title}</strong>{status && <span className="advanced-step-status"> · {status}</span>}
              {stage && stage.operations > 0 && <span className="advanced-step-status"> · {stage.operations} editable operation{stage.operations === 1 ? '' : 's'}</span>}</p>
            {stage && (stage.changes.length > 0 || stage.checks.length > 0) ? <details>
              <summary>{stage.changes[0] ?? stage.checks[0]}</summary>
              {stage.changes.length > 0 && <><h4>Changed</h4><ul>{stage.changes.map(c => <li key={c}>{c}</li>)}</ul></>}
              {stage.checks.length > 0 && <><h4>Checked</h4><ul>{stage.checks.map(c => <li key={c}>{c}</li>)}</ul></>}
            </details> : <p className="lr-hint">{hint}</p>}
          </div>
        </li>;
      })}
    </ol>
    {shown && !running && <div className="advanced-retouch-quality" role="status">
      <p>{shown.summary}</p>
      <p>Skin texture kept: {percent(shown.quality.textureRetention)} · skin colour drift: {shown.quality.skinShift === null ? 'not measured' : shown.quality.skinShift.toFixed(3)} · clipped pixels: {percent(shown.quality.clippedOriginal)} before, {percent(shown.quality.clippedFinal)} after{shown.quality.corrected ? ' · quality control softened a step and measured again' : ''}.</p>
      <p className="lr-hint">Step back through “Auto advanced retouch” entries in the history to compare RAW, corrected, retouched and final. Export the master as 16-bit TIFF in Adobe RGB and web copies in sRGB from the Export step.</p>
    </div>}
  </section>;
}
