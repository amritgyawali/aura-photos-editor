import type { DuplicateSetDto } from '../../ipc/types';

/**
 * The duplicate review panel: two frames side by side, and one button.
 *
 * PHASE-08 section 9's MFE task, and acceptance criterion 3: "near-identical frames are
 * marked with a confidence and reviewable side by side".
 *
 * ## The one thing this panel must never imply
 *
 * That pressing "keep this one" deletes the other. It does not, it cannot, and section
 * 12's fourth failure mode is named "duplicate deletion anxiety" for a reason:
 * photographers have been burned by software that tidied their card. So the button's
 * label, the sentence beside it and the panel's own heading all say the same thing three
 * different ways, and [`keepConsequence`] is the sentence that says it in full.
 *
 * What the button actually does is move `keepHint` and set `userChosen`. Phase 12 still
 * decides what a client sees, and every other frame in the set stays exactly as eligible
 * as it was.
 */

/** The heading for one set, by class. */
export function setHeading(set: DuplicateSetDto): string {
  switch (set.kind) {
    case 'identical':
      return 'The same file, imported twice';
    case 'near_identical':
      return 'The same photograph, more than once';
    default:
      return 'Alternatives from the same moment';
  }
}

/**
 * What this set means for delivery, in one sentence.
 *
 * The distinction between the second class and the third is the entire product
 * consequence of this phase, so it is stated rather than implied by a badge colour.
 */
export function setConsequence(set: DuplicateSetDto): string {
  if (!set.capsGallery) {
    return 'These are different photographs of one moment. All of them stay eligible, and AURA will choose between them when you cull.';
  }
  return `Only one of these ${set.photoIds.length} frames should reach a gallery. Nothing has been deleted; you can still see and export every one of them.`;
}

/**
 * What pressing "keep this one" will and will not do.
 *
 * Section 12's fourth failure mode, answered in the interface rather than in a runbook.
 */
export function keepConsequence(): string {
  return 'This marks which frame AURA should start from. It does not delete anything, and it does not stop you choosing a different frame later.';
}

/**
 * How sure the classification is, as a photographer would read it.
 *
 * Words rather than a percentage, for `confidenceLabel`'s reason on the timeline: a
 * photographer deciding whether to trust a grouping wants to know whether to look, and
 * "0.83" does not answer that. The number is still shown beside it for anybody who
 * wants it.
 */
export function confidenceLabel(set: DuplicateSetDto): string {
  if (set.confidence >= 0.85) {
    return 'certain';
  }
  if (set.confidence >= 0.7) {
    return 'confident';
  }
  if (set.confidence >= 0.55) {
    return 'worth a look';
  }
  return 'uncertain';
}

/**
 * The frames to show beside the kept one.
 *
 * Everything except the hint, in order. A set of two draws one comparison; a set of ten
 * draws nine, and the panel scrolls - a photographer who bracketed ten frames wants to
 * see all nine alternatives, not the first two and a count.
 */
export function comparisons(set: DuplicateSetDto): string[] {
  return set.photoIds.filter((id) => id !== set.keepHint);
}

/**
 * Whether the hint was the photographer's choice or the automation's.
 *
 * Shown because it changes what the panel is asking. If the photographer already chose,
 * the panel is showing them their own decision; if not, it is asking for one.
 */
export function hintSource(set: DuplicateSetDto): string {
  return set.userChosen ? 'You chose this frame.' : 'AURA suggests this frame.';
}

/**
 * The sets worth putting in front of somebody, in the order to do it.
 *
 * Capping sets first, because those are the ones with a consequence; then least
 * confident first inside each group, because an uncertain suppression is the one most
 * likely to be wrong and the most expensive to get wrong - it hides a frame the
 * photographer never sees.
 */
export function reviewOrder(sets: DuplicateSetDto[]): DuplicateSetDto[] {
  return [...sets].sort((a, b) => {
    if (a.capsGallery !== b.capsGallery) {
      return a.capsGallery ? -1 : 1;
    }
    return a.confidence - b.confidence;
  });
}

/** Side-by-side review of one moment's duplicate sets. */
export function DuplicatePanel({
  sets,
  onKeep,
}: {
  sets: DuplicateSetDto[];
  onKeep: (photoId: string) => void;
}): JSX.Element {
  const ordered = reviewOrder(sets);

  if (ordered.length === 0) {
    return (
      <section className="duplicates" aria-label="Duplicates">
        <p>No frames in this moment repeat another. Every one of them is a different photograph.</p>
      </section>
    );
  }

  return (
    <section className="duplicates" aria-label="Duplicates">
      {ordered.map((set) => (
        <article key={set.keepHint} className="duplicate-set">
          <h3>{setHeading(set)}</h3>
          <p className="duplicate-consequence">{setConsequence(set)}</p>
          <p className="duplicate-confidence">
            {confidenceLabel(set)} ({set.confidence.toFixed(2)})
          </p>
          <ul className="duplicate-reasons">
            {set.reasons.map((reason) => (
              <li key={reason}>{reason}</li>
            ))}
          </ul>

          <div className="duplicate-compare">
            <figure className="duplicate-kept">
              <span>{set.keepHint}</span>
              <figcaption>{hintSource(set)}</figcaption>
            </figure>
            {comparisons(set).map((photoId) => (
              <figure key={photoId} className="duplicate-other">
                <span>{photoId}</span>
                <figcaption>
                  <button type="button" onClick={() => onKeep(photoId)}>
                    Keep this one instead
                  </button>
                </figcaption>
              </figure>
            ))}
          </div>

          <p className="duplicate-reassurance">{keepConsequence()}</p>
        </article>
      ))}
    </section>
  );
}
