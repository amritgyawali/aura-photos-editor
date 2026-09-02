import type { CurateSpreadDto } from '../../ipc/types';

/**
 * PHASE-29. One spread in detail: both pages, the four pairing measurements, and why.
 *
 * Pure - rows and callbacks in, nothing fetched.
 *
 * ## `facingKnown` is why this component exists as its own view
 *
 * A spread whose subjects' facing could not be measured is not a spread whose subjects face
 * outward. Rendering a zero facing score as a failed pairing would report a defect in every spread
 * of every album on this build, because phase 06's detector finds no faces. So the term is drawn in
 * grey and says "not measured" rather than showing a bar at zero.
 *
 * ## The four measurements travel together
 *
 * A photographer who disagrees with a pairing wants to know *which* of the four the optimiser was
 * happy with. "These two are the same tonal weight but one is much warmer" is actionable; a single
 * number is not.
 */
export type SpreadViewProps = {
  /** The spread being looked at, or null. */
  spread: CurateSpreadDto | null;
};

/** One measurement, with an honest rendering of an unmeasured one. */
function Term({
  label,
  value,
  known,
  detail,
}: {
  label: string;
  value: number;
  known: boolean;
  detail: string;
}) {
  return (
    <div className="spread-term" data-known={known}>
      <dt>{label}</dt>
      <dd>{known ? detail : 'not measured'}</dd>
      {known ? (
        <span className="spread-bar" style={{ width: `${Math.min(100, value * 100)}%` }} />
      ) : null}
    </div>
  );
}

export function SpreadView({ spread }: SpreadViewProps) {
  if (!spread) {
    return (
      <section className="curate-spread" aria-label="Spread">
        <p className="empty">Choose a spread to see why these two pages are together.</p>
      </section>
    );
  }

  return (
    <section className="curate-spread" aria-label="Spread">
      <header>
        <h4>
          Spread {spread.index + 1} · {spread.chapter.replace(/_/g, ' ')}
        </h4>
        {spread.single ? <span className="spread-single">a single page</span> : null}
      </header>

      <div className="spread-pages">
        <div className="spread-page" data-side="left">
          {spread.left ?? <span className="blank">blank</span>}
        </div>
        <div className="spread-page" data-side="right">
          {spread.right ?? <span className="blank">blank</span>}
        </div>
      </div>

      {spread.single ? null : (
        <dl className="spread-terms">
          <Term
            label="tonal weight"
            value={1 - Math.min(1, spread.tonalGap / 0.34)}
            known
            detail={`${(spread.tonalGap * 100).toFixed(0)}% apart`}
          />
          <Term
            label="warmth"
            value={1 - Math.min(1, spread.warmthGapK / 800)}
            known
            detail={`${spread.warmthGapK.toFixed(0)} K apart`}
          />
          <Term
            label="facing inward"
            value={spread.facingScore}
            known={spread.facingKnown}
            detail={spread.facingScore.toFixed(2)}
          />
          <Term
            label="how alike"
            value={1 - spread.similarity}
            known
            detail={spread.similarity.toFixed(2)}
          />
        </dl>
      )}

      <ul className="spread-reasons">
        {spread.reasons.map((reason) => (
          <li key={reason.code} className={reason.caveat ? 'caveat' : 'argument'}>
            {reason.text}
          </li>
        ))}
      </ul>
    </section>
  );
}
