import { useEffect, useRef, useState } from 'react';
import { api, asIpcError, inTauri } from '../../ipc/client';
import { REGIONAL_MATCH_LABELS, regionalColorMatch, type RegionalMatchReport, type RegionalStrengths } from '../../ipc/regionalColorMatch';

const FIELDS = [['amount', 'Amount'], ['tone', 'Tone'], ['color', 'Colour'], ['whiteBalance', 'White balance']] as const;
const strengthsOf = (report: RegionalMatchReport): RegionalStrengths =>
  Object.fromEntries(report.regions.map(region => [region.name, { ...region.controls }]));

export function RegionalColorMatchPanel({ photoId, referenceId, referenceAvailable, revision, disabled, onApply, onBusyChange }: {
  photoId: string; referenceId: string; referenceAvailable: boolean; revision?: string; disabled: boolean;
  onApply: (action: () => Promise<unknown>) => Promise<void>; onBusyChange: (busy: boolean) => void;
}): JSX.Element {
  const [report, setReport] = useState<RegionalMatchReport | null>(null);
  const [draft, setDraft] = useState<RegionalStrengths>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [reload, setReload] = useState(0);
  const mounted = useRef(true);
  const running = useRef(false);
  const job = useRef<string | null>(null);
  const generation = useRef(0);
  const stopped = useRef(false);
  const identity = `${photoId}:${referenceId}`;
  const active = useRef(identity); active.current = identity;
  useEffect(() => { mounted.current = true; return () => {
    mounted.current = false; stopped.current = true;
    if (job.current) void api.cancelJob(job.current).catch(() => undefined);
    onBusyChange(false);
  }; }, [onBusyChange]);
  useEffect(() => {
    if (!inTauri()) return;
    const token = ++generation.current;
    setReport(null); setError(null); setMessage(null);
    void regionalColorMatch.status(photoId).then(saved => {
      if (!mounted.current || token !== generation.current || running.current) return;
      if (saved && saved.photoId === photoId && (!referenceId || saved.reference.id === referenceId)) {
        setReport(saved); setDraft(strengthsOf(saved));
      }
    }).catch(cause => { if (mounted.current && token === generation.current) setError(asIpcError(cause).message); });
    return () => { ++generation.current; };
  }, [photoId, referenceId, revision, reload]);
  const run = async (action: (id: string, current: () => boolean) => Promise<void>) => {
    if (running.current || disabled || !inTauri()) return;
    running.current = true; ++generation.current; stopped.current = false;
    setBusy(true); onBusyChange(true); setError(null); setMessage(null);
    const id = crypto.randomUUID(); job.current = id;
    const current = () => mounted.current && active.current === identity && !stopped.current;
    try { await action(id, current); }
    catch (cause) { if (current()) setError(asIpcError(cause).message); }
    finally {
      job.current = null; running.current = false;
      if (mounted.current) {
        setBusy(false); onBusyChange(false);
        if (stopped.current || active.current !== identity) setReload(value => value + 1);
      }
    }
  };
  const analyse = () => void run(async (id, current) => {
    if (!referenceAvailable || !referenceId) return;
    const value = await regionalColorMatch.prepare(photoId, referenceId, id);
    if (current()) { setReport(value); setDraft(strengthsOf(value)); setMessage('Regional analysis ready. Choose strengths, then apply to inspect the saved result.'); }
  });
  const apply = () => void run(async (id, current) => {
    if (!report?.current || !report.regions.length) return;
    await onApply(async () => {
      const value = await regionalColorMatch.apply(photoId, report.sessionId, report.recipeHash, draft, id).catch(cause => {
        if (current()) setError(asIpcError(cause).message);
        throw cause;
      });
      if (current()) {
        setReport(value); setDraft(strengthsOf(value));
        setMessage(value.changed ? 'Regional strengths saved. Use Undo to compare.' : 'Regional strengths checked; the saved edit is unchanged.');
      }
    });
  });
  const locked = disabled || busy || !inTauri();
  const hasCorrection = report?.regions.some(region => region.correctionAvailable) ?? false;
  return <details className="regional-color-match"><summary>Match skin, hair, clothing and background separately</summary>
    <p className="lr-hint">Measure supported regions in this photo and the selected reference. Each region gets its own colour treatment and saved selection. Your existing local edits stay in place.</p>
    <button type="button" disabled={locked || !referenceAvailable || !referenceId} onClick={analyse}>Analyse regional colour match</button>
    {report && <>
      <p>Regional reference: {report.reference.name}.</p>
      {!report.current && <p role="status">This photo changed. Analyse again before applying regional strengths.</p>}
      {!report.regions.length && <p>No shared supported regions were found. Try a similar reference or use whole-photo matching.</p>}
      {report.regions.map(region => <fieldset key={region.name} disabled={locked || !report.current || !region.correctionAvailable}>
        <legend>{REGIONAL_MATCH_LABELS[region.name]}</legend>
        {FIELDS.map(([field, label]) => <label key={field}>{label} ({Math.round((draft[region.name]?.[field] ?? 1) * 100)}%)
          <input type="range" aria-label={`${REGIONAL_MATCH_LABELS[region.name]} match ${label.toLowerCase()}`} min="0" max="200" step="1"
            value={Math.round((draft[region.name]?.[field] ?? 1) * 100)}
            onChange={event => setDraft(value => ({ ...value, [region.name]: { ...region.controls, ...value[region.name], [field]: Number(event.target.value) / 100 } }))}/>
        </label>)}
        <p className="lr-hint">Appearance distance {region.beforeDistance.toFixed(2)} → {region.afterDistance.toFixed(2)}.</p>
        {region.reason && <p>{region.reason}</p>}
      </fieldset>)}
      {report.unavailable.length > 0 && <details><summary>Unavailable regions ({report.unavailable.length})</summary><ul>
        {report.unavailable.map(region => <li key={region.name}>{REGIONAL_MATCH_LABELS[region.name]}: {region.reason}</li>)}
      </ul></details>}
      <button type="button" disabled={locked || !report.current || !hasCorrection} onClick={apply}>Apply regional colour strengths</button>
      <button type="button" disabled={locked || !report.current || !hasCorrection} onClick={() => setDraft(Object.fromEntries(report.regions.map(region => [region.name, { ...region.controls, amount: 0 }])))}>Set all regional amounts to zero</button>
      <button type="button" disabled={locked} onClick={() => setDraft(strengthsOf(report))}>Discard regional strength changes</button>
      <p className="lr-hint">{report.fullResolutionChecked ? 'Checked on native-resolution renders.' : 'Apply to check native-resolution output.'} Regions use local tone, saturation and white balance; whole-photo curves and HSL remain separate. Multiple people share the selected region strength.</p>
    </>}
    {busy && <><p role="status">Measuring regional colour and checking selections…</p><button type="button" onClick={() => {
      stopped.current = true; setMessage('Stopping regional matching.');
      if (job.current) void api.cancelJob(job.current).catch(cause => { if (mounted.current) setError(asIpcError(cause).message); });
    }}>Stop regional colour matching</button></>}
    {message && <p role="status">{message}</p>}
    {error && <><p role="alert">{error}</p><button type="button" disabled={locked} onClick={() => setReload(value => value + 1)}>Reload regional match</button></>}
  </details>;
}
