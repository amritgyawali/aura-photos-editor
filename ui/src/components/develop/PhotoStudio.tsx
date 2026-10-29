import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { api, asIpcError, develop, editProfiles, inTauri, syncSettings, pickWhiteBalance, type EditProfile } from '../../ipc/client';
import type { HistoryDto, RecipeDto, RenderDto } from '../../ipc/types';
import { nativeRetouch } from '../../ipc/nativeRetouch';
import { previewSource, useProgressivePreview } from '../../state/previewCache';
import { LightroomPanel } from './LightroomPanel';
import { Histogram } from './Histogram';
import { clippingPreview, imagePoint, type ClippingMode } from './previewTools';
import { SnapshotPanel } from './SnapshotPanel';
import { SyncSettingsPanel } from './SyncSettingsPanel';
import { NativeRetouchWorkspace } from './NativeRetouchWorkspace';
import { PortraitAutoReport } from './PortraitAutoReport';
import { PhotoPassPanel } from './PhotoPassPanel';
import { photoPass } from '../../ipc/photoPass';
import { MasksPanel, type BrushSettings, type MaskTool } from './MasksPanel';
import { coverageImageUrl } from './rgbImage';
import { PairedReference } from '../look/PairedReference';
import { ColorMatchPanel } from '../look/ColorMatchPanel';
import { ReferenceComparison } from '../look/ReferenceComparison';
import type { ColorReference } from '../../ipc/colorMatch';
import { useStore } from '../../state/store';
import { localMasks, masksOf, type MaskCoverage, type MaskSource } from '../../ipc/localMasks';
import type { BrushStroke } from '../../ipc/nativeRetouch';
import { PortraitFinishPanel, type LiquifyTool } from './PortraitFinishPanel';
import { finishOf, studioFinish, type StudioFinish } from '../../ipc/studioFinish';
import { AutomaticFinishControls } from './AutomaticFinishControls';
import { StudioColorPanel } from './StudioColorPanel';
import { MaterialLabPanel } from './MaterialLabPanel';
import { SpatialLabPanel } from './SpatialLabPanel';
import { ColorLabPanel } from './ColorLabPanel';
import { WorkflowLabPanel } from './WorkflowLabPanel';
import { PhotoAnalysisPanel } from './PhotoAnalysisPanel';
import { FramingPanel } from './FramingPanel';
import { EvotoWorkflowPanel } from './EvotoWorkflowPanel';
import { evotoWorkflow } from '../../ipc/evotoWorkflow';
import { probePixel } from './framingMath';
import { framingWorkflows } from '../../ipc/framingWorkflows';
import { maskWorkflows } from '../../ipc/maskWorkflows';
import './editorPanels.css';

const EDITOR_PANELS = [
  ['all', 'All tools'], ['develop', 'Color & light'], ['portrait', 'Portrait studio'],
  ['masks', 'Masks'], ['references', 'Reference matching'], ['finish', 'Automatic finish'], ['history', 'History'],
  ['materials', 'Material & texture'], ['inspect', 'Photo inspection'],
  ['spatial', 'Spatial repair'], ['colorlab', 'Colour lab'], ['review', 'Review & checkpoints'],
  ['palette', 'Palette & tone review'], ['detailreview', 'Detail review'], ['composition', 'Composition review'],
  ['deliveryreview', 'Delivery statistics'], ['surfacereview', 'Surface review'], ['lightingreview', 'Lighting geometry'],
  ['texturefidelity', 'Texture fidelity'], ['colourstability', 'Colour stability'], ['deliveryintegrity', 'Delivery integrity'],
] as const;
type EditorPanel = typeof EDITOR_PANELS[number][0];
import { useEffectRequest } from '../../state/effectRequest';

/** The crop a recipe shows, as normalised edges of the whole frame. */
function cropOf(recipe: RecipeDto | null): [number, number, number, number] {
  try {
    const crop = (JSON.parse(recipe?.body ?? '{}') as { geometry?: { crop?: number[] } }).geometry?.crop;
    if (Array.isArray(crop) && crop.length === 4 && crop.every(Number.isFinite)) return crop as [number, number, number, number];
  } catch { /* an unreadable recipe shows the whole frame */ }
  return [0, 0, 1, 1];
}

export function PhotoStudio({ projectId, photoId, photoIds, disabled, revision = 0, referenceMode = false, featureMode = null, active = true, onBusyChange }: {
  projectId: string; photoId: string; photoIds?: readonly string[]; disabled: boolean; revision?: number; referenceMode?: boolean; onBusyChange: (busy: boolean) => void;
  featureMode?: 'liquify' | 'skin' | 'glare' | null;
  active?: boolean;
}): JSX.Element {
  const [recipe, setRecipe] = useState<RecipeDto | null>(null);
  const [history, setHistory] = useState<HistoryDto | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [writeError, setWriteError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [retouchOpen, setRetouchOpen] = useState(false);
  const inRetouch = retouchOpen || featureMode === 'skin' || featureMode === 'glare';
  const [panel, setPanel] = useState<EditorPanel>('all');
  const [comparisonReference, setComparisonReference] = useState<ColorReference | null>(null);
  const selection = useStore(state => state.selection);
  const selectedPhotoIds = useMemo(() => [...selection], [selection]);
  const referenceWorkspace = referenceMode || (featureMode === null && panel === 'references');
  const shows = (value: EditorPanel) => referenceMode ? value === 'references' : featureMode === 'liquify' ? value === 'portrait' : panel === 'all' || panel === value;
  // A tool chosen in the Effects library opens Retouch, which then picks the tool up. ADR-0135.
  const effectRequest = useEffectRequest(state => state.request);
  useEffect(() => { if (effectRequest && !referenceMode && featureMode === null) setRetouchOpen(true); }, [effectRequest, referenceMode, featureMode]);
  const writing = useRef(false);
  const [loading, setLoading] = useState(true);
  const [mode, setMode] = useState<'essentials' | 'advanced'>('essentials');
  const [view, setView] = useState<'edited' | 'original' | 'split' | 'color'>('edited');
  const [colorPreview, setColorPreview] = useState<RenderDto | null>(null);
  const [colorError, setColorError] = useState<string | null>(null);
  const [probing, setProbing] = useState(false);
  const [probe, setProbe] = useState<ReturnType<typeof probePixel> | null>(null);
  const maskJob = useRef<string | null>(null);
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
  // Masking (ADR-0102): the selected mask, the drawing tool, the brush and the overlay.
  const [selectedMask, setSelectedMask] = useState<string | null>(null);
  const [maskMessage, setMaskMessage] = useState<string | null>(null);
  const [maskTool, setMaskTool] = useState<MaskTool>(null);
  const [brush, setBrush] = useState<BrushSettings>({ size: 0.04, feather: 0.5, flow: 1, erase: false });
  const [maskOverlay, setMaskOverlay] = useState(true);
  const [coverage, setCoverage] = useState<MaskCoverage | null>(null);
  const drag = useRef<{ start: [number, number]; box: [number, number]; points: [number, number][]; erase: boolean } | null>(null);
  const [sketch, setSketch] = useState<[number, number][] | null>(null);
  // Evoto-style finishing (ADR-0108): the liquify brush, when one is chosen.
  const [liquifyTool, setLiquifyTool] = useState<LiquifyTool>(null);
  useEffect(() => {
    if (featureMode !== null) setRetouchOpen(false);
    setLiquifyTool(null); setMaskTool(null); setView('edited');
    drag.current = null; setSketch(null);
  }, [featureMode]);
  useEffect(() => {
    if (!referenceMode) return;
    setRetouchOpen(false); setPicking(false); setMaskTool(null); setLiquifyTool(null);
    setProbing(false); drag.current = null; setSketch(null);
  }, [referenceMode]);
  useEffect(() => {
    let active = true;
    if (inTauri()) editProfiles.list().then(value => { if (active) setProfiles(value); })
      .catch(cause => { if (active) setNotice(`Presets unavailable: ${asIpcError(cause).message}`); });
    return () => { active = false; };
  }, []);
  useEffect(() => () => onBusyChange(false), [onBusyChange]);
  // The photograph at its own resolution, edited and as taken, cached for the session and on
  // disk (ADR-0097): a fast first look while the full-quality preview is made.
  const [loaded, setLoaded] = useState(0);
  const version = recipe?.photoId === photoId && recipe.recipeHash ? recipe.recipeHash : loaded ? `load-${loaded}` : null;
  const editedPreview = useProgressivePreview(version && !inRetouch ? `${projectId}:${photoId}:edited:${version}` : null, photoId,
    quality => develop.renderImage(quality === 'full' ? { photoId, level: 'full', purpose: 'interactive' }
      : { photoId, level: 'screen', screen: [1600, 1600], purpose: 'interactive' }), refresh,
    () => nativeRetouch.live(projectId, photoId, 'edited'));
  const originalPreview = useProgressivePreview(version && !inRetouch ? `${projectId}:${photoId}:original` : null, photoId,
    quality => nativeRetouch.original(projectId, photoId, quality), refresh);
  // Edits wait for the first look at a new version; the full-quality one follows unblocked.
  const waiting = Boolean(version) && !inRetouch && !editedPreview.current && !editedPreview.error;
  // Only a save in flight holds the rest of the app. Loading or waiting for a preview disables
  // this editor's own controls (below) and nothing else: the editor stays mounted behind every
  // other screen, and a slow camera file once locked the whole window while the Cull ran.
  useEffect(() => { if (!inRetouch) onBusyChange(busy); }, [busy, onBusyChange, inRetouch]);
  const render: RenderDto | null = editedPreview.image;
  const edited = useMemo(() => previewSource(render), [render]);
  const original = useMemo(() => previewSource(originalPreview.image), [originalPreview.image]);
  const displayedRender = view === 'color' ? colorPreview : view === 'original' ? originalPreview.image : render;
  const colorSource = useMemo(() => previewSource(colorPreview), [colorPreview]);
  const proof = useMemo(() => displayedRender && clipping !== 'off' ? clippingPreview(displayedRender, clipping) : view === 'color' ? colorSource : edited, [displayedRender, edited, clipping, view, colorSource]);
  useEffect(() => {
    if (view !== 'color' || !recipe?.recipeHash) { setColorPreview(null); setColorError(null); return; }
    let active = true; setColorPreview(null); setColorError(null);
    void framingWorkflows.colorOnly(projectId, photoId, recipe.recipeHash).then(value => { if (active) setColorPreview(value); })
      .catch(cause => { if (active) setColorError(asIpcError(cause).message); });
    return () => { active = false; };
  }, [view, projectId, photoId, recipe?.recipeHash]);
  useEffect(() => () => { const id = maskJob.current; if (id) void api.cancelJob(id).catch(() => undefined); }, [photoId]);
  const problem = error ?? editedPreview.error ?? (render && !edited ? 'The renderer returned incomplete image data. Retry the preview.' : null);
  // Clear only when changing photographs; keep the last preview during a save.
  useEffect(() => {
    setRecipe(null); setHistory(null); setAspect(null); setNotice(null);
    setWriteError(null);
    setProbe(null); setProbing(false); setColorPreview(null);
    setPicking(false); setPosition(null);
    setSelectedMask(null); setMaskMessage(null); setMaskTool(null); setCoverage(null); setLiquifyTool(null);
  }, [photoId, projectId]);
  const masks = useMemo(() => masksOf(recipe), [recipe]);
  const finish = useMemo(() => finishOf(recipe), [recipe]);
  // What the last finishing save could not do here (people tools with no face), for the panel.
  const [finishNotes, setFinishNotes] = useState<string[]>([]);
  const saveFinish = (next: StudioFinish, label: string) => void write(async () => {
    const saved = await studioFinish.save(projectId, photoId, next, label);
    setFinishNotes(saved?.notes ?? []);
  });
  const crop = useMemo(() => cropOf(recipe), [recipe]);
  // The selected mask's coverage, for the red overlay, refreshed with every saved change.
  useEffect(() => {
    let active = true;
    if (!selectedMask || !maskOverlay || !inTauri() || !masks.some(m => m.id === selectedMask)) { setCoverage(null); return; }
    localMasks.coverage(projectId, photoId, selectedMask).then(value => { if (active) setCoverage(value); })
      .catch(() => { if (active) setCoverage(null); });
    return () => { active = false; };
  }, [projectId, photoId, selectedMask, maskOverlay, masks]);
  const overlaid = useMemo(() => render && coverage ? coverageImageUrl(render, coverage, 0.5, [235, 40, 40], crop) : null, [render, coverage, crop]);
  useEffect(() => {
    let active = true;
    if (inRetouch) { setLoading(false); return; }
    setError(null); setLoading(true);
    if (!inTauri()) {
      setError('Open the AURA desktop app to edit this photograph.'); setLoading(false); return;
    }
    void Promise.all([develop.imageRecipe({ photoId }), develop.imageHistory({ photoId })]).then(([nextRecipe, nextHistory]) => {
      if (!active) return;
      setRecipe(nextRecipe); setHistory(nextHistory); setLoaded(value => value + 1);
    }).catch(cause => { if (active) setError(asIpcError(cause).message); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [photoId, projectId, refresh, revision, inRetouch]);

  /** Run one saving action. Resolves to null when it saved, or to the reason it did not, so a
   *  tool that started the save can end its own progress state instead of reporting success. */
  const write = useCallback(async (action: () => Promise<unknown>): Promise<string | null> => {
    if (writing.current) return 'Another edit is still being saved. No edit was saved; try again when it finishes.';
    writing.current = true;
    setBusy(true); setWriteError(null);
    try { await action(); setRefresh(value => value + 1); return null; }
    catch (cause) {
      const message = asIpcError(cause).message;
      setWriteError(message);
      // Reconcile with saved state even if an operation failed after a partial
      // write. A rejected edit does not make the preview unreadable.
      setRefresh(value => value + 1);
      return message;
    }
    finally { writing.current = false; setBusy(false); }
  }, []);

  const pick = (x: number, y: number) => {
    if (disabled || busy || loading || waiting || !recipe || problem) return;
    void write(async () => {
      await pickWhiteBalance(projectId, photoId, x, y);
      setPicking(false); setView('edited'); setNotice('White balance saved as one manual edit. Use Undo to compare.');
    });
  };

  /** A point on the displayed (cropped) photograph, as whole-frame coordinates. */
  const framePoint = (event: React.PointerEvent<HTMLDivElement>): [number, number] | null => {
    if (!render) return null;
    const box = event.currentTarget.getBoundingClientRect();
    const point = imagePoint(event.clientX - box.left, event.clientY - box.top, box.width, box.height, render.width / render.height);
    if (!point) return null;
    return [crop[0] + point[0] * (crop[2] - crop[0]), crop[1] + point[1] * (crop[3] - crop[1])];
  };
  const finishDrawing = (end: [number, number] | null) => {
    const started = drag.current;
    drag.current = null; setSketch(null);
    if (started && liquifyTool && end) {
      const points = started.points.length > 1 ? started.points : [started.points[0] ?? end, end];
      const stroke = { mode: liquifyTool.mode, radius: liquifyTool.size, strength: liquifyTool.strength, points };
      saveFinish({ ...finish, liquify: [...(finish.liquify ?? []), stroke] }, `Liquify: ${liquifyTool.mode}`);
      return;
    }
    if (!started || !maskTool || !end) return;
    const [sx, sy] = started.start;
    let source: MaskSource | null = null;
    if (maskTool.kind === 'object') {
      const bounds: [number, number, number, number] = [Math.min(sx, end[0]), Math.min(sy, end[1]), Math.max(sx, end[0]), Math.max(sy, end[1])];
      if (bounds[2] - bounds[0] < 0.01 || bounds[3] - bounds[1] < 0.01) return;
      const tool = maskTool;
      void write(async () => {
        const made = await localMasks.create(projectId, photoId, { what: 'object', bounds, into: tool.into, mode: tool.into ? tool.mode : 'add', invert: tool.invert });
        setMaskMessage(made.message);
        if (made.maskId) setSelectedMask(made.maskId);
        setMaskTool(null);
      });
      return;
    }
    if (maskTool.kind === 'linear') {
      if (Math.hypot(end[0] - sx, end[1] - sy) < 0.01) return;
      source = { type: 'linear', start: [sx, sy], end };
    } else if (maskTool.kind === 'radial') {
      const radii: [number, number] = [Math.max(0.01, Math.abs(end[0] - sx)), Math.max(0.01, Math.abs(end[1] - sy))];
      source = { type: 'radial', centre: [sx, sy], radii, angle: 0, feather: 0.5 };
    } else {
      const stroke: BrushStroke = { erase: started.erase, radius: brush.size, opacity: brush.flow, points: started.points.map(([x, y]) => [x, y, 1]) };
      // Further strokes on the same brush mask paint into its brush, as in Lightroom.
      const target = masks.find(m => m.id === maskTool.into);
      const last = target?.components[target.components.length - 1];
      if (target && last?.source.type === 'brush' && maskTool.mode === 'add') {
        const brushed = { ...last, source: { ...last.source, strokes: [...last.source.strokes, stroke] } };
        const next = masks.map(m => m.id === target.id ? { ...target, components: [...target.components.slice(0, -1), brushed] } : m);
        void write(() => localMasks.save(projectId, photoId, next, stroke.erase ? 'Mask brush: erase' : 'Mask brush'));
        return;
      }
      if (stroke.erase) return;
      source = { type: 'brush', strokes: [stroke], feather: brush.feather };
    }
    const tool = maskTool;
    void write(async () => {
      const made = await localMasks.create(projectId, photoId, { what: 'geometry', source, into: tool.into, mode: tool.into ? tool.mode : 'add', invert: tool.invert });
      setMaskMessage(made.message);
      if (made.maskId) setSelectedMask(made.maskId);
      // Keep painting into the brush just made; a gradient is placed once.
      setMaskTool(tool.kind === 'brush' && made.maskId ? { ...tool, into: made.maskId, mode: 'add', invert: false } : null);
    });
  };

  if (retouchOpen || featureMode === 'skin' || featureMode === 'glare') return <NativeRetouchWorkspace key={`${projectId}:${photoId}:${featureMode}`} projectId={projectId} photoId={photoId} disabled={disabled} revision={revision} feature={featureMode === 'skin' || featureMode === 'glare' ? featureMode : undefined} onBusyChange={onBusyChange} onClose={() => setRetouchOpen(false)} />;

  return <section className="photo-studio" aria-label="Photo editor">
    <div className="studio-toolbar">
      <div><span className="eyebrow">PHOTO STUDIO</span><h2>Make it yours.</h2></div>
      <div className="view-switch" aria-label="Preview mode">
        {(['edited', 'original', 'split', 'color'] as const).map(mode => <button type="button" key={mode} aria-pressed={view === mode} disabled={!edited || !original} onClick={() => { setView(mode); setPicking(false); setProbe(null); }}>{mode === 'split' ? 'Compare' : mode === 'edited' ? 'Edited' : mode === 'color' ? 'Colour only' : 'Original'}</button>)}
      </div>
    </div>
    {problem && <p role="alert">{problem} <button type="button" onClick={() => setRefresh(value => value + 1)}>Retry preview</button></p>}
    {writeError && <p role="alert">Edit could not be saved: {writeError} <button type="button" onClick={() => setWriteError(null)}>Dismiss edit error</button></p>}
    <nav hidden={referenceMode || featureMode !== null} className="editor-panel-navigation" aria-label="Editing panels">
      {EDITOR_PANELS.map(([value, label]) => <button type="button" key={value} aria-pressed={panel === value}
        disabled={busy || disabled} onClick={() => {
          setPanel(value); setPicking(false); setMaskTool(null); setLiquifyTool(null);
          setProbing(false);
          drag.current = null; setSketch(null);
        }}>{label}</button>)}
      <button type="button" disabled={disabled || busy || loading || waiting || !recipe || Boolean(problem)}
        onClick={() => { setRetouchOpen(true); setPicking(false); }}>Precision retouch</button>
    </nav>
    <div className={`studio-layout editor-panel-layout${referenceWorkspace ? ' is-reference-workspace' : ''}`}>
      <div className="editor-preview-column">
        <ReferenceComparison enabled={referenceWorkspace} reference={comparisonReference}>
        <div className={`studio-canvas${picking ? ' studio-picking' : ''}${maskTool || liquifyTool ? ' studio-masking' : ''}`} aria-busy={busy || loading || waiting}
          onPointerDown={event => {
            if ((!maskTool && !liquifyTool) || view !== 'edited' || busy) return;
            const point = framePoint(event);
            if (!point) return;
            event.currentTarget.setPointerCapture?.(event.pointerId);
            const box = event.currentTarget.getBoundingClientRect();
            drag.current = { start: point, box: [event.clientX - box.left, event.clientY - box.top], points: [point], erase: brush.erase || event.altKey };
            setSketch([[event.clientX - box.left, event.clientY - box.top]]);
          }}
          onPointerMove={event => {
            if (!drag.current || (!maskTool && !liquifyTool)) return;
            const box = event.currentTarget.getBoundingClientRect();
            const here: [number, number] = [event.clientX - box.left, event.clientY - box.top];
            if (liquifyTool || maskTool?.kind === 'brush') {
              const point = framePoint(event);
              if (point) drag.current.points.push(point);
              setSketch(value => [...(value ?? []), here]);
            } else setSketch([drag.current.box, here]);
          }}
          onPointerUp={event => finishDrawing(framePoint(event))}
          onPointerCancel={() => { drag.current = null; setSketch(null); }}
          onClick={event => {
            if (probing && displayedRender) {
              const box = event.currentTarget.getBoundingClientRect();
              const point = imagePoint(event.clientX - box.left, event.clientY - box.top, box.width, box.height, displayedRender.width / displayedRender.height);
              if (point) { try { setProbe(probePixel(displayedRender, point)); } catch (cause) { setNotice(cause instanceof Error ? cause.message : asIpcError(cause).message); } }
              return;
            }
            if (!picking || !aspect) return;
            const box = event.currentTarget.getBoundingClientRect();
            const point = imagePoint(event.clientX - box.left, event.clientY - box.top, box.width, box.height, aspect);
            if (point) pick(...point);
          }}>
          {edited && original ? <>
            <img src={view === 'original' ? original : (view === 'edited' && overlaid) || (proof ?? edited)} alt={view === 'original' ? 'Original photograph' : view === 'color' ? 'Colour-only photograph' : 'Edited photograph'} />
            {sketch && liquifyTool && <svg className="studio-sketch" aria-hidden="true"><polyline points={sketch.map(p => p.join(',')).join(' ')} fill="none" stroke="#fc5" strokeWidth={4} strokeLinecap="round" strokeLinejoin="round" opacity={0.8} /></svg>}
            {sketch && maskTool && <svg className="studio-sketch" aria-hidden="true">
              {maskTool.kind === 'brush' ? <polyline points={sketch.map(p => p.join(',')).join(' ')} fill="none" stroke={drag.current?.erase ? '#6cf' : '#f55'} strokeWidth={6} strokeLinecap="round" strokeLinejoin="round" opacity={0.7} />
                : maskTool.kind === 'object' && sketch.length === 2 ? <rect x={Math.min(sketch[0]?.[0] ?? 0, sketch[1]?.[0] ?? 0)} y={Math.min(sketch[0]?.[1] ?? 0, sketch[1]?.[1] ?? 0)} width={Math.abs((sketch[1]?.[0] ?? 0) - (sketch[0]?.[0] ?? 0))} height={Math.abs((sketch[1]?.[1] ?? 0) - (sketch[0]?.[1] ?? 0))} fill="none" stroke="#5f5" strokeWidth={2} strokeDasharray="6 4" />
                : maskTool.kind === 'linear' && sketch.length === 2 ? <line x1={sketch[0]?.[0]} y1={sketch[0]?.[1]} x2={sketch[1]?.[0]} y2={sketch[1]?.[1]} stroke="#f55" strokeWidth={2} />
                  : sketch.length === 2 ? <ellipse cx={sketch[0]?.[0]} cy={sketch[0]?.[1]} rx={Math.abs((sketch[1]?.[0] ?? 0) - (sketch[0]?.[0] ?? 0))} ry={Math.abs((sketch[1]?.[1] ?? 0) - (sketch[0]?.[1] ?? 0))} fill="none" stroke="#f55" strokeWidth={2} /> : null}
            </svg>}
            <img src={original} alt="" hidden onLoad={event => { const img = event.currentTarget; if (img.naturalHeight) setAspect(img.naturalWidth / img.naturalHeight); }} />
            {view === 'split' && <img className="studio-original-overlay" src={original} alt="Original side of comparison" style={{ clipPath: `inset(0 ${100 - split}% 0 0)` }} />}
            {view === 'split' && <div className="studio-divider" style={{ left: `${split}%` }} aria-hidden="true" />}
            <span className="studio-caption">{view === 'split' ? 'Original / Edited' : view === 'original' ? 'Original' : view === 'color' ? colorPreview ? 'Colour only' : 'Preparing colour-only preview…' : 'Edited'}</span>
            {(busy || loading || waiting) && <span className="studio-updating" role="status">Updating preview…</span>}
          </> : <p>{problem ? 'Photo preview unavailable' : 'Loading your photograph…'}</p>}
        </div>
        </ReferenceComparison>
        {colorError && <p role="alert">Colour-only preview: {colorError}</p>}
        <button type="button" aria-pressed={probing} disabled={!displayedRender || view === 'split'} onClick={() => { setProbing(!probing); setPicking(false); setMaskTool(null); setLiquifyTool(null); }}>Inspect RGB / Lab point</button>
        {probing && <p className="lr-hint">Click the displayed photograph. Readout measures rendered sRGB and CIE Lab D65, not source RAW sensor values.</p>}
        {probe && <output aria-label="Photo colour readout">RGB {probe.rgb.join(', ')} · Lab {probe.lab.map(v => v.toFixed(2)).join(', ')}</output>}
        {view === 'split' && <label className="compare-control">Before / after<input type="range" min={0} max={100} value={split} onChange={event => setSplit(Number(event.target.value))} aria-label="Before and after divider" /></label>}
        <label className="compare-control">Clipping warnings<select value={clipping} onChange={event => setClipping(event.target.value as ClippingMode)}>
          <option value="off">Off</option><option value="shadows">Shadows</option><option value="highlights">Highlights</option><option value="both">Both</option>
        </select></label>
        {clipping !== 'off' && <p className="lr-hint">Edited preview only: blue/black hatching marks near-black pixels; red/white marks a near-clipped channel. Exports are unaffected.</p>}
        <p className="studio-footnote" role="status">{busy ? 'Saving your edit…' : loading ? 'Rendering your photograph…' : problem ? 'Preview needs attention.' : writeError ? 'The last edit failed. Showing saved settings.' : 'Edits saved. Your original stays untouched.'}{render && (editedPreview.quality === 'live' ? ' · Live preview - finishing the retouch…' : editedPreview.upgrading ? ' · Quick preview - rendering full quality…' : ` · Full quality ${render.width} × ${render.height}`)}</p>
        {render?.backend === 'wgpu' && <p className="studio-footnote" title="The graphics card processed the final color conversion for this preview.">GPU output</p>}
        {render?.notes.filter(note => note.isCaveat).map(note => <p className="studio-footnote" key={`${note.stage}:${note.reason}`}>{note.detail ?? note.reason}</p>)}
      </div>
      <div className="studio-adjustments-column">
      <fieldset className="studio-adjustments lr-adjustments" disabled={disabled || busy || loading || waiting || !recipe || Boolean(problem)}>
        <legend>{panel === 'all' ? 'Develop' : EDITOR_PANELS.find(([value]) => value === panel)?.[1]}</legend>
        <div className="studio-edit-modes" aria-label="Editing controls" hidden={!shows('develop')}>
          <button type="button" onClick={() => { setRetouchOpen(true); setPicking(false); }}>Retouch</button>
          {(['essentials', 'advanced'] as const).map(value => <button type="button" key={value} aria-pressed={mode === value} onClick={() => setMode(value)}>{value === 'essentials' ? 'Essentials' : 'Advanced'}</button>)}
        </div>
        <p hidden={!shows('develop')}><strong>Auto enhance</strong> measures light, colour, noise and the scene, detects faces, then retouches skin, heals temporary blemishes and finishes eyes and teeth where it measures a need. Each stage is saved as its own step, so you can go back to any of them and edit by hand; your manual adjustments are protected.</p>
        <div hidden={!shows('develop') && !shows('finish')}><PortraitAutoReport recipe={recipe}/></div>
        <div hidden={!shows('develop')}><PhotoPassPanel recipe={recipe} disabled={disabled || busy || loading || waiting || !recipe || Boolean(problem)}
          onRun={() => void write(() => photoPass.run(projectId, photoId))}/></div>
        <details className="lr-section" hidden={!shows('masks')} open={panel === 'masks' || masks.length > 0 || maskTool !== null}><summary>Masking{masks.length ? ` (${masks.length})` : ''}</summary><div className="lr-section-body">
          <MasksPanel masks={masks} selected={selectedMask} disabled={disabled || busy || loading || waiting || !recipe || Boolean(problem)}
            message={maskMessage} tool={maskTool} brush={brush} overlay={maskOverlay}
            onSelect={setSelectedMask} onTool={tool => { setMaskTool(tool); if (tool) { setLiquifyTool(null); setView('edited'); setPicking(false); } }}
            onBrush={setBrush} onOverlay={setMaskOverlay}
            onWorkflow={request => void write(async () => {
              if (!selectedMask || !recipe) return;
              const cancelId = crypto.randomUUID(); maskJob.current = cancelId;
              try { const made = await maskWorkflows.apply(projectId, photoId, selectedMask, recipe.recipeHash, request, cancelId);
                setMaskMessage(made.message); if (made.maskId) setSelectedMask(made.maskId);
              } finally { maskJob.current = null; }
            })}
            onCreate={request => void write(async () => {
              const made = await localMasks.create(projectId, photoId, request);
              setMaskMessage(made.message);
              if (made.maskId) setSelectedMask(made.maskId);
            })}
            onSave={(next, label) => void write(() => localMasks.save(projectId, photoId, next, label))} />
        </div></details>
        <details className="lr-section" hidden={!shows('portrait')} open={featureMode === 'liquify' || panel === 'portrait' || liquifyTool !== null}><summary>Portrait studio: face, body, background, makeup</summary><div className="lr-section-body">
          <PortraitFinishPanel finish={finish} disabled={disabled || busy || loading || waiting || !recipe || Boolean(problem)}
            liquify={liquifyTool} onSave={saveFinish} subject={`${projectId}/${photoId}`} initialTab={featureMode === 'liquify' ? 'liquify' : 'face'} notes={finishNotes}
            onSuggestLight={() => studioFinish.suggestLight(projectId, photoId)}
            onFindFaces={() => evotoWorkflow.faces(projectId, photoId).then(found => found.faces)}
            onLiquify={tool => { setLiquifyTool(tool); if (tool) { setMaskTool(null); setView('edited'); setPicking(false); } }} />
        </div></details>
        <details className="lr-section" hidden={!shows('portrait')}><summary>Framing, delivery and review: ID photo, crops, cut-out, social</summary><div className="lr-section-body">
          <EvotoWorkflowPanel projectId={projectId} photoId={photoId} disabled={disabled || busy || loading || waiting || !recipe || Boolean(problem)} onWrite={write} />
        </div></details>
        {render && edited && <Histogram render={render} />}
        {notice && <p role="status" className="lr-notice">{notice}</p>}
        <details className="lr-section" hidden={!shows('develop')}><summary>White balance picker</summary><div className="lr-section-body" onKeyDown={event => { if (event.key === 'Escape') setPicking(false); }}>
          <p className="lr-hint">Choose a neutral gray or white midtone in the original. Avoid clipped highlights. Click the photograph or enter a position below.</p>
          <button type="button" aria-pressed={picking} onClick={() => { setPicking(!picking); setView('original'); }}>{picking ? 'Cancel neutral picker' : 'Pick neutral area'}</button>
          <label>Horizontal position (%)<input type="number" min={0} max={100} value={pickX} onChange={event => setPickX(Number(event.target.value))} /></label>
          <label>Vertical position (%)<input type="number" min={0} max={100} value={pickY} onChange={event => setPickY(Number(event.target.value))} /></label>
          <button type="button" disabled={![pickX, pickY].every(value => Number.isFinite(value) && value >= 0 && value <= 100)} onClick={() => pick(pickX / 100, pickY / 100)}>Sample this position</button>
        </div></details>
        <div hidden={!shows('develop')}>
        <StudioColorPanel projectId={projectId} photoId={photoId} recipe={recipe} disabled={disabled || busy || loading || waiting || !recipe || Boolean(problem)} onApply={async () => { setRefresh(value => value + 1); }}/>
        <FramingPanel key={`framing:${photoId}`} projectId={projectId} photoId={photoId} recipe={recipe} original={original}
          disabled={disabled || busy || loading || waiting || !recipe || Boolean(problem)} onSave={async (geometry, label) => { await write(() => develop.setParam({ projectId, photoId, path: 'geometry', value: geometry, label })); }}/>
        <LightroomPanel recipe={recipe} disabled={disabled || busy || loading || waiting || !recipe || Boolean(problem)} aspect={aspect} profiles={profiles} mode={mode}
          syncControls={<SyncSettingsPanel key={`${projectId}:${photoId}`} projectId={projectId} photoId={photoId} disabled={disabled || busy || loading || waiting}
            onSync={(targets, groups) => void write(async () => {
              const report = await syncSettings(projectId, photoId, targets, false, groups);
              setNotice(`Settings synced to ${report.synced} photos.${report.failed.length ? ` Failed: ${report.failed.join('; ')}` : ''}`);
            })} />}
          onSetParam={(path, value, label) => void write(() => develop.setParam({ projectId, photoId, path, value, label }))}
          onAuto={() => void write(async () => { await develop.enhancePhoto({ photoId }); await photoPass.run(projectId, photoId); setPosition(null); })}
          onApplyProfile={(profileId, strength) => void write(async () => {
            const report = await editProfiles.apply(photoId, profileId, strength);
            setNotice(report.adaptations.length ? report.adaptations.join(' ') : `Applied ${profiles.find(p => p.id === profileId)?.name ?? profileId}.`);
          })}
          onSync={includeGeometry => void write(async () => {
            const report = await syncSettings(projectId, photoId, [], includeGeometry);
            setNotice(`Settings synced to ${report.synced} photo${report.synced === 1 ? '' : 's'}.${report.failed.length ? ` ${report.failed.length} could not be updated.` : ''}`);
          })} />
        </div>
        <div hidden={!shows('materials')}>
          <MaterialLabPanel projectId={projectId} photoId={photoId} recipe={recipe}
            disabled={disabled || busy || loading || waiting || !recipe || Boolean(problem)} onApply={async () => { setRefresh(value => value + 1); }}/>
        </div>
        <div hidden={!shows('spatial')}><SpatialLabPanel projectId={projectId} photoId={photoId} recipe={recipe} disabled={disabled || busy || loading || waiting || !recipe || Boolean(problem)} onApply={async()=>{setRefresh(value=>value+1);}}/></div>
        <div hidden={!shows('colorlab')}><ColorLabPanel projectId={projectId} photoId={photoId} recipe={recipe} disabled={disabled || busy || loading || waiting || !recipe || Boolean(problem)} onApply={async()=>{setRefresh(value=>value+1);}}/></div>
        <div hidden={!shows('review')}><WorkflowLabPanel projectId={projectId} photoId={photoId} photoIds={photoIds} recipe={recipe} disabled={disabled || busy || loading || waiting || !recipe || Boolean(problem)} onEdited={async()=>{setRefresh(value=>value+1);}}/></div>
        <div hidden={!shows('inspect') && !shows('palette') && !shows('detailreview') && !shows('composition') && !shows('deliveryreview') && !shows('surfacereview') && !shows('lightingreview') && !shows('texturefidelity') && !shows('colourstability') && !shows('deliveryintegrity')}>
          <PhotoAnalysisPanel projectId={projectId} photoId={photoId} recipe={recipe}
            focusGroup={panel === 'palette' ? 'Palette & tone review' : panel === 'detailreview' ? 'Detail review' : panel === 'composition' ? 'Composition review' : panel === 'deliveryreview' ? 'Delivery statistics' : panel === 'surfacereview' ? 'Surface review' : panel === 'lightingreview' ? 'Lighting geometry' : panel === 'texturefidelity' ? 'Texture fidelity' : panel === 'colourstability' ? 'Colour stability' : panel === 'deliveryintegrity' ? 'Delivery integrity' : undefined}
            disabled={disabled || busy || loading || waiting || !recipe || Boolean(problem)}/>
        </div>
        <div hidden={!shows('finish')} className="editor-finish-panel" aria-label="Automatic finishing panel">
          <AutomaticFinishControls key={`${projectId}:${photoId}`} projectId={projectId} photoId={photoId}
            revision={recipe?.recipeHash ?? ''} disabled={disabled || busy || loading || waiting || !recipe || Boolean(problem)} onApply={write}/>
        </div>
        <div className="studio-history">
          <button type="button" disabled={!history?.canUndo} onClick={() => void write(async () => { await develop.historyStep({ projectId, photoId, action: 'undo' }); setPosition(null); })}>Undo</button>
          <button type="button" disabled={!history?.canRedo} onClick={() => void write(async () => { await develop.historyStep({ projectId, photoId, action: 'redo' }); setPosition(null); })}>Redo</button>
          <button type="button" onClick={() => void write(() => develop.historyStep({ projectId, photoId, action: 'reset_original' }))}>Reset photo</button>
        </div>
        <div hidden={!shows('history')}>
        <SnapshotPanel key={photoId} names={history?.snapshots ?? []} disabled={disabled || busy || loading || waiting}
          onTake={name => void write(() => develop.snapshot({ projectId, photoId, action: 'take', name }))}
          onRestore={name => void write(() => develop.snapshot({ projectId, photoId, action: 'restore', name }))} />
        </div>
      </fieldset>
      <div hidden={!shows('references')} className="editor-reference-panel" aria-label="Reference matching panel">
      <ColorMatchPanel key={`colour:${projectId}:${photoId}`} projectId={projectId} photoId={photoId} revision={recipe?.recipeHash}
        active={active && shows('references')} selectedPhotoIds={selectedPhotoIds} onReferenceChange={setComparisonReference}
        disabled={disabled || busy || loading || waiting || !recipe || Boolean(problem)} onApply={write} />
      <PairedReference key={`paired:${projectId}:${photoId}`} photoId={photoId} disabled={disabled || busy || loading || waiting || !recipe || Boolean(problem)} onApply={write} />
      </div>
      </div>
    </div>
    {history && <details hidden={!shows('history')} className="advanced-tools" open={history.entries.some(entry => entry.source !== 'user')}><summary>Review every edit on this photo ({history.entries.length})</summary>
      {history.entries.length === 0 ? <p>No edits have been saved yet.</p> : <ol className="photo-edit-history">
        <li key="original" aria-current={position === 0 ? 'step' : undefined}><strong>Original photograph</strong>
          <button type="button" disabled={disabled || busy || loading || waiting} onClick={() => void write(async () => { await develop.historyStep({ projectId, photoId, action: 'goto:0' }); setPosition(0); })}>Go back to here</button></li>
        {history.entries.map(entry => <li key={entry.seq} aria-current={(position ?? (history.canRedo ? null : history.entries[history.entries.length - 1]?.seq)) === entry.seq ? 'step' : undefined}>
        <strong>{entry.label}</strong><p>{new Date(entry.atMs).toLocaleString()} · {entry.source === 'user' ? 'Your edit' : 'Automatic edit'}</p>
        <p>Changed: {entry.changed.join(', ') || 'No parameter changes'}</p>
        <button type="button" disabled={disabled || busy || loading || waiting} aria-label={`Go back to step ${entry.seq}: ${entry.label}`}
          onClick={() => void write(async () => { await develop.historyStep({ projectId, photoId, action: `goto:${entry.seq}` }); setPosition(entry.seq); })}>Go back to here</button>
      </li>)}</ol>}
      <p>Use Undo and Redo, or “Go back to here”, to move through saved edits. Going back discards nothing: Redo still moves forward, and a new edit continues from the step you chose.</p>
    </details>}
  </section>;
}
