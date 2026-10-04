import { act, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { CleanupPanel } from './CleanupPanel';

const fixtures = vi.hoisted(() => {
  const STATUS = {
    photos: 100,
    examined: 100,
    coverage: 1,
    withProposals: 0,
    applied: 0,
    blocked: [0, 0, 0, 0, 100],
    checkNames: ['size_cap', 'denylist', 'identity_protect', 'structure_span', 'confidence'],
    borrowed: 0,
    filled: 0,
    inpainted: 0,
    reverted: 0,
    maskCovered: 0,
    detectorTrained: false,
    inpaintAvailable: false,
  };
  return {
    STATUS,
    decideCleanup: vi.fn().mockResolvedValue({}),
    disableCleanup: vi.fn().mockResolvedValue({}),
    manualRemove: vi.fn().mockResolvedValue({ proposal: null, blocked: null }),
    cleanupStatus: vi.fn(() => Promise.resolve(STATUS)),
    imageCleanup: vi.fn().mockResolvedValue([]),
    cleanupBlocked: vi.fn().mockResolvedValue([]),
  };
});

vi.mock('../../ipc/client', () => ({
  inTauri: () => true,
  asIpcError: (error: unknown) => ({ code: 'AURA-TEST-0001', message: String(error) }),
  cleanup: {
    cleanupStatus: fixtures.cleanupStatus,
    imageCleanup: fixtures.imageCleanup,
    cleanupBlocked: fixtures.cleanupBlocked,
    decideCleanup: fixtures.decideCleanup,
    disableCleanup: fixtures.disableCleanup,
    manualRemove: fixtures.manualRemove,
  },
  develop: {
    renderImage: vi.fn().mockResolvedValue({
      width: 8,
      height: 8,
      rgbBase64: '',
      colourSpace: 'srgb',
      icc: '',
      renderHash: 'r',
      backend: 'processor',
      stagesRun: [],
      notes: [],
      ms: 1,
    }),
  },
}));

async function mount(projectId: string | null, photoId: string | null) {
  const result = render(
    <CleanupPanel projectId={projectId} photoId={photoId} onError={() => undefined} />,
  );
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
  return result;
}

describe('CleanupPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('says what to do rather than rendering an empty queue with no wedding open', async () => {
    await mount(null, null);
    expect(screen.getByText(/Open a wedding/)).toBeDefined();
  });

  it('reads the project status even with no photograph selected', async () => {
    // The status is the thing worth showing on this build: nothing is proposed on any frame,
    // and the refusal histogram plus `maskCovered` is what says why.
    await mount('prj_1', null);
    expect(fixtures.cleanupStatus).toHaveBeenCalledWith('prj_1');
    expect(fixtures.imageCleanup).not.toHaveBeenCalled();
  });

  it('asks for the proposals and the refusals of one photograph together', async () => {
    await mount('prj_1', 'pho_1');
    expect(fixtures.imageCleanup).toHaveBeenCalledWith('pho_1');
    expect(fixtures.cleanupBlocked).toHaveBeenCalledWith('pho_1');
  });

  it('offers the manual tool only once a photograph is open', async () => {
    await mount('prj_1', null);
    expect(screen.getByText(/Select a photograph to remove something by hand/)).toBeDefined();
  });

  it('has no field a description of what to generate could go in', async () => {
    // `docs/generative-policy.md` promises AURA never generates from a description, and the way
    // that promise is kept is that no type on this surface could carry one. The panel must not
    // be where it reappears.
    const { container } = await mount('prj_1', 'pho_1');
    const texts = [...container.querySelectorAll('input, textarea')].filter((element) => {
      const type = element.getAttribute('type');
      return element.tagName === 'TEXTAREA' || type === null || type === 'text';
    });
    expect(texts).toHaveLength(0);
  });
});
