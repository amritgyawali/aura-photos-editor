import type { ChapterDto } from '../../ipc/types';

/**
 * How long a chapter lasted, as a photographer would say it.
 *
 * "1 h 45 m", not "105 minutes". Durations on a timeline are read at a glance and
 * compared with each other, and a reader who has to divide by sixty to compare the
 * ceremony with the dance floor is doing arithmetic the strip should have done.
 */
export function durationLabel(chapter: ChapterDto): string {
  const minutes = Math.max(0, Math.round(chapter.durationMinutes));
  if (minutes < 60) {
    return `${minutes} m`;
  }
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest === 0 ? `${hours} h` : `${hours} h ${rest} m`;
}

/**
 * The name shown on a chapter card.
 *
 * The photographer's own label always wins over the automation's chapter title. That is
 * the same rule `userLocked` enforces in the database, expressed in the one place a
 * photographer actually looks.
 */
export function chapterTitle(chapter: ChapterDto): string {
  const label = chapter.label?.trim();
  return label && label.length > 0 ? label : chapter.title;
}

/**
 * How wide a chapter's card should be, as a fraction of the strip.
 *
 * Proportional to **duration**, not to frame count, and the difference matters. A
 * ninety-minute dinner with forty photographs and a six-minute cake cutting with forty
 * photographs are not the same shape of event, and a strip drawn by frame count shows
 * them as identical. The strip is a picture of the *day*.
 *
 * Every chapter gets at least `MIN_WIDTH_FRACTION`, because a two-minute exit that is
 * 0.4 % of the day still has to be clickable.
 */
export const MIN_WIDTH_FRACTION = 0.04;

export function chapterWidths(chapters: ChapterDto[]): number[] {
  if (chapters.length === 0) {
    return [];
  }
  const spans = chapters.map((chapter) => Math.max(1, chapter.endMs - chapter.startMs));
  const total = spans.reduce((sum, span) => sum + span, 0);
  const raw = spans.map((span) => span / total);
  const floored = raw.map((fraction) => Math.max(fraction, MIN_WIDTH_FRACTION));
  const flooredTotal = floored.reduce((sum, fraction) => sum + fraction, 0);
  // Renormalise, so the floor does not push the strip past 100 %.
  return floored.map((fraction) => fraction / flooredTotal);
}

/**
 * The badge a chapter carries, or null.
 *
 * Exactly one badge, and the order below is the priority: a locked chapter is the
 * photographer's and says so first, a chapter that needs review is the next most useful
 * thing to know, and a cloud-named chapter is provenance rather than a call to action.
 */
export function chapterBadge(chapter: ChapterDto): string | null {
  if (chapter.userLocked) {
    return 'yours';
  }
  if (chapter.needsReview) {
    return 'needs review';
  }
  return null;
}

/**
 * Why this chapter is this chapter, as one line.
 *
 * Invariant 2: every AI decision carries confidence and reasons. The strip has room for
 * one reason, so it shows the first - which the backend orders as the strongest - and
 * the panel shows the rest on expand.
 */
export function chapterReason(chapter: ChapterDto): string {
  const first = chapter.reasons[0];
  if (first) {
    return first;
  }
  return `${chapter.imageCount} photographs, mostly ${chapter.dominantScene.replace(/_/g, ' ')}`;
}

/**
 * The confidence, as a percentage a person can read.
 *
 * Rounded to a whole number and never shown as 100 %: the highest a chapter can reach
 * automatically is 0.98, and a photographer who sees "100 %" on an automated decision
 * has been told something untrue. A locked chapter is the exception and it shows no
 * percentage at all, because a photographer's decision has no confidence - it has an
 * author.
 */
export function confidenceLabel(chapter: ChapterDto): string | null {
  if (chapter.userLocked) {
    return null;
  }
  return `${Math.round(chapter.confidence * 100)}%`;
}
