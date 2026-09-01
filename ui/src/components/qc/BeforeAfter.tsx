import type { QcReplacementDto, QcRoundDto } from '../../ipc/types';

/**
 * PHASE-27. What was tried on one finding, and what it did.
 *
 * Pure - rows in, nothing fetched. Two shapes share this component because they answer the same
 * question from opposite directions: a **round** is what AURA changed about a photograph, and a
 * **replacement** is AURA choosing a different photograph.
 *
 * ## A round shows what was promised beside what was delivered
 *
 * `expectedGain` is what the check predicted a remedy would close; `realisedShare` is the fraction
 * of that prediction the re-inspection actually measured. A round below half was put back, and it
 * is shown with the number that decided it rather than as a bare "reverted" - because the useful
 * fact for a photographer is not that AURA gave up, it is that the frame did not respond to what
 * AURA thought was wrong with it.
 *
 * **Collateral is a separate column and it names the check that took it.** A remedy that closed
 * its own finding and opened a worse one somewhere else is the failure this loop exists to catch,
 * and folding it into a single verdict would hide which inspection noticed.
 *
 * ## A replacement shows both frames' numbers, never the difference
 *
 * A photographer looking at a swap wants to know what each frame measured. A stored subtraction
 * cannot be read back as two numbers, and "0.3 better" says nothing about whether the frame that
 * went in is actually good.
 *
 * There is no image in this component and there cannot be. Phase 13's rule - evidence is a crop
 * rectangle, a list of frame ids or a list of parameter deltas, never a pixel - and the frames are
 * named so the grid can show them.
 */
export type BeforeAfterProps = {
  /** The rounds run on the open finding, in order. */
  rounds: QcRoundDto[];
  /** The swap that finding produced, when it produced one. */
  replacement: QcReplacementDto | null;
};

/** How each round outcome reads. */
const OUTCOME_LABEL: Record<string, string> = {
  fix_applied: 'kept',
  fix_reverted: 'put back',
  fix_insufficient: 'put back - it barely moved',
  collateral_damage: 'put back - it made something else worse',
  loop_exhausted: 'no more attempts',
  escalated: 'handed to you',
};

function percent(value: number): string {
  return `${Math.round(value * 100)}%`;
}

export function BeforeAfter({ rounds, replacement }: BeforeAfterProps) {
  if (rounds.length === 0 && !replacement) {
    return (
      <section className="qc-before-after" aria-label="What was tried">
        <p className="qc-before-after__empty">
          Nothing has been tried on this finding yet.
        </p>
      </section>
    );
  }

  return (
    <section className="qc-before-after" aria-label="What was tried">
      {rounds.length > 0 ? (
        <table className="qc-before-after__rounds">
          <caption>What AURA tried</caption>
          <thead>
            <tr>
              <th scope="col">Attempt</th>
              <th scope="col">What</th>
              <th scope="col">Before</th>
              <th scope="col">After</th>
              <th scope="col">Of what it promised</th>
              <th scope="col">Side effects</th>
              <th scope="col">Outcome</th>
            </tr>
          </thead>
          <tbody>
            {rounds.map((round) => (
              <tr
                key={round.round}
                className={round.kept ? 'qc-before-after__row' : 'qc-before-after__row is-reverted'}
              >
                <th scope="row">{round.round}</th>
                <td>
                  {round.remedyKind} — {round.remedyTarget}
                </td>
                <td>{round.deviationBefore.toFixed(2)}</td>
                <td>{round.deviationAfter.toFixed(2)}</td>
                <td>{percent(round.realisedShare)}</td>
                <td>
                  {round.collateralCategory
                    ? `${round.collateral.toFixed(2)} in ${round.collateralCategory}`
                    : 'none measured'}
                </td>
                <td>{OUTCOME_LABEL[round.outcome] ?? round.outcome}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : null}

      {replacement ? (
        <section className="qc-before-after__swap" aria-label="Frame swapped">
          <h4>AURA used a different photograph</h4>
          <dl>
            <div>
              <dt>Was</dt>
              <dd>
                {replacement.replaced} — {replacement.metricBefore.toFixed(2)}
              </dd>
            </div>
            <div>
              <dt>Now</dt>
              <dd>
                {replacement.replacement} — {replacement.metricAfter.toFixed(2)}
              </dd>
            </div>
            <div>
              <dt>How sure</dt>
              <dd>{percent(replacement.confidence)}</dd>
            </div>
            <div>
              <dt>Coverage</dt>
              <dd>{replacement.coverageHeld ? 'still guaranteed' : 'not re-validated'}</dd>
            </div>
          </dl>
          <p>{replacement.note}</p>
        </section>
      ) : null}
    </section>
  );
}
