import { useCallback, useEffect, useState } from 'react';

import { api, asIpcError } from '../../ipc/client';
import type { IntegrityStatusDto } from '../../ipc/types';

/**
 * The technical filter chips and the review queue.
 *
 * PHASE-09 section 9's MFE task: "filter chips (soft, blinked, clipped, noisy) and a
 * technical-review queue."
 *
 * ## Why the chips are not hard-coded here
 *
 * `IntegrityStatusDto` carries `flagNames` beside `flagCounts`, in the same order, and
 * this component reads the pair. A chip list written as a literal in the interface is a
 * second copy of `IntegrityFlags::ALL`, and the first time somebody adds a fifteenth
 * flag the two would disagree - silently, because a missing chip looks like a wedding
 * with no such frames in it.
 *
 * ## Why the counts are labelled "at most"
 *
 * One frame can be soft *and* noisy, so the chip counts overlap and their sum is an
 * upper bound rather than a total. The header says so, because a photographer who adds
 * four chips together and gets more than their whole wedding has been told something
 * untrue.
 *
 * ## What a chip is not
 *
 * A chip is a filter. It does not reject, hide or reorder a delivery; it shows the
 * frames carrying one mark so a photographer can look at them. `intentional_motion` and
 * `eyes_closed_ok` are chips too, and they are the two a photographer is most likely to
 * want to filter *for*.
 */

/** The four chips section 9 names, in the order they are offered. */
export const DEFAULT_CHIPS = ['subject_soft', 'eyes_closed', 'highlight_lost', 'heavy_noise'];

/** What one flag is called in the interface. */
export function chipLabel(flag: string): string {
  switch (flag) {
    case 'subject_soft':
      return 'Soft';
    case 'camera_shake':
      return 'Shaken';
    case 'subject_motion':
      return 'Subject moved';
    case 'intentional_motion':
      return 'Deliberate blur';
    case 'back_focus':
      return 'Focus behind';
    case 'front_focus':
      return 'Focus short';
    case 'highlight_lost':
      return 'Blown';
    case 'shadow_lost':
      return 'Crushed';
    case 'heavy_noise':
      return 'Noisy';
    case 'eyes_closed':
      return 'Blinked';
    case 'eyes_closed_ok':
      return 'Eyes closed on purpose';
    case 'squint':
      return 'Squinting';
    case 'no_subject_detected':
      return 'No subject';
    case 'mixed_light_risk':
      return 'Mixed light';
    default:
      return flag.replace(/_/g, ' ');
  }
}

/**
 * True when a chip describes something wrong.
 *
 * The three that do not are drawn differently, because a chip that reads "Deliberate
 * blur" in the same red as "Blown" teaches a photographer that the product thinks a pan
 * is a fault.
 */
export function isDefectChip(flag: string): boolean {
  return !['intentional_motion', 'eyes_closed_ok', 'no_subject_detected'].includes(flag);
}

/** The chips worth offering: the four defaults plus anything this wedding actually has. */
export function chipsFor(status: IntegrityStatusDto): string[] {
  const present = status.flagNames.filter(
    (name, index) => (status.flagCounts[index] ?? 0) > 0 && !DEFAULT_CHIPS.includes(name),
  );
  return [...DEFAULT_CHIPS, ...present];
}

/** How many frames carry one flag. */
export function chipCount(status: IntegrityStatusDto, flag: string): number {
  const index = status.flagNames.indexOf(flag);
  return index < 0 ? 0 : (status.flagCounts[index] ?? 0);
}

/**
 * The header sentence, which says what was checked before it says what was found.
 *
 * Both denominators, for the reason phase 08's `coverageSentence` gives: a queue drawn
 * over a half-checked wedding describes half a wedding, and a photographer who does not
 * know that will read an empty queue as good news.
 */
export function coverageSentence(status: IntegrityStatusDto): string {
  if (status.scored === 0) {
    return 'No photographs have been checked yet.';
  }
  const coverage = Math.round(status.coverage * 100);
  const aware = Math.round(status.subjectAware * 100);
  const head = `${status.scored} of ${status.photos} photographs checked (${coverage}%).`;
  if (aware >= 60) {
    return head;
  }
  return `${head} Only ${aware}% of them had a face or person to judge against, so the rest were measured on the whole frame.`;
}

/** The queue's own header, and the reason its counts do not add up to the total. */
export function queueSentence(status: IntegrityStatusDto): string {
  if (status.defectiveAtMost === 0) {
    return 'Nothing was marked.';
  }
  return `At most ${status.defectiveAtMost} frames carry a mark - at most, because one frame can be soft and noisy at the same time.`;
}

/** The note shown when a camera model has not been measured. */
export function uncalibratedSentence(status: IntegrityStatusDto): string | null {
  if (status.uncalibrated.length === 0) {
    return null;
  }
  return `AURA has not measured ${status.uncalibrated.join(', ')} yet, so photographs from ${
    status.uncalibrated.length === 1 ? 'that body' : 'those bodies'
  } were judged more cautiously.`;
}

/** Technical filter chips over one project, and the frames one chip selects. */
export function FilterChips({
  projectId,
  onSelect,
}: {
  projectId: string;
  onSelect: (photoIds: string[]) => void;
}): JSX.Element {
  const [status, setStatus] = useState<IntegrityStatusDto | null>(null);
  const [active, setActive] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setStatus(await api.integrityStatus(projectId));
      setError(null);
    } catch (raised) {
      setError(asIpcError(raised).message);
    }
  }, [projectId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const choose = useCallback(
    async (flag: string) => {
      const next = active === flag ? null : flag;
      setActive(next);
      if (next === null) {
        onSelect([]);
        return;
      }
      try {
        onSelect(await api.flaggedImages({ projectId, flags: [next] }));
        setError(null);
      } catch (raised) {
        setError(asIpcError(raised).message);
      }
    },
    [active, onSelect, projectId],
  );

  if (error !== null) {
    return (
      <section className="chips" aria-label="Technical filters">
        <p className="chips-error">{error}</p>
      </section>
    );
  }

  if (status === null) {
    return (
      <section className="chips" aria-label="Technical filters">
        <p>Reading the technical check…</p>
      </section>
    );
  }

  const note = uncalibratedSentence(status);

  return (
    <section className="chips" aria-label="Technical filters">
      <p className="chips-coverage">{coverageSentence(status)}</p>
      <p className="chips-queue">{queueSentence(status)}</p>
      {note !== null && <p className="chips-uncalibrated">{note}</p>}

      <ul className="chips-list">
        {chipsFor(status).map((flag) => (
          <li key={flag}>
            <button
              type="button"
              className={[
                isDefectChip(flag) ? 'chip-defect' : 'chip-note',
                active === flag ? 'chip-active' : '',
              ]
                .filter(Boolean)
                .join(' ')}
              aria-pressed={active === flag}
              onClick={() => void choose(flag)}
            >
              {chipLabel(flag)} ({chipCount(status, flag)})
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}
