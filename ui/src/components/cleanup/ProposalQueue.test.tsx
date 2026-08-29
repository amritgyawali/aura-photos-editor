import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { ProposalQueue } from './ProposalQueue';
import type {
  CleanupBlockedDto,
  CleanupProposalDto,
  CleanupStatusDto,
} from '../../ipc/types';

/**
 * PHASE-24. The four things this panel must get right, and every one of them is about what AURA
 * declined to do rather than about what it did.
 */

function status(over: Partial<CleanupStatusDto> = {}): CleanupStatusDto {
  return {
    photos: 400,
    examined: 400,
    coverage: 1,
    withProposals: 0,
    applied: 0,
    blocked: [0, 400, 0, 0, 0],
    checkNames: [
      'size_cap',
      'denylist',
      'identity_protect',
      'structure_span',
      'confidence',
    ],
    borrowed: 0,
    filled: 0,
    inpainted: 0,
    reverted: 0,
    maskCovered: 0,
    detectorTrained: false,
    inpaintAvailable: false,
    ...over,
  };
}

function proposal(over: Partial<CleanupProposalDto> = {}): CleanupProposalDto {
  return {
    proposalId: 'prp_1',
    photoId: 'pht_1',
    region: { x: 0.02, y: 0.85, w: 0.06, h: 0.06 },
    class: 'bin',
    classText: 'a bin, a crate or a catering tub',
    areaFrac: 0.0036,
    salience: 0.8,
    method: 'fill',
    borrowedFrom: null,
    model: null,
    confidence: 0.72,
    artefactScore: 0.02,
    autonomy: 'require_review',
    scene: 'reception_entrance',
    reasons: [
      {
        code: 'texture_uniform',
        text: 'the surroundings are even enough to copy from',
        weight: 1,
        isRefusal: false,
        evidence: null,
      },
    ],
    accepted: null,
    applied: false,
    mayApplyUnattended: false,
    versions: [1, 1, 1],
    ...over,
  };
}

const blocked: CleanupBlockedDto[] = [
  {
    region: { x: 0.4, y: 0.4, w: 0.1, h: 0.1 },
    check: 'denylist',
    code: 'protection_unknown',
    text: 'AURA cannot yet tell where people, dresses and rings are in this photograph',
  },
];

describe('ProposalQueue', () => {
  it('leads with the mask coverage rather than with the proposal count', () => {
    // Rule 1, and the reason the panel exists in this shape. At `maskCovered = 0` a build with no
    // segmenter would otherwise look exactly like a build that examined every photograph and found
    // them all clear.
    const { container } = render(
      <ProposalQueue status={status()} proposals={[]} blocked={blocked} />,
    );
    // The blocked row carries the same sentence, so the headline is found by its own class
    // rather than by the text - two elements saying it is the panel working, not a duplicate.
    const headline = container.querySelector(".cleanup-queue__headline");
    expect(headline?.textContent).toMatch(/cannot yet tell where people, dresses and rings are/i);
    expect(screen.getByText(/could check for people/i)).toBeTruthy();
  });

  it('says the detector cannot name things, rather than that nothing was found', () => {
    // The second most misleading sentence this panel could print. Masks are complete here, so the
    // headline has to be about the detector.
    render(
      <ProposalQueue
        status={status({ maskCovered: 1, detectorTrained: false })}
        proposals={[]}
        blocked={[]}
      />,
    );
    expect(screen.getByText(/cannot yet tell what those things are/i)).toBeTruthy();
  });

  it('only says nothing was found when it actually looked and found nothing', () => {
    render(
      <ProposalQueue
        status={status({ maskCovered: 1, detectorTrained: true })}
        proposals={[]}
        blocked={[]}
      />,
    );
    expect(screen.getByText(/found nothing worth tidying out/i)).toBeTruthy();
  });

  it('renders the refusals open when there is nothing else to show', () => {
    // Rule 2: a refusal is a decision, not an absence.
    const { container } = render(
      <ProposalQueue status={status()} proposals={[]} blocked={blocked} />,
    );
    const details = container.querySelector('details');
    expect(details?.hasAttribute('open')).toBe(true);
    expect(screen.getByText(/people, dress, rings or cake/i)).toBeTruthy();
  });

  it('names the source of a borrow on the row', () => {
    // Rule 3. "Real pixels from another frame" and "texture from this one" are different promises.
    render(
      <ProposalQueue
        status={status({ maskCovered: 1, detectorTrained: true, withProposals: 1 })}
        proposals={[proposal({ method: 'borrow', borrowedFrom: 'pht_source' })]}
        blocked={[]}
      />,
    );
    expect(screen.getByText(/real pixels from another frame/i)).toBeTruthy();
    expect(screen.getByText(/pht_source/)).toBeTruthy();
  });

  it('offers exactly two answers and no strength', () => {
    // Rule 5. There is no slider on this panel and no prop that could carry one.
    const onDecide = vi.fn();
    const { container } = render(
      <ProposalQueue
        status={status({ maskCovered: 1, detectorTrained: true, withProposals: 1 })}
        proposals={[proposal()]}
        blocked={[]}
        onDecide={onDecide}
      />,
    );
    expect(screen.getByText('Tidy it')).toBeTruthy();
    expect(screen.getByText('Leave it')).toBeTruthy();
    expect(container.querySelector('input[type="range"]')).toBeNull();
    expect(container.querySelector('textarea')).toBeNull();
  });

  it('says a proposal is waiting when it cannot be applied unattended', () => {
    render(
      <ProposalQueue
        status={status({ maskCovered: 1, detectorTrained: true, withProposals: 1 })}
        proposals={[proposal({ mayApplyUnattended: false })]}
        blocked={[]}
      />,
    );
    expect(screen.getByText(/waiting for you/i)).toBeTruthy();
  });
});
