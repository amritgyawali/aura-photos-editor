import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { QcGroupDto, QcTicketDto } from '../../ipc/types';
import { TicketQueue } from './TicketQueue';

/**
 * PHASE-27. Every test here is about **the bulk action recording a verdict and nothing else**.
 *
 * Agreeing that forty findings are real is a statement about the findings. Instructing AURA to act
 * on forty frames unattended is a statement about the remedies, and the two are different
 * judgements made with different amounts of attention. ADR-0056 section 5, asserted rather than
 * documented - a later contributor adding an "and fix them" button to the bulk bar would be adding
 * exactly the feature this phase decided against, and would find out here.
 */

function ticket(overrides: Partial<QcTicketDto> = {}): QcTicketDto {
  return {
    ticketId: 'tkt_1',
    imageId: 'pho_1',
    category: 'consistency',
    code: 'consistency_drift',
    diagnosis: 'This frame sits outside the light it was shot in.',
    deviation: 3.1,
    threshold: 2,
    unit: 'tolerances',
    severity: 0.55,
    remedyKind: 'resolve_param',
    remedyTarget: 'white_balance: node',
    expectedGain: 0.82,
    confidence: 0.78,
    autonomy: 'review',
    mayActUnattended: false,
    round: 0,
    status: 'open',
    outcomeCode: null,
    scene: 'ceremony',
    evidenceKind: 'anchors',
    evidenceFrames: ['pho_7', 'pho_9'],
    evidenceCrop: null,
    reasons: ['Three anchors agree and this frame does not.'],
    ...overrides,
  };
}

function groups(): QcGroupDto[] {
  return [
    {
      category: 'consistency',
      worst: 0.55,
      tickets: [
        ticket(),
        ticket({ ticketId: 'tkt_2', severity: 0.4, diagnosis: 'A second frame drifts too.' }),
      ],
    },
    {
      category: 'skin',
      worst: 0.2,
      tickets: [
        ticket({
          ticketId: 'tkt_3',
          category: 'skin',
          severity: 0.2,
          diagnosis: 'Her skin sits away from where it does in the rest of the wedding.',
        }),
      ],
    },
  ];
}

describe('TicketQueue', () => {
  it('records a bulk verdict through the callback that cannot authorise a remedy', () => {
    const onDecideBulk = vi.fn();
    render(
      <TicketQueue
        groups={groups()}
        selectedTicketId={null}
        onSelect={vi.fn()}
        onDecide={vi.fn()}
        onDecideBulk={onDecideBulk}
      />,
    );
    fireEvent.click(screen.getByLabelText('Select: This frame sits outside the light it was shot in.'));
    fireEvent.click(screen.getByRole('button', { name: /These are real/ }));
    // Two arguments, and neither of them is a remedy authorisation.
    expect(onDecideBulk).toHaveBeenCalledTimes(1);
    const call = onDecideBulk.mock.calls[0];
    expect(call).toBeDefined();
    expect(call).toHaveLength(2);
    expect(call?.[1]).toBe('accepted');
  });

  it('tells the reader that a bulk verdict changes no photograph', () => {
    render(
      <TicketQueue
        groups={groups()}
        selectedTicketId={null}
        onSelect={vi.fn()}
        onDecide={vi.fn()}
        onDecideBulk={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByLabelText('Select: This frame sits outside the light it was shot in.'));
    expect(screen.getByText(/does not change any photograph/i)).toBeTruthy();
  });

  it('offers a remedy only on one finding at a time, next to its numbers', () => {
    const onDecide = vi.fn();
    render(
      <TicketQueue
        groups={groups()}
        selectedTicketId="tkt_1"
        onSelect={vi.fn()}
        onDecide={onDecide}
        onDecideBulk={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: /let AURA fix it/ }));
    expect(onDecide).toHaveBeenCalledWith('tkt_1', 'accepted', true);
    // And the measurement is beside the sentence rather than the sentence alone.
    expect(screen.getAllByText(/3\.10 tolerances against 2\.00/)[0]).toBeTruthy();
  });

  it('keeps the order the wire gave it rather than re-sorting', () => {
    render(
      <TicketQueue
        groups={groups()}
        selectedTicketId={null}
        onSelect={vi.fn()}
        onDecide={vi.fn()}
        onDecideBulk={vi.fn()}
      />,
    );
    const headings = screen.getAllByRole('heading', { level: 3 }).map((node) => node.textContent);
    expect(headings[0]).toContain('Matching the room');
    expect(headings[1]).toContain('Skin');
  });

  it('does not read an empty queue as a clean gallery', () => {
    render(
      <TicketQueue
        groups={[]}
        selectedTicketId={null}
        onSelect={vi.fn()}
        onDecide={vi.fn()}
        onDecideBulk={vi.fn()}
      />,
    );
    expect(screen.getByText(/not the same as nothing being wrong/i)).toBeTruthy();
  });
});
