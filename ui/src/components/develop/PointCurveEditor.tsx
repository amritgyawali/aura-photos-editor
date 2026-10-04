import { useEffect, useRef, useState } from 'react';

export type CurvePoint = [number, number];

type Props = {
  points: CurvePoint[];
  /** Line colour: the channel the curve belongs to. */
  colour: string;
  disabled: boolean;
  /** Called once per gesture, when the pointer is released. */
  onCommit: (points: CurvePoint[]) => void;
};

const SIZE = 256;
const IDENTITY: CurvePoint[] = [[0, 0], [255, 255]];

/** Keep a curve valid for the recipe: first x 0, last x 255, x strictly increasing, 0..255. */
export function normaliseCurve(points: CurvePoint[]): CurvePoint[] {
  const sorted = points
    .map(([x, y]) => [Math.round(Math.min(255, Math.max(0, x))), Math.round(Math.min(255, Math.max(0, y)))] as CurvePoint)
    .sort((a, b) => a[0] - b[0]);
  const out: CurvePoint[] = [];
  for (const point of sorted) {
    const last = out[out.length - 1];
    if (last && point[0] <= last[0]) continue;
    out.push(point);
  }
  const first = out[0];
  if (!first || first[0] !== 0) out.unshift([0, first ? Math.min(first[1], 255) : 0]);
  const end = out[out.length - 1];
  if (!end || end[0] !== 255) out.push([255, end ? Math.max(end[1], 0) : 255]);
  return out.length >= 2 ? out : IDENTITY;
}

/** A monotone-cubic preview of the curve, for drawing only; the renderer does the real one. */
function path(points: CurvePoint[]): string {
  if (points.length < 2) return '';
  const n = points.length;
  const xs = points.map(p => p[0]);
  const ys = points.map(p => p[1]);
  const d: number[] = [];
  for (let i = 0; i < n - 1; i += 1) d.push(((ys[i + 1] ?? 0) - (ys[i] ?? 0)) / Math.max(1, (xs[i + 1] ?? 0) - (xs[i] ?? 0)));
  const m: number[] = xs.map((_, i) => {
    if (i === 0) return d[0] ?? 0;
    if (i === n - 1) return d[n - 2] ?? 0;
    const a = d[i - 1] ?? 0;
    const b = d[i] ?? 0;
    return a * b <= 0 ? 0 : (a + b) / 2;
  });
  let out = `M ${xs[0]} ${SIZE - 1 - (ys[0] ?? 0)}`;
  for (let i = 0; i < n - 1; i += 1) {
    const x0 = xs[i] ?? 0;
    const x1 = xs[i + 1] ?? 0;
    const h = (x1 - x0) / 3;
    const c1y = (ys[i] ?? 0) + (m[i] ?? 0) * h;
    const c2y = (ys[i + 1] ?? 0) - (m[i + 1] ?? 0) * h;
    out += ` C ${x0 + h} ${SIZE - 1 - c1y} ${x1 - h} ${SIZE - 1 - c2y} ${x1} ${SIZE - 1 - (ys[i + 1] ?? 0)}`;
  }
  return out;
}

/**
 * Lightroom's point curve: click to add a point, drag to move it, double-click to remove it.
 * The black and white ends move vertically only, so the curve always spans the range.
 */
export function PointCurveEditor({ points, colour, disabled, onCommit }: Props): JSX.Element {
  const [draft, setDraft] = useState<CurvePoint[]>(() => normaliseCurve(points));
  const dragging = useRef<number | null>(null);
  const moved = useRef(false);
  const svg = useRef<SVGSVGElement | null>(null);
  useEffect(() => { if (dragging.current === null) setDraft(normaliseCurve(points)); }, [points]);

  const toCurve = (event: { clientX: number; clientY: number }): CurvePoint => {
    const box = svg.current?.getBoundingClientRect();
    if (!box || box.width === 0) return [0, 0];
    return [((event.clientX - box.left) / box.width) * 255, (1 - (event.clientY - box.top) / box.height) * 255];
  };

  const down = (event: React.PointerEvent<SVGSVGElement>) => {
    if (disabled) return;
    const [x, y] = toCurve(event);
    const hit = draft.findIndex(([px, py]) => Math.hypot(px - x, py - y) < 12);
    moved.current = false;
    if (hit >= 0) { dragging.current = hit; }
    else {
      const next = normaliseCurve([...draft, [x, y]]);
      setDraft(next);
      dragging.current = next.findIndex(([px]) => px === Math.round(x));
      moved.current = true;
    }
    (event.target as Element).setPointerCapture?.(event.pointerId);
  };

  const move = (event: React.PointerEvent<SVGSVGElement>) => {
    const index = dragging.current;
    if (index === null || disabled) return;
    const [x, y] = toCurve(event);
    setDraft(current => {
      const next = current.map(p => [...p] as CurvePoint);
      const point = next[index];
      if (!point) return current;
      const isEnd = index === 0 || index === next.length - 1;
      const left = next[index - 1]?.[0] ?? -1;
      const right = next[index + 1]?.[0] ?? 256;
      point[0] = isEnd ? point[0] : Math.round(Math.min(right - 1, Math.max(left + 1, x)));
      point[1] = Math.round(Math.min(255, Math.max(0, y)));
      return next;
    });
    moved.current = true;
  };

  const up = () => {
    if (dragging.current === null) return;
    dragging.current = null;
    if (moved.current) onCommit(normaliseCurve(draft));
  };

  const remove = (event: React.MouseEvent<SVGSVGElement>) => {
    if (disabled) return;
    const [x, y] = toCurve(event);
    const hit = draft.findIndex(([px, py], i) => i > 0 && i < draft.length - 1 && Math.hypot(px - x, py - y) < 12);
    if (hit < 0) return;
    const next = draft.filter((_, i) => i !== hit);
    setDraft(next);
    onCommit(next);
  };

  return <svg ref={svg} className="curve-editor" viewBox={`0 0 ${SIZE} ${SIZE}`} role="img" aria-label="Tone curve"
    onPointerDown={down} onPointerMove={move} onPointerUp={up} onPointerLeave={up} onDoubleClick={remove}>
    {[64, 128, 192].map(v => <g key={v}><line x1={v} y1={0} x2={v} y2={SIZE} className="curve-grid" /><line x1={0} y1={v} x2={SIZE} y2={v} className="curve-grid" /></g>)}
    <line x1={0} y1={SIZE - 1} x2={SIZE - 1} y2={0} className="curve-diagonal" />
    <path d={path(draft)} fill="none" stroke={colour} strokeWidth={2} />
    {draft.map(([x, y], i) => <circle key={`${i}:${x}`} cx={x} cy={SIZE - 1 - y} r={5} className="curve-point" />)}
  </svg>;
}
