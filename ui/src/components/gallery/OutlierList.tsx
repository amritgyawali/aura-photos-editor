import type { GalleryOutlierDto } from '../../ipc/types';

/**
 * PHASE-25. The frames that would not come, worst first.
 *
 * Section 6.4 makes outliers a first-class output and section 2.1 says what they are for: "this is
 * exactly the Phase 27 QC input". This panel is what a photographer sees before phase 27 exists.
 *
 * Three rules:
 *
 * 1. **The sentence comes from the wire, not from here.** `description` is assembled by
 *    `Outlier::describe` from the residuals - "+310 K warmer than the anchors, skin cast 4.2 dE00" -
 *    so this panel and phase 27's QC ticket say the same thing about the same frame. A second
 *    version assembled in TypeScript would drift from the Rust one within two releases.
 *
 * 2. **These are residuals, not deviations.** A frame here is one the correction *could not reach*,
 *    not one that started a long way off. A frame that began 400 K out and was fully corrected is
 *    not in this list, and a frame that began 900 K out and moved the 450 K it was allowed to is.
 *    Listing the raw deviation would fill this queue with frames the product already fixed, which
 *    is the fastest way to make a photographer stop opening it.
 *
 * 3. **Three ways to be here, and they need different actions.** A frame the bounds could not reach
 *    is a frame to re-edit. A skin cast is a frame to look at. A node whose references disagree is
 *    not a claim about the frame at all - it is fixed by pinning an anchor, and the row says so.
 *
 * Pure: rows and callbacks in, no fetching, no pixels.
 */
export type OutlierListProps = {
  /** The queue, worst first. */
  outliers: GalleryOutlierDto[];
  /** Which frame is selected. */
  selectedPhotoId?: string | null;
  /** Open a frame. */
  onSelect?: (photoId: string) => void;
  /** Jump to the node this frame should have matched. */
  onOpenNode?: (nodeId: string) => void;
};

/** What a photographer should do about this row. */
function advice(outlier: GalleryOutlierDto): string {
  if (outlier.reasons.some((reason) => reason.code === 'anchors_disagree')) {
    return 'The references for this part of the wedding disagree. Pin one you trust.';
  }
  if (outlier.reasons.some((reason) => reason.code === 'skin_outlier')) {
    return 'This person looks different here from how they look elsewhere.';
  }
  return 'AURA moved this frame as far as it is allowed to and it is still out of line.';
}

export function OutlierList({
  outliers,
  selectedPhotoId,
  onSelect,
  onOpenNode,
}: OutlierListProps): JSX.Element {
  if (outliers.length === 0) {
    return (
      <section className="outlier-list outlier-list--empty">
        <p>
          Every photograph AURA could match came into line with the part of the wedding it belongs
          to.
        </p>
      </section>
    );
  }

  return (
    <section className="outlier-list" aria-label="Photographs that are still out of line">
      <header className="outlier-list__header">
        <h4>Still out of line</h4>
        <span>
          {outliers.length} photograph{outliers.length === 1 ? '' : 's'}
        </span>
      </header>
      <ol>
        {outliers.map((outlier) => (
          <li
            key={outlier.photoId}
            className={
              outlier.photoId === selectedPhotoId ? 'outlier-row outlier-row--selected' : 'outlier-row'
            }
          >
            <button
              type="button"
              className="outlier-row__open"
              onClick={onSelect ? () => onSelect(outlier.photoId) : undefined}
            >
              {outlier.photoId.slice(0, 12)}
            </button>
            <span className="outlier-row__description">{outlier.description}</span>
            <span className="outlier-row__advice">{advice(outlier)}</span>
            <span
              className="outlier-row__deviation"
              title="How far out of line, as a share of the furthest AURA is allowed to move a frame"
            >
              {Math.round(outlier.deviation * 100)} %
            </span>
            {onOpenNode ? (
              <button
                type="button"
                className="outlier-row__node"
                onClick={() => onOpenNode(outlier.nodeId)}
              >
                Show its part of the wedding
              </button>
            ) : null}
          </li>
        ))}
      </ol>
    </section>
  );
}
