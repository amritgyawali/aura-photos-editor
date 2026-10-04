import { act, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { Inspector } from './Inspector';

// The six cards each fetch on mount. This suite is about the rail rather than about them, so
// every command answers with nothing and the assertions are about which tab is drawn.
vi.mock('../../ipc/client', () => ({
  inTauri: () => false,
  asIpcError: (error: unknown) => ({
    code: 'AURA-TEST-0001',
    message: String(error),
  }),
  api: {
    imageIntegrity: vi.fn().mockResolvedValue(null),
    integrityStatus: vi.fn().mockResolvedValue(null),
    findSimilar: vi.fn().mockResolvedValue({ neighbours: [] }),
    indexStatus: vi.fn().mockResolvedValue(null),
    imageDescriptors: vi.fn().mockResolvedValue(null),
  },
  emotion: {
    imageEmotion: vi.fn().mockResolvedValue(null),
    reactionsOf: vi.fn().mockResolvedValue([]),
    rankedByEmotion: vi.fn().mockResolvedValue([]),
    preferFrame: vi.fn().mockResolvedValue(undefined),
  },
  composition: {
    imageComposition: vi.fn().mockResolvedValue(null),
    dismissCompositionFlag: vi.fn().mockResolvedValue(undefined),
  },
  explain: {
    explainImage: vi.fn().mockResolvedValue(null),
  },
}));

/**
 * Render and let every card's mount effect settle.
 *
 * The six cards each fetch on mount and the fetches resolve immediately here, so without this
 * the state updates land after the assertion and React warns. Awaiting one macrotask inside
 * `act` is the whole of it.
 */
async function mount(element: JSX.Element): Promise<ReturnType<typeof render>> {
  let result!: ReturnType<typeof render>;
  await act(async () => {
    result = render(element);
    await Promise.resolve();
  });
  return result;
}

describe('Inspector', () => {
  it('says what to do rather than rendering an empty rail when nothing is selected', async () => {
    await mount(<Inspector projectId="prj_1" photoId={null} onError={() => undefined} />);
    expect(screen.getByText(/Select a photograph/)).toBeDefined();
  });

  it('offers all six readings of one photograph', async () => {
    await mount(<Inspector projectId="prj_1" photoId="pho_1" onError={() => undefined} />);
    for (const tab of ['Frame', 'Moment', 'Framing', 'Why', 'Alike', 'Best of']) {
      expect(screen.getByRole('button', { name: tab })).toBeDefined();
    }
  });

  it('opens on the frame, because whether a photograph worked is the first question', async () => {
    await mount(<Inspector projectId="prj_1" photoId="pho_1" onError={() => undefined} />);
    expect(screen.getByRole('button', { name: 'Frame' }).getAttribute('aria-pressed')).toBe(
      'true',
    );
  });

  it('has no control that culls, keeps or deletes', async () => {
    // Phase 09's rule, inherited by 10, 11 and 13: a measurement is evidence and the deciding
    // phase owns the cull. A rail that grew a Reject button would be that rule breaking in the
    // one place a photographer would not notice it had.
    const { container } = await mount(
      <Inspector projectId="prj_1" photoId="pho_1" onError={() => undefined} />,
    );
    const labels = [...container.querySelectorAll('button')].map((button) =>
      (button.textContent ?? '').toLowerCase(),
    );
    for (const forbidden of ['reject', 'delete', 'cull', 'discard']) {
      expect(labels.some((label) => label.includes(forbidden))).toBe(false);
    }
  });
});
