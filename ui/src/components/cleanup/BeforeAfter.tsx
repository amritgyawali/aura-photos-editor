import { useState } from 'react';

import type { CleanupProposalDto } from '../../ipc/types';

/**
 * PHASE-24. The before and after for one proposed removal.
 *
 * Section 9's SFE row asks for "proposal queue with before/after". The interesting decision here
 * is what this component is *not* given.
 *
 * **It takes two image sources and never fetches one.** Phase 13's rule - evidence can never be a
 * pixel - means no command on the cleanup surface returns image bytes. The develop view already
 * knows how to render a photograph at a region; this asks it for the same region twice, once with
 * the recipe's `cleanup[]` and once without, and shows the two. What the cleanup surface adds is
 * the *rectangle and the method* those two renders differ by, which is what the caption says.
 *
 * **It shows the region outline over both.** A removal is invisible when it works, which is the
 * whole point and is also why a before-and-after of a good one looks like two identical
 * photographs. Without the outline a photographer cannot tell whether they are looking at a subtle
 * repair or at a control that failed to load.
 *
 * **It says where the pixels came from, in every state.** A borrow and a fill are different
 * promises to a client, and this is the screen where a photographer decides whether they are
 * comfortable making one.
 */
export type BeforeAfterProps = {
  /** The proposal being previewed. */
  proposal: CleanupProposalDto;
  /** The photograph as it was shot, at whatever level the caller rendered. */
  beforeSrc: string | null;
  /** The same render with this removal applied. */
  afterSrc: string | null;
  /** Called when the photographer accepts or rejects from here. */
  onDecide?: (accept: boolean) => void;
};

type Showing = 'before' | 'after';

const METHOD_SENTENCE: Record<string, string> = {
  borrow: 'These pixels are real: they come from another frame of the same moment.',
  fill: 'These pixels are texture from elsewhere in this same photograph, moved.',
  inpaint: 'These pixels were made up by a model.',
};

export function BeforeAfter({
  proposal,
  beforeSrc,
  afterSrc,
  onDecide,
}: BeforeAfterProps) {
  const [showing, setShowing] = useState<Showing>('after');
  const src = showing === 'after' ? afterSrc : beforeSrc;

  // The region as percentages, so the outline scales with whatever the caller rendered.
  const outline = {
    left: `${proposal.region.x * 100}%`,
    top: `${proposal.region.y * 100}%`,
    width: `${proposal.region.w * 100}%`,
    height: `${proposal.region.h * 100}%`,
  };

  return (
    <figure className="cleanup-before-after" aria-label="Before and after">
      <div className="cleanup-before-after__frame">
        {src ? (
          <img src={src} alt={showing === 'after' ? 'After tidying' : 'As shot'} />
        ) : (
          <p className="cleanup-before-after__pending">Rendering…</p>
        )}
        {/* A removal that works is invisible. The outline is what makes the comparison legible. */}
        <span
          className="cleanup-before-after__region"
          style={outline}
          aria-hidden="true"
        />
      </div>

      <div className="cleanup-before-after__toggle" role="group" aria-label="Compare">
        <button
          type="button"
          aria-pressed={showing === 'before'}
          onClick={() => setShowing('before')}
        >
          As shot
        </button>
        <button
          type="button"
          aria-pressed={showing === 'after'}
          onClick={() => setShowing('after')}
        >
          Tidied
        </button>
      </div>

      <figcaption className="cleanup-before-after__caption">
        <p>
          <strong>{proposal.classText}</strong>, covering{' '}
          {Math.round(proposal.areaFrac * 100)}% of the frame.
        </p>
        <p className="cleanup-before-after__provenance">
          {METHOD_SENTENCE[proposal.method] ?? proposal.method}
          {proposal.borrowedFrom ? ` Source: ${proposal.borrowedFrom}.` : ''}
        </p>
        {/* The self-check's own number, so a photographer can see the product checked its work. */}
        <p className="cleanup-before-after__selfcheck">
          AURA checked its own result and scored it {proposal.artefactScore.toFixed(3)} - lower is
          cleaner. Anything it did not like was undone before you saw it.
        </p>
      </figcaption>

      {onDecide ? (
        <div className="cleanup-before-after__actions">
          <button type="button" onClick={() => onDecide(true)}>
            Tidy it
          </button>
          <button type="button" onClick={() => onDecide(false)}>
            Leave it
          </button>
        </div>
      ) : null}
    </figure>
  );
}

export default BeforeAfter;
