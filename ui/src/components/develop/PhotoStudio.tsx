import { useCallback, useEffect, useMemo, useState } from 'react';
import { api, asIpcError, develop, editProfiles, inTauri, syncSettings, type EditProfile } from '../../ipc/client';
import type { HistoryDto, RecipeDto, RenderDto } from '../../ipc/types';
import { rgbDataUrl } from './rgbImage';
import { LightroomPanel } from './LightroomPanel';

export function PhotoStudio({ projectId, photoId, disabled, revision = 0, onBusyChange }: {
  projectId: string; photoId: string; disabled: boolean; revision?: number; onBusyChange: (busy: boolean) => void;
}): JSX.Element {
  const [recipe, setRecipe] = useState<RecipeDto | null>(null);
  const [history, setHistory] = useState<HistoryDto | null>(null);
  const [render, setRender] = useState<RenderDto | null>(null);
  const [original, setOriginal] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [view, setView] = useState<'edited' | 'original' | 'split'>('edited');
  const [split, setSplit] = useState(50);
  const [refresh, setRefresh] = useState(0);
  const [profiles, setProfiles] = useState<EditProfile[]>([]);
  const [aspect, setAspect] = useState<number | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  useEffect(() => { if (inTauri()) editProfiles.list().then(setProfiles).catch(() => undefined); }, []);
  useEffect(() => { onBusyChange(busy); }, [busy, onBusyChange]);
  const edited = useMemo(() => render ? rgbDataUrl(render) : null, [render]);
  useEffect(() => {
    let active = true;
    setError(null); setRender(null); setRecipe(null); setHistory(null); setOriginal(null);
    if (!inTauri()) return;
    void Promise.all([
      develop.imageRecipe({ photoId }), develop.imageHistory({ photoId }),
      develop.renderImage({ photoId, level: 'screen', screen: [1400, 1000], purpose: 'interactive' }),
      api.getPreview({ projectId, photoId, level: 'proxy', priority: 'interactive' }),
    ]).then(([nextRecipe, nextHistory, nextRender, preview]) => {
      if (!active) return;
      setRecipe(nextRecipe); setHistory(nextHistory); setRender(nextRender); setOriginal(preview.dataUrl);
    }).catch(cause => { if (active) setError(asIpcError(cause).message); });
    return () => { active = false; };
  }, [photoId, projectId, refresh, revision]);

  const write = useCallback(async (action: () => Promise<unknown>) => {
    setBusy(true); setError(null);
    try { await action(); setRefresh(value => value + 1); }
    catch (cause) { setError(asIpcError(cause).message); }
    finally { setBusy(false); }
  }, []);

  return <section className="photo-studio" aria-label="Photo editor">
    <div className="studio-toolbar">
      <div><span className="eyebrow">PHOTO STUDIO</span><h2>Make it yours.</h2></div>
      <div className="view-switch" aria-label="Preview mode">
        {(['edited', 'original', 'split'] as const).map(mode => <button type="button" key={mode} aria-pressed={view === mode} disabled={!edited || !original} onClick={() => setView(mode)}>{mode === 'split' ? 'Compare' : mode === 'edited' ? 'Edited' : 'Original'}</button>)}
      </div>
    </div>
    {error && <p role="alert">{error} <button type="button" onClick={() => setRefresh(value => value + 1)}>Retry preview</button></p>}
    <div className="studio-layout">
      <div>
        <div className="studio-canvas">
          {edited && original ? <>
            <img src={view === 'original' ? original : edited} alt={view === 'original' ? 'Original photograph' : 'Edited photograph'} />
            <img src={original} alt="" hidden onLoad={event => { const img = event.currentTarget; if (img.naturalHeight) setAspect(img.naturalWidth / img.naturalHeight); }} />
            {view === 'split' && <img className="studio-original-overlay" src={original} alt="Original side of comparison" style={{ clipPath: `inset(0 ${100 - split}% 0 0)` }} />}
            {view === 'split' && <div className="studio-divider" style={{ left: `${split}%` }} aria-hidden="true" />}
            <span className="studio-caption">{view === 'split' ? 'Original / Edited' : view === 'original' ? 'Original' : 'Edited'}</span>
          </> : <p>{error ? 'Photo preview unavailable' : 'Loading your photograph…'}</p>}
        </div>
        {view === 'split' && <label className="compare-control">Before / after<input type="range" min={0} max={100} value={split} onChange={event => setSplit(Number(event.target.value))} aria-label="Before and after divider" /></label>}
        <p className="studio-footnote">Edits are saved automatically. Your original stays untouched.</p>
        {render?.notes.filter(note => note.isCaveat).map(note => <p className="studio-footnote" key={`${note.stage}:${note.reason}`}>{note.detail ?? note.reason}</p>)}
      </div>
      <fieldset className="studio-adjustments lr-adjustments" disabled={disabled || busy || !recipe}>
        <legend>Develop</legend>
        <p>Every Lightroom panel. <strong>Auto</strong> measures this photo and edits it in one click; anything you move stays yours.</p>
        {notice && <p role="status" className="lr-notice">{notice}</p>}
        <LightroomPanel recipe={recipe} disabled={disabled || busy || !recipe} aspect={aspect} profiles={profiles}
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
