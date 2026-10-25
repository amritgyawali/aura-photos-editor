import { useEffect, useRef, useState } from 'react';
import { api, inTauri } from '../../ipc/client';
import { Refusal, reasonOf } from './refusal';
import { colorMatch, DEFAULT_MATCH_CONTROLS, pickColorReference, pickColorReferences, type ColorMatchControls,
  type ColorMatchIntent, type ColorMatchReport, type ColorReference, type ReferenceMetadata } from '../../ipc/colorMatch';
import { matchColorCollection, type ColorMatchOutcome } from './matchColorCollection';
import { FINE_MATCH_FIELDS } from '../../ipc/colorMatchDetails';
import { ColorReferenceLibrary } from './ColorReferenceLibrary';
import { RegionalColorMatchPanel } from './RegionalColorMatchPanel';
import { ReferenceLibraryManager } from './ReferenceLibraryManager';
import { LightroomPresetExport } from './LightroomPresetExport';
import { toneSentence } from './toneSentence';
import { PALETTE_ENGINES } from './colorMatchEngines';
import { barelyChanged, slightMessage } from './visibleChange';
import { COLOR_MATCH_PREFERENCES, matchPreference, preferenceOf, recommendedReferences } from './colorMatchPreferences';
import './colorMatchWorkspace.css';

const SLIDERS = [
  ['amount', 'Match amount'], ['tone', 'Tone match'], ['color', 'Colour match'], ['whiteBalance', 'Warmth match'],
] as const;

export function ColorMatchPanel({ photoId, projectId, disabled, revision, onApply, active = true, selectedPhotoIds = [], onReferenceChange }: {
  photoId: string; projectId?: string; disabled: boolean; revision?: string;
  active?: boolean; selectedPhotoIds?: readonly string[];
  onReferenceChange?: (reference: ColorReference | null) => void;
  /** Saves through the editor. Resolves to a reason when nothing was saved (a refusal, or
   *  another edit still saving); older callers that resolve to nothing are treated as saved. */
  onApply: (action: () => Promise<unknown>) => Promise<unknown>;
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
  const [live, setLive] = useState(false);
  const [quality, setQuality] = useState<'quick' | 'precise'>('precise');
  const [matchIntent, setMatchIntent] = useState<ColorMatchIntent>('natural');
  const [referenceTab, setReferenceTab] = useState<'recommended' | 'mine'>('recommended');
  const [syncScope, setSyncScope] = useState<'selected' | 'collection'>('selected');
  const [liveSignal, setLiveSignal] = useState(0);
  const liveEnabled = useRef(live); liveEnabled.current = live;
  const visible = useRef(active); visible.current = active;
  const disabledRef = useRef(disabled); disabledRef.current = disabled;
  // A parent locks the editor while onApply saves. Only that phase owns its lock;
  // an unrelated workflow that starts during analysis must delay our write.
  const ownWrite = useRef(false);
  const intent = useRef(0);
  const pendingLive = useRef(false);
  const reportRef = useRef(report); reportRef.current = report;
  const controlsRef = useRef(controls); controlsRef.current = controls;
  const qualityRef = useRef(quality); qualityRef.current = quality;
  const matchIntentRef = useRef(matchIntent); matchIntentRef.current = matchIntent;
  const referenceCallback = useRef(onReferenceChange); referenceCallback.current = onReferenceChange;
  const mounted = useRef(true);
  const running = useRef(false);
  const job = useRef<string | null>(null);
  const generation = useRef(0);
  const stopped = useRef(false);
  const activePhoto = useRef(photoId);
  activePhoto.current = photoId;
  const activeReference = useRef(selected);
  activeReference.current = selected;
  const acceptReport = (value: ColorMatchReport | null) => { reportRef.current = value; setReport(value); };
  const requestLive = () => {
    ++intent.current;
    if (liveEnabled.current && visible.current) { pendingLive.current = true; setLiveSignal(value => value + 1); }
  };
  const changeControls = (value: ColorMatchControls) => {
    controlsRef.current = value; setControls(value); requestLive();
  };
  const stop = () => {
    ++intent.current; pendingLive.current = false; stopped.current = true;
    liveEnabled.current = false; setLive(false);
    if (job.current) void api.cancelJob(job.current).catch(cause => { if (mounted.current) setError(reasonOf(cause)); });
  };

  useEffect(() => {
    referenceCallback.current?.(references.find(item => item.id === selected) ?? (report?.reference.id === selected ? report.reference : null));
  }, [references, selected, report?.reference]);
  useEffect(() => () => { referenceCallback.current?.(null); }, []);
  useEffect(() => {
    if (!active) stop();
  }, [active]);

  useEffect(() => { mounted.current = true; return () => {
    mounted.current = false;
    pendingLive.current = false; ++intent.current;
    stopped.current = true;
    if (job.current) void api.cancelJob(job.current).catch(() => undefined);
  }; }, []);
  useEffect(() => {
    acceptReport(null); setOutcomes([]); setMessage(null); setError(null);
    pendingLive.current = false; ++intent.current;
    if (job.current) {
      stopped.current = true;
      void api.cancelJob(job.current).catch(cause => { if (mounted.current) setError(reasonOf(cause)); });
    }
  }, [photoId]);
  useEffect(() => {
    if (!inTauri()) return;
    const token = ++generation.current;
    void Promise.all([colorMatch.list(), colorMatch.status(photoId)]).then(([items, saved]) => {
      if (!mounted.current || token !== generation.current || running.current) return;
      setReferences(items); acceptReport(saved);
      if (saved) {
        if (!pendingLive.current) {
          setSelected(saved.reference.id); setControls(saved.controls); setQuality(saved.fitting?.quality ?? 'precise');
          setMatchIntent(saved.fitting?.intent ?? 'natural');
        }
        if (!saved.current) { pendingLive.current = false; liveEnabled.current = false; setLive(false); }
      }
      else {
        const reference = items.find(item => item.id === activeReference.current) ?? items[0];
        if (!pendingLive.current) { setSelected(reference?.id || ''); setControls(reference?.defaultControls ?? { ...DEFAULT_MATCH_CONTROLS }); }
      }
    }).catch(cause => { if (mounted.current && token === generation.current) setError(reasonOf(cause)); });
  }, [photoId, revision]);

  // A save the editor refused ends this panel's work as a refusal, with its own message, so the
  // panel never keeps a progress line for a match that was not saved.
  const save = async (action: () => Promise<void>) => {
    const outcome = await onApply(action);
    if (typeof outcome === 'string' && outcome) throw new Refusal(outcome);
  };
  const run = async (action: (id: string, current: () => boolean) => Promise<void>) => {
    if (running.current || disabledRef.current || !visible.current || !inTauri()) return;
    running.current = true; ++generation.current; setBusy(true); setError(null); setMessage(null);
    stopped.current = false;
    const id = crypto.randomUUID(); job.current = id;
    const current = () => mounted.current && visible.current && activePhoto.current === photoId && !stopped.current;
    try { await action(id, current); }
    catch (cause) { if (current()) { setMessage(null); setError(reasonOf(cause)); } }
    finally { job.current = null; running.current = false; if (mounted.current) {
      setBusy(false);
      if (stopped.current && activePhoto.current === photoId) setMessage('Stopped. Completed changes remain saved.');
    } }
  };
  useEffect(() => {
    if (!live || !active || disabled || busy || regionalBusy || !pendingLive.current || !selected || !inTauri()) return;
    const timer = window.setTimeout(() => {
      if (!pendingLive.current || running.current || !visible.current || !liveEnabled.current) return;
      const sequence = intent.current;
      const referenceId = activeReference.current;
      const next = structuredClone(controlsRef.current);
      const mode = qualityRef.current;
      const purpose = matchIntentRef.current;
      pendingLive.current = false;
      void run(async (id, current) => {
        const latest = () => current() && sequence === intent.current && referenceId === activeReference.current && liveEnabled.current;
        let analysis = reportRef.current;
        if (analysis && !analysis.current) {
          liveEnabled.current = false; setLive(false);
          setError('This photo changed. Analyse the reference again before enabling real-time matching.'); return;
        }
        if (!analysis || analysis.reference.id !== referenceId || (analysis.fitting?.quality ?? 'precise') !== mode ||
            (analysis.fitting?.intent ?? 'natural') !== purpose ||
            (purpose === 'palette' && !PALETTE_ENGINES.includes(analysis.fitting?.engine ?? ''))) {
          analysis = purpose === 'natural' ? await colorMatch.prepare(photoId, referenceId, id, mode)
            : await colorMatch.prepare(photoId, referenceId, id, mode, purpose);
          if (!current()) return;
          acceptReport(analysis);
        }
        if (!latest()) return;
        if (!analysis.current || analysis.photoId !== photoId) throw new Refusal('The analysis is no longer current. Analyse this photo again.');
        if (disabledRef.current) { pendingLive.current = true; return; }
        const session = analysis.sessionId;
        ownWrite.current = true;
        try {
          await save(async () => {
            if (!latest()) return;
            const result = await colorMatch.apply(photoId, session, next, id);
            if (!current()) return;
            if (!result.current || !result.fullResolutionChecked || result.photoId !== photoId || result.reference.id !== referenceId) {
              throw new Refusal('This photo returned no current native-resolution colour check. Analyse the reference again.');
            }
            acceptReport(result);
            if (latest()) { setControls(result.controls); setMessage('Real-time colour strengths saved. Use Undo to compare.'); }
          });
        } finally { ownWrite.current = false; }
      });
    }, 350);
    return () => window.clearTimeout(timer);
  }, [liveSignal, live, active, disabled, busy, regionalBusy, selected]);
  const upload = () => run(async (id, current) => {
    const path = await pickColorReference();
    if (!path || !current()) return;
    const reference = await colorMatch.import(path, id);
    if (!current()) return;
    setReferences(items => [reference, ...items.filter(item => item.id !== reference.id)]);
    activeReference.current = reference.id; setSelected(reference.id); acceptReport(null);
    setControls(reference.defaultControls ?? { ...DEFAULT_MATCH_CONTROLS });
    controlsRef.current = reference.defaultControls ?? { ...DEFAULT_MATCH_CONTROLS }; requestLive();
    setMessage('Reference saved. Analyse its colour treatment against this photo.');
  });
  const analyse = () => run(async (id, current) => {
    const result = matchIntent === 'palette' ? await colorMatch.prepare(photoId, selected, id, quality, matchIntent)
      : quality === 'precise' ? await colorMatch.prepare(photoId, selected, id) : await colorMatch.prepare(photoId, selected, id, quality);
    if (!current()) return;
    acceptReport(result);
    setMessage('Analysis ready. Adjust the strengths, then apply to see the saved result.');
  });
  const matchNow = () => {
    pendingLive.current = false;
    const sequence = ++intent.current;
    const referenceId = selected;
    const mode = quality;
    const purpose = matchIntent;
    const strengths = structuredClone(controlsRef.current);
    void run(async (id, current) => {
      const latest = () => current() && sequence === intent.current && referenceId === activeReference.current;
      setMessage('Matching the selected reference to this photo…');
      const analysis = await colorMatch.prepare(photoId, referenceId, id, mode, purpose);
      if (!latest()) return;
      if (!analysis.current || analysis.photoId !== photoId || analysis.reference.id !== referenceId) {
        throw new Refusal('The reference or photo changed. Match this photo again.');
      }
      acceptReport(analysis);
      if (disabledRef.current) throw new Refusal('Another edit is running. Match this photo again when it finishes.');
      ownWrite.current = true;
      try {
        await save(async () => {
          if (!latest()) return;
          const result = await colorMatch.apply(photoId, analysis.sessionId, strengths, id);
          if (!current()) return;
          if (!result.current || !result.fullResolutionChecked || result.photoId !== photoId || result.reference.id !== referenceId) {
            throw new Refusal('This photo returned no current native-resolution colour check. Match this photo again.');
          }
          acceptReport(result); setControls(result.controls);
          setMessage(barelyChanged(result) ? slightMessage(result)
            : result.afterDistance >= result.beforeDistance - 0.0001 && !result.fitting?.transfer?.learnedFrom
            ? 'No closer colour match passed this photo’s safety checks. No stronger match was saved.'
            : result.changed ? `Matched ${result.reference.name} to this photo. Use Undo to compare.`
              : 'This photo already has the selected reference match saved.');
        });
      } finally { ownWrite.current = false; }
    });
  };
  const apply = (next = controls) => run(async (id, current) => {
    if (!report || !report.current || report.photoId !== photoId || report.reference.id !== selected ||
        (report.fitting?.quality ?? 'precise') !== quality || (report.fitting?.intent ?? 'natural') !== matchIntent) return;
    await save(async () => {
      const result = await colorMatch.apply(photoId, report.sessionId, next, id);
      if (!current()) return;
      if (!result.current || !result.fullResolutionChecked || result.photoId !== photoId || result.reference.id !== selected) {
        throw new Refusal('This photo returned no current native-resolution colour check. Analyse the reference again.');
      }
      acceptReport(result); setControls(result.controls);
      setMessage(barelyChanged(result) ? slightMessage(result)
        : result.changed ? 'Colour strengths saved. Use Undo to compare.' : 'These strengths did not change the saved edit.');
    });
  });
  const choose = (reference: ColorReference) => {
    ++generation.current; activeReference.current = reference.id;
    setSelected(reference.id); acceptReport(null); setMessage('Reference selected. Click Match reference to this photo to apply its colours.');
    setControls(reference.defaultControls ?? { ...DEFAULT_MATCH_CONTROLS });
    controlsRef.current = reference.defaultControls ?? { ...DEFAULT_MATCH_CONTROLS }; requestLive();
  };
  const manage = (reference: ColorReference, metadata: ReferenceMetadata, remove = false) => void run(async (_id, current) => {
    const updated = await colorMatch.manage(reference.id, metadata, remove ? 'remove' : 'update');
    if (!current()) return;
    setReferences(items => updated ? items.map(item => item.id === updated.id ? updated : item) : items.filter(item => item.id !== reference.id));
    if (!updated && selected === reference.id && report?.reference.id !== reference.id) { setSelected(''); acceptReport(null); }
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
      } catch (cause) { if (current()) failures.push(reasonOf(cause)); }
    }
    if (!current()) return;
    if (imported[0]) choose(imported[0]);
    if (paths.length) setMessage(`${imported.length} references saved; ${failures.length} could not be imported.`);
    if (failures.length) setError(failures.join(' '));
  });
  const collection = (scope: 'selected' | 'collection' = 'collection') => run(async (id, current) => {
    if (!projectId || !selected) return;
    setOutcomes([]);
    await save(async () => {
      await matchColorCollection(projectId, selected, { ...controls }, id,
        () => !current(),
        value => { if (current()) setMessage(value); },
        result => { if (current()) setOutcomes(items => [...items, result]); },
        { quality, ...(matchIntent === 'palette' ? { intent: matchIntent } : {}), ...(scope === 'selected' ? { photoIds: selectedPhotoIds.filter(value => value !== photoId) } : {}) });
      if (!current()) return;
      const saved = await colorMatch.status(photoId);
      if (current() && saved) { acceptReport(saved); setControls(saved.controls); }
    });
  });

  const locked = !active || disabled || busy || regionalBusy || !inTauri();
  // During our own live job the next slider value is queued, while other tools stay locked.
  const strengthsLocked = !active || regionalBusy || !inTauri() || (disabled && !(live && ownWrite.current)) || (busy && !live);
  const libraryReference = references.some(item => item.id === selected);
  const removedSession = !libraryReference && report?.reference.id === selected;
  const ready = Boolean(report?.current && report.photoId === photoId && report.reference.id === selected &&
    (report.fitting?.quality ?? 'precise') === quality && (report.fitting?.intent ?? 'natural') === matchIntent &&
    (matchIntent !== 'palette' || PALETTE_ENGINES.includes(report.fitting?.engine ?? '')));
  const learnedFrom = report?.current && report.photoId === photoId && report.reference.id === selected
    ? report.fitting?.transfer?.learnedFrom : undefined;
  const savedStrengths = Boolean(report && SLIDERS.every(([key]) => controls[key] === report.controls[key]) &&
    FINE_MATCH_FIELDS.every(([key]) => (controls.detail?.[key] ?? 1) === (report.controls.detail?.[key] ?? 1)));
  return <section className="lr-panel color-match-panel" aria-label="Reference colour matching" aria-busy={busy || regionalBusy}>
    <header className="color-match-heading"><span>AI STYLE</span><h3>Color Match</h3></header>
    <p className="lr-hint">Choose an edited JPEG, PNG or TIFF to guide the colour of a different photo. Tone and colour are matched; facial retouching stays with this photo.</p>
    <label className="color-match-live"><input type="checkbox" checked={live} disabled={!active || regionalBusy || (!live && (disabled || busy)) || !inTauri()}
      onChange={event => {
        liveEnabled.current = event.target.checked; setLive(event.target.checked);
        if (event.target.checked) requestLive(); else stop();
      }}/>Real-Time Color Adjustments</label>
    <p className="lr-hint">When enabled, reference and strength changes are saved after a short pause. Undo keeps earlier edits available.</p>
    <div className="color-match-primary-actions">
      <button type="button" disabled={locked} onClick={uploadSeveral}><span aria-hidden="true">＋</span>Upload References</button>
      {projectId && <button type="button" disabled={locked || !captureName.trim()} onClick={capture}><span aria-hidden="true">▧</span>Create Reference</button>}
    </div>
    <details><summary>Upload options</summary>
      <button type="button" disabled={locked} onClick={() => void upload()}>Upload colour reference</button>
      <button type="button" disabled={locked} onClick={uploadSeveral}>Upload several colour references</button>
    </details>
    {projectId && <details><summary>Use this edited photo as a reference</summary>
      <label>Edited reference name<input maxLength={120} value={captureName} disabled={locked} onChange={event => setCaptureName(event.target.value)}/></label>
      <button type="button" disabled={locked || !captureName.trim()} onClick={capture}>Create colour reference from this edit</button>
    </details>}
    <label>Matching goal<select aria-label="Matching goal" value={matchIntent} disabled={strengthsLocked} onChange={event => {
      const next = event.target.value === 'palette' ? 'palette' : 'natural';
      matchIntentRef.current = next; setMatchIntent(next); requestLive();
    }}><option value="natural">Natural look</option><option value="palette">Reference palette and tone</option></select></label>
    <p className="lr-hint">Natural look keeps a different scene's tonal balance. Reference palette and tone makes a stronger match to the reference's colour distribution, with native skin and highlight protection. Scene content can affect the result.</p>
    <label>Matching mode<select aria-label="Matching mode" value={quality} disabled={strengthsLocked} onChange={event => {
      const next = event.target.value === 'quick' ? 'quick' : 'precise';
      qualityRef.current = next; setQuality(next); requestLive();
    }}><option value="quick">Quick Mode</option><option value="precise">Precise Mode</option></select></label>
    <p className="lr-hint">Quick uses a smaller fitting budget. Both modes check saved output at native resolution.</p>
    <label>Saved colour reference<select aria-label="Saved colour reference" value={selected} disabled={locked}
      onChange={event => { const reference = references.find(item => item.id === event.target.value); if (reference) choose(reference); else {
        ++intent.current; pendingLive.current = false; activeReference.current = ''; setSelected(''); acceptReport(null);
      } }}>
      <option value="">Choose a reference</option>
      {removedSession && <option value={selected}>{report.reference.name} (saved match only)</option>}
      {references.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}
    </select></label>
    <button type="button" disabled={locked || !libraryReference} onClick={matchNow}>Match reference to this photo</button>
    {selected && (!ready || !report?.fullResolutionChecked) && <p role="status" className="lr-hint">
      The selected reference has not been applied to this photo. Match it to update Your photograph.
    </p>}
    <div className="color-match-tabs" role="tablist" aria-label="Colour reference views">
      <button type="button" role="tab" aria-selected={referenceTab === 'recommended'} aria-controls="recommended-color-references" id="recommended-color-tab"
        onClick={() => setReferenceTab('recommended')}>Recommended</button>
      <button type="button" role="tab" aria-selected={referenceTab === 'mine'} aria-controls="my-color-references" id="my-color-tab"
        onClick={() => setReferenceTab('mine')}>My References</button>
    </div>
    <div role="tabpanel" id={referenceTab === 'recommended' ? 'recommended-color-references' : 'my-color-references'}
      aria-labelledby={referenceTab === 'recommended' ? 'recommended-color-tab' : 'my-color-tab'}>
      <p className="lr-hint">{referenceTab === 'recommended' ? 'Recommended from your library: shared measured regions, then favorites.' : 'Your uploaded and created references, stored locally.'}</p>
      <div className="color-match-reference-cards">{(referenceTab === 'recommended' ? recommendedReferences(references, report?.matchedRegions ?? []) : references).map(reference =>
        <button type="button" key={reference.id} aria-label={`Select reference ${reference.name}`} aria-pressed={reference.id === selected} disabled={locked} onClick={() => choose(reference)}>
          {reference.preview || reference.thumbnail ? <img src={reference.preview || reference.thumbnail} alt="" loading="lazy"/> : <span className="color-match-no-preview">Colour descriptor</span>}
          <span>{reference.name}</span>
        </button>)}</div>
      {!references.length && <p>Upload a finished photo or create a reference from your edit to begin.</p>}
    </div>
    <ColorReferenceLibrary references={references} selected={selected} disabled={locked} controls={controls} onSelect={choose} onManage={manage}/>
    {removedSession && <p>This reference was removed from the library. You can still adjust its saved match. Upload it again to analyse a new match.</p>}
    <button type="button" disabled={locked || !libraryReference} onClick={() => void analyse()}>Analyse reference match</button>
    <label>Color Match Preferences<select aria-label="Color Match Preferences" value={preferenceOf(controls)} disabled={strengthsLocked || !selected} onChange={event => {
      const next = matchPreference(event.target.value); if (next) changeControls(next);
    }}><option value="custom">Custom</option>{COLOR_MATCH_PREFERENCES.map(preference => <option key={preference.id} value={preference.id}>{preference.name}</option>)}</select></label>
    <p className="lr-hint">AURA strengths: Subtle 50%, Standard 100%, Strong 150%, Extreme 200%. Tone, colour and warmth are 100%. Native skin and highlight checks remain active.</p>
    <fieldset disabled={strengthsLocked || !selected || Boolean(report && !report.current)}>
      <legend>Colour matching strengths</legend>
      {learnedFrom && <p className="lr-hint" data-testid="colour-match-learned-controls">Amount scales your own edit up to 100%. Tone, colour, warmth and the fine controls have no separate part in a learned edit.</p>}
      {SLIDERS.map(([key, label]) => <label key={key}>{label} ({Math.round(controls[key] * 100)}%)
        <input type="range" aria-label={label} min="0" max={learnedFrom && key === 'amount' ? '100' : '200'} step="1"
          disabled={Boolean(learnedFrom) && key !== 'amount'} value={Math.round(controls[key] * 100)}
          onChange={event => changeControls({ ...controlsRef.current, [key]: Number(event.target.value) / 100 })}/>
      </label>)}
      <details hidden={Boolean(learnedFrom)}><summary>Fine matching controls</summary>
        <p className="lr-hint">Adjust each part of the matched look. These strengths act within the overall tone, colour and warmth settings. Stronger settings still pass the native highlight and skin-colour checks.</p>
        {FINE_MATCH_FIELDS.map(([key, label]) => <label key={key}>{label} ({Math.round((controls.detail?.[key] ?? 1) * 100)}%)
          <input type="range" aria-label={label} min="0" max="200" step="1" value={Math.round((controls.detail?.[key] ?? 1) * 100)}
            onChange={event => changeControls({ ...controlsRef.current, detail: { ...controlsRef.current.detail, [key]: Number(event.target.value) / 100 } })}/>
        </label>)}
      </details>
      <button type="button" disabled={locked || !ready} onClick={() => void apply()}>Apply colour strengths</button>
      <button type="button" disabled={locked || !ready} onClick={() => void apply({ ...controls, amount: 0 })}>Remove matched treatment</button>
    </fieldset>
    <div className="color-match-footer">
      <button type="button" disabled={locked || !libraryReference} onClick={() => {
        const reference = references.find(item => item.id === selected); if (reference) manage(reference, { defaultControls: structuredClone(controls) });
      }}>Save Preset</button>
      {projectId && <button type="button" disabled={locked || !libraryReference || (syncScope === 'selected' && !selectedPhotoIds.some(value => value !== photoId))}
        onClick={() => void collection(syncScope)}>Sync</button>}
    </div>
    <p className="lr-hint">Save Preset remembers these matching strengths for the selected reference.</p>
    {projectId && <label>Sync destination<select aria-label="Sync destination" value={syncScope} disabled={locked} onChange={event => setSyncScope(event.target.value === 'collection' ? 'collection' : 'selected')}>
      <option value="selected">Selected photos ({new Set(selectedPhotoIds.filter(value => value !== photoId)).size})</option>
      <option value="collection">Entire collection</option>
    </select></label>}
    {projectId && <details open={outcomes.length > 0 || undefined}><summary>Match this collection</summary>
      <p className="lr-hint">Apply the chosen strengths to every photo in this collection. Each photo is measured separately and checked at native resolution. New photos start from their camera baseline.</p>
      <button type="button" disabled={locked || !libraryReference} onClick={() => void collection()}>Match colour across this collection</button>
      {outcomes.length > 0 && <ol aria-label="Collection colour matching results">{outcomes.map((item, index) =>
        <li key={`${item.photoId}:${index}`}>{item.name}: {item.outcome}. {item.detail}</li>)}</ol>}
    </details>}
    <ReferenceLibraryManager disabled={locked} controls={controls} onUpdated={setReferences}/>
    <RegionalColorMatchPanel key={`regions:${photoId}:${selected}`} photoId={photoId} referenceId={selected}
      referenceAvailable={libraryReference} revision={revision} disabled={!active || disabled || busy || !inTauri()}
      onApply={onApply} onBusyChange={setRegionalBusy}/>
    {report && <>
      {!report.current && <p role="status">This analysis is no longer current. Analyse again to keep your saved edits.</p>}
      <p className="lr-hint">Each change starts from the edit saved at analysis. New edits require a fresh analysis. Reference images without an ICC profile are treated as sRGB.</p>
      <p>Measured regions: {report.matchedRegions.join(', ')}.</p>
      {report.matchedRegions.length < 2 && <p>Only overall colour could be compared. Similar lighting and subject matter will give a more useful reference.</p>}
      <p data-testid="colour-match-measurement">Appearance distance: {report.beforeDistance.toFixed(2)} → {report.afterDistance.toFixed(2)}.
        {' '}{report.fullResolutionChecked ? 'Checked on native-resolution renders.' : report.fitting?.nativeCandidateChecked
          ? 'Fitted endpoint checked at native resolution. Apply to save and verify chosen strengths.' : 'Analysis preview; apply to check native-resolution renders.'}
        {report.protectedFields > 0 && ` ${report.protectedFields} manual fields protected.`}</p>
      {report.fullResolutionChecked && report.fitting?.transfer &&
        <p data-testid="colour-match-components">Tone difference: {report.fitting.transfer.renderedBefore.tone.toFixed(2)} → {report.fitting.transfer.renderedAfter.tone.toFixed(2)}.
          {report.fitting.intent === 'palette' && <> Colour palette difference: {report.fitting.transfer.renderedBefore.palette.toFixed(2)} → {report.fitting.transfer.renderedAfter.palette.toFixed(2)}.</>}
          {' '}Lower means closer to the reference.</p>}
      {report.visibleChange !== undefined && <p className="lr-hint" data-testid="colour-match-visible">Visible change from your original: {report.visibleChange.toFixed(1)} (below about 0.45 is hard to see).</p>}
      {learnedFrom && <p role="status" data-testid="colour-match-learned">Learned from your own edit of {learnedFrom}. AURA applies that same colour and tone operation to this photo, checked at native resolution.</p>}
      {!learnedFrom && report.fitting?.transfer?.originalRefused && <p className="lr-hint" data-testid="colour-match-original-refused">{report.fitting.transfer.originalRefused} The look was estimated from the edited photo alone.</p>}
      {report.fitting?.transfer?.toneEstimate && <p className="lr-hint" data-testid="colour-match-tone">{toneSentence(report.fitting.transfer.toneEstimate)}</p>}
      {report.fitting?.nativeCandidateChecked && (report.fitting.nativeBacktrackFactor ?? 1) < 1 &&
        <p className="lr-hint">Colour, smoothness, skin and highlight checks limited the fitted {report.fitting.intent === 'palette' ? 'palette' : 'look'} to {Math.round((report.fitting.nativeBacktrackFactor ?? 1) * 100)}% of the unconstrained fit. Your strengths adjust this protected endpoint.</p>}
    </>}
    {busy && <><p role="status">{message ?? 'Measuring colour and checking the image…'}</p>
      <button type="button" onClick={stop}>Stop colour matching</button></>}
    {!busy && message && <p role="status">{message}</p>}
    {error && <p role="alert">{error}</p>}
    <LightroomPresetExport key={`${photoId}:${report?.sessionId ?? ''}:${revision ?? ''}:${JSON.stringify(controls)}:${selected}:${report?.current ?? false}`}
      photoId={photoId} sessionId={report?.sessionId ?? ''} referenceName={report?.reference.name ?? ''}
      eligible={ready && Boolean(report?.fullResolutionChecked) && savedStrengths} disabled={locked}/>
  </section>;
}
