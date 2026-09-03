import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { StoryPanel } from './StoryPanel';

const fixtures = vi.hoisted(() => {
  function chapter(ordinal: number, startMs: number, endMs: number) {
    return {
      segmentId: `seg_${ordinal}`,
      ordinal,
      chapter: 'ceremony',
      label: null,
      title: `Chapter ${ordinal}`,
      startMs,
      endMs,
      durationMinutes: (endMs - startMs) / 60000,
      dominantScene: 'ceremony_vows',
      confidence: 0.8,
      keyFrame: 'pho_1',
      imageCount: 40,
      reasons: ['the light stopped moving'],
      userLocked: false,
      needsReview: false,
    };
  }
  const OUTLINE = {
    chapters: [chapter(0, 0, 600_000), chapter(1, 600_000, 1_800_000)],
    coverage: 1,
    needsReview: [],
    sceneVer: 1,
    taxonomyVer: 1,
  };
  return {
    OUTLINE,
    storyOutline: vi.fn(() => Promise.resolve(OUTLINE)),
    moveChapterBoundary: vi.fn().mockResolvedValue({ id: 'seg_0' }),
    mergeChapters: vi.fn().mockResolvedValue({ id: 'seg_0' }),
    setChapter: vi.fn().mockResolvedValue({ id: 'seg_0' }),
  };
});

vi.mock('../../ipc/client', () => ({
  inTauri: () => true,
  asIpcError: (error: unknown) => ({ code: 'AURA-TEST-0001', message: String(error) }),
  api: {
    storyOutline: fixtures.storyOutline,
    storyStatus: vi.fn().mockResolvedValue({
      photos: 100,
      classified: 100,
      coverage: 1,
      segments: 2,
      needsReview: 0,
      locked: 0,
      fallbacks: 0,
      modelVer: 1,
      taxonomyVer: 1,
    }),
    sceneProfiles: vi.fn().mockResolvedValue([]),
    moveChapterBoundary: fixtures.moveChapterBoundary,
    mergeChapters: fixtures.mergeChapters,
    setChapter: fixtures.setChapter,
  },
}));

async function mount(projectId: string | null) {
  const result = render(<StoryPanel projectId={projectId} onError={() => undefined} />);
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
  return result;
}

function selectFirstChapter() {
  fireEvent.change(screen.getByLabelText('Chapter'), { target: { value: 'seg_0' } });
}

describe('StoryPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('says what to do rather than rendering an empty timeline with no wedding open', async () => {
    await mount(null);
    expect(screen.getByText(/Open a wedding/)).toBeDefined();
  });

  it('offers every chapter for editing', async () => {
    await mount('prj_1');
    expect(screen.getByRole('option', { name: /Chapter 0/ })).toBeDefined();
    expect(screen.getByRole('option', { name: /Chapter 1/ })).toBeDefined();
  });

  it('says what moving a boundary will do, and that it locks both chapters', async () => {
    await mount('prj_1');
    selectFirstChapter();
    fireEvent.change(screen.getByLabelText('Where this chapter ends'), {
      target: { value: '660000' },
    });
    expect(screen.getByText(/become yours and re-analysis will not move them back/)).toBeDefined();
  });

  it('clamps a drag into the legal range rather than letting the backend refuse it', async () => {
    // `AURA-ML-5025` refuses a move that would empty a chapter. A photographer should never be
    // the one to find that out, so the control cannot express it in the first place.
    await mount('prj_1');
    selectFirstChapter();
    const slider = screen.getByLabelText('Where this chapter ends') as HTMLInputElement;
    expect(Number(slider.min)).toBe(1000);
    expect(Number(slider.max)).toBe(1_799_000);
  });

  it('sends the clamped boundary, never the raw one', async () => {
    await mount('prj_1');
    selectFirstChapter();
    fireEvent.change(screen.getByLabelText('Where this chapter ends'), {
      target: { value: '900000' },
    });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Move the boundary' }));
      await Promise.resolve();
    });
    expect(fixtures.moveChapterBoundary).toHaveBeenCalledWith({
      segmentId: 'seg_0',
      newEndMs: 900_000,
    });
  });

  it('says what a merge will do before it does it', async () => {
    await mount('prj_1');
    selectFirstChapter();
    expect(screen.getByText(/One chapter of 80 photographs, and it becomes yours/)).toBeDefined();
  });

  it('has no boundary control on the last chapter of the day', async () => {
    await mount('prj_1');
    fireEvent.change(screen.getByLabelText('Chapter'), { target: { value: 'seg_1' } });
    expect(screen.queryByLabelText('Where this chapter ends')).toBe(null);
    expect(screen.getByText(/last chapter of the day/)).toBeDefined();
  });
});
