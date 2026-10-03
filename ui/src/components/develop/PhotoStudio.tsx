import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { api, asIpcError, develop, editProfiles, inTauri, syncSettings, pickWhiteBalance, type EditProfile } from '../../ipc/client';
import type { HistoryDto, RecipeDto, RenderDto } from '../../ipc/types';
import { rgbDataUrl } from './rgbImage';
import { LightroomPanel } from './LightroomPanel';
import { Histogram } from './Histogram';
import { clippingPreview, imagePoint, type ClippingMode } from './previewTools';
import { SnapshotPanel } from './SnapshotPanel';
import { SyncSettingsPanel } from './SyncSettingsPanel';
import { NativeRetouchWorkspace } from './NativeRetouchWorkspace';
import { PortraitAutoReport } from './PortraitAutoReport';

export function PhotoStudio({ projectId, photoId, disabled, revision = 0, onBusyChange }: {
  projectId: string; photoId: string; disabled: boolean; revision?: number; onBusyChange: (busy: boolean) => void;
}): JSX.Element {
  const [recipe, setRecipe] = useState<RecipeDto | null>(null);
  const [history, setHistory] = useState<HistoryDto | null>(null);
  const [render, setRender] = useState<RenderDto | null>(null);
  const [original, setOriginal] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [retouchOpen, setRetouchOpen] = useState(false);
  const writing = useRef(false);
  const [loading, setLoading] = useState(true);
  const [mode, setMode] = useState<'essentials' | 'advanced'>('essentials');
  const [view, setView] = useState<'edited' | 'original' | 'split'>('edited');
  const [split, setSplit] = useState(50);
  const [refresh, setRefresh] = useState(0);
  const [profiles, setProfiles] = useState<EditProfile[]>([]);
  const [aspect, setAspect] = useState<number | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [clipping, setClipping] = useState<ClippingMode>('off');
  const [picking, setPicking] = useState(false);
  const [pickX, setPickX] = useState(50);
  const [pickY, setPickY] = useState(50);
  // Which history step is applied, when this view moved there; null means unknown or head.
  const [position, setPosition] = useState<number | null>(null);
  useEffect(() => {
    let active = true;
    if (inTauri()) editProfiles.list().then(value => { if (active) setProfiles(value); })
      .catch(cause => { if (active) setNotice(`Presets unavailable: ${asIpcError(cause).message}`); });
    return () => { active = false; };
  }, []);
  useEffect(() => { if (!retouchOpen) onBusyChange(busy || loading); }, [busy, loading, onBusyChange, retouchOpen]);
  useEffect(() => () => onBusyChange(false), [onBusyChange]);
  const edited = useMemo(() => render ? rgbDataUrl(render) : null, [render]);
  const proof = useMemo(() => render && clipping !== 'off' ? clippingPreview(render, clipping) : edited, [render, edited, clipping]);
  const problem = error ?? (render && !edited ? 'The renderer returned incomplete image data. Retry the preview.' : null);
  // Clear only when changing photographs; keep the last preview during a save.
  useEffect(() => {
    setRender(null); setRecipe(null); setHistory(null); setOriginal(null); setAspect(null); setNotice(null);
    setPicking(false); setPosition(null);
  }, [photoId, projectId]);
  useEffect(() => {
    let active = true;
    if (retouchOpen) { setLoading(false); return; }
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
  }, [photoId, projectId, refresh, revision, retouchOpen]);

  const write = useCallback(async (action: () => Promise<unknown>) => {
    if (writing.current) return;
    writing.current = true;
    setBusy(true); setError(null);
    try { await action(); setRefresh(value => value + 1); }
    catch (cause) { setError(asIpcError(cause).message); }
    finally { writing.current = false; setBusy(false); }
  }, []);

  const pick = (x: number, y: number) => {
    if (disabled || busy || loading || !recipe || problem) return;
    void write(async () => {
      await pickWhiteBalance(projectId, photoId, x, y);
      setPicking(false); setView('edited'); setNotice('White balance saved as one manual edit. Use Undo to compare.');
    });
  };

  if (retouchOpen) return <NativeRetouchWorkspace key={`${projectId}:${photoId}`} projectId={projectId} photoId={photoId} disabled={disabled} revision={revision} onBusyChange={onBusyChange} onClose={() => setRetouchOpen(false)} />;

  return <section className="photo-studio" aria-label="Photo editor">
    <div className="studio-toolbar">
      <div><span className="eyebrow">PHOTO STUDIO</span><h2>Make it yours.</h2></div>
      <div className="view-switch" aria-label="Preview mode">
        {(['edited', 'original', 'split'] as const).map(mode => <button type="button" key={mode} aria-pressed={view === mode} disabled={!edited || !original} onClick={() => { setView(mode); setPicking(false); }}>{mode === 'split' ? 'Compare' : mode === 'edited' ? 'Edited' : 'Original'}</button>)}
      </div>
    </div>
    {problem && <p role="alert">{problem} <button type="button" onClick={() => setRefresh(value => value + 1)}>Retry preview</button></p>}
    <div className="studio-layout">
      <div>
        <div className={`studio-canvas${picking ? ' studio-picking' : ''}`} aria-busy={busy || loading}
          onClick={event => {
            if (!picking || !aspect) return;
            const box = event.currentTarget.getBoundingClientRect();
            const point = imagePoint(event.clientX - box.left, event.clientY - box.top, box.width, box.height, aspect);
            if (point) pick(...point);
          }}>
          {edited && original ? <>
            <img src={view === 'original' ? original : proof ?? edited} alt={view === 'original' ? 'Original photograph' : 'Edited photograph'} />
            <img src={original} alt="" hidden onLoad={event => { const img = event.currentTarget; if (img.naturalHeight) setAspect(img.naturalWidth / img.naturalHeight); }} />
            {view === 'split' && <img className="studio-original-overlay" src={original} alt="Original side of comparison" style={{ clipPath: `inset(0 ${100 - split}% 0 0)` }} />}
            {view === 'split' && <div className="studio-divider" style={{ left: `${split}%` }} aria-hidden="true" />}
            <span className="studio-caption">{view === 'split' ? 'Original / Edited' : view === 'original' ? 'Original' : 'Edited'}</span>
            {(busy || loading) && <span className="studio-updating" role="status">Updating preview…</span>}
          </> : <p>{problem ? 'Photo preview unavailable' : 'Loading your photograph…'}</p>}
        </div>
        {view === 'split' && <label className="compare-control">Before / after<input type="range" min={0} max={100} value={split} onChange={event => setSplit(Number(event.target.value))} aria-label="Before and after divider" /></label>}
        <label className="compare-control">Clipping warnings<select value={clipping} onChange={event => setClipping(event.target.value as ClippingMode)}>
          <option value="off">Off</option><option value="shadows">Shadows</option><option value="highlights">Highlights</option><option value="both">Both</option>
        </select></label>
        {clipping !== 'off' && <p className="lr-hint">Edited preview only: blue/black hatching marks near-black pixels; red/white marks a near-clipped channel. Exports are unaffected.</p>}
        <p className="studio-footnote" role="status">{busy ? 'Saving your edit…' : loading ? 'Rendering your photograph…' : problem ? 'Preview needs attention.' : 'Edits saved. Your original stays untouched.'}{render && ` · Preview ${render.width} × ${render.height}`}</p>
        {render?.notes.filter(note => note.isCaveat).map(note => <p className="studio-footnote" key={`${note.stage}:${note.reason}`}>{note.detail ?? note.reason}</p>)}
      </div>
      <fieldset className="studio-adjustments lr-adjustments" disabled={disabled || busy || loading || !recipe || Boolean(problem)}>
        <legend>Develop</legend>
        <div className="studio-edit-modes" aria-label="Editing controls">
          <button type="button" onClick={() => { setRetouchOpen(true); setPicking(false); }}>Retouch</button>
          {(['essentials', 'advanced'] as const).map(value => <button type="button" key={value} aria-pressed={mode === value} onClick={() => setMode(value)}>{value === 'essentials' ? 'Essentials' : 'Advanced'}</button>)}
        </div>
        <p><strong>Auto enhance</strong> measures light, colour, noise and the scene, detects faces, then retouches skin, heals temporary blemishes and finishes eyes and teeth where it measures a need. Each stage is saved as its own step, so you can go back to any of them and edit by hand; your manual adjustments are protected.</p>
        <PortraitAutoReport recipe={recipe}/>
        {render && edited && <Histogram render={render} />}
        {notice && <p role="status" className="lr-notice">{notice}</p>}
        <details className="lr-section"><summary>White balance picker</summary><div className="lr-section-body" onKeyDown={event => { if (event.key === 'Escape') setPicking(false); }}>
          <p className="lr-hint">Choose a neutral gray or white midtone in the original. Avoid clipped highlights. Click the photograph or enter a position below.</p>
          <button type="button" aria-pressed={picking} onClick={() => { setPicking(!picking); setView('original'); }}>{picking ? 'Cancel neutral picker' : 'Pick neutral area'}</button>
          <label>Horizontal position (%)<input type="number" min={0} max={100} value={pickX} onChange={event => setPickX(Number(event.target.value))} /></label>
          <label>Vertical position (%)<input type="number" min={0} max={100} value={pickY} onChange={event => setPickY(Number(event.target.value))} /></label>
          <button type="button" disabled={![pickX, pickY].every(value => Number.isFinite(value) && value >= 0 && value <= 100)} onClick={() => pick(pickX / 100, pickY / 100)}>Sample this position</button>
        </div></details>
        <LightroomPanel recipe={recipe} disabled={disabled || busy || loading || !recipe || Boolean(problem)} aspect={aspect} profiles={profiles} mode={mode}
          syncControls={<SyncSettingsPanel key={`${projectId}:${photoId}`} projectId={projectId} photoId={photoId} disabled={disabled || busy || loading}
            onSync={(targets, groups) => void write(async () => {
              const report = await syncSettings(projectId, photoId, targets, false, groups);
              setNotice(`Settings synced to ${report.synced} photos.${report.failed.length ? ` Failed: ${report.failed.join('; ')}` : ''}`);
            })} />}
          onSetParam={(path, value, label) => void write(() => develop.setParam({ projectId, photoId, path, value, label }))}
          onAuto={() => void write(async () => { await develop.enhancePhoto({ photoId }); setPosition(null); })}
          onApplyProfile={(profileId, strength) => void write(async () => {
            const report = await editProfiles.apply(photoId, profileId, strength);
            setNotice(report.adaptations.length ? report.adaptations.join(' ') : `Applied ${profiles.find(p => p.id === profileId)?.name ?? profileId}.`);
          })}
          onSync={includeGeometry => void write(async () => {
            const report = await syncSettings(projectId, photoId, [], includeGeometry);
            setNotice(`Settings synced to ${report.synced} photo${report.synced === 1 ? '' : 's'}.${report.failed.length ? ` ${report.failed.length} could not be updated.` : ''}`);
          })} />
        <div className="studio-history">
          <button type="button" disabled={!history?.canUndo} onClick={() => void write(async () => { await develop.historyStep({ projectId, photoId, action: 'undo' }); setPosition(null); })}>Undo</button>
          <button type="button" disabled={!history?.canRedo} onClick={() => void write(async () => { await develop.historyStep({ projectId, photoId, action: 'redo' }); setPosition(null); })}>Redo</button>
          <button type="button" onClick={() => void write(() => develop.historyStep({ projectId, photoId, action: 'reset_original' }))}>Reset photo</button>
        </div>
        <SnapshotPanel key={photoId} names={history?.snapshots ?? []} disabled={disabled || busy || loading}
          onTake={name => void write(() => develop.snapshot({ projectId, photoId, action: 'take', name }))}
          onRestore={name => void write(() => develop.snapshot({ projectId, photoId, action: 'restore', name }))} />
      </fieldset>
    </div>
    {history && <details className="advanced-tools" open={history.entries.some(entry => entry.source !== 'user')}><summary>Review every edit on this photo ({history.entries.length})</summary>
      {history.entries.length === 0 ? <p>No edits have been saved yet.</p> : <ol className="photo-edit-history">
        <li key="original" aria-current={position === 0 ? 'step' : undefined}><strong>Original photograph</strong>
          <button type="button" disabled={disabled || busy || loading} onClick={() => void write(async () => { await develop.historyStep({ projectId, photoId, action: 'goto:0' }); setPosition(0); })}>Go back to here</button></li>
        {history.entries.map(entry => <li key={entry.seq} aria-current={(position ?? (history.canRedo ? null : history.entries[history.entries.length - 1]?.seq)) === entry.seq ? 'step' : undefined}>
        <strong>{entry.label}</strong><p>{new Date(entry.atMs).toLocaleString()} · {entry.source === 'user' ? 'Your edit' : 'Automatic edit'}</p>
        <p>Changed: {entry.changed.join(', ') || 'No parameter changes'}</p>
        <button type="button" disabled={disabled || busy || loading} aria-label={`Go back to step ${entry.seq}: ${entry.label}`}
          onClick={() => void write(async () => { await develop.historyStep({ projectId, photoId, action: `goto:${entry.seq}` }); setPosition(entry.seq); })}>Go back to here</button>
      </li>)}</ol>}
      <p>Use Undo and Redo, or “Go back to here”, to move through saved edits. Going back discards nothing: Redo still moves forward, and a new edit continues from the step you chose.</p>
    </details>}
  </section>;
}
