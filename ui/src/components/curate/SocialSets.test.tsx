import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { CurateSocialDto } from '../../ipc/types';
import { SocialSets } from './SocialSets';

/**
 * PHASE-29. These tests are about **an unfilled slot being shown rather than filled**.
 *
 * A wedding with no exit photographs gets a nine-image grid and a sentence, not a tenth frame
 * promoted out of another slot to make the number right. Phase 12's rule - the product cannot invent
 * coverage - in the smallest place it applies.
 */

function sets(overrides: Partial<CurateSocialDto> = {}): CurateSocialDto {
  return {
    grid: [
      {
        imageId: 'pht_1',
        aspect: '1:1',
        slot: 'portrait',
        rank: 0,
        legibility: 0.72,
        reasons: [],
        accepted: null,
      },
    ],
    story: [],
    hero: null,
    captions: [
      { imageId: 'pht_1', chapter: 'portraits', text: 'the couple portrait', source: 'template' },
    ],
    unfilled: [['exit', 1]],
    ...overrides,
  };
}

describe('SocialSets', () => {
  it('names the slots this wedding could not fill', () => {
    render(<SocialSets sets={sets()} onDecide={vi.fn()} />);
    expect(screen.getByText(/Nothing in this wedding for 1 of the exit slots/)).toBeTruthy();
  });

  it('shows the caption beside the frame it belongs to', () => {
    render(<SocialSets sets={sets()} onDecide={vi.fn()} />);
    expect(screen.getByText('the couple portrait')).toBeTruthy();
  });

  it('tells a photographer the captions are theirs to edit', () => {
    render(<SocialSets sets={sets()} onDecide={vi.fn()} />);
    expect(screen.getByText(/never invents a name, a place or a claim/)).toBeTruthy();
  });

  it('says a legibility was not measured rather than showing zero', () => {
    render(
      <SocialSets
        sets={sets({
          grid: [
            {
              imageId: 'pht_9',
              aspect: 'original',
              slot: 'detail',
              rank: 0,
              legibility: 0,
              reasons: [],
              accepted: null,
            },
          ],
        })}
        onDecide={vi.fn()}
      />,
    );
    expect(screen.getByText('not measured')).toBeTruthy();
  });

  it('records a decision against the set the frame is in', () => {
    const onDecide = vi.fn();
    render(<SocialSets sets={sets()} onDecide={onDecide} />);
    fireEvent.click(screen.getByText('Skip'));
    expect(onDecide).toHaveBeenCalledWith('pht_1', 'social_grid', false);
  });
});
