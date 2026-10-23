import { useEffect, useState } from 'react';
import { asIpcError } from '../../ipc/client';
import { automaticStrength, AUTOMATIC_CATEGORIES, type AutomaticFinish, type FinishStrengths } from '../../ipc/automaticStrength';

export function AutomaticFinishControls({ projectId, photoId, revision, disabled, onApply }: {
  projectId: string; photoId: string; revision: string; disabled: boolean;
  onApply: (action: () => Promise<unknown>) => Promise<void>;
}): JSX.Element {
  const [report, setReport] = useState<AutomaticFinish | null>(null);
  const [draft, setDraft] = useState<FinishStrengths>({ overall: 1, categories: {} });
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [reload, setReload] = useState(0);
  useEffect(() => {
    let current = true;
    setReport(null); setError(null);
    void automaticStrength.read(projectId, photoId).then(value => {
      if (current) { setReport(value); setDraft(value.strengths); }
    }).catch(cause => { if (current) setError(asIpcError(cause).message); });
    return () => { current = false; };
  }, [projectId, photoId, revision, reload]);
  const locked = disabled || busy || !report;
  const apply = async () => {
    if (locked || !report) return;
    setBusy(true); setError(null);
    try {
      await onApply(async () => {
        const value = await automaticStrength.apply(projectId, photoId, report.recipeHash, draft).catch(cause => {
          setError(asIpcError(cause).message); throw cause;
        });
        setReport(value); setDraft(value.strengths);
      });
    } catch (cause) { setError(asIpcError(cause).message); }
    finally { setBusy(false); }
  };
  return <details className="automatic-finish-controls">
    <summary>Adjust automatic finish</summary>
    <p className="lr-hint">Fine-tune the saved automatic retouch without detecting again. 100% restores its original strength; 0% removes its effect. Each tool keeps its maximum strength.</p>
    {!report && !error && <p role="status">Reading automatic finishing steps…</p>}
    {report && !report.categories.length && <p>Run automatic retouch to make its finishing controls available.</p>}
    {report && report.categories.length > 0 && <fieldset disabled={locked}>
      <legend>Automatic retouch strengths</legend>
      <label>Overall retouch ({Math.round(draft.overall * 100)}%)
        <input type="range" aria-label="Overall automatic retouch" min="0" max="200" step="1" value={Math.round(draft.overall * 100)}
          onChange={event => setDraft(value => ({ ...value, overall: Number(event.target.value) / 100 }))}/>
      </label>
      {report.categories.map(({ category, operations }) => <label key={category}>
        {AUTOMATIC_CATEGORIES[category]} ({Math.round((draft.categories[category] ?? 1) * 100)}%) · {operations} steps
        <input type="range" aria-label={AUTOMATIC_CATEGORIES[category]} min="0" max="200" step="1" value={Math.round((draft.categories[category] ?? 1) * 100)}
          onChange={event => setDraft(value => ({ ...value, categories: { ...value.categories, [category]: Number(event.target.value) / 100 } }))}/>
      </label>)}
      <button type="button" onClick={() => void apply()}>Apply automatic strengths</button>
      <button type="button" onClick={() => setDraft({ overall: 1, categories: {} })}>Restore original strengths</button>
      <button type="button" onClick={() => setDraft(report.strengths)}>Discard strength changes</button>
      {report.newOperations > 0 && <p>New automatic steps will use these strengths when you apply.</p>}
    </fieldset>}
    {error && <><p role="alert">{error}</p><button type="button" disabled={disabled || busy} onClick={() => setReload(value => value + 1)}>Reload finishing controls</button></>}
  </details>;
}
