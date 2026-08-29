import { useState } from 'react';

import type { CleanupBlockedDto, CropRectDto, ManualRemoveDto } from '../../ipc/types';

/**
 * PHASE-24. The manual removal tool: one region, drawn by a person, confirmed by a person.
 *
 * Section 2.2 puts "removing guests the client dislikes" out of scope as an automated feature and
 * offers it "only as a manual tool with explicit confirmation". This is that tool, and the two
 * things it does *not* relax are the point of it.
 *
 * **It still runs the whole safety engine.** A person choosing a rectangle is a reason to skip the
 * detector, not a reason to skip the filter. The size cap, the denylist, the identity check, the
 * structure check and the confidence check all run, in that order, on the region they drew - so a
 * rectangle over the bride's hands comes back as a refusal naming the check rather than as a
 * removal.
 *
 * **It cannot remove a person, and there is no confirmation that changes that.** A hand-drawn
 * region is `unclassified`, which cannot be shown to be extraneous to the wedding, and
 * `background_person` is refused by the safety engine, by a CHECK in migration 24 and by a trigger.
 * The confirmation this component collects is confirmation that a *photographer* wants this
 * region gone - it is not permission for AURA to decide that somebody is not part of a wedding.
 *
 * **The confirmation is a field on the wire, not an implication of the call.** `confirmed: false`
 * is refused by the command, so a future UI that dropped the dialog would get an error rather than
 * a removal.
 *
 * The component is pure. The caller owns the drawing surface - the develop view already has one
 * for masks and crops - and hands the rectangle in.
 */
export type ManualRemoveProps = {
  /** The rectangle the photographer has drawn, or `null` before they have drawn one. */
  region: CropRectDto | null;
  /** Send it. The caller supplies `confirmed: true`; this component collects the confirmation. */
  onRemove?: (region: CropRectDto) => Promise<ManualRemoveDto> | void;
  /** Clear the drawn region. */
  onClear?: () => void;
};

const CHECK_SENTENCE: Record<string, string> = {
  size_cap: 'That region is larger than AURA will ever tidy on its own.',
  denylist:
    'That region overlaps a face, skin, hands, a dress, rings or the cake, so AURA will not touch it.',
  identity_protect: 'That region touches somebody this wedding is about.',
  structure_span:
    'That region crosses a straight line or a repeating pattern, which tidying would bend.',
  confidence: 'AURA could not replace that region with anything it is confident about.',
};

export function ManualRemove({ region, onRemove, onClear }: ManualRemoveProps) {
  const [confirming, setConfirming] = useState(false);
  const [blocked, setBlocked] = useState<CleanupBlockedDto | null>(null);
  const [done, setDone] = useState(false);

  async function send() {
    if (!region || !onRemove) {
      return;
    }
    setBlocked(null);
    setDone(false);
    const result = await onRemove(region);
    setConfirming(false);
    if (result && typeof result === 'object') {
      setBlocked(result.blocked);
      setDone(result.proposal !== null);
    }
  }

  return (
    <section className="cleanup-manual" aria-label="Remove something yourself">
      <h3>Remove something yourself</h3>

      <p className="cleanup-manual__note">
        Draw a box around whatever you want gone. AURA still checks it against everything it
        protects - people, skin, hands, the dress, the rings and the cake - and will refuse if the
        box touches any of them.
      </p>

      {/* The rule that does not bend, said on the screen rather than only in the code. */}
      <p className="cleanup-manual__never">
        AURA will not remove a person. That is a decision about a human being, and it is not one
        this product makes.
      </p>

      {region ? (
        <p className="cleanup-manual__region">
          {Math.round(region.w * region.h * 1000) / 10}% of the frame selected.
        </p>
      ) : (
        <p className="cleanup-manual__region">Nothing selected yet.</p>
      )}

      {!confirming ? (
        <div className="cleanup-manual__actions">
          <button
            type="button"
            disabled={!region}
            onClick={() => setConfirming(true)}
          >
            Remove this
          </button>
          {onClear ? (
            <button type="button" onClick={onClear} disabled={!region}>
              Clear
            </button>
          ) : null}
        </div>
      ) : (
        // Section 2.2's explicit confirmation. A dialog rather than a second click on the same
        // button, so that agreeing is a separate act from choosing.
        <div className="cleanup-manual__confirm" role="alertdialog">
          <p>
            This will replace what is inside the box with pixels from elsewhere in this photograph
            or from another frame of the same moment. It will be listed in the delivery report.
          </p>
          <button type="button" onClick={() => void send()}>
            Yes, remove it
          </button>
          <button type="button" onClick={() => setConfirming(false)}>
            Cancel
          </button>
        </div>
      )}

      {blocked ? (
        <p className="cleanup-manual__blocked" role="status">
          {CHECK_SENTENCE[blocked.check] ?? blocked.text}
        </p>
      ) : null}

      {done ? (
        <p className="cleanup-manual__done" role="status">
          Done. It is in the review queue and in the delivery report.
        </p>
      ) : null}
    </section>
  );
}

export default ManualRemove;
