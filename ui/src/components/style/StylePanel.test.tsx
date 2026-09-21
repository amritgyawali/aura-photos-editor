import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { StylePanel } from './StylePanel';

const fixtures = vi.hoisted(() => {
  const PROFILE = {
    profileId: 'prf_1',
    name: 'personal',
    version: 2,
    status: 'candidate',
    trainedPairs: 1200,
    strength: 0.6,
    overallDe00: 2.4,
    taughtBuckets: 31,
    usable: true,
    engineVer: 'aura-style/1',
    trainedAt: 0,
  };
  const REPORT = {
    profile: PROFILE,
    perBucket: [
      {
        key: 'ceremony/daylight',
        group: 'ceremony',
        lighting: 'daylight',
        title: 'Ceremony, daylight',
        samples: 140,
        heldOut: 20,
        matchDe00: 1.8,
        level: 'bucket',
        weak: false,
      },
      {
        key: 'reception/tungsten',
        group: 'reception',
        lighting: 'tungsten',
        title: 'Reception, tungsten light',
        samples: 11,
        heldOut: 0,
        matchDe00: null,
        level: 'group',
        weak: true,
      },
    ],
    weakBuckets: ['reception/tungsten'],
    recommendation: 'Shoot one more reception under tungsten light.',
    acceptedPairs: 1200,
    rejectedPairs: 90,
    acceptance: 0.93,
    metCeiling: false,
  };
  const STATUS = {
    profiles: 1,
    active: 'prf_1',
    activeName: 'personal',
    activeVersion: 2,
    trainedPairs: 1200,
    strength: 0.6,
    overallDe00: 2.4,
    chapterOverrides: [],
  };
  return {
    PROFILE,
    REPORT,
    STATUS,
    listProfiles: vi.fn(() => Promise.resolve([PROFILE])),
    styleStatus: vi.fn(() => Promise.resolve(STATUS)),
    profileReport: vi.fn(() => Promise.resolve(REPORT)),
    adoptProfile: vi.fn().mockResolvedValue({}),
    setProjectProfile: vi.fn().mockResolvedValue({}),
    compareProfiles: vi.fn().mockResolvedValue([]),
  };
});

vi.mock('../../ipc/client', () => ({
  inTauri: () => true,
  asIpcError: (error: unknown) => ({ code: 'AURA-TEST-0001', message: String(error) }),
  style: {
    listProfiles: fixtures.listProfiles,
    styleStatus: fixtures.styleStatus,
    profileReport: fixtures.profileReport,
    adoptProfile: fixtures.adoptProfile,
    setProjectProfile: fixtures.setProjectProfile,
    compareProfiles: fixtures.compareProfiles,
    scanArchive: vi.fn().mockResolvedValue(null),
    trainProfile: vi.fn().mockResolvedValue(null),
  },
}));

async function mount(projectId: string | null) {
  const result = render(<StylePanel projectId={projectId} onError={() => undefined} />);
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
  return result;
}

describe('StylePanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('reads the profiles even with no wedding open, because a profile is not a project', async () => {
    await mount(null);
    expect(fixtures.listProfiles).toHaveBeenCalled();
    expect(fixtures.styleStatus).not.toHaveBeenCalled();
  });

  it('names the active profile and what it was learned from', async () => {
    await mount('prj_1');
    expect(screen.getByText(/personal v2, learned from 1,200 pairs/)).toBeDefined();
  });

  it('shows the bucket a photographer has not taught it, and says nothing was measured', async () => {
    // `matchDe00: null` is "nothing was held out here", and rendering it as zero would be a
    // perfect score where the product knows least. Phase 17's own rule about the report.
    await mount('prj_1');
    // The matrix is a table of cells; the reception/mixed leaf is the one with no held-out
    // photographs behind it.
    const cell = screen
      .getAllByTitle(/photographs/)
      .find((element) => element.getAttribute('title')?.includes('not measured'));
    expect(cell).toBeDefined();
    fireEvent.click(cell as HTMLElement);
    expect(screen.getByText(/nothing was held out here, so there is no measurement/)).toBeDefined();
  });

  it('adopts a profile and points the open wedding at it in one action', async () => {
    await mount('prj_1');
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Use this profile' }));
      await Promise.resolve();
    });
    expect(fixtures.adoptProfile).toHaveBeenCalledWith({ profileId: 'prf_1' });
    expect(fixtures.setProjectProfile).toHaveBeenCalledWith({
      projectId: 'prj_1',
      profileId: 'prf_1',
    });
  });
});
