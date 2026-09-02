import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { CurateHeroDto } from '../../ipc/types';
import { HeroGrid } from './HeroGrid';

/**
 * PHASE-29. These tests are about **why a photographer stops arguing with a pick**.
 *
 * Two frames from the same kiss can differ by 0.004. What decided between them is a constraint, and
 * a grid that showed only the score would leave somebody comparing two numbers that are the same
 * number. ADR-0060 section 3, asserted rather than documented.
 */

function hero(overrides: Partial<CurateHeroDto> = {}): CurateHeroDto {
  return {
    imageId: 'pht_1',
    rank: 0,
    score: 0.88,
    confidence: 0.74,
    terms: [
      ['technical', 0.91],
      ['emotion', 0.95],
      ['composition', 0.8],
      ['uniqueness', 0.7],
      ['story', 0.6],
    ],
    chapter: 'ceremony',
    scale: 'tight',
    binding: 'unconstrained',
    bindingText: 'the next strongest frame in the wedding',
    reasons: [{ code: 'emotional_peak', text: 'the peak of its moment', weight: 0.95, caveat: false }],
    accepted: null,
    ...overrides,
  };
}

describe('HeroGrid', () => {
  it('shows the binding constraint beside the score', () => {
    render(
      <HeroGrid
        heroes={[
          hero({
            binding: 'moment_exhausted',
            bindingText: 'that shot is already in the set',
          }),
        ]}
        onDecide={vi.fn()}
      />,
    );
    expect(screen.getByText('that shot is already in the set')).toBeTruthy();
  });

  it('renders a caveat differently from an argument', () => {
    render(
      <HeroGrid
        heroes={[
          hero({
            reasons: [
              { code: 'emotional_peak', text: 'the peak of its moment', weight: 0.9, caveat: false },
              {
                code: 'uniqueness_unavailable',
                text: 'AURA could not tell how similar this is to the rest',
                weight: -0.05,
                caveat: true,
              },
            ],
          }),
        ]}
        onDecide={vi.fn()}
      />,
    );
    const caveat = screen.getByText(/could not tell how similar/);
    const argument = screen.getByText('the peak of its moment');
    expect(caveat.className).toBe('caveat');
    expect(argument.className).toBe('argument');
  });

  it('says a shot scale was not measured rather than showing nothing', () => {
    render(<HeroGrid heroes={[hero({ scale: 'unknown' })]} onDecide={vi.fn()} />);
    expect(screen.getByText(/not measured/)).toBeTruthy();
  });

  it('makes rejecting exactly as cheap as accepting', () => {
    // A panel where accepting is one click and rejecting is a modal is a panel that measures
    // agreement it did not earn.
    const onDecide = vi.fn();
    render(<HeroGrid heroes={[hero()]} onDecide={onDecide} />);
    fireEvent.click(screen.getByText('Not this one'));
    expect(onDecide).toHaveBeenCalledWith('pht_1', false);
    fireEvent.click(screen.getByText('Keep'));
    expect(onDecide).toHaveBeenCalledWith('pht_1', true);
    expect(onDecide).toHaveBeenCalledTimes(2);
  });

  it('says so when there is no portfolio rather than drawing an empty grid', () => {
    render(<HeroGrid heroes={[]} onDecide={vi.fn()} />);
    expect(screen.getByText(/No portfolio picks yet/)).toBeTruthy();
  });
});
