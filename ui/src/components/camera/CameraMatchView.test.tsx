import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type {
  CameraReportDto,
  CameraStatusDto,
  CameraTransformDto,
  ShooterBiasDto,
} from '../../ipc/types';
import { CameraMatchView } from './CameraMatchView';

/**
 * PHASE-26. Every test here is about **not overstating what was measured**.
 *
 * The view's job is not to render numbers - any component can do that. Its job is to make sure a
 * photographer can never mistake a correction derived from a fabricated brand setting for one
 * measured from their own wedding, and the three assertions below are the three ways that could go
 * wrong.
 */

function status(overrides: Partial<CameraStatusDto> = {}): CameraStatusDto {
  return {
    photos: 3200,
    matched: 3200,
    coverage: 1,
    cameras: 2,
    fingerprinted: 2,
    solvedFromPairs: 1,
    blended: 0,
    baselineOnly: 1,
    pairs: 34,
    pairsRejected: 12,
    heldoutPairs: 9,
    flashSeparated: 2,
    shootersMeasured: 1,
    shootersCapped: 1,
    disabled: 0,
    userEdited: 0,
    skinDe00Before: 0,
    skinDe00After: 0,
    worstSkinDe00: 0,
    referenceId: 'cam_lead',
    referenceSource: 'primary_shooter',
    unknownBrands: [],
    baselinesMeasured: false,
    skinFieldAvailable: false,
    policyVer: 1,
    ...overrides,
  };
}

function report(overrides: Partial<CameraReportDto> = {}): CameraReportDto {
  return {
    cameraId: 'cam_second',
    flash: 'ambient',
    shooter: 'second',
    isReference: false,
    headline: 'These two cameras never photographed the same thing under the same light.',
    evidence: 'No available-light evidence from this wedding.',
    corrections: ['Made warmer by 220 K.'],
    withdrawals: ['These two cameras never photographed the same thing under the same light.'],
    skinDe00After: 0,
    meetsPromise: true,
    magnitude: 0.24,
    confidence: 0.35,
    ...overrides,
  };
}

function transform(overrides: Partial<CameraTransformDto> = {}): CameraTransformDto {
  return {
    cameraId: 'cam_second',
    flash: 'ambient',
    referenceId: 'cam_lead',
    dCct: 220,
    dTint: -2.4,
    dExposure: 0.12,
    dSaturation: 1.5,
    channelGain: [1, 1, 1],
    contrastShape: [1, 1, 1],
    skinDe00Before: 0,
    skinDe00After: 0,
    skinCapped: false,
    skinLocusValid: true,
    source: 'brand_baseline',
    blend: 0,
    evidencePairs: 0,
    heldoutPairs: 0,
    heldoutBefore: 0,
    heldoutAfter: 0,
    heldoutImproved: null,
    boundedBy: null,
    magnitude: 0.24,
    confidence: 0.35,
    enabled: true,
    userEdited: false,
    reasons: [],
    ...overrides,
  };
}

const noop = vi.fn();

function view(props: Partial<Parameters<typeof CameraMatchView>[0]> = {}) {
  return (
    <CameraMatchView
      status={status()}
      reports={[report()]}
      transforms={[transform()]}
      shooterBias={[]}
      pairs={[]}
      expandedCameraId={null}
      running={false}
      onExpand={noop}
      onRunPass={noop}
      onSetReference={noop}
      onToggleEnabled={noop}
      {...props}
    />
  );
}

describe('CameraMatchView', () => {
  it('never renders a skin promise while no photograph carries a skin region', () => {
    render(view());
    expect(screen.getByText(/Skin was not measured at this wedding/i)).toBeTruthy();
    expect(screen.queryByText(/dE00 apart and is now/i)).toBeNull();
  });

  it('says a brand fallback has not been measured in this build', () => {
    render(view());
    const caveat = screen.getByText(/came from what AURA knows about the manufacturer/i);
    expect(caveat.textContent).toContain('not been measured');
  });

  it('renders a measured brand fallback without the caveat', () => {
    render(view({ status: status({ baselinesMeasured: true }) }));
    const caveat = screen.getByText(/came from what AURA knows about the manufacturer/i);
    expect(caveat.textContent).not.toContain('not been measured');
  });

  it('leads a camera row with its evidence rather than with how far it moved', () => {
    // The whole design rule of this view. `headline` reads the reason set; 220 K is not what a
    // photographer needs to see first when the correction came from a general brand setting.
    render(view());
    const row = screen.getByRole('button', {
      name: /never photographed the same thing/i,
    });
    expect(row.textContent).toContain('from the brand');
    expect(row.textContent).not.toContain('220');
  });

  it('never renders an unrun held-out check as a check that passed', () => {
    render(view({ expandedCameraId: 'cam_second/ambient' }));
    expect(screen.getByText(/not checked - too few spare photographs/i)).toBeTruthy();
  });

  it('reports a failed held-out check as a fallback rather than as silence', () => {
    render(
      view({
        expandedCameraId: 'cam_second/ambient',
        transforms: [transform({ heldoutImproved: false, heldoutPairs: 7 })],
      }),
    );
    expect(screen.getByText(/the brand setting was used instead/i)).toBeTruthy();
  });

  it('shows a shooter habit as measured and applied, never only applied', () => {
    const bias: ShooterBiasDto = {
      shooter: 'second',
      cameraId: 'cam_second',
      scene: 'ceremony',
      measuredEv: -0.62,
      appliedEv: 0.3,
      frames: 240,
      capped: true,
      reasons: [],
    };
    render(view({ shooterBias: [bias] }));
    expect(screen.getByText('-0.62 EV')).toBeTruthy();
    expect(screen.getByText(/\+0\.30 EV \(deliberately less than all of it\)/)).toBeTruthy();
  });

  it('says there is nothing to match when one camera shot the wedding', () => {
    render(view({ status: status({ cameras: 1 }) }));
    expect(screen.getByText(/nothing to match it to/i)).toBeTruthy();
  });
});
