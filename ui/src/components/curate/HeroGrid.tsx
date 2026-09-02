import type { CurateHeroDto } from '../../ipc/types';

/**
 * PHASE-29. The portfolio, in rank order.
 *
 * Pure - rows and callbacks in, nothing fetched.
 *
 * ## The binding constraint is why a photographer stops arguing
 *
 * Two frames from the same kiss can differ by 0.004, and a grid that showed only the score would
 * leave somebody comparing two numbers that are the same number. What actually decided between them
 * is almost always a constraint - the chapter had four already, the moment was represented, the set
 * was becoming all close-ups - and `bindingText` is that sentence.
 *
 * ## Rejecting is one click
 *
 * A curation panel where accepting is one click and rejecting is a modal is a panel that measures
 * agreement it did not earn. Both buttons are the same size and sit in the same place.
 *
 * ## A caveat is grey, not red
 *
 * `reason.caveat` says AURA could not check something rather than found something wrong, and the two
 * must not look the same. On this build the uniqueness term is often unmeasurable, so this is the
 * difference between "we checked and it is unlike the rest" and "we could not check".
 */
export type HeroGridProps = {
  /** The portfolio, best first. */
  heroes: CurateHeroDto[];
  /** Record accept or reject on one pick. */
  onDecide: (imageId: string, accepted: boolean) => void;
};

export function HeroGrid({ heroes, onDecide }: HeroGridProps) {
  if (heroes.length === 0) {
    return (
      <section className="curate-heroes" aria-label="Portfolio">
        <p className="empty">
          No portfolio picks yet. Curate this wedding to see which frames AURA would put on a
          website.
        </p>
      </section>
    );
  }

  return (
    <section className="curate-heroes" aria-label="Portfolio">
      <ol className="hero-grid">
        {heroes.map((hero) => (
          <li key={hero.imageId} className="hero-card" data-accepted={hero.accepted ?? 'undecided'}>
            <header>
              <span className="hero-rank">{hero.rank + 1}</span>
              <span className="hero-chapter">{hero.chapter.replace(/_/g, ' ')}</span>
              <span className="hero-score" title="Blended score">
                {hero.score.toFixed(2)}
              </span>
            </header>

            <p className="hero-binding">{hero.bindingText}</p>

            <dl className="hero-terms">
              {hero.terms.map(([name, value]) => (
                <div key={name}>
                  <dt>{name}</dt>
                  <dd>{value.toFixed(2)}</dd>
                </div>
              ))}
            </dl>

            <ul className="hero-reasons">
              {hero.reasons.map((reason) => (
                <li key={reason.code} className={reason.caveat ? 'caveat' : 'argument'}>
                  {reason.text}
                </li>
              ))}
            </ul>

            <footer>
              <span className="hero-confidence">
                {hero.scale === 'unknown'
                  ? 'how close the photographer was: not measured'
                  : `${hero.scale} shot`}
                {' · '}
                confidence {hero.confidence.toFixed(2)}
              </span>
              <span className="hero-actions">
                <button type="button" onClick={() => onDecide(hero.imageId, true)}>
                  Keep
                </button>
                <button type="button" onClick={() => onDecide(hero.imageId, false)}>
                  Not this one
                </button>
              </span>
            </footer>
          </li>
        ))}
      </ol>
    </section>
  );
}
