import { useCallback, useEffect, useState } from 'react';

import { api, asIpcError, inTauri } from '../../ipc/client';
import type {
  ChapterDto,
  SceneProfileDto,
  StoryOutlineDto,
  StoryStatusDto,
} from '../../ipc/types';
import {
  chapterBadge,
  chapterReason,
  chapterTitle,
  chapterWidths,
  confidenceLabel,
  durationLabel,
} from './ChapterStrip';

/**
 * One sentence about the scene pass, including every reason the timeline might be
 * empty.
 *
 * The phase 05 rule, inherited for the third time: report coverage when you report a
 * result. An empty chapter strip on an unclassified wedding is a support ticket, and
 * this is the sentence that answers it before it is filed.
 */
export function coverageSentence(status: StoryStatusDto): string {
  if (status.photos === 0) {
    return 'This project has no photographs yet.';
  }
  if (status.classified === 0) {
    return `None of the ${status.photos} photographs have been read for scenes yet. AURA needs the perceptual pass to have run first.`;
  }
  const percent = Math.round(status.coverage * 100);
  if (status.chapters === 0) {
    return `${status.classified} of ${status.photos} photographs labelled (${percent}%). The story has not been built yet.`;
  }
  return `${status.classified} of ${status.photos} photographs labelled (${percent}%), in ${status.chapters} chapters.`;
}

/**
 * What the segmentation fell back to, when it did.
 *
 * `AURA-ML-5026`. A photographer whose ten-hour wedding came out as four chapters needs
 * to know that the boundaries came from the clock rather than from the photographs,
 * because the answer - adjust them by hand - is different from the answer to a
 * mislabelled chapter.
 */
export function fallbackSentence(status: StoryStatusDto): string | null {
  if (!status.gapsOnly || status.chapters === 0) {
    return null;
  }
  return 'AURA could not divide this wedding into a sensible set of chapters, so the boundaries are where you moved location. Drag them to where they should be; your edits are kept.';
}

/**
 * How many chapters want a look, as a call to action.
 *
 * Section 6.4: low-confidence chapters are flagged rather than silently guessed, and
 * acceptance criterion 6 is that a photographer can see which. So this is a prompt, not
 * a status line.
 */
export function reviewPrompt(status: StoryStatusDto): string | null {
  if (status.needsReview === 0) {
    return null;
  }
  const plural = status.needsReview === 1 ? 'chapter' : 'chapters';
  return `${status.needsReview} ${plural} are uncertain. Check the names before you deliver - every later decision is made per chapter.`;
}

/**
 * What the photographer has already decided, as reassurance.
 *
 * Not decoration. Acceptance criterion 4 is "editing a boundary or renaming a chapter
 * persists through re-analysis", and the way a photographer verifies that is by seeing
 * the count survive a re-run.
 */
export function lockedSentence(status: StoryStatusDto): string | null {
  if (status.locked === 0) {
    return null;
  }
  const plural = status.locked === 1 ? 'chapter is' : 'chapters are';
  return `${status.locked} ${plural} yours. Re-analysis will not change them.`;
}

/**
 * The scene profile line for one chapter.
 *
 * ADR-0016 decision 4, at the point of use: this is the sentence that answers "why is
 * my dance floor being judged this way", and it is the rationale a product manager
 * signed off rather than a restatement of the numbers.
 */
export function profileFor(
  chapter: ChapterDto,
  profiles: SceneProfileDto[],
): SceneProfileDto | null {
  return profiles.find((profile) => profile.scene === chapter.dominantScene) ?? null;
}

/**
 * The story timeline: a chapter strip with counts, durations and review flags.
 *
 * Section 11 budgets 200 ms for this to open, which is why it is two calls -
 * `storyStatus` and `storyOutline` - rather than one per chapter.
 */
export function Timeline({ projectId }: { projectId: string }): JSX.Element {
  const [status, setStatus] = useState<StoryStatusDto | null>(null);
  const [outline, setOutline] = useState<StoryOutlineDto | null>(null);
  const [profiles, setProfiles] = useState<SceneProfileDto[]>([]);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!inTauri()) {
      return;
    }
    try {
      const [nextStatus, nextOutline, nextProfiles] = await Promise.all([
        api.storyStatus(projectId),
        api.storyOutline(projectId),
        api.sceneProfiles(projectId),
      ]);
      setStatus(nextStatus);
      setOutline(nextOutline);
      setProfiles(nextProfiles);
      setProblem(null);
    } catch (error) {
      setProblem(asIpcError(error).message);
    }
  }, [projectId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  if (problem) {
    return <section className="timeline timeline--problem">{problem}</section>;
  }
  if (!status || !outline) {
    return <section className="timeline timeline--loading">Reading the story…</section>;
  }

  const widths = chapterWidths(outline.chapters);
  const fallback = fallbackSentence(status);
  const review = reviewPrompt(status);
  const locked = lockedSentence(status);

  return (
    <section className="timeline">
      <header className="timeline__header">
        <p className="timeline__coverage">{coverageSentence(status)}</p>
        {fallback ? <p className="timeline__fallback">{fallback}</p> : null}
        {review ? <p className="timeline__review">{review}</p> : null}
        {locked ? <p className="timeline__locked">{locked}</p> : null}
      </header>

      <ol className="timeline__strip">
        {outline.chapters.map((chapter, index) => {
          const badge = chapterBadge(chapter);
          const confidence = confidenceLabel(chapter);
          const profile = profileFor(chapter, profiles);
          const open = expanded === chapter.segmentId;
          return (
            <li
              className="timeline__chapter"
              key={chapter.segmentId}
              style={{ flexBasis: `${(widths[index] ?? 0) * 100}%` }}
            >
              <button
                className="timeline__chapter-button"
                onClick={() => setExpanded(open ? null : chapter.segmentId)}
                type="button"
              >
                <span className="timeline__chapter-title">{chapterTitle(chapter)}</span>
                <span className="timeline__chapter-meta">
                  {chapter.imageCount} · {durationLabel(chapter)}
                  {confidence ? ` · ${confidence}` : ''}
                </span>
                {badge ? <span className="timeline__badge">{badge}</span> : null}
              </button>
              {open ? (
                <div className="timeline__detail">
                  <ul className="timeline__reasons">
                    {chapter.reasons.map((reason) => (
                      <li key={reason}>{reason}</li>
                    ))}
                  </ul>
                  {profile ? (
                    <p className="timeline__profile" title={profile.rationale}>
                      Judged as {profile.title}: {profile.editingIntent} grade,
                      {profile.mustCover ? ' coverage guaranteed.' : ' no coverage guarantee.'}
                    </p>
                  ) : null}
                </div>
              ) : (
                <p className="timeline__reason">{chapterReason(chapter)}</p>
              )}
            </li>
          );
        })}
      </ol>
    </section>
  );
}
