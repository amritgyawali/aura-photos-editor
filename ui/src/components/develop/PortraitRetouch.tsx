import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { api, asIpcError, develop, inTauri, portrait } from '../../ipc/client';
import type { PortraitAnalysis, PortraitRetouch as PortraitSettings, RegionAdjustment } from '../../ipc/client';
import { rgbDataUrl } from './rgbImage';
import {
  DEFAULT_OVERLAY,
  OPERATOR_GROUPS,
  PORTRAIT_OPERATORS,
  REGION_COLOURS,
  boxFromDrag,
  composeOverlay,
  decodeAlpha,
  percent,
} from './portraitOverlay';

/**
 * Portrait retouch: what AURA found in a photograph, shown, and what a photographer asks for.
 *
 * Four rules this workspace exists to keep:
 *
 * 1. **What AURA found is shown before anything is changed.** Every face with its eyes, nose and
 *    mouth, and every region - skin, eyes, iris, whites, brows, lips, teeth, hair, beard, body,
 *    clothing, background, sky - can be laid over the photograph, so a photographer can see the
 *    region an adjustment will touch before touching it.
 * 2. **A face AURA missed can be drawn.** "Draw a face" turns a drag into a face box that travels
 *    in the edit, so the render and the export find the same face.
 * 3. **Nothing here reshapes a person or changes their skin tone.** The operators are tone and
 *    colour changes in place; there is no prop, handler or control that could carry a warp.
 * 4. **The automatic retouch never overwrites a choice.** Once a slider is moved here, the
 *    automatic pass leaves this photograph's retouch alone and says so.
 */

export type PortraitRetouchViewProps = {
  analysis: PortraitAnalysis | null;
  settings: PortraitSettings | null;
  strengths: Readonly<Record<string, number>>;
  adjustment: RegionAdjustment | null;
  imageUrl: string | null;
  originalUrl: string | null;
  comparing: boolean;
  overlay: ReadonlyArray<string>;
  showFaces: boolean;
  drawing: boolean;
  busy: boolean;
  error: string | null;
  explanation: ReadonlyArray<string>;
  onToggleRegion: (region: string) => void;
  onToggleFaces: () => void;
  onStrength: (op: string, value: number) => void;
  onCommit: () => void;
  onAuto: (style: 'natural' | 'soft' | 'polished') => void;
  onClear: () => void;
  onCompare: (showing: boolean) => void;
  onDrawing: (drawing: boolean) => void;
  onDrawFace: (box: [number, number, number, number]) => void;
  onClearFaces: () => void;
  onAdjustRegion: (region: string) => void;
  onAdjust: (field: keyof Omit<RegionAdjustment, 'region'>, value: number) => void;
};

/** The overlay, drawn into a canvas the size of the analysis grid and stretched over the photo. */
function OverlayCanvas({ analysis, overlay }: { analysis: PortraitAnalysis; overlay: ReadonlyArray<string> }): JSX.Element {
  const ref = useRef<HTMLCanvasElement | null>(null);
  const { overlayWidth: width, overlayHeight: height } = analysis;
  const rgba = useMemo(() => {
    // Broad regions first, small features last, so the teeth are never hidden under the face.
    const layers = analysis.regions
      .filter((r) => overlay.includes(r.region))
      .sort((a, b) => b.coverage - a.coverage)
      .map((r) => ({ alpha: decodeAlpha(r.alphaBase64), colour: REGION_COLOURS[r.region] ?? [255, 255, 255] }));
    return composeOverlay(width, height, layers);
  }, [analysis, overlay, width, height]);
  useEffect(() => {
    const canvas = ref.current;
    const context = canvas?.getContext?.('2d');
    if (!canvas || !context || typeof ImageData === 'undefined') return;
    context.putImageData(new ImageData(new Uint8ClampedArray(rgba), width, height), 0, 0);
  }, [rgba, width, height]);
  return <canvas ref={ref} className="portrait-overlay" width={width} height={height} aria-hidden="true" data-testid="portrait-overlay" />;
}

/** Face boxes and landmarks in normalised coordinates, plus the drag that draws a missed face. */
function FaceLayer({ analysis, showFaces, drawing, hints, onDrawFace }: {
  analysis: PortraitAnalysis | null; showFaces: boolean; drawing: boolean;
  hints: ReadonlyArray<[number, number, number, number]>;
  onDrawFace: (box: [number, number, number, number]) => void;
}): JSX.Element {
  const [start, setStart] = useState<[number, number] | null>(null);
  const [current, setCurrent] = useState<[number, number] | null>(null);
  const position = (event: React.PointerEvent<SVGSVGElement>): [number, number] => {
    const rect = event.currentTarget.getBoundingClientRect();
    return [
      rect.width > 0 ? (event.clientX - rect.left) / rect.width : 0,
      rect.height > 0 ? (event.clientY - rect.top) / rect.height : 0,
    ];
  };
  const pending = start && current ? boxFromDrag(start, current) : null;
  return (
    <svg
      className={drawing ? 'portrait-faces is-drawing' : 'portrait-faces'}
      viewBox="0 0 1 1"
      preserveAspectRatio="none"
      data-testid="portrait-faces"
      onPointerDown={(event) => { if (drawing) { setStart(position(event)); setCurrent(position(event)); } }}
      onPointerMove={(event) => { if (drawing && start) setCurrent(position(event)); }}
      onPointerUp={(event) => {
        if (drawing && start) {
          const box = boxFromDrag(start, position(event));
          if (box) onDrawFace(box);
        }
        setStart(null); setCurrent(null);
      }}
    >
      {showFaces && analysis?.faces.map((face, index) => (
        <g key={`${face.bbox.join(',')}:${index}`} className={`portrait-face source-${face.source}`}>
          <rect x={face.bbox[0]} y={face.bbox[1]} width={face.bbox[2]} height={face.bbox[3]} />
          {[face.leftEye, face.rightEye, face.nose, face.mouth].map((point, i) => (
            <circle key={i} cx={point[0]} cy={point[1]} r={0.006} />
          ))}
        </g>
      ))}
      {hints.map((hint, index) => (
        <rect key={`hint-${index}`} className="portrait-hint" x={hint[0]} y={hint[1]} width={hint[2]} height={hint[3]} />
      ))}
      {pending && <rect className="portrait-hint is-pending" x={pending[0]} y={pending[1]} width={pending[2]} height={pending[3]} />}
    </svg>
  );
}

const ADJUSTMENTS: ReadonlyArray<{ field: keyof Omit<RegionAdjustment, 'region'>; label: string; min: number; max: number; step: number }> = [
  { field: 'exposure', label: 'Brightness (stops)', min: -1.5, max: 1.5, step: 0.05 },
  { field: 'contrast', label: 'Contrast', min: -60, max: 60, step: 1 },
  { field: 'saturation', label: 'Saturation', min: -80, max: 80, step: 1 },
  { field: 'warmth', label: 'Warmth (K)', min: -1500, max: 1500, step: 50 },
];

export function PortraitRetouchView(props: PortraitRetouchViewProps): JSX.Element {
  const { analysis, settings, strengths, adjustment, overlay } = props;
  const found = new Set((analysis?.regions ?? []).map((r) => r.region));
  const faces = analysis?.faces.length ?? 0;
  const hints = settings?.hints ?? [];
  const summary = analysis === null
    ? 'Looking for faces…'
    : faces === 0
      ? 'No face found yet.'
      : `${faces} face${faces === 1 ? '' : 's'} found in ${analysis.ms} ms.`;

  return (
    <section className="portrait-retouch" aria-label="Portrait retouch">
      <div className="studio-toolbar">
        <div><span className="eyebrow">PORTRAIT RETOUCH</span><h2>Skin, eyes, smile and hair.</h2></div>
        <p data-testid="portrait-summary" className="portrait-summary">{summary}</p>
      </div>
      {props.error && <p role="alert">{props.error}</p>}
      <div className="studio-layout">
        <div>
          <div className="studio-canvas portrait-stage">
            {props.imageUrl ? <>
              {/* The frame is exactly the size of the displayed image, so the overlay and the
                  face layer, stretched over it, line up with the pixels they describe. */}
              <div className="portrait-frame">
                <img src={props.comparing && props.originalUrl ? props.originalUrl : props.imageUrl}
                  alt={props.comparing ? 'Original photograph' : 'Retouched photograph'} />
                {analysis && !props.comparing && <OverlayCanvas analysis={analysis} overlay={overlay} />}
                <FaceLayer analysis={analysis} showFaces={props.showFaces && !props.comparing} drawing={props.drawing}
                  hints={hints} onDrawFace={props.onDrawFace} />
              </div>
              <span className="studio-caption">{props.comparing ? 'Original' : 'Retouched'}</span>
            </> : <p>Loading your photograph…</p>}
          </div>
          <div className="portrait-stage-tools">
            <button type="button" aria-pressed={props.comparing} disabled={!props.originalUrl}
              onPointerDown={() => props.onCompare(true)} onPointerUp={() => props.onCompare(false)}
              onPointerLeave={() => props.onCompare(false)} data-testid="portrait-compare">Hold to see the original</button>
            <button type="button" aria-pressed={props.showFaces} onClick={props.onToggleFaces}>Faces and landmarks</button>
            <button type="button" aria-pressed={props.drawing} disabled={props.busy} onClick={() => props.onDrawing(!props.drawing)}
              data-testid="portrait-draw">{props.drawing ? 'Drag over the face…' : 'Draw a face AURA missed'}</button>
            {hints.length > 0 && <button type="button" disabled={props.busy} onClick={props.onClearFaces}>Remove drawn faces</button>}
          </div>
          <fieldset className="portrait-regions" aria-label="What AURA found">
            <legend>What AURA found <small>(tap to show)</small></legend>
            {(analysis?.regions ?? []).map((region) => {
              const colour = REGION_COLOURS[region.region] ?? [255, 255, 255];
              return <button key={region.region} type="button" className="region-chip" aria-pressed={overlay.includes(region.region)}
                onClick={() => props.onToggleRegion(region.region)} data-testid={`region-${region.region}`}
                title={`${Math.round(region.coverage * 1000) / 10}% of the frame · ${Math.round(region.confidence * 100)}% sure`}>
                <i style={{ background: `rgb(${colour.join(',')})` }} />{region.label}
              </button>;
            })}
          </fieldset>
          {(analysis?.notes ?? []).map((note) => <p className="studio-footnote" key={note}>{note}</p>)}
        </div>
        <fieldset className="studio-adjustments portrait-controls" disabled={props.busy || !settings}>
          <legend>Retouch</legend>
          <div className="portrait-presets" role="group" aria-label="Automatic retouch">
            {(['soft', 'natural', 'polished'] as const).map((style) => (
              <button key={style} type="button" className={style === 'natural' ? 'is-primary' : undefined}
                onClick={() => props.onAuto(style)} data-testid={`portrait-auto-${style}`}>
                {style === 'natural' ? 'Auto retouch' : style === 'soft' ? 'Subtle' : 'Polished'}
              </button>
            ))}
          </div>
          <p className="studio-footnote">Measured from this face. Moles, freckles and skin tone are kept.</p>
          {settings?.protected && <p className="studio-footnote" data-testid="portrait-protected">Your settings: automatic retouching will not change them.</p>}
          {props.explanation.length > 0 && <ul className="portrait-explanation" data-testid="portrait-explanation">
            {props.explanation.map((line) => <li key={line}>{line}</li>)}
          </ul>}
          {OPERATOR_GROUPS.map((group) => (
            <div className="portrait-group" key={group}>
              <h3>{group}</h3>
              {PORTRAIT_OPERATORS.filter((op) => op.group === group).map((op) => {
                const value = percent(strengths[op.op] ?? 0);
                const missing = analysis !== null && !found.has(op.region);
                return <label key={op.op} className="studio-control portrait-slider" title={op.hint}>
                  <span>{op.label}<small>{missing ? ' · not found here' : ` · ${value}`}</small></span>
                  <input type="range" min={0} max={100} step={1} value={value} aria-label={op.label}
                    onChange={(event) => props.onStrength(op.op, Number(event.target.value) / 100)}
                    onPointerUp={props.onCommit} onKeyUp={props.onCommit} onBlur={props.onCommit} />
                </label>;
              })}
            </div>
          ))}
          <div className="portrait-group">
            <h3>Adjust one region</h3>
            <select aria-label="Region to adjust" value={adjustment?.region ?? ''} onChange={(event) => props.onAdjustRegion(event.target.value)}>
              <option value="">Choose a region…</option>
              {(analysis?.regions ?? []).map((r) => <option key={r.region} value={r.region}>{r.label}</option>)}
            </select>
            {adjustment && ADJUSTMENTS.map((a) => (
              <label key={a.field} className="studio-control portrait-slider">
                <span>{a.label}<small> · {Number(adjustment[a.field] ?? 0)}</small></span>
                <input type="range" min={a.min} max={a.max} step={a.step} value={Number(adjustment[a.field] ?? 0)} aria-label={a.label}
                  onChange={(event) => props.onAdjust(a.field, Number(event.target.value))}
                  onPointerUp={props.onCommit} onKeyUp={props.onCommit} onBlur={props.onCommit} />
              </label>
            ))}
          </div>
          <button type="button" onClick={props.onClear} data-testid="portrait-clear">Remove all retouching</button>
          {settings && settings.foreignOps.length > 0 && <p className="studio-footnote">
            Earlier retouch steps this build cannot apply: {settings.foreignOps.join(', ')}.
          </p>}
        </fieldset>
      </div>
    </section>
  );
}

/** A region adjustment with its untouched fields left out, so a slider at zero adjusts nothing. */
export function withoutZeros(adjustment: RegionAdjustment): RegionAdjustment {
  const out: RegionAdjustment = { region: adjustment.region };
  for (const field of ['exposure', 'contrast', 'saturation', 'warmth', 'shadows', 'highlights'] as const) {
    const value = adjustment[field];
    if (typeof value === 'number' && value !== 0) out[field] = value;
  }
  return out;
}

/** The workspace, wired to the desktop. */
export function PortraitRetouch({ projectId, photoId, disabled, onBusyChange }: {
  projectId: string; photoId: string; disabled: boolean; onBusyChange: (busy: boolean) => void;
}): JSX.Element {
  const [analysis, setAnalysis] = useState<PortraitAnalysis | null>(null);
  const [settings, setSettings] = useState<PortraitSettings | null>(null);
  const [strengths, setStrengths] = useState<Record<string, number>>({});
  const [adjustments, setAdjustments] = useState<RegionAdjustment[]>([]);
  const [selected, setSelected] = useState<string>('');
  const [imageUrl, setImageUrl] = useState<string | null>(null);
  const [originalUrl, setOriginalUrl] = useState<string | null>(null);
  const [comparing, setComparing] = useState(false);
  const [overlay, setOverlay] = useState<string[]>(DEFAULT_OVERLAY);
  const [showFaces, setShowFaces] = useState(true);
  const [drawing, setDrawing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [explanation, setExplanation] = useState<string[]>([]);
  const dirty = useRef(false);

  useEffect(() => { onBusyChange(busy); }, [busy, onBusyChange]);

  const adopt = useCallback((next: PortraitSettings) => {
    setSettings(next);
    setStrengths(Object.fromEntries(next.ops.map((op) => [op.op, op.strength])));
    setAdjustments(next.adjustments);
  }, []);

  const renderNow = useCallback(async () => {
    const render = await develop.renderImage({ photoId, level: 'screen', screen: [1400, 1000], purpose: 'interactive' });
    setImageUrl(rgbDataUrl(render));
  }, [photoId]);

  const analyseNow = useCallback(async () => {
    setAnalysis(await portrait.analyse(photoId));
  }, [photoId]);

  useEffect(() => {
    let active = true;
    setAnalysis(null); setSettings(null); setImageUrl(null); setOriginalUrl(null); setError(null); setExplanation([]);
    if (!inTauri()) return;
    void Promise.all([
      portrait.analyse(photoId), portrait.settings(photoId),
      develop.renderImage({ photoId, level: 'screen', screen: [1400, 1000], purpose: 'interactive' }),
      api.getPreview({ projectId, photoId, level: 'proxy', priority: 'interactive' }),
    ]).then(([nextAnalysis, nextSettings, render, preview]) => {
      if (!active) return;
      setAnalysis(nextAnalysis); adopt(nextSettings); setImageUrl(rgbDataUrl(render)); setOriginalUrl(preview.dataUrl);
    }).catch((cause) => { if (active) setError(asIpcError(cause).message); });
    return () => { active = false; };
  }, [adopt, photoId, projectId]);

  const run = useCallback(async (action: () => Promise<void>) => {
    setBusy(true); setError(null);
    try { await action(); } catch (cause) { setError(asIpcError(cause).message); } finally { setBusy(false); }
  }, []);

  const save = useCallback((hints?: [number, number, number, number][] | null, reanalyse = false) => run(async () => {
    const next = await portrait.set({
      projectId, photoId,
      ops: Object.entries(strengths).filter(([, strength]) => strength > 0).map(([op, strength]) => ({ op, strength })),
      adjustments: adjustments.map(withoutZeros).filter((a) => Object.keys(a).length > 1),
      hints: hints ?? null,
      label: 'Portrait retouch',
    });
    adopt(next);
    setExplanation([]);
    if (reanalyse) await analyseNow();
    await renderNow();
  }), [adjustments, adopt, analyseNow, photoId, projectId, renderNow, run, strengths]);

  const adjustment = adjustments.find((a) => a.region === selected) ?? (selected ? { region: selected } : null);

  return <PortraitRetouchView
    analysis={analysis} settings={settings} strengths={strengths} adjustment={adjustment}
    imageUrl={imageUrl} originalUrl={originalUrl} comparing={comparing} overlay={overlay}
    showFaces={showFaces} drawing={drawing} busy={busy || disabled} error={error} explanation={explanation}
    onToggleRegion={(region) => setOverlay((now) => now.includes(region) ? now.filter((r) => r !== region) : [...now, region])}
    onToggleFaces={() => setShowFaces((now) => !now)}
    onStrength={(op, value) => { dirty.current = true; setStrengths((now) => ({ ...now, [op]: value })); }}
    onCommit={() => { if (dirty.current) { dirty.current = false; void save(); } }}
    onAuto={(style) => void run(async () => {
      const next = await portrait.auto(projectId, photoId, style);
      adopt(next); setExplanation(next.explanation);
      await renderNow();
    })}
    onClear={() => { setStrengths({}); setAdjustments([]); dirty.current = false; void run(async () => {
      adopt(await portrait.set({ projectId, photoId, ops: [], adjustments: [], hints: null, label: 'Removed portrait retouch' }));
      await renderNow();
    }); }}
    onCompare={setComparing}
    onDrawing={setDrawing}
    onDrawFace={(box) => { setDrawing(false); void save([...(settings?.hints ?? []), box], true); }}
    onClearFaces={() => void save([], true)}
    onAdjustRegion={setSelected}
    onAdjust={(field, value) => {
      if (!selected) return;
      dirty.current = true;
      setAdjustments((now) => {
        const rest = now.filter((a) => a.region !== selected);
        const current = now.find((a) => a.region === selected) ?? { region: selected };
        return [...rest, { ...current, [field]: value }];
      });
    }}
  />;
}
