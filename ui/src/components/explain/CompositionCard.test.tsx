import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { CompositionDto, CompositionReasonDto } from '../../ipc/types';
import { useThumbnails } from '../../stores/thumbnailStore';
import {
  aestheticSentence,
  backgroundSentence,
  boxStyle,
  cancelledSentence,
  CompositionCard,
  confidenceSentence,
  cropHintSentence,
  dismissible,
  headline,
  horizonSentence,
  jointCutSentence,
  notAnalysedSentence,
  orderedReasons,
  overlayDescription,
  percent,
  placementSentence,
  scoreLabel,
} from './CompositionCard';

afterEach(() => {
  cleanup();
  useThumbnails.getState().clear();
});

const photoA = 'pht_00000000-0000-7000-8000-000000000001';
const photoB = 'pht_00000000-0000-7000-8000-000000000002';

const reason = (over: Partial<CompositionReasonDto> = {}): CompositionReasonDto => ({
  code: 'joint_cut',
  text: 'the bottom edge cuts through a knee, which reads as a severed limb',
  weight: -0.42,
  exoneration: false,
  evidence: { x: 0.41, y: 0.77, w: 0.18, h: 0.14 },
  ...over,
});

const result = (over: Partial<CompositionDto> = {}): CompositionDto => ({
  photoId: photoA,
  tiltDeg: 2.4,
  tiltIntentional: false,
  horizonConf: 0.88,
  horizonSource: 'gradient',
  headroom: 0.09,
  thirdsOffset: 0.08,
  balance: 0.77,
  negativeSpace: 0.31,
  jointCuts: [
    {
      joint: 'knee',
      edge: 'bottom',
      atJoint: true,
      severity: 0.74,
      flagged: true,
      area: { x: 0.41, y: 0.77, w: 0.18, h: 0.14 },
    },
  ],
  headCrop: false,
  edgeIntrusions: [{ x: 0, y: 0.48, w: 0.08, h: 0.31 }],
  clutter: 1.25,
  brightBlobs: [{ x: 0.72, y: 0.12, w: 0.11, h: 0.13 }],
  headMerge: false,
  colourCompetition: 0.23,
  aesthetic: 0.68,
  compositionScore: 0.58,
  cropSuggestionHint: {
    region: { x: 0.18, y: 0.08, w: 0.67, h: 0.84 },
    safeMargin: 0.08,
    straightenDeg: -2.4,
    confidence: 0.84,
    actionable: true,
  },
  scene: 'couple_portrait',
  relativeComposition: 0.61,
  keypointSubjects: 2,
  flags: ['horizon_tilted', 'joint_cut', 'centred'],
  hasViolation: true,
  reasons: [
    reason(),
    reason({
      code: 'horizon_tilted',
      text: 'the horizon is off level by more than this portrait usually carries',
      weight: -0.2,
      evidence: null,
    }),
    reason({
      code: 'centred',
      text: 'the subject is centred, which is right for this symmetrical portrait',
      weight: 0,
      exoneration: true,
      evidence: null,
    }),
  ],
  confidence: 0.9,
  userReviewed: false,
  modelVer: 100,
  analysisVer: 1,
  rulesVer: 1,
  ...over,
});

type Deferred<T> = {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (reason: unknown) => void;
};

function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((accept, refuse) => {
    resolve = accept;
    reject = refuse;
  });
  return { promise, resolve, reject };
}

describe('score and exact explanations', () => {
  it('uses editorial bands without making a keep or reject decision', () => {
    const labels = [0.9, 0.7, 0.5, 0.2].map(scoreLabel);
    expect(labels).toEqual([
      'cleanly composed',
      'compositionally sound',
      'compositionally unsettled',
      'strongly compromised',
    ]);
    for (const label of labels) {
      expect(label).not.toMatch(/keep|reject|deliver|cull/i);
    }
  });

  it('leads with the strongest backend sentence and renders every sentence verbatim', () => {
    const composition = result();
    expect(headline(composition)).toContain(composition.reasons[0]?.text);

    render(<CompositionCard photoId={photoA} result={composition} />);
    const explanations = screen.getByRole('region', {
      name: 'exact composition explanations',
    });
    for (const item of composition.reasons) {
      expect(within(explanations).getByText(item.text)).toBeTruthy();
    }
  });

  it('puts penalties first and labels exonerations as no-penalty context', () => {
    const composition = result({
      reasons: [
        reason({ code: 'centred', weight: 0, exoneration: true, text: 'centred on purpose' }),
        reason({ code: 'joint_cut', weight: -0.4, text: 'cut at a knee' }),
      ],
    });
    expect(orderedReasons(composition).map((item) => item.code)).toEqual([
      'joint_cut',
      'centred',
    ]);
    render(<CompositionCard photoId={photoA} result={composition} />);
    expect(screen.getByText(/Context — no penalty:/)).toBeTruthy();
    expect(screen.getByText(/Counts against the score:/)).toBeTruthy();
  });
});

describe('the accessible overlay', () => {
  it('draws the guides over the focused photograph at its real aspect ratio', () => {
    useThumbnails.getState().put(photoA, {
      photoId: photoA,
      dataUrl: 'data:image/png;base64,AA==',
      tier: 1,
      width: 3,
      height: 2,
      source: 'embedded',
    });

    const { container } = render(<CompositionCard photoId={photoA} result={result()} />);
    const preview = container.querySelector<HTMLImageElement>('.composition-card__preview');
    const canvas = container.querySelector<HTMLElement>('.composition-card__canvas');
    expect(preview?.src).toContain('data:image/png;base64,AA==');
    expect(canvas?.style.aspectRatio).toBe('3 / 2');
    expect(container.querySelector('[data-testid="composition-overlay"]')).toBeTruthy();
  });

  it('draws thirds, a measured horizon, all evidence classes and the crop hint', () => {
    const composition = result();
    const { container } = render(<CompositionCard photoId={photoA} result={composition} />);

    expect(
      screen.getByRole('img', { name: /Composition guides and evidence/i }),
    ).toBeTruthy();
    expect(container.querySelectorAll('[data-overlay-kind="thirds"] line')).toHaveLength(4);
    expect(container.querySelector('[data-overlay-kind="horizon"] line')?.getAttribute('transform')).toBe(
      'rotate(2.4 50 50)',
    );
    expect(container.querySelectorAll('[data-overlay-kind="reason-evidence"] rect')).toHaveLength(
      1,
    );
    expect(container.querySelectorAll('[data-overlay-kind="joint-cuts"] rect')).toHaveLength(1);
    expect(container.querySelectorAll('[data-overlay-kind="edge-intrusions"] rect')).toHaveLength(
      1,
    );
    expect(container.querySelectorAll('[data-overlay-kind="bright-blobs"] rect')).toHaveLength(1);
    expect(container.querySelector('[data-overlay-kind="crop-hint"] rect')).toBeTruthy();
  });

  it('does not rely on colour: every stroke pattern has a written key and violations have words', () => {
    render(<CompositionCard photoId={photoA} result={result()} />);
    const legend = screen.getByRole('list', { name: 'Overlay key' });
    expect(within(legend).getByText(/rule-of-thirds guides/)).toBeTruthy();
    expect(within(legend).getByText(/measured horizon/)).toBeTruthy();
    expect(within(legend).getByText(/reason evidence/)).toBeTruthy();
    expect(within(legend).getByText(/body and edge violations/)).toBeTruthy();
    expect(within(legend).getByText(/crop hint, not an applied crop/)).toBeTruthy();
    expect(screen.getByRole('region', { name: 'body crop audit' }).textContent).toContain(
      'Violation: The bottom edge cuts at the knee',
    );
    expect(screen.getByRole('region', { name: 'composition marks' }).textContent).toContain(
      'Context, not a fault: centred',
    );
  });

  it('announces an absent horizon rather than drawing a level line', () => {
    const noHorizon = result({
      horizonSource: 'none',
      horizonConf: 0,
      reasons: [
        reason({
          code: 'horizon_uncertain',
          text: 'there is no clear horizon here',
          weight: 0,
          exoneration: true,
          evidence: null,
        }),
      ],
    });
    const { container } = render(<CompositionCard photoId={photoA} result={noHorizon} />);
    expect(container.querySelector('[data-overlay-kind="horizon"]')).toBeNull();
    expect(screen.getByText(/No reliable horizon was found/)).toBeTruthy();
    expect(overlayDescription(noHorizon)).toContain('No measured horizon');
  });

  it('positions normalised boxes as percentages', () => {
    expect(boxStyle({ x: 0.25, y: 0.5, w: 0.1, h: 0.2 })).toEqual({
      left: '25.00%',
      top: '50.00%',
      width: '10.00%',
      height: '20.00%',
    });
  });
});

describe('measurements in photographer-facing words', () => {
  it('states direction, intent and confidence for the horizon', () => {
    expect(horizonSentence(result())).toContain('2.4 degrees clockwise');
    expect(horizonSentence(result())).toContain('88% confidence');
    expect(horizonSentence(result({ tiltDeg: -8, tiltIntentional: true }))).toContain(
      'counter-clockwise',
    );
    expect(horizonSentence(result({ tiltDeg: -8, tiltIntentional: true }))).toContain(
      'deliberate',
    );
  });

  it('uses the scene-relative clutter scale and real thirds metric', () => {
    expect(backgroundSentence(result())).toContain("125% of this scene's tolerance");
    expect(backgroundSentence(result())).toContain('above the limit');
    expect(placementSentence(result())).toContain('8%');
    expect(placementSentence(result())).toContain('balance is 77%');
  });

  it('calls the crop a hint and says it was not applied', () => {
    const sentence = cropHintSentence(result());
    expect(sentence).toContain('crop hint');
    expect(sentence).toContain('Nothing has been applied');
    expect(cropHintSentence(result({ cropSuggestionHint: null }))).toContain('no safe crop');
  });

  it('shows the backend crop threshold decision instead of recreating it', () => {
    const cut = result().jointCuts[0];
    expect(cut).toBeDefined();
    if (cut) {
      expect(jointCutSentence(cut)).toContain('severity 74%');
      expect(jointCutSentence(cut)).toContain('flagged as a violation');
      expect(jointCutSentence({ ...cut, flagged: false })).toContain('below the flag threshold');
    }
  });

  it('clamps percentages used for presentation', () => {
    expect(percent(0.714)).toBe('71%');
    expect(percent(1.4)).toBe('100%');
    expect(percent(-0.2)).toBe('0%');
  });
});

describe('loading, empty, error and cancellation states', () => {
  it('announces loading while the read is pending', () => {
    const pending = deferred<CompositionDto | null>();
    render(
      <CompositionCard photoId={photoA} fetchComposition={() => pending.promise} />,
    );
    expect(screen.getByRole('status').textContent).toContain('Reading this photograph');
    expect(screen.getByLabelText('composition').getAttribute('aria-busy')).toBe('true');
  });

  it('never describes an unanalysed frame as clean', () => {
    render(<CompositionCard photoId={photoA} result={null} />);
    expect(screen.getByText(notAnalysedSentence())).toBeTruthy();
    expect(notAnalysedSentence()).toContain('not the same as finding');
  });

  it('renders typed failures as alerts', async () => {
    render(
      <CompositionCard
        photoId={photoA}
        fetchComposition={async () =>
          Promise.reject({
            code: 'AURA-DB-3006',
            message: 'The composition row could not be read.',
            runbookUrl: 'https://aura.app/e/AURA-DB-3006',
            retryable: true,
          })
        }
      />,
    );
    expect((await screen.findByRole('alert')).textContent).toContain(
      'The composition row could not be read.',
    );
  });

  it('renders cancellation as a neutral stopped state, not an error', async () => {
    render(
      <CompositionCard
        photoId={photoA}
        fetchComposition={async () =>
          Promise.reject({
            code: 'AURA-ML-5011',
            message: 'The AI step was stopped.',
            runbookUrl: 'https://aura.app/e/AURA-ML-5011',
            retryable: false,
          })
        }
      />,
    );
    expect(await screen.findByText(cancelledSentence())).toBeTruthy();
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('ignores an old request after the selected photograph changes', async () => {
    const first = deferred<CompositionDto | null>();
    const second = deferred<CompositionDto | null>();
    const fetchComposition = vi.fn((photoId: string) =>
      photoId === photoA ? first.promise : second.promise,
    );
    const view = render(
      <CompositionCard photoId={photoA} fetchComposition={fetchComposition} />,
    );
    expect(fetchComposition).toHaveBeenCalledWith(photoA);

    view.rerender(
      <CompositionCard photoId={photoB} fetchComposition={fetchComposition} />,
    );
    expect(fetchComposition).toHaveBeenCalledWith(photoB);

    await act(async () => {
      second.resolve(result({ photoId: photoB }));
      await second.promise;
    });
    await waitFor(() => {
      expect(screen.getByLabelText('composition').getAttribute('data-photo-id')).toBe(photoB);
    });

    await act(async () => {
      first.resolve(result({ photoId: photoA }));
      await first.promise;
    });
    expect(screen.getByLabelText('composition').getAttribute('data-photo-id')).toBe(photoB);
  });
});

describe('dismissing a mark', () => {
  it('offers only violation flags and states the limited consequence', () => {
    expect(dismissible('joint_cut')).toBe(true);
    expect(dismissible('horizon_tilted')).toBe(true);
    expect(dismissible('intentional_tilt')).toBe(false);
    expect(dismissible('centred')).toBe(false);
    expect(dismissible('no_geometry')).toBe(false);

    render(<CompositionCard photoId={photoA} result={result()} />);
    expect(screen.getByText(/does not crop, straighten, keep or reject/i)).toBeTruthy();
    expect(screen.queryByRole('button', { name: /centred.*is wrong/i })).toBeNull();
  });

  it('replaces the card with the backend result after a dismissal', async () => {
    const dismissCompositionFlag = vi.fn(async () =>
      result({
        flags: ['horizon_tilted', 'centred'],
        userReviewed: true,
      }),
    );
    render(
      <CompositionCard
        photoId={photoA}
        result={result()}
        dismissCompositionFlag={dismissCompositionFlag}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: /joint cut.*is wrong/i }));
    await waitFor(() => {
      expect(dismissCompositionFlag).toHaveBeenCalledWith({ photoId: photoA, flag: 'joint_cut' });
      expect(screen.queryByRole('button', { name: /joint cut.*is wrong/i })).toBeNull();
    });
    expect(screen.getByText(/photographer reviewed/)).toBeTruthy();
  });
});

describe('confidence', () => {
  it('labels the fallback as reference aesthetics rather than learned output', () => {
    const unavailable = result({
      reasons: [
        reason({
          code: 'aesthetic_unavailable',
          text: 'the learned component did not run',
          weight: 0,
          exoneration: true,
          evidence: null,
        }),
      ],
    });
    expect(aestheticSentence(unavailable)).toBe('reference aesthetic 68%');
    expect(aestheticSentence(result())).toBe('learned aesthetic 68%');
  });

  it('names every missing input represented on the wire', () => {
    const weak = result({
      confidence: 0.48,
      scene: 'unknown',
      horizonSource: 'none',
      flags: ['no_geometry'],
      reasons: [
        reason({ code: 'no_rule', weight: 0, exoneration: true, evidence: null }),
        reason({
          code: 'aesthetic_unavailable',
          weight: 0,
          exoneration: true,
          evidence: null,
        }),
      ],
    });
    const sentence = confidenceSentence(weak);
    expect(sentence).toContain('no face or body geometry');
    expect(sentence).toContain('neutral framing rules');
    expect(sentence).toContain('aesthetic head was unavailable');
    expect(sentence).toContain('no reliable horizon');
  });
});
