import type { ChapterDto } from '../../ipc/types';

/**
 * Where a boundary may be dragged to, in milliseconds.
 *
 * The backend refuses a move that would empty either side (`AURA-ML-5025`), and the
 * editor's job is to make that refusal unreachable rather than to discover it. So the
 * range is the earlier chapter's start plus one frame's worth of time, up to the later
 * chapter's end minus the same.
 *
 * Returns null when there is no boundary to move - the last chapter, or a pair that is
 * already degenerate.
 */
export const MIN_CHAPTER_MS = 1000;

export function boundaryRange(
  chapter: ChapterDto,
  next: ChapterDto | undefined,
): { min: number; max: number } | null {
  if (!next) {
    return null;
  }
  const min = chapter.startMs + MIN_CHAPTER_MS;
  const max = next.endMs - MIN_CHAPTER_MS;
  if (max <= min) {
    return null;
  }
  return { min, max };
}

/**
 * Clamp a proposed boundary into the legal range.
 *
 * Clamping rather than rejecting: a drag that overshoots is a hand moving a few pixels
 * too far, not a decision, and snapping it to the edge is what every timeline in every
 * editing application does.
 */
export function clampBoundary(
  proposed: number,
  chapter: ChapterDto,
  next: ChapterDto | undefined,
): number | null {
  const range = boundaryRange(chapter, next);
  if (!range) {
    return null;
  }
  return Math.min(Math.max(Math.round(proposed), range.min), range.max);
}

/**
 * What moving this boundary will do, in a sentence, before it is done.
 *
 * Both chapters are named because both are changed and both are locked - ADR-0016
 * decision 2 - and a photographer who thinks they are editing one chapter will be
 * surprised when the next re-analysis leaves the other one alone too.
 */
export function boundarySummary(
  chapter: ChapterDto,
  next: ChapterDto | undefined,
  proposedMs: number,
): string | null {
  const clamped = clampBoundary(proposedMs, chapter, next);
  if (clamped === null || !next) {
    return null;
  }
  const shift = Math.round((clamped - chapter.endMs) / 1000);
  if (shift === 0) {
    return null;
  }
  const direction = shift > 0 ? 'later' : 'earlier';
  const seconds = Math.abs(shift);
  const amount = seconds < 120 ? `${seconds} s` : `${Math.round(seconds / 60)} m`;
  return `Move the boundary ${amount} ${direction}. Both “${chapter.title}” and “${next.title}” become yours and re-analysis will not move them back.`;
}

/**
 * Whether two chapters may be merged.
 *
 * Adjacency only. The backend refuses a non-adjacent merge because merging chapters 2
 * and 5 would silently absorb 3 and 4 and no dialog asked about those; the editor
 * disables the control for the same reason rather than letting the refusal be the way a
 * photographer learns it.
 */
export function canMerge(a: ChapterDto, b: ChapterDto): boolean {
  return Math.abs(a.ordinal - b.ordinal) === 1;
}

/**
 * What merging will do, in a sentence.
 */
export function mergeSummary(a: ChapterDto, b: ChapterDto): string {
  if (!canMerge(a, b)) {
    return 'Those chapters are not next to each other. Merging them would absorb the ones in between.';
  }
  const [first, second] = a.ordinal < b.ordinal ? [a, b] : [b, a];
  const total = first.imageCount + second.imageCount;
  return `“${second.title}” joins “${first.title}”. One chapter of ${total} photographs, and it becomes yours.`;
}

/**
 * Whether a split at this photograph is allowed.
 *
 * Splitting at the first photograph would leave the earlier chapter empty, which the
 * backend refuses. The editor knows the position, so it can say so first.
 */
export function canSplit(positionInChapter: number, chapterSize: number): boolean {
  return positionInChapter > 0 && positionInChapter < chapterSize;
}

/**
 * What splitting will do, in a sentence.
 */
export function splitSummary(
  chapter: ChapterDto,
  positionInChapter: number,
): string {
  if (!canSplit(positionInChapter, chapter.imageCount)) {
    return 'A chapter has to keep at least one photograph on each side of a split.';
  }
  const before = positionInChapter;
  const after = chapter.imageCount - positionInChapter;
  return `“${chapter.title}” becomes two chapters of ${before} and ${after} photographs, and both become yours.`;
}
