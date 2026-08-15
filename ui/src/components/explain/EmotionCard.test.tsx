import { describe, expect, it } from 'vitest';

import type {
  EmotionDto,
  EmotionReasonDto,
  FaceExpressionDto,
  InteractionDto,
} from '../../ipc/types';
import {
  confidenceSentence,
  cropStyle,
  gapSentence,
  headline,
  notScoredSentence,
  percent,
  scoreLabel,
} from './EmotionCard';
import { momentSubtitle, orderForBrowser, peakSentence } from './MomentBrowser';

const photo = 'pht_00000000-0000-7000-8000-000000000001';

const face = (over: Partial<FaceExpressionDto> = {}): FaceExpressionDto => ({
  faceId: 'fac_00000000-0000-8000-8000-000000000001',
  identityId: null,
  // smile, genuineness, laughter, tears, surprise, tenderness, composed, discomfort
  channels: [0.85, 0.9, 0.92, 0.02, 0.2, 0.3, 0.05, 0.02],
  gaze: 'camera',
  confidence: 0.8,
  readsAsCrying: false,
  posedSmile: false,
  crop: { x: 0.3, y: 0.3, w: 0.2, h: 0.2 },
  ...over,
});

const reason = (over: Partial<EmotionReasonDto> = {}): EmotionReasonDto => ({
  code: 'laughter',
  text: 'somebody is laughing here',
  weight: 0.4,
  caveat: false,
  evidence: null,
  ...over,
});

const interaction = (over: Partial<InteractionDto> = {}): InteractionDto => ({
  kind: 'hug',
  title: 'Hug',
  strength: 0.7,
  milestone: false,
  ...over,
});

const reading = (over: Partial<EmotionDto> = {}): EmotionDto => ({
  photoId: photo,
  faces: [face()],
  channelNames: [
    'smile',
    'genuineness',
    'laughter',
    'tears',
    'surprise',
    'tenderness',
    'composed',
    'discomfort',
  ],
  interactions: [interaction()],
  mutualGaze: false,
  peakProximity: 1.0,
  reactionOf: null,
  emotionScore: 0.71,
  narrativeWeight: 0.0,
  scene: 'candid',
  reasons: [reason()],
  confidence: 0.8,
  source: 'local',
  modelVer: 100,
  analysisVer: 1,
  weightsVer: 1,
  ...over,
});

describe('the score label', () => {
  it('names four bands and never says "keep" or "reject"', () => {
    const labels = [0.9, 0.6, 0.3, 0.05].map(scoreLabel);
    expect(labels).toEqual(['a strong moment', 'worth a look', 'quiet', 'nothing in particular']);
    // Section 2.2 puts the choosing in phase 12. A label that said "deliver" would be
    // this card starting early, which is the one thing its header forbids.
    for (const label of labels) {
      expect(label).not.toMatch(/keep|reject|deliver|cull/i);
    }
  });
});

describe('the headline', () => {
  it('says what was found before it says what it scored', () => {
    expect(headline(reading())).toBe(
      'AURA reads this as worth a look, mostly because somebody is laughing here.',
    );
  });

  it('falls back to the label alone when nothing was credited', () => {
    const quiet = reading({
      emotionScore: 0.1,
      reasons: [reason({ code: 'unremarkable', text: 'nothing here stands out', weight: 0 })],
    });
    expect(headline(quiet)).toBe('AURA reads this as nothing in particular.');
  });

  it('never leads with a caveat', () => {
    // A caveat says how much to trust the number, not what is in the photograph. Leading
    // with "no faces were found" would be a headline about the pipeline.
    const caveated = reading({
      reasons: [
        reason({ code: 'no_faces', text: 'no face was found here', weight: 0, caveat: true }),
        reason({ code: 'laughter', text: 'somebody is laughing here', weight: 0.4 }),
      ],
    });
    expect(headline(caveated)).toContain('laughing');
  });
});

describe('an unscored photograph', () => {
  it('is described as unread rather than as flat', () => {
    // Migration 10's sixth property, carried out to the interface: "not scored" is not
    // "no feeling". A card that drew a zero would tell a photographer AURA looked.
    const sentence = notScoredSentence();
    expect(sentence).toContain('has not read');
    expect(sentence).toContain('not the same as');
  });
});

describe('the confidence sentence', () => {
  it('degrades in three steps and names the reasons at the bottom', () => {
    expect(confidenceSentence(reading({ confidence: 0.9 }))).toContain('confident');
    expect(confidenceSentence(reading({ confidence: 0.5 }))).toContain('moderately');
    expect(confidenceSentence(reading({ confidence: 0.2 }))).toContain('what was missing');
  });
});

describe('the crop style', () => {
  it('is expressed in percentages so one rectangle fits every preview size', () => {
    // The same argument phase 09's card makes: percentages address a 240 px thumbnail,
    // a 2048 px proxy and the original without the card knowing which is on screen.
    expect(cropStyle({ x: 0.25, y: 0.5, w: 0.1, h: 0.2 })).toEqual({
      left: '25%',
      top: '50%',
      width: '10%',
      height: '20%',
    });
  });
});

describe('the percentage', () => {
  it('rounds to whole numbers and clamps', () => {
    expect(percent(0.714)).toBe('71%');
    expect(percent(1.4)).toBe('100%');
    expect(percent(-0.2)).toBe('0%');
  });
});

describe('the reaction gap', () => {
  it('keeps the sign, because a reaction before its action is a real case', () => {
    // Two cameras have two clocks and phase 01's alignment is good to about a second.
    // Rounding the sign away would hide a clock-skew problem.
    expect(gapSentence(2400)).toBe('2s after');
    expect(gapSentence(-2400)).toBe('2s before');
    expect(gapSentence(400)).toBe('0.4s after');
  });
});

describe('the moment browser', () => {
  it('orders by emotion score, strongest first, with a stable tie-break', () => {
    const ordered = orderForBrowser([
      { photoId: 'b', emotionScore: 0.5 },
      { photoId: 'a', emotionScore: 0.5 },
      { photoId: 'c', emotionScore: 0.9 },
    ]);
    expect(ordered.map((row) => row.photoId)).toEqual(['c', 'a', 'b']);
  });

  it('describes an unresolved peak as an absence rather than as a choice', () => {
    // A bracketed detail shot genuinely has no apex, and pointing at a rounding error is
    // how phase 29 ends up building an album spread around one.
    expect(peakSentence({ resolved: false, margin: 0.01, frames: 14 })).toContain(
      'no single frame',
    );
    expect(peakSentence({ resolved: true, margin: 0.32, frames: 6 })).toContain('strongest');
  });

  it('says how many frames a moment holds before it says which one won', () => {
    expect(momentSubtitle(6, 0.71)).toBe('6 frames · strongest reads 71%');
  });
});
