import { useCallback, useEffect, useState } from 'react';

import { emotion as emotionApi, asIpcError } from '../../ipc/client';
import type { MomentPeakDto, RankedByEmotionDto } from '../../ipc/types';

/**
 * The moment browser, sorted by emotion, and the reaction pair viewer beside it.
 *
 * PHASE-10 section 9's MFE task: "moment browser sorted by emotion, reaction pair
 * viewer".
 *
 * ## What this is not
 *
 * **It is not a shortlist.** Section 2.2 puts final selection in phase 12 and album
 * sequencing in phase 29. This panel orders frames by `emotionScore` because "ranks
 * every frame by emotional value" is what phase 10 shipped, and an ordering is not a
 * selection: there is no checkbox, no star, no export and no count of "how many you
 * would keep". The one thing a photographer can do here that changes state is tell AURA
 * which frame of a moment is the one, and that changes what the browser leads with and
 * nothing else.
 *
 * The temptation to add a keep button here is the strongest in the product so far, and
 * it is the reason this header exists.
 *
 * ## The three states of a peak
 *
 * Two would not be enough. A moment can have a clear best frame, can have been examined
 * and found flat, or can not have been scored at all - and only the first of those is a
 * peak. A browser that drew the second as a peak would point at a rounding error, and
 * phase 29 builds album spreads around what this panel points at.
 */

/** Enough of a peak to describe it. */
export type PeakSummary = Pick<MomentPeakDto, 'resolved' | 'margin' | 'frames'>;

/**
 * Order a project's frames for the browser.
 *
 * Strongest first, with the photo id as the tie-break so that two renders of the same
 * data produce the same order. A sort that fell back to input order would reshuffle a
 * flat moment every time the panel re-fetched.
 */
export function orderForBrowser(rows: RankedByEmotionDto[]): RankedByEmotionDto[] {
  return [...rows].sort(
    (left, right) =>
      right.emotionScore - left.emotionScore || left.photoId.localeCompare(right.photoId),
  );
}

/** What a moment's peak reads like. */
export function peakSentence(peak: PeakSummary): string {
  if (!peak.resolved) {
    return `The ${peak.frames} frames here are within ${Math.round(peak.margin * 100)}% of each other, so no single frame is the one.`;
  }
  return `The strongest frame of this moment wins by ${Math.round(peak.margin * 100)}%.`;
}

/** The line under a moment's title. */
export function momentSubtitle(frames: number, topScore: number): string {
  return `${frames} frames · strongest reads ${Math.round(topScore * 100)}%`;
}

/** What the browser needs from its parent. */
export type MomentBrowserProps = {
  projectId: string;
  /** Injected in tests. Live code lets the panel fetch. */
  rows?: RankedByEmotionDto[];
  /** How many frames to ask for. */
  limit?: number;
  /** Called when a frame is chosen, so the parent can show the Emotion card. */
  onSelect?: (photoId: string) => void;
};

/** The panel. */
export function MomentBrowser({
  projectId,
  rows,
  limit,
  onSelect,
}: MomentBrowserProps): JSX.Element {
  const [loaded, setLoaded] = useState<RankedByEmotionDto[] | undefined>(rows);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const next = await emotionApi.rankedByEmotion({ projectId, limit });
      setLoaded(next);
      setError(null);
    } catch (thrown) {
      setError(asIpcError(thrown).message);
    }
  }, [limit, projectId]);

  useEffect(() => {
    if (rows === undefined) {
      void load();
    }
  }, [load, rows]);

  if (error) {
    return (
      <section className="moment-browser moment-browser--error" aria-label="moments by emotion">
        <p>{error}</p>
      </section>
    );
  }

  if (loaded === undefined) {
    return (
      <section className="moment-browser moment-browser--loading" aria-label="moments by emotion">
        <p>Reading this wedding…</p>
      </section>
    );
  }

  const ordered = orderForBrowser(loaded);

  return (
    <section className="moment-browser" aria-label="moments by emotion">
      <header className="moment-browser__header">
        <h2>Moments by emotion</h2>
        <p className="moment-browser__note">
          An ordering, not a shortlist. AURA has not chosen anything here.
        </p>
      </header>
      <ol className="moment-browser__list">
        {ordered.map((row) => (
          <li key={row.photoId} className="moment-browser__row" data-photo-id={row.photoId}>
            <button
              type="button"
              className="moment-browser__button"
              onClick={() => onSelect?.(row.photoId)}
            >
              <span className="moment-browser__score">{Math.round(row.emotionScore * 100)}%</span>
              <span className="moment-browser__id">{row.photoId}</span>
            </button>
          </li>
        ))}
      </ol>
      {ordered.length === 0 ? (
        <p className="moment-browser__empty">
          Nothing in this wedding has been read yet. That is not the same as finding nothing in it.
        </p>
      ) : null}
    </section>
  );
}
