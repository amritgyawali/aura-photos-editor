import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';

import type { ProtectedFeatureDto, RetouchPlanDto, RetouchStatusDto } from '../../ipc/types';

import { RetouchPanel } from './RetouchPanel';

function protectedFeature(overrides: Partial<ProtectedFeatureDto> = {}): ProtectedFeatureDto {
  return {
    identityId: 'idt_1',
    kind: 'mole',
    area: { x: -0.16, y: 0.34, w: 0.04, h: 0.04 },
    confidence: 0.9,
    source: 'cross_frame',
    frames: 12,
    spanMinutes: 240,
    firstSeenPhoto: 'pht_1',
    absolute: false,
    ...overrides,
  };
}

function plan(overrides: Partial<RetouchPlanDto> = {}): RetouchPlanDto {
  return {
    photoId: 'pht_1',
    ops: [
      {
        kind: 'blemish',
        strength: 0.7,
        area: { x: 0.4, y: 0.4, w: 0.02, h: 0.02 },
        method: 'patch',
        identityId: null,
        lumaEv: 0,
        chroma: 0,
      },
    ],
    identityStrengths: [{ identityId: 'idt_1', strength: 0.7 }],
    protected: [protectedFeature()],
    texture: {
      bandRatio: 0.96,
      floor: 0.9,
      passed: true,
      measuredOn: 4096,
      resolves: 0,
      withdrawn: false,
    },
    preset: 'natural',
    reasons: [
      {
        code: 'blemish_removed',
        text: 'a temporary mark was removed and the skin around it kept its own texture',
        weight: 0.1,
        withdrawal: false,
        evidence: null,
      },
      {
        code: 'anomaly_uncertain',
        text: 'AURA was not sure whether this mark was temporary or part of how this person looks, so it left it alone',
        weight: -0.02,
        withdrawal: true,
        evidence: null,
      },
    ],
    confidence: 0.8,
    scene: 'couple_portrait',
    budgetUsed: 0.2,
    userEdited: false,
    reviewed: false,
    needsReview: false,
    modelVer: 100,
    analysisVer: 1,
    presetVer: 1,
    ...overrides,
  };
}

function status(overrides: Partial<RetouchStatusDto> = {}): RetouchStatusDto {
  return {
    photos: 400,
    planned: 380,
    coverage: 0.95,
    actedOn: 0.4,
    maskCovered: 0,
    blemishesRemoved: 128,
    anomaliesLeft: 44,
    protectedCounts: [3, 1, 0, 0, 1, 0],
    protectedKinds: ['mole', 'freckle', 'birthmark', 'scar', 'tattoo', 'dimple'],
    textureResolved: 6,
    textureWithdrawn: 1,
    meanBandRatio: 0.95,
    meanStrength: 0.62,
    maxIdentitySpread: 0,
    presetCounts: [0, 20, 350, 10],
    presetNames: ['off', 'light', 'natural', 'polished'],
    needsReview: 3,
    userEdited: 2,
    unpresetScenes: [],
    modelVer: 100,
    analysisVer: 1,
    presetVer: 1,
    ...overrides,
  };
}

function renderPanel(overrides: Partial<React.ComponentProps<typeof RetouchPanel>> = {}) {
  const props = {
    status: status(),
    plan: plan(),
    onSetPreset: vi.fn(),
    onSetStrength: vi.fn(),
    onClearProtection: vi.fn(),
    onAccept: vi.fn(),
    onCompare: vi.fn(),
    comparing: false,
    ...overrides,
  };
  render(<RetouchPanel {...props} />);
  return props;
}

afterEach(cleanup);

describe('RetouchPanel', () => {
  it('shows what was left alone as well as what was done', () => {
    renderPanel();
    expect(screen.getByTestId('retouch-ops').textContent).toContain('Mark removed');
    // Rule 1. The most common question about a retoucher is why a mark is still there.
    expect(screen.getByTestId('retouch-left-alone').textContent).toContain('left it alone');
  });

  it('shows the texture measurement with the number of samples behind it', () => {
    renderPanel();
    const texture = screen.getByTestId('retouch-texture');
    expect(texture.textContent).toContain('96%');
    expect(texture.textContent).toContain('90%');
    expect(texture.textContent).toContain('4096 samples');
  });

  it('says so when the retouch was withdrawn rather than showing a ratio', () => {
    renderPanel({
      plan: plan({
        ops: [],
        texture: {
          bandRatio: 0.82,
          floor: 0.9,
          passed: false,
          measuredOn: 4096,
          resolves: 3,
          withdrawn: true,
        },
      }),
    });
    expect(screen.getByTestId('retouch-texture').textContent).toContain('nothing was applied');
    expect(screen.getByTestId('retouch-caveats').textContent).toContain('left it alone');
  });

  it('offers no control at all for a tattoo', () => {
    // Rule 3. A disabled control invites somebody to look for the setting that enables it, and
    // there is not one: section 10.1 gates tattoo removal at zero.
    renderPanel({
      plan: plan({ protected: [protectedFeature({ kind: 'tattoo', absolute: true })] }),
    });
    expect(screen.getByTestId('retouch-protected-absolute').textContent).toContain(
      'never alters tattoos',
    );
    expect(screen.queryByTestId('retouch-unprotect-tattoo-0')).toBeNull();
  });

  it('lets a photographer stop protecting a mole, and says why it was protected', () => {
    const props = renderPanel();
    expect(screen.getByTestId('retouch-protected').textContent).toContain('12 photographs across 4.0 hours');
    fireEvent.click(screen.getByTestId('retouch-unprotect-mole-0'));
    expect(props.onClearProtection).toHaveBeenCalledTimes(1);
  });

  it('records a preset and a per-person strength through the caller', () => {
    const props = renderPanel();
    fireEvent.click(screen.getByTestId('retouch-preset-light'));
    expect(props.onSetPreset).toHaveBeenCalledWith('light');

    fireEvent.change(screen.getByTestId('retouch-strength-idt_1'), {
      target: { value: '40' },
    });
    expect(props.onSetStrength).toHaveBeenCalledWith('idt_1', 0.4);
  });

  it('says that a strength applies to the whole wedding', () => {
    // Section 6.4's whole point: a strength set on one frame and not the rest is a bride whose
    // skin changes character between the ceremony and the reception.
    renderPanel();
    expect(screen.getByTestId('retouch-people').textContent).toContain('everywhere in this wedding');
  });

  it('says when the heads are untrained rather than describing the output as learned', () => {
    renderPanel({
      plan: plan({
        reasons: [
          {
            code: 'head_untrained',
            text: 'AURA is using its measured retouching rather than a learned model in this build',
            weight: -0.05,
            withdrawal: false,
            evidence: null,
          },
        ],
      }),
    });
    expect(screen.getByTestId('retouch-caveats').textContent).toContain('measured retouching');
  });

  it('renders nothing to argue with before a photograph has been retouched', () => {
    renderPanel({ plan: null });
    expect(screen.getByTestId('retouch-empty').textContent).toContain('not retouched');
  });

  it('never offers to reshape, slim or lighten anybody', () => {
    // Rule 5, as a test. Section 11 of docs/plan/CLAUDE.md forbids all three permanently, and
    // the panel is the surface where one would first appear as a slider.
    const { container } = render(
      <RetouchPanel
        status={status()}
        plan={plan()}
        onSetPreset={vi.fn()}
        onSetStrength={vi.fn()}
        onClearProtection={vi.fn()}
        onAccept={vi.fn()}
        onCompare={vi.fn()}
        comparing={false}
      />,
    );
    const text = container.textContent?.toLowerCase() ?? '';
    for (const banned of ['slim', 'reshape', 'lighten', 'whiten', 'smooth skin']) {
      expect(text).not.toContain(banned);
    }
  });
});
