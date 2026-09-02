import type { CurateBwDto } from '../../ipc/types';

/**
 * PHASE-29. The monochrome candidates, each with the mix solved for that frame.
 *
 * Pure - rows and callbacks in, nothing fetched.
 *
 * ## The eight bands are drawn, not named
 *
 * Section 13's second acceptance criterion is that B&W suggestions come with per-frame mixes rather
 * than a single preset, and a panel that showed a preset name would make that unfalsifiable from the
 * screen. The bar chart is eight numbers a photographer can see differ between two frames.
 *
 * ## Nothing here applies anything
 *
 * Accepting records a decision. The conversion itself is written into the recipe by the develop
 * panel, with a person behind it - because a product that could convert a wedding to monochrome on
 * its own is a product that decides a wedding is monochrome. ADR-0059 section 3.
 *
 * ## When the skin bands are empty
 *
 * It means nobody in the frame has a measured skin locus, which is **not** the same as no people
 * being in it. On this build it is every frame, and the caveat says so rather than the panel
 * implying the mix protected somebody.
 */
export type BwPicksProps = {
  /** The candidates, best first. */
  picks: CurateBwDto[];
  /** Record accept or reject on one pick. */
  onDecide: (imageId: string, accepted: boolean) => void;
};

/** The eight bands, in the recipe's own order. */
const BANDS = ['red', 'orange', 'yellow', 'green', 'aqua', 'blue', 'purple', 'magenta'];

export function BwPicks({ picks, onDecide }: BwPicksProps) {
  if (picks.length === 0) {
    return (
      <section className="curate-bw" aria-label="Black and white">
        <p className="empty">
          No monochrome suggestions. AURA offers a frame only when losing the colour would make it
          stronger, and only once it has measured the skin of everybody in it.
        </p>
      </section>
    );
  }

  return (
    <section className="curate-bw" aria-label="Black and white">
      <ol className="bw-list">
        {picks.map((pick) => (
          <li key={pick.imageId} className="bw-card" data-accepted={pick.accepted ?? 'undecided'}>
            <header>
              <span className="bw-score">{pick.score.toFixed(2)}</span>
              <span className="bw-confidence">confidence {pick.confidence.toFixed(2)}</span>
            </header>

            <ul className="bw-mix" aria-label="Band mix">
              {pick.mix.map((value, index) => (
                <li key={BANDS[index] ?? index} className="bw-band">
                  <span className="bw-band-name">{BANDS[index] ?? index}</span>
                  <span
                    className="bw-band-bar"
                    data-protected={pick.skinBands.includes(index) ? 'skin' : 'free'}
                    style={{ width: `${Math.abs(value)}%` }}
                    title={`${BANDS[index] ?? index}: ${value}`}
                  />
                  <span className="bw-band-value">{value}</span>
                </li>
              ))}
            </ul>

            <dl className="bw-terms">
              {pick.terms.map(([name, value]) => (
                <div key={name}>
                  <dt>{name}</dt>
                  <dd>{value.toFixed(2)}</dd>
                </div>
              ))}
            </dl>

            <ul className="bw-reasons">
              {pick.reasons.map((reason) => (
                <li key={reason.code} className={reason.caveat ? 'caveat' : 'argument'}>
                  {reason.text}
                </li>
              ))}
            </ul>

            <footer>
              <button type="button" onClick={() => onDecide(pick.imageId, true)}>
                Use in black and white
              </button>
              <button type="button" onClick={() => onDecide(pick.imageId, false)}>
                Keep the colour
              </button>
            </footer>
          </li>
        ))}
      </ol>
    </section>
  );
}
