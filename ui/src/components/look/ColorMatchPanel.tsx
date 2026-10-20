import { useEffect, useRef, useState } from 'react';
import { api, asIpcError, inTauri } from '../../ipc/client';
import { colorMatch, DEFAULT_MATCH_CONTROLS, pickColorReference, type ColorMatchControls,
  type ColorMatchReport, type ColorReference } from '../../ipc/colorMatch';
import { matchColorCollection, type ColorMatchOutcome } from './matchColorCollection';

const SLIDERS = [
  ['amount', 'Match amount'], ['tone', 'Tone match'], ['color', 'Colour match'], ['whiteBalance', 'Warmth match'],
] as const;

export function ColorMatchPanel({ photoId, projectId, disabled, revision, onApply }: {
  photoId: string; projectId?: string; disabled: boolean; revision?: string;
  onApply: (action: () => Promise<unknown>) => Promise<void>;
}): JSX.Element {
  const [references, setReferences] = useState<ColorReference[]>([]);
  const [selected, setSelected] = useState('');
  const [report, setReport] = useState<ColorMatchReport | null>(null);
  const [controls, setControls] = useState<ColorMatchControls>({ ...DEFAULT_MATCH_CONTROLS });
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [outcomes, setOutcomes] = useState<ColorMatchOutcome[]>([]);
  const mounted = useRef(true);
  const running = useRef(false);
  const job = useRef<string | null>(null);
  const generation = useRef(0);
  const stopped = useRef(false);

  useEffect(() => { mounted.current = true; return () => {
    mounted.current = false;
    stopped.current = true;
    if (job.current) void api.cancelJob(job.current).catch(() => undefined);
  }; }, []);
  useEffect(() => {
    if (!inTauri()) return;
    const token = ++generation.current;
    void Promise.all([colorMatch.list(), colorMatch.status(photoId)]).then(([items, saved]) => {
      if (!mounted.current || token !== generation.current || running.current) return;
      setReferences(items); setReport(saved);
      if (saved) { setSelected(saved.reference.id); setControls(saved.controls); }
      else setSelected(previous => previous || items[0]?.id || '');
    }).catch(cause => { if (mounted.current && token === generation.current) setError(asIpcError(cause).message); });
  }, [photoId, revision]);

  const run = async (action: (id: string) => Promise<void>) => {
    if (running.current || disabled || !inTauri()) return;
    running.current = true; ++generation.current; setBusy(true); setError(null); setMessage(null);
    stopped.current = false;
    const id = crypto.randomUUID(); job.current = id;
    try { await action(id); }
    catch (cause) { if (mounted.current) setError(asIpcError(cause).message); }
    finally { job.current = null; running.current = false; if (mounted.current) {
      setBusy(false);
      if (stopped.current) setMessage('Colour matching stopped. Completed edits remain saved.');
    } }
  };
  const upload = () => run(async id => {
    const path = await pickColorReference();
    if (!path || !mounted.current) return;
    const reference = await colorMatch.import(path, id);
    if (!mounted.current || stopped.current) return;
    setReferences(items => [reference, ...items.filter(item => item.id !== reference.id)]);
    setSelected(reference.id); setReport(null);
    setMessage('Reference saved. Analyse its colour treatment against this photo.');
  });
  const analyse = () => run(async id => {
    const result = await colorMatch.prepare(photoId, selected, id);
    if (!mounted.current || stopped.current) return;
    setReport(result);
    setMessage('Analysis ready. Adjust the strengths, then apply to see the saved result.');
  });
  const apply = (next = controls) => run(async id => {
    if (!report || !report.current) return;
    await onApply(async () => {
      const result = await colorMatch.apply(photoId, report.sessionId, next, id);
      if (!mounted.current) return;
      setReport(result); setControls(result.controls);
      setMessage(result.changed ? 'Colour strengths saved. Use Undo to compare.' : 'These strengths did not change the saved edit.');
    });
  });
  const collection = () => run(async id => {
    if (!projectId || !selected) return;
    setOutcomes([]);
    await onApply(async () => {
      await matchColorCollection(projectId, selected, { ...controls }, id,
        () => stopped.current || !mounted.current,
        value => { if (mounted.current) setMessage(value); },
        result => { if (mounted.current) setOutcomes(items => [...items, result]); });
      if (!mounted.current) return;
      const saved = await colorMatch.status(photoId);
      if (mounted.current && saved) { setReport(saved); setControls(saved.controls); }
    });
  });

  const locked = disabled || busy || !inTauri();
  const ready = Boolean(report?.current && report.reference.id === selected);
  return <section className="lr-panel color-match-panel" aria-label="Reference colour matching" aria-busy={busy}>
    <h3>Reference colour matching</h3>
    <p className="lr-hint">Choose an edited JPEG, PNG or TIFF to guide the colour of a different photo. Tone and colour are matched; facial retouching stays with this photo.</p>
    <button type="button" disabled={locked} onClick={() => void upload()}>Upload colour reference</button>
    <label>Saved colour reference<select aria-label="Saved colour reference" value={selected} disabled={locked}
      onChange={event => { setSelected(event.target.value); setReport(null); setMessage(null); }}>
      <option value="">Choose a reference</option>
      {references.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}
    </select></label>
    <button type="button" disabled={locked || !selected} onClick={() => void analyse()}>Analyse reference match</button>
    <fieldset disabled={locked || !selected || Boolean(report && !report.current)}>
      <legend>Colour matching strengths</legend>
      {SLIDERS.map(([key, label]) => <label key={key}>{label} ({Math.round(controls[key] * 100)}%)
        <input type="range" aria-label={label} min="0" max="100" step="1" value={Math.round(controls[key] * 100)}
          onChange={event => setControls(value => ({ ...value, [key]: Number(event.target.value) / 100 }))}/>
      </label>)}
      <button type="button" disabled={!ready} onClick={() => void apply()}>Apply colour strengths</button>
      <button type="button" disabled={!ready} onClick={() => void apply({ ...controls, amount: 0 })}>Remove matched treatment</button>
    </fieldset>
    {projectId && <details><summary>Match this collection</summary>
      <p className="lr-hint">Apply the chosen strengths to every photo in this collection. Each photo is measured separately and checked at native resolution. Save initial corrections on new photos first.</p>
      <button type="button" disabled={locked || !selected} onClick={() => void collection()}>Match colour across this collection</button>
      {outcomes.length > 0 && <ol aria-label="Collection colour matching results">{outcomes.map((item, index) =>
        <li key={`${item.photoId}:${index}`}>{item.name}: {item.outcome}. {item.detail}</li>)}</ol>}
    </details>}
    {report && <>
      {!report.current && <p role="status">This photo changed after analysis. Analyse again to keep your newer edits.</p>}
      <p className="lr-hint">Each change starts from the edit saved at analysis. New edits require a fresh analysis. Reference images without an ICC profile are treated as sRGB.</p>
      <p>Measured regions: {report.matchedRegions.join(', ')}.</p>
      {report.matchedRegions.length < 2 && <p>Only overall colour could be compared. Similar lighting and subject matter will give a more useful reference.</p>}
      <p data-testid="colour-match-measurement">Appearance distance: {report.beforeDistance.toFixed(2)} → {report.afterDistance.toFixed(2)}.
        {' '}{report.fullResolutionChecked ? 'Checked on native-resolution renders.' : 'Analysis preview; apply to check native-resolution renders.'}
        {report.protectedFields > 0 && ` ${report.protectedFields} manual fields protected.`}</p>
    </>}
    {busy && <><p role="status">{message ?? 'Measuring colour and checking the image…'}</p>
      <button type="button" onClick={() => { stopped.current = true; if (job.current) void api.cancelJob(job.current).catch(cause => setError(asIpcError(cause).message)); }}>Stop colour matching</button></>}
    {!busy && message && <p role="status">{message}</p>}
    {error && <p role="alert">{error}</p>}
  </section>;
}
