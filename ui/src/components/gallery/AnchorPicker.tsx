import type { GalleryDeltaDto, SceneNodeDto } from '../../ipc/types';

/**
 * PHASE-25. Which frames a part of the wedding is matched to, and the photographer's veto over it.
 *
 * Section 6.1: "users can pin or reject anchors in the UI; pinned anchors are authoritative, which
 * gives professionals direct control over the look of a scene."
 *
 * Four rules:
 *
 * 1. **An unanchored node is the loudest thing on this panel.** When `target` is null AURA could
 *    not find three frames it was confident enough about, so *nothing in that part of the wedding
 *    was matched to anything* - and the frames all look untouched, which is exactly what a
 *    perfectly consistent chapter also looks like. The two are opposite outcomes and the panel says
 *    which one this is.
 *
 * 2. **A pin is a veto, not a vote.** A pinned frame is used as an anchor whatever the four terms
 *    scored it, because a photographer looking at a photograph knows something the terms do not.
 *    The button says "Use this as a reference" rather than "Suggest".
 *
 * 3. **Rejection is as durable as pinning.** Automation never re-selects a frame somebody threw
 *    out, and the panel says so on the button rather than leaving a person to discover it.
 *
 * 4. **Cohesion is shown, and a disagreeing set is called out.** When the chosen frames disagree
 *    with each other more than the chapter's own frames do, the anchor selection has gone wrong
 *    rather than the frames having drifted - and the fix is to pin a better one, which is a
 *    different action from re-editing a photograph.
 *
 * The component is pure: rows and callbacks in, no fetching, no pixels.
 */
export type AnchorPickerProps = {
  /** The node being looked at. */
  node: SceneNodeDto;
  /** Its frames' deltas, in capture order - the candidate list. */
  deltas: GalleryDeltaDto[];
  /** Pin a frame as an anchor, or reject it. Re-solves this node and no other. */
  onPin: (photoId: string, pinned: boolean) => void;
  /** Select a frame to look at. */
  onSelect?: (photoId: string) => void;
  /** Which frame is selected. */
  selectedPhotoId?: string | null;
  /** True while a pass is running, to disable the buttons. */
  busy?: boolean;
};

/** True when this node's anchors disagree more than its frames do. */
function anchorsDisagree(node: SceneNodeDto): boolean {
  return node.reasons.some((reason) => reason.code === 'anchors_disagree');
}

export function AnchorPicker({
  node,
  deltas,
  onPin,
  onSelect,
  selectedPhotoId,
  busy = false,
}: AnchorPickerProps): JSX.Element {
  const anchors = new Set(node.anchors);
  const disagree = anchorsDisagree(node);

  return (
    <section className="anchor-picker" aria-label={`Reference frames for ${node.label}`}>
      <header className="anchor-picker__header">
        <h4>{node.label}</h4>
        <span className="anchor-picker__count">
          {node.imageCount} photograph{node.imageCount === 1 ? '' : 's'}
        </span>
      </header>

      {node.target === null ? (
        <p className="anchor-picker__unanchored" role="status">
          AURA could not find three frames it was confident enough about to anchor this part of the
          wedding, so <strong>nothing here was matched to anything</strong>. Pin a frame you trust
          and it will be used as the reference.
        </p>
      ) : (
        <dl className="anchor-picker__target">
          <div>
            <dt>Warmth</dt>
            <dd>
              {Math.round(node.target.cctK)} K ±{Math.round(node.target.cctTol)}
            </dd>
          </div>
          <div>
            <dt>Tint</dt>
            <dd>
              {node.target.tint.toFixed(1)} ±{node.target.tintTol.toFixed(1)}
            </dd>
          </div>
          <div>
            <dt>Subject brightness</dt>
            <dd>
              {(node.target.subjectLuma * 100).toFixed(0)} % ±
              {(node.target.lumaTol * 100).toFixed(0)}
            </dd>
          </div>
          <div>
            <dt>Agreement</dt>
            <dd className={disagree ? 'anchor-picker__cohesion--low' : undefined}>
              {Math.round(node.target.cohesion * 100)} % over {node.target.anchorCount} frames
            </dd>
          </div>
        </dl>
      )}

      {disagree ? (
        <p className="anchor-picker__warning" role="status">
          The frames AURA picked as references disagree with each other, so it has not matched
          anything to them. Pinning one you trust will fix this part of the wedding.
        </p>
      ) : null}

      <ol className="anchor-picker__list">
        {deltas.map((delta) => {
          const isAnchor = anchors.has(delta.photoId);
          return (
            <li
              key={delta.photoId}
              className={delta.photoId === selectedPhotoId ? 'anchor-row anchor-row--selected' : 'anchor-row'}
            >
              <button
                type="button"
                className="anchor-row__name"
                onClick={onSelect ? () => onSelect(delta.photoId) : undefined}
              >
                {delta.photoId.slice(0, 12)}
                {isAnchor ? <span className="anchor-row__badge">reference</span> : null}
                {delta.userEdited ? <span className="anchor-row__badge">yours</span> : null}
              </button>
              <span className="anchor-row__moved">
                {delta.dCct === 0 && delta.dExposure === 0
                  ? 'unchanged'
                  : `${delta.dCct >= 0 ? '+' : ''}${Math.round(delta.dCct)} K, ${
                      delta.dExposure >= 0 ? '+' : ''
                    }${delta.dExposure.toFixed(2)} EV`}
              </span>
              {delta.boundedBy ? (
                <span className="anchor-row__bounded" title={`Clamped on ${delta.boundedBy}`}>
                  clamped
                </span>
              ) : null}
              <span className="anchor-row__actions">
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => onPin(delta.photoId, true)}
                  title="Use this frame as a reference for this part of the wedding. AURA will never
                         overwrite this."
                >
                  Use as reference
                </button>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => onPin(delta.photoId, false)}
                  title="Never use this frame as a reference. AURA will not pick it again."
                >
                  Never use
                </button>
              </span>
            </li>
          );
        })}
      </ol>
    </section>
  );
}
