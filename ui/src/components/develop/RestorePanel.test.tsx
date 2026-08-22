import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';

import type {
  ArtefactReportDto,
  RestoreFaceDto,
  RestorePlanDto,
  RestoreReasonDto,
  RestoreStatusDto,
} from '../../ipc/types';

import { RestorePanel } from './RestorePanel';

const TIER_NAMES = ['off', 'light', 'standard', 'strong'];

function face(overrides: Partial<RestoreFaceDto> = {}): RestoreFaceDto {
  return {
    identityId: 'idt_1',
    area: { x: 0.2, y: 0.2, w: 0.4, h: 0.4 },
    sharpness: 0.55,
    strength: 0.2,
    identityDrift: 0.01,
    resolves: 0,
    skipped: false,
    skippedBecause: null,
    ...overrides,
  };
}

function report(overrides: Partial<ArtefactReportDto> = {}): ArtefactReportDto {
  return {
    textureRetention: 0.97,
    ringing: 0.0031,
    identityDrift: 0.01,
    measuredOn: 40_000,
    resolves: 0,
    denoiseReduced: false,
    sharpenReduced: false,
    faceSkipped: false,
    ...overrides,
  };
}

function reason(overrides: Partial<RestoreReasonDto> = {}): RestoreReasonDto {
  return {
    code: 'restore_tier_from_measured_noise',
    text: 'the amount of noise reduction was chosen from how much noise was actually measured',
    subject: 'denoise',
    weight: 0.6,
    restraint: false,
    area: null,
    ...overrides,
  };
}

function plan(overrides: Partial<RestorePlanDto> = {}): RestorePlanDto {
  return {
    photoId: 'pht_1',
    denoise: 'standard',
    denoiseLuminance: 0.3,
    denoiseColour: 0.42,
    denoiseSigma: 0.012,
    denoiseCamera: 'sony ilce-7m3',
    denoiseMeasured: false,
    sharpenKernel: 1.4,
    sharpenAmount: 0.2,
    sharpenSkinAttenuation: 0.8,
    sharpenCoverage: 0.4,
    faceRecovery: 0.2,
    faces: [face()],
    facesRecovered: 1,
    facesSkippedIdentity: 0,
    selfcheck: report(),
    runWhere: 'local_cpu',
    runWhen: 'export',
    regionCovered: true,
    reasons: [reason()],
    confidence: 0.82,
    scene: 'dance_floor',
    userEdited: false,
    reviewed: false,
    ...overrides,
  };
}

function status(overrides: Partial<RestoreStatusDto> = {}): RestoreStatusDto {
  return {
    photos: 100,
    planned: 100,
    coverage: 1,
    actedOn: 42,
    regionCovered: 0,
    tiers: [58, 20, 22, 0],
    tierNames: TIER_NAMES,
    sharpened: 0,
    sharpenRefusals: [
      {
        code: 'restore_sharpen_no_regions',
        text: 'AURA could not tell where the skin, the sky and the out-of-focus background were',
        count: 100,
      },
    ],
    facesRecovered: 0,
    facesSkippedIdentity: 3,
    worstIdentityDrift: 0.02,
    meanTextureRetention: 0.96,
    meanRinging: 0,
    reduced: 4,
    needsReview: 2,
    userEdited: 1,
    unmeasuredCameras: ['sony ilce-7m3'],
    unlistedScenes: [],
    versions: [0, 1, 1],
    ...overrides,
  };
}

afterEach(() => cleanup());

describe('RestorePanel', () => {
  it('shows what was held back as prominently as what was done', () => {
    // Rule 1. Twenty of the thirty reason codes in this phase are refusals, and the commonest
    // question a photographer has is why a frame was *not* sharpened.
    render(<RestorePanel status={status()} plan={plan()} />);
    expect(screen.getByText(/could not tell where the skin/i)).toBeTruthy();
    expect(screen.getByText(/Why sharpening was held back/i)).toBeTruthy();
  });

  it('gives a declined face its own block and the measured distance', () => {
    // **Rule 2, and the reason this panel exists in the shape it does.** A face left alone to keep
    // somebody looking like themselves is the single thing this phase most needs to say out loud,
    // so it is a headline rather than a line in a list.
    const declined = face({
      skipped: true,
      strength: 0,
      identityDrift: 0.134,
      resolves: 3,
      skippedBecause: 'restore_identity_drift_skipped',
    });
    render(
      <RestorePanel
        status={status()}
        plan={plan({ faces: [declined], facesRecovered: 0, facesSkippedIdentity: 1 })}
      />,
    );
    const block = screen.getByTestId('restore-declined');
    expect(block.textContent).toMatch(/would have started to change what that person looks like/i);
    expect(block.textContent).toContain('0.1340');
    expect(block.textContent).toMatch(/3 attempts/);
  });

  it('has no control that could set an amount', () => {
    // Rule 3. A photographer chooses which of four; how far each goes is a product decision the
    // contract bounds. A range input on this panel would make `docs/restoration.md` a description
    // of the defaults rather than a promise.
    const { container } = render(<RestorePanel status={status()} plan={plan()} />);
    expect(container.querySelectorAll('input[type="range"]').length).toBe(0);
    expect(container.querySelectorAll('input[type="number"]').length).toBe(0);
  });

  it('offers the four tiers and reports which one is set', () => {
    const onOverride = vi.fn();
    render(<RestorePanel status={status()} plan={plan()} onOverride={onOverride} />);
    const light = screen.getByRole('button', { name: 'Light' });
    const standard = screen.getByRole('button', { name: 'Standard' });
    expect(standard.getAttribute('aria-pressed')).toBe('true');
    expect(light.getAttribute('aria-pressed')).toBe('false');
    fireEvent.click(light);
    expect(onOverride).toHaveBeenCalledWith({ denoise: 'light' });
  });

  it('keeps sharpening and face recovery as separate switches', () => {
    // A photographer can want a frame sharpened and want no model near anybody's face.
    const onOverride = vi.fn();
    render(<RestorePanel status={status()} plan={plan()} onOverride={onOverride} />);
    fireEvent.click(screen.getByLabelText(/Recover soft faces/i));
    expect(onOverride).toHaveBeenCalledWith({ faceRecovery: false });
    expect(onOverride).not.toHaveBeenCalledWith(expect.objectContaining({ sharpen: expect.anything() }));
  });

  it('refuses to print a ratio measured over too few samples', () => {
    // Rule 4, and phase 21's rule before it: a ratio over a handful of pixels is arithmetic
    // rather than evidence, and three decimal places invite somebody to act on it.
    render(
      <RestorePanel status={status()} plan={plan({ selfcheck: report({ measuredOn: 11 }) })} />,
    );
    expect(screen.getByTestId('restore-texture').textContent).toMatch(/too small an area/i);
    expect(screen.getByTestId('restore-ringing').textContent).toMatch(/too small an area/i);
  });

  it('prints the ratios when there were enough samples', () => {
    render(<RestorePanel status={status()} plan={plan()} />);
    expect(screen.getByTestId('restore-texture').textContent).toBe('97%');
    expect(screen.getByTestId('restore-ringing').textContent).toBe('0.0031');
  });

  it('names the cameras it could not measure', () => {
    // Rule 5. The one thing on this panel a photographer can act on.
    render(<RestorePanel status={status()} plan={plan()} />);
    const note = screen.getByTestId('restore-unmeasured');
    expect(note.textContent).toContain('sony ilce-7m3');
    expect(note.textContent).toMatch(/holds back from its strongest setting/i);
  });

  it('says a photograph was left alone rather than showing an empty measurement', () => {
    render(<RestorePanel status={status()} plan={plan({ selfcheck: null })} />);
    expect(screen.getByText(/nothing was changed in this photograph/i)).toBeTruthy();
  });

  it('says nothing has been looked at when there is no plan', () => {
    render(<RestorePanel status={status()} plan={null} />);
    expect(screen.getByText(/has not been looked at yet/i)).toBeTruthy();
  });

  it('never labels a decision as a deletion or a rejection', () => {
    // Nothing in this phase culls. The wording has to make that obvious, because a panel that
    // said "rejected" beside a frame would be read as a shortlist.
    const { container } = render(<RestorePanel status={status()} plan={plan()} />);
    const text = (container.textContent ?? '').toLowerCase();
    for (const forbidden of ['reject', 'delete', 'discard', 'cull']) {
      expect(text).not.toContain(forbidden);
    }
  });

  it('offers acceptance only until it has been accepted', () => {
    const onAccept = vi.fn();
    const { rerender } = render(
      <RestorePanel status={status()} plan={plan()} onAccept={onAccept} />,
    );
    fireEvent.click(screen.getByRole('button', { name: /Looks right/i }));
    expect(onAccept).toHaveBeenCalled();
    rerender(<RestorePanel status={status()} plan={plan({ reviewed: true })} onAccept={onAccept} />);
    expect(screen.queryByRole('button', { name: /Looks right/i })).toBeNull();
  });
});
