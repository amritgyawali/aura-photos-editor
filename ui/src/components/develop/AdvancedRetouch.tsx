import { useEffect, useRef, useState } from 'react';
import { ADVANCED_STAGES, advancedRetouch, readAdvancedReport, type AdvancedPreset, type StageOutcome } from '../../ipc/advancedRetouch';
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
const REGION_LABELS = {
  eye_image_left: 'Image-left eye / under-eye', eye_image_right: 'Image-right eye / under-eye',
  nose: 'Nose', mouth: 'Mouth', chin_neck: 'Chin / adjacent neck',
} as const;

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
  const [preset, setPreset] = useState<AdvancedPreset>('professional');
  const [completed, setCompleted] = useState<{ photoId: string; inputBody: string | undefined; recipe: RecipeDto } | null>(null);
  const mounted = useRef(true);
  const activePhoto = useRef(photoId);
  activePhoto.current = photoId;
  const runVersion = useRef(0);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  useEffect(() => { setCompleted(null); }, [photoId, recipe?.body]);
  useEffect(() => { runVersion.current += 1; setLive({}); setRunning(false); }, [photoId]);
  const shown = readAdvancedReport(completed?.photoId === photoId && completed.inputBody === recipe?.body
    ? completed.recipe : recipe);
  const current = Object.entries(live).filter(([, v]) => v.state === 'running').map(([n]) => Number(n)).sort((a, b) => b - a)[0];
  const start = () => onRun(async () => {
    const version = ++runVersion.current;
    const currentRun = () => mounted.current && activePhoto.current === photoId && runVersion.current === version;
    setRunning(true); setLive({}); setCompleted(null);
    let stop: (() => void) | null = null;
    try {
      stop = await advancedRetouch.onProgress(event => {
        if (event.photoId === photoId && currentRun()) setLive(prev => ({ ...prev, [event.number]: { state: event.state, outcome: event.outcome } }));
      }).catch(() => null);
      const result = await advancedRetouch.run(projectId, photoId, undefined, preset);
      if (currentRun()) setCompleted({ photoId, inputBody: recipe?.body, recipe: result.recipe });
    } finally {
      stop?.();
      if (currentRun()) setRunning(false);
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
      {shown.revisionStatus !== 'current' && <p>{shown.revisionStatus === 'changed'
        ? 'These measurements belong to earlier settings. Run the checks again for this edit.'
        : 'This historical report has no saved settings comparison. Run the checks again to verify this edit.'}</p>}
      <p>{shown.quality.measurementStatus === 'measured'
        ? `QC measured at ${shown.quality.analysisWidth} × ${shown.quality.analysisHeight} pixels. Inspect full-resolution output before delivery.`
        : 'QC is incomplete. Missing measurements and older reports do not verify the final retouch.'}</p>
      <p>Skin texture kept: {percent(shown.quality.textureRetention)} · skin colour drift: {shown.quality.skinShift === null ? 'not measured' : shown.quality.skinShift.toFixed(3)} · clipped pixels: {percent(shown.quality.clippedOriginal)} before, {percent(shown.quality.clippedFinal)} after{shown.quality.corrected ? ' · quality control softened a step and measured again' : ''}.</p>
      {!!shown.quality.contours?.length && <details>
        <summary>Inspect checks for each facial region</summary>
        <p className="lr-hint">These landmark-based areas check for new seams and weakened edges. They do not identify every skin pixel or confirm blemish removal.</p>
        <table aria-label="Facial region contour checks">
          <thead><tr><th>Face</th><th>Area</th><th>Result</th><th>New seams</th><th>Weakened edges / source edges</th></tr></thead>
          <tbody>{shown.quality.contours.flatMap(c => c.regions.map(r => <tr key={`${c.face}-${r.region}`}>
            <td>{c.face + 1}</td><td>{REGION_LABELS[r.region]}</td>
            <td>{!r.measured ? 'Not measured' : r.review ? 'Review required' : 'Within pilot limits'}</td>
            <td>{r.newSeamPairs}</td><td>{r.weakenedEdgePairs} / {r.sourceEdgePairs}</td>
          </tr>))}</tbody>
        </table>
      </details>}
      <p className="lr-hint">Step back through “Auto advanced retouch” entries in the history to compare RAW, corrected, retouched and final. Export the master as 16-bit TIFF in Adobe RGB and web copies in sRGB from the Export step.</p>
    </div>}
  </section>;
}
