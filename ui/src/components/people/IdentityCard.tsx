import { useEffect, useState } from 'react';

import { api, inTauri } from '../../ipc/client';
import type { IdentityCardDto } from '../../ipc/types';

/** The roles a photographer can set from a card, in the order the panel offers them. */
export const ASSIGNABLE_ROLES: ReadonlyArray<{ value: string; label: string }> = [
  { value: 'bride', label: 'Bride' },
  { value: 'groom', label: 'Groom' },
  { value: 'family_close', label: 'Close family' },
  { value: 'family_extended', label: 'Extended family' },
  { value: 'vip', label: 'VIP' },
  { value: 'child', label: 'Child' },
  { value: 'guest', label: 'Guest' },
  { value: 'vendor', label: 'Vendor' },
  { value: 'unknown', label: 'Not sure yet' },
];

/**
 * The role, as a photographer reads it.
 *
 * `couple` is deliberately worded as a question rather than as a fact. The evidence
 * identifies a pair; which of the two is the bride is not a photographic fact and this
 * product never guesses it - so the card invites a confirmation instead of stating one.
 */
export function roleLabel(identity: IdentityCardDto): string {
  switch (identity.role) {
    case 'bride':
      return 'Bride';
    case 'groom':
      return 'Groom';
    case 'couple':
      return 'One of the couple - confirm which';
    case 'family_close':
      return 'Close family';
    case 'family_extended':
      return 'Extended family';
    case 'vip':
      return 'VIP';
    case 'child':
      return 'Child';
    case 'guest':
      return 'Guest';
    case 'vendor':
      return 'Vendor';
    default:
      return 'Not identified yet';
  }
}

/**
 * How the role was decided, in one line.
 *
 * A locked role says who decided rather than how sure the product is, because once a
 * photographer has said "this is the bride" the confidence is not the interesting
 * number - the fact that automation cannot change it is.
 */
export function roleProvenance(identity: IdentityCardDto): string {
  if (identity.userLocked) {
    return 'set by you; re-analysis will not change it';
  }
  const percent = Math.round(identity.roleConfidence * 100);
  return `suggested by AURA, ${percent}% confident`;
}

/** Faces counted, with the gated ones accounted for rather than hidden. */
export function faceCountLabel(identity: IdentityCardDto): string {
  const gated = identity.faces - identity.votingFaces;
  const frames = `${identity.frames} photograph${identity.frames === 1 ? '' : 's'}`;
  if (gated <= 0) {
    return frames;
  }
  return `${frames} - ${gated} face${gated === 1 ? '' : 's'} too small or unclear to group`;
}

/**
 * Whether this identity looks like two looks of one person.
 *
 * `subCount` above zero means the grouping grew extra match targets for a change of
 * outfit or hairstyle. Worth surfacing: it is also the shape a wrongly merged pair of
 * relatives has, so a photographer who sees it should look at the card twice.
 */
export function varianceNote(identity: IdentityCardDto): string | null {
  if (identity.subCount === 0) {
    return null;
  }
  return `Two or more different looks were grouped here (spread ${identity.variance.toFixed(
    2,
  )}). If this is two people, split it.`;
}

/** The importance slider's label. 0.5 is neutral and says so. */
export function importanceLabel(importance: number): string {
  if (Math.abs(importance - 0.5) < 0.01) {
    return 'Normal';
  }
  return importance > 0.5
    ? `More important (${importance.toFixed(2)})`
    : `Less important (${importance.toFixed(2)})`;
}

export type IdentityCardProps = {
  projectId: string;
  identity: IdentityCardDto;
  selected: boolean;
  onSelect: (identityId: string) => void;
  onSetRole: (identityId: string, role: string) => void;
  onRename: (identityId: string, label: string | null) => void;
  onSetImportance: (identityId: string, importance: number) => void;
  onMergeWith: (identityId: string) => void;
};

/**
 * One person, as a card in the People panel.
 *
 * The cover thumbnail is fetched through `identityCover`, which is the only route from
 * the sealed biometric store to a screen. It is fetched per card rather than in a batch
 * because a panel of sixty identities should not decrypt sixty crops before it can draw
 * anything, and a card that has scrolled out of view has cancelled nothing but has also
 * cost nothing.
 */
export function IdentityCard({
  projectId,
  identity,
  selected,
  onSelect,
  onSetRole,
  onRename,
  onSetImportance,
  onMergeWith,
}: IdentityCardProps): JSX.Element {
  const [cover, setCover] = useState<string | null>(null);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(identity.label ?? '');

  useEffect(() => {
    if (!inTauri() || identity.coverFaceId === null) {
      return;
    }
    let live = true;
    api
      .identityCover(projectId, identity.coverFaceId)
      .then((crop) => {
        if (live) {
          setCover(crop.dataUrl);
        }
      })
      .catch(() => {
        // A crop that will not decrypt is not worth a banner on a card. The face is
        // still counted, the identity is still usable, and AURA-SEC-9002 has already
        // been logged with the reason.
        if (live) {
          setCover(null);
        }
      });
    return () => {
      live = false;
    };
  }, [projectId, identity.coverFaceId]);

  const note = varianceNote(identity);

  return (
    <li
      className={selected ? 'identity-card selected' : 'identity-card'}
      aria-label={identity.label ?? roleLabel(identity)}
    >
      <button
        type="button"
        className="cover"
        onClick={() => onSelect(identity.id)}
        aria-label={`Select ${identity.label ?? 'this person'}`}
      >
        {cover === null ? (
          <span className="cover-placeholder" aria-hidden="true" />
        ) : (
          <img src={cover} alt="" width={64} height={64} />
        )}
      </button>

      <div className="identity-body">
        {editing ? (
          <form
            onSubmit={(event) => {
              event.preventDefault();
              onRename(identity.id, draft.trim() === '' ? null : draft.trim());
              setEditing(false);
            }}
          >
            <label htmlFor={`name-${identity.id}`}>Name</label>
            <input
              id={`name-${identity.id}`}
              value={draft}
              onChange={(event) => setDraft(event.target.value)}
            />
            <button type="submit">Save</button>
          </form>
        ) : (
          <button type="button" className="name" onClick={() => setEditing(true)}>
            {identity.label ?? 'Unnamed'}
          </button>
        )}

        <p className="role">
          {roleLabel(identity)} <span className="provenance">({roleProvenance(identity)})</span>
        </p>
        <p className="counts">{faceCountLabel(identity)}</p>
        {note !== null && <p className="variance-note">{note}</p>}

        <ul className="reasons">
          {identity.roleReasons.map((reason) => (
            <li key={reason}>{reason}</li>
          ))}
        </ul>

        <label htmlFor={`role-${identity.id}`}>This person is</label>
        <select
          id={`role-${identity.id}`}
          value={identity.role}
          onChange={(event) => onSetRole(identity.id, event.target.value)}
        >
          {ASSIGNABLE_ROLES.map((role) => (
            <option key={role.value} value={role.value}>
              {role.label}
            </option>
          ))}
        </select>

        <label htmlFor={`importance-${identity.id}`}>
          Importance: {importanceLabel(identity.importance)}
        </label>
        <input
          id={`importance-${identity.id}`}
          type="range"
          min={0}
          max={100}
          value={Math.round(identity.importance * 100)}
          onChange={(event) => onSetImportance(identity.id, Number(event.target.value) / 100)}
        />

        <button type="button" onClick={() => onMergeWith(identity.id)}>
          Merge with…
        </button>

        {identity.companions.length > 0 && (
          <p className="companions">
            Seen with{' '}
            {identity.companions
              .slice(0, 3)
              .map((companion) => `${companion.label ?? 'someone'} (${companion.frames})`)
              .join(', ')}
          </p>
        )}
      </div>
    </li>
  );
}
