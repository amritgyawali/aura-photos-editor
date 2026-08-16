import type { DecisionDto } from '../../ipc/types';

/**
 * PHASE-12's rejection drawer, and the keeper's explanation beside it.
 *
 * Section 13's third acceptance criterion: "every decision is explained, and
 * every keeper offers a runner-up to compare." Section 12's fourth failure mode
 * is users distrusting automation, and the mitigation named there is exactly
 * this drawer plus the one-click override underneath it.
 *
 * ## What this component deliberately does not show
 *
 * The four sub-scores. They are in the catalog and phase 13's ledger will show
 * them; leading a rejection with "technical 0.31" starts an argument about a
 * number instead of an argument about a photograph. Every reason the engine
 * produces is already a sentence about something visible - the subject is not in
 * focus, another frame of the same moment is stronger, the gallery already
 * carries several frames very like this one - and the frame that won is one
 * click away.
 *
 * ## "Not checked" is not "not good enough"
 *
 * A photograph with `not_analysed` among its reasons was never considered. The
 * drawer says so in those words and offers no override, because overriding a
 * decision nobody made is not a thing a photographer should be invited to do.
 */

/** The reason code that means nobody looked. */
export const NOT_ANALYSED = 'not_analysed';

/** The headline for a decision, in either direction. */
export function headline(decision: DecisionDto): string {
  if (decision.kept) {
    const lead = decision.selected?.reasons[0];
    return lead
      ? `In the gallery: ${lead.text}.`
      : 'In the gallery.';
  }
  if (decision.rejected?.reasons.some((reason) => reason.code === NOT_ANALYSED)) {
    return 'Not considered: AURA has not checked this photograph yet. That is not a judgement about it.';
  }
  const lead = decision.rejected?.reasons[0];
  return lead ? `Not in the gallery: ${lead.text}.` : 'Not in the gallery.';
}

/** True when the photographer may usefully overrule this decision. */
export function canOverride(decision: DecisionDto): boolean {
  return !decision.rejected?.reasons.some((reason) => reason.code === NOT_ANALYSED);
}

export type RejectReasonsProps = {
  decision: DecisionDto | null;
  /** Ask to keep, remove, or withdraw an earlier choice. */
  onOverride?: (action: 'keep' | 'reject' | 'clear') => void;
  /** Show the frame that won, or the alternative to this one. */
  onCompare?: (photoId: string) => void;
};

export function RejectReasons({ decision, onOverride, onCompare }: RejectReasonsProps) {
  if (!decision) {
    return (
      <section className="reject-reasons reject-reasons--empty" aria-label="Why">
        <p>
          AURA has not chosen a gallery for this wedding yet, so there is no decision to
          explain.
        </p>
      </section>
    );
  }

  const reasons = decision.kept
    ? decision.selected?.reasons ?? []
    : decision.rejected?.reasons ?? [];
  const compareWith = decision.kept
    ? decision.selected?.runnerUp ?? null
    : decision.rejected?.keptInstead ?? null;
  const overridable = canOverride(decision);

  return (
    <section
      className={decision.kept ? 'reject-reasons reject-reasons--kept' : 'reject-reasons'}
      aria-label="Why"
    >
      <p className="reject-reasons__headline">{headline(decision)}</p>

      <ul className="reject-reasons__list">
        {reasons.map((reason) => (
          <li key={reason.code} data-code={reason.code} data-veto={reason.veto}>
            {reason.text}
          </li>
        ))}
      </ul>

      {decision.rejected?.wasPeak ? (
        <p className="reject-reasons__peak" role="note">
          This was the strongest instant of its moment. AURA delivered a different frame of
          the same instant rather than dropping it quietly.
        </p>
      ) : null}

      {decision.selected?.protected ? (
        <p className="reject-reasons__protected" role="note">
          This photograph is held in the gallery by a coverage guarantee. Making the gallery
          smaller will not remove it.
        </p>
      ) : null}

      {compareWith ? (
        <button
          type="button"
          className="reject-reasons__compare"
          onClick={() => onCompare?.(compareWith)}
        >
          {decision.kept ? 'Compare with the alternative' : 'See the one that was kept'}
        </button>
      ) : null}

      {overridable ? (
        <div className="reject-reasons__actions">
          {decision.kept ? (
            <button type="button" onClick={() => onOverride?.('reject')}>
              Leave this one out
            </button>
          ) : (
            <button type="button" onClick={() => onOverride?.('keep')}>
              Keep this one
            </button>
          )}
          <button type="button" onClick={() => onOverride?.('clear')}>
            Let AURA decide
          </button>
        </div>
      ) : (
        <p className="reject-reasons__blocked" role="note">
          Run the analysis to include this photograph in the choice.
        </p>
      )}
    </section>
  );
}
