import { useMemo } from 'react';

import type {
  CleanupBlockedDto,
  CleanupProposalDto,
  CleanupStatusDto,
} from '../../ipc/types';

/**
 * PHASE-24. The review queue: what AURA would tidy out of a photograph, and everything it refused.
 *
 * Section 9's SFE deliverable, and the panel in this product with the most unusual job. Every
 * earlier review surface answers "what did AURA do to my photographs". This one has to answer
 * "what would AURA take *away*", and the honest answer on this build is "nothing, and here is
 * exactly why" - so the refusals are not a footnote here, they are most of the screen.
 *
 * Five rules:
 *
 * 1. **The mask-coverage figure is the headline, not the proposal count.** At zero, every
 *    candidate in this project was refused because AURA could not show the region was clear of
 *    people - not because it looked and found somebody. Those are different rows, different reason
 *    codes and different runbooks. A panel that led with "0 suggestions" would let a build with no
 *    segmenter look like a build that examined every photograph and found them all clear.
 *
 * 2. **A refusal is rendered as a decision, not as an absence.** Each blocked candidate shows the
 *    check that stopped it and the sentence for its code. More than half of `CleanupCode` is
 *    refusals, and teaching a photographer what AURA will never do is most of the trust this
 *    feature needs.
 *
 * 3. **The method is always visible, and a borrow says where from.** "Real pixels from another
 *    frame of this moment" and "texture copied from this photograph" are different promises, and
 *    a client will eventually ask which. `borrowedFrom` is on the row rather than in a tooltip.
 *
 * 4. **Accepting is one click and applying is another.** `onDecide` marks a proposal accepted;
 *    nothing here writes a recipe. A panel that did both would leave a disclosure saying a removal
 *    happened on a photograph that still has the bin in it.
 *
 * 5. **Nothing on this surface has a strength.** Yes, no, or "leave this one alone". There is no
 *    prop that could carry an amount and none that could carry a description of what should be
 *    there instead - `docs/generative-policy.md`'s promise, as a component signature.
 *
 * The component is pure: it receives rows and callbacks, fetches nothing and renders no pixels.
 * The before-and-after is `BeforeAfter`, which asks the develop view for the same region twice.
 */
export type ProposalQueueProps = {
  /** What the project pass covered and refused. */
  status: CleanupStatusDto | null;
  /** The selected photograph's proposals, strongest first. */
  proposals: CleanupProposalDto[];
  /** What the safety engine refused on the same photograph. */
  blocked: CleanupBlockedDto[];
  /** Accept or reject one proposal. */
  onDecide?: (proposalId: string, accept: boolean) => void;
  /** Leave this photograph alone entirely. */
  onDisable?: (disabled: boolean) => void;
  /** Show the before-and-after for one proposal. */
  onPreview?: (proposalId: string) => void;
};

/** How a method reads to somebody who has not read the phase document. */
const METHOD_LABEL: Record<string, string> = {
  borrow: 'real pixels from another frame of this moment',
  fill: 'texture copied from elsewhere in this photograph',
  inpaint: 'pixels made up by a model',
};

/** How each safety check reads. */
const CHECK_LABEL: Record<string, string> = {
  size_cap: 'too large to tidy automatically',
  denylist: 'people, dress, rings or cake',
  identity_protect: 'somebody this wedding is about',
  structure_span: 'a straight line or a repeating pattern',
  confidence: 'not confident enough, or nothing could replace it',
};

function percent(value: number): string {
  return `${Math.round(value * 100)}%`;
}

export function ProposalQueue({
  status,
  proposals,
  blocked,
  onDecide,
  onDisable,
  onPreview,
}: ProposalQueueProps) {
  /**
   * Rule 1. The sentence the header leads with, chosen from the coverage rather than from the
   * proposal count.
   */
  const headline = useMemo(() => {
    if (!status) {
      return 'This wedding has not been looked at for distractions yet.';
    }
    if (status.examined === 0) {
      return 'This wedding has not been looked at for distractions yet.';
    }
    if (status.maskCovered <= 0) {
      return (
        'AURA cannot yet tell where people, dresses and rings are in these photographs, so it ' +
        'will not tidy anything out of them. Nothing has been changed and every photograph is ' +
        'still usable.'
      );
    }
    if (!status.detectorTrained) {
      return (
        'AURA can see that something is drawing the eye in some of these photographs, but it ' +
        'cannot yet tell what those things are - so it will not suggest removing any of them.'
      );
    }
    if (status.withProposals === 0) {
      return 'AURA looked at every photograph and found nothing worth tidying out.';
    }
    return `${status.withProposals} photographs have something AURA could tidy out.`;
  }, [status]);

  return (
    <section className="cleanup-queue" aria-label="Distraction cleanup">
      <header className="cleanup-queue__header">
        <h2>Tidying up</h2>
        <p className="cleanup-queue__headline">{headline}</p>
        {status ? (
          <dl className="cleanup-queue__stats">
            <div>
              <dt>Looked at</dt>
              <dd>
                {status.examined} of {status.photos} ({percent(status.coverage)})
              </dd>
            </div>
            <div>
              {/* Rule 1. The number to read first, and it is labelled as such. */}
              <dt>Could check for people</dt>
              <dd data-low={status.maskCovered <= 0 ? 'true' : 'false'}>
                {percent(status.maskCovered)} of those
              </dd>
            </div>
            <div>
              <dt>Tidied</dt>
              <dd>
                {status.applied} ({status.borrowed} borrowed, {status.filled} filled
                {status.inpainted > 0 ? `, ${status.inpainted} generated` : ''})
              </dd>
            </div>
            <div>
              <dt>Undone by AURA itself</dt>
              <dd>{status.reverted}</dd>
            </div>
          </dl>
        ) : null}
        {onDisable ? (
          <button type="button" onClick={() => onDisable(true)}>
            Leave this photograph alone
          </button>
        ) : null}
      </header>

      {proposals.length > 0 ? (
        <ol className="cleanup-queue__proposals">
          {proposals.map((proposal) => (
            <li key={proposal.proposalId} className="cleanup-proposal">
              <div className="cleanup-proposal__what">
                <strong>{proposal.classText}</strong>
                <span className="cleanup-proposal__area">
                  {percent(proposal.areaFrac)} of the frame
                </span>
              </div>

              {/* Rule 3. */}
              <p className="cleanup-proposal__method">
                {METHOD_LABEL[proposal.method] ?? proposal.method}
                {proposal.borrowedFrom ? (
                  <span className="cleanup-proposal__source">
                    {' '}
                    (from {proposal.borrowedFrom})
                  </span>
                ) : null}
              </p>

              <ul className="cleanup-proposal__reasons">
                {proposal.reasons.map((reason) => (
                  <li
                    key={reason.code}
                    data-refusal={reason.isRefusal ? 'true' : 'false'}
                  >
                    {reason.text}
                  </li>
                ))}
              </ul>

              <p className="cleanup-proposal__band">
                {proposal.mayApplyUnattended
                  ? 'AURA is confident enough to do this without asking'
                  : 'Waiting for you'}
              </p>

              <div className="cleanup-proposal__actions">
                {onPreview ? (
                  <button type="button" onClick={() => onPreview(proposal.proposalId)}>
                    Show me
                  </button>
                ) : null}
                {/* Rule 4 and rule 5: two answers, no slider. */}
                {onDecide ? (
                  <>
                    <button
                      type="button"
                      onClick={() => onDecide(proposal.proposalId, true)}
                    >
                      Tidy it
                    </button>
                    <button
                      type="button"
                      onClick={() => onDecide(proposal.proposalId, false)}
                    >
                      Leave it
                    </button>
                  </>
                ) : null}
              </div>
            </li>
          ))}
        </ol>
      ) : (
        <p className="cleanup-queue__empty">
          Nothing is being suggested for this photograph.
        </p>
      )}

      {/* Rule 2. Not a footnote. */}
      {blocked.length > 0 ? (
        <details className="cleanup-queue__blocked" open={proposals.length === 0}>
          <summary>
            {blocked.length} thing{blocked.length === 1 ? '' : 's'} AURA decided not to touch
          </summary>
          <ul>
            {blocked.map((row, index) => (
              <li key={`${row.code}-${index}`}>
                <span className="cleanup-blocked__check">
                  {CHECK_LABEL[row.check] ?? row.check}
                </span>
                <span className="cleanup-blocked__text">{row.text}</span>
              </li>
            ))}
          </ul>
        </details>
      ) : null}
    </section>
  );
}

export default ProposalQueue;
