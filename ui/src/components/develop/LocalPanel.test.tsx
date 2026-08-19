import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';

import type { LocalPlanDto, LocalStatusDto } from '../../ipc/types';

import { LocalPanel } from './LocalPanel';

const OPERATIONS = [
  'face_light',
  'subject_enhance',
  'background_balance',
  'shine_control',
  'dodge_burn_low',
  'dodge_burn_mid',
];

function plan(overrides: Partial<LocalPlanDto> = {}): LocalPlanDto {
  return {
    photoId: 'pht_1',
    strengths: [0.85, 0.4, 0.6, 0.5, 0.3, 0.3],
    operations: OPERATIONS,
    faces: [
      {
        identityId: null,
        exposureEv: 0.32,
        shadows: 24,
        highlights: -6,
        lumaBefore: 0.22,
        lumaTarget: 0.48,
        lumaAfter: 0.41,
        noiseCapEv: 0.9,
        maskScale: 1,
      },
    ],
    subjectClarity: 12,
    subjectTexture: 6,
    subjectContrast: 4,
    backgroundEv: -0.3,
    backgroundSaturation: -10,
    competitionRatio: 1.4,
    chromaEnergy: 0.05,
    meanLumaBefore: 0.5,
    meanLumaAfter: 0.49,
    shineRegions: 0,
    shineEv: 0,
    shineBoxes: [],
    shaping: [],
    faceSpread: 0,
    groupFair: true,
    budgetUsed: 0.42,
    gated: [],
    reasons: [{ code: 'face_lit', text: 'the light on this face was lifted', weight: 0, operation: 'face_light', withdrawal: false, evidence: null }],
    confidence: 0.8,
    scene: 'ceremony',
    userEdited: false,
    reviewed: false,
    needsReview: false,
    modelVer: 100,
    analysisVer: 1,
    policyVer: 1,
    shapingVer: 1,
    ...overrides,
  };
}

function status(overrides: Partial<LocalStatusDto> = {}): LocalStatusDto {
  return {
    photos: 100,
    planned: 80,
    coverage: 0.8,
    actedOn: 0.6,
    maskCovered: 1,
    opCounts: [40, 10, 12, 5, 3, 3],
    opNames: OPERATIONS,
    gatedCounts: [0, 0, 0, 0, 0, 0],
    gatedNames: ['face', 'subject', 'background', 'skin', 'hair', 'sky'],
    meanBudgetUsed: 0.3,
    shineReduced: 5,
    meanShineEv: -0.2,
    groupSolved: 12,
    needsReview: 2,
    userEdited: 1,
    unpoliciedScenes: [],
    modelVer: 100,
    analysisVer: 1,
    policyVer: 1,
    shapingVer: 1,
    ...overrides,
  };
}

const noop = () => {};

afterEach(cleanup);

describe('LocalPanel', () => {
  it('says nothing has been looked at rather than showing an empty edit', () => {
    render(
      <LocalPanel
        status={null}
        plan={null}
        onSetStrength={noop}
        onAccept={noop}
        onCompare={noop}
        comparing={false}
      />,
    );
    expect(screen.getByTestId('local-empty').textContent).toContain('has not looked');
  });

  it('shows a strength for every operation', () => {
    render(
      <LocalPanel
        status={status()}
        plan={plan()}
        onSetStrength={noop}
        onAccept={noop}
        onCompare={noop}
        comparing={false}
      />,
    );
    for (const operation of OPERATIONS) {
      expect(screen.getByTestId(`local-strength-${operation}`)).not.toBeNull();
    }
  });

  it('shows a gated operation as unavailable rather than as off', () => {
    // The most common state on a build with no phase 18, and the one that must not read as
    // "AURA decided this photograph needed nothing".
    render(
      <LocalPanel
        status={status({ maskCovered: 0 })}
        plan={plan({
          gated: [{ operation: 'background_balance', maskKind: 'background' }],
          strengths: [0.85, 0.4, 0, 0.5, 0.3, 0.3],
        })}
        onSetStrength={noop}
        onAccept={noop}
        onCompare={noop}
        comparing={false}
      />,
    );
    const gate = screen.getByTestId('local-gate-background_balance');
    expect(gate.textContent).toContain('could not find the background');
    const slider = screen.getByTestId('local-strength-background_balance') as HTMLInputElement;
    expect(slider.disabled).toBe(true);
    expect(screen.getByTestId('local-caveats').textContent).toContain('left out');
  });

  it('says what stopped a lift rather than only how far it went', () => {
    render(
      <LocalPanel
        status={status()}
        plan={plan()}
        onSetStrength={noop}
        onAccept={noop}
        onCompare={noop}
        comparing={false}
      />,
    );
    const table = screen.getByTestId('local-faces');
    expect(table.textContent).toContain('Could have moved');
    expect(table.textContent).toContain('+0.90 EV');
  });

  it('reports a group that could not be evened without claiming it was', () => {
    render(
      <LocalPanel
        status={status()}
        plan={plan({
          faces: [
            { identityId: null, exposureEv: 0.6, shadows: 40, highlights: -10, lumaBefore: 0.14, lumaTarget: 0.5, lumaAfter: 0.2, noiseCapEv: 0.6, maskScale: 1 },
            { identityId: null, exposureEv: 0, shadows: 0, highlights: 0, lumaBefore: 0.5, lumaTarget: 0.5, lumaAfter: 0.5, noiseCapEv: 1.2, maskScale: 1 },
          ],
          faceSpread: 0.3,
          groupFair: false,
        })}
        onSetStrength={noop}
        onAccept={noop}
        onCompare={noop}
        comparing={false}
      />,
    );
    const group = screen.getByTestId('local-group');
    expect(group.textContent).toContain('could not even this group out');
    expect(group.textContent).toContain('Nobody was darkened');
  });

  it('records a strength change against the operation it names', () => {
    const onSetStrength = vi.fn();
    render(
      <LocalPanel
        status={status()}
        plan={plan()}
        onSetStrength={onSetStrength}
        onAccept={noop}
        onCompare={noop}
        comparing={false}
      />,
    );
    fireEvent.change(screen.getByTestId('local-strength-dodge_burn_low'), {
      target: { value: '0' },
    });
    expect(onSetStrength).toHaveBeenCalledWith('dodge_burn_low', 0);
  });

  it('stops offering to accept a plan the photographer has taken over', () => {
    render(
      <LocalPanel
        status={status()}
        plan={plan({ userEdited: true })}
        onSetStrength={noop}
        onAccept={noop}
        onCompare={noop}
        comparing={false}
      />,
    );
    expect(screen.getByTestId('local-user-edited').textContent).toContain('by hand');
    expect(screen.queryByTestId('local-accept')).toBeNull();
  });

  it('never offers to keep, reject or smooth anything', () => {
    // Section 2.2's boundary, as a test. Phase 12 owns delivery and phase 20 owns texture.
    const { container } = render(
      <LocalPanel
        status={status()}
        plan={plan()}
        onSetStrength={noop}
        onAccept={noop}
        onCompare={noop}
        comparing={false}
      />,
    );
    const text = container.textContent?.toLowerCase() ?? '';
    for (const forbidden of ['keep', 'reject', 'deliver', 'cull', 'smooth', 'blur']) {
      expect(text).not.toContain(forbidden);
    }
  });
});
