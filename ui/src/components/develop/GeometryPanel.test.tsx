import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';

import type {
  CropVariantDto,
  GeometryPlanDto,
  GeometryReasonDto,
  GeometryStatusDto,
} from '../../ipc/types';

import { GeometryPanel } from './GeometryPanel';

function variant(overrides: Partial<CropVariantDto> = {}): CropVariantDto {
  return {
    ordinal: 0,
    aspect: 'original',
    purpose: 'primary',
    rect: { x: 0, y: 0, w: 1, h: 1 },
    score: 0.5,
    safe: true,
    refusal: null,
    longEdgeFraction: 1,
    ...overrides,
  };
}

function reason(overrides: Partial<GeometryReasonDto> = {}): GeometryReasonDto {
  return {
    code: 'geometry_tilt_negligible',
    text: 'the horizon was already level, so nothing was rotated',
    weight: -0.2,
    refusal: true,
    safety: false,
    area: null,
    ...overrides,
  };
}

function plan(overrides: Partial<GeometryPlanDto> = {}): GeometryPlanDto {
  return {
    photoId: 'pht_1',
    scene: 'ceremony',
    lensSource: 'none',
    lensProfile: null,
    lensDistortion: false,
    lensVignette: 0,
    lensCa: false,
    lensMeasured: false,
    rotateDeg: 0,
    rotateConf: 0.42,
    keystone: null,
    crops: [variant()],
    primaryCrop: 0,
    safety: {
      facesIntact: true,
      resolutionOk: true,
      contentKept: true,
      considered: 0,
      atRisk: 0,
      longEdgeFraction: 1,
      regions: [],
    },
    reasons: [reason()],
    confidence: 0.8,
    keptOriginal: true,
    userEdited: false,
    reviewed: false,
    versions: [1, 1],
    ...overrides,
  };
}

function status(overrides: Partial<GeometryStatusDto> = {}): GeometryStatusDto {
  return {
    photos: 100,
    planned: 100,
    coverage: 1,
    actedOn: 18,
    keptOriginal: 82,
    conservatism: 0.82,
    straightened: 12,
    meanRotationDeg: 1.4,
    keystoned: 2,
    cropped: 6,
    variants: 240,
    cropRefusals: [],
    lensSources: [0, 40, 5, 55],
    lensSourceNames: ['embedded', 'database', 'estimated', 'none'],
    lensesMissing: [],
    facesChecked: 0,
    facesCut: 0,
    userEdited: 3,
    pendingReview: 9,
    ...overrides,
  };
}

afterEach(() => {
  cleanup();
});

describe('GeometryPanel', () => {
  it('offers the revert on every plan, including one that was never cropped', () => {
    const onRevert = vi.fn();
    render(<GeometryPanel status={status()} plan={plan()} onRevert={onRevert} />);

    const revert = screen.getByTestId('geometry-revert');
    fireEvent.click(revert);
    expect(onRevert).toHaveBeenCalledTimes(1);
  });

  it('says a zero means nothing when nothing was protected', () => {
    render(<GeometryPanel status={status({ facesChecked: 0, facesCut: 0 })} plan={null} />);

    expect(screen.getByTestId('geometry-safety').textContent).toContain(
      'No faces were found in this project',
    );
  });

  it('reports the safety count against its denominator when there is one', () => {
    render(<GeometryPanel status={status({ facesChecked: 412, facesCut: 0 })} plan={null} />);

    expect(screen.getByTestId('geometry-safety').textContent).toContain('0 of 412');
  });

  it('shows what was left alone, not only what was changed', () => {
    render(<GeometryPanel status={status()} plan={plan()} />);

    const restraints = screen.getByTestId('geometry-restraints');
    expect(restraints.textContent).toContain('the horizon was already level');
  });

  it('shows the horizon confidence even when nothing was rotated', () => {
    render(<GeometryPanel status={status()} plan={plan({ rotateDeg: 0, rotateConf: 0.42 })} />);

    expect(screen.getByTestId('geometry-rotation').textContent).toContain('42%');
  });

  it('names the reason a variant was refused rather than hiding it', () => {
    render(
      <GeometryPanel
        status={status()}
        plan={plan({
          crops: [
            variant(),
            variant({
              ordinal: 1,
              aspect: '1:1',
              purpose: 'social',
              safe: false,
              refusal: 'geometry_crop_cuts_face',
            }),
          ],
        })}
      />,
    );

    expect(screen.getByTestId('geometry-refused').textContent).toContain('geometry_crop_cuts_face');
  });

  it('will not deliver a variant the safety filter refused', () => {
    const onChooseVariant = vi.fn();
    render(
      <GeometryPanel
        status={status()}
        plan={plan({
          crops: [
            variant(),
            variant({ ordinal: 1, aspect: '1:1', safe: false, refusal: 'geometry_crop_cuts_face' }),
          ],
        })}
        onChooseVariant={onChooseVariant}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Square' }));
    expect(onChooseVariant).not.toHaveBeenCalled();
  });

  it('says out loud when a lens profile is a reference model rather than a measurement', () => {
    render(
      <GeometryPanel
        status={status()}
        plan={plan({ lensSource: 'database', lensDistortion: true, lensMeasured: false })}
      />,
    );

    expect(screen.getByTestId('geometry-reference-profile').textContent).toContain(
      'reference model',
    );
  });

  it('names the lenses a studio could have profiled', () => {
    render(
      <GeometryPanel
        status={status({ lensesMissing: [{ lens: 'AURA 35mm f/1.4', count: 220 }] })}
        plan={null}
      />,
    );

    expect(screen.getByTestId('geometry-missing-lenses').textContent).toContain('AURA 35mm f/1.4');
  });

  it('leads with how much of the wedding was kept exactly as shot', () => {
    render(<GeometryPanel status={status()} plan={null} />);

    expect(screen.getByTestId('geometry-kept-original').textContent).toContain('82');
  });
});
