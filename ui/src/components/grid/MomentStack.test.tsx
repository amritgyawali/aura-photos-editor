import { describe, expect, it } from 'vitest';

import type { DuplicateSetDto, MomentDto, MomentStatusDto } from '../../ipc/types';
import {
  PREVIEW_FRAMES,
  burstSentence,
  canMergeStacks,
  canSplitAt,
  countBadge,
  coverageSentence,
  duplicateSentence,
  framesByBurst,
  implausibleSentence,
  keeperSentence,
  spanLabel,
  stackBadge,
  suppressedLabel,
} from './MomentStack';
import {
  comparisons,
  confidenceLabel,
  hintSource,
  keepConsequence,
  reviewOrder,
  setConsequence,
  setHeading,
} from './DuplicatePanel';

const photo = (n: number) => `pht_00000000-0000-7000-8000-00000000000${n}`;

const frame = (n: number, burstIx = 0, suppressed = false): MomentDto['frames'][number] => ({
  photoId: photo(n),
  position: n,
  burstIx,
  suppressed,
});

const moment = (over: Partial<MomentDto> = {}): MomentDto => ({
  momentId: 'mom_00000000-0000-7000-8000-000000000001',
  segmentId: 'seg_00000000-0000-7000-8000-000000000001',
  cover: photo(0),
  frames: [frame(0), frame(1), frame(2)],
  frameCount: 3,
  burstCount: 1,
  cameraCount: 1,
  startMs: 0,
  endMs: 1_400,
  durationS: 1,
  diversity: 0.1,
  suggestedKeepers: 1,
  confidence: 0.86,
  reasons: ['3 frames over 1.4 s'],
  userLocked: false,
  duplicateSets: 0,
  ...over,
});

const status = (over: Partial<MomentStatusDto> = {}): MomentStatusDto => ({
  photos: 3_000,
  groupable: 3_000,
  grouped: 3_000,
  coverage: 1,
  moments: 900,
  locked: 0,
  meanSize: 3.3,
  bursts: 1_100,
  duplicates: [0, 0, 0],
  medianIntervalMs: 900,
  implausible: false,
  embedVer: 100,
  groupVer: 1,
  profileVer: 1,
  ...over,
});

const set = (over: Partial<DuplicateSetDto> = {}): DuplicateSetDto => ({
  kind: 'near_identical',
  photoIds: [photo(0), photo(1)],
  keepHint: photo(0),
  confidence: 0.88,
  reasons: ['difference hash within 2 of 4 bits and appearance within 0.004'],
  userChosen: false,
  capsGallery: true,
  ...over,
});

describe('the count badge', () => {
  it('is the frame count, and a moment of one carries none', () => {
    expect(countBadge(moment({ frameCount: 14 }))).toBe('14');
    // A badge on every cell carries no information and turns a scannable grid into a
    // wall of numerals.
    expect(countBadge(moment({ frameCount: 1 }))).toBeNull();
  });
});

describe('the second badge', () => {
  it('shows the photographer first, then a consequence, then provenance', () => {
    expect(stackBadge(moment({ userLocked: true, duplicateSets: 2, cameraCount: 2 }))).toBe(
      'yours',
    );
    expect(stackBadge(moment({ duplicateSets: 2, cameraCount: 2 }))).toBe('2 duplicate sets');
    expect(stackBadge(moment({ duplicateSets: 1 }))).toBe('1 duplicate set');
    expect(stackBadge(moment({ cameraCount: 2 }))).toBe('2 cameras');
    expect(stackBadge(moment())).toBeNull();
  });
});

describe('the span label', () => {
  it('reads in the unit the moment actually happened in', () => {
    // A bouquet toss shown as "0 m" would be worse than useless.
    expect(spanLabel(moment({ startMs: 0, endMs: 400 }))).toBe('400 ms');
    expect(spanLabel(moment({ startMs: 0, endMs: 1_400 }))).toBe('1.4 s');
    expect(spanLabel(moment({ startMs: 0, endMs: 45_000 }))).toBe('45 s');
    expect(spanLabel(moment({ startMs: 0, endMs: 240_000 }))).toBe('4 m');
    expect(spanLabel(moment({ frameCount: 1 }))).toBe('single frame');
  });
});

describe('the burst sentence', () => {
  it('distinguishes a held shutter from three separate tries', () => {
    expect(burstSentence(moment({ frameCount: 15, burstCount: 1 }))).toContain(
      'One press of the shutter',
    );
    expect(burstSentence(moment({ frameCount: 15, burstCount: 3 }))).toContain(
      '3 separate takes',
    );
    expect(burstSentence(moment({ frameCount: 1, burstCount: 1 }))).toBeNull();
  });
});

describe('the frames-by-burst partition', () => {
  it('keeps every frame and orders the rows', () => {
    const stack = moment({
      frames: [frame(0, 1), frame(1, 0), frame(2, 1), frame(3, 0)],
      frameCount: 4,
      burstCount: 2,
    });
    const rows = framesByBurst(stack);
    expect(rows).toHaveLength(2);
    expect(rows.flat()).toHaveLength(4);
    // Burst 0 first, whatever order the frames arrived in.
    expect(rows[0]?.every((f) => f.burstIx === 0)).toBe(true);
  });

  it('does not drop a row when burst indices are sparse', () => {
    // The backend writes dense indices; assuming it would silently lose a frame if it
    // ever did not, and a photographer counting frames would find one missing.
    const stack = moment({
      frames: [frame(0, 0), frame(1, 7)],
      frameCount: 2,
      burstCount: 2,
    });
    expect(framesByBurst(stack).flat()).toHaveLength(2);
  });
});

describe('the keeper sentence', () => {
  it('says one for a bracketed subject and more for an evolving one', () => {
    expect(keeperSentence(moment({ suggestedKeepers: 1 }))).toContain('one of them is the keeper');
    expect(keeperSentence(moment({ suggestedKeepers: 3 }))).toContain('up to 3');
    expect(keeperSentence(moment({ frameCount: 1 }))).toBe('One frame.');
  });
});

describe('splitting and merging', () => {
  it('refuses a split before the first frame, which would empty the original', () => {
    const stack = moment();
    expect(canSplitAt(stack, photo(0))).toBe(false);
    expect(canSplitAt(stack, photo(1))).toBe(true);
    expect(canSplitAt(stack, 'pht_not-in-this-moment')).toBe(false);
    expect(canSplitAt(moment({ frameCount: 1, frames: [frame(0)] }), photo(0))).toBe(false);
  });

  it('merges within a chapter but not across one, and never with itself', () => {
    const a = moment();
    const b = moment({ momentId: 'mom_00000000-0000-7000-8000-000000000002' });
    expect(canMergeStacks(a, b)).toBe(true);
    expect(canMergeStacks(a, a)).toBe(false);
    expect(canMergeStacks(a, null)).toBe(false);
    expect(
      canMergeStacks(a, moment({ momentId: 'mom_x', segmentId: 'seg_other' })),
    ).toBe(false);
  });
});

describe('the coverage sentence', () => {
  it('measures against groupable frames, not against every photograph', () => {
    // An ungrouped frame with no embedding is a phase 05 gap, and reporting it as a
    // phase 08 failure would send somebody looking in the wrong place.
    const sentence = coverageSentence(
      status({ photos: 3_000, groupable: 1_200, grouped: 1_200, moments: 350, coverage: 1 }),
    );
    expect(sentence).toContain('1200 of 1200 analysed photographs');
    // And the project total is still named, so a photographer does not conclude that
    // 1,800 photographs have vanished.
    expect(sentence).toContain('3000 in the project');
    expect(sentence).toContain('350 moments');
  });

  it('explains every reason the view can be empty', () => {
    expect(coverageSentence(status({ photos: 0 }))).toContain('no photographs');
    expect(coverageSentence(status({ groupable: 0 }))).toContain('perceptual pass');
    expect(coverageSentence(status({ moments: 0 }))).toContain('Nothing has been grouped');
  });

  it('names the frames that could not be placed', () => {
    const sentence = coverageSentence(status({ groupable: 1_000, grouped: 940, moments: 300 }));
    expect(sentence).toContain('60 could not be placed');
  });
});

describe('the duplicate sentence', () => {
  it('says nothing was deleted, every time it says anything at all', () => {
    const sentence = duplicateSentence(status({ duplicates: [2, 9, 40] }));
    expect(sentence).toContain('2 sets of the same file');
    expect(sentence).toContain('9 sets');
    expect(sentence).toContain('Nothing has been deleted');
    // Variants are not counted: they are not stored and they constrain nothing.
    expect(sentence).not.toContain('40');
  });

  it('is silent when nothing repeats', () => {
    expect(duplicateSentence(status({ duplicates: [0, 0, 12] }))).toBeNull();
  });
});

describe('the implausible warning', () => {
  it('fires only when the pass said so, and says the grouping is still usable', () => {
    expect(implausibleSentence(status())).toBeNull();
    const warning = implausibleSentence(status({ implausible: true, meanSize: 24.5 }));
    expect(warning).toContain('24.5');
    expect(warning).toContain('still usable');
  });
});

describe('a suppressed frame', () => {
  it('is explained rather than rejected', () => {
    expect(suppressedLabel(frame(1, 0, true))).toBe('same photograph as the one kept');
    expect(suppressedLabel(frame(1))).toBeNull();
  });
});

describe('the duplicate panel', () => {
  it('names the class in words a photographer would use', () => {
    expect(setHeading(set({ kind: 'identical' }))).toContain('imported twice');
    expect(setHeading(set({ kind: 'near_identical' }))).toContain('same photograph');
    expect(setHeading(set({ kind: 'variant' }))).toContain('Alternatives');
  });

  it('states the delivery consequence and always says nothing was deleted', () => {
    const capping = setConsequence(set());
    expect(capping).toContain('Only one of these 2 frames');
    expect(capping).toContain('Nothing has been deleted');
    expect(setConsequence(set({ capsGallery: false, kind: 'variant' }))).toContain(
      'All of them stay eligible',
    );
  });

  it('promises that keeping one deletes nothing', () => {
    // Section 12's fourth failure mode is duplicate deletion anxiety, and it is
    // answered in the interface rather than in a runbook.
    expect(keepConsequence()).toContain('does not delete anything');
  });

  it('reads a confidence as a word', () => {
    expect(confidenceLabel(set({ confidence: 0.95 }))).toBe('certain');
    expect(confidenceLabel(set({ confidence: 0.75 }))).toBe('confident');
    expect(confidenceLabel(set({ confidence: 0.6 }))).toBe('worth a look');
    expect(confidenceLabel(set({ confidence: 0.4 }))).toBe('uncertain');
  });

  it('compares every frame against the kept one', () => {
    const many = set({ photoIds: [photo(0), photo(1), photo(2), photo(3)] });
    expect(comparisons(many)).toHaveLength(3);
    expect(comparisons(many)).not.toContain(many.keepHint);
  });

  it('says whose choice the hint was', () => {
    expect(hintSource(set({ userChosen: true }))).toContain('You chose');
    expect(hintSource(set())).toContain('AURA suggests');
  });

  it('reviews capping sets first, least confident first inside them', () => {
    const ordered = reviewOrder([
      set({ keepHint: photo(1), capsGallery: false, confidence: 0.2 }),
      set({ keepHint: photo(2), capsGallery: true, confidence: 0.9 }),
      set({ keepHint: photo(3), capsGallery: true, confidence: 0.6 }),
    ]);
    expect(ordered.map((entry) => entry.keepHint)).toEqual([photo(3), photo(2), photo(1)]);
  });
});

describe('the preview cap', () => {
  it('is a small number, because a stack is scanned rather than read', () => {
    expect(PREVIEW_FRAMES).toBeGreaterThan(1);
    expect(PREVIEW_FRAMES).toBeLessThanOrEqual(6);
  });
});
