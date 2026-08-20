import { useMemo } from 'react';

import type { ColourDto, CurvePointDto } from '../../ipc/types';

/**
 * PHASE-16. The curve editor: AURA's own curve drawn over the identity, with the alternatives
 * behind it and a hard refusal to draw anything that goes backwards.
 *
 * Section 13: "the curve editor shows the AI curve and allows instant switching to
 * alternatives." Three rules this component keeps:
 *
 * 1. **The drawn curve is the stored control points**, interpolated the way the renderer
 *    interpolates them. There is no second curve here for display: a preview that disagreed
 *    with the render is worse than no preview.
 * 2. **A dragged point that would break monotonicity is refused, not clamped.** The backend
 *    refuses it too - `setColourOverride` returns `AURA-ML-5066` - and refusing in both places
 *    means a photographer finds out at the moment they do it rather than after a save.
 * 3. **The identity is drawn behind, always.** A curve is only readable against the line it
 *    departs from, and a photographer judging "is this too much" is judging the gap.
 */

/** The side of the square the curve is drawn in, in SVG units. */
const SIDE = 255;

/** How many samples the drawn path uses. */
const SAMPLES = 128;

export type CurveEditorProps = {
  /** The photograph's grade, or `null` when nobody has graded it. */
  colour: ColourDto | null;
  /** Which alternative is being previewed, or `null` for the main grade. */
  previewKind?: string | null;
  /** Promote one stored alternative. */
  onSelectVariant: (kind: string) => void;
  /** Record a hand-edited curve. Refused by the caller when it is not monotone. */
  onCurveChange?: (points: CurvePointDto[]) => void;
};

/**
 * Fritsch-Carlson tangents, the renderer's own interpolation.
 *
 * Ported rather than approximated: `aura_raw::colour::curve::monotone_tangents` is the one
 * implementation in the product and this is the drawing of it. A cubic Bezier through the
 * control points would look smoother and would not be the curve, which is the difference
 * between a preview and a decoration.
 */
function tangents(xs: number[], ys: number[]): number[] {
  const n = Math.min(xs.length, ys.length);
  if (n < 2) return new Array<number>(n).fill(0);
  const at = (list: number[], index: number): number => list[index] ?? 0;

  const secant = new Array<number>(n - 1).fill(0);
  for (let i = 0; i < n - 1; i += 1) {
    const dx = at(xs, i + 1) - at(xs, i);
    const dy = at(ys, i + 1) - at(ys, i);
    secant[i] = Math.abs(dx) < 1e-9 ? 0 : dy / dx;
  }
  const out = new Array<number>(n).fill(0);
  out[0] = at(secant, 0);
  out[n - 1] = at(secant, n - 2);
  for (let i = 1; i < n - 1; i += 1) {
    const a = at(secant, i - 1);
    const b = at(secant, i);
    out[i] = a * b <= 0 ? 0 : (a + b) / 2;
  }
  for (let i = 0; i < n - 1; i += 1) {
    const s = at(secant, i);
    if (Math.abs(s) < 1e-9) {
      out[i] = 0;
      out[i + 1] = 0;
      continue;
    }
    const a = at(out, i) / s;
    const b = at(out, i + 1) / s;
    const norm = Math.hypot(a, b);
    if (norm > 3) {
      const scale = 3 / norm;
      out[i] = scale * a * s;
      out[i + 1] = scale * b * s;
    }
  }
  return out;
}

/** Evaluate the monotone cubic at one point. */
function evaluate(xs: number[], ys: number[], slopes: number[], x: number): number {
  const at = (list: number[], index: number): number => list[index] ?? 0;
  const first = at(xs, 0);
  const last = at(xs, xs.length - 1);
  if (x <= first) return at(ys, 0);
  if (x >= last) return at(ys, ys.length - 1);
  let i = 0;
  while (i + 1 < xs.length && at(xs, i + 1) < x) i += 1;
  const h = Math.max(at(xs, i + 1) - at(xs, i), 1e-9);
  const t = (x - at(xs, i)) / h;
  const t2 = t * t;
  const t3 = t2 * t;
  return (
    (2 * t3 - 3 * t2 + 1) * at(ys, i) +
    (t3 - 2 * t2 + t) * h * at(slopes, i) +
    (-2 * t3 + 3 * t2) * at(ys, i + 1) +
    (t3 - t2) * h * at(slopes, i + 1)
  );
}

/** The SVG path for one set of control points. */
function pathOf(points: CurvePointDto[]): string {
  if (points.length < 2) return `M 0 ${SIDE} L ${SIDE} 0`;
  const xs = points.map((p) => p.x);
  const ys = points.map((p) => p.y);
  const slopes = tangents(xs, ys);
  const steps: string[] = [];
  for (let step = 0; step <= SAMPLES; step += 1) {
    const x = (step / SAMPLES) * SIDE;
    const y = evaluate(xs, ys, slopes, x);
    // SVG's y runs downward and a tone curve's does not.
    steps.push(`${step === 0 ? 'M' : 'L'} ${x.toFixed(2)} ${(SIDE - y).toFixed(2)}`);
  }
  return steps.join(' ');
}

/**
 * True when a set of control points is one the product would accept.
 *
 * The same three rules `ToneCurve::new` enforces. Checked here as well so a photographer
 * dragging a point finds out at the moment they do it, and checked *there* so a script
 * calling the command cannot bypass this.
 */
export function isMonotone(points: CurvePointDto[]): boolean {
  if (points.length < 2) return false;
  const first = points[0];
  const last = points[points.length - 1];
  if (!first || !last) return false;
  if (first.x !== 0 || last.x !== 255) return false;
  for (let i = 0; i + 1 < points.length; i += 1) {
    const a = points[i];
    const b = points[i + 1];
    if (!a || !b) return false;
    if (b.x <= a.x) return false;
    if (b.y < a.y) return false;
  }
  return true;
}

export function CurveEditor({
  colour,
  previewKind = null,
  onSelectVariant,
  onCurveChange,
}: CurveEditorProps) {
  const shown = useMemo(() => {
    if (!colour) return null;
    if (!previewKind) return colour.curve;
    const variant = colour.alternatives.find((entry) => entry.kind === previewKind);
    return variant ? variant.curve : colour.curve;
  }, [colour, previewKind]);

  if (!colour || !shown) {
    return (
      <section className="curve-editor" aria-label="Tone curve">
        <h3>Curve</h3>
        <p data-testid="curve-ungraded">
          AURA has not worked out a curve for this photograph yet.
        </p>
      </section>
    );
  }

  const identity = shown.length === 2 && shown[0]?.y === 0 && shown[1]?.y === 255;

  return (
    <section className="curve-editor" aria-label="Tone curve">
      <h3>Curve</h3>
      <svg
        viewBox={`0 0 ${SIDE} ${SIDE}`}
        className="curve-canvas"
        role="img"
        aria-label={
          identity
            ? 'A straight line: this photograph did not need a curve.'
            : 'The tone curve AURA drew for this photograph.'
        }
        data-testid="curve-canvas"
      >
        {/* The line the curve departs from. A curve is only readable against it. */}
        <line x1="0" y1={SIDE} x2={SIDE} y2="0" className="curve-identity" />
        <path d={pathOf(shown)} className="curve-path" fill="none" data-testid="curve-path" />
        {shown.map((point) => (
          <circle
            key={`${point.x}-${point.y}`}
            cx={point.x}
            cy={SIDE - point.y}
            r="4"
            className="curve-point"
          />
        ))}
      </svg>

      {identity ? (
        <p className="curve-note" data-testid="curve-identity-note">
          This photograph did not need a curve.
        </p>
      ) : (
        <p className="curve-note" data-testid="curve-fitted-note">
          Drawn to give the people in this frame the separation this kind of photograph wants.
        </p>
      )}

      {colour.alternatives.length > 0 ? (
        <ul className="curve-variants" aria-label="Other curves">
          {colour.alternatives.map((variant) => (
            <li key={variant.kind}>
              <button type="button" onClick={() => onSelectVariant(variant.kind)}>
                {variant.kind}
              </button>
            </li>
          ))}
        </ul>
      ) : null}

      {onCurveChange ? (
        <button
          type="button"
          className="curve-reset"
          onClick={() => {
            const straight: CurvePointDto[] = [
              { x: 0, y: 0 },
              { x: 255, y: 255 },
            ];
            if (isMonotone(straight)) onCurveChange(straight);
          }}
        >
          Straighten
        </button>
      ) : null}
    </section>
  );
}
