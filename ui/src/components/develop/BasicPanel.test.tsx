import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';

import { BasicPanel } from './BasicPanel';
import { ToneReviewPanel } from './ToneReviewPanel';
import type { RecipeDto, ToneDto, ToneStatusDto } from '../../ipc/types';

function recipe(overrides: Partial<RecipeDto> = {}): RecipeDto {
  return {
    photoId: 'pht_1',
    body: '{}',
    recipeHash: 'a'.repeat(64),
    schema: 1,
    engine: 'aura-render/1.0.0',
    source: 'ai',
    confidence: 0.9,
    decisionId: null,
    userEditedFields: [],
    params: [],
    ...overrides,
  };
}

function tone(overrides: Partial<ToneDto> = {}): ToneDto {
  return {
    photoId: 'pht_1',
    exposureEv: 0.45,
    exposureConf: 0.82,
    temperatureK: 3400,
    tint: 6,
    wbConf: 0.71,
    confidence: 0.76,
    illuminants: [
      {
        kind: 'tungsten',
        cctK: 3400,
        tint: 6,
        weight: 0.8,
        chroma: 0.01,
        source: 'known_neutral',
        region: null,
      },
    ],
    mixedLight: false,
    dominantOnSubject: 0,
    subjectLumaBefore: 0.22,
    subjectLumaTarget: 0.5,
    skinDe00Estimate: 1.4,
    alternatives: [],
    reasons: [
      {
        code: 'subject_underexposed',
        text: 'the faces here sat at 22 % and this kind of photograph wants 45 to 55 %',
        weight: -0.04,
        evidence: null,
      },
    ],
    scene: 'indoor_ceremony',
    faceAnchored: true,
    backlit: false,
    colouredLight: false,
    clippingAdded: 0,
    constrainedIdentities: 2,
    userEdited: false,
    userExposureEv: null,
    userTemperatureK: null,
    userTint: null,
    reviewed: false,
    needsReview: false,
    modelVer: 1,
    analysisVer: 1,
    targetsVer: 1,
    ...overrides,
  };
}

function status(overrides: Partial<ToneStatusDto> = {}): ToneStatusDto {
  return {
    photos: 100,
    estimated: 100,
    coverage: 1,
    faceAnchored: 0.62,
    skinConstrained: 0.71,
    mixedLight: 4,
    colouredLight: 2,
    needsReview: 3,
    userEdited: 1,
    meanEv: 0.3,
    meanCct: 4200,
    illuminantCounts: [],
    illuminantNames: [],
    segmentsAnchored: 6,
    segments: 6,
    loci: 12,
    untargetedScenes: [],
    modelVer: 1,
    analysisVer: 1,
    targetsVer: 1,
    ...overrides,
  };
}

const noop = () => undefined;

afterEach(cleanup);

describe('BasicPanel', () => {
  it('says a frame has no estimate rather than showing a neutral one', () => {
    render(
      <BasicPanel
        tone={null}
        recipe={recipe()}
        status={null}
        onOverride={noop}
        onAccept={noop}
        onResetToAi={noop}
      />,
    );
    expect(screen.getByTestId('basic-unestimated').textContent).toContain('not worked out');
    expect(screen.queryByLabelText(/Temperature/)).toBeNull();
  });

  it('reports the two confidences separately', () => {
    render(
      <BasicPanel
        tone={tone()}
        recipe={recipe()}
        status={null}
        onOverride={noop}
        onAccept={noop}
        onResetToAi={noop}
      />,
    );
    const text = screen.getByTestId('basic-provenance').textContent ?? '';
    expect(text).toContain('82%');
    expect(text).toContain('71%');
  });

  it('marks a value the photographer set as protected', () => {
    render(
      <BasicPanel
        tone={tone()}
        recipe={recipe({ userEditedFields: ['global.temperature'] })}
        status={null}
        onOverride={noop}
        onAccept={noop}
        onResetToAi={noop}
      />,
    );
    expect(screen.getAllByLabelText('You set this. AURA will not change it.')).toHaveLength(1);
  });

  it('says when the exposure was not anchored on a face', () => {
    render(
      <BasicPanel
        tone={tone({ faceAnchored: false })}
        recipe={recipe()}
        status={null}
        onOverride={noop}
        onAccept={noop}
        onResetToAi={noop}
      />,
    );
    expect(screen.getByTestId('basic-anchor').textContent).toContain('No faces here');
  });

  it('states mixed light rather than implying it', () => {
    render(
      <BasicPanel
        tone={tone({ mixedLight: true })}
        recipe={recipe()}
        status={null}
        onOverride={noop}
        onAccept={noop}
        onResetToAi={noop}
      />,
    );
    expect(screen.getByTestId('basic-mixed-light').textContent).toContain(
      'more than one colour of light',
    );
  });

  it('says a coloured light was kept on purpose', () => {
    render(
      <BasicPanel
        tone={tone({ colouredLight: true })}
        recipe={recipe()}
        status={null}
        onOverride={noop}
        onAccept={noop}
        onResetToAi={noop}
      />,
    );
    expect(screen.getByTestId('basic-coloured-light').textContent).toContain('kept it');
  });

  it('says when no skin reference bounded the colour', () => {
    render(
      <BasicPanel
        tone={tone({ constrainedIdentities: 0 })}
        recipe={recipe()}
        status={null}
        onOverride={noop}
        onAccept={noop}
        onResetToAi={noop}
      />,
    );
    expect(screen.getByTestId('basic-unconstrained').textContent).toContain('light alone');
  });

  it('sends only the value that changed', () => {
    const onOverride = vi.fn();
    render(
      <BasicPanel
        tone={tone()}
        recipe={recipe()}
        status={null}
        onOverride={onOverride}
        onAccept={noop}
        onResetToAi={noop}
      />,
    );
    fireEvent.change(screen.getByLabelText(/Temperature/), { target: { value: '3800' } });
    expect(onOverride).toHaveBeenCalledWith({ temperatureK: 3800 });
  });

  it('offers reset-to-AI only once the photographer has taken the frame over', () => {
    const { rerender } = render(
      <BasicPanel
        tone={tone()}
        recipe={recipe()}
        status={null}
        onOverride={noop}
        onAccept={noop}
        onResetToAi={noop}
      />,
    );
    expect((screen.getByTestId('basic-reset-ai') as HTMLButtonElement).disabled).toBe(true);
    rerender(
      <BasicPanel
        tone={tone({ userEdited: true })}
        recipe={recipe()}
        status={null}
        onOverride={noop}
        onAccept={noop}
        onResetToAi={noop}
      />,
    );
    expect((screen.getByTestId('basic-reset-ai') as HTMLButtonElement).disabled).toBe(false);
  });

  it('shows the photographer own number and still says what AURA suggested', () => {
    render(
      <BasicPanel
        tone={tone({ userEdited: true, userTemperatureK: 3000 })}
        recipe={recipe()}
        status={null}
        onOverride={noop}
        onAccept={noop}
        onResetToAi={noop}
      />,
    );
    expect((screen.getByLabelText(/Temperature/) as HTMLInputElement).value).toBe('3000');
    // The disagreement stays visible: this is what the review queue and phase 30 read.
    expect(screen.getByTestId('basic-provenance').textContent).toContain('3400');
  });

  it('reports coverage and the face-anchored fraction beside it', () => {
    render(
      <BasicPanel
        tone={tone()}
        recipe={recipe()}
        status={status()}
        onOverride={noop}
        onAccept={noop}
        onResetToAi={noop}
      />,
    );
    const text = screen.getByTestId('basic-coverage').textContent ?? '';
    expect(text).toContain('100%');
    expect(text).toContain('62%');
  });

  it('offers no control that grades', () => {
    render(
      <BasicPanel
        tone={tone()}
        recipe={recipe()}
        status={null}
        onOverride={noop}
        onAccept={noop}
        onResetToAi={noop}
      />,
    );
    // Section 2.2's boundary, asserted rather than remembered.
    for (const forbidden of [/Contrast/, /Saturation/, /Vibrance/, /Curve/, /Clarity/]) {
      expect(screen.queryByLabelText(forbidden)).toBeNull();
    }
  });
});

describe('ToneReviewPanel', () => {
  const queue = [
    tone({ photoId: 'pht_1', scene: 'reception_dancing', wbConf: 0.31 }),
    tone({ photoId: 'pht_2', scene: 'reception_dancing', wbConf: 0.4 }),
    tone({ photoId: 'pht_3', scene: 'indoor_ceremony', wbConf: 0.52 }),
  ];

  it('groups the queue by scene', () => {
    render(
      <ToneReviewPanel queue={queue} onAcceptAll={noop} onAdjust={noop} onOpen={noop} />,
    );
    expect(screen.getByLabelText('Reception dancing')).toBeTruthy();
    expect(screen.getByLabelText('Indoor ceremony')).toBeTruthy();
  });

  it('accepts a whole scene in one action', () => {
    const onAcceptAll = vi.fn();
    render(
      <ToneReviewPanel queue={queue} onAcceptAll={onAcceptAll} onAdjust={noop} onOpen={noop} />,
    );
    fireEvent.click(screen.getByTestId('tone-review-accept-reception_dancing'));
    expect(onAcceptAll).toHaveBeenCalledWith(['pht_1', 'pht_2']);
  });

  it('shifts a whole scene relative to each frame own temperature', () => {
    const onAdjust = vi.fn();
    render(
      <ToneReviewPanel queue={queue} onAcceptAll={noop} onAdjust={onAdjust} onOpen={noop} />,
    );
    fireEvent.change(screen.getByLabelText('Warm or cool them all', { selector: '#tone-review-shift-reception_dancing' }), {
      target: { value: '200' },
    });
    fireEvent.click(screen.getByTestId('tone-review-adjust-reception_dancing'));
    expect(onAdjust).toHaveBeenCalledTimes(2);
    expect(onAdjust).toHaveBeenCalledWith('pht_1', { temperatureK: 3600 });
  });

  it('offers nothing that keeps or rejects a photograph', () => {
    render(
      <ToneReviewPanel queue={queue} onAcceptAll={noop} onAdjust={noop} onOpen={noop} />,
    );
    const labels = screen
      .getAllByRole('button')
      .map((button) => (button.textContent ?? '').toLowerCase());
    for (const label of labels) {
      expect(label).not.toContain('keep');
      expect(label).not.toContain('reject');
      expect(label).not.toContain('delete');
      expect(label).not.toContain('deliver');
    }
  });
});
