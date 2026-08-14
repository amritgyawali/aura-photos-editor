import { describe, expect, it } from 'vitest';

import type { ChapterDto, SceneProfileDto, StoryStatusDto } from '../../ipc/types';
import {
  MIN_WIDTH_FRACTION,
  chapterBadge,
  chapterReason,
  chapterTitle,
  chapterWidths,
  confidenceLabel,
  durationLabel,
} from './ChapterStrip';
import {
  MIN_CHAPTER_MS,
  boundaryRange,
  boundarySummary,
  canMerge,
  canSplit,
  clampBoundary,
  mergeSummary,
  splitSummary,
} from './BoundaryEditor';
import {
  coverageSentence,
  fallbackSentence,
  lockedSentence,
  profileFor,
  reviewPrompt,
} from './Timeline';

const HOUR = 3_600_000;

const chapter = (over: Partial<ChapterDto> = {}): ChapterDto => ({
  segmentId: 'seg_00000000-0000-7000-8000-000000000001',
  ordinal: 0,
  chapter: 'ceremony',
  label: null,
  title: 'Ceremony',
  startMs: 0,
  endMs: HOUR,
  durationMinutes: 60,
  dominantScene: 'ceremony',
  confidence: 0.86,
  keyFrame: 'pht_00000000-0000-7000-8000-000000000001',
  imageCount: 240,
  reasons: ['218 of 240 frames carry a scene label, most often ceremony'],
  userLocked: false,
  needsReview: false,
  ...over,
});

const status = (over: Partial<StoryStatusDto> = {}): StoryStatusDto => ({
  photos: 4000,
  classified: 4000,
  coverage: 1,
  chapters: 11,
  needsReview: 0,
  locked: 0,
  penalty: 0.07,
  gapsOnly: false,
  sceneVer: 100,
  taxonomyVer: 1,
  ritualsKnown: 48,
  ...over,
});

const profile = (over: Partial<SceneProfileDto> = {}): SceneProfileDto => ({
  scene: 'ceremony',
  title: 'Ceremony',
  keeperMin: 0.12,
  keeperMax: 0.3,
  maxAcceptableNoise: 0.62,
  maxAcceptableBlur: 0.35,
  subjectFocusWeight: 0.45,
  emotionWeight: 0.3,
  compositionWeight: 0.25,
  editingIntent: 'neutral',
  mustCover: true,
  rationale: 'The reference row for the whole file.',
  ...over,
});

describe('chapter strip', () => {
  it('reads a duration the way a photographer would say it', () => {
    expect(durationLabel(chapter({ durationMinutes: 45 }))).toBe('45 m');
    expect(durationLabel(chapter({ durationMinutes: 60 }))).toBe('1 h');
    expect(durationLabel(chapter({ durationMinutes: 105 }))).toBe('1 h 45 m');
  });

  it("prefers the photographer's own label over the automation's title", () => {
    expect(chapterTitle(chapter({ label: 'Saptapadi' }))).toBe('Saptapadi');
    expect(chapterTitle(chapter({ label: '   ' }))).toBe('Ceremony');
    expect(chapterTitle(chapter())).toBe('Ceremony');
  });

  it('sizes chapters by duration, not by frame count', () => {
    // A 90-minute dinner with 40 frames and a 6-minute cake with 40 frames are not the
    // same shape of event, and a strip drawn by frame count would show them identically.
    const dinner = chapter({ startMs: 0, endMs: 90 * 60_000, imageCount: 40 });
    const cake = chapter({
      segmentId: 'seg_2',
      ordinal: 1,
      startMs: 90 * 60_000,
      endMs: 96 * 60_000,
      imageCount: 40,
    });
    const widths = chapterWidths([dinner, cake]);
    expect(widths[0]).toBeGreaterThan(widths[1] ?? 0);
  });

  it('gives even the shortest chapter something clickable', () => {
    const long = chapter({ startMs: 0, endMs: 10 * HOUR });
    const brief = chapter({ segmentId: 'seg_2', ordinal: 1, startMs: 10 * HOUR, endMs: 10 * HOUR + 60_000 });
    const widths = chapterWidths([long, brief]);
    expect(widths[1]).toBeGreaterThanOrEqual(MIN_WIDTH_FRACTION * 0.5);
  });

  it('sums to one whatever the floor did', () => {
    const many = Array.from({ length: 12 }, (_, index) =>
      chapter({
        segmentId: `seg_${index}`,
        ordinal: index,
        startMs: index * 60_000,
        endMs: index * 60_000 + 60_000,
      }),
    );
    const total = chapterWidths(many).reduce((sum, width) => sum + width, 0);
    expect(total).toBeCloseTo(1, 5);
  });

  it('badges a locked chapter before a review flag', () => {
    expect(chapterBadge(chapter({ userLocked: true, needsReview: true }))).toBe('yours');
    expect(chapterBadge(chapter({ needsReview: true }))).toBe('needs review');
    expect(chapterBadge(chapter())).toBeNull();
  });

  it("shows no confidence on a photographer's own chapter", () => {
    // A photographer's decision has no confidence; it has an author.
    expect(confidenceLabel(chapter({ userLocked: true }))).toBeNull();
    expect(confidenceLabel(chapter({ confidence: 0.86 }))).toBe('86%');
  });

  it('always has a reason to show', () => {
    expect(chapterReason(chapter())).toContain('scene label');
    expect(chapterReason(chapter({ reasons: [] }))).toContain('240 photographs');
  });
});

describe('boundary editor', () => {
  const first = chapter({ ordinal: 0, startMs: 0, endMs: HOUR });
  const second = chapter({ segmentId: 'seg_2', ordinal: 1, startMs: HOUR, endMs: 2 * HOUR });

  it('has no range on the last chapter', () => {
    expect(boundaryRange(second, undefined)).toBeNull();
  });

  it('keeps a frame on each side', () => {
    const range = boundaryRange(first, second);
    expect(range).not.toBeNull();
    expect(range?.min).toBe(MIN_CHAPTER_MS);
    expect(range?.max).toBe(2 * HOUR - MIN_CHAPTER_MS);
  });

  it('clamps an overshooting drag rather than refusing it', () => {
    expect(clampBoundary(-50_000, first, second)).toBe(MIN_CHAPTER_MS);
    expect(clampBoundary(9 * HOUR, first, second)).toBe(2 * HOUR - MIN_CHAPTER_MS);
  });

  it('names both chapters, because both are locked', () => {
    const summary = boundarySummary(first, second, HOUR + 600_000);
    expect(summary).toContain('Ceremony');
    expect(summary).toContain('become yours');
  });

  it('says nothing when the boundary has not moved', () => {
    expect(boundarySummary(first, second, HOUR)).toBeNull();
  });

  it('refuses a non-adjacent merge before the backend has to', () => {
    const far = chapter({ segmentId: 'seg_9', ordinal: 5 });
    expect(canMerge(first, second)).toBe(true);
    expect(canMerge(first, far)).toBe(false);
    expect(mergeSummary(first, far)).toContain('not next to each other');
  });

  it('adds the frame counts when it merges', () => {
    expect(mergeSummary(first, second)).toContain('480 photographs');
  });

  it('refuses a split that would empty a side', () => {
    expect(canSplit(0, 240)).toBe(false);
    expect(canSplit(240, 240)).toBe(false);
    expect(canSplit(100, 240)).toBe(true);
    expect(splitSummary(first, 0)).toContain('at least one photograph');
    expect(splitSummary(first, 100)).toContain('100 and 140');
  });
});

describe('timeline header', () => {
  it('explains an empty timeline rather than showing nothing', () => {
    expect(coverageSentence(status({ photos: 0 }))).toContain('no photographs');
    expect(coverageSentence(status({ classified: 0 }))).toContain('perceptual pass');
    expect(coverageSentence(status({ chapters: 0 }))).toContain('not been built');
  });

  it('reports partial coverage as a percentage', () => {
    const sentence = coverageSentence(status({ classified: 1600, coverage: 0.4 }));
    expect(sentence).toContain('40%');
    expect(sentence).toContain('11 chapters');
  });

  it('says when the boundaries came from the clock', () => {
    expect(fallbackSentence(status())).toBeNull();
    expect(fallbackSentence(status({ gapsOnly: true }))).toContain('moved location');
  });

  it('prompts for review rather than reporting it', () => {
    expect(reviewPrompt(status())).toBeNull();
    const prompt = reviewPrompt(status({ needsReview: 3 }));
    expect(prompt).toContain('3 chapters are uncertain');
    expect(prompt).toContain('before you deliver');
  });

  it('uses the singular for one chapter', () => {
    expect(reviewPrompt(status({ needsReview: 1 }))).toContain('1 chapter are uncertain');
    expect(lockedSentence(status({ locked: 1 }))).toContain('1 chapter is yours');
  });

  it('reassures that locked chapters survive a re-analysis', () => {
    expect(lockedSentence(status())).toBeNull();
    expect(lockedSentence(status({ locked: 4 }))).toContain('will not change them');
  });

  it('finds the profile a chapter is judged by', () => {
    const profiles = [profile(), profile({ scene: 'dance_floor', title: 'Dance Floor' })];
    expect(profileFor(chapter(), profiles)?.title).toBe('Ceremony');
    expect(profileFor(chapter({ dominantScene: 'venue' }), profiles)).toBeNull();
  });
});
