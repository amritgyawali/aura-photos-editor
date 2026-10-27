import { useEffect, useMemo, useRef, useState } from 'react';
import { api, asIpcError, inTauri } from '../../ipc/client';
import { colorMatch, levelOf, oneClickMatch, pickColorReferences, PREFERENCE_LEVELS, preferenceControls,
  type ColorMatchControls, type ColorMatchPreset, type ColorMatchReport, type ColorRecommendation, type ColorReference,
  type PreferenceLevel, type QuickMatchReport } from '../../ipc/colorMatch';
import { useStore } from '../../state/store';
import { ColorMatchPanel } from './ColorMatchPanel';
import { collectionTargets, syncColorMatch, type SyncOutcome, type SyncTarget } from './syncColorMatch';
import './colorMatchStudio.css';

/** The reference the canvas shows beside the photo. `chosen` is true when the photographer just picked it. */
export type ActiveReference = { id: string; name: string; thumbnail?: string };

type Tab = 'recommended' | 'mine' | 'presets';
type Mode = 'quick' | 'advanced';
const TABS: readonly (readonly [Tab, string])[] = [['recommended', 'Recommended'], ['mine', 'My References'], ['presets', 'Presets']];
const STRENGTHS = [['amount', 'Match amount'], ['tone', 'Tone match'], ['color', 'Colour match'], ['whiteBalance', 'Warmth match']] as const;
const LEVEL_NAMES: Record<PreferenceLevel, string> = {
  light: 'Light', natural: 'Natural', standard: 'Standard', strong: 'Strong', extreme: 'Extreme', custom: 'Custom',
};

function remembered<T extends string>(key: string, fallback: T, allowed: readonly T[]): T {
  try { const value = localStorage.getItem(key); return allowed.includes(value as T) ? value as T : fallback; }
  catch { return fallback; }
}
function remember(key: string, value: string): void {
  try { localStorage.setItem(key, value); } catch { /* a per-viewer convenience; nothing depends on it */ }
}
function isQuick(report: ColorMatchReport | QuickMatchReport | null): report is QuickMatchReport {
  return Boolean(report && 'limited' in report);
}

/** What happened, in one or two sentences. */
export function describeMatch(report: QuickMatchReport): string {
  if (report.controls.amount === 0 && report.requestedAmount === 0) return 'Matched look removed. The photo is back to its edit from before matching.';
  const requested = levelOf({ ...report.controls, amount: report.requestedAmount });
  const parts = [`Matched to ${report.reference.name}${requested === 'custom' ? '' : ` · ${LEVEL_NAMES[requested]}`}.`];
  if (report.limited) {
    const why = report.limitedBy === 'highlights' ? 'so highlights keep their detail'
      : report.limitedBy === 'skin' ? 'so skin stays natural'
        : report.limitedBy === 'distance' ? 'because more would move the photo away from the reference'
          : 'so highlights and skin stay natural';
    parts.push(`Reduced to ${Math.round(report.controls.amount * 100)}% of the reference look ${why}.`);
  }
  if (report.cautious) parts.push("This photo's light is far from the reference's, so a lighter, cautious version of the look was used.");
  if (report.changed === 0) parts.push('No change was needed at this level.');
  if (report.referenceMonochrome) parts.push('The reference is black and white, so this photo was converted too.');
  if (report.createdBaseline) parts.push('Its unedited look was saved first, so Undo returns to it.');
  return parts.join(' ');
}

export function ColorMatchStudio({ projectId, photoId, revision, disabled, onApply, onReferenceChange }: {
  projectId: string; photoId: string; revision?: string; disabled: boolean;
  onApply: (action: () => Promise<unknown>) => Promise<void>;
  onReferenceChange?: (reference: ActiveReference | null, chosen: boolean) => void;
}): JSX.Element {
  const [references, setReferences] = useState<ColorReference[]>([]);
  const [recommended, setRecommended] = useState<ColorRecommendation[] | null>(null);
  const [recommending, setRecommending] = useState(false);
  const [presets, setPresets] = useState<ColorMatchPreset[]>([]);
  const [selected, setSelected] = useState('');
  const [report, setReport] = useState<ColorMatchReport | QuickMatchReport | null>(null);
  const [controls, setControls] = useState<ColorMatchControls>(preferenceControls('standard'));
  const [realtime, setRealtime] = useState(() => remembered('aura.colorMatch.realtime', 'on', ['on', 'off'] as const) === 'on');
  const [mode, setMode] = useState<Mode>(() => remembered('aura.colorMatch.mode', 'quick', ['quick', 'advanced'] as const));
  const [tab, setTab] = useState<Tab>(() => remembered('aura.colorMatch.tab', 'recommended', ['recommended', 'mine', 'presets'] as const));
  const [large, setLarge] = useState(() => remembered('aura.colorMatch.grid', 'large', ['large', 'small'] as const) === 'large');
  const [pending, setPending] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [naming, setNaming] = useState<'reference' | 'preset' | null>(null);
  const [name, setName] = useState('');
  const [query, setQuery] = useState('');
  const [syncScope, setSyncScope] = useState<'selected' | 'all'>('selected');
  const [syncOptions, setSyncOptions] = useState(false);
  const [outcomes, setOutcomes] = useState<SyncOutcome[]>([]);
  const selection = useStore(state => state.selection);
  const activeProject = useStore(state => state.activeProjectId);
  const rows = useStore(state => state.rows);
  const mounted = useRef(true);
  const running = useRef(false);
  const job = useRef<string | null>(null);
  const stopped = useRef(false);
  const generation = useRef(0);
  const activePhoto = useRef(photoId);
  activePhoto.current = photoId;
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const latest = useRef({ controls, selected });
  latest.current = { controls, selected };
  const recommendedFor = useRef<string | null>(null);
  const restoredFor = useRef<string | null>(null);
  const announce = useRef(onReferenceChange);
  announce.current = onReferenceChange;

  const byId = useMemo(() => new Map(references.map(item => [item.id, item])), [references]);
  const current = byId.get(selected);
  const level = levelOf(controls);

  useEffect(() => { mounted.current = true; return () => {
    mounted.current = false; stopped.current = true;
    if (timer.current) clearTimeout(timer.current);
    if (job.current) void api.cancelJob(job.current).catch(() => undefined);
  }; }, []);
  useEffect(() => {
    setReport(null); setOutcomes([]); setMessage(null); setError(null); setPending(false); setRecommended(null);
    recommendedFor.current = null; restoredFor.current = null;
    if (timer.current) clearTimeout(timer.current);
    if (job.current) { stopped.current = true; void api.cancelJob(job.current).catch(() => undefined); }
  }, [photoId]);
  // Saved state for this photo: its library, presets and any match it already carries. The
  // editor reloads after every save; a reload only replaces the report when the saved match
  // is not the one this panel just made (Undo, or the detailed panel, changed it).
  useEffect(() => {
    if (!inTauri()) return;
    const token = ++generation.current;
    void Promise.resolve().then(() => Promise.all([colorMatch.list(), colorMatch.status(photoId), oneClickMatch.presets('list')])).then(([items, saved, kept]) => {
      if (!mounted.current || token !== generation.current || running.current) return;
      setReferences(items); setPresets(kept);
      // A remembered Presets tab with no presets would hide every reference.
      if (!kept.length) setTab(value => value === 'presets' ? 'recommended' : value);
      setReport(prior => prior && saved && prior.sessionId === saved.sessionId && prior.current === saved.current
        && prior.controls.amount === saved.controls.amount ? prior : saved);
      if (restoredFor.current === photoId) return;
      restoredFor.current = photoId;
      if (saved) {
        setSelected(saved.reference.id); setPending(false);
        if (saved.controls.amount > 0) setControls(saved.controls);
        announce.current?.({ id: saved.reference.id, name: saved.reference.name, thumbnail: saved.reference.thumbnail }, false);
      }
    }).catch(cause => { if (mounted.current && token === generation.current) setError(asIpcError(cause).message); });
  }, [photoId, revision]);
  // Recommendations are measured once per photo and library size, only when someone looks.
  useEffect(() => {
    const key = `${photoId}:${references.length}`;
    if (!inTauri() || tab !== 'recommended' || recommended !== null || references.length === 0 || recommendedFor.current === key) return;
    recommendedFor.current = key;
    setRecommending(true);
    Promise.resolve().then(() => oneClickMatch.recommend(photoId, crypto.randomUUID()))
      .then(items => { if (mounted.current && recommendedFor.current === key) setRecommended(items); })
      .catch(cause => { if (mounted.current && recommendedFor.current === key) { setRecommended([]); setError(asIpcError(cause).message); } })
      .finally(() => { if (mounted.current && recommendedFor.current === key) setRecommending(false); });
  }, [photoId, tab, references.length, recommended]);

  const run = async (action: (id: string, current: () => boolean) => Promise<void>): Promise<void> => {
    if (running.current || disabled || !inTauri()) return;
    running.current = true; setBusy(true); setError(null); stopped.current = false;
    const id = crypto.randomUUID(); job.current = id;
    const current = () => mounted.current && activePhoto.current === photoId && !stopped.current;
    try { await action(id, current); }
    catch (cause) { if (current()) setError(asIpcError(cause).message); }
    finally {
      job.current = null; running.current = false;
      if (mounted.current) {
        setBusy(false);
        if (stopped.current && activePhoto.current === photoId) setMessage('Stopped. Finished changes stay saved.');
      }
    }
  };
  /** Match inside a running action; the editor saves, refreshes and keeps history. */
  const matchNow = async (id: string, current: () => boolean, referenceId: string, next: ColorMatchControls, reanalyse = false) => {
    await onApply(async () => {
      try {
        const result = await oneClickMatch.quick(photoId, referenceId, next, id, reanalyse);
        if (!current()) return;
        // Keep the requested level showing: a reduced Extreme is still the Extreme that was asked
        // for, and removing a match keeps the level for the next one.
        setReport(result); if (next.amount > 0) setControls({ ...next }); setPending(false);
        setMessage(describeMatch(result));
      } catch (cause) { if (current()) { setError(asIpcError(cause).message); setPending(true); } }
    });
  };
  const match = (referenceId = selected, next = controls, reanalyse = false) =>
    void run((id, current) => matchNow(id, current, referenceId, next, reanalyse));
  const scheduleMatch = () => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      timer.current = null;
      const { controls: next, selected: referenceId } = latest.current;
      if (referenceId) match(referenceId, next);
    }, 650);
  };

  const choose = (reference: ColorReference, next = controls) => {
    if (busy) return;
    setSelected(reference.id); setControls(next); setMessage(null); setError(null);
    announce.current?.({ id: reference.id, name: reference.name, thumbnail: reference.thumbnail }, true);
    if (realtime) match(reference.id, next);
    else { setPending(true); setMessage('Press Apply match to use this reference.'); }
  };
  const chooseLevel = (value: PreferenceLevel) => {
    if (value === 'custom') { setMode('advanced'); remember('aura.colorMatch.mode', 'advanced'); return; }
    const next = mode === 'quick' ? preferenceControls(value) : { ...controls, amount: preferenceControls(value).amount };
    setControls(next);
    if (!selected) return;
    if (realtime) match(selected, next); else setPending(true);
  };
  const changeStrength = (key: typeof STRENGTHS[number][0], value: number) => {
    setControls(prior => ({ ...prior, [key]: value }));
    if (!selected) return;
    setPending(true);
    if (realtime) scheduleMatch();
  };
  const upload = () => void run(async (id, current) => {
    const paths = await pickColorReferences();
    if (!paths.length || !current()) return;
    const added: ColorReference[] = []; const failures: string[] = [];
    for (const [index, path] of paths.entries()) {
      if (!current()) break;
      setMessage(`Saving reference ${index + 1} of ${paths.length}…`);
      try {
        const reference = await colorMatch.import(path, id);
        added.push(reference);
        if (current()) setReferences(items => [reference, ...items.filter(item => item.id !== reference.id)]);
      } catch (cause) { failures.push(asIpcError(cause).message); }
    }
    if (!current()) return;
    setRecommended(null); setTab('mine'); remember('aura.colorMatch.tab', 'mine');
    if (failures.length) setError(`${failures.length} could not be saved: ${failures.join(' ')}`);
    const first = added[0];
    if (!first) { setMessage(null); return; }
    setSelected(first.id);
    announce.current?.({ id: first.id, name: first.name, thumbnail: first.thumbnail }, true);
    if (realtime) await matchNow(id, current, first.id, controls);
    else { setPending(true); setMessage(`${added.length} reference${added.length === 1 ? '' : 's'} saved. Press Apply match to use ${first.name}.`); }
  });
  const createReference = () => void run(async (id, current) => {
    const reference = await colorMatch.capture(projectId, photoId, name.trim(), id);
    if (!current()) return;
    setReferences(items => [reference, ...items.filter(item => item.id !== reference.id)]);
    setRecommended(null); setNaming(null); setTab('mine'); remember('aura.colorMatch.tab', 'mine');
    setMessage(`Saved this edit as “${reference.name}”. Open another photo and choose it to match.`);
  });
  const savePreset = () => void run(async (_id, current) => {
    if (!selected) return;
    const kept = await oneClickMatch.presets('save', { name: name.trim(), referenceId: selected, level, controls });
    if (!current()) return;
    setPresets(kept); setNaming(null); setTab('presets'); remember('aura.colorMatch.tab', 'presets');
    setMessage(`Preset “${name.trim()}” saved.`);
  });
  const removePreset = (preset: ColorMatchPreset) => void run(async (_id, current) => {
    const kept = await oneClickMatch.presets('remove', { id: preset.id });
    if (current()) { setPresets(kept); setMessage(`Preset “${preset.name}” removed.`); }
  });
  const applyPreset = (preset: ColorMatchPreset) => {
    const reference = byId.get(preset.referenceId);
    if (!preset.available || !reference) { setError(`“${preset.referenceName}” is no longer in your references. Upload it again to use this preset.`); return; }
    if (preset.level === 'custom') { setMode('advanced'); remember('aura.colorMatch.mode', 'advanced'); }
    choose(reference, preset.controls);
  };
  const sync = () => void run(async (id, current) => {
    if (!selected) return;
    setOutcomes([]);
    let targets: SyncTarget[];
    if (syncScope === 'selected') {
      const names = new Map(rows.map(row => [row.id, row.fileName]));
      targets = activeProject === projectId ? [...selection].filter(item => item !== photoId)
        .map(item => ({ id: item, fileName: names.get(item) ?? 'Photo' })) : [];
      if (!targets.length) { setMessage('Select photos in the library first, or choose Whole collection in Sync options.'); return; }
    } else {
      setMessage('Finding photos in this collection…');
      targets = (await collectionTargets(projectId, () => !current())).filter(item => item.id !== photoId);
    }
    if (!current()) return;
    await onApply(async () => {
      await syncColorMatch(targets, selected, { ...controls }, id, () => !current(),
        value => { if (current()) setMessage(value); },
        result => { if (current()) setOutcomes(items => [...items, result]); });
    });
  });
  const stop = () => {
    stopped.current = true;
    if (timer.current) { clearTimeout(timer.current); timer.current = null; }
    if (job.current) void api.cancelJob(job.current).catch(cause => setError(asIpcError(cause).message));
  };

  const locked = disabled || busy || !inTauri();
  const matched = report && report.current && report.reference.id === selected && report.photoId === photoId;
  const selectedCount = activeProject === projectId ? [...selection].filter(item => item !== photoId).length : 0;
  const recommendedItems = (recommended ?? []).flatMap(item => {
    const reference = byId.get(item.referenceId);
    return reference ? [{ reference, note: item.reasons.slice(0, 2).join(' · ') || 'Different scene' }] : [];
  });
  const filtered = references.filter(item => `${item.name} ${item.group ?? ''}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()));
  const card = (reference: ColorReference, note?: string, key = reference.id) =>
    <button type="button" key={key} className="cm-card" disabled={locked} aria-pressed={reference.id === selected}
      title={note ? `${reference.name} — ${note}` : reference.name} onClick={() => choose(reference)}>
      {reference.thumbnail ? <img src={reference.thumbnail} alt="" loading="lazy" /> : <span className="cm-card-blank" aria-hidden="true" />}
      <span className="cm-card-name">{reference.favorite ? '★ ' : ''}{reference.name}</span>
      {note && <small>{note}</small>}
      {matched && report.reference.id === reference.id && <span className="cm-card-badge">Matched</span>}
    </button>;

  return <section className="lr-panel color-match-studio" aria-label="AI Color Match" aria-busy={busy}>
    <header className="cm-header">
      <h3>AI Color Match</h3>
      <label className="cm-toggle" title="Match as soon as you choose a reference or a preference">
        <span>Real-time</span>
        <input type="checkbox" role="switch" aria-label="Real-time color adjustments" checked={realtime}
          onChange={event => { setRealtime(event.target.checked); remember('aura.colorMatch.realtime', event.target.checked ? 'on' : 'off'); }} />
      </label>
    </header>
    <p className="lr-hint">Match this photo to the colour and tone of an edited photo. Supported formats: JPEG, PNG, TIFF, from 8 pixels to 64 megapixels. Retouching stays with this photo.</p>
    <div className="cm-actions">
      <button type="button" className="cm-tile" disabled={locked} onClick={upload}><span aria-hidden="true">＋</span>Upload References</button>
      <button type="button" className="cm-tile" disabled={locked} aria-expanded={naming === 'reference'}
        onClick={() => { setNaming(naming === 'reference' ? null : 'reference'); setName('Edited photo'); }}><span aria-hidden="true">⧉</span>Create Reference</button>
    </div>
    {naming && <form className="cm-name" onSubmit={event => { event.preventDefault(); if (name.trim()) { if (naming === 'reference') createReference(); else savePreset(); } }}>
      <label>{naming === 'reference' ? 'Reference name' : 'Preset name'}
        <input maxLength={naming === 'reference' ? 120 : 60} value={name} disabled={locked} autoFocus onChange={event => setName(event.target.value)} /></label>
      <button type="submit" disabled={locked || !name.trim()}>{naming === 'reference' ? 'Save this edit as a reference' : 'Save preset'}</button>
      <button type="button" onClick={() => setNaming(null)}>Cancel</button>
    </form>}
    <label className="cm-row">Mode
      <select aria-label="Matching mode" value={mode} disabled={busy}
        onChange={event => { const value = event.target.value as Mode; setMode(value); remember('aura.colorMatch.mode', value); }}>
        <option value="quick">Quick Mode</option><option value="advanced">Advanced Mode</option>
      </select></label>
    <div className="cm-tabs" role="tablist" aria-label="Reference lists">
      {TABS.map(([value, label]) => <button type="button" role="tab" key={value} aria-selected={tab === value}
        onClick={() => { setTab(value); remember('aura.colorMatch.tab', value); }}>{label}</button>)}
      <button type="button" className="cm-density" aria-pressed={!large} aria-label={large ? 'Show small thumbnails' : 'Show large thumbnails'}
        onClick={() => { setLarge(!large); remember('aura.colorMatch.grid', large ? 'small' : 'large'); }}>{large ? '▦' : '▣'}</button>
    </div>
    <div role="tabpanel" aria-label={TABS.find(([value]) => value === tab)?.[1]} className={`cm-grid${large ? '' : ' cm-grid-small'}`}>
      {tab === 'recommended' && (references.length === 0
        ? <p className="cm-empty">Upload edited photos you like, or save one of your edits with Create Reference. AURA then recommends the best fits for each photo.</p>
        : recommending ? <p className="cm-empty" role="status">Finding references that suit this photo…</p>
          : recommendedItems.length ? recommendedItems.map(({ reference, note }) => card(reference, note))
            : <p className="cm-empty">No recommendation yet. Choose from My References.</p>)}
      {tab === 'mine' && <>
        {references.length > 8 && <label className="cm-search">Find reference<input type="search" value={query} onChange={event => setQuery(event.target.value)} /></label>}
        {filtered.map(reference => card(reference))}
        {!filtered.length && <p className="cm-empty">{references.length ? 'No reference matches this search.' : 'No references yet. Use Upload References or Create Reference.'}</p>}
      </>}
      {tab === 'presets' && (presets.length ? presets.map(preset => {
        const reference = byId.get(preset.referenceId);
        return <div className="cm-preset" key={preset.id}>
          <button type="button" className="cm-card" disabled={locked} onClick={() => applyPreset(preset)}
            aria-label={`Use preset ${preset.name}`} title={`${preset.referenceName} · ${LEVEL_NAMES[preset.level] ?? 'Custom'}`}>
            {reference?.thumbnail ? <img src={reference.thumbnail} alt="" loading="lazy" /> : <span className="cm-card-blank" aria-hidden="true" />}
            <span className="cm-card-name">{preset.name}</span>
            <small>{preset.available ? `${LEVEL_NAMES[preset.level] ?? 'Custom'} · ${preset.referenceName}` : 'Reference removed'}</small>
          </button>
          <button type="button" className="cm-remove" disabled={locked} aria-label={`Remove preset ${preset.name}`} onClick={() => removePreset(preset)}>×</button>
        </div>;
      }) : <p className="cm-empty">Save Preset keeps a reference with its preference so you can reuse it in one click.</p>)}
    </div>
    <label className="cm-row">Color Match Preferences
      <select aria-label="Color match preference" value={level} disabled={locked}
        onChange={event => chooseLevel(event.target.value as PreferenceLevel)}>
        {PREFERENCE_LEVELS.map(([value, label]) => <option key={value} value={value}>{label}</option>)}
        <option value="custom">Custom…</option>
      </select></label>
    {level === 'extreme' && <p className="lr-hint">Extreme goes past the reference. If a photo's highlights or skin would suffer, AURA uses the strongest amount that passes and tells you.</p>}
    {mode === 'advanced' && <fieldset className="cm-strengths" disabled={locked}>
      <legend>Strengths</legend>
      {STRENGTHS.map(([key, label]) => <label key={key}>{label} ({Math.round(controls[key] * 100)}%)
        <input type="range" aria-label={label} min="0" max="200" step="1" value={Math.round(controls[key] * 100)}
          onChange={event => changeStrength(key, Number(event.target.value) / 100)} /></label>)}
      <button type="button" disabled={!selected} onClick={() => match(selected, controls, true)}>Measure again and match</button>
    </fieldset>}
    {(pending || !realtime) && <button type="button" className="cm-primary" disabled={locked || !selected} onClick={() => match()}>Apply match</button>}
    {matched && report.controls.amount > 0 && <button type="button" disabled={locked} onClick={() => match(selected, { ...controls, amount: 0 })}>Remove matched look</button>}
    {busy && <div className="cm-progress" role="status"><span>{message ?? 'Matching colour and checking the full-resolution photo…'}</span>
      <button type="button" onClick={stop}>Stop</button></div>}
    {!busy && message && <p role="status" className="cm-message">{message}</p>}
    {error && <p role="alert">{error}</p>}
    {matched && <p className="cm-measure" data-testid="one-click-measurement">
      {current?.name ?? report.reference.name}: colour distance {report.beforeDistance.toFixed(1)} → {report.afterDistance.toFixed(1)}
      {report.fullResolutionChecked ? ', checked at full resolution.' : '.'}
      {report.protectedFields > 0 && ` ${report.protectedFields} of your own settings were kept.`}
      {isQuick(report) && report.limited && (report.limitedBy === 'highlights' ? ' Limited to protect highlights.'
        : report.limitedBy === 'skin' ? ' Limited to protect skin.' : report.limitedBy === 'distance' ? ' Limited where it stopped getting closer.'
          : ' Limited to protect highlights and skin.')}</p>}
    <footer className="cm-footer">
      <button type="button" disabled={locked || !selected} aria-expanded={naming === 'preset'}
        onClick={() => { setNaming(naming === 'preset' ? null : 'preset'); setName(current ? `${current.name.replace(/\.[a-z0-9]+$/i, '')} · ${LEVEL_NAMES[level]}` : 'Colour match'); }}>Save Preset</button>
      <button type="button" className="cm-primary" disabled={locked || !selected} onClick={sync}
        title={syncScope === 'selected' ? `Match ${selectedCount} selected photo${selectedCount === 1 ? '' : 's'}` : 'Match every photo in this collection'}>
        Sync{syncScope === 'selected' ? ` (${selectedCount})` : ' all'}</button>
      <button type="button" aria-label="Sync options" aria-expanded={syncOptions} onClick={() => setSyncOptions(!syncOptions)}>⚙</button>
    </footer>
    {syncOptions && <fieldset className="cm-sync-options" disabled={busy}>
      <legend>Sync to</legend>
      <label><input type="radio" name={`cm-sync-${photoId}`} checked={syncScope === 'selected'} onChange={() => setSyncScope('selected')} />Selected photos ({selectedCount})</label>
      <label><input type="radio" name={`cm-sync-${photoId}`} checked={syncScope === 'all'} onChange={() => setSyncScope('all')} />Whole collection</label>
      <p className="lr-hint">Each photo is measured on its own and matched to the same reference and preference, then checked at full resolution. Photos you edited by hand keep those settings.</p>
    </fieldset>}
    {outcomes.length > 0 && <details className="cm-outcomes" open={outcomes.some(item => item.outcome === 'failed')}>
      <summary>Sync results ({outcomes.length})</summary>
      <ol aria-label="Sync results">{outcomes.map((item, index) => <li key={`${item.photoId}:${index}`} data-outcome={item.outcome}>
        <strong>{item.name}</strong>: {item.outcome === 'limited' ? 'matched, reduced' : item.outcome}. {item.detail}</li>)}</ol>
    </details>}
    {mode === 'advanced' && <details className="cm-detailed">
      <summary>Detailed matching: regions, fine controls, library</summary>
      <ColorMatchPanel key={`detailed:${photoId}`} projectId={projectId} photoId={photoId} revision={revision} disabled={disabled || busy} onApply={onApply} />
    </details>}
  </section>;
}
