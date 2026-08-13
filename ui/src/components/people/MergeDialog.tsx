import { useState } from 'react';

import type { IdentityCardDto } from '../../ipc/types';

/**
 * Which identity survives a merge.
 *
 * The one with more faces, and the tie broken by id so the answer is stable. It matters
 * because the survivor keeps its name, its role and its `userLocked` flag - so merging
 * the named identity *into* the unnamed one would silently discard the photographer's
 * work, which is the mistake this function exists to prevent.
 *
 * A named identity always wins, regardless of face count. Somebody typed that name.
 */
export function chooseSurvivor(
  left: IdentityCardDto,
  right: IdentityCardDto,
): { survivor: IdentityCardDto; absorbed: IdentityCardDto } {
  const leftNamed = left.label !== null || left.userLocked;
  const rightNamed = right.label !== null || right.userLocked;

  if (leftNamed !== rightNamed) {
    return leftNamed
      ? { survivor: left, absorbed: right }
      : { survivor: right, absorbed: left };
  }
  if (left.faces !== right.faces) {
    return left.faces > right.faces
      ? { survivor: left, absorbed: right }
      : { survivor: right, absorbed: left };
  }
  return left.id <= right.id
    ? { survivor: left, absorbed: right }
    : { survivor: right, absorbed: left };
}

/**
 * The sentence the dialog shows before the photographer commits.
 *
 * It names what is kept and what goes, because "merge" is the one people-panel action
 * that destroys a row, and a photographer who has just named two identities needs to
 * know which name survives before they press it - not after.
 */
export function mergeSummary(left: IdentityCardDto, right: IdentityCardDto): string {
  const { survivor, absorbed } = chooseSurvivor(left, right);
  const keeping = survivor.label ?? 'the unnamed person';
  const moving = absorbed.label ?? 'the unnamed person';
  return `${absorbed.faces} face(s) will move from ${moving} to ${keeping}. ${keeping} keeps its name, role and importance; ${moving} is removed. You can undo this.`;
}

/**
 * True when the two identities look like different people and a merge is probably
 * wrong.
 *
 * A warning rather than a refusal. Two identities that were never photographed together
 * *and* both have a confident role are the shape of two real, distinct people - but a
 * photographer looking at the two cards knows something the co-occurrence graph does
 * not, and the product does not overrule them.
 */
export function suspiciousMerge(left: IdentityCardDto, right: IdentityCardDto): string | null {
  const together = left.companions.find((companion) => companion.id === right.id);
  if (together !== undefined) {
    return null;
  }
  if (left.roleConfidence > 0.6 && right.roleConfidence > 0.6) {
    return 'These two were never photographed together and both have a confident role. That is what two different people look like - check the faces before merging.';
  }
  return null;
}

export type MergeDialogProps = {
  /** The identity the photographer started from. */
  source: IdentityCardDto;
  /** Everybody else in the project. */
  candidates: IdentityCardDto[];
  onCancel: () => void;
  /** Called with the survivor first, then the absorbed one. */
  onConfirm: (survivorId: string, absorbedId: string) => void;
};

/**
 * The merge dialog.
 *
 * Two identities in, one out, and the choice of which survives is made by
 * [`chooseSurvivor`] rather than by the order the photographer happened to click - see
 * that function for why.
 */
export function MergeDialog({
  source,
  candidates,
  onCancel,
  onConfirm,
}: MergeDialogProps): JSX.Element {
  const others = candidates.filter((candidate) => candidate.id !== source.id);
  const [targetId, setTargetId] = useState<string>(others[0]?.id ?? '');
  const target = others.find((candidate) => candidate.id === targetId) ?? null;
  const warning = target === null ? null : suspiciousMerge(source, target);

  return (
    <div className="merge-dialog" role="dialog" aria-label="Merge identities">
      <h3>Merge {source.label ?? 'this person'} with…</h3>

      {others.length === 0 ? (
        <p>There is nobody else in this project to merge with.</p>
      ) : (
        <>
          <label htmlFor="merge-target">Merge with</label>
          <select
            id="merge-target"
            value={targetId}
            onChange={(event) => setTargetId(event.target.value)}
          >
            {others.map((candidate) => (
              <option key={candidate.id} value={candidate.id}>
                {candidate.label ?? 'Unnamed'} - {candidate.faces} face(s)
              </option>
            ))}
          </select>

          {target !== null && <p className="summary">{mergeSummary(source, target)}</p>}
          {warning !== null && (
            <p className="warning" role="alert">
              {warning}
            </p>
          )}

          <button
            type="button"
            disabled={target === null}
            onClick={() => {
              if (target === null) {
                return;
              }
              const { survivor, absorbed } = chooseSurvivor(source, target);
              onConfirm(survivor.id, absorbed.id);
            }}
          >
            Merge
          </button>
        </>
      )}

      <button type="button" onClick={onCancel}>
        Cancel
      </button>
    </div>
  );
}
