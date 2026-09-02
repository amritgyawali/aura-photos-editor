import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { CurateAlbumDto, CurateSpreadDto } from '../../ipc/types';
import { AlbumBuilder } from './AlbumBuilder';

/**
 * PHASE-29. Two things are asserted here and both are rules rather than preferences.
 *
 * **A drag never crosses a chapter.** The drop target refuses the move before the command is sent,
 * and the command refuses it again. A wedding album whose ceremony follows its reception is not an
 * album with an unusual sequence; it is an album that is wrong.
 *
 * **A rhythm score measured over almost nothing is not reported as a result.** On this build eight
 * per cent is the realistic figure, so this is the common case rather than the exotic one.
 */

function spread(index: number, chapter: string, left: string, right: string | null): CurateSpreadDto {
  return {
    spreadId: `spr_${index}`,
    index,
    left,
    right,
    single: right === null,
    chapter,
    pairScore: 0.7,
    tonalGap: 0.05,
    warmthGapK: 100,
    facingScore: 0,
    facingKnown: false,
    similarity: 0.4,
    reasons: [],
  };
}

function album(overrides: Partial<CurateAlbumDto> = {}): CurateAlbumDto {
  return {
    spreads: [
      spread(0, 'ceremony', 'pht_1', 'pht_2'),
      spread(1, 'ceremony', 'pht_3', 'pht_4'),
      spread(2, 'reception', 'pht_5', 'pht_6'),
    ],
    chapters: [
      { chapter: 'ceremony', first: 0, len: 2, target: 2 },
      { chapter: 'reception', first: 2, len: 1, target: 2 },
    ],
    size: 6,
    targetSize: 80,
    rhythmScore: 1,
    rhythmMeasurable: 0.08,
    pairingScore: 0.7,
    userOrdered: false,
    coverage: [
      ['kiss', 'covered'],
      ['cake', 'missing'],
    ],
    warnings: ['the album has no photograph of the cake - and neither does the gallery'],
    reasons: [],
    summary: '6 images across 3 spreads (asked for 80).',
    ...overrides,
  };
}

describe('AlbumBuilder', () => {
  it('does not report a rhythm score measured over almost nothing as a result', () => {
    render(<AlbumBuilder album={album()} onSelectSpread={vi.fn()} onReorder={vi.fn()} />);
    const rhythm = screen.getByText('rhythm').closest('div');
    expect(rhythm?.textContent).toContain('measured on only 8%');
    expect(rhythm?.textContent).not.toContain('1.00 over');
  });

  it('reports a rhythm score measured over enough of the album', () => {
    render(
      <AlbumBuilder
        album={album({ rhythmMeasurable: 0.8, rhythmScore: 0.62 })}
        onSelectSpread={vi.fn()}
        onReorder={vi.fn()}
      />,
    );
    const rhythm = screen.getByText('rhythm').closest('div');
    expect(rhythm?.textContent).toContain('0.62 over 80%');
  });

  it('reorders inside a chapter and refuses to reorder across one', () => {
    const onReorder = vi.fn();
    render(<AlbumBuilder album={album()} onSelectSpread={vi.fn()} onReorder={onReorder} />);

    // Inside the ceremony: allowed, and the whole order is sent back.
    fireEvent.dragStart(screen.getByText('pht_1'));
    fireEvent.drop(screen.getByText('pht_4'));
    expect(onReorder).toHaveBeenCalledWith(['pht_2', 'pht_3', 'pht_4', 'pht_1', 'pht_5', 'pht_6']);

    onReorder.mockClear();

    // Ceremony into reception: refused before the command is sent.
    fireEvent.dragStart(screen.getByText('pht_1'));
    fireEvent.drop(screen.getByText('pht_5'));
    expect(onReorder).not.toHaveBeenCalled();
  });

  it('shows the album coverage and its warnings', () => {
    render(<AlbumBuilder album={album()} onSelectSpread={vi.fn()} onReorder={vi.fn()} />);
    expect(screen.getByText('cake: missing')).toBeTruthy();
    expect(screen.getByText(/neither does the gallery/)).toBeTruthy();
  });

  it('says when the photographer set the order', () => {
    render(
      <AlbumBuilder album={album({ userOrdered: true })} onSelectSpread={vi.fn()} onReorder={vi.fn()} />,
    );
    expect(screen.getByText(/You set this order/)).toBeTruthy();
  });

  it('says so when there is no album rather than drawing an empty one', () => {
    render(<AlbumBuilder album={null} onSelectSpread={vi.fn()} onReorder={vi.fn()} />);
    expect(screen.getByText(/No album yet/)).toBeTruthy();
  });
});
