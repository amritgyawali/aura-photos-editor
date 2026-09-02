import type { CurateSocialDto } from '../../ipc/types';

/**
 * PHASE-29. The grid set, the story set, the hero and their captions.
 *
 * Pure - rows and callbacks in, nothing fetched.
 *
 * ## An unfilled slot is shown, never filled
 *
 * A wedding with no exit photographs gets a nine-image grid and a sentence, not a tenth frame
 * promoted out of another slot to make the number right. Phase 12's rule - the product cannot invent
 * coverage - in the smallest place it applies.
 *
 * ## Captions say where they came from
 *
 * Every word in a caption came from this wedding's own chapter, scene and ritual labels, and a
 * drafted one that did not was replaced by the template before it reached the catalog. The panel
 * tells a photographer to edit it, because "the ceremony, and the vows" is a sentence AURA can prove
 * it is entitled to write rather than one anybody would post.
 */
export type SocialSetsProps = {
  /** The three sets and their captions. */
  sets: CurateSocialDto | null;
  /** Record accept or reject on one pick. */
  onDecide: (imageId: string, kind: string, accepted: boolean) => void;
};

export function SocialSets({ sets, onDecide }: SocialSetsProps) {
  if (!sets || (sets.grid.length === 0 && sets.story.length === 0 && !sets.hero)) {
    return (
      <section className="curate-social" aria-label="Social sets">
        <p className="empty">No social sets yet. Curate this wedding to build them.</p>
      </section>
    );
  }

  const captionFor = (imageId: string) =>
    sets.captions.find((caption) => caption.imageId === imageId)?.text ?? '';

  const renderSet = (title: string, picks: typeof sets.grid, kind: string) => (
    <div className="social-set">
      <h4>{title}</h4>
      <ol>
        {picks.map((pick) => (
          <li key={pick.imageId} data-accepted={pick.accepted ?? 'undecided'}>
            <span className="social-slot">{pick.slot}</span>
            <span className="social-aspect">{pick.aspect}</span>
            <span className="social-legibility" title="How well it reads at thumbnail size">
              {pick.legibility > 0 ? pick.legibility.toFixed(2) : 'not measured'}
            </span>
            <span className="social-caption">{captionFor(pick.imageId)}</span>
            <span className="social-actions">
              <button type="button" onClick={() => onDecide(pick.imageId, kind, true)}>
                Post
              </button>
              <button type="button" onClick={() => onDecide(pick.imageId, kind, false)}>
                Skip
              </button>
            </span>
          </li>
        ))}
      </ol>
    </div>
  );

  return (
    <section className="curate-social" aria-label="Social sets">
      {sets.unfilled.length > 0 ? (
        <ul className="social-unfilled">
          {sets.unfilled.map(([slot, short]) => (
            <li key={slot}>
              Nothing in this wedding for {short} of the {slot} slots.
            </li>
          ))}
        </ul>
      ) : null}

      {sets.hero ? renderSet('Hero', [sets.hero], 'social_hero') : null}
      {renderSet('Grid', sets.grid, 'social_grid')}
      {renderSet('Story', sets.story, 'social_story')}

      <p className="social-caption-note">
        Captions are built from this wedding&rsquo;s own chapter and ritual labels, so AURA never
        invents a name, a place or a claim. Edit them before you post.
      </p>
    </section>
  );
}
