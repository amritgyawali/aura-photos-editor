import { useEffect, useRef, useState } from 'react';
import { asIpcError, delivery, pickPhotoFolder } from '../../ipc/client';
import { framingWorkflows } from '../../ipc/framingWorkflows';
import type { RecipeDto } from '../../ipc/types';
import { eyeRotation, faceFrame, pixelCrop, printPixels, ratioCrop, subjectRotation, validCrop, type Crop, type FramingContext } from './framingMath';

type Geometry = { crop: Crop; rotate: number; perspective?: unknown };
type Preset = { name: string; ratio: number; followOrientation: boolean; headShare: number; topMargin: number; horizontal: number };
const STORAGE = 'aura-framing-presets-v1';
const messageOf = (cause: unknown) => cause instanceof Error ? cause.message : asIpcError(cause).message;
export function readFramingPresets(): Preset[] {
  const value: unknown = JSON.parse(localStorage.getItem(STORAGE) ?? '[]');
  if (!Array.isArray(value) || value.length > 32 || value.some(p => typeof p !== 'object' || !p || typeof p.name !== 'string' || p.name.length > 60 || !Number.isFinite(p.ratio) || p.ratio < .05 || p.ratio > 20 || typeof p.followOrientation !== 'boolean' || !Number.isFinite(p.headShare) || p.headShare < .1 || p.headShare > .9 || !Number.isFinite(p.topMargin) || p.topMargin < 0 || p.topMargin > .5 || !Number.isFinite(p.horizontal) || p.horizontal < .1 || p.horizontal > .9)) throw new Error('Saved framing templates are invalid. Clear them before saving a new template.');
  return value as Preset[];
}
function geometryOf(recipe: RecipeDto | null): Geometry {
  try { const value = JSON.parse(recipe?.body ?? '{}').geometry as Geometry; if (value && validCrop(value.crop)) return value; } catch { /* loading a new photo */ }
  return { crop: [0, 0, 1, 1], rotate: 0, perspective: null };
}
function FreeCrop({ source, crop, disabled, onChange }: { source: string | null; crop: Crop; disabled: boolean; onChange: (crop: Crop) => void }): JSX.Element {
  const drag = useRef<number | null>(null);
  const container = useRef<HTMLDivElement>(null);
  return <div className="framing-canvas" ref={container} onPointerMove={event => {
    if (drag.current === null || disabled || !container.current) return;
    const box = container.current.getBoundingClientRect();
    const x = Math.max(0, Math.min(1, (event.clientX - box.left) / box.width));
    const y = Math.max(0, Math.min(1, (event.clientY - box.top) / box.height));
    const next: Crop = [...crop];
    if (drag.current === 0 || drag.current === 2) next[0] = Math.min(x, crop[2] - .001); else next[2] = Math.max(x, crop[0] + .001);
    if (drag.current < 2) next[1] = Math.min(y, crop[3] - .001); else next[3] = Math.max(y, crop[1] + .001);
    if (validCrop(next)) onChange(next);
  }} onPointerUp={() => { drag.current = null; }} onPointerCancel={() => { drag.current = null; }}>
    {source ? <img src={source} alt="Uncropped framing preview" draggable={false} /> : <p>Load the original preview to draw a crop.</p>}
    <div className="framing-rectangle" style={{ left: `${crop[0] * 100}%`, top: `${crop[1] * 100}%`, width: `${(crop[2] - crop[0]) * 100}%`, height: `${(crop[3] - crop[1]) * 100}%` }}/>
    {['top left', 'top right', 'bottom left', 'bottom right'].map((name, i) => <button type="button" key={name} aria-label={`Drag crop ${name}`} disabled={disabled || !source}
      style={{ left: `${crop[i % 2 === 0 ? 0 : 2] * 100}%`, top: `${crop[i < 2 ? 1 : 3] * 100}%` }} onPointerDown={event => { event.currentTarget.setPointerCapture?.(event.pointerId); drag.current = i; }}/>) }
  </div>;
}

export function FramingPanel({ projectId, photoId, recipe, original, disabled, onSave }: {
  projectId: string; photoId: string; recipe: RecipeDto | null; original: string | null; disabled: boolean;
  onSave: (geometry: Geometry, label: string) => Promise<void>;
}): JSX.Element {
  const [context, setContext] = useState<FramingContext | null>(null);
  const [draft, setDraft] = useState(() => geometryOf(recipe));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [ratioWidth, setRatioWidth] = useState(4), [ratioHeight, setRatioHeight] = useState(5);
  const [followOrientation, setFollowOrientation] = useState(false);
  const [pixels, setPixels] = useState<[number, number]>([1200, 1500]);
  const [millimeters, setMillimeters] = useState<[number, number]>([35, 45]);
  const [ppi, setPpi] = useState(300);
  const [face, setFace] = useState(0);
  const [headShare, setHeadShare] = useState(.5), [topMargin, setTopMargin] = useState(.1), [horizontal, setHorizontal] = useState(.5);
  const [presetName, setPresetName] = useState(''), [presets, setPresets] = useState<Preset[]>([]);
  const active = useRef(photoId); active.current = photoId;
  const activeHash = useRef(recipe?.recipeHash); activeHash.current = recipe?.recipeHash;
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  useEffect(() => { setDraft(geometryOf(recipe)); setContext(null); setError(null); setNotice(null); }, [photoId, recipe?.recipeHash]);
  useEffect(() => { setContext(null); }, [photoId]);
  const run = async (action: () => Promise<void>) => {
    if (disabled || busy) return; setBusy(true); setError(null);
    try { await action(); } catch (cause) { if (mounted.current && active.current === photoId) setError(messageOf(cause)); }
    finally { if (mounted.current) setBusy(false); }
  };
  const propose = (action: () => Crop, label: string) => {
    try { const crop = action(); if (!validCrop(crop)) throw new Error('The framing would produce an invalid crop.'); setDraft(value => ({ ...value, crop })); setNotice(`${label} ready. Apply framing to save.`); setError(null); }
    catch (cause) { setError(messageOf(cause)); }
  };
  const ratio = () => {
    let value = ratioWidth / ratioHeight;
    if (followOrientation && context) value = context.width > context.height ? Math.max(value, 1/value) : Math.min(value, 1/value);
    if (!Number.isFinite(value) || value < .05 || value > 20) throw new Error('Aspect ratio must be between 1:20 and 20:1.');
    return value;
  };
  const requireContext = () => { if (!context) throw new Error('Measure framing first.'); if (draft.rotate !== 0 || draft.perspective) throw new Error('Reset rotation and perspective before source-coordinate crop placement.'); return context; };
  const selectedFace = context?.faces[face];
  const center = (bounds: Crop) => [(bounds[0] + bounds[2])/2, (bounds[1] + bounds[3])/2];
  const locked = disabled || busy;
  return <details className="lr-section framing-panel"><summary>Crop & framing studio</summary><div className="lr-section-body">
    <p className="lr-hint">Draw a free crop or measure the original for exact sizing and subject placement. Measurements never save edits. Apply the draft once to create one history step.</p>
    <button type="button" disabled={locked || !recipe?.recipeHash} onClick={() => void run(async () => {
      const hash = recipe?.recipeHash ?? '';
      const value = await framingWorkflows.context(projectId, photoId, hash);
      if (mounted.current && active.current === photoId && activeHash.current === hash) { setContext(value); setFace(0); }
    })}>{busy ? 'Measuring framing…' : 'Measure source and subjects'}</button>
    {context && <><p>Original: {context.width} × {context.height} pixels.</p><ul>{context.reasons.map(reason => <li key={reason}>{reason}</li>)}</ul></>}
    <FreeCrop source={original} crop={draft.crop} disabled={locked || draft.rotate !== 0 || Boolean(draft.perspective)} onChange={crop => setDraft(value => ({ ...value, crop }))}/>
    {(draft.rotate !== 0 || Boolean(draft.perspective)) && <p className="lr-hint">Crop handles address the original frame. Reset rotation and perspective before dragging; aligned rotations may expose empty corners, so review the saved preview.</p>}
    {(['Left', 'Top', 'Right', 'Bottom'] as const).map((label, i) => <label key={label}>{label} (%)<input type="number" aria-label={`Crop ${label.toLowerCase()}`} min={0} max={100} step={.1} value={(draft.crop[i] ?? 0) * 100} disabled={locked}
      onChange={event => { const next: Crop = [...draft.crop]; next[i] = Number(event.target.value) / 100; setDraft(value => ({ ...value, crop: next })); }}/></label>)}
    <label>Custom ratio width<input type="number" min={.1} max={100} value={ratioWidth} onChange={e => setRatioWidth(Number(e.target.value))}/></label>
    <label>Custom ratio height<input type="number" min={.1} max={100} value={ratioHeight} onChange={e => setRatioHeight(Number(e.target.value))}/></label>
    <label><input type="checkbox" checked={followOrientation} onChange={e => setFollowOrientation(e.target.checked)}/> Follow original orientation</label>
    <button type="button" disabled={locked || !context} onClick={() => propose(() => { const c = requireContext(); return ratioCrop(c.width, c.height, ratio()); }, 'Custom ratio')}>Prepare custom ratio crop</button>
    {pixels.map((value, i) => <label key={i}>Pixel crop {i === 0 ? 'width' : 'height'}<input type="number" min={1} max={30000} value={value} onChange={e => setPixels(p => i === 0 ? [Number(e.target.value), p[1]] : [p[0], Number(e.target.value)])}/></label>)}
    <button type="button" disabled={locked || !context} onClick={() => propose(() => { const c = requireContext(); return pixelCrop(c.width, c.height, pixels); }, 'Exact pixel crop')}>Prepare exact pixel crop</button>
    <details><summary>Physical print sizing</summary>
      {millimeters.map((value, i) => <label key={i}>Print {i === 0 ? 'width' : 'height'} (mm)<input type="number" min={1} max={1000} value={value} onChange={e => setMillimeters(p => i === 0 ? [Number(e.target.value), p[1]] : [p[0], Number(e.target.value)])}/></label>)}
      <label>Print PPI<input type="number" min={1} max={1200} value={ppi} onChange={e => setPpi(Number(e.target.value))}/></label>
      <button type="button" disabled={locked || !context} onClick={() => propose(() => { const c = requireContext(); const size = printPixels(millimeters, ppi); setPixels(size); return ratioCrop(c.width, c.height, size[0]/size[1]); }, 'Physical print crop')}>Prepare print crop and pixel size</button>
      <p className="lr-hint">PPI calculates pixel dimensions; this does not certify passport requirements or embed printer resolution metadata. Save framing before exporting. Export fits within the requested pixels and never upscales.</p>
      <button type="button" disabled={locked || !recipe} onClick={() => void run(async () => {
        const size = printPixels(millimeters, ppi);
        const current = geometryOf(recipe);
        if (JSON.stringify(current) !== JSON.stringify(draft)) throw new Error('Apply framing before exporting this print.');
        const destination = await pickPhotoFolder('Choose a separate folder for this framed print'); if (!destination) return;
        const report = await delivery.exportRun({ projectId, sets: [{ name: 'framed-print', imageIds: [photoId], format: 'jpeg', quality: 95, colour: 'srgb', bitDepth: 8, resize: `fit:${size[0]}x${size[1]}`, sharpen: 'print', naming: '{original}_{set}', sidecar: false }], destination, destinationKind: 'folder', copyright: null, contact: null, creator: null, keywords: [], stripGps: true, stripCameraSerial: true, verify: true });
        if (report.requested !== 1 || report.written !== 1 || report.verified !== 1 || report.renderFailed !== 0 || report.corrupt !== 0) throw new Error('The framed print was not completely written and verified. Review Export before retrying.');
        if (mounted.current && active.current === photoId) setNotice(`Framed print exported within ${size[0]} × ${size[1]} pixels; source unchanged.`);
      })}>Export this saved framed print</button>
    </details>
    <button type="button" disabled={locked || !context?.subjectBounds} onClick={() => propose(() => { const c = requireContext(); return ratioCrop(c.width, c.height, ratio(), center(c.subjectBounds ?? [0,0,1,1])); }, 'Subject-centred crop')}>Place crop on subject</button>
    <label>Framing face<select value={face} disabled={locked || !context?.faces.length} onChange={e => setFace(Number(e.target.value))}>{context?.faces.map((f, i) => <option key={i} value={i}>Face {i+1} ({Math.round(f.confidence*100)}%)</option>)}</select></label>
    <button type="button" disabled={locked || !selectedFace} onClick={() => propose(() => { const c = requireContext(); if (!selectedFace) throw new Error('No measured face selected.'); return ratioCrop(c.width, c.height, ratio(), center(selectedFace.bounds), 1.5); }, 'Face-centred crop')}>Place crop on face</button>
    <label>Face height in crop (%)<input type="range" min={10} max={90} value={headShare*100} onChange={e => setHeadShare(Number(e.target.value)/100)}/></label>
    <label>Top margin (%)<input type="range" min={0} max={50} value={topMargin*100} onChange={e => setTopMargin(Number(e.target.value)/100)}/></label>
    <label>Face horizontal position (%)<input type="range" min={10} max={90} value={horizontal*100} onChange={e => setHorizontal(Number(e.target.value)/100)}/></label>
    <button type="button" disabled={locked || !selectedFace} onClick={() => propose(() => { const c = requireContext(); if (!selectedFace) throw new Error('No measured face.'); return faceFrame(c, selectedFace, ratio(), headShare, topMargin, horizontal); }, 'Portrait framing rules')}>Prepare portrait framing rules</button>
    <button type="button" disabled={locked || !selectedFace || selectedFace.eyesMeasured < 2} onClick={() => { try { setDraft(value => ({ ...value, rotate: eyeRotation(selectedFace!, context!.width, context!.height) })); setError(null); } catch (e) { setError(messageOf(e)); } }}>Align measured eye line</button>
    <button type="button" disabled={locked || context?.subjectAxis == null} onClick={() => { try { setDraft(value => ({ ...value, rotate: subjectRotation(context!.subjectAxis!) })); setError(null); } catch (e) { setError(messageOf(e)); } }}>Align subject silhouette axis</button>
    <p className="lr-hint">Subject-axis alignment uses the silhouette's long axis, not a skeleton. Review posture, tilted objects and group photos. Eye alignment is disabled when both eyes were not measured.</p>
    <details onToggle={event => { if (event.currentTarget.open) { try { setPresets(readFramingPresets()); } catch (e) { setError(messageOf(e)); } } }}><summary>Reusable framing templates</summary>
      <button type="button" disabled={locked} onClick={() => { try { localStorage.removeItem(STORAGE); setPresets([]); setError(null); } catch (e) { setError(messageOf(e)); } }}>Clear local framing templates</button>
      <label>Framing template name<input maxLength={60} value={presetName} onChange={e => setPresetName(e.target.value)}/></label>
      <button type="button" disabled={locked || !presetName.trim()} onClick={() => { try { const list = readFramingPresets().filter(p => p.name !== presetName.trim()); if (list.length >= 32) throw new Error('Keep at most 32 framing templates.'); const next = [...list, { name: presetName.trim(), ratio: ratio(), followOrientation, headShare, topMargin, horizontal }]; localStorage.setItem(STORAGE, JSON.stringify(next)); setPresets(next); setNotice('Framing template saved on this device.'); } catch (e) { setError(messageOf(e)); } }}>Save framing template</button>
      {presets.map(p => <div key={p.name}><button type="button" disabled={locked} onClick={() => { setRatioWidth(p.ratio); setRatioHeight(1); setFollowOrientation(p.followOrientation); setHeadShare(p.headShare); setTopMargin(p.topMargin); setHorizontal(p.horizontal); setNotice('Template loaded. Prepare the chosen crop before applying.'); }}>{p.name}</button><button type="button" disabled={locked} aria-label={`Remove framing template ${p.name}`} onClick={() => { try { const next = presets.filter(v => v.name !== p.name); localStorage.setItem(STORAGE, JSON.stringify(next)); setPresets(next); } catch (e) { setError(messageOf(e)); } }}>Remove</button></div>)}
    </details>
    <button type="button" disabled={locked || !validCrop(draft.crop)} onClick={() => void run(async () => { await onSave(draft, 'Crop and framing'); })}>Apply framing</button>
    <button type="button" disabled={locked} onClick={() => { setDraft(geometryOf(recipe)); setError(null); setNotice(null); }}>Discard framing draft</button>
    {notice && <p role="status">{notice}</p>}{error && <p role="alert">{error}</p>}
  </div></details>;
}
