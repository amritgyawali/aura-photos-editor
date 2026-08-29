import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';

import type { CropVariantDto, GeometryPlanDto, GeometryStatusDto } from '../../ipc/types';

import { GeometryPanel } from './GeometryPanel';

const REFUSALS = ['crop_cuts_face', 'crop_cuts_hands', 'crop_too_small', 'crop_loses_content'];

function original(): CropVariantDto {
  return {
    purpose: 'original',
    title: 'As shot',
    aspect: 'original',
    rect: { x: 0, y: 0, w: 1, h: 1 },
    score: 0.62,
    safe: true,
  };
}

function plan(overrides: Partial<GeometryPlanDto> = {}): GeometryPlanDto {
  return {
    photoId: 'pht_1',
    scene: 'couple_portrait',
    lensSource: 'profile',
    lensId: 'Canon RF 24-70mm F2.8 L IS USM',
    lensProfile: 'Canon RF 24-70mm F2.8 L IS USM',
    lensSynthetic: true,
    distortion: [0.031, -0.0084, 0],
    vignette: 0.46,
    ca: [1.00042, 0.99961],
    rotateDeg: -1.8,
    rotateConf: 0.88,
    keystoneVertical: null,
    keystoneHorizontal: null,
    keystoneStretch: null,
    keystoneVerticals: 0,
    crops: [original()],
    primaryCrop: 0,
    keptOriginal: true,
    safety: {
      facesIntact: true,
      resolutionOk: true,
      contentKept: true,
      facesChecked: 2,
      handsChecked: 0,
      isEvidence: true,
      refused: [0, 0, 0, 0],
      refusedNames: REFUSALS,
    },
    reasons: [
      {
        code: 'crop_kept_original',
        text: 'The framing as shot was kept: nothing AURA tried was clearly better.',
        weight: 0.02,
        restraint: true,
        evidence: null,
      },
      {
        code: 'levelled',
        text: 'The horizon was levelled.',
        weight: 0.08,
        restraint: false,
        evidence: null,
      },
    ],
    confidence: 0.81,
    profileVer: 1,
    analysisVer: 1,
    rulesVer: 1,
    userEdited: false,
    ...overrides,
  };
}

function status(overrides: Partial<GeometryStatusDto> = {}): GeometryStatusDto {
  return {
    photos: 3200,
    planned: 3200,
    coverage: 1,
    keptOriginal: 0.86,
    profileCovered: 0.71,
    levelled: 340,
    meanRotateDeg: 1.4,
    keystoned: 12,
    variantCounts: [3200, 440, 210, 180, 120],
    variantNames: ['As shot', 'Delivered', 'Album', 'Social', 'Wide'],
    refusedCounts: [1204, 0, 880, 310],
    refusedNames: REFUSALS,
    missingProfiles: ['MYSTERY 58mm'],
    unpoliciedScenes: [],
    needsReview: 14,
    userEdited: 6,
    profilesSynthetic: true,
    profilesKnown: 8,
    profileVer: 1,
    analysisVer: 1,
    rulesVer: 1,
    ...overrides,
  };
}

function noop() {}

afterEach(cleanup);

describe('GeometryPanel', () => {
  it('says the frame was delivered as shot when it was', () => {
    render(
      <GeometryPanel
        status={status()}
        plan={plan()}
        onSelectCrop={noop}
        onSetFraming={noop}
        onAccept={noop}
        onCompare={noop}
        comparing={false}
      />,
    );
    expect(screen.getByTestId('geometry-summary').textContent).toContain('Delivered as shot');
  });

  it('renders what it left alone before what it changed', () => {
    render(
      <GeometryPanel
        status={status()}
        plan={plan()}
        onSelectCrop={noop}
        onSetFraming={noop}
        onAccept={noop}
        onCompare={noop}
        comparing={false}
      />,
    );
    // The restraint list exists and is separate from the action list. Seven frames in ten
    // carry nothing but restraint, and burying it makes the panel read as broken.
    expect(screen.getByTestId('geometry-restraints').textContent).toContain(
      'nothing AURA tried was clearly better',
    );
    expect(screen.getByTestId('geometry-actions').textContent).toContain('levelled');
  });

  it('says a fabricated lens profile was not measured', () => {
    render(
      <GeometryPanel
        status={status()}
        plan={plan()}
        onSelectCrop={noop}
        onSetFraming={noop}
        onAccept={noop}
        onCompare={noop}
        comparing={false}
      />,
    );
    expect(screen.getByTestId('geometry-caveats').textContent).toContain('not measured');
    expect(screen.getByTestId('geometry-lens').textContent).toContain('has not measured');
  });

  it('never renders an unchecked safety rule as a passed one', () => {
    render(
      <GeometryPanel
        status={status()}
        plan={plan({
          safety: {
            facesIntact: true,
            resolutionOk: true,
            contentKept: true,
            facesChecked: 0,
            handsChecked: 0,
            isEvidence: false,
            refused: [0, 0, 0, 0],
            refusedNames: REFUSALS,
          },
        })}
        onSelectCrop={noop}
        onSetFraming={noop}
        onAccept={noop}
        onCompare={noop}
        comparing={false}
      />,
    );
    const caveats = screen.getByTestId('geometry-caveats').textContent ?? '';
    expect(caveats).toContain('not the same as a crop being proven safe');
    expect(caveats).toContain('cannot see hands yet');
  });

  it('offers revert as the whole frame at zero degrees', () => {
    const onSetFraming = vi.fn();
    render(
      <GeometryPanel
        status={status()}
        plan={plan()}
        onSelectCrop={noop}
        onSetFraming={onSetFraming}
        onAccept={noop}
        onCompare={noop}
        comparing={false}
      />,
    );
    fireEvent.click(screen.getByTestId('geometry-revert'));
    expect(onSetFraming).toHaveBeenCalledWith({ x: 0, y: 0, w: 1, h: 1 }, 0);
  });

  it('always renders the frame as shot as the first crop', () => {
    render(
      <GeometryPanel
        status={status()}
        plan={plan({
          crops: [
            original(),
            {
              purpose: 'primary',
              title: 'Delivered',
              aspect: 'original',
              rect: { x: 0.05, y: 0.04, w: 0.82, h: 0.84 },
              score: 0.74,
              safe: true,
            },
          ],
          primaryCrop: 1,
          keptOriginal: false,
        })}
        onSelectCrop={noop}
        onSetFraming={noop}
        onAccept={noop}
        onCompare={noop}
        comparing={false}
      />,
    );
    const buttons = screen.getAllByRole('button', { pressed: false });
    expect(screen.getByTestId('geometry-variant-original')).toBeTruthy();
    expect(buttons.length).toBeGreaterThan(0);
    expect(screen.getByTestId('geometry-summary').textContent).toContain('Re-framed');
  });

  it('says how many framings it rejected and why', () => {
    render(
      <GeometryPanel
        status={status()}
        plan={plan({
          safety: {
            facesIntact: true,
            resolutionOk: true,
            contentKept: true,
            facesChecked: 3,
            handsChecked: 0,
            isEvidence: true,
            refused: [12, 0, 4, 1],
            refusedNames: REFUSALS,
          },
        })}
        onSelectCrop={noop}
        onSetFraming={noop}
        onAccept={noop}
        onCompare={noop}
        comparing={false}
      />,
    );
    const text = screen.getByTestId('geometry-refusals').textContent ?? '';
    expect(text).toContain('17 other framings');
    expect(text).toContain('12 that cut a face');
  });

  it('has no control that could crop, cull or fill', () => {
    const { container } = render(
      <GeometryPanel
        status={status()}
        plan={plan()}
        onSelectCrop={noop}
        onSetFraming={noop}
        onAccept={noop}
        onCompare={noop}
        comparing={false}
      />,
    );
    const text = (container.textContent ?? '').toLowerCase();
    for (const forbidden of ['fill', 'reject', 'delete', 'generate']) {
      expect(text).not.toContain(forbidden);
    }
  });
});
