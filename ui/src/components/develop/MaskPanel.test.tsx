import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';

import { MaskPanel, limitingFactor } from './MaskPanel';
import type { MaskDto, MaskStatusDto } from '../../ipc/types';

vi.mock('../../ipc/client', () => ({
  maskApi: {
    maskStatus: vi.fn(),
    imageMasks: vi.fn(),
    ensureMasks: vi.fn(),
    maskOverlay: vi.fn(),
    maskAllowance: vi.fn(),
    editMask: vi.fn(),
    regenerateMask: vi.fn(),
    maskKinds: vi.fn(),
  },
}));

import { maskApi } from '../../ipc/client';

function mask(overrides: Partial<MaskDto> = {}): MaskDto {
  return {
    id: 'msk_1',
    imageId: 'pht_1',
    kind: 'skin',
    identityId: null,
    identityName: null,
    form: 'alpha8',
    width: 192,
    height: 128,
    bytes: 24576,
    feather: 0,
    confidence: 0.9,
    edgeQuality: 0.85,
    edge: 'matted',
    allowance: 0.87,
    allowsAggressive: true,
    reasons: [{ code: 'seeded_by_face', text: 'Found from a face AURA detected.' }],
    userEdited: false,
    modelVer: 1,
    ...overrides,
  };
}

function status(overrides: Partial<MaskStatusDto> = {}): MaskStatusDto {
  return {
    selected: 340,
    masked: 312,
    masks: 6240,
    userEdited: 4,
    lowQuality: 11,
    meanConfidence: 0.81,
    meanEdgeQuality: 0.72,
    payloadBytes: 40_000_000,
    bytesPerImage: 128_205,
    modelVer: 1,
    analysisVer: 1,
    headTrained: false,
    ...overrides,
  };
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('MaskPanel', () => {
  it('says how many of the selected frames are masked, as two numbers rather than a ratio', async () => {
    vi.mocked(maskApi.maskStatus).mockResolvedValue(status());
    vi.mocked(maskApi.imageMasks).mockResolvedValue([mask()]);

    render(<MaskPanel projectId="prj_1" imageId="pht_1" />);

    await waitFor(() =>
      expect(screen.getByText(/312 of 340 selected photographs/)).toBeTruthy(),
    );
  });

  it('says nothing is selected rather than showing a coverage of zero', async () => {
    // A project where the cull has not run has no denominator. A coverage figure computed
    // against one that does not exist reads as a failure rather than as a question nobody has
    // asked yet.
    vi.mocked(maskApi.maskStatus).mockResolvedValue(status({ selected: 0, masked: 0 }));
    vi.mocked(maskApi.imageMasks).mockResolvedValue([]);

    render(<MaskPanel projectId="prj_1" imageId="pht_1" />);

    await waitFor(() => expect(screen.getByText(/Nothing is selected yet/)).toBeTruthy());
    expect(screen.queryByText(/0 of 0/)).toBeNull();
  });

  it('says the learned segmentation is not trained in this build', async () => {
    vi.mocked(maskApi.maskStatus).mockResolvedValue(status({ headTrained: false }));
    vi.mocked(maskApi.imageMasks).mockResolvedValue([mask()]);

    render(<MaskPanel projectId="prj_1" imageId="pht_1" />);

    await waitFor(() =>
      expect(screen.getByText(/learned segmentation is not trained/)).toBeTruthy(),
    );
  });

  it('offers to find regions when nobody has looked yet', async () => {
    // An empty list is "nobody looked", not "there is nothing here". The two are different and
    // the panel must not merge them.
    vi.mocked(maskApi.maskStatus).mockResolvedValue(status());
    vi.mocked(maskApi.imageMasks).mockResolvedValue([]);

    render(<MaskPanel projectId="prj_1" imageId="pht_1" />);

    await waitFor(() => expect(screen.getByText(/Nobody has looked for regions/)).toBeTruthy());
    expect(screen.getByRole('button', { name: 'Find regions' })).toBeTruthy();
  });

  it('marks a region that cannot carry an aggressive operation', async () => {
    vi.mocked(maskApi.maskStatus).mockResolvedValue(status());
    vi.mocked(maskApi.imageMasks).mockResolvedValue([
      mask({ allowance: 0.31, allowsAggressive: false, edgeQuality: 0.1 }),
    ]);

    render(<MaskPanel projectId="prj_1" imageId="pht_1" />);

    await waitFor(() => expect(screen.getByText('Limited')).toBeTruthy());
  });

  it('names which of the two numbers is limiting, as a sentence', () => {
    // The panel must say what to do about it. "Amber" does not.
    const badEdge = mask({ confidence: 0.95, edgeQuality: 0.2, allowance: 0.43 });
    expect(limitingFactor(badEdge)).toMatch(/edge of this region/);

    const badClass = mask({ confidence: 0.2, edgeQuality: 0.95, allowance: 0.43 });
    expect(limitingFactor(badClass)).toMatch(/not certain this region is what it says/);
  });

  it('never limits a region the photographer drew', () => {
    const mine = mask({ userEdited: true, confidence: 0.1, edgeQuality: 0.1, allowance: 1 });
    expect(limitingFactor(mine)).toBeNull();
  });

  it('never says a mask is verified, correct or guaranteed', async () => {
    // The vocabulary check. A mask is a measurement with two uncertainties attached, and a
    // panel that called one "correct" would be making a claim nothing in this phase supports.
    vi.mocked(maskApi.maskStatus).mockResolvedValue(status());
    vi.mocked(maskApi.imageMasks).mockResolvedValue([mask()]);

    const { container } = render(<MaskPanel projectId="prj_1" imageId="pht_1" />);
    await waitFor(() => expect(screen.getByText('Skin')).toBeTruthy());

    const text = container.textContent ?? '';
    for (const word of ['verified', 'guaranteed', 'perfect', 'exact']) {
      expect(text.toLowerCase()).not.toContain(word);
    }
  });
});
