import { describe, expect, it } from 'vitest';

import type { EyeStateDto, IntegrityDto, IntegrityStatusDto } from '../../ipc/types';
import {
  chipCount,
  chipLabel,
  chipsFor,
  coverageSentence,
  isDefectChip,
  queueSentence,
  uncalibratedSentence,
  DEFAULT_CHIPS,
} from './FilterChips';
import {
  confidenceSentence,
  cropStyle,
  dismissConsequence,
  dismissible,
  exposureSentence,
  eyeLabel,
  eyeSentence,
  headline,
  listedEyes,
  noiseSentence,
  notCheckedSentence,
  orderedReasons,
  scoreLabel,
} from './IntegrityCard';

const photo = 'pht_00000000-0000-7000-8000-000000000001';

const eye = (over: Partial<EyeStateDto> = {}): EyeStateDto => ({
  faceId: 'fac_00000000-0000-8000-8000-000000000001',
  identityId: null,
  state: 'open',
  confidence: 0.9,
  intentional: false,
  gates: true,
  areaFrac: 0.08,
  crop: { x: 0.3, y: 0.3, w: 0.2, h: 0.2 },
  ...over,
});

const verdict = (over: Partial<IntegrityDto> = {}): IntegrityDto => ({
  photoId: photo,
  subjectSharpness: 0.82,
  bgSharpness: 0.3,
  focusOffset: 0,
  relativeSharpness: 0.5,
  motion: 'none',
  motionSeverity: 0,
  exposure: 'good',
  clipHi: 0,
  clipLo: 0,
  evOffset: 0.05,
  noiseSigmaRel: 0.6,
  closedEyeRatio: 0,
  gatingFaces: 1,
  technicalScore: 0.91,
  scene: 'couple_portrait',
  flags: [],
  hasDefect: false,
  reasons: [
    {
      code: 'clean',
      text: 'focus, motion, exposure, noise and eyes all read normally here',
      weight: 0,
      exoneration: true,
      evidence: null,
    },
  ],
  eyes: [eye()],
  confidence: 0.95,
  userReviewed: false,
  modelVer: 100,
  analysisVer: 1,
  calibVer: 1,
  ...over,
});

const status = (over: Partial<IntegrityStatusDto> = {}): IntegrityStatusDto => ({
  photos: 100,
  scored: 100,
  coverage: 1,
  subjectAware: 0.9,
  reviewed: 0,
  flagCounts: [4, 0, 0, 2, 0, 0, 3, 0, 1, 5, 7, 0, 6, 0],
  flagNames: [
    'subject_soft',
    'camera_shake',
    'subject_motion',
    'intentional_motion',
    'back_focus',
    'front_focus',
    'highlight_lost',
    'shadow_lost',
    'heavy_noise',
    'eyes_closed',
    'eyes_closed_ok',
    'squint',
    'no_subject_detected',
    'mixed_light_risk',
  ],
  meanScore: 0.78,
  defectiveAtMost: 20,
  uncalibrated: [],
  modelVer: 100,
  analysisVer: 1,
  calibVer: 1,
  ...over,
});

describe('a photograph nobody has checked', () => {
  it('is never described as clean', () => {
    // Migration 9's fifth property, carried out to the interface: the absence of a
    // verdict means nobody looked, and phase 12 reads "nothing wrong" as evidence.
    expect(notCheckedSentence()).toContain('not the same as finding nothing wrong');
  });
});

describe('the headline', () => {
  it('names the strongest penalty rather than only a number', () => {
    const soft = verdict({
      technicalScore: 0.31,
      reasons: [
        {
          code: 'subject_soft',
          text: 'the subject is softer than this camera should manage',
          weight: -0.45,
          exoneration: false,
          evidence: { x: 0.3, y: 0.3, w: 0.2, h: 0.2 },
        },
      ],
    });
    expect(headline(soft)).toContain('softer than this camera should manage');
    expect(headline(soft)).toContain('badly affected');
  });

  it('says so plainly when nothing counted against the frame', () => {
    expect(headline(verdict())).toContain('Nothing counted against it');
  });

  it('reads the score in words a photographer would use', () => {
    expect(scoreLabel(0.9)).toBe('technically clean');
    expect(scoreLabel(0.7)).toBe('usable');
    expect(scoreLabel(0.5)).toBe('has a problem');
    expect(scoreLabel(0.2)).toBe('badly affected');
  });
});

describe('the confidence sentence', () => {
  it('says why the confidence is low, not only that it is', () => {
    const weak = verdict({
      confidence: 0.58,
      scene: 'unknown',
      flags: ['no_subject_detected'],
      reasons: [
        {
          code: 'uncalibrated',
          text: 'AURA has not measured this camera model yet',
          weight: 0,
          exoneration: true,
          evidence: null,
        },
      ],
    });
    const sentence = confidenceSentence(weak);
    expect(sentence).toContain('no face or person was found');
    expect(sentence).toContain('has not been measured yet');
    expect(sentence).toContain('no scene label');
  });

  it('says nothing extra when every input was present', () => {
    expect(confidenceSentence(verdict())).toBe('Confidence 0.95.');
  });
});

describe('the measurements in words', () => {
  it('distinguishes the four exposure verdicts', () => {
    expect(exposureSentence(verdict({ exposure: 'good' }))).toContain('needs nothing');
    expect(exposureSentence(verdict({ exposure: 'recoverable', evOffset: -1.8 }))).toContain(
      'can be brought back',
    );
    expect(exposureSentence(verdict({ exposure: 'marginal', evOffset: -2.4 }))).toContain(
      'cost some quality',
    );
    expect(exposureSentence(verdict({ exposure: 'lost', evOffset: 2.1 }))).toContain(
      'not in the file to recover',
    );
  });

  it('quotes the stops only when there are stops worth quoting', () => {
    expect(exposureSentence(verdict({ evOffset: 0.05 }))).not.toContain('stops');
    expect(exposureSentence(verdict({ evOffset: -1.8 }))).toContain('1.8 stops under');
  });

  it('expresses noise against the scene, because that is what the number is', () => {
    expect(noiseSentence(verdict({ noiseSigmaRel: 0.6 }))).toContain('60%');
    expect(noiseSentence(verdict({ noiseSigmaRel: 1.7 }))).toContain('more than usual');
    expect(noiseSentence(verdict({ noiseSigmaRel: 0 }))).toContain('no flat area');
  });
});

describe('the eye sentence', () => {
  it('never confuses "nobody blinked" with "nobody was judged"', () => {
    expect(eyeSentence(verdict({ gatingFaces: 0, eyes: [] }))).toContain(
      'prominent enough for their eyes to count',
    );
    expect(eyeSentence(verdict({ gatingFaces: 3, eyes: [eye(), eye(), eye()] }))).toContain(
      'Eyes open on all 3',
    );
  });

  it('says a justified closure is justified', () => {
    const kiss = verdict({
      gatingFaces: 2,
      eyes: [eye({ state: 'closed', intentional: true }), eye({ state: 'closed', intentional: true })],
    });
    expect(eyeSentence(kiss)).toContain('this moment explains it');
  });

  it('counts only the unjustified closures as a problem', () => {
    const mixed = verdict({
      gatingFaces: 2,
      eyes: [eye({ state: 'closed', intentional: true }), eye({ state: 'closed' })],
    });
    expect(eyeSentence(mixed)).toContain('1 of 2');
    expect(eyeSentence(mixed)).toContain('nothing to explain it');
  });

  it('labels the five states in a photographer\'s words', () => {
    expect(eyeLabel(eye({ state: 'closed' }))).toBe('eyes closed');
    expect(eyeLabel(eye({ state: 'closed', intentional: true }))).toContain('part of the moment');
    expect(eyeLabel(eye({ state: 'looking_down' }))).toBe('looking down');
    expect(eyeLabel(eye({ state: 'occluded' }))).toBe('eyes hidden');
    expect(eyeLabel(eye({ state: 'open' }))).toBe('eyes open');
  });
});

describe('the reason list', () => {
  it('puts the penalties first and the good news after', () => {
    const mixed = verdict({
      reasons: [
        { code: 'shallow_depth_of_field', text: 'deliberate', weight: 0, exoneration: true, evidence: null },
        { code: 'subject_soft', text: 'soft', weight: -0.4, exoneration: false, evidence: null },
        { code: 'eyes_closed_ok', text: 'a kiss', weight: 0, exoneration: true, evidence: null },
      ],
    });
    expect(orderedReasons(mixed).map((reason) => reason.code)).toEqual([
      'subject_soft',
      'shallow_depth_of_field',
      'eyes_closed_ok',
    ]);
  });
});

describe('the evidence crop', () => {
  it('positions in percentages so one rectangle fits every preview size', () => {
    expect(cropStyle({ x: 0.25, y: 0.5, w: 0.1, h: 0.2 })).toEqual({
      left: '25.00%',
      top: '50.00%',
      width: '10.00%',
      height: '20.00%',
    });
  });
});

describe('the faces the card lists', () => {
  it('puts the gating faces first and caps the list', () => {
    const crowd = verdict({
      eyes: [
        eye({ faceId: 'a', gates: false, areaFrac: 0.4 }),
        eye({ faceId: 'b', gates: true, areaFrac: 0.05 }),
        ...Array.from({ length: 8 }, (_, index) =>
          eye({ faceId: `guest-${index}`, gates: false, areaFrac: 0.01 }),
        ),
      ],
    });
    const listed = listedEyes(crowd);
    expect(listed[0]?.faceId).toBe('b');
    expect(listed).toHaveLength(6);
  });
});

describe('dismissing a mark', () => {
  it('is offered for faults and refused for the three that are not', () => {
    expect(dismissible('subject_soft')).toBe(true);
    expect(dismissible('heavy_noise')).toBe(true);
    expect(dismissible('intentional_motion')).toBe(false);
    expect(dismissible('eyes_closed_ok')).toBe(false);
    expect(dismissible('no_subject_detected')).toBe(false);
  });

  it('promises only what it does', () => {
    // Section 2.2 kept in the interface: this panel cannot change a delivery.
    expect(dismissConsequence()).toContain('does not change whether the photograph is delivered');
  });
});

describe('the filter chips', () => {
  it('offers section 9\'s four by default', () => {
    expect(DEFAULT_CHIPS).toEqual(['subject_soft', 'eyes_closed', 'highlight_lost', 'heavy_noise']);
  });

  it('reads the flag names from the backend rather than hard-coding them', () => {
    const chips = chipsFor(status());
    expect(chips).toContain('intentional_motion');
    expect(chips).toContain('eyes_closed_ok');
    // A flag with no frames is not offered.
    expect(chips).not.toContain('camera_shake');
  });

  it('counts by name, not by position', () => {
    expect(chipCount(status(), 'eyes_closed')).toBe(5);
    expect(chipCount(status(), 'eyes_closed_ok')).toBe(7);
    expect(chipCount(status(), 'nonexistent')).toBe(0);
  });

  it('draws the three that are not faults differently', () => {
    expect(isDefectChip('subject_soft')).toBe(true);
    expect(isDefectChip('intentional_motion')).toBe(false);
    expect(isDefectChip('eyes_closed_ok')).toBe(false);
  });

  it('names both denominators, and warns when the subject-aware one is low', () => {
    expect(coverageSentence(status())).toContain('100 of 100');
    const global = coverageSentence(status({ subjectAware: 0.02 }));
    expect(global).toContain('2%');
    expect(global).toContain('measured on the whole frame');
  });

  it('says the queue count is an upper bound', () => {
    expect(queueSentence(status())).toContain('At most 20');
    expect(queueSentence(status({ defectiveAtMost: 0 }))).toBe('Nothing was marked.');
  });

  it('names an unmeasured camera body', () => {
    expect(uncalibratedSentence(status())).toBeNull();
    const note = uncalibratedSentence(status({ uncalibrated: ['Hasselblad X2D 100C'] }));
    expect(note).toContain('Hasselblad X2D 100C');
    expect(note).toContain('more cautiously');
  });

  it('labels every flag it can be given', () => {
    for (const flag of status().flagNames) {
      expect(chipLabel(flag).length).toBeGreaterThan(0);
      expect(chipLabel(flag)).not.toContain('_');
    }
  });
});
