import { useEffect, useRef, useState } from 'react';
import { api, asIpcError, inTauri } from '../../ipc/client';
import { colorMatch, DEFAULT_MATCH_CONTROLS, pickColorReference, pickColorReferences, type ColorMatchControls,
  type ColorMatchReport, type ColorReference, type ReferenceMetadata } from '../../ipc/colorMatch';
import { matchColorCollection, type ColorMatchOutcome } from './matchColorCollection';
import { FINE_MATCH_FIELDS } from '../../ipc/colorMatchDetails';
import { ColorReferenceLibrary } from './ColorReferenceLibrary';
import { RegionalColorMatchPanel } from './RegionalColorMatchPanel';
import { ReferenceLibraryManager } from './ReferenceLibraryManager';

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
  const [regionalBusy, setRegionalBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [outcomes, setOutcomes] = useState<ColorMatchOutcome[]>([]);
  const [captureName, setCaptureName] = useState('Edited photo');
  const mounted = useRef(true);
  const running = useRef(false);
  const job = useRef<string | null>(null);
  const generation = useRef(0);
  const stopped = useRef(false);
  const activePhoto = useRef(photoId);
  activePhoto.current = photoId;
  const activeReference = useRef(selected);
  activeReference.current = selected;

  useEffect(() => { mounted.current = true; return () => {
    mounted.current = false;
    stopped.current = true;
    if (job.current) void api.cancelJob(job.current).catch(() => undefined);
  }; }, []);
  useEffect(() => {
    setReport(null); setOutcomes([]); setMessage(null); setError(null);
    if (job.current) {
      stopped.current = true;
      void api.cancelJob(job.current).catch(cause => { if (mounted.current) setError(asIpcError(cause).message); });
    }
  }, [photoId]);
  useEffect(() => {
    if (!inTauri()) return;
    const token = ++generation.current;
    void Promise.all([colorMatch.list(), colorMatch.status(photoId)]).then(([items, saved]) => {
      if (!mounted.current || token !== generation.current || running.current) return;
      setReferences(items); setReport(saved);
      if (saved) { setSelected(saved.reference.id); setControls(saved.controls); }
      else {
        const reference = items.find(item => item.id === activeReference.current) ?? items[0];
        setSelected(reference?.id || ''); setControls(reference?.defaultControls ?? { ...DEFAULT_MATCH_CONTROLS });
      }
    }).catch(cause => { if (mounted.current && token === generation.current) setError(asIpcError(cause).message); });
  }, [photoId, revision]);

  const run = async (action: (id: string, current: () => boolean) => Promise<void>) => {
    if (running.current || disabled || !inTauri()) return;
    running.current = true; ++generation.current; setBusy(true); setError(null); setMessage(null);
    stopped.current = false;
    const id = crypto.randomUUID(); job.current = id;
    const current = () => mounted.current && activePhoto.current === photoId && !stopped.current;
    try { await action(id, current); }
    catch (cause) { if (current()) setError(asIpcError(cause).message); }
    finally { job.current = null; running.current = false; if (mounted.current) {
      setBusy(false);
      if (stopped.current && activePhoto.current === photoId) setMessage('Stopped. Completed changes remain saved.');
    } }
  };
  const upload = () => run(async (id, current) => {
    const path = await pickColorReference();
    if (!path || !current()) return;
    const reference = await colorMatch.import(path, id);
    if (!current()) return;
    setReferences(items => [reference, ...items.filter(item => item.id !== reference.id)]);
    setSelected(reference.id); setReport(null);
    setControls(reference.defaultControls ?? { ...DEFAULT_MATCH_CONTROLS });
    setMessage('Reference saved. Analyse its colour treatment against this photo.');
  });
  const analyse = () => run(async (id, current) => {
    const result = await colorMatch.prepare(photoId, selected, id);
    if (!current()) return;
    setReport(result);
    setMessage('Analysis ready. Adjust the strengths, then apply to see the saved result.');
  });
  const apply = (next = controls) => run(async (id, current) => {
    if (!report || !report.current) return;
    await onApply(async () => {
      const result = await colorMatch.apply(photoId, report.sessionId, next, id);
      if (!current()) return;
      setReport(result); setControls(result.controls);
      setMessage(result.changed ? 'Colour strengths saved. Use Undo to compare.' : 'These strengths did not change the saved edit.');
    });
  });
  const choose = (reference: ColorReference) => {
    setSelected(reference.id); setReport(null); setMessage(null);
    setControls(reference.defaultControls ?? { ...DEFAULT_MATCH_CONTROLS });
  };
  const manage = (reference: ColorReference, metadata: ReferenceMetadata, remove = false) => void run(async (_id, current) => {
    const updated = await colorMatch.manage(reference.id, metadata, remove ? 'remove' : 'update');
    if (!current()) return;
    setReferences(items => updated ? items.map(item => item.id === updated.id ? updated : item) : items.filter(item => item.id !== reference.id));
    if (!updated && selected === reference.id && report?.reference.id !== reference.id) { setSelected(''); setReport(null); }
    setMessage(updated ? 'Reference library updated.' : 'Reference removed from the library. Saved photo edits are kept.');
  });
  const capture = () => void run(async (id, current) => {
    if (!projectId) return;
    const reference = await colorMatch.capture(projectId, photoId, captureName, id);
    if (!current()) return;
    setReferences(items => [reference, ...items.filter(item => item.id !== reference.id)]);
    choose(reference); setMessage('Edited preview saved as a reusable colour reference.');
  });
  const uploadSeveral = () => void run(async (id, current) => {
    const paths = await pickColorReferences();
    const imported: ColorReference[] = []; const failures: string[] = [];
    for (const path of paths) {
      if (!current()) break;
      try {
        const reference = await colorMatch.import(path, id);
        if (!current()) break;
        imported.push(reference);
        setReferences(items => [reference, ...items.filter(item => item.id !== reference.id)]);
      } catch (cause) { if (current()) failures.push(asIpcError(cause).message); }
    }
    if (!current()) return;
    if (imported[0]) choose(imported[0]);
    if (paths.length) setMessage(`${imported.length} references saved; ${failures.length} could not be imported.`);
    if (failures.length) setError(failures.join(' '));
  });
  const collection = () => run(async (id, current) => {
    if (!projectId || !selected) return;
    setOutcomes([]);
    await onApply(async () => {
      await matchColorCollection(projectId, selected, { ...controls }, id,
        () => !current(),
        value => { if (current()) setMessage(value); },
        result => { if (current()) setOutcomes(items => [...items, result]); });
      if (!current()) return;
      const saved = await colorMatch.status(photoId);
      if (current() && saved) { setReport(saved); setControls(saved.controls); }
    });
  });

  const locked = disabled || busy || regionalBusy || !inTauri();
  const libraryReference = references.some(item => item.id === selected);
  const removedSession = !libraryReference && report?.reference.id === selected;
  const ready = Boolean(report?.current && report.photoId === photoId && report.reference.id === selected);
  return <section className="lr-panel color-match-panel" aria-label="Reference colour matching" aria-busy={busy || regionalBusy}>
    <h3>Reference colour matching</h3>
    <p className="lr-hint">Choose an edited JPEG, PNG or TIFF to guide the colour of a different photo. Tone and colour are matched; facial retouching stays with this photo.</p>
    <button type="button" disabled={locked} onClick={() => void upload()}>Upload colour reference</button>
    <button type="button" disabled={locked} onClick={uploadSeveral}>Upload several colour references</button>
    {projectId && <details><summary>Use this edited photo as a reference</summary>
      <label>Edited reference name<input maxLength={120} value={captureName} disabled={locked} onChange={event => setCaptureName(event.target.value)}/></label>
      <button type="button" disabled={locked || !captureName.trim()} onClick={capture}>Create colour reference from this edit</button>
    </details>}
    <label>Saved colour reference<select aria-label="Saved colour reference" value={selected} disabled={locked}
      onChange={event => { const reference = references.find(item => item.id === event.target.value); if (reference) choose(reference); else { setSelected(''); setReport(null); } }}>
      <option value="">Choose a reference</option>
      {removedSession && <option value={selected}>{report.reference.name} (saved match only)</option>}
      {references.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}
    </select></label>
    <ColorReferenceLibrary references={references} selected={selected} disabled={locked} controls={controls} onSelect={choose} onManage={manage}/>
    {removedSession && <p>This reference was removed from the library. You can still adjust its saved match. Upload it again to analyse a new match.</p>}
    <button type="button" disabled={locked || !libraryReference} onClick={() => void analyse()}>Analyse reference match</button>
    <fieldset disabled={locked || !selected || Boolean(report && !report.current)}>
      <legend>Colour matching strengths</legend>
      {SLIDERS.map(([key, label]) => <label key={key}>{label} ({Math.round(controls[key] * 100)}%)
        <input type="range" aria-label={label} min="0" max="200" step="1" value={Math.round(controls[key] * 100)}
          onChange={event => setControls(value => ({ ...value, [key]: Number(event.target.value) / 100 }))}/>
      </label>)}
      <details><summary>Fine matching controls</summary>
        <p className="lr-hint">Adjust each part of the matched look. These strengths act within the overall tone, colour and warmth settings. Stronger settings still pass the native highlight and skin-colour checks.</p>
        {FINE_MATCH_FIELDS.map(([key, label]) => <label key={key}>{label} ({Math.round((controls.detail?.[key] ?? 1) * 100)}%)
          <input type="range" aria-label={label} min="0" max="200" step="1" value={Math.round((controls.detail?.[key] ?? 1) * 100)}
            onChange={event => setControls(value => ({ ...value, detail: { ...value.detail, [key]: Number(event.target.value) / 100 } }))}/>
        </label>)}
      </details>
      <button type="button" disabled={!ready} onClick={() => void apply()}>Apply colour strengths</button>
      <button type="button" disabled={!ready} onClick={() => void apply({ ...controls, amount: 0 })}>Remove matched treatment</button>
    </fieldset>
    {projectId && <details><summary>Match this collection</summary>
      <p className="lr-hint">Apply the chosen strengths to every photo in this collection. Each photo is measured separately and checked at native resolution. Save initial corrections on new photos first.</p>
      <button type="button" disabled={locked || !libraryReference} onClick={() => void collection()}>Match colour across this collection</button>
      {outcomes.length > 0 && <ol aria-label="Collection colour matching results">{outcomes.map((item, index) =>
        <li key={`${item.photoId}:${index}`}>{item.name}: {item.outcome}. {item.detail}</li>)}</ol>}
    </details>}
    <ReferenceLibraryManager disabled={locked} controls={controls} onUpdated={setReferences}/>
    <RegionalColorMatchPanel key={`regions:${photoId}:${selected}`} photoId={photoId} referenceId={selected}
      referenceAvailable={libraryReference} revision={revision} disabled={disabled || busy || !inTauri()}
      onApply={onApply} onBusyChange={setRegionalBusy}/>
    {report && <>
      {!report.current && <p role="status">This analysis is no longer current. Analyse again to keep your saved edits.</p>}
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
