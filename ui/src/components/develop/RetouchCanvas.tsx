import { useEffect, useId, useMemo, useRef, useState, type PointerEvent } from 'react';
import type { BrushPoint, BrushStroke, NativeRetouchEdit } from '../../ipc/nativeRetouch';

export type RetouchMode = 'ellipse' | 'paint' | 'erase' | 'pan' | 'gradient';
type Props = {
  src: string | null; width: number; height: number; compare: boolean; disabled: boolean;
  beforeSrc?: string | null; split?: boolean;
  maskView?: boolean;
  coverageView?: boolean;
  draft: NativeRetouchEdit; mode: RetouchMode; radius: number; opacity: number;
  overlay: boolean; sourceMode: boolean;
  onTarget: (point: [number, number]) => void; onSource: (point: [number, number]) => void;
  onStroke: (stroke: BrushStroke) => void; onNotice: (message: string) => void;
  onGradient?: (start: [number, number], end: [number, number]) => void;
};

export function RetouchCanvas(props: Props) {
  const { src, width, height, draft, mode, disabled, compare, overlay } = props;
  const viewport = useRef<HTMLDivElement>(null);
  const surface = useRef<HTMLDivElement>(null);
  const gesture = useRef<{ id: number; x: number; y: number; left: number; top: number; stroke: BrushStroke | null; gradient?: [number, number] } | null>(null);
  const [active, setActive] = useState<BrushStroke | null>(null);
  const [cursor, setCursor] = useState<BrushPoint | null>(null);
  const [activeGradient, setActiveGradient] = useState<{start:[number,number];end:[number,number]}|null>(null);
  const [size, setSize] = useState({ width: 800, height: 600 });
  const [zoom, setZoom] = useState(1);
  const [splitPosition, setSplitPosition] = useState(50);
  const dividerPointer = useRef<number | null>(null);
  const split = Boolean(props.split && props.beforeSrc && src && !compare);
  useEffect(() => {
    dividerPointer.current = null;
    gesture.current = null;
    setActive(null);
    setCursor(null);
    setActiveGradient(null);
  }, [split, props.maskView, props.coverageView]);
  const maskId = useId().replaceAll(':', '');
  useEffect(() => {
    const element = viewport.current;
    if (!element) return;
    const measure = () => setSize({ width: element.clientWidth || 800, height: element.clientHeight || 600 });
    measure();
    if (typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(measure); observer.observe(element);
    return () => observer.disconnect();
  }, []);
  const fit = Math.min(size.width / width, size.height / height, 1);
  const imageWidth = width * fit * zoom;
  const imageHeight = height * fit * zoom;
  const canvasWidth = Math.max(size.width, imageWidth);
  const canvasHeight = Math.max(size.height, imageHeight);
  useEffect(() => {
    const element = viewport.current;
    if (element) { element.scrollLeft = (canvasWidth - size.width) / 2; element.scrollTop = (canvasHeight - size.height) / 2; }
  }, [zoom, canvasWidth, canvasHeight, size]);
  const setScale = (value: number) => setZoom(Math.min(8, Math.max(1, value)));
  const position = (event: PointerEvent): BrushPoint => {
    const rect = surface.current?.getBoundingClientRect();
    if (!rect || !rect.width || !rect.height) return [.5, .5, 1];
    return [Math.max(0, Math.min(1, (event.clientX - rect.left) / rect.width)),
      Math.max(0, Math.min(1, (event.clientY - rect.top) / rect.height)),
      event.pointerType === 'pen' ? Math.max(.1, Math.min(1, event.pressure || .1)) : 1];
  };
  const start = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 && event.button !== 1) return;
    if (gesture.current) return;
    const pan = mode === 'pan' || event.button === 1 || split || props.maskView || props.coverageView;
    if (!pan && (disabled || compare || !src)) return;
    event.preventDefault();
    event.currentTarget.focus({ preventScroll: true });
    if (!pan && (event.altKey || props.sourceMode)) { const [x, y] = position(event); props.onSource([x, y]); return; }
    if (!pan && mode === 'ellipse') { const [x, y] = position(event); props.onTarget([x, y]); return; }
    const stroke = pan || mode === 'gradient' ? null : { erase: mode === 'erase', radius: props.radius, opacity: props.opacity, points: [position(event)] };
    const [gx,gy] = position(event);
    const gradient: [number,number] | undefined = !pan && mode === 'gradient' ? [gx,gy] : undefined;
    gesture.current = { id: event.pointerId, x: event.clientX, y: event.clientY,
      left: viewport.current?.scrollLeft ?? 0, top: viewport.current?.scrollTop ?? 0, stroke, gradient };
    event.currentTarget.setPointerCapture?.(event.pointerId);
    setActive(stroke);
    setActiveGradient(gradient?{start:gradient,end:gradient}:null);
  };
  const move = (event: PointerEvent<HTMLDivElement>) => {
    if (mode === 'paint' || mode === 'erase') setCursor(position(event));
    const current = gesture.current;
    if (!current || current.id !== event.pointerId) return;
    if (current.gradient) {
      const [x,y] = position(event); setActiveGradient({start:current.gradient,end:[x,y]}); return;
    }
    if (!current.stroke) {
      if (viewport.current) {
        viewport.current.scrollLeft = current.left - (event.clientX - current.x);
        viewport.current.scrollTop = current.top - (event.clientY - current.y);
      }
      return;
    }
    const point = position(event);
    const last = current.stroke.points[current.stroke.points.length - 1];
    if (last && Math.hypot((point[0] - last[0]) * width, (point[1] - last[1]) * height) < .75 && Math.abs(point[2] - last[2]) < .03) return;
    if (current.stroke.points.length >= 1024) { props.onNotice('Stroke limit reached. Release and start another stroke.'); return; }
    current.stroke = { ...current.stroke, points: [...current.stroke.points, point] };
    setActive(current.stroke);
  };
  const finish = (event: PointerEvent<HTMLDivElement>, cancel = false) => {
    const current = gesture.current;
    if (!current || current.id !== event.pointerId) return;
    gesture.current = null; setActive(null); setActiveGradient(null);
    if (event.currentTarget.hasPointerCapture?.(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
    if (!cancel && !disabled && !compare && !split && !props.maskView && !props.coverageView && current.gradient) {
      const [x,y] = position(event);
      if (Math.hypot(current.gradient[0]-x,current.gradient[1]-y)<.001) props.onNotice('Drag a longer gradient, or enter its coordinates.');
      else props.onGradient?.(current.gradient,[x,y]);
    }
    if (!cancel && !disabled && !compare && !split && !props.maskView && !props.coverageView && current.stroke) {
      const point = position(event);
      const last = current.stroke.points[current.stroke.points.length - 1];
      if (last && current.stroke.points.length < 1024 && (point[0] !== last[0] || point[1] !== last[1])) {
        point[2] = last[2];
        current.stroke.points.push(point);
      }
      props.onStroke(current.stroke);
    }
  };
  const strokeShape = (stroke: BrushStroke, index: number) => <g key={index} opacity={stroke.opacity} fill={stroke.erase ? 'black' : 'white'} stroke={stroke.erase ? 'black' : 'white'}>
    {stroke.points.map((point, p) => {
      const next = stroke.points[p + 1] ?? point;
      const radius = stroke.radius * Math.min(width, height);
      return <line key={p} x1={point[0] * width} y1={point[1] * height} x2={next[0] * width} y2={next[1] * height}
        strokeWidth={radius * (Math.max(.1, point[2]) + Math.max(.1, next[2]))} strokeLinecap="round"/>;
    })}
  </g>;
  // Previously painted paths are unchanged during pointer movement; only the active stroke redraws.
  const paintedGuide = useMemo(() => (draft.mask?.strokes ?? []).map(strokeShape), [draft.mask, width, height]);
  const gradientGuide = activeGradient ?? draft.selection?.gradient;
  const moveDivider = (event: PointerEvent<HTMLDivElement>) => {
    event.stopPropagation();
    if (dividerPointer.current !== event.pointerId) return;
    setSplitPosition(Math.round(position(event)[0] * 100));
  };
  const releaseDivider = (event: PointerEvent<HTMLDivElement>) => {
    event.stopPropagation();
    if (dividerPointer.current !== event.pointerId) return;
    dividerPointer.current = null;
    if (event.currentTarget.hasPointerCapture?.(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
  };
  return <div className="retouch-canvas">
    <div className="retouch-view-tools" aria-label="Photo zoom">
      <button type="button" onClick={() => setScale(1)}>Fit</button>
      <button type="button" disabled={zoom <= 1} onClick={() => setScale(zoom / 1.5)} aria-label="Zoom out">−</button>
      <output aria-live="polite">{Math.round(fit * zoom * 100)}% preview</output>
      <button type="button" disabled={zoom >= 8} onClick={() => setScale(zoom * 1.5)} aria-label="Zoom in">+</button>
      <button type="button" onClick={() => setScale(1 / fit)}>1:1 preview</button>
      <span>Middle-drag or Hand to pan · + / − to zoom · 0 to fit</span>
    </div>
    {split && <div className="retouch-comparison-tools">
      <label>Before/after split<input type="range" min="0" max="100" step="1" value={splitPosition}
        aria-valuetext={`${splitPosition}% before, ${100 - splitPosition}% retouched`}
        onChange={event => setSplitPosition(Number(event.target.value))}/></label>
      <button type="button" onClick={() => setSplitPosition(50)}>Center divider</button>
      <p>Before is on the left; retouched is on the right. Drag the divider or use the slider. Drag the photo to pan.</p>
    </div>}
    <div className="retouch-viewport" ref={viewport} role="region" aria-label="Retouch photo viewport" tabIndex={0}
      onKeyDown={event => {
        if (event.target !== event.currentTarget && event.target !== surface.current) return;
        if (event.key === '+' || event.key === '=') setScale(zoom * 1.5);
        else if (event.key === '-') setScale(zoom / 1.5);
        else if (event.key === '0') setScale(1);
        else if (event.key.startsWith('Arrow')) {
          const element = viewport.current;
          if (element) { element.scrollLeft += event.key === 'ArrowRight' ? 80 : event.key === 'ArrowLeft' ? -80 : 0;
            element.scrollTop += event.key === 'ArrowDown' ? 80 : event.key === 'ArrowUp' ? -80 : 0; }
        } else return;
        event.preventDefault(); event.stopPropagation();
      }}>
      <div style={{ width: canvasWidth, height: canvasHeight, position: 'relative' }}>
        <div ref={surface} className="retouch-photo-surface" role="group" tabIndex={0} aria-label="Retouch image interaction"
          onPointerDown={start} onPointerMove={move} onPointerUp={event => finish(event)} onPointerCancel={event => finish(event, true)}
          onPointerLeave={() => setCursor(null)}
          onLostPointerCapture={event => finish(event, true)}
          style={{ width: imageWidth, height: imageHeight, left: (canvasWidth - imageWidth) / 2, top: (canvasHeight - imageHeight) / 2,
            cursor: mode === 'pan' || split || props.maskView || props.coverageView ? 'grab' : props.sourceMode ? 'copy' : 'crosshair' }}>
          {src ? <img src={src} draggable={false} alt={props.coverageView ? 'Saved retouch coverage' : props.maskView ? 'Selection mask' : compare ? 'Before native retouch' : 'Retouched photograph'}/> : <p>Loading retouch preview…</p>}
          {split && <>
            <img className="retouch-before-layer" src={props.beforeSrc ?? undefined} draggable={false} alt="Before native retouch comparison"
              style={{ clipPath: `inset(0 ${100 - splitPosition}% 0 0)` }}/>
            <div className="retouch-compare-labels" aria-hidden="true"><span>Before</span><span>Retouched</span></div>
            <div className="retouch-compare-divider" style={{ left: `${splitPosition}%` }} aria-hidden="true"
              onPointerDown={event => {
                event.stopPropagation();
                if (event.button !== 0 || dividerPointer.current !== null) return;
                event.preventDefault(); dividerPointer.current = event.pointerId;
                event.currentTarget.setPointerCapture?.(event.pointerId); moveDivider(event);
              }} onPointerMove={moveDivider} onPointerUp={releaseDivider} onPointerCancel={releaseDivider}
              onLostPointerCapture={releaseDivider}><span>↔</span></div>
          </>}
          {src && overlay && !compare && !split && !props.maskView && !props.coverageView && <svg aria-hidden="true" viewBox={`0 0 ${width} ${height}`}>
            {gradientGuide ? <>
              <line x1={gradientGuide.start[0]*width} y1={gradientGuide.start[1]*height} x2={gradientGuide.end[0]*width} y2={gradientGuide.end[1]*height}
                stroke="white" strokeWidth="2" vectorEffect="non-scaling-stroke"/>
              {[gradientGuide.start,gradientGuide.end].map((p,i)=><circle key={i} cx={p[0]*width} cy={p[1]*height} r={6/(fit*zoom)} fill={(i===1)!==Boolean(draft.selection?.inverted)?'white':'black'} stroke="white" vectorEffect="non-scaling-stroke"/>)}
            </> : draft.mask || active ? <>
              <defs><mask id={maskId} maskUnits="userSpaceOnUse" x="0" y="0" width={width} height={height}>
                <rect width={width} height={height} fill="black"/>{paintedGuide}{active && strokeShape(active, 128)}
              </mask></defs>
              <rect width={width} height={height} fill="rgba(161,130,255,.4)" mask={`url(#${maskId})`}/>
            </> : <ellipse cx={draft.region[0] * width} cy={draft.region[1] * height} rx={draft.region[2] * width} ry={draft.region[3] * height}
              fill="rgba(161,130,255,.15)" stroke="white" strokeWidth="1.5" vectorEffect="non-scaling-stroke"/>}
            {draft.source && <><circle cx={draft.source[0] * width} cy={draft.source[1] * height} r={6 / (fit * zoom)} fill="none" stroke="white" strokeWidth="1.5" vectorEffect="non-scaling-stroke"/>
              <line x1={draft.source[0] * width} y1={draft.source[1] * height} x2={draft.region[0] * width} y2={draft.region[1] * height} stroke="white" strokeDasharray="4 4" vectorEffect="non-scaling-stroke"/></>}
            {cursor && (mode === 'paint' || mode === 'erase') && <circle cx={cursor[0] * width} cy={cursor[1] * height}
              r={props.radius * Math.min(width, height) * cursor[2]} fill="none" stroke={mode === 'erase' ? '#ffcfad' : 'white'} strokeWidth="1.5" vectorEffect="non-scaling-stroke"/>}
          </svg>}
        </div>
      </div>
    </div>
    <p className="lr-hint">{props.coverageView ? 'Saved retouch only, nothing here is edited. Drag to pan or zoom in to inspect any area.' : props.maskView ? 'White is selected; black is protected. Skin and spot tools may affect a smaller area. Turn off the mask preview to draw.' : split ? 'Comparison only. Turn off Split comparison to paint or choose a source. Both views use the same zoom and pan.' : draft.selection?.gradient ? 'Drag to place a gradient. White selects; black protects. Preview selection mask shows brightness limits too.' : draft.mask ? `${draft.mask.strokes.length} mask strokes. Selection guide shows the painted area; Preview selection mask shows inversion, feathering and brightness limits.` : 'Click to place an ellipse. Preview selection mask shows inversion, feathering and brightness limits.'}</p>
  </div>;
}
