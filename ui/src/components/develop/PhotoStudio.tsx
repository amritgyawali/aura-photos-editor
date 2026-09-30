import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { api, asIpcError, develop, editProfiles, inTauri, syncSettings, type EditProfile } from '../../ipc/client';
import type { HistoryDto, RecipeDto, RenderDto } from '../../ipc/types';
import { rgbDataUrl } from './rgbImage';
import { LightroomPanel } from './LightroomPanel';
import { Histogram } from './Histogram';

export function PhotoStudio({ projectId, photoId, disabled, revision = 0, onBusyChange }: {
  projectId: string; photoId: string; disabled: boolean; revision?: number; onBusyChange: (busy: boolean) => void;
}): JSX.Element {
  const [recipe, setRecipe] = useState<RecipeDto | null>(null);
  const [history, setHistory] = useState<HistoryDto | null>(null);
  const [render, setRender] = useState<RenderDto | null>(null);
  const [original, setOriginal] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const writing = useRef(false);
  const [loading, setLoading] = useState(true);
  const [mode, setMode] = useState<'essentials' | 'advanced'>('essentials');
  const [view, setView] = useState<'edited' | 'original' | 'split'>('edited');
  const [split, setSplit] = useState(50);
  const [refresh, setRefresh] = useState(0);
  const [profiles, setProfiles] = useState<EditProfile[]>([]);
  const [aspect, setAspect] = useState<number | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  useEffect(() => {
    let active = true;
    if (inTauri()) editProfiles.list().then(value => { if (active) setProfiles(value); })
      .catch(cause => { if (active) setNotice(`Presets unavailable: ${asIpcError(cause).message}`); });
    return () => { active = false; };
  }, []);
  useEffect(() => { onBusyChange(busy || loading); }, [busy, loading, onBusyChange]);
  useEffect(() => () => onBusyChange(false), [onBusyChange]);
  const edited = useMemo(() => render ? rgbDataUrl(render) : null, [render]);
  const problem = error ?? (render && !edited ? 'The renderer returned incomplete image data. Retry the preview.' : null);
  // Clear only when changing photographs; keep the last preview during a save.
  useEffect(() => {
    setRender(null); setRecipe(null); setHistory(null); setOriginal(null); setAspect(null); setNotice(null);
  }, [photoId, projectId]);
  useEffect(() => {
    let active = true;
    setError(null); setLoading(true);
    if (!inTauri()) {
      setError('Open the AURA desktop app to edit this photograph.'); setLoading(false); return;
    }
    void Promise.all([
      develop.imageRecipe({ photoId }), develop.imageHistory({ photoId }),
      develop.renderImage({ photoId, level: 'screen', screen: [1400, 1000], purpose: 'interactive' }),
      api.getPreview({ projectId, photoId, level: 'proxy', priority: 'interactive' }),
    ]).then(([nextRecipe, nextHistory, nextRender, preview]) => {
      if (!active) return;
      setRecipe(nextRecipe); setHistory(nextHistory); setRender(nextRender); setOriginal(preview.dataUrl);
    }).catch(cause => { if (active) setError(asIpcError(cause).message); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [photoId, projectId, refresh, revision]);

  const write = useCallback(async (action: () => Promise<unknown>) => {
    if (writing.current) return;
    writing.current = true;
    setBusy(true); setError(null);
    try { await action(); setRefresh(value => value + 1); }
    catch (cause) { setError(asIpcError(cause).message); }
    finally { writing.current = false; setBusy(false); }
  }, []);

  return <section className="photo-studio" aria-label="Photo editor">
    <div className="studio-toolbar">
      <div><span className="eyebrow">PHOTO STUDIO</span><h2>Make it yours.</h2></div>
      <div className="view-switch" aria-label="Preview mode">
        {(['edited', 'original', 'split'] as const).map(mode => <button type="button" key={mode} aria-pressed={view === mode} disabled={!edited || !original} onClick={() => setView(mode)}>{mode === 'split' ? 'Compare' : mode === 'edited' ? 'Edited' : 'Original'}</button>)}
      </div>
    </div>
    {problem && <p role="alert">{problem} <button type="button" onClick={() => setRefresh(value => value + 1)}>Retry preview</button></p>}
    <div className="studio-layout">
      <div>
        <div className="studio-canvas" aria-busy={busy || loading}>
          {edited && original ? <>
            <img src={view === 'original' ? original : edited} alt={view === 'original' ? 'Original photograph' : 'Edited photograph'} />
            <img src={original} alt="" hidden onLoad={event => { const img = event.currentTarget; if (img.naturalHeight) setAspect(img.naturalWidth / img.naturalHeight); }} />
            {view === 'split' && <img className="studio-original-overlay" src={original} alt="Original side of comparison" style={{ clipPath: `inset(0 ${100 - split}% 0 0)` }} />}
            {view === 'split' && <div className="studio-divider" style={{ left: `${split}%` }} aria-hidden="true" />}
            <span className="studio-caption">{view === 'split' ? 'Original / Edited' : view === 'original' ? 'Original' : 'Edited'}</span>
            {(busy || loading) && <span className="studio-updating" role="status">Updating preview…</span>}
          </> : <p>{problem ? 'Photo preview unavailable' : 'Loading your photograph…'}</p>}
        </div>
        {view === 'split' && <label className="compare-control">Before / after<input type="range" min={0} max={100} value={split} onChange={event => setSplit(Number(event.target.value))} aria-label="Before and after divider" /></label>}
        <p className="studio-footnote" role="status">{busy ? 'Saving your edit…' : loading ? 'Rendering your photograph…' : problem ? 'Preview needs attention.' : 'Edits saved. Your original stays untouched.'}{render && ` · Preview ${render.width} × ${render.height}`}</p>
        {render?.notes.filter(note => note.isCaveat).map(note => <p className="studio-footnote" key={`${note.stage}:${note.reason}`}>{note.detail ?? note.reason}</p>)}
      </div>
      <fieldset className="studio-adjustments lr-adjustments" disabled={disabled || busy || loading || !recipe || Boolean(problem)}>
        <legend>Develop</legend>
        <div className="studio-edit-modes" aria-label="Editing controls">
          {(['essentials', 'advanced'] as const).map(value => <button type="button" key={value} aria-pressed={mode === value} onClick={() => setMode(value)}>{value === 'essentials' ? 'Essentials' : 'Advanced'}</button>)}
        </div>
        <p>Start with <strong>Auto enhance</strong>, then fine-tune. Automatic edits respect your manual adjustments.</p>
        {render && edited && <Histogram render={render} />}
        {notice && <p role="status" className="lr-notice">{notice}</p>}
        <LightroomPanel recipe={recipe} disabled={disabled || busy || loading || !recipe || Boolean(problem)} aspect={aspect} profiles={profiles} mode={mode}
          onSetParam={(path, value, label) => void write(() => develop.setParam({ projectId, photoId, path, value, label }))}
          onAuto={() => void write(() => develop.enhancePhoto({ photoId }))}
          onApplyProfile={(profileId, strength) => void write(async () => {
            const report = await editProfiles.apply(photoId, profileId, strength);
            setNotice(report.adaptations.length ? report.adaptations.join(' ') : `Applied ${profiles.find(p => p.id === profileId)?.name ?? profileId}.`);
          })}
          onSync={includeGeometry => void write(async () => {
            const report = await syncSettings(projectId, photoId, [], includeGeometry);
            setNotice(`Settings synced to ${report.synced} photo${report.synced === 1 ? '' : 's'}.${report.failed.length ? ` ${report.failed.length} could not be updated.` : ''}`);
          })} />
        <div className="studio-history">
          <button type="button" disabled={!history?.canUndo} onClick={() => void write(() => develop.historyStep({ projectId, photoId, action: 'undo' }))}>Undo</button>
          <button type="button" disabled={!history?.canRedo} onClick={() => void write(() => develop.historyStep({ projectId, photoId, action: 'redo' }))}>Redo</button>
          <button type="button" onClick={() => void write(() => develop.historyStep({ projectId, photoId, action: 'reset_original' }))}>Reset photo</button>
        </div>
      </fieldset>
    </div>
    {history && <details className="advanced-tools"><summary>Review every edit on this photo ({history.entries.length})</summary>
      {history.entries.length === 0 ? <p>No edits have been saved yet.</p> : <ol className="photo-edit-history">{history.entries.map(entry => <li key={entry.seq}>
        <strong>{entry.label}</strong><p>{new Date(entry.atMs).toLocaleString()} · {entry.source === 'user' ? 'Your edit' : 'Automatic edit'}</p>
        <p>Changed: {entry.changed.join(', ') || 'No parameter changes'}</p>
      </li>)}</ol>}
      <p>Use Undo and Redo to move through saved edits and compare the result.</p>
    </details>}
  </section>;
}
