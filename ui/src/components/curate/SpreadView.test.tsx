import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type { CurateSpreadDto } from '../../ipc/types';
import { SpreadView } from './SpreadView';

/**
 * PHASE-29. These tests are about the one distinction this view exists to hold:
 * **a spread nobody could measure and a spread that measured badly must not look the same.**
 *
 * On this build phase 06's detector finds no faces, so `facingKnown` is false almost everywhere. A
 * view that rendered a zero facing score as a failed pairing would report a defect in every spread
 * of every album. Phase 27's rule, on a screen.
 */

function spread(overrides: Partial<CurateSpreadDto> = {}): CurateSpreadDto {
  return {
    spreadId: 'spr_1',
    index: 3,
    left: 'pht_1',
    right: 'pht_2',
    single: false,
    chapter: 'ceremony',
    pairScore: 0.71,
    tonalGap: 0.06,
    warmthGapK: 120,
    facingScore: 0,
    facingKnown: false,
    similarity: 0.4,
    reasons: [],
    ...overrides,
  };
}

describe('SpreadView', () => {
  it('says a facing was not measured rather than drawing a zero', () => {
    render(<SpreadView spread={spread()} />);
    const term = screen.getByText('facing inward').closest('.spread-term');
    expect(term?.getAttribute('data-known')).toBe('false');
    expect(term?.textContent).toContain('not measured');
  });

  it('draws a measured facing of zero as a measurement', () => {
    render(<SpreadView spread={spread({ facingKnown: true, facingScore: 0 })} />);
    const term = screen.getByText('facing inward').closest('.spread-term');
    expect(term?.getAttribute('data-known')).toBe('true');
    expect(term?.textContent).toContain('0.00');
    expect(term?.textContent).not.toContain('not measured');
  });

  it('shows all four measurements rather than one number', () => {
    render(<SpreadView spread={spread()} />);
    expect(screen.getByText('tonal weight')).toBeTruthy();
    expect(screen.getByText('warmth')).toBeTruthy();
    expect(screen.getByText('facing inward')).toBeTruthy();
    expect(screen.getByText('how alike')).toBeTruthy();
  });

  it('draws a single page as a single rather than as a broken pair', () => {
    render(<SpreadView spread={spread({ single: true, right: null })} />);
    expect(screen.getByText('a single page')).toBeTruthy();
    expect(screen.queryByText('facing inward')).toBeNull();
  });

  it('says so when nothing is selected', () => {
    render(<SpreadView spread={null} />);
    expect(screen.getByText(/Choose a spread/)).toBeTruthy();
  });
});
