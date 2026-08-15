import { useCallback, useEffect, useState } from 'react';

import { api, asIpcError, inTauri } from '../../ipc/client';
import type { DuplicateSetDto, MomentDto, MomentListDto, MomentStatusDto } from '../../ipc/types';

/**
 * The moments view: three thousand loose files as a few hundred stacked cells.
 *
 * PHASE-08 section 2.1's UI, and acceptance criterion 1: "the grid can switch between
 * 'all frames' and 'moments' views, with correct counts".
 *
 * ## What a stack shows before it is opened
 *
 * A cover, a count badge, and at most one other badge. That is deliberately austere: a
 * photographer scans a wedding's worth of these, and a cell carrying five pieces of
 * metadata is a cell nobody reads. Everything else - the reasons, the bursts, the
 * duplicate sets - appears on expand, which is where a photographer has decided to pay
 * attention.
 *
 * ## Nothing here culls
 *
 * Section 2.2 puts every decision about a photograph's fate in phase 12. This component
 * can split a stack, merge two, pin one, and move a keep hint. It cannot reject a frame,
 * and there is no control on it that would - `suppressed` marks a frame as one that a
 * duplicate set caps out of the gallery, and it is drawn as an explanation rather than
 * as a decision.
 */

/** How many frames a stack shows as thumbnails before it says "and N more". */
export const PREVIEW_FRAMES = 4;

/**
 * The count badge on a collapsed stack.
 *
 * Frames, not bursts. A photographer looking at a wedding wants to know how many
 * photographs are behind this one; the burst structure is a detail of how they were
 * taken and belongs inside.
 *
 * A moment of one shows no badge at all rather than a "1": a badge that is on every
 * cell carries no information and turns a scannable grid into a wall of numerals.
 */
export function countBadge(moment: MomentDto): string | null {
  return moment.frameCount > 1 ? String(moment.frameCount) : null;
}

/**
 * The one other badge a stack carries, or null.
 *
 * Exactly one, and the order below is the priority - the same rule `chapterBadge` uses
 * on the timeline, for the same reason:
 *
 * 1. **yours** - the photographer split, merged or pinned this. It outranks everything
 *    because it is the only badge that says re-analysis will not touch this cell.
 * 2. **duplicates** - at least one set here caps what may be delivered. This is the
 *    badge that earns a click, because it is the one with an action behind it.
 * 3. **2 cameras** - two photographers caught this instant. Provenance rather than a
 *    call to action, and the reason phase 26 exists.
 */
export function stackBadge(moment: MomentDto): string | null {
  if (moment.userLocked) {
    return 'yours';
  }
  if (moment.duplicateSets > 0) {
    return moment.duplicateSets === 1 ? '1 duplicate set' : `${moment.duplicateSets} duplicate sets`;
  }
  if (moment.cameraCount > 1) {
    return `${moment.cameraCount} cameras`;
  }
  return null;
}

/**
 * How long the photographer spent on this moment, as they would say it.
 *
 * Sub-second for a burst, seconds for a take, minutes for a sequence. A bouquet toss
 * shown as "0 m" would be worse than useless; the whole point of the number is that it
 * distinguishes a held shutter from a re-set pose.
 */
export function spanLabel(moment: MomentDto): string {
  const ms = Math.max(0, moment.endMs - moment.startMs);
  if (moment.frameCount <= 1) {
    return 'single frame';
  }
  if (ms < 1_000) {
    return `${ms} ms`;
  }
  if (ms < 60_000) {
    const seconds = ms / 1_000;
    return `${seconds < 10 ? seconds.toFixed(1) : Math.round(seconds)} s`;
  }
  return `${Math.round(ms / 60_000)} m`;
}

/**
 * What the bursts inside a moment say, as one line.
 *
 * The distinction section 2.1 draws, in the one place a photographer can act on it: a
 * moment with three bursts of five is somebody who tried three times, and a moment with
 * one burst of fifteen is somebody who held the button. Those want different culls.
 */
export function burstSentence(moment: MomentDto): string | null {
  if (moment.frameCount <= 1 || moment.burstCount === 0) {
    return null;
  }
  if (moment.burstCount === 1) {
    return `One press of the shutter, ${moment.frameCount} frames.`;
  }
  return `${moment.burstCount} separate takes, ${moment.frameCount} frames in all.`;
}

/**
 * Frames grouped by burst, in order. The expanded stack's rows.
 *
 * A partition, always: every frame is in exactly one burst, which is what makes the
 * expanded view reconcilable with the count badge. `burstIx` is dense from the backend,
 * but this does not assume it - a gap would silently drop a row, and a photographer
 * counting frames would find one missing.
 */
export function framesByBurst(moment: MomentDto): MomentDto['frames'][] {
  const seen = new Map<number, MomentDto['frames']>();
  for (const frame of moment.frames) {
    const row = seen.get(frame.burstIx);
    if (row) {
      row.push(frame);
    } else {
      seen.set(frame.burstIx, [frame]);
    }
  }
  return [...seen.entries()].sort(([a], [b]) => a - b).map(([, row]) => row);
}

/**
 * How many keepers this moment can support, as a sentence.
 *
 * Section 6.4, and it is advice rather than a limit: phase 12 owns the cull and this
 * says what the evidence supports. The wording says "can" rather than "will" for
 * exactly that reason.
 */
export function keeperSentence(moment: MomentDto): string {
  if (moment.frameCount <= 1) {
    return 'One frame.';
  }
  if (moment.suggestedKeepers === 1) {
    return 'These frames say the same thing, so one of them is the keeper.';
  }
  return `The action changes across these frames, so up to ${moment.suggestedKeepers} of them could be keepers.`;
}

/**
 * Whether a stack can be split at a frame.
 *
 * Not at the first frame: that would leave the original empty, and the backend refuses
 * it with `AURA-ML-5029`. Refusing in the UI as well means the photographer gets a
 * disabled control rather than an error toast, which is the difference between a rule
 * they can see and a rule they discover.
 */
export function canSplitAt(moment: MomentDto, photoId: string): boolean {
  if (moment.frameCount < 2) {
    return false;
  }
  const position = moment.frames.findIndex((frame) => frame.photoId === photoId);
  return position > 0;
}

/**
 * Whether two stacks can be merged.
 *
 * The backend's rule, mirrored: same wedding, same chapter, not the same moment. It
 * does not require adjacency - two photographers' takes on one kiss can be minutes
 * apart on a mis-set clock.
 */
export function canMergeStacks(a: MomentDto | null, b: MomentDto | null): boolean {
  if (!a || !b || a.momentId === b.momentId) {
    return false;
  }
  return a.segmentId === b.segmentId;
}

/**
 * One sentence about the grouping pass, including every reason the view might be empty.
 *
 * Phase 05's rule, inherited a fourth time: report coverage when you report a result.
 * The denominator is **groupable** rather than photographs, because a frame with no
 * embedding cannot be grouped by any amount of trying and reporting a phase 05 gap as a
 * phase 08 failure sends somebody looking in the wrong place.
 */
export function coverageSentence(status: MomentStatusDto): string {
  if (status.photos === 0) {
    return 'This project has no photographs yet.';
  }
  if (status.groupable === 0) {
    return `None of the ${status.photos} photographs have been analysed yet. AURA needs the perceptual pass to have run before it can group them.`;
  }
  if (status.moments === 0) {
    return `${status.groupable} of ${status.photos} photographs are ready to group. Nothing has been grouped yet.`;
  }
  const percent = Math.round(status.coverage * 100);
  const size = status.meanSize.toFixed(1);
  const ungrouped = status.groupable - status.grouped;
  const unanalysed = status.photos - status.groupable;
  // Both denominators, always. The percentage is against `groupable` because that is
  // what phase 08 can be held to, and the photograph total is there because a
  // photographer reading "1200 of 1200" on a 3,000-image wedding would otherwise
  // conclude that 1,800 photographs had vanished.
  const scope =
    unanalysed > 0
      ? `${status.grouped} of ${status.groupable} analysed photographs (${status.photos} in the project)`
      : `${status.grouped} of ${status.groupable} photographs`;
  const tail =
    ungrouped > 0 ? ` ${ungrouped} could not be placed and are shown on their own.` : '';
  return `${scope} are in ${status.moments} moments (${percent}%), ${size} frames each.${tail}`;
}

/**
 * What the duplicate counters say, or null when there is nothing to report.
 *
 * Identical and near-identical only. A variant set is not counted here because it is not
 * stored and constrains nothing: the alternatives are the frames of a burst, which the
 * expanded stack already shows.
 */
export function duplicateSentence(status: MomentStatusDto): string | null {
  const [identical, nearIdentical] = status.duplicates;
  if (identical === 0 && nearIdentical === 0) {
    return null;
  }
  const parts: string[] = [];
  if (identical > 0) {
    parts.push(`${identical} set${identical === 1 ? '' : 's'} of the same file imported twice`);
  }
  if (nearIdentical > 0) {
    parts.push(
      `${nearIdentical} set${nearIdentical === 1 ? '' : 's'} where only one frame should reach the gallery`,
    );
  }
  return `${parts.join(', ')}. Nothing has been deleted, and you can see every frame.`;
}

/**
 * The warning when the grouping came out at an implausible size. `AURA-ML-5030`.
 *
 * A photographer whose wedding came out as one moment per frame, or as forty moments
 * for three thousand files, needs to know that before they cull - because the answer
 * (regroup, or split by hand) is different from the answer to a single bad stack.
 */
export function implausibleSentence(status: MomentStatusDto): string | null {
  if (!status.implausible) {
    return null;
  }
  return `AURA grouped this wedding into an unusual number of moments (${status.meanSize.toFixed(1)} frames each). The grouping is still usable and every stack can be split or merged by hand.`;
}

/**
 * Whether a frame is one a duplicate set caps out of the gallery.
 *
 * Drawn as an explanation, never as a rejection. The frame is still there, still
 * openable, and still exportable by hand; what this marks is that phase 12 will not
 * deliver it *and* its twin.
 */
export function suppressedLabel(frame: MomentDto['frames'][number]): string | null {
  return frame.suppressed ? 'same photograph as the one kept' : null;
}

/** The stacked grid. */
export function MomentStack({ projectId }: { projectId: string }): JSX.Element {
  const [list, setList] = useState<MomentListDto | null>(null);
  const [status, setStatus] = useState<MomentStatusDto | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [sets, setSets] = useState<DuplicateSetDto[]>([]);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!inTauri()) {
      return;
    }
    try {
      const [moments, header] = await Promise.all([
        api.listMoments({ projectId, segmentId: null }),
        api.momentStatus(projectId),
      ]);
      setList(moments);
      setStatus(header);
      setError(null);
    } catch (raised) {
      setError(asIpcError(raised).message);
    }
  }, [projectId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const expand = useCallback(async (momentId: string) => {
    setExpanded((current) => (current === momentId ? null : momentId));
    if (!inTauri()) {
      return;
    }
    try {
      setSets(await api.momentDuplicates(momentId));
    } catch (raised) {
      setError(asIpcError(raised).message);
    }
  }, []);

  const lock = useCallback(
    async (momentId: string, locked: boolean) => {
      if (!inTauri()) {
        return;
      }
      try {
        await api.lockMoment({ momentId, locked });
        await refresh();
      } catch (raised) {
        setError(asIpcError(raised).message);
      }
    },
    [refresh],
  );

  const split = useCallback(
    async (momentId: string, photoId: string) => {
      if (!inTauri()) {
        return;
      }
      try {
        await api.splitMoment({ momentId, photoId });
        await refresh();
      } catch (raised) {
        setError(asIpcError(raised).message);
      }
    },
    [refresh],
  );

  const undo = useCallback(async () => {
    if (!inTauri()) {
      return;
    }
    try {
      await api.undoMomentEdit(projectId);
      await refresh();
    } catch (raised) {
      setError(asIpcError(raised).message);
    }
  }, [projectId, refresh]);

  return (
    <section className="moments" aria-label="Moments">
      <header className="moments-header">
        <h2>Moments</h2>
        {status ? <p className="moments-coverage">{coverageSentence(status)}</p> : null}
        {status ? <p className="moments-duplicates">{duplicateSentence(status)}</p> : null}
        {status && implausibleSentence(status) ? (
          <p className="moments-warning" role="status">
            {implausibleSentence(status)}
          </p>
        ) : null}
        <button type="button" onClick={() => void undo()}>
          Undo last grouping change
        </button>
      </header>

      {error ? (
        <p className="moments-error" role="alert">
          {error}
        </p>
      ) : null}

      <ol className="moment-grid">
        {(list?.moments ?? []).map((moment) => (
          <li
            key={moment.momentId}
            className={moment.userLocked ? 'moment-cell locked' : 'moment-cell'}
          >
            <button
              type="button"
              className="moment-cover"
              aria-expanded={expanded === moment.momentId}
              onClick={() => void expand(moment.momentId)}
            >
              <span className="moment-cover-id">{moment.cover}</span>
              {countBadge(moment) ? (
                <span className="moment-count">{countBadge(moment)}</span>
              ) : null}
              {stackBadge(moment) ? (
                <span className="moment-badge">{stackBadge(moment)}</span>
              ) : null}
              <span className="moment-span">{spanLabel(moment)}</span>
            </button>

            {expanded === moment.momentId ? (
              <div className="moment-detail">
                <p>{burstSentence(moment)}</p>
                <p>{keeperSentence(moment)}</p>
                <ul className="moment-reasons">
                  {moment.reasons.map((reason) => (
                    <li key={reason}>{reason}</li>
                  ))}
                </ul>
                {framesByBurst(moment).map((row, index) => (
                  <ol key={row[0]?.photoId ?? index} className="moment-burst">
                    {row.map((frame) => (
                      <li key={frame.photoId} className={frame.suppressed ? 'suppressed' : ''}>
                        <span>{frame.photoId}</span>
                        {suppressedLabel(frame) ? (
                          <em className="moment-suppressed">{suppressedLabel(frame)}</em>
                        ) : null}
                        <button
                          type="button"
                          disabled={!canSplitAt(moment, frame.photoId)}
                          onClick={() => void split(moment.momentId, frame.photoId)}
                        >
                          Split here
                        </button>
                      </li>
                    ))}
                  </ol>
                ))}
                <button
                  type="button"
                  onClick={() => void lock(moment.momentId, !moment.userLocked)}
                >
                  {moment.userLocked ? 'Let AURA regroup this' : 'Keep this grouping'}
                </button>
                {sets.length > 0 ? (
                  <p className="moment-sets">
                    {sets.length} set{sets.length === 1 ? '' : 's'} of duplicates inside this
                    moment.
                  </p>
                ) : null}
              </div>
            ) : null}
          </li>
        ))}
      </ol>
    </section>
  );
}
