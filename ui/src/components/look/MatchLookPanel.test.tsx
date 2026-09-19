import { describe, expect, it } from 'vitest';

import type { LookMatchDto, LookStatusDto, ReferenceOriginDto } from '../../ipc/types';
import {
  FETCH_UNAVAILABLE,
  SOURCE_CHOICES,
  coverageSentence,
  matchSentence,
  originSentence,
  percent,
  routeWorks,
  sourceLabel,
} from './MatchLookPanel';

function status(overrides: Partial<LookStatusDto> = {}): LookStatusDto {
  return {
    profiles: 0,
    selected: null,
    selectedName: '',
    selectedOrigin: '',
    strength: 1,
    appliable: 0,
    photographs: 0,
    baselineCoverage: 0,
    networkTransportAvailable: false,
    ...overrides,
  };
}

function origin(overrides: Partial<ReferenceOriginDto> = {}): ReferenceOriginDto {
  return {
    kind: 'instagram',
    title: '@somebody',
    key: 'instagram:somebody',
    understood: true,
    refusal: null,
    ...overrides,
  };
}

function matched(overrides: Partial<LookMatchDto> = {}): LookMatchDto {
  return {
    profile: 'prof_1',
    beforeDe00: 6,
    afterDe00: 2,
    realisedShare: 0.667,
    reached: true,
    frames: 60,
    userEdited: 0,
    buckets: [],
    reasons: [],
    ...overrides,
  };
}

describe('MatchLookPanel', () => {
  it('names every route in a photographer’s words', () => {
    for (const choice of SOURCE_CHOICES) {
      expect(sourceLabel(choice)).not.toBe(choice);
    }
  });

  it('shows an unknown route verbatim rather than swallowing it', () => {
    expect(sourceLabel('carrier_pigeon')).toBe('carrier_pigeon');
  });

  // The whole point of the panel's honesty: the route that cannot work is offered, disabled, and
  // explained. Omitting it would leave a photographer looking for a setting they had missed.
  it('offers the fetch route and marks it as one that does not work', () => {
    expect(SOURCE_CHOICES).toContain('public_url');
    expect(routeWorks('public_url')).toBe(false);
    expect(routeWorks('folder')).toBe(true);
    expect(routeWorks('instagram_export')).toBe(true);
  });

  it('says what to do instead of downloading, in the same sentence as the refusal', () => {
    expect(FETCH_UNAVAILABLE).toContain('cannot download');
    expect(FETCH_UNAVAILABLE).toContain('folder');
    // The link is not wasted and the panel says so, because a photographer who pasted one
    // should not think it was ignored.
    expect(FETCH_UNAVAILABLE).toContain('still recorded');
  });

  it('never promises the page was checked', () => {
    // Nothing is resolved, so nothing is verified. ADR-0035 decision 8's rule, in the phase
    // where a tick beside a pasted handle would be the easiest thing in the world to add.
    const sentence = originSentence(origin());
    expect(sentence).toContain('@somebody');
    expect(sentence.toLowerCase()).not.toContain('verified');
    expect(sentence.toLowerCase()).not.toContain('found');
    expect(sentence.toLowerCase()).not.toContain('exists');
  });

  it('shows the refusal when an address will not parse', () => {
    expect(
      originSentence(origin({ understood: false, refusal: 'that looks like a file path' })),
    ).toBe('that looks like a file path');
  });

  it('says nothing at all when the box is empty', () => {
    expect(originSentence(null)).toBe('');
    expect(originSentence(origin({ title: '' }))).toBe('');
  });

  // Phase 18's rule about denominators, in the panel that would most easily have hidden one.
  it('says how much of the wedding a look can reach, with both numbers', () => {
    const sentence = coverageSentence(status({ photographs: 900, appliable: 360 }));
    expect(sentence).toContain('360');
    expect(sentence).toContain('900');
  });

  it('tells a photographer what to do when nothing has been analysed yet', () => {
    const sentence = coverageSentence(status({ photographs: 900, appliable: 0 }));
    expect(sentence).toContain('has been analysed');
    expect(sentence).toContain('Autopilot');
  });

  it('asks for photographs before it asks for anything else', () => {
    expect(coverageSentence(status())).toContain('Import some photographs');
    expect(coverageSentence(null)).toContain('Import some photographs');
  });

  // Phase 27's rule: measured against what the gap was, never against the ceiling.
  it('leads with how much of the gap closed rather than with a threshold', () => {
    const sentence = matchSentence(matched());
    expect(sentence).toContain('67%');
    expect(sentence).toContain('60');
  });

  it('says a hand-edited frame was left alone', () => {
    expect(matchSentence(matched({ userEdited: 4 }))).toContain('4');
    expect(matchSentence(matched({ userEdited: 4 }))).toContain('exactly as you made them');
  });

  it('does not report a failure when there was nothing to close', () => {
    // A gallery that already matched its reference closed no gap and that is not a bad result.
    // Phase 29's lesson about a gate nothing can meet, as a sentence rather than a number.
    expect(matchSentence(matched({ beforeDe00: 0, afterDe00: 0, realisedShare: 1 }))).toContain(
      'already sat where',
    );
  });

  it('says a look has not been measured rather than showing a zero', () => {
    expect(matchSentence(null)).toContain('not measured');
    expect(matchSentence(matched({ frames: 0 }))).toContain('no analysed photographs');
  });

  it('never shows a strength above what the reference asked for', () => {
    expect(percent(1)).toBe('100%');
    expect(percent(1.8)).toBe('100%');
    expect(percent(-0.2)).toBe('0%');
    expect(percent(0.355)).toBe('36%');
  });
});
